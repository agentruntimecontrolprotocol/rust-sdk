//! Permission challenge and lease lifecycle (ARCP v1.1 §9).
//!
//! Capability grammar lives at §9.2; enforcement at §9.3; subsetting at
//! §9.4; expiration at §9.5; budgets at §9.6; model use at §9.7;
//! provisioned credentials at §9.8.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use chrono::{DateTime, Utc};
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
        remaining: &HashMap<String, f64>,
    ) -> Option<&'a str> {
        for c in &child.amounts {
            let parent_remaining = remaining
                .get(&c.currency)
                .copied()
                .or_else(|| self.max(&c.currency));
            let Some(parent_remaining) = parent_remaining else {
                return Some(&c.currency);
            };
            if c.amount > parent_remaining {
                return Some(&c.currency);
            }
        }
        None
    }
}

/// `model.use` lease capability (ARCP v1.1 §9.7).
///
/// Wire shape is a list of model glob patterns. The Rust SDK supports the
/// protocol's minimal `*` wildcard semantics: all other characters match
/// literally and fragments must appear in order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelUse {
    /// Permitted model glob patterns.
    pub patterns: Vec<String>,
}

impl ModelUse {
    /// Empty model-use constraint.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    /// True when `model` is permitted by one of this lease's patterns.
    #[must_use]
    pub fn matches(&self, model: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| glob_star_matches(pattern, model))
    }

    /// Subset check (ARCP v1.1 §9.4): every child pattern must be
    /// implied by at least one parent pattern. Returns the first child
    /// pattern that widens the parent envelope.
    #[must_use]
    pub fn subset_violation<'a>(&'a self, child: &'a Self) -> Option<&'a str> {
        child
            .patterns
            .iter()
            .find(|pattern| {
                !self
                    .patterns
                    .iter()
                    .any(|parent| glob_pattern_subsumes(parent, pattern))
            })
            .map(String::as_str)
    }
}

fn glob_star_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == value;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut rest = value;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if index == 0 {
            let Some(next) = rest.strip_prefix(part) else {
                return false;
            };
            rest = next;
            continue;
        }
        let Some(pos) = rest.find(part) else {
            return false;
        };
        rest = &rest[pos + part.len()..];
    }
    pattern.ends_with('*')
        || parts
            .last()
            .is_none_or(|last| rest.is_empty() || last.is_empty())
}

fn glob_pattern_subsumes(parent: &str, child: &str) -> bool {
    if parent == "*" || parent == child {
        return true;
    }
    if !parent.contains('*') {
        return false;
    }
    if !child.contains('*') {
        return glob_star_matches(parent, child);
    }

    let parent_prefix = parent.split_once('*').map_or(parent, |(prefix, _)| prefix);
    let child_prefix = child.split_once('*').map_or(child, |(prefix, _)| prefix);
    let parent_suffix = parent.rsplit_once('*').map_or(parent, |(_, suffix)| suffix);
    let child_suffix = child.rsplit_once('*').map_or(child, |(_, suffix)| suffix);

    child_prefix.starts_with(parent_prefix) && child_suffix.ends_with(parent_suffix)
}

