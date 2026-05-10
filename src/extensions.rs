//! Extension namespace registry and unknown-message classification (RFC §21).
//!
//! Two responsibilities live here:
//!
//! 1. **Namespace validation** via [`is_extension_name`]. Extension names
//!    follow one of two forms: `arcpx.<vendor-or-domain>.<name>.v<n>`
//!    (recommended) or a reverse-DNS prefix such as `com.acme.workflow.v2`.
//! 2. **Type classification.** Given a wire-level `type` string, what should
//!    a receiver do? [`classify_type`] returns a [`TypeClassification`] that
//!    drives the dispatch decision per §21.3.
//!
//! The bare `x-` prefix is reserved for transport-internal experimental
//! fields and MUST NOT appear on long-lived deployments. We accept it for
//! parsing but classify it as [`TypeClassification::ReservedExperimental`].

use std::collections::BTreeSet;
use std::sync::OnceLock;

/// What a receiver should do with a wire-level message `type` string.
///
/// This is used by the transport / dispatch layers to decide between
/// "process normally", "respond with `nack` `UNIMPLEMENTED`", and
/// "silently drop", per RFC §21.3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeClassification {
    /// A core protocol type that the receiver MUST implement. If unknown to
    /// this build, respond with `nack` `UNIMPLEMENTED`.
    Core,
    /// An advertised, registered extension type. Dispatch to its handler.
    KnownExtension,
    /// A namespaced extension that is NOT advertised in the negotiated
    /// `capabilities.extensions` set. Per §21.3, respond with `nack`
    /// `UNIMPLEMENTED` (or silently drop if the sender marked it
    /// `extensions.optional: true`).
    UnknownExtension,
    /// Transport-internal experimental field (`x-...`). MUST NOT be relied
    /// on in long-lived deployments; receivers MAY drop silently.
    ReservedExperimental,
    /// Type string is malformed (empty or fails namespace rules).
    Malformed,
}

/// Per-session/runtime registry of advertised extension names (§7, §21.2).
///
/// Construct one per session; the runtime's session bookkeeping owns it.
#[derive(Debug, Clone, Default)]
pub struct ExtensionRegistry {
    advertised: BTreeSet<String>,
}

impl ExtensionRegistry {
    /// Construct an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a registry pre-populated with `names`.
    ///
    /// # Errors
    ///
    /// Returns the first malformed name, if any.
    pub fn from_names<I, S>(names: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut reg = Self::new();
        for name in names {
            reg.register(name.into())?;
        }
        Ok(reg)
    }

    /// Register an extension name.
    ///
    /// # Errors
    ///
    /// Returns the offending name if it does not match an extension naming
    /// rule (§21.1).
    pub fn register(&mut self, name: String) -> Result<(), String> {
        if !is_extension_name(&name) {
            return Err(name);
        }
        self.advertised.insert(name);
        Ok(())
    }

    /// True when `name` was previously registered.
    #[must_use]
    pub fn is_advertised(&self, name: &str) -> bool {
        self.advertised.contains(name)
    }

    /// Number of advertised extensions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.advertised.len()
    }

    /// True when the registry has no advertised extensions.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.advertised.is_empty()
    }

    /// Iterate the advertised names in sorted order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.advertised.iter().map(String::as_str)
    }

    /// Classify a wire-level `type` string per §21.3.
    #[must_use]
    pub fn classify(&self, type_name: &str) -> TypeClassification {
        if type_name.is_empty() {
            return TypeClassification::Malformed;
        }
        if type_name.starts_with("x-") {
            return TypeClassification::ReservedExperimental;
        }
        if is_core_type(type_name) {
            return TypeClassification::Core;
        }
        if is_extension_name(type_name) {
            if self.is_advertised(type_name) {
                TypeClassification::KnownExtension
            } else {
                TypeClassification::UnknownExtension
            }
        } else {
            TypeClassification::Malformed
        }
    }
}

/// Classify a wire-level `type` string against the empty registry.
///
/// Convenience for transports that do not yet have a session-scoped
/// registry (e.g. pre-handshake messages).
#[must_use]
pub fn classify_type(type_name: &str) -> TypeClassification {
    ExtensionRegistry::new().classify(type_name)
}

/// True if `name` follows the §21.1 extension naming rules.
///
/// Two recognised forms:
///
/// - `arcpx.<vendor-or-domain>[.<name>].v<n>` — `arcpx.` prefix, at least
///   one segment after the prefix, terminating in a `vN` version segment.
///   The RFC's own capabilities example uses `arcpx.example.v1`, so the
///   intermediate `<name>` segment is optional.
/// - `<reverse-dns>.<...>.v<n>` — at least three dot-separated segments
///   total (TLD + at least one body segment + `vN` version).
#[must_use]
pub fn is_extension_name(name: &str) -> bool {
    name.strip_prefix("arcpx.").map_or_else(
        || looks_like_reverse_dns(name) && is_dotted_versioned(name, 3),
        |rest| is_dotted_versioned(rest, 2),
    )
}

/// True if `type_name` is one of the core protocol types from RFC §6.2.
#[must_use]
pub fn is_core_type(type_name: &str) -> bool {
    core_type_set().contains(type_name)
}

