//! What a church's subscription lets it do.
//!
//! One table, keyed by plan, mirrors `plan_features` in the approved auth plan
//! (section 7). The UI reads it from the session and greys out what's locked;
//! the core checks it before starting anything a plan gates. Console control,
//! manual mixing, meters and playback are never on this list: they never lock.

use serde::{Deserialize, Serialize};

/// A paid plan. Prices live on the website (`website/src/pricing.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Plan {
    Essentials,
    Pro,
    Campus,
}

impl Plan {
    pub fn name(self) -> &'static str {
        match self {
            Plan::Essentials => "Essentials",
            Plan::Pro => "Pro",
            Plan::Campus => "Campus",
        }
    }
}

/// Stripe's subscription status, as the webhook writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubscriptionStatus {
    Trialing,
    Active,
    /// Payment failed; Stripe is retrying. Full plan, with a banner for admins.
    PastDue,
    /// The trial ended without a card. Resumes when one is added.
    Paused,
    Canceled,
    Unpaid,
    IncompleteExpired,
}

impl SubscriptionStatus {
    /// Whether this status gets the plan's features (otherwise: lapsed).
    pub fn grants_plan(self) -> bool {
        matches!(self, Self::Trialing | Self::Active | Self::PastDue)
    }
}

/// Something a plan turns on. Everything else is always available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Feature {
    AutoMix,
    RecordServices,
    CloudSync,
    MixReports,
    AiEq,
}

impl Feature {
    /// The cheapest plan that has it, for "Available on …".
    pub fn min_plan(self) -> Plan {
        match self {
            Feature::AutoMix | Feature::RecordServices => Plan::Essentials,
            Feature::CloudSync | Feature::MixReports | Feature::AiEq => Plan::Pro,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Feature::AutoMix => "Auto-mix",
            Feature::RecordServices => "Recording services",
            Feature::CloudSync => "Cloud sync",
            Feature::MixReports => "Mix reports",
            Feature::AiEq => "AI EQ",
        }
    }
}

/// Limits and switches for one church.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entitlements {
    /// Input channels auto-mix may ride at once.
    pub max_ai_channels: u16,
    pub auto_mix: bool,
    /// New recordings. Playing back old ones is always allowed.
    pub record_services: bool,
    pub cloud_sync: bool,
    pub mix_reports: bool,
    pub ai_eq: bool,
    /// Team logins; `None` is unlimited.
    pub team_logins: Option<u16>,
    /// Rooms (one Mac and console each).
    pub rooms: u16,
}

/// dLive's input count; "all inputs" on Pro and Campus.
pub const ALL_INPUTS: u16 = 128;
/// Campus includes this many rooms before extra-room add-ons.
pub const CAMPUS_ROOMS: u16 = 3;

impl Entitlements {
    pub fn for_plan(plan: Plan, extra_rooms: u16) -> Self {
        match plan {
            Plan::Essentials => Self {
                max_ai_channels: 32,
                auto_mix: true,
                record_services: true,
                cloud_sync: false,
                mix_reports: false,
                ai_eq: false,
                team_logins: Some(3),
                rooms: 1,
            },
            Plan::Pro => Self {
                max_ai_channels: ALL_INPUTS,
                auto_mix: true,
                record_services: true,
                cloud_sync: true,
                mix_reports: true,
                ai_eq: true,
                team_logins: None,
                rooms: 1,
            },
            Plan::Campus => Self {
                rooms: CAMPUS_ROOMS + extra_rooms,
                ..Self::for_plan(Plan::Pro, 0)
            },
        }
    }

    /// No plan, or it lapsed: manual mixing and playback only.
    pub fn lapsed() -> Self {
        Self {
            max_ai_channels: 0,
            auto_mix: false,
            record_services: false,
            cloud_sync: false,
            mix_reports: false,
            ai_eq: false,
            team_logins: None,
            rooms: 1,
        }
    }

    pub fn allows(&self, feature: Feature) -> bool {
        match feature {
            Feature::AutoMix => self.auto_mix,
            Feature::RecordServices => self.record_services,
            Feature::CloudSync => self.cloud_sync,
            Feature::MixReports => self.mix_reports,
            Feature::AiEq => self.ai_eq,
        }
    }
}

