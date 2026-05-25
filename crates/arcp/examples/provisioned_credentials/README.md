# provisioned_credentials

Demonstrates a runtime configured with a LiteLLM-style credential provisioner.

Run:

```sh
cargo run --example provisioned_credentials
```

The example submits a `tool.invoke` with:

- `cost.budget: ["USD:1"]`
- `model.use: ["tier-fast/*"]`

The provisioner prints the simulated upstream calls:

```text
[provisioner] POST /key/generate budget=USD:1 models=tier-fast/*
[client] job.accepted credential id=cred_0000000000000001 scheme=bearer
[provisioner] POST /key/delete id=cred_0000000000000001
```

Only credential ids are printed. The bearer value is carried to the job submitter
on `job.accepted` but is treated as a secret by runtime diagnostics and
subscription fanout.
