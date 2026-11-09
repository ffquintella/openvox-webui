//! MFA policies: which authentication level a login must reach.

use anyhow::{bail, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

pub const ACR_PRIMARY: u8 = 1;
pub const ACR_MFA: u8 = 2;
/// Level 2 reached with a phishing-resistant factor (OQ-6): a WebAuthn
/// ceremony with a key method. Policies ask for it with
/// `require_phishing_resistant`, never with `required_acr: 3`.
pub const ACR_PHISHING_RESISTANT: u8 = 3;
const MAX_POLICIES: usize = 100;

/// Empty lists match everything. All non-empty lists must match.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PolicyConditions {
    /// The user holds at least one of these roles.
    #[serde(default)]
    pub roles: Vec<String>,
    /// The primary identity came from one of these providers.
    #[serde(default)]
    pub providers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyInput {
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "hundred")]
    pub priority: i64,
    #[serde(default)]
    pub conditions: PolicyConditions,
    #[serde(default = "mfa")]
    pub required_acr: u8,
    /// Only a phishing-resistant factor satisfies the policy (OQ-6): TOTP
    /// and recovery codes are neither offered nor accepted.
    #[serde(default)]
    pub require_phishing_resistant: bool,
}

impl PolicyInput {
    /// Level the policy demands.
    pub fn level(&self) -> u8 {
        if self.require_phishing_resistant {
            ACR_PHISHING_RESISTANT
        } else {
            self.required_acr
        }
    }
}

fn yes() -> bool {
    true
}
fn hundred() -> i64 {
    100
}
fn mfa() -> u8 {
    ACR_MFA
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthPolicy {
    pub id: Uuid,
    #[serde(flatten)]
    pub policy: PolicyInput,
}

impl PolicyConditions {
    fn matches(&self, roles: &[String], provider: &str) -> bool {
        (self.roles.is_empty() || self.roles.iter().any(|r| roles.contains(r)))
            && (self.providers.is_empty() || self.providers.iter().any(|p| p == provider))
    }
}

/// Level a login must reach: the highest of the enabled matching policies, or
/// the primary level when none applies (no policy means no second factor).
pub fn required_acr(policies: &[AuthPolicy], roles: &[String], provider: &str) -> u8 {
    policies
        .iter()
        .filter(|p| p.policy.enabled && p.policy.conditions.matches(roles, provider))
        .map(|p| p.policy.level())
        .max()
        .unwrap_or(ACR_PRIMARY)
}

/// `required_acr` when the sign-in provider is unknown (step-up, an
/// enrollment-only session): every enabled policy whose role condition
/// matches counts, whatever its provider condition, so the answer is never
/// lower than the one of any sign-in path.
pub fn required_acr_for_roles(policies: &[AuthPolicy], roles: &[String]) -> u8 {
    policies
        .iter()
        .filter(|p| {
            let c = &p.policy.conditions;
            p.policy.enabled && (c.roles.is_empty() || c.roles.iter().any(|r| roles.contains(r)))
        })
        .map(|p| p.policy.level())
        .max()
        .unwrap_or(ACR_PRIMARY)
}

pub async fn list(pool: &SqlitePool, org: Uuid) -> Result<Vec<AuthPolicy>> {
    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, String, i64, i64, String, i64, i64)> = sqlx::query_as(
        "SELECT id, name, enabled, priority, conditions, required_acr, require_phishing_resistant \
         FROM auth_policies WHERE organization_id = ? ORDER BY priority, name",
    )
    .bind(org.to_string())
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|(id, name, enabled, priority, conditions, acr, pr)| {
            Ok(AuthPolicy {
                id: Uuid::parse_str(&id)?,
                policy: PolicyInput {
                    name,
                    enabled: enabled != 0,
                    priority,
                    conditions: serde_json::from_str(&conditions)?,
                    required_acr: acr as u8,
                    require_phishing_resistant: pr != 0,
                },
            })
        })
        .collect()
}

pub fn validate(policies: &[PolicyInput]) -> Result<()> {
    if policies.len() > MAX_POLICIES {
        bail!("At most {MAX_POLICIES} policies are allowed");
    }
    for p in policies {
        if p.name.trim().is_empty() {
            bail!("A policy needs a name");
        }
        if !(ACR_PRIMARY..=ACR_MFA).contains(&p.required_acr) {
            bail!("required_acr must be 1 (primary factor) or 2 (multi-factor)");
        }
        if p.require_phishing_resistant && p.required_acr != ACR_MFA {
            bail!("require_phishing_resistant needs required_acr 2");
        }
    }
    Ok(())
}

