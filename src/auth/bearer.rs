//! `bearer` authentication scheme (RFC §8.2).

use std::collections::HashMap;

use async_trait::async_trait;

use super::{AuthOutcome, Authenticator};
use crate::error::ARCPError;
use crate::messages::{AuthScheme, Capabilities, ClientIdentity, Credentials};

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
