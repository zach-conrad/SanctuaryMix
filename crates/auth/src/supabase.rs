//! Supabase Auth (GoTrue) and the church tables, called from the core.
//!
//! People sign in on the website, not in the app: [`AuthProvider::begin_sign_in`]
//! opens the website's account page with a PKCE challenge, the person signs in
//! there however they like (email, Google, or a new account), and the website
//! sends the browser to `sanctuarymix://auth/callback?code=…`. The app trades
//! that one-time code plus its PKCE secret at the `app-handoff` Edge Function
//! for a session of its own (supabase/functions/app-handoff).
//!
//! Sign-in gives an access token (one hour) and a single-use refresh token.
//! The refresh token, the person and their church are saved together in the
//! OS keychain after every exchange, so the app opens signed in without the
//! network. The access token never leaves memory, and nothing here is ever
//! handed to the webview. The plan is re-checked at launch and between
//! services ([`AuthProvider::refresh`]); offline, the last confirmed plan holds
//! for [`OFFLINE_GRACE_DAYS`].

use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::plan::{Access, Plan, SubscriptionStatus};
use crate::{AuthError, AuthProvider, LocalGuest, Organization, Result, Role, Session, User};

/// How long a church's plan holds without reaching the server.
pub const OFFLINE_GRACE_DAYS: i64 = 14;
const DAY_SECS: i64 = 86_400;

const KEYCHAIN_SERVICE: &str = "app.sanctuarymix.desktop";
const KEYCHAIN_ACCOUNT: &str = "supabase-session";

/// Where to sign in. The URL and publishable key are public by design; the
/// database's row-level security decides what each person can read.
#[derive(Debug, Clone)]
pub struct SupabaseConfig {
    pub url: String,
    pub publishable_key: String,
    /// The website page the app sends people to for signing in.
    pub website_url: String,
    /// Where the website sends the browser back: the app's deep link.
    pub redirect_url: String,
}

impl SupabaseConfig {
    /// The SanctuaryMix project (the same one the website uses).
    pub fn sanctuarymix() -> Self {
        Self {
            url: "https://pfymsavkmnnxpbayyrsw.supabase.co".into(),
            publishable_key: "sb_publishable_QBbXzdnsQJOjNElfeJwhTQ_5MQYAwD8".into(), // gitleaks:allow (publishable, not a secret)
            website_url: "https://sanctuarymix.vercel.app/account/".into(),
            redirect_url: "sanctuarymix://auth/callback".into(),
        }
    }
}

/// Where the saved sign-in lives between launches.
pub trait SecretStore: Send + Sync {
    fn load(&self) -> Option<String>;
    fn save(&self, value: &str) -> std::result::Result<(), String>;
    fn clear(&self);
}

/// The macOS Keychain or Windows Credential Manager.
pub struct KeychainStore {
    entry: Option<keyring::Entry>,
}

impl KeychainStore {
    pub fn new() -> Self {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
            .inspect_err(|e| log::error!("keychain unavailable, sign-in won't be kept: {e}"))
            .ok();
        Self { entry }
    }
}

impl Default for KeychainStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStore for KeychainStore {
    fn load(&self) -> Option<String> {
        match self.entry.as_ref()?.get_password() {
            Ok(v) => Some(v),
            Err(keyring::Error::NoEntry) => None,
            Err(e) => {
                log::warn!("couldn't read the saved sign-in: {e}");
                None
            }
        }
    }

    fn save(&self, value: &str) -> std::result::Result<(), String> {
        let entry = self.entry.as_ref().ok_or("keychain unavailable")?;
        entry.set_password(value).map_err(|e| e.to_string())
    }

    fn clear(&self) {
        if let Some(entry) = &self.entry {
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(e) => log::warn!("couldn't remove the saved sign-in: {e}"),
            }
        }
    }
}

/// For tests, and for builds without a keychain.
#[derive(Default)]
pub struct MemoryStore(Mutex<Option<String>>);

impl SecretStore for MemoryStore {
    fn load(&self) -> Option<String> {
        self.0.lock().unwrap().clone()
    }
    fn save(&self, value: &str) -> std::result::Result<(), String> {
        *self.0.lock().unwrap() = Some(value.to_owned());
        Ok(())
    }
    fn clear(&self) {
        *self.0.lock().unwrap() = None;
    }
}

