# ARCP Rust SDK — Recipes

End-to-end examples that combine multiple SDK features into realistic
agent patterns.  Each recipe is a self-contained directory with a
`client.rs` (the caller side) and a `server.rs` (the runtime side).

## Recipes

| Recipe | Highlights |
|--------|------------|
| [email-vendor-leases](email-vendor-leases/) | Lease subset denial as a recoverable `tool_result` error (§13.4); `x-vendor.*` event-kind namespace (§15 / §8.2) |
| [mcp-skill](mcp-skill/) | MCP → ARCP bridge: one long-lived ARCP session per MCP process; each Claude Code `/research` call submits a `planner` job |
| [multi-agent-budget](multi-agent-budget/) | Cascading cost budgets (§9.6): planner decomposes, delegates workers each with a budget slice; `BUDGET_EXHAUSTED` caps spending |
| [stream-resume](stream-resume/) | Chunked streaming result (§8.4) + transport drop + session resume (§6.3) with `EventLog` replay |

## Running the examples

Each recipe has its own instructions at the top of `server.rs`.
The general pattern is:

```
# terminal 1 – start the runtime
cargo run --example <recipe-name>-server

# terminal 2 – run the client
cargo run --example <recipe-name>-client
```

Because the server and client are placed in `recipes/` rather than
`examples/`, add them to `Cargo.toml` as named example targets if you
want `cargo run --example` to work directly, or build them with:

```
cargo build --examples
```
