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

use crate::ids::{JobId, MessageId, SessionId};
pub use crate::messages::JobState;

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

    /// Iterate the cancellation tokens of all in-flight jobs.
    pub(crate) fn inner_iter(&self) -> Vec<CancellationToken> {
        self.inner.iter().map(|r| r.entry.cancel.clone()).collect()
    }
}
