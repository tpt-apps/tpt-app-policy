# TPT Approval Engine

Say who must approve a request, from approval rules you write once. The engine
gives a decision and the approvers. Your application or workflow system acts on
that decision. The engine does not send email, track tasks or hold state.

## Quick start

```text
tpt-approve validate --rules expense.approval.yaml
tpt-approve check request.json --rules expense.approval.yaml
tpt-approve check request.json --rules expense.approval.yaml --format text
tpt-approve doctor
```

The example in `examples/approval/` runs as shown:

```text
tpt-approve check examples/approval/requests/director.json --rules examples/approval/expense.approval.yaml
```

## Commands

### `validate --rules FILE`

Checks the rules file. It prints the approval name and rule count, then checks
the amounts the rules cover:

- `warning: no rule covers ...` means some requests get `review`. Fix this by
  adding a rule for that range.
- `note: ...` is information. The main case is an automatic rule and an approval
  rule that both match the same amount. The approval wins.

Warnings do not change the exit code. Errors do (exit 2).

### `check REQUEST --rules FILE [--format json|text]`

Evaluates one request. `REQUEST` is a JSON object file. The default output is
JSON. `text` prints the decision, the approvers, the requirements, and which
rules matched and failed.

The output is the same result format as TPT App Policy, so systems that read
`tpt-policy` output can read it too.

### `doctor`

Prints the version and platform, checks that the working folder is writable, and
checks the built-in rules give the expected decisions.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Approved automatically. Or, for `validate`, the rules are valid. |
| `1` | A file could not be read or written. |
| `2` | Invalid approval rules, or a bad command line. |
| `3` | The request is not a JSON object, or is not valid JSON. |
| `5` | `doctor` found a problem. |
| `10` | Approval is required. The output names the approvers. |
| `20` | Review. No rule covers this request. |

A request that no rule covers is never approved. It gets `review`.

## Rules file

```yaml
approval: expense-approval
version: "1.0.0"
rules:
  - id: small-expense
    description: Expenses under $1,000 are approved automatically
    when:
      amount: "<1000"
    decision: automatic

  - id: manager-band
    description: Expenses from $1,000 to $5,000 need manager approval
    when:
      amount: "1000-5000"
    approver:
      role: manager

  - id: director-band
    when:
      amount: ">5000"
    approver: director
    requirement: Quote attached
```

- `approval` is the name. It appears in every result.
- `version` is optional. It defaults to `1.0.0`.
- Each rule has:
  - `when`: one or more fields to test. All of them must match.
  - Exactly one of `decision: automatic`, or `approver`.
  - `id` (optional): the name shown in explanations. Defaults to `rule-N`, counting from 1. IDs must be unique.
  - `description` and `requirement` (optional). A requirement is something that
    must be true before the decision is final, such as "Quote attached".

### `approver`

Three forms are accepted:

```yaml
approver: manager              # one role
approver: [manager, finance]   # both must approve
approver:
  role: manager                # the same as "approver: manager"
```

### Values in `when`

| Written as | Meaning |
|---|---|
| `"<1000"` | less than 1000 |
| `"<=1000"` | at most 1000 |
| `">5000"` | more than 5000 |
| `">=5000"` | at least 5000 |
| `"1000-5000"` | from 1000 to 5000, both ends included |
| `"-5--1"` | from -5 to -1, both ends included |
| `"=sales"` or `sales` | equal to the text `sales` |
| `42` | equal to the number 42 |

A range starts before its end, so `"5000-1000"` is refused. An operator must be
followed by a number, so `"<abc"` is refused.

## Several rules match

Every matching rule adds its approvers and requirements. The approvals are
merged in rule order, with no duplicates. If one rule approves automatically and
another requires approval, the approval wins.

## Gaps and overlaps

`validate` checks one numeric field across all rules. It tests each boundary in
the rules, each gap between boundaries, and a point beyond each end. A point that
no rule covers is reported as a warning. The check only runs when every rule tests
the same single field against numbers. Otherwise it says the check was skipped.

## Output

`check` returns the same result as `tpt-policy check`:

| Field | Meaning |
|---|---|
| `decision` | `approved`, `approval_required`, `review` |
| `approvers` | The roles that must approve, in rule order. |
| `requirements` | Requirements from the matching rules. |
| `matched_rules` | The IDs of the rules that matched. |
| `failed_rules` | The rules that did not match, and why. |
| `explanations` | One message per matching rule. |
| `input_sha256` | A fingerprint of the request, so an audit can show which request produced the decision. |

The request itself is not stored in the output.

## Limits in this version

- One request per `check` run. Batch processing is not in this version.
- Gap checks cover one numeric field. Rules that test several fields are not
  checked for gaps.
- Approvals are not tracked. The engine does not know whether someone has
  approved, only who must.
- Roles are plain names. The engine does not look up people or their levels.

## Privacy

Nothing leaves the machine. The request is read from the file you give. The
output contains the decision, rule names and the fingerprint, not the request.