/// All core wire-level type strings from RFC §6.2.
fn core_type_set() -> &'static BTreeSet<&'static str> {
    static SET: OnceLock<BTreeSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        [
            // identity & authentication
            "session.open",
            "session.challenge",
            "session.authenticate",
            "session.accepted",
            "session.unauthenticated",
            "session.rejected",
            "session.refresh",
            "session.evicted",
            "session.close",
            // control
            "ping",
            "pong",
            "ack",
            "nack",
            "cancel",
            "cancel.accepted",
            "cancel.refused",
            "interrupt",
            "resume",
            "backpressure",
            "checkpoint.create",
            "checkpoint.restore",
            // execution
            "tool.invoke",
            "tool.result",
            "tool.error",
            "job.accepted",
            "job.started",
            "job.progress",
            "job.heartbeat",
            "job.checkpoint",
            "job.completed",
            "job.failed",
            "job.cancelled",
            "job.schedule",
            "workflow.start",
            "workflow.complete",
            "agent.delegate",
            "agent.handoff",
            // streaming
            "stream.open",
            "stream.chunk",
            "stream.close",
            "stream.error",
            // human-in-the-loop
            "human.input.request",
            "human.input.response",
            "human.choice.request",
            "human.choice.response",
            "human.input.cancelled",
            // permissions & leases
            "permission.request",
            "permission.grant",
            "permission.deny",
            "lease.granted",
            "lease.extended",
            "lease.revoked",
            "lease.refresh",
            // subscriptions
            "subscribe",
            "subscribe.accepted",
            "subscribe.event",
            "unsubscribe",
            "subscribe.closed",
            // artifacts
            "artifact.put",
            "artifact.fetch",
            "artifact.ref",
            "artifact.release",
            // events & telemetry
            "event.emit",
            "log",
            "metric",
            "trace.span",
        ]
        .into_iter()
        .collect()
    })
}

/// True if `s` has at least `min_segments` dot-separated alphanumeric
/// segments where the last segment is a `vN` version.
fn is_dotted_versioned(s: &str, min_segments: usize) -> bool {
    let segments: Vec<&str> = s.split('.').collect();
    if segments.len() < min_segments {
        return false;
    }
    if !segments.iter().all(|seg| {
        !seg.is_empty()
            && seg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    }) {
        return false;
    }
    let last = segments[segments.len() - 1];
    let Some(version_body) = last.strip_prefix('v') else {
        return false;
    };
    !version_body.is_empty() && version_body.chars().all(|c| c.is_ascii_digit())
}

/// True if `name` looks like a reverse-DNS prefix (e.g. `com.acme.workflow.v2`).
///
/// Conservative heuristic: must contain at least three dot-separated
/// segments, must not start with a digit, and the first segment must look
/// like a TLD (lowercase ASCII letters, length 2..=8). This is intentionally
/// strict to avoid swallowing typos as valid extensions.
fn looks_like_reverse_dns(name: &str) -> bool {
    let mut parts = name.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    if first.is_empty()
        || first.len() < 2
        || first.len() > 8
        || !first.chars().all(|c| c.is_ascii_lowercase())
    {
        return false;
    }
    // We don't require the second segment here; the dotted-versioned check
    // does the structural validation. We're only ruling out things like
    // "ping" (no dot) or "1.2" (digit prefix).
    true
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

    #[test]
    fn arcpx_namespace_is_valid_extension() {
        assert!(is_extension_name("arcpx.example.v1"));
        assert!(is_extension_name("arcpx.acme-corp.workflow.v2"));
    }

    #[test]
    fn reverse_dns_namespace_is_valid_extension() {
        assert!(is_extension_name("com.acme.workflow.v2"));
        assert!(is_extension_name("io.example.metric.v1"));
    }

    #[test]
    fn invalid_namespaces_are_rejected() {
        assert!(!is_extension_name(""));
        assert!(!is_extension_name("ping")); // no dot
        assert!(!is_extension_name("arcpx.foo")); // missing version
        assert!(!is_extension_name("arcpx.foo.v")); // empty version body
        assert!(!is_extension_name("arcpx.foo.bar.v1.x")); // version not last
        assert!(!is_extension_name("1.2.3")); // digit-leading TLD
        assert!(!is_extension_name("com.acme.workflow")); // missing version
        assert!(!is_extension_name("com.acme.workflow.v")); // empty version
        assert!(!is_extension_name("com.acme.workflow.vfoo")); // non-numeric version
    }

    #[test]
    fn classify_recognises_core_types() {
        assert_eq!(classify_type("session.open"), TypeClassification::Core);
        assert_eq!(classify_type("job.progress"), TypeClassification::Core);
        assert_eq!(classify_type("metric"), TypeClassification::Core);
    }

    #[test]
    fn classify_advertised_extension() {
        let mut reg = ExtensionRegistry::new();
        reg.register("arcpx.example.v1".into()).expect("valid");
        assert_eq!(
            reg.classify("arcpx.example.v1"),
            TypeClassification::KnownExtension,
        );
    }

    #[test]
    fn classify_unadvertised_extension() {
        let reg = ExtensionRegistry::new();
        assert_eq!(
            reg.classify("arcpx.example.v1"),
            TypeClassification::UnknownExtension,
        );
    }

    #[test]
    fn classify_experimental_prefix() {
        assert_eq!(
            classify_type("x-flaky-thing"),
            TypeClassification::ReservedExperimental,
        );
    }

    #[test]
    fn classify_malformed_input() {
        assert_eq!(classify_type(""), TypeClassification::Malformed);
        assert_eq!(
            classify_type("not.a.real.type"),
            TypeClassification::Malformed
        );
    }

    #[test]
    fn registry_rejects_malformed_extension() {
        let mut reg = ExtensionRegistry::new();
        let err = reg
            .register("not-an-extension".into())
            .expect_err("must reject");
        assert_eq!(err, "not-an-extension");
        assert!(reg.is_empty());
    }

    #[test]
    fn registry_from_names_propagates_error() {
        let result = ExtensionRegistry::from_names(["arcpx.foo.v1", "broken"]);
        assert_eq!(result.unwrap_err(), "broken");
    }
}
