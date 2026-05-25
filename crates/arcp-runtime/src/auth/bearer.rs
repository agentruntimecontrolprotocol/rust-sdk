//! `bearer` authentication scheme (ARCP v1.1 §6.1).
//!
//! Bearer is the only authentication scheme normative in v1.1. The runtime
//! validates the opaque token carried in `session.open.payload.auth.token`
//! against its configured trust store and resolves it to a principal.

use std::collections::HashMap;

use async_trait::async_trait;

use arcp_core::auth::{AuthOutcome, Authenticator};
use arcp_core::error::ARCPError;
use arcp_core::messages::{AuthScheme, Capabilities, ClientIdentity, Credentials};

/// Trivial in-memory bearer-token store mapping `token -> principal`.
///
/// Suitable for tests, examples, and small deployments. Real deployments
/// should plug their own [`Authenticator`] in front of an external trust
/// store.
#[derive(Debug, Default)]
pub struct BearerAuthenticator {
    tokens: HashMap<String, String>,
}

impl BearerAuthenticator {
    /// Construct an empty authenticator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a `token -> principal` mapping. Returns `self` for chaining.
    #[must_use]
    pub fn with_token(mut self, token: impl Into<String>, principal: impl Into<String>) -> Self {
        self.tokens.insert(token.into(), principal.into());
        self
    }
}

#[async_trait]
impl Authenticator for BearerAuthenticator {
    fn scheme(&self) -> AuthScheme {
        AuthScheme::Bearer
    }

    async fn authenticate(
        &self,
        creds: &Credentials,
        _client: &ClientIdentity,
        _negotiated: &Capabilities,
    ) -> Result<AuthOutcome, ARCPError> {
        let Some(token) = &creds.token else {
            return Ok(AuthOutcome::Reject {
                reason: "bearer scheme requires a token".into(),
            });
        };
        Ok(self.tokens.get(token).map_or_else(
            || AuthOutcome::Reject {
                reason: "unknown bearer token".into(),
            },
            |principal| AuthOutcome::Accept {
                principal: principal.clone(),
            },
        ))
    }
}
