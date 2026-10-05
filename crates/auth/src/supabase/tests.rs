use std::collections::VecDeque;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::*;
use crate::{Feature, Permission};

/// A canned response per request, in order, from a local HTTP server. Records
/// each request line and body.
struct FakeServer {
    url: String,
    seen: Arc<Mutex<Vec<String>>>,
}

async fn fake(responses: Vec<(u16, serde_json::Value)>) -> FakeServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let mut queue: VecDeque<_> = responses.into();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            // Read headers, then the body by Content-Length.
            let (head_end, length) = loop {
                let n = sock.read(&mut chunk).await.unwrap();
                buf.extend_from_slice(&chunk[..n]);
                if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&buf[..i]).to_lowercase();
                    let len = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .map(|v| v.trim().parse::<usize>().unwrap())
                        .unwrap_or(0);
                    break (i + 4, len);
                }
            };
            while buf.len() < head_end + length {
                let n = sock.read(&mut chunk).await.unwrap();
                buf.extend_from_slice(&chunk[..n]);
            }
            let text = String::from_utf8_lossy(&buf);
            let line = text.lines().next().unwrap_or_default().to_owned();
            let body = String::from_utf8_lossy(&buf[head_end..]).to_string();
            log.lock().unwrap().push(format!("{line} {body}"));
            let (status, json) = queue.pop_front().unwrap_or((500, json!({})));
            let payload = json.to_string();
            let reply = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                payload.len()
            );
            sock.write_all(reply.as_bytes()).await.unwrap();
        }
    });
    FakeServer { url, seen }
}

fn config(url: &str) -> SupabaseConfig {
    SupabaseConfig {
        url: url.into(),
        publishable_key: "sb_publishable_test".into(),
        website_url: "https://sanctuarymix.test/account/".into(),
        redirect_url: "sanctuarymix://auth/callback".into(),
    }
}

fn tokens(refresh: &str) -> serde_json::Value {
    json!({
        "access_token": "access-1",
        "refresh_token": refresh,
        "expires_in": 3600,
        "user": { "id": "u1", "email": "pat@stmarks.org", "user_metadata": { "full_name": "Pat Lee" } }
    })
}

fn membership(role: &str, plan: &str, trial_days: i64) -> serde_json::Value {
    let ends = chrono::Utc::now() + chrono::Duration::days(trial_days);
    json!([{ "role": role, "organizations": {
        "id": "o1", "name": "St. Mark's", "plan": plan, "trial_ends_at": ends.to_rfc3339()
    } }])
}

/// A store whose contents the test can read after the provider is gone.
#[derive(Clone, Default)]
struct SharedStore(Arc<MemoryStore>);
impl SecretStore for SharedStore {
    fn load(&self) -> Option<String> {
        self.0.load()
    }
    fn save(&self, v: &str) -> std::result::Result<(), String> {
        self.0.save(v)
    }
    fn clear(&self) {
        self.0.clear()
    }
}

const CODE: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// Opens the website (as the app would) and comes back through the deep link.
async fn sign_in(auth: &SupabaseAuth) -> Result<Session> {
    auth.begin_sign_in().await.unwrap();
    auth.complete_sign_in(format!("sanctuarymix://auth/callback?code={CODE}"))
        .await
}

#[tokio::test]
async fn website_handoff_reads_church_and_saves_it() {
    let server = fake(vec![
        (200, tokens("r1")),
        (200, membership("engineer", "pro", 20)),
    ])
    .await;
    let store = SharedStore::default();
    let auth = SupabaseAuth::new(config(&server.url), Box::new(store.clone()));
    assert!(!auth.current_session().await.authenticated);

    // Sign-in starts on the website, carrying only the PKCE challenge.
    let url = url::Url::parse(&auth.begin_sign_in().await.unwrap()).unwrap();
    assert_eq!(
        url.as_str().split('?').next(),
        Some("https://sanctuarymix.test/account/")
    );
    let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let challenge = q["app_challenge"].clone();
    assert_eq!(challenge.len(), 43);

    // Someone else's link is refused and doesn't use up the sign-in.
    assert!(auth
        .complete_sign_in(format!("https://evil.example/?code={CODE}"))
        .await
        .is_err());

    let s = auth
        .complete_sign_in(format!("sanctuarymix://auth/callback?code={CODE}"))
        .await
        .unwrap();
    assert!(s.authenticated);
    assert_eq!(s.user.display_name, "Pat Lee");
    assert_eq!(s.active_org.as_ref().unwrap().name, "St. Mark's");
    assert_eq!(s.role, Role::Engineer);
    assert_eq!(s.access.status, Some(SubscriptionStatus::Trialing));
    assert!(s.access.require(Feature::CloudSync).is_ok());

    let seen = server.seen.lock().unwrap().clone();
    assert!(seen[0].starts_with("POST /functions/v1/app-handoff"));
    let body: serde_json::Value =
        serde_json::from_str(&seen[0][seen[0].find('{').unwrap()..]).unwrap();
    assert_eq!(body["code"], CODE);
    // The secret sent with the code is the one behind the challenge.
    let verifier = body["code_verifier"].as_str().unwrap();
    assert_eq!(
        URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
        challenge
    );
    assert!(seen[1].starts_with("GET /rest/v1/memberships?select=role%2Corganizations"));

    // The secret is single-use.
    assert!(auth
        .complete_sign_in(format!("sanctuarymix://auth/callback?code={CODE}"))
        .await
        .is_err());

    // The next launch opens signed in, without the network.
    let saved = store.load().unwrap();
    assert!(saved.contains("r1") && !saved.contains("access-1"));
    let again = SupabaseAuth::new(config("http://127.0.0.1:9"), Box::new(store.clone()));
    assert_eq!(again.current_session().await, s);
}

