# TPT Document Validator: command reference

`tpt-document` checks JSON and XML documents, such as invoices, purchase orders,
supplier records and shipping records. Each document gets a verdict: **PASS**,
**REVIEW** or **FAIL**.

Written for: people checking documents before they go into another system.

## Quick start

```sh
tpt-document validate invoice.xml --schema templates/invoice.schema.yaml --policy templates/invoice.policy.yaml
```

```text
REVIEW invoice.xml (approval_required, approvers: finance)
```

The schema says what a valid document looks like. The policy says what a
valid document should lead to. Both are described in
[SCHEMA_REFERENCE.md](SCHEMA_REFERENCE.md) and [POLICY_REFERENCE.md](POLICY_REFERENCE.md).

## Verdicts

The pipeline runs in order:

1. **Parse.** Read the file as JSON or XML. A file that does not parse is FAIL.
2. **Schema.** Check the fields. Any schema error is FAIL, and the policy is not run.
3. **Policy.** If a policy is given, evaluate the document against it. The decision sets the verdict.

| Verdict | When |
|---|---|
| `PASS` | The schema passes, and the policy decision is `approved` (or there is no policy). |
| `REVIEW` | The policy decision is `review` or `approval_required`. |
| `FAIL` | The document does not parse, fails the schema, or the policy decision is `rejected`. |

Each document's result lists everything found. A FAIL from the schema lists
every field error. A FAIL from parsing gives the parser's message against the
field `(document)`.

## Command

### `validate <documents>... --schema FILE [--policy FILE] [--out DIR]`

| Argument or flag | Meaning |
|---|---|
| `<documents>...` | One or more files, each `.json` or `.xml`. |
| `--schema FILE` | Required. The schema, in YAML. |
| `--policy FILE` | Optional. A policy from `tpt-policy`. Documents that pass the schema are evaluated against it. |
| `--out DIR` | Optional. Writes one `<name>.result.json` per document. |

Each document prints one line: the verdict, the file, and for a policy result,
the decision and the approvers. Schema errors follow, one per line.

The exit code is the worst verdict across all the documents:

| Code | Meaning |
|---|---|
| `0` | Every document passed. |
| `20` | At least one needs review, and none failed. |
| `30` | At least one failed. |
| `1` | A file could not be read or written. The run stops. |
| `2` | Invalid schema or policy, an unknown document type, or a bad command line. |

Codes `20` and `30` match `tpt-policy` for review and rejected.

A file that cannot be read stops the run with exit `1`. A document that cannot
be parsed is not a read error. It gets FAIL, and the run goes on.

### `doctor`

Checks the install. It parses a built-in XML document and checks it against a
built-in schema. Exits `0` when all checks pass.

## Documents

### JSON

The file must hold one JSON object. Fields are the object's keys. Values keep
their JSON types, so `"2000"` does not pass an `integer` or `number` field.

### XML

The XML rules are fixed, so one schema works for XML and JSON:

- **The root element is not a field.** Its children are the fields.
  `<invoice><total>10</total></invoice>` has the field `total`.
- **An element with only text** is that text. An empty element is null.
- **Attributes** become fields with an `@` in front of the name. `<supplier id="S-17">`
  has the field `supplier.@id`. Use `supplier.@id` in a schema to check it.
- **Repeated elements** become a list. Two `<line>` elements make the field `line`
  a list of two values. A schema can check `line.0` and `line.1`.
- **Text beside child elements** is kept under `#text`.
- **Entities** such as `&amp;` and `&#65;` are decoded.
- **XML values are text.** The schema checks them in text mode, so `"5750.00"` is
  read as a number. The policy sees the typed value, so `total gte 5000` works.

Some valid XML is refused:

- A root element with text and no child elements, such as `<doc>text</doc>`.
- Malformed XML, such as an unclosed tag. The verdict is FAIL with the position.

Only XML 1.0 is supported. DTDs and external entities are not read.

## Output files

With `--out DIR`, each document gets `<name>.result.json`:

```json
{
  "tool": "tpt-document",
  "tool_version": "2026.1.0",
  "document": { "file": "invoice.xml", "format": "xml", "sha256": "..." },
  "schema": { "name": "invoice", "version": "1.0.0", "engine_version": "2026.1.0" },
  "policy": { "name": "invoice-approval", "version": "1.0.0" },
  "verdict": "REVIEW",
  "schema_errors": [],
  "evaluation": { "decision": "approval_required", "approvers": ["finance"], "...": "..." }
}
```

The result has no timestamp, so the same document and rules give the same file.
`sha256` is the hash of the document file as read, so it can be checked against
the source.

## Templates

`examples/documents/templates/` has schemas and a policy for these document types:

| Template | Schema file |
|---|---|
| Invoice | `invoice.schema.yaml` (with `invoice.policy.yaml`) |
| Purchase order | `purchase-order.schema.yaml` |
| Supplier | `supplier.schema.yaml` |
| Customer | `customer.schema.yaml` |
| Shipping record | `shipping.schema.yaml` |

They are starting points. Copy one, and change the fields and rules to match
your documents.

Sample documents are in `examples/documents/`:

- `invoice.xml`: REVIEW, because the total is over 5,000.
- `invoice-bad.json`: FAIL, with several schema errors.
- `broken.xml`: FAIL, malformed XML.
- `purchase-order.json` and `shipment.xml`: PASS.

## Limits in this version

- XML 1.0 only. No DTDs, namespaces are read as plain names, and no XSD.
- Each document is loaded whole. There is no streaming.
- The HTML report and the Docker image are not built yet.
- Document-level rules across several documents, such as duplicate invoice numbers,
  are not supported. Use `tpt-data` with `unique` for that.
- The invoice arithmetic check (subtotal, tax and total agreeing) belongs to the
  Invoice Validator, not this product.
