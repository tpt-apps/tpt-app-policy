# Quickstart

From zero to a working rule in a few minutes. You need `tpt-policy` on your
`PATH` (see [INSTALL.md](INSTALL.md)).

## 1. Write a policy

Save this as `expense.yaml`:

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

Each rule has:

- `id`: a name that appears in the output.
- `description`: optional, shown by `explain`.
- `when`: the condition on the input.
- `then`: the decision, and any approvers or requirements.

## 2. Check the policy

```sh
tpt-policy validate expense.yaml
```

```text
valid: expense (version 1.0.0, 1 rules, 0 tests)
```

Break the file on purpose (for example, change `gt` to `greater_than`) and run
it again. The error says what is wrong, where, why, and how to fix it.

## 3. Write some input

Save this as `expense.json`:

```json
{ "expense": { "amount": 6200 } }
```

## 4. Evaluate it

```sh
tpt-policy check expense.yaml expense.json
```

```json
{
  "decision": "approval_required",
  "approvers": ["manager"],
  "matched_rules": ["manager-approval"],
  ...
}
```

The exit code is `10`, which is the code for `approval_required`. Try a
smaller amount, such as `500`, and the decision becomes `review` because no rule
matched and the default applies.

For a readable version with the failed rules:

```sh
tpt-policy explain expense.yaml expense.json
```

## 5. Add a test

Tests live in the policy file, so they travel with the rules:

```yaml
tests:
  - name: big expense needs a manager
    input:
      expense:
        amount: 6200
    expect:
      decision: approval_required
      matched_rules: [manager-approval]
```

Run them:

```sh
tpt-policy test expense.yaml
```

```text
PASS big expense needs a manager

1 tests passed
```

A failing test prints `FAIL` with the reason and exits `4`.

## Next

- [CLI_REFERENCE.md](CLI_REFERENCE.md): every command and flag.
- [POLICY_REFERENCE.md](POLICY_REFERENCE.md): every operator and field.
- [INTEGRATION.md](INTEGRATION.md): call the policy from another system.
- The [examples](../examples/) folder has a full expense policy and a purchasing
  policy with tests.