/// The church's plan as this Mac knows it. Comes from the church's row in
/// Supabase today; from the signed licence token once billing is live.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Access {
    /// `None` when signed out or working on this computer only.
    pub plan: Option<Plan>,
    pub status: Option<SubscriptionStatus>,
    pub entitlements: Entitlements,
}

impl Access {
    pub fn from_subscription(plan: Plan, status: SubscriptionStatus, extra_rooms: u16) -> Self {
        let entitlements = if status.grants_plan() {
            Entitlements::for_plan(plan, extra_rooms)
        } else {
            Entitlements::lapsed()
        };
        Self {
            plan: Some(plan),
            status: Some(status),
            entitlements,
        }
    }

    /// Signed out: the console and playback still work, nothing else.
    pub fn none() -> Self {
        Self {
            plan: None,
            status: None,
            entitlements: Entitlements::lapsed(),
        }
    }

    /// Signed in, but the plan hasn't been confirmed online within the grace
    /// period: manual mixing and playback only until it checks in.
    pub fn unconfirmed(plan: Option<Plan>) -> Self {
        Self {
            plan,
            status: None,
            entitlements: Entitlements::lapsed(),
        }
    }

    /// `Ok` if the plan has `feature`, otherwise a sentence for the operator.
    pub fn require(&self, feature: Feature) -> Result<(), String> {
        if self.entitlements.allows(feature) {
            return Ok(());
        }
        Err(match (self.plan, self.status) {
            (None, _) => format!("{} needs a SanctuaryMix plan. Sign in in Settings.", feature.label()),
            (Some(_), None) => format!(
                "{} needs SanctuaryMix to check your plan. Connect this computer to the internet.",
                feature.label()
            ),
            (Some(_), Some(s)) if !s.grants_plan() => format!(
                "{} is paused until your plan is active again. An admin can fix billing on the website.",
                feature.label()
            ),
            _ => format!("{} is available on {}.", feature.label(), feature.min_plan().name()),
        })
    }

    /// `Ok` if auto-mix may ride this many channels at once.
    pub fn require_ai_channels(&self, count: usize) -> Result<(), String> {
        self.require(Feature::AutoMix)?;
        let max = self.entitlements.max_ai_channels;
        if count > usize::from(max) {
            return Err(format!(
                "Your plan covers up to {max} channels under auto-mix. Remove {} or move to Pro.",
                count - usize::from(max)
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_match_the_pricing_page() {
        let e = Entitlements::for_plan(Plan::Essentials, 0);
        assert_eq!(
            (e.max_ai_channels, e.team_logins, e.rooms),
            (32, Some(3), 1)
        );
        assert!(e.auto_mix && !e.cloud_sync && !e.mix_reports);

        let p = Entitlements::for_plan(Plan::Pro, 0);
        assert_eq!((p.max_ai_channels, p.team_logins, p.rooms), (128, None, 1));
        assert!(p.cloud_sync && p.mix_reports && p.ai_eq);

        assert_eq!(Entitlements::for_plan(Plan::Campus, 2).rooms, 5);
    }

    #[test]
    fn lapsed_keeps_nothing_a_plan_gates() {
        let a = Access::from_subscription(Plan::Pro, SubscriptionStatus::Paused, 0);
        assert_eq!(a.entitlements, Entitlements::lapsed());
        assert!(a.require(Feature::AutoMix).unwrap_err().contains("paused"));
        assert!(
            Access::from_subscription(Plan::Pro, SubscriptionStatus::PastDue, 0)
                .require(Feature::AutoMix)
                .is_ok()
        );
    }

    #[test]
    fn locked_features_say_which_plan_has_them() {
        let a = Access::from_subscription(Plan::Essentials, SubscriptionStatus::Active, 0);
        assert_eq!(
            a.require(Feature::CloudSync).unwrap_err(),
            "Cloud sync is available on Pro."
        );
        assert!(a.require_ai_channels(32).is_ok());
        assert!(a.require_ai_channels(33).unwrap_err().contains("Remove 1"));
        assert!(Access::none()
            .require(Feature::AutoMix)
            .unwrap_err()
            .contains("Sign in"));
        assert!(Access::unconfirmed(Some(Plan::Pro))
            .require(Feature::AutoMix)
            .unwrap_err()
            .contains("internet"));
    }
}