/// The signed-in person's church, as last confirmed by the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Church {
    id: String,
    name: String,
    role: Role,
    plan: Plan,
    /// Unix seconds.
    trial_ends_at: i64,
}

/// What's kept in the keychain.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    refresh_token: String,
    user: User,
    /// `None` until they set up a church (on the website).
    church: Option<Church>,
    /// When the server last confirmed the church and plan (unix seconds).
    checked_at: i64,
}

/// The session the app sees for a saved sign-in at time `now`.
fn session_at(saved: &Saved, now: i64) -> Session {
    let Some(church) = &saved.church else {
        // Signed in without a church: like working locally, nothing paid for.
        return Session {
            user: saved.user.clone(),
            active_org: None,
            role: Role::Admin,
            authenticated: true,
            access: Access::none(),
        };
    };
    let access = if now - saved.checked_at > OFFLINE_GRACE_DAYS * DAY_SECS {
        Access::unconfirmed(Some(church.plan))
    } else {
        // Billing isn't live yet: a trial that ends without a card pauses.
        let status = if now < church.trial_ends_at {
            SubscriptionStatus::Trialing
        } else {
            SubscriptionStatus::Paused
        };
        Access::from_subscription(church.plan, status, 0)
    };
    Session {
        user: saved.user.clone(),
        active_org: Some(Organization {
            id: church.id.clone(),
            name: church.name.clone(),
        }),
        role: church.role,
        authenticated: true,
        access,
    }
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

#[derive(Default)]
struct Inner {
    saved: Option<Saved>,
    access_token: Option<String>,
    /// PKCE secret for a website sign-in in progress.
    pkce_verifier: Option<String>,
}

pub struct SupabaseAuth {
    config: SupabaseConfig,
    http: reqwest::Client,
    store: Box<dyn SecretStore>,
    inner: Mutex<Inner>,
    /// Refresh tokens are single-use: one exchange at a time.
    exchange: tokio::sync::Mutex<()>,
}

/// GoTrue's token response.
#[derive(Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: String,
    user: ApiUser,
}

#[derive(Deserialize)]
struct ApiUser {
    id: String,
    email: Option<String>,
    #[serde(default)]
    user_metadata: serde_json::Value,
}

impl ApiUser {
    fn into_user(self) -> User {
        let meta = |k: &str| {
            self.user_metadata
                .get(k)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        };
        let display_name = meta("full_name")
            .or_else(|| meta("name"))
            .or_else(|| {
                self.email
                    .as_deref()
                    .and_then(|e| e.split('@').next())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "SanctuaryMix user".into());
        User {
            id: self.id,
            display_name,
            email: self.email,
        }
    }
}

/// A failed call: no answer at all, or the server's error.
enum ApiError {
    Network(String),
    Server { code: String, message: String },
}

impl ApiError {
    fn into_auth(self) -> AuthError {
        match self {
            ApiError::Network(e) => {
                log::warn!("auth server unreachable: {e}");
                AuthError::Offline
            }
            ApiError::Server { code, message } => match code.as_str() {
                // The handoff function's own sentences are written for the operator.
                "handoff_invalid" => {
                    AuthError::Failed(format!("{}.", message.trim_end_matches('.')))
                }
                "over_request_rate_limit" | "over_email_send_rate_limit" => AuthError::Failed(
                    "Too many tries for now. Wait a few minutes and try again.".into(),
                ),
                _ if message.is_empty() => {
                    AuthError::Failed("Sign-in didn't work. Try again.".into())
                }
                _ => AuthError::Failed(format!(
                    "Sign-in didn't work: {}",
                    message.trim_end_matches('.')
                )),
            },
        }
    }

