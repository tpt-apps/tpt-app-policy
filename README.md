# TPT App Policy

Define business rules in a YAML file. Check JSON data against them. Get a
decision and the rules that produced it. Runs locally, offline, with no account.

> Status: **0.1 (pre-release), not yet tagged.** The CLI, policy format, HTTP
> service and `doctor` command work. The Docker image was tested on Windows only.
> Licensed MIT OR Apache-2.0. See [TODO.md](TODO.md).

## Documentation

- [Install](docs/INSTALL.md) · [Quickstart](docs/QUICKSTART.md) · [Concepts](docs/CONCEPTS.md)
- [CLI reference](docs/CLI_REFERENCE.md) · [Policy reference](docs/POLICY_REFERENCE.md)
- [Integration](docs/INTEGRATION.md) · [Troubleshooting](docs/TROUBLESHOOTING.md) · [FAQ and tips](docs/FAQ.md)
- [Security](docs/SECURITY.md) · [Get help](docs/SUPPORT.md)
- Data Validator (in development): [command reference](docs/DATA_VALIDATOR.md) · [schema reference](docs/SCHEMA_REFERENCE.md)
- Document Validator (in development): [command reference](docs/DOCUMENT_VALIDATOR.md)
- Invoice Validator (in development): [command reference](docs/INVOICE_VALIDATOR.md)

## Quickstart

```sh
cargo build --release
```

Write a policy (`expense.yaml`):

```yaml
policy: expense
version: "1.0.0"

rules:
  - id: manager-approval
    description: Expenses over $1,000 need manager approval
    when:
      expense.amount:
        gt: 1000
    then:
      decision: approval_required
      approver: manager
```

Write some data (`expense.json`):

```json
{ "expense": { "amount": 6200 } }
```

Check it:

```sh
tpt-policy check expense.yaml expense.json
```

```json
{
  "decision": "approval_required",
  "approvers": ["manager"],
  "matched_rules": ["manager-approval"],
  "...": "..."
}
```

Exit code `10`. See the exit code table below.

## Commands

| Command | Purpose |
|---|---|
| `tpt-policy validate <policy>` | Check the policy file for errors. |
| `tpt-policy check <policy> <input>` | Print the decision as JSON. Use `-` for stdin. |
| `tpt-policy explain <policy> <input>` | Print a readable explanation, including failed rules. Add `--format json` for JSON. |
| `tpt-policy run <policy>` | Read JSON from stdin, print the decision as JSON. |
| `tpt-policy test <policy>` | Run the `tests:` block in the policy file. |
| `tpt-policy serve <policy>` | Serve evaluations over HTTP. See below. |
| `tpt-policy doctor` | Check that this install works. |

### HTTP service

```sh
TPT_POLICY_TOKEN=change-me tpt-policy serve expense.yaml --listen 127.0.0.1:8080

curl -X POST http://127.0.0.1:8080/v1/evaluate \
  -H "Authorization: Bearer change-me" \
  -H "Content-Type: application/json" \
  -d '{"expense": {"amount": 6200}}'
```

- `POST /v1/evaluate` takes the input JSON and returns the same JSON as `check`.
- `GET /healthz` returns `{"status": "ok"}` and needs no token.
- Listens on `127.0.0.1` by default. Listening on another address without a token prints a warning.
- Request bodies over 1 MB are rejected with 413.
- Errors are JSON with `what`, `why` and `fix`.
- Single-threaded and intended for local or internal use, not for direct exposure to the internet.

## Decisions

When several rules match, the most severe decision wins:

`rejected` > `approval_required` > `review` > `approved`

Approvers and requirements from every matching rule are merged. If no rule
matches, the policy's `default_decision` applies. It defaults to `review`, so
gaps in the rules need a person to look at them.

## Operators

| Operator | Example | Notes |
|---|---|---|
| `equals` / shorthand | `status: active` | Numbers compare by value, so `5000` equals `5000.0`. |
| `not_equals` | `status: {not_equals: active}` | |
| `gt`, `gte`, `lt`, `lte` | `amount: {gte: 1000, lt: 5000}` | Numbers, or text compared alphabetically. |
| `in`, `not_in` | `country: {in: [NZ, AU]}` | |
| `contains` | `tags: {contains: urgent}` | Works on lists and on text. |
| `exists` | `ref: {exists: true}` | `false` means missing or null. |
| `all`, `any`, `not` | `any: [ {...}, {...} ]` | Combine conditions. Sibling keys in one `when` are AND-ed. |

A missing field makes its check fail. Only `exists: false` passes for a missing field.

Field paths use dots (`expense.amount`). Numbers index into lists (`items.0.sku`).

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Approved, or the command succeeded |
| `10` | Approval required |
| `20` | Review |
| `30` | Rejected |
| `1` | A file could not be read |
| `2` | Invalid policy or command line |
| `3` | Input is not valid JSON |
| `4` | One or more inline tests failed |
| `5` | `doctor` found a failing check |

## Examples

- [examples/expense](examples/expense/) — the expense demo from the spec.
- [examples/purchasing](examples/purchasing/) — rules with inline tests.

```sh
tpt-policy test examples/purchasing/purchasing.yaml
```

## Licence

Dual-licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT license](LICENSE-MIT)

at your option. Unless you state otherwise, any contribution you submit is
dual-licensed as above, without additional terms.
