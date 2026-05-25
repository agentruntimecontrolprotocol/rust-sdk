//! Concrete [`Authenticator`][arcp_core::auth::Authenticator] implementations.
//!
//! ARCP v1.1 §6.1 defines `bearer` as the sole normative auth scheme;
//! `signed_jwt` and `none` are SDK extensions for runtimes that need them.
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
