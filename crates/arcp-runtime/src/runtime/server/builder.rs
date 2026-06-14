//! Runtime builder / configuration (split from server.rs, #74).

#[allow(clippy::wildcard_imports)]
use super::*;

/// Runtime configuration.
pub struct RuntimeBuilder {
    auth: AuthRegistry,
    tools: ToolRegistry,
    advertised_capabilities: Capabilities,
    runtime_identity: RuntimeIdentity,
    session_lease_seconds: Option<u64>,
    ack_window: Option<u64>,
    credential_provisioner: Option<Arc<dyn CredentialProvisioner>>,
    terminal_retention: Duration,
}

impl Default for RuntimeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for RuntimeBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeBuilder")
            .field("advertised_capabilities", &self.advertised_capabilities)
            .field("runtime_identity", &self.runtime_identity)
            .field("session_lease_seconds", &self.session_lease_seconds)
            .finish_non_exhaustive()
    }
}

impl RuntimeBuilder {
    /// New builder with empty auth registry, default capabilities, and the
    /// crate's identity (`arcp-rs`).
    #[must_use]
    pub fn new() -> Self {
        Self {
            auth: AuthRegistry::new(),
            tools: ToolRegistry::new(),
            advertised_capabilities: Capabilities::default(),
            runtime_identity: RuntimeIdentity {
                kind: IMPL_KIND.to_owned(),
                version: IMPL_VERSION.to_owned(),
                fingerprint: None,
                trust_level: Some("trusted".into()),
            },
            session_lease_seconds: Some(3600),
            ack_window: None,
            credential_provisioner: None,
            // Default terminal-job retention window (#72). Terminal jobs
            // remain visible to `session.list_jobs` / `job.subscribe` for
            // this long after completion, then are swept to bound memory.
            terminal_retention: Duration::from_secs(300),
        }
    }

    /// Register one authenticator. Multiple may be added (one per scheme).
    #[must_use]
    pub fn with_authenticator(mut self, auth: Box<dyn Authenticator>) -> Self {
        self.auth.register(auth);
        self
    }

    /// Set the tool registry (replaces any previously set).
    #[must_use]
    pub fn with_tools(mut self, tools: ToolRegistry) -> Self {
        self.tools = tools;
        self
    }

    /// Set the capability set the runtime advertises.
    #[must_use]
    pub fn with_capabilities(mut self, caps: Capabilities) -> Self {
        self.advertised_capabilities = caps;
        self
    }

    /// Override the runtime identity.
    #[must_use]
    pub fn with_identity(mut self, ident: RuntimeIdentity) -> Self {
        self.runtime_identity = ident;
        self
    }

    /// Override the default session lease length.
    #[must_use]
    pub const fn with_session_lease_seconds(mut self, seconds: u64) -> Self {
        self.session_lease_seconds = Some(seconds);
        self
    }

    /// Set the size of the `session.ack` sliding window (ARCP v1.1 §6.5).
    ///
    /// When set, the writer will pause outbound countable envelopes once
    /// `emitted - last_processed_seq >= window` and resume on the next
    /// `session.ack`. Set to `None` (default) to disable window-based
    /// flow control entirely.
    ///
    /// A window of `0` makes the gate immediately unsatisfiable for the
    /// very first countable event and is normalized to `None`
    /// (disabled) rather than installing a guaranteed deadlock.
    #[must_use]
    pub const fn with_ack_window(mut self, window: u64) -> Self {
        self.ack_window = if window == 0 { None } else { Some(window) };
        self
    }

    /// Set how long terminal jobs (and their idempotency records) are
    /// retained before the maintenance sweep evicts them (#72, #85).
    ///
    /// Terminal jobs stay visible to `session.list_jobs` and
    /// `job.subscribe` for this window; afterward they are dropped from the
    /// [`JobRegistry`] and the idempotency index so a long-running runtime
    /// does not accumulate state without bound. `Duration::ZERO` evicts
    /// terminal jobs on the next sweep. Defaults to 300 seconds.
    #[must_use]
    pub const fn with_terminal_retention(mut self, retention: Duration) -> Self {
        self.terminal_retention = retention;
        self
    }

    /// Register a provisioner for ARCP v1.1 lease-bound credentials.
    #[must_use]
    pub fn with_credential_provisioner(
        mut self,
        provisioner: Arc<dyn CredentialProvisioner>,
    ) -> Self {
        self.credential_provisioner = Some(provisioner);
        self.advertised_capabilities.model_use = Some(true);
        self.advertised_capabilities.provisioned_credentials = Some(true);
        self
    }

    /// Construct an [`ARCPRuntime`] sharing this configuration. The
    /// returned runtime is cheap to clone.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Storage`] if the in-memory event log cannot be
    /// initialised (extremely unlikely; signals `SQLite` link failure).
    pub async fn build(self) -> Result<ARCPRuntime, ARCPError> {
        if self.advertised_capabilities.provisioned_credentials == Some(true)
            && self.credential_provisioner.is_none()
        {
            return Err(ARCPError::FailedPrecondition {
                detail: "provisioned_credentials advertised without a CredentialProvisioner".into(),
            });
        }
        let event_log = EventLog::in_memory().await?;
        let inner = Arc::new(RuntimeInner {
            auth: self.auth,
            tools: self.tools,
            advertised_capabilities: self.advertised_capabilities,
            runtime_identity: self.runtime_identity,
            session_lease_seconds: self.session_lease_seconds,
            ack_window: self.ack_window,
            extension_registry: ExtensionRegistry::new(),
            event_log,
            artifacts: ArtifactStore::new(),
            subscriptions: SubscriptionManager::new(),
            jobs: JobRegistry::new(),
            session_principals: Arc::new(DashMap::new()),
            credential_provisioner: self.credential_provisioner,
            credential_ledger: CredentialLedger::new(),
            idempotency_index: DashMap::new(),
            terminal_retention: self.terminal_retention,
            resume_registry: Arc::new(DashMap::new()),
        });
        // Background maintenance task (#72, #85): periodically sweep
        // terminal jobs past the retention window and evict their
        // idempotency records. Holds a Weak ref so it exits once the last
        // ARCPRuntime handle is dropped. The cadence is a fixed interval
        // (independent of the retention window) so retention only controls
        // *eligibility*, not how often the sweep runs.
        let weak = Arc::downgrade(&inner);
        let retention = self.terminal_retention;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(MAINTENANCE_INTERVAL);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            // The first tick fires immediately; skip the sweep work it
            // would otherwise do on an empty runtime.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                let Some(inner) = weak.upgrade() else { break };
                sweep_terminal_state(&inner, retention);
            }
        });
        Ok(ARCPRuntime { inner })
    }
}
