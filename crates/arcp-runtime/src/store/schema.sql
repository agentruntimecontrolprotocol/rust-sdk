-- ARCP event log schema. Backs transport-level dedup, ARCP v1.1 §7.6
-- subscription backfill, and ARCP v1.1 §6.3 resume.
--
-- One row per envelope. (session_id, id) is the dedup key for transport
-- idempotency; rowid (auto-incrementing) gives total replay order. The
-- secondary indexes accelerate subscription backfill filters and
-- correlation lookups.

CREATE TABLE IF NOT EXISTS events (
    rowid           INTEGER PRIMARY KEY AUTOINCREMENT,
    id              TEXT    NOT NULL,
    session_id      TEXT,
    job_id          TEXT,
    stream_id       TEXT,
    subscription_id TEXT,
    type_name       TEXT    NOT NULL,
    correlation_id  TEXT,
    causation_id    TEXT,
    trace_id        TEXT,
    span_id         TEXT,
    idempotency_key TEXT,
    timestamp_utc   TEXT    NOT NULL,
    priority        TEXT    NOT NULL DEFAULT 'normal',
    event_seq       INTEGER,
    body            TEXT    NOT NULL,
    UNIQUE (session_id, id)
);

CREATE INDEX IF NOT EXISTS events_session_idx       ON events (session_id);
CREATE INDEX IF NOT EXISTS events_job_idx           ON events (job_id);
-- Accelerates §7.6 job.subscribe history replay (seq > from_event_seq).
CREATE INDEX IF NOT EXISTS events_job_seq_idx        ON events (job_id, event_seq);
CREATE INDEX IF NOT EXISTS events_stream_idx        ON events (stream_id);
CREATE INDEX IF NOT EXISTS events_subscription_idx  ON events (subscription_id);
CREATE INDEX IF NOT EXISTS events_type_idx          ON events (type_name);
CREATE INDEX IF NOT EXISTS events_correlation_idx   ON events (correlation_id);
CREATE INDEX IF NOT EXISTS events_causation_idx     ON events (causation_id);
CREATE INDEX IF NOT EXISTS events_trace_idx         ON events (trace_id);
CREATE INDEX IF NOT EXISTS events_timestamp_idx     ON events (timestamp_utc);

-- Idempotency lookup for §6.4 logical command intent.
-- (session_principal, idempotency_key) -> message id
CREATE TABLE IF NOT EXISTS idempotency (
    session_principal TEXT NOT NULL,
    idempotency_key   TEXT NOT NULL,
    message_id        TEXT NOT NULL,
    created_utc       TEXT NOT NULL,
    PRIMARY KEY (session_principal, idempotency_key)
);

-- Outstanding provisioned credentials for revocation recovery (ARCP v1.1 §9.8).
CREATE TABLE IF NOT EXISTS outstanding_credentials (
    credential_id TEXT PRIMARY KEY,
    job_id        TEXT    NOT NULL,
    issued_at     TEXT    NOT NULL,
    revoked_at    TEXT
);

CREATE INDEX IF NOT EXISTS outstanding_credentials_job_idx
    ON outstanding_credentials (job_id);
