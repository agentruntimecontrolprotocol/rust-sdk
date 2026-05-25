//! Authentication scheme adapters (RFC §8.2).
//!
//! Each [`Authenticator`] implementation validates one auth scheme. The
//! runtime composes these into an [`AuthRegistry`]; on `session.open` the
//! runtime dispatches by [`AuthScheme`] and either accepts directly or
//! issues a [`session.challenge`][crate::messages::SessionChallengePayload].

use std::collections::HashMap;

use async_trait::async_trait;

use crate::error::ARCPError;
use crate::messages::{AuthScheme, Capabilities, ClientIdentity, Credentials};

/// Outcome of an authentication attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthOutcome {
    /// Credentials suffice; the session can be accepted directly.
    Accept {
        /// Principal identifier extracted from the credentials.
        principal: String,
    },
    /// Runtime needs to issue a challenge; client must respond with
    /// `session.authenticate`.
    Challenge {
        /// Challenge nonce or instructions.
        challenge: String,
    },
    /// Credentials rejected.
    Reject {
        /// Human-readable reason.
        reason: String,
    },
}

/// Adapter trait for one auth scheme.
#[async_trait]
pub trait Authenticator: Send + Sync {
    /// Scheme this authenticator handles.
    fn scheme(&self) -> AuthScheme;

    /// Validate `creds` against the runtime trust store.
    ///
    /// `client` is the attestation block from `session.open`; `negotiated`
    /// is the capability set the runtime is willing to honour. The
    /// `none` scheme uses `negotiated` to gate on `anonymous: true`.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError`] for unrecoverable internal failures (e.g.
    /// trust store unreachable). Credential rejection is reported through
    /// [`AuthOutcome::Reject`], not via `Err`.
    async fn authenticate(
        &self,
        creds: &Credentials,
        client: &ClientIdentity,
        negotiated: &Capabilities,
    ) -> Result<AuthOutcome, ARCPError>;

    /// Verify the response to a previously issued challenge. Default
    /// implementation rejects everything (single-shot schemes don't need
    /// to override).
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError`] for unrecoverable internal failures.
    async fn verify_challenge_response(
        &self,
        _challenge: &str,
        _response: &str,
    ) -> Result<AuthOutcome, ARCPError> {
        Ok(AuthOutcome::Reject {
            reason: "this scheme does not use challenges".into(),
        })
    }
}

/// Runtime-owned set of authenticators, keyed by scheme.
pub struct AuthRegistry {
    by_scheme: HashMap<AuthSchemeKey, Box<dyn Authenticator>>,
}

impl Default for AuthRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for AuthRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthRegistry")
            .field("schemes", &self.by_scheme.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl AuthRegistry {
    /// Construct an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_scheme: HashMap::new(),
        }
    }

    /// Register `auth` for the scheme it advertises.
    pub fn register(&mut self, auth: Box<dyn Authenticator>) {
        self.by_scheme.insert(auth.scheme().into(), auth);
    }

    /// Look up the authenticator for `scheme`, or `None` if unsupported.
    #[must_use]
    pub fn get(&self, scheme: &AuthScheme) -> Option<&dyn Authenticator> {
        self.by_scheme
            .get(&AuthSchemeKey::from(scheme.clone()))
            .map(AsRef::as_ref)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AuthSchemeKey(String);

impl From<AuthScheme> for AuthSchemeKey {
    fn from(s: AuthScheme) -> Self {
        let name = match s {
            AuthScheme::Bearer => "bearer",
            AuthScheme::SignedJwt => "signed_jwt",
            AuthScheme::None => "none",
            AuthScheme::Mtls => "mtls",
            AuthScheme::Oauth2 => "oauth2",
        };
        Self(name.to_owned())
    }
}
