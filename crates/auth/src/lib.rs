//! Authentication boundary.
//!
//! The app depends only on [`AuthProvider`]. Today that is [`LocalGuest`],
//! which signs everyone in as a local operator so the console is never locked
//! out on a Sunday morning. The approved plan is Supabase Auth: sign-in runs
//! in the system browser (OAuth + PKCE), returns through a `sanctuarymix://`
//! deep link, and tokens live in the OS keychain. That provider implements the
//! same trait without touching the UI or commands.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("sign-in is not available yet")]
    NotImplemented,
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
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    async fn current_session(&self) -> Session;
    /// Starts a browser sign-in and returns the URL to open.
    async fn begin_sign_in(&self) -> Result<String>;
    /// Finishes sign-in from the deep-link callback URL.
    async fn complete_sign_in(&self, callback_url: String) -> Result<Session>;
    async fn sign_out(&self) -> Session;
}

/// Placeholder provider: always a local admin, sign-in not yet available.
#[derive(Default)]
pub struct LocalGuest;

impl LocalGuest {
    fn session() -> Session {
        Session {
            user: User {
                id: "local".into(),
                display_name: "Local operator".into(),
                email: None,
            },
            active_org: None,
            role: Role::Admin,
            authenticated: false,
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
        assert!(matches!(
            auth.begin_sign_in().await,
            Err(AuthError::NotImplemented)
        ));
    }
}
