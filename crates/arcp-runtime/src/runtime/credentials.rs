//! Provisioned credential support for lease-bound jobs (ARCP v1.1 §9.8).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use arcp_core::error::ARCPError;
use arcp_core::ids::{JobId, SessionId};
use arcp_core::messages::LeaseRequest;

// Wire types now live in `arcp_core::messages`; re-export them at the runtime
// `credentials` path for backwards compatibility with v1.x users.
pub use arcp_core::messages::{CredentialId, CredentialScheme, ProvisionedCredential};

/// Job metadata supplied to a [`CredentialProvisioner`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialJobContext {
    /// Job receiving the credential.
    pub job_id: JobId,
    /// Owning session.
    pub session_id: SessionId,
    /// Authenticated principal, if any.
    pub principal: Option<String>,
    /// Parent job for delegated jobs.
    pub parent_job_id: Option<JobId>,
}

/// Vendor-neutral async provisioner interface.
#[async_trait]
pub trait CredentialProvisioner: Send + Sync {
    /// Issue credentials constrained by `lease` for `ctx`.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError`] when the upstream cannot issue the requested
    /// credential.
    async fn issue(
        &self,
        lease: &LeaseRequest,
        ctx: &CredentialJobContext,
    ) -> Result<Vec<ProvisionedCredential>, ARCPError>;

    /// Revoke one credential by id.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError`] when revocation failed.
    async fn revoke(&self, id: &CredentialId) -> Result<(), ARCPError>;
}

/// Reference provisioner for tests and examples.
#[derive(Default)]
pub struct InMemoryCredentialProvisioner {
    counter: AtomicU64,
    issued: Mutex<Vec<ProvisionedCredential>>,
    revoked: Mutex<Vec<CredentialId>>,
}

impl std::fmt::Debug for InMemoryCredentialProvisioner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryCredentialProvisioner")
            .finish_non_exhaustive()
    }
}

impl InMemoryCredentialProvisioner {
    /// Snapshot issued credentials. Secret values are present for test assertions.
    #[must_use]
    pub fn issued_credentials(&self) -> Vec<ProvisionedCredential> {
        self.issued
            .lock()
            .map_or_else(|_| Vec::new(), |g| g.clone())
    }

    /// Snapshot revoked credential ids.
    #[must_use]
    pub fn revoked_ids(&self) -> Vec<CredentialId> {
        self.revoked
            .lock()
            .map_or_else(|_| Vec::new(), |g| g.clone())
    }

