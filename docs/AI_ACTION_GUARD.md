# TPT AI Action Guard

Decide whether an action an AI agent asks to take may go ahead. The guard
answers **ALLOW**, **DENY** or **REQUIRE_APPROVAL** from rules you write. It
gives a reason for each answer.

The guard never runs the action. Your system runs the action, and only after
ALLOW. For REQUIRE_APPROVAL, your system asks a person first.

## Quick start

```text
tpt-ai-guard check request.json --policy customer-actions.policy.yaml
tpt-ai-guard check request.json --policy customer-actions.policy.yaml --format text
tpt-ai-guard validate --policy customer-actions.policy.yaml
tpt-ai-guard doctor
```

Example, from `examples/ai-guard/`:

```text
tpt-ai-guard check examples/ai-guard/requests/refund-large.json --policy examples/ai-guard/customer-actions.policy.yaml
```

```json
{
  "decision": "REQUIRE_APPROVAL",
  "reason": "refund exceeds $5,000 limit",
  "action": "refund_customer",
  "matched_rules": ["refund-over-limit"],
  "policy": { "name": "customer-actions", "version": "1.0.0" },
  "input_sha256": "..."
}
```

## Request

A JSON object with a string `action`. Other fields are the details the rules test:

```json
{ "action": "refund_customer", "amount": 7500, "customer": "C-1009" }
```

## Policy

The policy is a list of rules. Each rule names an action, may add conditions,
and gives a decision:

```yaml
name: customer-actions
version: "1.0.0"
rules:
  - id: refund-over-limit
    action: refund_customer
    when:
      amount:
        gt: 5000
    decision:
      require_approval: true
    reason: refund exceeds $5,000 limit
```

- `action` (required): the action the rule covers.
- `when` (optional): conditions on the request, written in the same form as
  `tpt-policy`: `equals`, `not_equals`, `gt`, `gte`, `lt`, `lte`, `in`, and so on.
  Without `when`, the rule covers every request for that action.
- `decision` (required): one of
  - `allow`, `deny` or `require_approval`, or
  - a map with exactly one of them set to `true`, such as `{ require_approval: true }`.
- `reason` (optional): shown in the verdict when this rule decides the outcome.
  Without it, the verdict says "rule '<id>' matched".
- `id` (optional): defaults to `rule-N`, counting from 1. IDs must be unique.

The file may also be a bare list of rules, with no `name` or `version`. The name
then defaults to `action-policy`.

## How the decision is made

1. Every rule for the requested action is checked against the request.
2. The most restrictive matching rule wins: **DENY**, then **REQUIRE_APPROVAL**,
   then **ALLOW**. If a deny and an allow both match, the answer is DENY.
3. The reason comes from the matching rules that gave the winning decision.
4. If no rule matches, the answer is **DENY**, with the reason "no rule allows
   action '...'; denied by default". Unknown actions and requests that miss every
   condition are denied.

A request with no `amount` does not match a rule that tests `amount`. So a refund
with no amount is denied under the example policy.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | ALLOW |
| `1` | A file could not be read |
| `2` | Invalid policy, or a bad command line |
| `3` | The request is not a JSON object, has no `action`, or is not valid JSON |
| `5` | `doctor` found a problem |
| `10` | REQUIRE_APPROVAL |
| `30` | DENY |

Your integration should treat any code other than `0` as "do not run the
action", and any code other than `0` or `10` as an error to investigate.

## Privacy

The verdict contains the action name, the decision, the reason, the rule IDs,
the policy name and version, and a SHA-256 fingerprint of the request. It does
not contain the request's fields, so card numbers and personal details in a
request are not written to the output. Nothing leaves the machine.

## What this version does not do

- **MCP or tool gating.** The guard does not sit in front of MCP servers or
  tool calls. That integration needs `tpt-mcpbox`, which is not in this
  repository yet. For now, your code calls the guard and then calls the tool.
- **Capability security.** Tokens or capabilities that limit what an agent can
  reach need `tpt-capsec`, which is not in this repository yet.
- **A server.** There is no REST interface yet. Use the CLI, or call the guard's
  Rust library from your code.
- **Approval workflow.** REQUIRE_APPROVAL says that approval is needed. It does
  not send the request to anyone, or record who approved it.
- **Docker.** No image yet.
