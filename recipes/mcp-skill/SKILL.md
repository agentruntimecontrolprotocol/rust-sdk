# research

Use the `research` MCP tool when the user asks a question that requires
gathering information from multiple sources, synthesising evidence, or
producing a structured report.

## When to use

- "Research X for me"
- "What do we know about Y?"
- "Give me a thorough answer on Z"

## Input fields

| Field        | Type   | Required | Description                              |
|--------------|--------|----------|------------------------------------------|
| `question`   | string | ✓        | The research question to answer          |
| `budget_usd` | number |          | Max spend in USD (default: 0.10)         |

## What happens

1. The MCP server submits a `planner` ARCP job with a `cost.budget` lease
   capped at `budget_usd`.
2. The planner decomposes the question and delegates sub-questions to
   `worker` agents, each sliced from the remaining budget.
3. Workers that exhaust their slice stop with `BUDGET_EXHAUSTED`; others
   continue.  The planner never exceeds the top-level cap.
4. The terminal `job.completed` result is returned as an MCP `text` block.

## Notes

- If `budget_usd` is omitted the default cap is $0.10.
- The MCP server maintains one persistent ARCP session; there is no
  per-call connection overhead.
