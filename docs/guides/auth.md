# Authentication (§6.1)

The Rust SDK includes bearer tokens, signed JWTs, and anonymous auth for local
or trusted deployments.

Spec reference: [§6.1](../../../spec/docs/draft-arcp-1.1.md#61-authentication).

## Bearer

Use `BearerAuthenticator` for static bearer tokens:

```rust
use arcp::auth::BearerAuthenticator;

let auth = BearerAuthenticator::new().with_token("secret-token", "alice@example.com");
```

The principal is attached to accepted sessions and jobs for authorization and
audit decisions.

## Signed JWT

Use the JWT authenticator when a deployment already issues signed tokens. The
authenticator validates signature and claims before accepting the session.

See [`examples/custom_auth.rs`](../../examples/custom_auth.rs).

## Anonymous

`NoneAuthenticator` allows sessions with `Credentials::None`. Advertise
`Capabilities { anonymous: Some(true), .. }` when this is intentional.

Anonymous auth is useful for local examples and tests, not for public network
listeners.

## Custom authenticators

Implement `Authenticator` to integrate an identity provider, mTLS verifier, or
host-specific session policy. Return an authenticated principal on success and
an ARCP auth error on failure.

## Resume invariant

Resume must preserve the original session authority. A production
implementation should bind resume credentials to the same principal that opened
the session.