    /// The refresh token is no good any more (signed out elsewhere, or revoked).
    fn ends_session(&self) -> bool {
        matches!(self, ApiError::Server { code, message }
            if matches!(code.as_str(),
                "refresh_token_not_found" | "refresh_token_already_used" | "session_not_found"
                | "session_expired" | "user_not_found" | "user_banned")
            || message.contains("Invalid Refresh Token"))
    }
}

async fn read_error(resp: reqwest::Response) -> ApiError {
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    let field = |k: &str| body.get(k).and_then(|v| v.as_str()).map(str::to_owned);
    ApiError::Server {
        code: field("error_code")
            .or_else(|| field("code"))
            .or_else(|| field("error"))
            .unwrap_or_else(|| status.as_u16().to_string()),
        message: field("msg")
            .or_else(|| field("message"))
            .or_else(|| field("error_description"))
            .unwrap_or_default(),
    }
}

impl SupabaseAuth {
    /// Restores the saved sign-in, if any. Never touches the network.
    pub fn new(config: SupabaseConfig, store: Box<dyn SecretStore>) -> Self {
        let saved = store.load().and_then(|raw| {
            serde_json::from_str::<Saved>(&raw)
                .inspect_err(|e| log::warn!("ignoring an unreadable saved sign-in: {e}"))
                .ok()
        });
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .build()
            .expect("HTTP client");
        Self {
            config,
            http,
            store,
            inner: Mutex::new(Inner {
                saved,
                ..Inner::default()
            }),
            exchange: tokio::sync::Mutex::new(()),
        }
    }

    fn session(&self) -> Session {
        match &self.inner.lock().unwrap().saved {
            Some(saved) => session_at(saved, now_secs()),
            None => LocalGuest::session(),
        }
    }

    async fn token(
        &self,
        grant: &str,
        body: serde_json::Value,
    ) -> std::result::Result<Tokens, ApiError> {
        self.post_tokens(
            format!("{}/auth/v1/token?grant_type={grant}", self.config.url),
            body,
        )
        .await
    }

    async fn post_tokens(
        &self,
        url: String,
        body: serde_json::Value,
    ) -> std::result::Result<Tokens, ApiError> {
        let resp = self
            .http
            .post(url)
            .header("apikey", &self.config.publishable_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(read_error(resp).await);
        }
        resp.json()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))
    }

    /// The person's church and role (their first membership).
    async fn church(
        &self,
        access_token: &str,
        user_id: &str,
    ) -> std::result::Result<Option<Church>, ApiError> {
        #[derive(Deserialize)]
        struct Row {
            role: Role,
            organizations: Option<Org>,
        }
        #[derive(Deserialize)]
        struct Org {
            id: String,
            name: String,
            plan: Plan,
            trial_ends_at: String,
        }
        let url = url::Url::parse_with_params(
            &format!("{}/rest/v1/memberships", self.config.url),
            [
                ("select", "role,organizations(id,name,plan,trial_ends_at)"),
                ("user_id", &format!("eq.{user_id}")),
                ("order", "created_at.asc"),
                ("limit", "1"),
            ],
        )
        .map_err(|e| ApiError::Network(e.to_string()))?;
        let resp = self
            .http
            .get(url)
            .header("apikey", &self.config.publishable_key)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(read_error(resp).await);
        }
        let rows: Vec<Row> = resp
            .json()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        Ok(rows.into_iter().next().and_then(|row| {
            let org = row.organizations?;
            let trial_ends_at = chrono::DateTime::parse_from_rfc3339(&org.trial_ends_at)
                .map(|t| t.timestamp())
                .inspect_err(|e| log::warn!("unreadable trial end {:?}: {e}", org.trial_ends_at))
                .unwrap_or(0);
            Some(Church {
                id: org.id,
                name: org.name,
                role: row.role,
                plan: org.plan,
                trial_ends_at,
            })
        }))
    }

    /// Reads the church with fresh tokens and saves everything. If the church
    /// can't be read, keeps the previous one and its check time.
    async fn finish(
        &self,
        tokens: Tokens,
        previous: Option<Saved>,
    ) -> std::result::Result<Session, ApiError> {
        let user = tokens.user.into_user();
        let (church, checked_at) = match self.church(&tokens.access_token, &user.id).await {
            Ok(church) => (church, now_secs()),
            Err(e) => match previous.filter(|p| p.user.id == user.id) {
                Some(p) => {
                    log::warn!("kept the last confirmed plan, couldn't read the church now");
                    (p.church, p.checked_at)
                }
                None => return Err(e),
            },
        };
        let saved = Saved {
            refresh_token: tokens.refresh_token,
            user,
            church,
            checked_at,
        };
        match serde_json::to_string(&saved) {
            Ok(raw) => {
                if let Err(e) = self.store.save(&raw) {
                    log::warn!(
                        "couldn't save the sign-in, it will be asked again next launch: {e}"
                    );
                }
            }
            Err(e) => log::error!("couldn't encode the sign-in: {e}"),
        }
        let session = session_at(&saved, now_secs());
        let mut inner = self.inner.lock().unwrap();
        inner.saved = Some(saved);
        inner.access_token = Some(tokens.access_token);
        Ok(session)
    }

    fn forget(&self) {
        self.store.clear();
        let mut inner = self.inner.lock().unwrap();
        inner.saved = None;
        inner.access_token = None;
    }
}

