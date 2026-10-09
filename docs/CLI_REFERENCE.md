# CLI reference

Binary: `tpt-policy`. Run `tpt-policy <command> --help` for the same details
from the tool itself.

## Global

| Flag | Meaning |
|---|---|
| `-h`, `--help` | Show help. |
| `-V`, `--version` | Print the product version. |
| `--verbose` | Print each step to stderr: policy loaded, input read, decision. |
| `--debug` | Also print internal detail to stderr: input fingerprint, matched rule IDs, engine version. Never input values. |
| `--json-logs` | Write log lines to stderr as JSON, one object per line. Works with `--verbose` or `--debug`. |

Logs go to stderr, so stdout stays the decision output. Logs are off unless one of the first two flags is set.

## Commands

### `validate <policy> [--quiet]`

Checks the policy without evaluating anything.

- Prints `valid: <name> (version <v>, <n> rules, <m> tests)` on success.
- `--quiet`: print nothing on success. Errors still print.
- Exits `2` with a diagnostic if the policy is invalid, or `1` if the file
  cannot be read.

### `check <policy> <input> [--format json|text|html] [--output FILE]`

Evaluates one input and prints the decision. `<input>` is a JSON file, or `-`
to read stdin.

- `--format json` (default): the full evaluation as JSON.
- `--format text`: a short readable summary.
- `--format html`: one self-contained HTML page with the decision, the approvers,
  the matched rules and the rules that did not match. No scripts and no external
  links, so it can be attached to an email. Use `--output` to save it.
- `--output FILE`: write the result to a file instead of stdout. Exit codes
  are unchanged. If the file cannot be written, the command exits `1`.
- Exit code is the decision: `0` approved, `10` approval required, `20` review,
  `30` rejected.

### `explain <policy> <input> [--format json|text|html] [--output FILE]`

Like `check`, but also lists every matched rule's explanation and every failed
rule with the first check that failed. Default format is `text`. `--output`
works the same way as on `check`.

### `run <policy> [--output FILE]`

Reads JSON from stdin, prints the decision as JSON. Same exit codes as `check`.
Useful in pipelines:

```sh
cat order.json | tpt-policy run purchasing.yaml
```

### `test <policy> [--quiet]`

Runs the `tests:` block in the policy file. Prints `PASS` or `FAIL` for each
test, then a summary. Exits `0` when all pass, `4` when any fail.
`--quiet` prints only failing tests and the failure summary.

### `serve <policy> [--listen ADDR] [--token TOKEN]`

Serves evaluations over HTTP. See the HTTP section below.

- `--listen`: default `127.0.0.1:8080`.
- `--token`: bearer token. Prefer the `TPT_POLICY_TOKEN` environment variable,
  which keeps the value out of shell history.

### `doctor`

## Config folders

A bare policy name, such as `expense`, is looked up in `./tpt/policies/` when no file of that name exists in the current folder. It tries `expense`, then `expense.yaml`, then `expense.yml`. Any path with a folder part is used as given. `--debug` says when a name was found this way. No other folder is read yet, and there is no hidden database.

Checks that this install works: version, platform, working-directory write
access, the optional `./tpt` config folder, and a built-in self-test. Exits `0`
when required checks pass, `5` when one fails.

## HTTP

| Method and path | Auth | Purpose |
|---|---|---|
| `POST /v1/evaluate` | Bearer token, if set | Body is the input JSON. Returns the same JSON as `check`. |
| `GET /healthz` | None | Returns `{"status": "ok"}`. |

- Request bodies over 1 MB return `413`.
- Errors are JSON with `what`, `why` and `fix` fields.
- Listening on a non-loopback address with no token prints a warning.
- The server handles one request at a time.

## Input

- Input must be a JSON document. Invalid JSON exits `3`.
- The CLI reads inputs up to 10 MB. Larger inputs exit `3` with a message
  that says how to split them.
- Numbers, strings, booleans, arrays and nested objects are all supported.
- Field names in the policy use dots for nesting (`expense.amount`).

## Exit codes

These are part of the public interface and will not be renumbered.

| Code | Meaning |
|---|---|
| `0` | Approved, or the command succeeded |
| `10` | Approval required |
| `20` | Review |
| `30` | Rejected |
| `1` | A file could not be read |
| `2` | Invalid policy, or a bad command line |
| `3` | Input is not valid JSON |
| `4` | One or more inline tests failed |
| `5` | `doctor` found a failing check |

Exit codes `0`, `10`, `20` and `30` are decision results from `check`, `run` and
`explain`. Codes `1` to `5` mean the command itself did not produce a decision.
