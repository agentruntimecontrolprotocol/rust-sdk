//! Job state machine and dispatch (RFC §10).
//!
//! Phase 3 implements the core lifecycle: a `tool.invoke` envelope is
//! turned into a `Job` that runs in its own tokio task with a
//! [`CancellationToken`]. The runtime emits `job.accepted`,
//! `job.started`, then a terminal `job.completed` / `job.failed` /
//! `job.cancelled`.
//!
//! The heartbeat watchdog and hard-kill escalation that the RFC describes
//! in §10.3 / §10.4 land in a follow-up phase; the cooperative cancel
//! path is in place via `CancellationToken`.

use dashmap::DashMap;
use std::sync::Arc;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::credentials::CredentialId;
use crate::ids::{JobId, MessageId, SessionId};
pub use crate::messages::JobState;
use crate::messages::LeaseRequest;

/// Per-job runtime bookkeeping.
#[derive(Debug)]
pub struct JobEntry {
    /// Job identifier.
    pub job_id: JobId,
    /// Owning session.
    pub session_id: SessionId,
    /// Correlation back to the originating `tool.invoke` envelope.
    pub correlation_id: MessageId,
    /// Cancellation token; child of the session token.
    pub cancel: CancellationToken,
    /// Current state.
    pub state: JobState,
    /// Agent reference (`name` or `name@version`) the job is running.
    /// For v1.0-style `tool.invoke` submissions this is the tool name.
    pub agent: String,
    /// Submission timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Highest event sequence emitted for this job (ARCP v1.1 §6.6).
    pub last_event_seq: u64,
    /// Parent job id for delegated / child jobs.
    pub parent_job_id: Option<JobId>,
    /// Provisioned credential ids issued for this job.
    pub credential_ids: Vec<CredentialId>,
    /// Accepted lease constraints for this job.
    pub lease: Option<LeaseRequest>,
}

/// Map of in-flight jobs, keyed by [`JobId`].
///
/// Cheap to clone; internally `Arc<DashMap<JobId, _>>`. The runtime's
/// dispatcher stores the spawned task's `JoinHandle` here so the runtime
/// can issue a hard kill (Phase 4+ surface) and cancellation can be
/// driven from outside the task.
#[derive(Clone, Default)]
pub struct JobRegistry {
    inner: Arc<DashMap<JobId, JobRecord>>,
}

impl std::fmt::Debug for JobRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobRegistry")
            .field("len", &self.inner.len())
            .finish()
    }
}

struct JobRecord {
    entry: JobEntry,
    join: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for JobRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobRecord")
            .field("entry", &self.entry)
            .field(
                "join_finished",
                &self.join.as_ref().is_some_and(JoinHandle::is_finished),
            )
            .finish()
    }
}

impl JobRegistry {
    /// Construct empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a new job; the registry takes ownership of the
    /// [`JoinHandle`] so it can be aborted (Phase 4+).
    pub fn insert(&self, entry: JobEntry, join: JoinHandle<()>) {
        let id = entry.job_id.clone();
        self.inner.insert(
            id,
            JobRecord {
                entry,
                join: Some(join),
            },
        );
    }

    /// Update the state for `job_id`.
    pub fn set_state(&self, job_id: &JobId, state: JobState) {
        if let Some(mut r) = self.inner.get_mut(job_id) {
            r.entry.state = state;
        }
    }

    /// Trigger cooperative cancellation for `job_id`. Returns `true` if
    /// the job was found and the token was triggered.
    #[must_use]
    pub fn cancel(&self, job_id: &JobId) -> bool {
        self.inner.get(job_id).is_some_and(|r| {
            r.entry.cancel.cancel();
            true
        })
    }

    /// Number of in-flight (or recently terminal) jobs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// True if the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Drop terminal jobs from the registry. Should be called periodically
    /// (Phase 5+) to cap memory.
    pub fn sweep_terminals(&self) {
        self.inner.retain(|_, r| !r.entry.state.is_terminal());
    }

    /// Snapshot of all jobs scoped to `session_id`, applying an optional
    /// filter (ARCP v1.1 §6.6). Results are sorted by `created_at`
    /// ascending so pagination cursors are stable.
    #[must_use]
    pub fn list_for_session(
        &self,
        session_id: &SessionId,
        filter: Option<&crate::messages::SessionListJobsFilter>,
    ) -> Vec<crate::messages::JobListEntry> {
        let mut out: Vec<crate::messages::JobListEntry> = self
            .inner
            .iter()
            .filter_map(|r| {
                let e = &r.entry;
                if e.session_id != *session_id {
                    return None;
                }
                if let Some(f) = filter {
                    let status = e.state.wire_str();
                    if !f.status.is_empty() && !f.status.iter().any(|s| s == status) {
                        return None;
                    }
                    if let Some(agent) = f.agent.as_deref() {
                        if e.agent != agent {
                            return None;
                        }
                    }
                    if let Some(after) = f.created_after {
                        if e.created_at <= after {
                            return None;
                        }
                    }
                    if let Some(before) = f.created_before {
                        if e.created_at >= before {
                            return None;
                        }
                    }
                }
                Some(crate::messages::JobListEntry {
                    job_id: e.job_id.clone(),
                    agent: e.agent.clone(),
                    status: e.state.wire_str().to_owned(),
                    parent_job_id: e.parent_job_id.clone(),
                    created_at: e.created_at,
                    trace_id: None,
                    last_event_seq: e.last_event_seq,
                })
            })
            .collect();
        out.sort_by_key(|e| e.created_at);
        out
    }