fn random_secret() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("OS random numbers");
    URL_SAFE_NO_PAD.encode(bytes)
}

#[async_trait]
impl AuthProvider for SupabaseAuth {
    async fn current_session(&self) -> Session {
        self.session()
    }

    async fn begin_sign_in(&self) -> Result<String> {
        let verifier = random_secret();
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let url = url::Url::parse_with_params(
            &self.config.website_url,
            [("app_challenge", challenge.as_str())],
        )
        .map_err(|e| AuthError::Failed(e.to_string()))?;
        self.inner.lock().unwrap().pkce_verifier = Some(verifier);
        Ok(url.into())
    }

    async fn complete_sign_in(&self, callback_url: String) -> Result<Session> {
        let url = url::Url::parse(&callback_url)
            .map_err(|_| AuthError::Failed("That sign-in link isn't valid.".into()))?;
        if !callback_url.starts_with(&self.config.redirect_url) {
            return Err(AuthError::Failed(
                "That sign-in link isn't for SanctuaryMix.".into(),
            ));
        }
        let code = url
            .query_pairs()
            .find(|(key, _)| key == "code")
            .map(|(_, v)| v.into_owned())
            .ok_or_else(|| {
                AuthError::Failed("That sign-in link is missing its code. Try again.".into())
            })?;
        let verifier = self
            .inner
            .lock()
            .unwrap()
            .pkce_verifier
            .take()
            .ok_or_else(|| AuthError::Failed("Start signing in again from SanctuaryMix.".into()))?;
        let _one = self.exchange.lock().await;
        let tokens = self
            .post_tokens(
                format!("{}/functions/v1/app-handoff", self.config.url),
                json!({ "code": code, "code_verifier": verifier }),
            )
            .await
            .map_err(ApiError::into_auth)?;
        self.finish(tokens, None).await.map_err(ApiError::into_auth)
    }

    async fn sign_out(&self) -> Session {
        let token = self.inner.lock().unwrap().access_token.clone();
        self.forget();
        if let Some(token) = token {
            // Ends this session on the server too. Best effort: signed out locally either way.
            let result = self
                .http
                .post(format!("{}/auth/v1/logout?scope=local", self.config.url))
                .header("apikey", &self.config.publishable_key)
                .bearer_auth(token)
                .send()
                .await;
            if let Err(e) = result {
                log::warn!("couldn't end the session on the server: {e}");
            }
        }
        LocalGuest::session()
    }

    async fn refresh(&self) -> Session {
        let _one = self.exchange.lock().await;
        let Some(previous) = self.inner.lock().unwrap().saved.clone() else {
            return LocalGuest::session();
        };
        let result = self
            .token(
                "refresh_token",
                json!({ "refresh_token": previous.refresh_token }),
            )
            .await;
        match result {
            Ok(tokens) => match self.finish(tokens, Some(previous)).await {
                Ok(session) => session,
                Err(_) => self.session(),
            },
            Err(e) if e.ends_session() => {
                log::info!("the saved sign-in has ended; signed out");
                self.forget();
                LocalGuest::session()
            }
            Err(ApiError::Network(e)) => {
                log::info!("offline, keeping the last confirmed plan: {e}");
                self.session()
            }
            Err(ApiError::Server { code, message }) => {
                log::warn!("couldn't refresh the sign-in ({code}): {message}");
                self.session()
            }
        }
    }
}

#[cfg(test)]
mod tests;
