//! Append-only `SQLite` event log.
//!
//! The reader paths (`list`, `get_by_id`, `count`) hold the connection
//! mutex for the duration of the SQL call inside `spawn_blocking`.
//! Clippy's `significant_drop_tightening` lint asks us to release the
//! mutex earlier, but the SQL call IS the only thing the closure does
//! and there's no concurrent contention path that benefits — every
//! caller goes through the same `spawn_blocking`. Suppressed
//! module-wide with rationale.
//!
//! Three operations matter:
//!
//! - [`EventLog::append`] inserts an envelope row and returns whether it
//!   was a new insert. A repeat insert with the same `(session_id, id)` is
//!   silently absorbed (transport-level dedup; logical idempotency for
//!   command intents is ARCP v1.1 §7.2).
//! - [`EventLog::list`] enumerates rows by `(session_id, after_rowid)` for
//!   subscription backfill (ARCP v1.1 §7.6) and resume (ARCP v1.1 §6.3).
//! - [`EventLog::get_by_id`] fetches a single row by message id.
//!
//! The synchronous `rusqlite` calls run inside `tokio::task::spawn_blocking`
//! behind an async facade so the event log can be used from inside the
//! `tokio` reactor without blocking it.

#![allow(clippy::significant_drop_tightening)]

use std::path::Path;
use std::sync::Arc;

use rusqlite::{params, Connection, OptionalExtension};
use tokio::sync::Mutex;
use tokio::task;

use arcp_core::envelope::{Envelope, RawEnvelope};
use arcp_core::error::ARCPError;

const SCHEMA: &str = include_str!("schema.sql");

/// Append-only `SQLite` event log.
///
/// Cheap to clone; internally holds an `Arc<Mutex<Connection>>` so that
/// concurrent calls serialise on the underlying connection.
#[derive(Clone)]
pub struct EventLog {
    inner: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for EventLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventLog").finish_non_exhaustive()
    }
}

/// Result of an [`EventLog::append`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppendOutcome {
    /// Row was inserted.
    Inserted,
    /// A row with the same `(session_id, id)` already existed; the insert
    /// was a no-op (transport-level dedup; not the same as ARCP v1.1 §7.2
    /// logical idempotency on `tool.invoke.idempotency_key`).
    Duplicate,
}

/// One persisted log row, returned from queries.
#[derive(Debug, Clone)]
pub struct LoggedEvent {
    /// Auto-incrementing primary key; gives total replay order.
    pub rowid: i64,
    /// The envelope as stored.
    pub envelope: RawEnvelope,
}

impl EventLog {
    /// Open an in-memory event log. Convenient for tests.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Storage`] if `SQLite` cannot create the in-memory
    /// database or apply the schema.
    pub async fn in_memory() -> Result<Self, ARCPError> {
        task::spawn_blocking(move || {
            let conn = Connection::open_in_memory()?;
            conn.execute_batch(SCHEMA)?;
            Ok::<_, rusqlite::Error>(Self {
                inner: Arc::new(Mutex::new(conn)),
            })
        })
        .await
        .map_err(|join| ARCPError::Internal {
            detail: format!("event log spawn_blocking join: {join}"),
        })?
        .map_err(|e| ARCPError::Storage {
            detail: e.to_string(),
        })
    }

