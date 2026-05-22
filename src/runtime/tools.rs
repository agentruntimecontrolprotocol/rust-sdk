//! Tool registry and handler trait.
//!
//! User code registers a [`ToolHandler`] for each tool the runtime should
//! be able to execute. The runtime dispatches `tool.invoke` envelopes by
//! looking up the handler in the [`ToolRegistry`] and driving it inside a
//! per-job tokio task with a [`tokio_util::sync::CancellationToken`].

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use super::context::ToolContext;
use crate::error::ARCPError;

/// Application-supplied tool handler.
///
/// Implementations should poll `cancel` at safe checkpoints to honour
/// cooperative cancellation (RFC §10.4).
#[async_trait]
pub trait ToolHandler: Send + Sync {
    /// Tool identifier (matches `tool.invoke.payload.tool`).
    fn name(&self) -> &str;

    /// Run the tool. Return either an inline JSON result or an error.
    ///
    /// `arguments` is the raw `arguments` block from the envelope.
    /// `ctx` is the per-job [`ToolContext`] — the handler polls
    /// `ctx.cancel` for cooperative cancellation.
    ///
    /// # Errors
    ///
    /// Implementations return [`ARCPError`] for any failure path. The
    /// runtime maps the error to a `job.failed` (or `job.cancelled`)
    /// envelope on the wire.
    async fn invoke(
        &self,
        arguments: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<serde_json::Value, ARCPError>;
}

/// Runtime-owned registry of tools.
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: Arc<HashMap<String, Arc<dyn ToolHandler>>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("names", &self.tools.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ToolRegistry {
    /// Construct an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a tool by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Arc<dyn ToolHandler>> {
        self.tools.get(name).cloned()
    }

    /// Number of registered tools.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// True if no tools are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

/// Builder for [`ToolRegistry`] — accumulate handlers, then `build`.
#[derive(Default)]
pub struct ToolRegistryBuilder {
    tools: HashMap<String, Arc<dyn ToolHandler>>,
}

impl std::fmt::Debug for ToolRegistryBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistryBuilder")
            .field("names", &self.tools.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ToolRegistryBuilder {
    /// Construct an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `handler` under its declared `name()`.
    #[must_use]
    pub fn with(mut self, handler: Arc<dyn ToolHandler>) -> Self {
        let name = handler.name().to_owned();
        self.tools.insert(name, handler);
        self
    }

    /// Finalise the registry.
    #[must_use]
    pub fn build(self) -> ToolRegistry {
        ToolRegistry {
            tools: Arc::new(self.tools),
        }
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
    use tokio_util::sync::CancellationToken;

    use super::*;

    struct EchoTool;

    #[async_trait]
    impl ToolHandler for EchoTool {
        fn name(&self) -> &'static str {
            "echo"
        }

        async fn invoke(
            &self,
            arguments: serde_json::Value,
            _ctx: ToolContext,
        ) -> Result<serde_json::Value, ARCPError> {
            Ok(arguments)
        }
    }

    #[tokio::test]
    async fn registry_round_trips_through_builder() {
        let reg = ToolRegistryBuilder::new().with(Arc::new(EchoTool)).build();
        assert!(!reg.is_empty());
        assert_eq!(reg.len(), 1);
        let echo = reg.get("echo").expect("registered");
        assert_eq!(echo.name(), "echo");

        // Invoking the handler through the trait obj exercises the dyn dispatch.
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let ctx = ToolContext {
            cancel: CancellationToken::new(),
            job_id: crate::ids::JobId::new(),
            session_id: crate::ids::SessionId::new(),
            correlation_id: crate::ids::MessageId::new(),
            out: tx,
            budget: crate::runtime::context::BudgetTracker::new(),
            lease: None,
        };
        let result = echo
            .invoke(serde_json::json!({"k": 1}), ctx)
            .await
            .expect("invoke");
        assert_eq!(result, serde_json::json!({"k": 1}));
    }

    #[test]
    fn empty_registry_reports_empty() {
        let reg = ToolRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
        assert!(reg.get("missing").is_none());
    }

    #[test]
    fn debug_impls_render_without_panicking() {
        let reg = ToolRegistryBuilder::new().with(Arc::new(EchoTool)).build();
        let s = format!("{reg:?}");
        assert!(s.contains("echo"));
        let builder = ToolRegistryBuilder::new().with(Arc::new(EchoTool));
        let bs = format!("{builder:?}");
        assert!(bs.contains("echo"));
    }
}
