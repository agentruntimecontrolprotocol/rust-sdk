//! Concrete [`Authenticator`][arcp_core::auth::Authenticator] implementations
//! for the schemes ARCP §8.2 defines: `bearer`, `signed_jwt`, and `none`.
//!
//! The trait, [`AuthOutcome`][arcp_core::auth::AuthOutcome], and the
//! [`AuthRegistry`][arcp_core::auth::AuthRegistry] live in `arcp-core` so
//! alternative runtimes can swap in their own validators.

pub mod bearer;
pub mod jwt;
pub mod none;

pub use bearer::BearerAuthenticator;
pub use jwt::SignedJwtAuthenticator;
pub use none::NoneAuthenticator;
