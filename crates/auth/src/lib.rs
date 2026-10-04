//! Authentication boundary.
//!
//! The app depends only on [`AuthProvider`]. Today that is [`LocalGuest`],
//! which signs everyone in as a local operator so the console is never locked
//! out on a Sunday morning. A hosted provider (e.g. OAuth/OIDC with church
//! team accounts and saved show files) implements the same trait later
//! without touching the UI or commands.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("sign-in is not available yet")]
    NotImplemented,
    #[error("invalid credentials")]
    InvalidCredentials,
}

pub type Result<T> = std::result::Result<T, AuthError>;

/// What a person may do at the console.
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
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub user: User,
    /// False for the built-in local session.
    pub authenticated: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    async fn current_session(&self) -> Session;
    async fn sign_in(&self, credentials: Credentials) -> Result<Session>;
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
                role: Role::Admin,
            },
            authenticated: false,
        }
    }
}

#[async_trait]
impl AuthProvider for LocalGuest {
    async fn current_session(&self) -> Session {
        Self::session()
    }

    async fn sign_in(&self, _credentials: Credentials) -> Result<Session> {
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
        assert_eq!(s.user.role, Role::Admin);
        assert!(!s.authenticated);
        let creds = Credentials {
            email: "a@b.c".into(),
            password: "x".into(),
        };
        assert!(matches!(
            auth.sign_in(creds).await,
            Err(AuthError::NotImplemented)
        ));
    }
}