    /// Open (or create) an event log backed by `path`.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Storage`] if `SQLite` cannot open or create the
    /// file or apply the schema.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, ARCPError> {
        let path = path.as_ref().to_path_buf();
        task::spawn_blocking(move || {
            let conn = Connection::open(&path)?;
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "synchronous", "NORMAL")?;
            conn.execute_batch(SCHEMA)?;
            Ok::<_, rusqlite::Error>(Self {
                inner: Arc::new(Mutex::new(conn)),
            })
        })
        .await
        .map_err(|join| ARCPError::Internal {
            detail: format!("event log spawn_blocking join: {join}"),
        })?
        .map_err(|e| ARCPError::Storage {
            detail: e.to_string(),
        })
    }

    /// Append one envelope to the log. Idempotent on `(session_id, id)`.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Serialization`] if the envelope cannot be
    /// serialised, [`ARCPError::Storage`] for any underlying `SQLite` error.
    pub async fn append(&self, envelope: &Envelope) -> Result<AppendOutcome, ARCPError> {
        let raw = envelope.clone().into_raw()?;
        let body = serde_json::to_string(&raw.payload)?;
        let inner = Arc::clone(&self.inner);
        task::spawn_blocking(move || {
            let session_id_str = raw.session_id.as_ref().map(ToString::to_string);
            let job_id_str = raw.job_id.as_ref().map(ToString::to_string);
            let stream_id_str = raw.stream_id.as_ref().map(ToString::to_string);
            let subscription_id_str = raw.subscription_id.as_ref().map(ToString::to_string);
            let correlation_id_str = raw.correlation_id.as_ref().map(ToString::to_string);
            let causation_id_str = raw.causation_id.as_ref().map(ToString::to_string);
            let trace_id_str = raw.trace_id.as_ref().map(ToString::to_string);
            let span_id_str = raw.span_id.as_ref().map(ToString::to_string);
            let idempotency_key_str = raw.idempotency_key.as_ref().map(ToString::to_string);
            let timestamp_str = raw.timestamp.to_rfc3339();
            let priority_str = priority_str(raw.priority);
            let event_seq = raw.event_seq.map(|s| i64::try_from(s).unwrap_or(i64::MAX));

            let conn = inner.blocking_lock();
            let changed = conn.execute(
                "INSERT OR IGNORE INTO events (
                    id, session_id, job_id, stream_id, subscription_id,
                    type_name, correlation_id, causation_id,
                    trace_id, span_id, idempotency_key,
                    timestamp_utc, priority, event_seq, body
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    raw.id.to_string(),
                    session_id_str,
                    job_id_str,
                    stream_id_str,
                    subscription_id_str,
                    raw.type_name,
                    correlation_id_str,
                    causation_id_str,
                    trace_id_str,
                    span_id_str,
                    idempotency_key_str,
                    timestamp_str,
                    priority_str,
                    event_seq,
                    body,
                ],
            )?;
            Ok::<_, rusqlite::Error>(if changed == 1 {
                AppendOutcome::Inserted
            } else {
                AppendOutcome::Duplicate
            })
        })
        .await
        .map_err(|join| ARCPError::Internal {
            detail: format!("event log spawn_blocking join: {join}"),
        })?
        .map_err(|e| ARCPError::Storage {
            detail: e.to_string(),
        })
    }

    /// List rows for `session_id` strictly after `after_rowid`, in replay
    /// order, capped at `limit` rows.
    ///
    /// Pass `after_rowid = 0` to start from the beginning.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Storage`] for any underlying `SQLite` error.
    pub async fn list(
        &self,
        session_id: &str,
        after_rowid: i64,
        limit: i64,
    ) -> Result<Vec<LoggedEvent>, ARCPError> {
        let inner = Arc::clone(&self.inner);
        let session_id = session_id.to_owned();
        task::spawn_blocking(move || -> Result<Vec<LoggedEvent>, rusqlite::Error> {
            let conn = inner.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT rowid, id, session_id, job_id, stream_id, subscription_id,
                    type_name, correlation_id, causation_id,
                    trace_id, span_id, idempotency_key,
                    timestamp_utc, priority, event_seq, body
                 FROM events
                 WHERE session_id = ?1 AND rowid > ?2
                 ORDER BY rowid ASC
                 LIMIT ?3",
            )?;
            let rows = stmt.query_map(params![session_id, after_rowid, limit], row_to_logged)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
        .await
        .map_err(|join| ARCPError::Internal {
            detail: format!("event log spawn_blocking join: {join}"),
        })?
        .map_err(|e| ARCPError::Storage {
            detail: e.to_string(),
        })
    }

    /// Replay buffered events for `job_id` whose `event_seq` is strictly
    /// greater than `from_event_seq`, in sequence order (ARCP v1.1 §7.6
    /// `job.subscribe` history replay). Only rows carrying an `event_seq`
    /// (countable job events) are returned.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Storage`] for any underlying `SQLite` error.
    pub async fn replay_job_events_after_seq(
        &self,
        job_id: &str,
        from_event_seq: u64,
        limit: i64,
    ) -> Result<Vec<LoggedEvent>, ARCPError> {
        let inner = Arc::clone(&self.inner);
        let job_id = job_id.to_owned();
        let from_seq = i64::try_from(from_event_seq).unwrap_or(i64::MAX);
        task::spawn_blocking(move || -> Result<Vec<LoggedEvent>, rusqlite::Error> {
            let conn = inner.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT rowid, id, session_id, job_id, stream_id, subscription_id,
                    type_name, correlation_id, causation_id,
                    trace_id, span_id, idempotency_key,
                    timestamp_utc, priority, event_seq, body
                 FROM events
                 WHERE job_id = ?1 AND event_seq IS NOT NULL AND event_seq > ?2
                 ORDER BY event_seq ASC
                 LIMIT ?3",
            )?;
            let rows = stmt.query_map(params![job_id, from_seq, limit], row_to_logged)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
        .await
        .map_err(|join| ARCPError::Internal {
            detail: format!("event log spawn_blocking join: {join}"),
        })?
        .map_err(|e| ARCPError::Storage {
            detail: e.to_string(),
        })
    }

    /// Fetch a single row by message id.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Storage`] for any underlying `SQLite` error.
    pub async fn get_by_id(&self, id: &str) -> Result<Option<LoggedEvent>, ARCPError> {
        let inner = Arc::clone(&self.inner);
        let id = id.to_owned();
        task::spawn_blocking(move || -> Result<Option<LoggedEvent>, rusqlite::Error> {
            let conn = inner.blocking_lock();
            let result = conn
                .query_row(
                    "SELECT rowid, id, session_id, job_id, stream_id, subscription_id,
                        type_name, correlation_id, causation_id,
                        trace_id, span_id, idempotency_key,
                        timestamp_utc, priority, event_seq, body
                     FROM events WHERE id = ?1",
                    params![id],
                    row_to_logged,
                )
                .optional()?;
            Ok(result)
        })
        .await
        .map_err(|join| ARCPError::Internal {
            detail: format!("event log spawn_blocking join: {join}"),
        })?
        .map_err(|e| ARCPError::Storage {
            detail: e.to_string(),
        })
    }

    /// Total event count (across all sessions). Useful for tests.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Storage`] for any underlying `SQLite` error.
    pub async fn count(&self) -> Result<i64, ARCPError> {
        let inner = Arc::clone(&self.inner);
        task::spawn_blocking(move || -> Result<i64, rusqlite::Error> {
            let conn = inner.blocking_lock();
            let n: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
            Ok(n)
        })
        .await
        .map_err(|join| ARCPError::Internal {
            detail: format!("event log spawn_blocking join: {join}"),
        })?
        .map_err(|e| ARCPError::Storage {
            detail: e.to_string(),
        })
    }
}

const fn priority_str(p: arcp_core::envelope::Priority) -> &'static str {
    match p {
        arcp_core::envelope::Priority::Low => "low",
        arcp_core::envelope::Priority::Normal => "normal",
        arcp_core::envelope::Priority::High => "high",
        arcp_core::envelope::Priority::Critical => "critical",
    }
}

