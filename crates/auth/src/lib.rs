//! Authentication boundary.
//!
//! The app depends only on [`AuthProvider`]. Today that is [`SampleAccount`]:
//! one built-in account with every feature, and the signed-out local session
//! for everyone else, so the console is never locked out on a Sunday morning.
//! The approved plan is Supabase Auth: email and password from an in-app form
//! (sent from this core, never stored by the webview), or Google in the system
//! browser (OAuth + PKCE) back through a `sanctuarymix://` deep link, with
//! tokens in the OS keychain. That provider implements the same trait without
//! touching the UI or commands.
//!
//! What a session may do comes from two places: its [`Role`] in the church
//! (who), and the church's plan in [`plan::Access`] (what's paid for).

pub mod plan;
mod sample;

pub use plan::{Access, Entitlements, Feature, Plan, SubscriptionStatus};
pub use sample::SampleAccount;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("sign-in is not available yet")]
    NotImplemented,
    #[error("That email and password don't match. Check them and try again.")]
    WrongCredentials,
    #[error("sign-in failed: {0}")]
    Failed(String),
}

pub type Result<T> = std::result::Result<T, AuthError>;

/// What a person may do at a church's console. Roles belong to a membership,
/// not a user, because one engineer may serve several churches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    /// Full control, including system setup.
    Admin,
    /// Can mix and accept AI changes.
    Engineer,
    /// Simplified view; AI suggestions need an engineer to approve.
    Volunteer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub display_name: String,
    pub email: Option<String>,
}

/// A church (or campus) whose team shares scenes and settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Organization {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub user: User,
    /// The church this session is working for; `None` when offline/local.
    pub active_org: Option<Organization>,
    /// The user's role in `active_org` (or locally).
    pub role: Role,
    /// False for the built-in local session.
    pub authenticated: bool,
    /// The church's plan and what it unlocks.
    pub access: Access,
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    async fn current_session(&self) -> Session;
    /// Signs in from the app's own email and password form.
    async fn sign_in_with_password(&self, _email: &str, _password: &str) -> Result<Session> {
        Err(AuthError::NotImplemented)
    }
    /// Starts a browser sign-in and returns the URL to open.
    async fn begin_sign_in(&self) -> Result<String>;
    /// Finishes sign-in from the deep-link callback URL.
    async fn complete_sign_in(&self, callback_url: String) -> Result<Session>;
    async fn sign_out(&self) -> Session;
}

/// No account: a local admin with manual mixing and playback only.
#[derive(Default)]
pub struct LocalGuest;

impl LocalGuest {
    /// The signed-out session every provider falls back to.
    pub fn session() -> Session {
        Session {
            user: User {
                id: "local".into(),
                display_name: "Local operator".into(),
                email: None,
            },
            active_org: None,
            role: Role::Admin,
            authenticated: false,
            access: Access::none(),
        }
    }
}

#[async_trait]
impl AuthProvider for LocalGuest {
    async fn current_session(&self) -> Session {
        Self::session()
    }

    async fn begin_sign_in(&self) -> Result<String> {
        Err(AuthError::NotImplemented)
    }

    async fn complete_sign_in(&self, _callback_url: String) -> Result<Session> {
        Err(AuthError::NotImplemented)
    }

    async fn sign_out(&self) -> Session {
        Self::session()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_guest_is_an_unauthenticated_admin() {
        let auth = LocalGuest;
        let s = auth.current_session().await;
        assert_eq!(s.role, Role::Admin);
        assert!(s.active_org.is_none());
        assert!(!s.authenticated);
        assert_eq!(s.access, Access::none());
        assert!(matches!(
            auth.begin_sign_in().await,
            Err(AuthError::NotImplemented)
        ));
    }
}
