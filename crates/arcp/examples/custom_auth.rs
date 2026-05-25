//! Custom bearer-token authenticator (RFC §6.1).
//!
//! Demonstrates plugging a custom `BearerAuthenticator` into the runtime.
//! The authenticator here validates a stateless signed token of the form:
//!
//!     `<principal>.<exp_epoch>.<hmac-sha256-hex>`
//!
//! where `hmac = HMAC-SHA256(secret, "<principal>.<exp_epoch>")`.
//!
//! Two scenarios run back-to-back:
//!   1. A valid token → `session.accepted`; one echo job completes.
//!   2. An invalid token → `session.rejected` (UNAUTHENTICATED).
//!
//! Run with:
//!     `cargo run --example custom_auth`

#![allow(
    clippy::todo,
    clippy::unimplemented,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::doc_markdown,
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    clippy::unused_async,
    clippy::diverging_sub_expression,
    clippy::no_effect_underscore_binding,
    clippy::let_unit_value,
    clippy::used_underscore_binding,
    clippy::let_underscore_untyped,
    clippy::struct_field_names,
    clippy::manual_let_else,
    clippy::map_unwrap_or,
    clippy::redundant_pub_crate,
    dead_code,
    unreachable_code,
    unused_assignments,
    unused_mut,
    unused_imports,
    unused_variables
)]

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use arcp::auth::{AuthOutcome, Authenticator};
use arcp::error::ARCPError;
use arcp::messages::{AuthScheme, Capabilities, ClientIdentity, Credentials};
use arcp::transport::MemoryTransport;
use arcp::{ARCPClient, Envelope};
use serde_json::json;

/// HMAC-SHA256 token of the form `principal.exp.sig`.
///
/// Replace this with a real JWKS verifier, an external auth service,
/// or any other verification logic your deployment requires.
struct HmacTokenAuthenticator {
    secret: String,
}

impl HmacTokenAuthenticator {
    fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
        }
    }

    /// Verify the token and return the resolved principal, or an error.
    fn verify(&self, token: &str) -> Result<String, ARCPError> {
        // 1. Split on '.' — expect exactly three parts.
        // 2. Re-derive HMAC-SHA256(secret, "principal.exp") and compare
        //    with constant-time equality.
        // 3. Check exp > now.
        // 4. Return Ok(principal).
        //
        // Pseudocode (real implementation uses hmac crate):
        //   let parts = token.splitn(3, '.').collect::<Vec<_>>();
        //   let [principal, exp, sig] = parts[..] else { ... };
        //   let expected = hmac_sha256(&self.secret, &format!("{principal}.{exp}"));
        //   constant_time_compare(sig_bytes, expected_bytes)?;
        //   if exp.parse::<u64>() < now_epoch() { return Err(Unauthenticated) }
        //   Ok(principal.to_string())
        todo!()
    }
}

#[async_trait]
impl Authenticator for HmacTokenAuthenticator {
    fn scheme(&self) -> AuthScheme {
        AuthScheme::Bearer
    }

    async fn authenticate(
        &self,
        _creds: &Credentials,
        _client: &ClientIdentity,
        _negotiated: &Capabilities,
    ) -> Result<AuthOutcome, ARCPError> {
        // Extract the bearer token from _creds, verify it, and return the outcome.
        //
        // let token = _creds.bearer_token().ok_or_else(|| ARCPError::Unauthenticated {
        //     detail: "missing bearer token".into(),
        // })?;
        // let principal = self.verify(token)?;
        // Ok(AuthOutcome::Accept { principal })
        todo!()
    }
}

/// Mint a token valid for `ttl_secs` seconds.
fn mint_token(principal: &str, ttl_secs: u64, secret: &str) -> String {
    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        + ttl_secs;
    let body = format!("{principal}.{exp}");
    // sig = hex(hmac_sha256(secret, body))  — todo: use `hmac` crate
    let sig: String = todo!(); // placeholder
    format!("{body}.{sig}")
}

type Client = ARCPClient<MemoryTransport>;

async fn run_echo_job(_client: &Client) -> Result<serde_json::Value, ARCPError> {
    // client.submit(tool="echo", arguments={"hello": "world"})
    // -> await terminal job.completed / job.failed
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let secret = "demo-secret";
    let authenticator = Arc::new(HmacTokenAuthenticator::new(secret));

    // --- Scenario 1: valid token ------------------------------------------------
    let good_token = mint_token("alice", 60, secret);
    println!(
        "minted token (truncated): {}...",
        &good_token[..24.min(good_token.len())]
    );

    // Build a runtime with the custom authenticator, then connect a client
    // presenting the good token as a Bearer credential.
    let good_client: Client = todo!(); // transport, identity, bearer=good_token
    let result = run_echo_job(&good_client).await?;
    println!("echo result: {result}");

    // --- Scenario 2: invalid signature -----------------------------------------
    let bad_token = "alice.0.deadbeef";
    let bad_client: Client = todo!(); // transport, identity, bearer=bad_token

    // The runtime should reject with UNAUTHENTICATED → session.rejected.
    match run_echo_job(&bad_client).await {
        Err(ARCPError::Unauthenticated { .. }) => {
            println!("bad token rejected as expected — done");
        }
        other => return Err(format!("unexpected outcome: {other:?}").into()),
    }
    Ok(())
}