/// `lease_request` capability block carried on `tool.invoke` per ARCP v1.1 §9.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LeaseRequest {
    /// Optional `cost.budget` capability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_budget: Option<CostBudget>,
    /// Optional `model.use` capability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_use: Option<ModelUse>,
    /// Lease expiry bound applied to any child lease or credential TTL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// Forward-compatible unknown lease capabilities.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl LeaseRequest {
    /// Empty lease request.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// True when the lease carries no known or extension capabilities.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cost_budget.is_none()
            && self.model_use.is_none()
            && self.expires_at.is_none()
            && self.extra.is_empty()
    }

    /// Validate the lease request at submission time against `now`
    /// (ARCP v1.1 §9.5).
    ///
    /// §9.5 requires `expires_at` to be UTC (`Z` suffix) and strictly in
    /// the future at submission; past or equal-to-now values are rejected
    /// with `INVALID_REQUEST`. The UTC invariant is enforced at the
    /// `DateTime<Utc>` type level; this method covers the future-ness
    /// check. Callers should invoke it before emitting `job.accepted`.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::InvalidRequest`] when `expires_at` is set and
    /// not strictly greater than `now`.
    pub fn validate_at(&self, now: DateTime<Utc>) -> Result<(), crate::error::ARCPError> {
        if let Some(expires_at) = self.expires_at {
            if expires_at <= now {
                return Err(crate::error::ARCPError::InvalidRequest {
                    detail: format!(
                        "lease.expires_at MUST be in the future at submission time \
                         (ARCP v1.1 §9.5): expires_at={expires_at} now={now}"
                    ),
                });
            }
        }
        Ok(())
    }

    /// Convenience wrapper around [`Self::validate_at`] using
    /// [`chrono::Utc::now`].
    ///
    /// # Errors
    ///
    /// See [`Self::validate_at`].
    pub fn validate(&self) -> Result<(), crate::error::ARCPError> {
        self.validate_at(Utc::now())
    }

    /// True when the lease carries an `expires_at` that is at or before
    /// `now` (ARCP v1.1 §9.5). Absent `expires_at` never expires.
    #[must_use]
    pub fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_some_and(|expires_at| now >= expires_at)
    }

    /// §9.4 subset check across all known lease capabilities.
    #[must_use]
    pub fn subset_violation(
        &self,
        child: &Self,
        remaining_budget: &HashMap<String, f64>,
    ) -> Option<LeaseSubsetViolation> {
        if let Some(child_budget) = child.cost_budget.as_ref() {
            let Some(parent_budget) = self.cost_budget.as_ref() else {
                return Some(LeaseSubsetViolation::CostBudget(
                    child_budget
                        .amounts
                        .first()
                        .map_or_else(|| "cost.budget".to_owned(), |a| a.currency.clone()),
                ));
            };
            if let Some(currency) = parent_budget.subset_violation(child_budget, remaining_budget) {
                return Some(LeaseSubsetViolation::CostBudget(currency.to_owned()));
            }
        }
        if let Some(child_models) = child.model_use.as_ref() {
            let Some(parent_models) = self.model_use.as_ref() else {
                return Some(LeaseSubsetViolation::ModelUse(
                    child_models
                        .patterns
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "model.use".to_owned()),
                ));
            };
            if let Some(pattern) = parent_models.subset_violation(child_models) {
                return Some(LeaseSubsetViolation::ModelUse(pattern.to_owned()));
            }
        }
        if let Some(child_expiry) = child.expires_at {
            let Some(parent_expiry) = self.expires_at else {
                return Some(LeaseSubsetViolation::ExpiresAtBeyondParent);
            };
            if child_expiry > parent_expiry {
                return Some(LeaseSubsetViolation::ExpiresAtBeyondParent);
            }
        }
        None
    }
}

/// Known reasons a child lease can fail ARCP v1.1 §9.4 subsetting.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum LeaseSubsetViolation {
    /// Child `cost.budget` exceeds the parent's remaining budget.
    CostBudget(String),
    /// Child `model.use` widens the parent model set.
    ModelUse(String),
    /// Child expiry exceeds parent expiry or parent has no expiry bound.
    ExpiresAtBeyondParent,
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::missing_panics_doc)]
mod model_use_tests {
    use super::*;

    #[test]
    fn parse_round_trips_through_serde() {
        let model_use = ModelUse {
            patterns: vec!["tier-fast/*".into()],
        };
        let json = serde_json::to_string(&model_use).unwrap();
        assert_eq!(json, "[\"tier-fast/*\"]");
        let back: ModelUse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, model_use);
    }

    #[test]
    fn matches_exact_and_glob() {
        let model_use = ModelUse {
            patterns: vec!["tier-fast/*".into(), "anthropic/claude-3-haiku".into()],
        };
        assert!(model_use.matches("tier-fast/small"));
        assert!(model_use.matches("anthropic/claude-3-haiku"));
        assert!(!model_use.matches("tier-slow/small"));
    }

    #[test]
    fn subset_rejects_expanded_set() {
        let parent = ModelUse {
            patterns: vec!["tier-fast/*".into()],
        };
        let child = ModelUse {
            patterns: vec!["*".into()],
        };
        assert_eq!(parent.subset_violation(&child), Some("*"));
    }

    #[test]
    fn subset_accepts_equal_or_narrower() {
        let parent = ModelUse {
            patterns: vec!["tier-fast/*".into()],
        };
        let equal = ModelUse {
            patterns: vec!["tier-fast/*".into()],
        };
        let narrower = ModelUse {
            patterns: vec!["tier-fast/small".into()],
        };
        assert!(parent.subset_violation(&equal).is_none());
        assert!(parent.subset_violation(&narrower).is_none());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::missing_panics_doc)]
