# Generic purchase-order export

A worked example for checking purchase-order lines exported from a finance system.

**The column names are invented.** This is not any vendor's export format. Rename the
columns in `po.schema.yaml` to match your system's export, or rename your export's
columns to match the schema. No connector is needed. The export is a file, and the
product reads files.

## Files

| File | Purpose |
|---|---|
| `po-lines.csv` | Six sample lines. Two are invalid on purpose. |
| `po.schema.yaml` | Checks each line: types, required fields, ranges, and allowed codes. |
| `po.policy.yaml` | Decides what happens to each valid line: approved, review or approval required. |

## Run it

From this folder:

```
tpt-data validate po-lines.csv --schema po.schema.yaml --policy po.policy.yaml --out results
```

Expected result (exit code 10, which means some records were invalid):

- 4 valid: 2 approved (PO-100001), 2 approval required (PO-100002 needs finance,
  PO-100003 needs manager, because the policy's approval rules outrank its review rule).
- 2 invalid: row 5 has a negative quantity and line total; row 6 has the cost centre
  `CC99`, which is not allowed.

`results/` holds `valid.csv`, `invalid.jsonl`, `errors.txt` and `summary.json`.
Records are checked against the schema first. The policy runs only on valid records.

## Adapting it

1. Export your purchase-order lines to CSV. Keep one line per row.
2. Change `po.schema.yaml`: field names, types, the allowed currencies and cost centres,
   and the `pattern` for the PO number.
3. Change `po.policy.yaml`: the approval thresholds and approvers that your process uses.
4. Run the command above. Keep the schema and policy in version control, and review changes to them.

Dates, duplicates and cross-file checks are not covered here. The data validator supports
uniqueness (`unique: true` in the schema). See [DATA_VALIDATOR](../../docs/DATA_VALIDATOR.md).
