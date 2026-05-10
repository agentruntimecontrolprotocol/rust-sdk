//! `none` authentication scheme (RFC §8.2).
//!
//! Only valid when `capabilities.anonymous: true` was negotiated. The
//! runtime MUST refuse otherwise.

use async_trait::async_trait;

use super::{AuthOutcome, Authenticator};
use crate::error::ARCPError;
use crate::messages::{AuthScheme, Capabilities, CapabilityName, ClientIdentity, Credentials};

/// Authenticator for the `none` scheme.
///
/// Accepts any credentials block but only when `negotiated.anonymous` is
/// `true`. The principal field is `"anonymous"`.
#[derive(Debug, Default)]
pub struct NoneAuthenticator;

impl NoneAuthenticator {
    /// Construct.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Authenticator for NoneAuthenticator {
    fn scheme(&self) -> AuthScheme {
        AuthScheme::None
    }

    async fn authenticate(
        &self,
        _creds: &Credentials,
        _client: &ClientIdentity,
        negotiated: &Capabilities,
    ) -> Result<AuthOutcome, ARCPError> {
        if negotiated.has(CapabilityName::Anonymous) {
            Ok(AuthOutcome::Accept {
                principal: "anonymous".into(),
            })
        } else {
            Ok(AuthOutcome::Reject {
                reason: "anonymous auth requires capabilities.anonymous: true".into(),
            })
        }
    }
}