    /// Increment and return the new `last_event_seq` for `job_id`.
    ///
    /// Returns `None` if the job is not registered.
    #[must_use]
    pub fn bump_event_seq(&self, job_id: &JobId) -> Option<u64> {
        self.inner.get_mut(job_id).map(|mut r| {
            r.entry.last_event_seq += 1;
            r.entry.last_event_seq
        })
    }

    /// Snapshot the public-facing fields of a job, if registered.
    ///
    /// Used by `job.subscribe` (ARCP v1.1 §7.6) to populate the
    /// acknowledgement.
    #[must_use]
    pub fn snapshot(&self, job_id: &JobId) -> Option<JobSnapshot> {
        self.inner.get(job_id).map(|r| {
            let e = &r.entry;
            JobSnapshot {
                job_id: e.job_id.clone(),
                session_id: e.session_id.clone(),
                state: e.state,
                agent: e.agent.clone(),
                parent_job_id: e.parent_job_id.clone(),
                last_event_seq: e.last_event_seq,
            }
        })
    }
}

/// Public projection of [`JobEntry`] returned by [`JobRegistry::snapshot`].
#[derive(Debug, Clone)]
pub struct JobSnapshot {
    /// Job identifier.
    pub job_id: JobId,
    /// Originating session.
    pub session_id: SessionId,
    /// Current state.
    pub state: JobState,
    /// Agent reference (`name` or `name@version`) the job is running.
    pub agent: String,
    /// Parent job id for delegated / child jobs.
    pub parent_job_id: Option<JobId>,
    /// Highest event sequence emitted for this job.
    pub last_event_seq: u64,
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
    use crate::ids::{JobId, MessageId, SessionId};

    fn make_entry(state: JobState) -> (JobEntry, tokio::task::JoinHandle<()>) {
        let cancel = CancellationToken::new();
        let entry = JobEntry {
            job_id: JobId::new(),
            session_id: SessionId::new(),
            correlation_id: MessageId::new(),
            cancel,
            state,
            agent: "test-tool".to_owned(),
            created_at: chrono::Utc::now(),
            last_event_seq: 0,
            parent_job_id: None,
            credential_ids: vec![],
            lease: None,
        };
        // A no-op task so the JoinHandle is well-formed.
        let join = tokio::spawn(async {});
        (entry, join)
    }

    #[test]
    fn job_state_terminals_are_classified_correctly() {
        for s in [JobState::Completed, JobState::Failed, JobState::Cancelled] {
            assert!(s.is_terminal(), "{s:?} should be terminal");
        }
        for s in [
            JobState::Accepted,
            JobState::Queued,
            JobState::Running,
            JobState::Blocked,
            JobState::Paused,
        ] {
            assert!(!s.is_terminal(), "{s:?} should NOT be terminal");
        }
    }

    #[tokio::test]
    async fn registry_insert_and_set_state_round_trip() {
        let reg = JobRegistry::new();
        assert!(reg.is_empty());
        let (entry, join) = make_entry(JobState::Accepted);
        let id = entry.job_id.clone();
        reg.insert(entry, join);
        assert_eq!(reg.len(), 1);
        reg.set_state(&id, JobState::Running);
    }

    #[tokio::test]
    async fn cancel_returns_false_for_unknown_job() {
        let reg = JobRegistry::new();
        let id = JobId::new();
        assert!(!reg.cancel(&id));
    }

    #[tokio::test]
    async fn cancel_triggers_token_for_known_job() {
        let reg = JobRegistry::new();
        let (entry, join) = make_entry(JobState::Running);
        let token = entry.cancel.clone();
        let id = entry.job_id.clone();
        reg.insert(entry, join);
        assert!(reg.cancel(&id));
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn sweep_terminals_drops_only_terminal_jobs() {
        let reg = JobRegistry::new();
        let (running, jh1) = make_entry(JobState::Running);
        let (done, jh2) = make_entry(JobState::Completed);
        let running_id = running.job_id.clone();
        let done_id = done.job_id.clone();
        reg.insert(running, jh1);
        reg.insert(done, jh2);
        assert_eq!(reg.len(), 2);
        reg.sweep_terminals();
        assert_eq!(reg.len(), 1);
        // Sweep is idempotent.
        reg.sweep_terminals();
        assert_eq!(reg.len(), 1);
        // Running job is the survivor; cancel still finds it.
        assert!(reg.cancel(&running_id));
        // Terminal job was already swept.
        assert!(!reg.cancel(&done_id));
    }
}