fn row_to_logged(row: &rusqlite::Row<'_>) -> rusqlite::Result<LoggedEvent> {
    let rowid: i64 = row.get("rowid")?;
    let id: String = row.get("id")?;
    let session_id: Option<String> = row.get("session_id")?;
    let job_id: Option<String> = row.get("job_id")?;
    let stream_id: Option<String> = row.get("stream_id")?;
    let subscription_id: Option<String> = row.get("subscription_id")?;
    let type_name: String = row.get("type_name")?;
    let correlation_id: Option<String> = row.get("correlation_id")?;
    let causation_id: Option<String> = row.get("causation_id")?;
    let trace_id: Option<String> = row.get("trace_id")?;
    let span_id: Option<String> = row.get("span_id")?;
    let idempotency_key: Option<String> = row.get("idempotency_key")?;
    let timestamp_utc: String = row.get("timestamp_utc")?;
    let priority: String = row.get("priority")?;
    let event_seq: Option<i64> = row.get("event_seq")?;
    let body: String = row.get("body")?;

    // We assemble a JSON Value of the raw envelope, then deserialise.
    // This keeps the "raw" representation honest and centralises parsing.
    let mut value = serde_json::Map::new();
    value.insert(
        "arcp".into(),
        serde_json::Value::String(arcp_core::PROTOCOL_VERSION.into()),
    );
    value.insert("id".into(), serde_json::Value::String(id));
    value.insert("timestamp".into(), serde_json::Value::String(timestamp_utc));
    value.insert("type".into(), serde_json::Value::String(type_name));
    let payload: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    value.insert("payload".into(), payload);
    insert_opt(&mut value, "session_id", session_id);
    insert_opt(&mut value, "job_id", job_id);
    insert_opt(&mut value, "stream_id", stream_id);
    insert_opt(&mut value, "subscription_id", subscription_id);
    insert_opt(&mut value, "correlation_id", correlation_id);
    insert_opt(&mut value, "causation_id", causation_id);
    insert_opt(&mut value, "trace_id", trace_id);
    insert_opt(&mut value, "span_id", span_id);
    insert_opt(&mut value, "idempotency_key", idempotency_key);
    if priority != "normal" {
        value.insert("priority".into(), serde_json::Value::String(priority));
    }
    if let Some(seq) = event_seq {
        value.insert(
            "event_seq".into(),
            serde_json::Value::Number(serde_json::Number::from(seq.max(0))),
        );
    }

    let envelope: RawEnvelope =
        serde_json::from_value(serde_json::Value::Object(value)).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(LoggedEvent { rowid, envelope })
}

