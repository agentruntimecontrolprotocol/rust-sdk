//! Permission challenge and lease lifecycle (RFC §15).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::LeaseId;

/// One entry in a `cost.budget` capability list (ARCP v1.1 §9.6).
///
/// On the wire this is an amount string like `"USD:5.00"` or
/// `"credits:1000"`; `currency` is a free-form identifier (`USD`,
/// `EUR`, `credits`, or runtime-defined) and `amount` is a non-negative
/// decimal.
#[derive(Debug, Clone, PartialEq)]
pub struct CostBudgetAmount {
    /// Currency identifier.
    pub currency: String,
    /// Maximum amount denominated in `currency`.
    pub amount: f64,
}

/// Errors from [`CostBudgetAmount::parse`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CostBudgetParseError {
    /// Missing `:` separator between currency and amount.
    #[error("missing ':' in cost.budget amount {0:?}")]
    MissingSeparator(String),
    /// Currency component was empty.
    #[error("empty currency in cost.budget amount {0:?}")]
    EmptyCurrency(String),
    /// Amount component could not be parsed as a decimal.
    #[error("invalid decimal in cost.budget amount {0:?}")]
    InvalidAmount(String),
    /// Amount component was negative.
    #[error("negative cost.budget amount {0:?}")]
    Negative(String),
}

impl CostBudgetAmount {
    /// Parse an amount string per §9.6 (`currency ":" decimal`).
    ///
    /// # Errors
    ///
    /// Returns [`CostBudgetParseError`] on any grammar violation.
    pub fn parse(input: &str) -> Result<Self, CostBudgetParseError> {
        let Some((currency, rest)) = input.split_once(':') else {
            return Err(CostBudgetParseError::MissingSeparator(input.to_owned()));
        };
        if currency.is_empty() {
            return Err(CostBudgetParseError::EmptyCurrency(input.to_owned()));
        }
        let amount: f64 = rest
            .parse()
            .map_err(|_| CostBudgetParseError::InvalidAmount(input.to_owned()))?;
        if !amount.is_finite() || amount < 0.0 {
            return Err(CostBudgetParseError::Negative(input.to_owned()));
        }
        Ok(Self {
            currency: currency.to_owned(),
            amount,
        })
    }

    /// Wire-level string form (e.g. `"USD:5.00"`).
    #[must_use]
    pub fn format(&self) -> String {
        format!("{}:{}", self.currency, self.amount)
    }
}

impl fmt::Display for CostBudgetAmount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}

impl Serialize for CostBudgetAmount {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.format())
    }
}

impl<'de> Deserialize<'de> for CostBudgetAmount {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(D::Error::custom)
    }
}

/// `cost.budget` lease capability (ARCP v1.1 §9.6).
///
/// Wire shape: an array of amount strings, one per currency, e.g.
/// `["USD:5.00", "credits:1000"]`. Multiple currencies are tracked
/// independently; each is decremented by `metric` events whose `name`
/// begins with `cost.` and whose `unit` matches the currency.
///
/// This type carries only the declared upper bounds. The runtime tracks
/// the remaining counter separately; see `runtime::context::BudgetTracker`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CostBudget {
    /// One entry per currency, declared at job submission time.
    pub amounts: Vec<CostBudgetAmount>,
}

impl CostBudget {
    /// Empty budget.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            amounts: Vec::new(),
        }
    }

    /// True if the budget declares no currencies (i.e. enforcement is
    /// disabled).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.amounts.is_empty()
    }

    /// Look up the declared maximum for `currency`, if any.
    #[must_use]
    pub fn max(&self, currency: &str) -> Option<f64> {
        self.amounts
            .iter()
            .find(|a| a.currency == currency)
            .map(|a| a.amount)
    }

    /// Subset check (ARCP v1.1 §9.4): every currency in `child` must
    /// have a max less than or equal to this budget's remaining amount
    /// for that currency. Returns the first violating currency.
    ///
    /// `remaining` supplies per-currency remaining values (`max -
    /// consumed`); a currency absent from `remaining` is treated as
    /// fully unspent (`remaining == max`).
    #[must_use]
    pub fn subset_violation<'a>(
        &'a self,
        child: &'a Self,
        remaining: &std::collections::HashMap<String, f64>,
    ) -> Option<&'a str> {
        for c in &child.amounts {
            let parent_remaining = remaining.get(&c.currency).copied().or_else(|| {
                self.max(&c.currency)
                // currency must be present on the parent at all
            })?;
            if c.amount > parent_remaining {
                return Some(&c.currency);
            }
        }
        None
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::missing_panics_doc)]
mod cost_budget_tests {
    use super::*;

