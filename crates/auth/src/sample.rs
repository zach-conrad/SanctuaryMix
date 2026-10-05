//! A built-in sample account, until Supabase Auth is wired up.
//!
//! `test@example.com` with password `1234` signs in as an admin of a sample
//! church on Campus, so every feature is on. The repo is public and these are
//! not secrets; remove this provider before launch.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::plan::{Access, Plan, SubscriptionStatus};
use crate::{AuthError, AuthProvider, LocalGuest, Organization, Result, Role, Session, User};

pub const SAMPLE_EMAIL: &str = "test@example.com";
pub const SAMPLE_PASSWORD: &str = "1234";

#[derive(Default)]
pub struct SampleAccount {
    signed_in: Mutex<bool>,
}

impl SampleAccount {
    /// `signed_in` restores the last run's sign-in.
    pub fn new(signed_in: bool) -> Self {
        Self {
            signed_in: Mutex::new(signed_in),
        }
    }

    fn signed_in_session() -> Session {
        Session {
            user: User {
                id: "sample-user".into(),
                display_name: "Test User".into(),
                email: Some(SAMPLE_EMAIL.into()),
            },
            active_org: Some(Organization {
                id: "sample-church".into(),
                name: "Sample Church".into(),
            }),
            role: Role::Admin,
            authenticated: true,
            access: Access::from_subscription(Plan::Campus, SubscriptionStatus::Active, 0),
        }
    }

    fn session(&self) -> Session {
        if *self.signed_in.lock().unwrap() {
            Self::signed_in_session()
        } else {
            LocalGuest::session()
        }
    }
}

#[async_trait]
impl AuthProvider for SampleAccount {
    async fn current_session(&self) -> Session {
        self.session()
    }

    async fn sign_in_with_password(&self, email: &str, password: &str) -> Result<Session> {
        if !email.trim().eq_ignore_ascii_case(SAMPLE_EMAIL) || password != SAMPLE_PASSWORD {
            return Err(AuthError::WrongCredentials);
        }
        *self.signed_in.lock().unwrap() = true;
        Ok(self.session())
    }

    async fn begin_sign_in(&self) -> Result<String> {
        Err(AuthError::NotImplemented)
    }

    async fn complete_sign_in(&self, _callback_url: String) -> Result<Session> {
        Err(AuthError::NotImplemented)
    }

    async fn sign_out(&self) -> Session {
        *self.signed_in.lock().unwrap() = false;
        self.session()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Feature;

    #[tokio::test]
    async fn sample_account_unlocks_everything() {
        let auth = SampleAccount::default();
        assert!(!auth.current_session().await.authenticated);

        let s = auth
            .sign_in_with_password(" Test@Example.com ", "1234")
            .await
            .unwrap();
        assert!(s.authenticated);
        assert_eq!(s.role, Role::Admin);
        for f in [
            Feature::AutoMix,
            Feature::RecordServices,
            Feature::CloudSync,
            Feature::MixReports,
            Feature::AiEq,
        ] {
            assert!(s.access.require(f).is_ok());
        }

        assert!(!auth.sign_out().await.authenticated);
    }

    #[tokio::test]
    async fn wrong_password_stays_signed_out() {
        let auth = SampleAccount::default();
        assert!(matches!(
            auth.sign_in_with_password(SAMPLE_EMAIL, "12345").await,
            Err(AuthError::WrongCredentials)
        ));
        assert!(!auth.current_session().await.authenticated);
    }
}
