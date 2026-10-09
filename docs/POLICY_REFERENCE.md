# Policy reference

A policy is a YAML file. It has metadata, a list of rules, and optionally inline
tests.

## Top-level keys

| Key | Required | Meaning |
|---|---|---|
| `policy` | Yes | Policy name. Appears in every output. |
| `version` | No | Policy version: `MAJOR`, `MAJOR.MINOR` or `MAJOR.MINOR.PATCH`, numbers only, such as `"2.1.0"`. Recorded in output, and shown as `name@version` (e.g. `purchasing@2.1.0`). Quote it: an unquoted `1.10` is read as a number. Defaults to `unversioned`. |
| `default_decision` | No | Decision when no rule matches. Defaults to `review`. |
| `rules` | Yes | List of rules, checked in file order. |
| `tests` | No | Inline test cases. See [Tests](#tests). |

## Rules

```yaml
rules:
  - id: purchase-director
    description: Purchases of $5,000 or more need director approval
    when:
      purchase.amount:
        gte: 5000
    then:
      decision: approval_required
      approver: director
      requirement: Quote attached
      warning: Check the vendor is approved
```

| Key | Required | Meaning |
|---|---|---|
| `id` | Yes | Unique rule name. Listed in `matched_rules`. |
| `description` | No | Explanation shown by `explain`. |
| `when` | Yes | The condition. See [Conditions](#conditions). |
| `then` | Yes | The outcome. See [Outcomes](#outcomes). |

## Conditions

A `when` block is a set of checks on fields. Sibling checks are AND-ed:

```yaml
when:
  expense.amount:
    gte: 1000
    lt: 5000
  expense.category: travel
```

That condition is true when the amount is between 1000 and 4999 (inclusive of
1000) and the category is `travel`.

Combine conditions with `all`, `any` and `not`:

```yaml
when:
  any:
    - region: { in: [NZ, AU] }
    - { amount: { lte: 100 } }
```

```yaml
when:
  not:
    status: { equals: draft }
```

### Operators

| Operator | Example | Notes |
|---|---|---|
| `equals` (or a bare value) | `status: active` | Numbers compare by value, so `5000` equals `5000.0`. |
| `not_equals` | `status: { not_equals: active }` | |
| `gt`, `gte`, `lt`, `lte` | `amount: { gte: 1000 }` | Numbers, or text compared alphabetically. |
| `in`, `not_in` | `country: { in: [NZ, AU] }` | Value must be one of the list. |
| `contains` | `tags: { contains: urgent }` | Works on lists, and on text for substrings. |
| `exists` | `reference: { exists: true }` | `true` means present and not null. `false` means missing or null. |

Not yet supported: regular expressions (`matches`). Planned after a regex
dependency is chosen.

### Missing fields

- Any check on a missing or null field fails.
- The exception is `exists: false`, which passes.

So `expense.amount: { lt: 5000 }` does not match an input with no `amount`.
Use `exists: false` if you need to match on absence.

### Field paths

- Dots separate levels: `expense.amount`.
- Numbers index into lists: `items.0.sku` is the SKU of the first item.

## Outcomes

```yaml
then:
  decision: approval_required
  approver: manager
  requirement: Receipt attached
  warning: Above the monthly budget
```

| Key | Meaning |
|---|---|
| `decision` | One of `approved`, `review`, `approval_required`, `rejected`. |
| `approver` | A role that must approve. Plain text, or `{ role: manager }`. |
| `requirement` | Something that must be true before the decision is final. |
| `warning` | A note that does not change the decision. |

### Combining decisions

When several rules match, the most severe decision wins:

`rejected` > `approval_required` > `review` > `approved`

Approvers, requirements and warnings from every matching rule are merged. The
order is the order the rules appear in the file, and duplicates are removed.

### Default decision

If no rule matches, the decision is `default_decision`. The default is
`review`, so a gap in the rules sends the case to a person.

## Output

`check` and `run` print JSON with these fields:

| Field | Meaning |
|---|---|
| `decision` | The final decision. |
| `approvers` | Merged list of approvers. |
| `requirements` | Merged list of requirements. |
| `matched_rules` | Rule IDs whose condition matched. |
| `failed_rules` | Rules that did not match, each with the first failed check. |
| `explanations` | For each matched rule, its decision and description. |
| `warnings` | Merged warnings. |
| `policy` | `name` and `version` of the policy used. |
| `engine_version` | Version of the evaluation engine. |
| `input_sha256` | SHA-256 of the input in canonical form (keys sorted, no extra whitespace). Two inputs with the same data give the same hash, so an audit can show which input produced a decision without storing the input. |

Output has no timestamp, so the same input always gives byte-identical output.
The input hash is part of that output, so it is also stable.

## Tests

```yaml
tests:
  - name: large purchase
    input:
      amount: 6000
    expect:
      decision: approval_required
      matched_rules: [purchase-director]
```

- `name`: shown in `test` output.
- `input`: JSON object, as for `check`.
- `expect.decision`: required decision.
- `expect.matched_rules`: optional. If given, must match exactly.

See [examples/purchasing/](../examples/purchasing/) for a full set.

## Validation

`validate` reports every problem it finds, with:

- **what**: the kind of problem.
- **where**: a key path such as `rules[0].when.amount.greater_than`, or a line and
  column for YAML syntax errors.
- **why** and **fix**: what to change.

Semantic errors give a key path and, where the file has the key in block style,
the line: `rules[0].when.amount.greater_than (line 6)`. The line is the one that
holds the key, or the list item. If the path cannot be followed in the file, for
example inside a flow-style `{...}` value, the line of the nearest key is given.

Example, from [examples/invalid/](../examples/invalid/):

```text
error: unknown operator at rules[0].when.expense.amount.greater_than
  why: 'greater_than' is not an operator
  fix: use one of: equals, not_equals, gt, gte, lt, lte, in, not_in, contains, exists
  file: examples/invalid/unknown-operator.yaml
```
