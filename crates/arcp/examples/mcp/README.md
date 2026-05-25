# mcp

ARCP runtime fronting an MCP server. Translates ARCP `tool.invoke`
envelopes into MCP `call_tool` calls, and emits the ARCP job lifecycle
back to the calling client (RFC §20).

## Before ARCP

MCP is a tool-discovery + tool-call protocol. It doesn't carry job
lifecycle, heartbeats, leases, or human-in-the-loop. Wrapping an MCP
tool for use inside an agent runtime usually means a bespoke shim.

## With ARCP

```text
ARCP client  --tool.invoke-->  bridge  --call_tool-->  MCP server
ARCP client  <--job.{accepted,started,completed,failed}--  bridge
```

```rust
async fn handle_invoke(send, mcp, request) {
    // job.accepted -> job.started -> call_via_mcp -> job.completed | job.failed
}
```

Each upstream MCP tool surfaces as `arcpx.mcp.tool.<name>.v1` so ARCP
clients can negotiate exactly which tools they require.

## ARCP primitives

- `tool.invoke` -> MCP `call_tool` translation — §20.
- `arcpx.mcp.tool.<name>.v1` capability namespace — §21.1, §20.
- ARCP job lifecycle wrapping a synchronous MCP call — §10.

## File tour

- `main.rs` — bridge loop + per-invoke handler.
- `upstream.rs` — `ClientSession` + `upstream_params` stubs (replace
  with a real `mcp-rs` binding when one stabilizes).

## Variations

- Map MCP `resources/list` -> ARCP `kind: event` streams.
- Cache the MCP `tools/list` and refresh on a schedule.
- Translate MCP errors with finer granularity than `FAILED_PRECONDITION`.
