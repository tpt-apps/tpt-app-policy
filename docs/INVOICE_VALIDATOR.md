# TPT Invoice Validator: command reference

`tpt-invoice` checks invoices before they go into accounting or an ERP system.
It checks the arithmetic, the supplier, and whether the invoice was already
processed. Each invoice gets one verdict: **PASS**, **REVIEW** or **REJECT**.

Written for: people who check supplier invoices before they are entered.

## Quick start

```sh
tpt-invoice validate invoice.json \
  --schema invoice.schema.yaml \
  --policy invoice.policy.yaml \
  --suppliers suppliers.json \
  --ledger ledger.jsonl
```

```text
PASS   invoice.json (approved)
       ok   line items 2 lines: each amount = quantity x unit price
       ok   subtotal   subtotal 500.00 = sum of lines
       ok   tax        tax 75.00 = subtotal x rate
       ok   total      total 575.00 = subtotal + tax
       ok   supplier   supplier 123-456-789 is approved
       ok   duplicate  not in the ledger
```

## Checks

The checks run in this order. Schema failures stop the run and are reported
alone.

| # | Check | Passes when |
|---|---|---|
| 1 | Schema | The header fields pass the schema (see [SCHEMA_REFERENCE.md](SCHEMA_REFERENCE.md)). |
| 2 | Line items | There is at least one line, and each line's `amount` equals `quantity` x `unit_price`. |
| 3 | Subtotal | `subtotal` equals the sum of the line amounts. |
| 4 | Tax | `tax` equals `subtotal` x `tax_rate`. Skipped, with a note, when there is no `tax_rate`. |
| 5 | Total | `total` equals `subtotal` + `tax`. |
| 6 | Supplier | `supplier.tax_id` is on the approved list. Skipped, with a note, when no list is given. |
| 7 | Duplicate | This supplier and invoice number are not already in the ledger. Skipped, with a note, when no ledger is given. |
| 8 | Policy | The business policy decision (optional). See [Verdicts](#verdicts). |

Money is checked to within **one cent**. A difference of up to 0.01 passes, so
rounding of unit prices does not cause false rejections. A larger difference
fails.

Amounts are read as numbers. In XML, text that holds a number is accepted.

## Verdicts

| Verdict | When |
|---|---|
| `PASS` | Checks 1 to 7 pass, and the policy decision is `approved` (or there is no policy). |
| `REVIEW` | Checks 1 to 7 pass, and the policy decision is `review` or `approval_required`. |
| `REJECT` | Any of checks 1 to 7 fails, or the policy decision is `rejected`. |

A REJECT from the policy is a decision, not a failure to run. It still gets
the full result.

## Input

An invoice is a JSON object or an XML document, as for
[DOCUMENT_VALIDATOR.md](DOCUMENT_VALIDATOR.md). Each invoice needs these fields:

| Field | Type | Used by |
|---|---|---|
| `invoice_number` | text | Duplicate check |
| `supplier.tax_id` | text | Supplier and duplicate checks |
| `lines` | list of line items | Line item checks |
| `subtotal`, `tax`, `total` | numbers | Checks 3 to 5 |
| `tax_rate` | number, such as `0.15` | Check 4 (optional) |

Each line item has `quantity`, `unit_price` and `amount`. Other fields, such as
`description`, are allowed and ignored.

### Line items in XML

Wrap the lines in `<lines>`, with one `<line>` per item:

```xml
<lines>
  <line>
    <quantity>5</quantity>
    <unit_price>1000.00</unit_price>
    <amount>5000.00</amount>
  </line>
</lines>
```

The wrapper is needed. A single `<line>` inside `<lines>` is read correctly, and
so are several.

## The supplier list

`--suppliers` takes a JSON array of approved tax IDs:

```json
["123-456-789", "987-654-321"]
```

Tax IDs are compared after trimming spaces. Any other JSON is an error (exit `2`).

## The ledger

`--ledger` names a file of JSON lines. Each accepted invoice adds one line:

```json
{"key":"123-456-789|INV-2001"}
```

The key is the supplier tax ID and the invoice number, joined by `|`.

- The file is created on the first run. If it cannot be read, the run stops with exit `1`.
- An invoice that is **not** rejected is added to the ledger straight away, so
  the same invoice twice in one command is caught too.
- A rejected invoice is not added. Fix it and submit it again.
- A REVIEW invoice is added. It was accepted for review, so a second copy is a duplicate.

Keep the ledger with the books. Deleting it lets duplicates through again.

## Policy

`--policy` takes a `tpt-policy` file, as in other products. It is evaluated only
for invoices that pass checks 1 to 7. Its typed values work the same way as in
the data validator: `total gte 5000` matches a total read from XML text.

Templates are in `examples/invoices/invoice.policy.yaml`.

## Command

### `validate <invoices>... --schema FILE [--policy FILE] [--suppliers FILE] [--ledger FILE] [--out DIR]`

| Flag | Meaning |
|---|---|
| `--schema FILE` | Required. Schema for the header fields. |
| `--policy FILE` | Optional. Business policy for invoices that pass checks 1 to 7. |
| `--suppliers FILE` | Optional. Approved supplier tax IDs. |
| `--ledger FILE` | Optional. Processed invoices, for duplicate detection. |
| `--out DIR` | Optional. Writes `<name>.result.json` for each invoice. |

Each invoice prints its verdict, then each check, then any approvers.
The result file holds the verdict, all checks, and the policy evaluation.
It has no timestamp, so repeated runs give the same file.

### `doctor`

Checks the install and runs a built-in self-test on the arithmetic checks.

### Exit codes

| Code | Meaning |
|---|---|
| `0` | Every invoice passed. |
| `20` | At least one needs review, and none were rejected. |
| `30` | At least one was rejected. |
| `1` | A file could not be read or written, or the ledger could not be read or written. The run stops. |
| `2` | Invalid schema, policy or supplier list, an unknown file type, or a bad command line. |

Codes `20` and `30` match `tpt-policy` and `tpt-document`.

## Examples

Sample invoices are in `examples/invoices/`:

| File | Expected verdict | Why |
|---|---|---|
| `invoice-ok.json` | PASS | Every check passes. |
| `invoice-large.xml` | REVIEW | Total 5,750.00 is over 5,000, so finance approval is needed. |
| `invoice-mismatch.json` | REJECT | 10 x 2.50 is 25.00, not 30.00. |
| `invoice-unknown-supplier.json` | REJECT | Supplier 555-000-111 is not approved. |
| `invoice-duplicate.json` | REJECT | Same supplier and number as `invoice-ok.json`, when the ledger holds it. |

## Limits in this version

- No bank or accounting integration. Import and export files only.
- The ledger is a file. Two runs at the same time on the same ledger can both
  miss a duplicate. Run them one after another.
- Tax is checked against one rate per invoice. Mixed-rate invoices need the rate
  to be pre-computed, so the tax field is checked as a total.
- Currency conversion is not checked. The policy can flag foreign currency for review.
- The HTML report and the Docker image are not built yet.
