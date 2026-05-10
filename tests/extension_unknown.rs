//! Integration tests for RFC §21 extension handling.
//!
//! Phase 1 covers the namespace-validation portion: the extension registry
//! accepts well-formed names from both `arcpx.*` and reverse-DNS forms,
//! rejects malformed ones, and classifies wire-level type strings into the
//! categories that drive the §21.3 dispatch decision.
//!
//! The dispatch portion (silent-drop for `extensions.optional = true`,
//! nack `UNIMPLEMENTED` otherwise) lands with the runtime in Phase 5.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]

use arcp::extensions::{classify_type, is_extension_name, ExtensionRegistry, TypeClassification};

#[test]
fn arcpx_two_segment_name_is_valid() {
    // RFC §7's own capabilities example uses `arcpx.example.v1` — vendor
    // segment + version segment, with the optional `<name>` middle segment
    // omitted. Must be accepted.
    assert!(is_extension_name("arcpx.example.v1"));
}

#[test]
fn arcpx_three_segment_name_is_valid() {
    assert!(is_extension_name("arcpx.acme.workflow.v3"));
}

#[test]
fn reverse_dns_name_is_valid() {
    assert!(is_extension_name("com.acme.workflow.v2"));
}

#[test]
fn names_without_version_are_rejected() {
    assert!(!is_extension_name("arcpx.example"));
    assert!(!is_extension_name("com.acme.workflow"));
}

#[test]
fn experimental_x_prefix_is_classified_separately() {
    // x- prefix is reserved for transport-internal experimental fields per
    // §21.1 — receivers MAY drop these silently and MUST NOT treat them as
    // valid long-lived extensions.
    assert_eq!(
        classify_type("x-debug-flag"),
        TypeClassification::ReservedExperimental,
    );
    assert!(!is_extension_name("x-debug-flag"));
}

#[test]
fn registry_advertises_then_classifies_known() {
    let reg = ExtensionRegistry::from_names(["arcpx.example.v1", "com.acme.workflow.v2"])
        .expect("valid namespaces");
    assert_eq!(reg.len(), 2);
    assert_eq!(
        reg.classify("arcpx.example.v1"),
        TypeClassification::KnownExtension,
    );
    assert_eq!(
        reg.classify("com.acme.workflow.v2"),
        TypeClassification::KnownExtension,
    );
}

#[test]
fn registry_classifies_unadvertised_extension() {
    let reg = ExtensionRegistry::new();
    assert_eq!(
        reg.classify("arcpx.example.v1"),
        TypeClassification::UnknownExtension,
    );
}

#[test]
fn registry_classifies_core_types_independent_of_advertised_set() {
    let reg = ExtensionRegistry::new();
    for core in [
        "session.open",
        "tool.invoke",
        "job.progress",
        "stream.chunk",
        "human.input.request",
        "permission.request",
        "lease.granted",
        "subscribe",
        "artifact.put",
        "metric",
    ] {
        assert_eq!(reg.classify(core), TypeClassification::Core, "{core}");
    }
}

#[test]
fn registry_rejects_register_of_malformed_name() {
    let mut reg = ExtensionRegistry::new();
    let err = reg
        .register("not-an-extension".into())
        .expect_err("must fail");
    assert_eq!(err, "not-an-extension");
    assert!(reg.is_empty());
}

#[test]
fn registry_classifies_malformed_input() {
    let reg = ExtensionRegistry::new();
    assert_eq!(reg.classify(""), TypeClassification::Malformed);
    assert_eq!(reg.classify("just-a-string"), TypeClassification::Malformed,);
}
