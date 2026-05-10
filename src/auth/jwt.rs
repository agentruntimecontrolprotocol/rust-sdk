//! `signed_jwt` authentication scheme (RFC §8.2).

use async_trait::async_trait;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::Deserialize;

use super::{AuthOutcome, Authenticator};
use crate::error::ARCPError;
use crate::messages::{AuthScheme, Capabilities, ClientIdentity, Credentials};

/// Authenticator for `signed_jwt`.
///
/// Carries an HMAC-SHA256 secret and the audience this runtime expects.
/// On successful validation the principal is the JWT's `sub` claim.
pub struct SignedJwtAuthenticator {
    decoding_key: DecodingKey,
    validation: Validation,
}

impl std::fmt::Debug for SignedJwtAuthenticator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignedJwtAuthenticator")
            .finish_non_exhaustive()
    }
}

impl SignedJwtAuthenticator {
    /// Construct an HS256 authenticator with `secret` and the audience the
    /// runtime expects to see in the `aud` claim.
    #[must_use]
    pub fn hs256(secret: &[u8], audience: impl Into<String>) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_audience(&[audience.into()]);
        Self {
            decoding_key: DecodingKey::from_secret(secret),
            validation,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
}

#[async_trait]
impl Authenticator for SignedJwtAuthenticator {
    fn scheme(&self) -> AuthScheme {
        AuthScheme::SignedJwt
    }

    async fn authenticate(
        &self,
        creds: &Credentials,
        _client: &ClientIdentity,
        _negotiated: &Capabilities,
    ) -> Result<AuthOutcome, ARCPError> {
        let Some(token) = &creds.token else {
            return Ok(AuthOutcome::Reject {
                reason: "signed_jwt scheme requires a token".into(),
            });
        };
        match decode::<Claims>(token, &self.decoding_key, &self.validation) {
            Ok(data) => Ok(AuthOutcome::Accept {
                principal: data.claims.sub,
            }),
            Err(err) => Ok(AuthOutcome::Reject {
                reason: format!("jwt validation failed: {err}"),
            }),
        }
    }
}