#[tokio::test]
async fn a_callback_the_app_didnt_start_is_refused() {
    let auth = SupabaseAuth::new(
        config("http://127.0.0.1:9"),
        Box::new(MemoryStore::default()),
    );
    let err = auth
        .complete_sign_in(format!("sanctuarymix://auth/callback?code={CODE}"))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("again"));
}

#[tokio::test]
async fn expired_code_says_so() {
    let server = fake(vec![(
        400,
        json!({ "error_code": "handoff_invalid", "msg": "That sign-in link has expired or was already used. Sign in again from SanctuaryMix" }),
    )])
    .await;
    let auth = SupabaseAuth::new(config(&server.url), Box::new(MemoryStore::default()));
    let err = sign_in(&auth).await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "That sign-in link has expired or was already used. Sign in again from SanctuaryMix."
    );
    assert!(!auth.current_session().await.authenticated);
}

#[tokio::test]
async fn offline_sign_in_says_offline() {
    let auth = SupabaseAuth::new(
        config("http://127.0.0.1:9"),
        Box::new(MemoryStore::default()),
    );
    assert!(matches!(sign_in(&auth).await, Err(AuthError::Offline)));
}

#[tokio::test]
async fn signed_in_without_a_church_has_no_plan() {
    let server = fake(vec![(200, tokens("r1")), (200, json!([]))]).await;
    let auth = SupabaseAuth::new(config(&server.url), Box::new(MemoryStore::default()));
    let s = sign_in(&auth).await.unwrap();
    assert!(s.authenticated);
    assert!(s.active_org.is_none());
    assert!(s.access.require(Feature::AutoMix).is_err());
}

#[tokio::test]
async fn refresh_rotates_the_token_and_picks_up_plan_changes() {
    let server = fake(vec![
        (200, tokens("r1")),
        (200, membership("admin", "essentials", 20)),
        (200, tokens("r2")),
        (200, membership("admin", "campus", 20)),
    ])
    .await;
    let store = SharedStore::default();
    let auth = SupabaseAuth::new(config(&server.url), Box::new(store.clone()));
    sign_in(&auth).await.unwrap();
    let s = auth.refresh().await;
    assert_eq!(s.access.plan, Some(Plan::Campus));
    assert!(store.load().unwrap().contains("r2"));
    let seen = server.seen.lock().unwrap().clone();
    assert!(seen[2].contains(r#""refresh_token":"r1""#));
}

#[tokio::test]
async fn revoked_refresh_token_signs_out_but_offline_does_not() {
    let server = fake(vec![
        (200, tokens("r1")),
        (200, membership("admin", "pro", 20)),
        (400, json!({ "error_code": "refresh_token_not_found", "msg": "Invalid Refresh Token: Refresh Token Not Found" })),
    ])
    .await;
    let store = SharedStore::default();
    let auth = SupabaseAuth::new(config(&server.url), Box::new(store.clone()));
    sign_in(&auth).await.unwrap();

    // Offline: same provider pointed nowhere keeps the saved session.
    let offline = SupabaseAuth::new(config("http://127.0.0.1:9"), Box::new(store.clone()));
    assert!(offline.refresh().await.authenticated);
    assert!(store.load().is_some());

    assert!(!auth.refresh().await.authenticated);
    assert!(store.load().is_none());
}

#[test]
fn plan_holds_offline_for_the_grace_period_then_waits_to_check_in() {
    let now = 1_800_000_000;
    let saved = |checked_days_ago: i64, trial_left_days: i64| Saved {
        refresh_token: "r".into(),
        user: User {
            id: "u".into(),
            display_name: "Pat".into(),
            email: None,
        },
        church: Some(Church {
            id: "o".into(),
            name: "St. Mark's".into(),
            role: Role::Volunteer,
            plan: Plan::Pro,
            trial_ends_at: now + trial_left_days * DAY_SECS,
        }),
        checked_at: now - checked_days_ago * DAY_SECS,
    };

    let fresh = session_at(&saved(13, 10), now);
    assert!(fresh.access.require(Feature::AutoMix).is_ok());
    assert!(fresh.require(Permission::ChangeAutoMixSetup).is_err());

    let stale = session_at(&saved(15, 10), now);
    assert!(stale.authenticated);
    assert!(stale
        .access
        .require(Feature::AutoMix)
        .unwrap_err()
        .contains("internet"));

    let trial_over = session_at(&saved(1, -1), now);
    assert_eq!(trial_over.access.status, Some(SubscriptionStatus::Paused));
    assert!(trial_over.access.require(Feature::RecordServices).is_err());
}
