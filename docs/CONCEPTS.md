# Concepts

## What a policy does

A policy is a set of business rules. You give it a JSON document, such as an
expense or a purchase order. It returns a **decision** and the rules that
produced it.

It does not call anything, store anything, or change the input. It answers one
question: given this data, what do the rules say?

## Decisions

Each decision means something to whatever system acts on it:

| Decision | Meaning |
|---|---|
| `approved` | Go ahead. |
| `review` | A person should look at this. |
| `approval_required` | Named approvers must sign off first. |
| `rejected` | Do not proceed. |

## Rules and outcomes

A rule has a condition (`when`) and an outcome (`then`). If the condition is
true for the input, the rule matches, and its outcome applies.

Several rules can match one input. The final decision is the most severe one:

`rejected` > `approval_required` > `review` > `approved`

Approvers, requirements and warnings from every matching rule are collected
together. Nothing is lost, so the output shows everything the rules asked for.

## No match

If no rule matches, the policy's `default_decision` applies. It is `review` by
default. Rules have to be written to cover what you expect. Anything they miss
goes to a person rather than being approved by accident.

## Determinism

The same policy and the same input always give the same output, byte for byte.
The engine does not read the clock, use random numbers, or call the network. This
makes results repeatable and easy to test.

## Explanations

Every result says:

- which rules matched, and why (`explanations`);
- which rules did not match, and the first check that failed (`failed_rules`);
- the policy name and version, and the engine version.

So a person can see why a decision was made, and an auditor can rerun it later.

## Where it runs

Locally, on the machine that runs it. It works offline, with no account and no
licence key. Other products in the TPT portfolio build on the same core.

See [POLICY_REFERENCE.md](POLICY_REFERENCE.md) for the rule syntax.
