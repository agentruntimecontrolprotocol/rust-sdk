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

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]
mod tests {
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde::Serialize;

    use super::*;
    use crate::messages::{AuthScheme, Capabilities, ClientIdentity, Credentials};

    #[derive(Serialize)]
    struct Mint<'a> {
        sub: &'a str,
        aud: &'a str,
        exp: usize,
    }

    fn ident() -> ClientIdentity {
        ClientIdentity {
            kind: "test".into(),
            version: "0".into(),
            fingerprint: None,
            principal: None,
        }
    }

    fn mint(secret: &[u8], sub: &str, aud: &str) -> String {
        let claims = Mint {
            sub,
            aud,
            exp: 9_999_999_999,
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret),
        )
        .expect("encode")
    }

    #[tokio::test]
    async fn valid_jwt_accepts_with_sub_as_principal() {
        let secret = b"shared-test-secret-9876543210";
        let auth = SignedJwtAuthenticator::hs256(secret, "arcp-test-runtime");
        let token = mint(secret, "alice@example.com", "arcp-test-runtime");
        let creds = Credentials {
            scheme: AuthScheme::SignedJwt,
            token: Some(token),
        };
        let outcome = auth
            .authenticate(&creds, &ident(), &Capabilities::default())
            .await
            .expect("auth call ok");
        match outcome {
            AuthOutcome::Accept { principal } => {
                assert_eq!(principal, "alice@example.com");
            }
            other => panic!("expected Accept, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn jwt_for_wrong_audience_is_rejected() {
        let secret = b"shared-test-secret-9876543210";
        let auth = SignedJwtAuthenticator::hs256(secret, "arcp-test-runtime");
        let token = mint(secret, "alice", "some-other-audience");
        let creds = Credentials {
            scheme: AuthScheme::SignedJwt,
            token: Some(token),
        };
        let outcome = auth
            .authenticate(&creds, &ident(), &Capabilities::default())
            .await
            .expect("auth call ok");
        assert!(matches!(outcome, AuthOutcome::Reject { .. }));
    }

    #[tokio::test]
    async fn jwt_with_wrong_secret_is_rejected() {
        let auth = SignedJwtAuthenticator::hs256(b"server-secret", "arcp-test-runtime");
        let token = mint(b"attacker-secret", "alice", "arcp-test-runtime");
        let creds = Credentials {
            scheme: AuthScheme::SignedJwt,
            token: Some(token),
        };
        let outcome = auth
            .authenticate(&creds, &ident(), &Capabilities::default())
            .await
            .expect("auth call ok");
        assert!(matches!(outcome, AuthOutcome::Reject { .. }));
    }

    #[tokio::test]
    async fn missing_token_is_rejected() {
        let auth = SignedJwtAuthenticator::hs256(b"x", "rt");
        let creds = Credentials {
            scheme: AuthScheme::SignedJwt,
            token: None,
        };
        let outcome = auth
            .authenticate(&creds, &ident(), &Capabilities::default())
            .await
            .expect("auth call ok");
        assert!(matches!(outcome, AuthOutcome::Reject { .. }));
    }

    #[test]
    fn scheme_reports_signed_jwt() {
        let auth = SignedJwtAuthenticator::hs256(b"x", "rt");
        assert!(matches!(auth.scheme(), AuthScheme::SignedJwt));
    }
}
