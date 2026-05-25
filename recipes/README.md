# ARCP Rust SDK — Recipes

Narrative end-to-end examples that combine multiple SDK features into
realistic agent patterns. Each recipe is a self-contained directory with a
`client.rs` (the caller side) and a `server.rs` (the runtime side).

> **Recipes are illustrative, not runnable as-is.** They are not wired into
> any `Cargo.toml` as `[[example]]` or `[[bin]]` targets and most still
> contain `todo!()` bodies marking the integration points. Read them as
> annotated walkthroughs; for runnable code, see
> [`crates/arcp/examples/`](../crates/arcp/examples/) and
> [`crates/arcp/tests/`](../crates/arcp/tests/).

## Recipes

| Recipe | Highlights |
|--------|------------|
| [email-vendor-leases](email-vendor-leases/) | Lease enforcement (§9.3) surfaced as a recoverable `tool_result` error; vendor event-kind namespace (§15 / §8.2) |
| [mcp-skill](mcp-skill/) | MCP → ARCP bridge: one long-lived ARCP session per MCP process; each Claude Code `/research` call submits a `planner` job |
| [multi-agent-budget](multi-agent-budget/) | Cascading cost budgets (§9.6): planner decomposes, delegates workers each with a budget slice; `BUDGET_EXHAUSTED` (§9.4 subsetting) caps spending |
| [stream-resume](stream-resume/) | Chunked streaming result (§8.4) + transport drop + session resume (§6.3) with `EventLog` replay |

## Adapting a recipe

To run a recipe locally, copy its `server.rs` / `client.rs` into
`crates/arcp/examples/<name>/` and add `[[example]]` entries to
`crates/arcp/Cargo.toml`:

```toml
[[example]]
name = "<recipe-name>-server"
path = "examples/<name>/server.rs"

[[example]]
name = "<recipe-name>-client"
path = "examples/<name>/client.rs"
```

Then replace the `todo!()` bodies with real implementations and run:

```sh
# terminal 1 – start the runtime
cargo run --example <recipe-name>-server

# terminal 2 – run the client
cargo run --example <recipe-name>-client
```
