# human_input

`human.input.request` fanned across phone (ntfy), email, and Slack.
First channel to answer wins; losers are told to settle. Deadline
elapsed → translates to `human.input.cancelled` per RFC §12.4.

## Before ARCP

"Page the on-call" usually means a custom integration per messaging
backend, hard-coded to one channel. Multi-channel fan-out, with
deadline + first-wins resolution, is hand-rolled every time.

## With ARCP

```rust
// for await env in client.events() {
//     if env.type == "human.input.request" {
//         tokio::spawn(fan_out(client, env));
//     }
// }
```

`fan_out` races the channels, sends `human.input.response` from the
winner, and tells the rest with `human.input.cancelled { code: "OK" }`.

## ARCP primitives

- `human.input.request` carrying `prompt`, `response_schema`,
  `expires_at` — §12.
- `human.input.response` from the winner — §12.
- `human.input.cancelled` with reason: `DEADLINE_EXCEEDED` for
  timeout, `OK + answered elsewhere` for losers — §12.4.

## File tour

- `main.rs` — drain inbound + spawn `fan_out`.
- `channels.rs` — ntfy / email / Slack adapters (stubs).

## Variations

- Add a `human.choice.request` flow for one-of-N pickers.
- Sticky channel: prefer the medium that answered last time.