    /// Validate a child credential lease against a parent lease.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::LeaseSubsetViolation`] when `child` widens `parent`.
    pub fn validate_child_constraints(
        parent: &LeaseRequest,
        child: &LeaseRequest,
    ) -> Result<(), ARCPError> {
        if let Some(violation) = parent.subset_violation(child, &HashMap::new()) {
            return Err(ARCPError::LeaseSubsetViolation {
                detail: format!("{violation:?}"),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl CredentialProvisioner for InMemoryCredentialProvisioner {
    async fn issue(
        &self,
        lease: &LeaseRequest,
        _ctx: &CredentialJobContext,
    ) -> Result<Vec<ProvisionedCredential>, ARCPError> {
        let n = self.counter.fetch_add(1, Ordering::AcqRel) + 1;
        let credential = ProvisionedCredential {
            id: CredentialId::new(n),
            scheme: CredentialScheme::Bearer,
            value: format!("test-token-{n}"),
            endpoint: "https://example.invalid/llm".into(),
            profile: Some("test".into()),
            constraints: Some(lease.clone()),
        };
        self.issued
            .lock()
            .map_err(|_| ARCPError::Internal {
                detail: "credential provisioner mutex poisoned".into(),
            })?
            .push(credential.clone());
        Ok(vec![credential])
    }

    async fn revoke(&self, id: &CredentialId) -> Result<(), ARCPError> {
        self.revoked
            .lock()
            .map_err(|_| ARCPError::Internal {
                detail: "credential provisioner mutex poisoned".into(),
            })?
            .push(id.clone());
        Ok(())
    }
}

/// In-memory ledger of outstanding credential ids by job.
#[derive(Clone, Default)]
pub struct CredentialLedger {
    inner: Arc<dashmap::DashMap<JobId, Vec<CredentialId>>>,
}

impl std::fmt::Debug for CredentialLedger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialLedger")
            .field("jobs", &self.inner.len())
            .finish()
    }
}

impl CredentialLedger {
    /// Construct an empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record credentials issued for `job_id`.
    pub fn record_issued(&self, job_id: &JobId, credentials: &[ProvisionedCredential]) {
        if credentials.is_empty() {
            return;
        }
        let ids = credentials.iter().map(|c| c.id.clone()).collect::<Vec<_>>();
        self.inner
            .entry(job_id.clone())
            .and_modify(|existing| existing.extend(ids.clone()))
            .or_insert(ids);
    }

    /// Outstanding ids for `job_id`.
    #[must_use]
    pub fn outstanding_for_job(&self, job_id: &JobId) -> Vec<CredentialId> {
        self.inner
            .get(job_id)
            .map_or_else(Vec::new, |entry| entry.value().clone())
    }

    /// Mark a credential as revoked.
    pub fn mark_revoked(&self, job_id: &JobId, credential_id: &CredentialId) {
        if let Some(mut ids) = self.inner.get_mut(job_id) {
            ids.retain(|id| id != credential_id);
            if ids.is_empty() {
                drop(ids);
                self.inner.remove(job_id);
            }
        }
    }
}

/// Revoke every outstanding credential for a job.
///
/// # Errors
///
/// Returns the last revocation error if all retry attempts fail for any
/// credential.
pub async fn revoke_all_for_job(
    ledger: &CredentialLedger,
    provisioner: &Arc<dyn CredentialProvisioner>,
    job_id: &JobId,
) -> Result<(), ARCPError> {
    let mut last_error = None;
    for id in ledger.outstanding_for_job(job_id) {
        let mut revoked = false;
        for attempt in 0..3 {
            match provisioner.revoke(&id).await {
                Ok(()) => {
                    ledger.mark_revoked(job_id, &id);
                    revoked = true;
                    break;
                }
                Err(e) => {
                    last_error = Some(e);
                    let delay = 10_u64.saturating_mul(1 << attempt);
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }
            }
        }
        if !revoked {
            tracing::warn!(credential_id = %id, job_id = %job_id, "credential revocation failed");
        }
    }
    if ledger.outstanding_for_job(job_id).is_empty() {
        Ok(())
    } else {
        Err(last_error.unwrap_or_else(|| ARCPError::Unavailable {
            detail: "credential revocation failed".into(),
        }))
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]
mod tests {
    use super::*;
    use arcp_core::messages::{CostBudget, CostBudgetAmount, ModelUse};

    fn lease(pattern: &str) -> LeaseRequest {
        LeaseRequest {
            cost_budget: Some(CostBudget {
                amounts: vec![CostBudgetAmount {
                    currency: "USD".into(),
                    amount: 1.0,
                }],
            }),
            model_use: Some(ModelUse {
                patterns: vec![pattern.into()],
            }),
            expires_at: None,
            extra: std::collections::BTreeMap::default(),
        }
    }

    #[tokio::test]
    async fn in_memory_provisioner_issues_and_revokes_round_trip() {
        let provisioner = InMemoryCredentialProvisioner::default();
        let ctx = CredentialJobContext {
            job_id: JobId::new(),
            session_id: SessionId::new(),
            principal: Some("p".into()),
            parent_job_id: None,
        };
        let creds = provisioner
            .issue(&lease("tier-fast/*"), &ctx)
            .await
            .expect("issue");
        assert_eq!(creds.len(), 1);
        assert_eq!(creds[0].value, "test-token-1");
        provisioner.revoke(&creds[0].id).await.expect("revoke");
        assert_eq!(provisioner.revoked_ids(), vec![creds[0].id.clone()]);
    }

    #[test]
    fn ledger_records_outstanding_until_revoke() {
        let ledger = CredentialLedger::new();
        let job_id = JobId::new();
        let credential = ProvisionedCredential {
            id: CredentialId::new(7),
            scheme: CredentialScheme::Bearer,
            value: "secret".into(),
            endpoint: "https://example.invalid".into(),
            profile: None,
            constraints: None,
        };
        ledger.record_issued(&job_id, std::slice::from_ref(&credential));
        assert_eq!(
            ledger.outstanding_for_job(&job_id),
            vec![credential.id.clone()]
        );
        ledger.mark_revoked(&job_id, &credential.id);
        assert!(ledger.outstanding_for_job(&job_id).is_empty());
    }

    #[test]
    fn child_credential_must_be_subset_of_parent() {
        let parent = lease("tier-fast/*");
        let child = lease("tier-fast/small");
        InMemoryCredentialProvisioner::validate_child_constraints(&parent, &child).expect("subset");
        let widened = lease("*");
        assert!(matches!(
            InMemoryCredentialProvisioner::validate_child_constraints(&parent, &widened),
            Err(ARCPError::LeaseSubsetViolation { .. })
        ));
    }
}