fn insert_opt(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<String>,
) {
    if let Some(v) = value {
        map.insert(key.to_owned(), serde_json::Value::String(v));
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
    use arcp_core::envelope::Envelope;
    use arcp_core::ids::SessionId;
    use arcp_core::messages::{MessageType, PingPayload};

    fn ping_envelope(session: &SessionId) -> Envelope {
        let mut env = Envelope::new(MessageType::Ping(PingPayload::default()));
        env.session_id = Some(session.clone());
        env
    }

    #[tokio::test]
    async fn append_and_list_round_trip() {
        let log = EventLog::in_memory().await.expect("open");
        let session = SessionId::new();
        for _ in 0..3 {
            let env = ping_envelope(&session);
            assert_eq!(
                log.append(&env).await.expect("append"),
                AppendOutcome::Inserted
            );
        }
        let rows = log.list(session.as_str(), 0, 10).await.expect("list");
        assert_eq!(rows.len(), 3);
        for w in rows.windows(2) {
            assert!(w[0].rowid < w[1].rowid, "rows out of order");
        }
    }

    #[tokio::test]
    async fn append_dedups_on_id() {
        let log = EventLog::in_memory().await.expect("open");
        let session = SessionId::new();
        let env = ping_envelope(&session);
        assert_eq!(
            log.append(&env).await.expect("first"),
            AppendOutcome::Inserted
        );
        assert_eq!(
            log.append(&env).await.expect("second"),
            AppendOutcome::Duplicate
        );
        assert_eq!(log.count().await.expect("count"), 1);
    }

    #[tokio::test]
    async fn list_respects_after_rowid_and_session_filter() {
        let log = EventLog::in_memory().await.expect("open");
        let session_a = SessionId::new();
        let session_b = SessionId::new();
        for _ in 0..2 {
            log.append(&ping_envelope(&session_a)).await.expect("a");
            log.append(&ping_envelope(&session_b)).await.expect("b");
        }
        let only_a = log.list(session_a.as_str(), 0, 100).await.expect("a only");
        assert_eq!(only_a.len(), 2);
        let after_first = log
            .list(session_a.as_str(), only_a[0].rowid, 100)
            .await
            .expect("after first");
        assert_eq!(after_first.len(), 1);
        assert_eq!(after_first[0].rowid, only_a[1].rowid);
    }

    #[tokio::test]
    #[allow(clippy::cast_precision_loss)]
    async fn replay_job_events_filters_by_seq_and_orders() {
        use arcp_core::ids::JobId;
        use arcp_core::messages::{JobProgressPayload, JobState};

        let log = EventLog::in_memory().await.expect("open");
        let session = SessionId::new();
        let job = JobId::new();
        // Append three job-scoped progress events with ascending seq, plus
        // one event for a different job that must be excluded.
        for seq in 1u64..=3 {
            let mut env = Envelope::new(MessageType::JobProgress(JobProgressPayload::new(
                seq as f64,
            )));
            env.session_id = Some(session.clone());
            env.job_id = Some(job.clone());
            env.event_seq = Some(seq);
            log.append(&env).await.expect("append job event");
        }
        let mut other = Envelope::new(MessageType::JobHeartbeat(
            arcp_core::messages::JobHeartbeatPayload {
                sequence: 1,
                deadline_ms: None,
                state: JobState::Running,
            },
        ));
        other.session_id = Some(session.clone());
        other.job_id = Some(JobId::new());
        other.event_seq = Some(99);
        log.append(&other).await.expect("append other job");

        // Replay seq > 1 → expect events 2 and 3 in order, no other job.
        let replayed = log
            .replay_job_events_after_seq(job.as_str(), 1, 100)
            .await
            .expect("replay");
        let seqs: Vec<u64> = replayed
            .iter()
            .filter_map(|e| e.envelope.event_seq)
            .collect();
        assert_eq!(seqs, vec![2, 3]);
        for ev in &replayed {
            assert_eq!(ev.envelope.job_id.as_ref(), Some(&job));
        }
    }

    #[tokio::test]
    async fn get_by_id_returns_stored_envelope() {
        let log = EventLog::in_memory().await.expect("open");
        let session = SessionId::new();
        let env = ping_envelope(&session);
        let original_id = env.id.clone();
        log.append(&env).await.expect("append");
        let got = log.get_by_id(original_id.as_str()).await.expect("get");
        let logged = got.expect("found");
        assert_eq!(logged.envelope.id, original_id);
        assert_eq!(logged.envelope.type_name, "ping");
    }

    #[tokio::test]
    async fn get_by_id_returns_none_for_unknown() {
        let log = EventLog::in_memory().await.expect("open");
        let got = log.get_by_id("msg_nonexistent01ABC").await.expect("get");
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn open_creates_file_backed_log() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("log.sqlite");
        let log = EventLog::open(&path).await.expect("open");
        let session = SessionId::new();
        log.append(&ping_envelope(&session)).await.expect("append");
        // Re-open the file and verify the row survives.
        drop(log);
        let log2 = EventLog::open(&path).await.expect("re-open");
        assert_eq!(log2.count().await.expect("count"), 1);
    }
}
