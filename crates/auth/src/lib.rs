//! Authentication boundary.
//!
//! The app depends only on [`AuthProvider`], which is [`SupabaseAuth`]: people
//! sign in on the website in their browser (email, Google, or a new account),
//! and it hands the sign-in back through a `sanctuarymix://` deep link, guarded
//! by PKCE so only the app that started it can finish it. The refresh token and the last confirmed
//! session live in the OS keychain, so the app opens signed in and keeps its
//! plan offline for [`OFFLINE_GRACE_DAYS`]. Signed out, everyone gets the
//! [`LocalGuest`] session: the console is never locked out on a Sunday morning.
//!
//! What a session may do comes from two places: its [`Role`] in the church
//! (who, see [`Permission`]), and the church's plan in [`plan::Access`] (what's
//! paid for).

pub mod plan;
mod supabase;

pub use plan::{Access, Entitlements, Feature, Plan, SubscriptionStatus};
pub use supabase::{
    KeychainStore, MemoryStore, SecretStore, SupabaseAuth, SupabaseConfig, OFFLINE_GRACE_DAYS,
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("sign-in is not available yet")]
    NotImplemented,
    #[error("Can't reach SanctuaryMix. Check the internet connection, or mix without signing in.")]
    Offline,
    #[error("{0}")]
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

/// Something only some roles may do. Mixing by hand, playback, Freeze and
/// Undo are never on this list: anyone at the desk can always use them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    /// Pick auto-mix channels, roles and room feel. Turning a ready setup on
    /// or off is open to everyone.
    ChangeAutoMixSetup,
    /// Room feels marked admin-only (the loosest guardrails).
    ChooseAdminFeel,
    /// Delete recordings and old multitracks.
    DeleteRecordings,
    /// Send a recording's moves back to the console.
    ReplayToConsole,
}

impl Role {
    pub fn can(self, permission: Permission) -> bool {
        match permission {
            Permission::ChooseAdminFeel => self == Role::Admin,
            Permission::ChangeAutoMixSetup
            | Permission::DeleteRecordings
            | Permission::ReplayToConsole => matches!(self, Role::Admin | Role::Engineer),
        }
    }
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

impl Session {
    /// `Ok` if this session's role may do it, otherwise a sentence for the operator.
    pub fn require(&self, permission: Permission) -> std::result::Result<(), String> {
        if self.role.can(permission) {
            return Ok(());
        }
        Err(match permission {
            Permission::ChangeAutoMixSetup => {
                "Ask an engineer or admin to change the auto-mix setup."
            }
            Permission::ChooseAdminFeel => {
                "Only an admin can choose this room feel. Ask an admin, or pick another."
            }
            Permission::DeleteRecordings => "Ask an engineer or admin to delete recordings.",
            Permission::ReplayToConsole => {
                "Ask an engineer or admin to send recorded moves to the console."
            }
        }
        .into())
    }
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    async fn current_session(&self) -> Session;
    /// Starts a browser sign-in and returns the URL to open.
    async fn begin_sign_in(&self) -> Result<String>;
    /// Finishes sign-in from the deep-link callback URL.
    async fn complete_sign_in(&self, callback_url: String) -> Result<Session>;
    async fn sign_out(&self) -> Session;
    /// Checks the account and plan with the server. Offline, keeps the last
    /// confirmed session (within the grace period). Callers only run this
    /// between services, so a plan never changes mid-service.
    async fn refresh(&self) -> Session {
        self.current_session().await
    }
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

    #[test]
    fn volunteers_mix_but_dont_change_setup() {
        use Permission::*;
        for p in [
            ChangeAutoMixSetup,
            ChooseAdminFeel,
            DeleteRecordings,
            ReplayToConsole,
        ] {
            assert!(Role::Admin.can(p));
            assert!(!Role::Volunteer.can(p));
        }
        assert!(Role::Engineer.can(ChangeAutoMixSetup));
        assert!(!Role::Engineer.can(ChooseAdminFeel));

        let mut s = LocalGuest::session();
        s.role = Role::Volunteer;
        assert!(s
            .require(DeleteRecordings)
            .unwrap_err()
            .contains("engineer or admin"));
    }
}