mod lease_request_tests {
    use super::*;

    fn budget(amount: f64) -> CostBudget {
        CostBudget {
            amounts: vec![CostBudgetAmount {
                currency: "USD".into(),
                amount,
            }],
        }
    }

    #[test]
    fn subset_rejects_cost_budget_overrun() {
        let parent = LeaseRequest {
            cost_budget: Some(budget(5.0)),
            ..LeaseRequest::default()
        };
        let child = LeaseRequest {
            cost_budget: Some(budget(6.0)),
            ..LeaseRequest::default()
        };
        assert_eq!(
            parent.subset_violation(&child, &HashMap::new()),
            Some(LeaseSubsetViolation::CostBudget("USD".into()))
        );
    }

    #[test]
    fn subset_rejects_model_use_widening() {
        let parent = LeaseRequest {
            model_use: Some(ModelUse {
                patterns: vec!["tier-fast/*".into()],
            }),
            ..LeaseRequest::default()
        };
        let child = LeaseRequest {
            model_use: Some(ModelUse {
                patterns: vec!["*".into()],
            }),
            ..LeaseRequest::default()
        };
        assert_eq!(
            parent.subset_violation(&child, &HashMap::new()),
            Some(LeaseSubsetViolation::ModelUse("*".into()))
        );
    }

    #[test]
    fn subset_rejects_expiry_beyond_parent() {
        let now = Utc::now();
        let parent = LeaseRequest {
            expires_at: Some(now),
            ..LeaseRequest::default()
        };
        let child = LeaseRequest {
            expires_at: Some(now + chrono::Duration::seconds(1)),
            ..LeaseRequest::default()
        };
        assert_eq!(
            parent.subset_violation(&child, &HashMap::new()),
            Some(LeaseSubsetViolation::ExpiresAtBeyondParent)
        );
    }

    #[test]
    fn validate_at_rejects_past_and_equal_expires_at() {
        let now = Utc::now();
        // Past.
        let past = LeaseRequest {
            expires_at: Some(now - chrono::Duration::seconds(1)),
            ..LeaseRequest::default()
        };
        assert!(past.validate_at(now).is_err());
        // Exactly now (not strictly future).
        let equal = LeaseRequest {
            expires_at: Some(now),
            ..LeaseRequest::default()
        };
        assert!(equal.validate_at(now).is_err());
        // Future is accepted.
        let future = LeaseRequest {
            expires_at: Some(now + chrono::Duration::seconds(1)),
            ..LeaseRequest::default()
        };
        assert!(future.validate_at(now).is_ok());
        // Absent expires_at is always valid.
        assert!(LeaseRequest::default().validate_at(now).is_ok());
    }

    #[test]
    fn is_expired_at_tracks_the_deadline() {
        let now = Utc::now();
        let lease = LeaseRequest {
            expires_at: Some(now),
            ..LeaseRequest::default()
        };
        assert!(lease.is_expired_at(now));
        assert!(lease.is_expired_at(now + chrono::Duration::seconds(1)));
        assert!(!lease.is_expired_at(now - chrono::Duration::seconds(1)));
        // No expiry never expires.
        assert!(!LeaseRequest::default().is_expired_at(now));
    }
}

/// Trust level (ARCP v1.1 §9; capability model).
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

/// Payload for `permission.request` (SDK extension; v1.1 §9 expresses
/// lease requests on `job.submit` payloads).
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

/// Payload for `lease.granted` (ARCP v1.1 §9; see also §9.5 expiration).
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