/// Replaces the organization's policies as one unit.
pub async fn replace(
    pool: &SqlitePool,
    org: Uuid,
    policies: &[PolicyInput],
) -> Result<Vec<AuthPolicy>> {
    validate(policies)?;
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM auth_policies WHERE organization_id = ?")
        .bind(org.to_string())
        .execute(&mut *tx)
        .await?;
    let now = Utc::now().to_rfc3339();
    for p in policies {
        sqlx::query(
            "INSERT INTO auth_policies (id, organization_id, name, enabled, priority, conditions, required_acr, require_phishing_resistant, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(org.to_string())
        .bind(p.name.trim())
        .bind(p.enabled as i64)
        .bind(p.priority)
        .bind(serde_json::to_string(&p.conditions)?)
        .bind(p.required_acr as i64)
        .bind(p.require_phishing_resistant as i64)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    list(pool, org).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pol(roles: &[&str], providers: &[&str], acr: u8, enabled: bool) -> AuthPolicy {
        AuthPolicy {
            id: Uuid::new_v4(),
            policy: PolicyInput {
                name: "p".into(),
                enabled,
                priority: 1,
                conditions: PolicyConditions {
                    roles: roles.iter().map(|s| s.to_string()).collect(),
                    providers: providers.iter().map(|s| s.to_string()).collect(),
                },
                required_acr: acr,
                require_phishing_resistant: false,
            },
        }
    }

    fn phishing_resistant(mut p: AuthPolicy) -> AuthPolicy {
        p.policy.require_phishing_resistant = true;
        p
    }

    /// OQ-6: a policy may require a phishing-resistant factor (level 3).
    #[test]
    fn phishing_resistant_policies_require_level_three() {
        let ps = vec![
            phishing_resistant(pol(&["admin"], &[], 2, true)),
            pol(&[], &[], 2, true),
            phishing_resistant(pol(&["viewer"], &[], 2, false)),
        ];
        assert_eq!(
            required_acr(&ps, &["admin".into()], "local"),
            ACR_PHISHING_RESISTANT
        );
        assert_eq!(required_acr(&ps, &["viewer".into()], "local"), ACR_MFA);
        // Step-up and enrollment do not know the provider: a role match is
        // enough (conservative).
        let oidc_only = vec![phishing_resistant(pol(&["admin"], &["oidc"], 2, true))];
        assert_eq!(required_acr(&oidc_only, &["admin".into()], "local"), 1);
        assert_eq!(
            required_acr_for_roles(&oidc_only, &["admin".into()]),
            ACR_PHISHING_RESISTANT
        );
        assert_eq!(required_acr_for_roles(&oidc_only, &["viewer".into()]), 1);
    }

    #[test]
    fn phishing_resistance_needs_the_multi_factor_level() {
        let mut p = phishing_resistant(pol(&[], &[], 2, true)).policy;
        assert!(validate(&[p.clone()]).is_ok());
        p.required_acr = 1;
        assert!(validate(&[p]).is_err());
        let parsed: PolicyInput = serde_json::from_value(serde_json::json!({"name": "x"})).unwrap();
        assert!(!parsed.require_phishing_resistant, "off by default");
    }

    #[test]
    fn no_policy_means_primary_level() {
        assert_eq!(required_acr(&[], &["admin".into()], "local"), ACR_PRIMARY);
    }

    #[test]
    fn highest_matching_enabled_policy_wins() {
        let ps = vec![
            pol(&["admin"], &[], 2, true),
            pol(&[], &["oidc"], 2, true),
            pol(&["viewer"], &[], 2, false),
        ];
        assert_eq!(required_acr(&ps, &["admin".into()], "local"), 2);
        assert_eq!(
            required_acr(&ps, &["viewer".into()], "local"),
            1,
            "disabled"
        );
        assert_eq!(required_acr(&ps, &["viewer".into()], "oidc"), 2);
    }

    #[test]
    fn all_non_empty_conditions_must_match() {
        let ps = vec![pol(&["admin"], &["local"], 2, true)];
        assert_eq!(required_acr(&ps, &["admin".into()], "oidc"), 1);
        assert_eq!(required_acr(&ps, &["admin".into()], "local"), 2);
    }

    #[test]
    fn rejects_bad_levels() {
        let mut p = pol(&[], &[], 3, true).policy;
        assert!(validate(&[p.clone()]).is_err());
        p.required_acr = 2;
        p.name = " ".into();
        assert!(validate(&[p]).is_err());
    }
}