    #[test]
    fn parse_amount_round_trips() {
        let a = CostBudgetAmount::parse("USD:5.00").unwrap();
        assert_eq!(a.currency, "USD");
        assert!((a.amount - 5.00).abs() < f64::EPSILON);
        assert_eq!(serde_json::to_string(&a).unwrap(), "\"USD:5\"");
    }

    #[test]
    fn parse_amount_rejects_negative() {
        assert!(matches!(
            CostBudgetAmount::parse("USD:-1"),
            Err(CostBudgetParseError::Negative(_))
        ));
    }

    #[test]
    fn parse_amount_rejects_missing_separator() {
        assert!(matches!(
            CostBudgetAmount::parse("USD"),
            Err(CostBudgetParseError::MissingSeparator(_))
        ));
    }

    #[test]
    fn budget_subset_rejects_excess_child() {
        let parent = CostBudget {
            amounts: vec![CostBudgetAmount::parse("USD:5.00").unwrap()],
        };
        let child = CostBudget {
            amounts: vec![CostBudgetAmount::parse("USD:6.00").unwrap()],
        };
        let remaining = std::collections::HashMap::new();
        assert_eq!(parent.subset_violation(&child, &remaining), Some("USD"));
    }

    #[test]
    fn budget_subset_uses_remaining_floor() {
        let parent = CostBudget {
            amounts: vec![CostBudgetAmount::parse("USD:5.00").unwrap()],
        };
        let child = CostBudget {
            amounts: vec![CostBudgetAmount::parse("USD:3.00").unwrap()],
        };
        let mut remaining = std::collections::HashMap::new();
        remaining.insert("USD".into(), 2.0);
        // Parent has $5 budget but only $2 remaining — child's $3
        // exceeds the remaining envelope.
        assert_eq!(parent.subset_violation(&child, &remaining), Some("USD"));
    }

    #[test]
    fn budget_amounts_serialize_as_list_of_strings() {
        let b = CostBudget {
            amounts: vec![
                CostBudgetAmount::parse("USD:5.00").unwrap(),
                CostBudgetAmount::parse("credits:1000").unwrap(),
            ],
        };
        let j = serde_json::to_value(&b).unwrap();
        assert_eq!(j, serde_json::json!(["USD:5", "credits:1000"]));
    }
}

/// Trust level (RFC §15.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    /// External / public.
    Untrusted,
    /// Limited access.
    Constrained,
    /// Internal.
    Trusted,
    /// System-level.
    Privileged,
}

/// Payload for `permission.request` (RFC §15.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionRequestPayload {
    /// Permission name (e.g. `payment.refund.create`).
    pub permission: String,
    /// Resource identifier.
    pub resource: String,
    /// Operation identifier.
    pub operation: String,
    /// Operator-facing reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Requested lease duration in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_lease_seconds: Option<u64>,
}

/// Payload for `permission.grant`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionGrantPayload {
    /// Granted permission.
    pub permission: String,
    /// Resource identifier.
    pub resource: String,
    /// Operation identifier.
    pub operation: String,
    /// Lease duration in seconds.
    pub lease_seconds: u64,
}

/// Payload for `permission.deny`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionDenyPayload {
    /// Denied permission.
    pub permission: String,
    /// Free-form reason.
    pub reason: String,
}

/// Payload for `lease.granted` (RFC §15.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseGrantedPayload {
    /// Newly minted lease id.
    pub lease_id: LeaseId,
    /// Permission the lease covers.
    pub permission: String,
    /// Resource the lease covers.
    pub resource: String,
    /// Operation the lease covers.
    pub operation: String,
    /// Absolute expiry time.
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Payload for `lease.refresh` — holder asks for an extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseRefreshPayload {
    /// The lease being refreshed.
    pub lease_id: LeaseId,
    /// Requested additional duration in seconds.
    pub additional_seconds: u64,
}

/// Payload for `lease.extended`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseExtendedPayload {
    /// The lease that was extended.
    pub lease_id: LeaseId,
    /// New absolute expiry time.
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Payload for `lease.revoked`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseRevokedPayload {
    /// The revoked lease.
    pub lease_id: LeaseId,
    /// Free-form reason for revocation.
    pub reason: String,
}
