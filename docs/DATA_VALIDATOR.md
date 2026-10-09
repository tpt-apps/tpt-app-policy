# TPT Data Validator: command reference

`tpt-data` checks records in CSV, JSON and JSON Lines files against a schema,
and writes the valid and invalid records separately. Optionally it also
evaluates the valid records against a policy.

Written for: people running `tpt-data` on their own files.

## Quick start

```sh
tpt-data validate customers.csv --schema customer.schema.yaml --out results
```

```text
7 records processed

2 valid
5 invalid

Row 2:
  email: 'bad-email' does not match the pattern ^[^@\s]+@[^@\s]+\.[^@\s]+$
...
written to results
```

The schema is described in [SCHEMA_REFERENCE.md](SCHEMA_REFERENCE.md).

## Commands

### `validate <input> --schema FILE [--policy FILE] [--format FORMAT] [--out DIR]`

| Argument or flag | Meaning |
|---|---|
| `<input>` | The file to check. |
| `--schema FILE` | Required. The schema, in YAML. |
| `--policy FILE` | Optional. A policy from `tpt-policy`. Each valid record is evaluated against it. |
| `--format csv\|json\|jsonl` | The input format. Defaults to the file extension: `.csv`, `.json`, `.jsonl` or `.ndjson`. Needed for any other extension. |
| `--out DIR` | Where to write the outputs. Default `tpt-data-out`. Created if missing. |

### `doctor`

Checks the install: version, platform, that the working directory is writable,
and a built-in check that a sample record passes and a bad one fails. Exits `0`
when everything passes.

## Input formats

| Format | Read | Notes |
|---|---|---|
| CSV | One record at a time | First line is the column names. Header names are trimmed. Empty cells are missing. |
| JSON Lines (`.jsonl`) | One record at a time | One JSON object per line. Blank lines are skipped. |
| JSON (`.json`) | The whole file is read into memory | Must be an array of objects. Use JSON Lines for files too large to fit in memory. |

Records are numbered from 1. For CSV, the header is not counted. Blank lines in
JSON Lines are not counted.

A record that cannot be read (a CSV row with the wrong number of cells, a line
that is not JSON, a JSON array item that is not an object) is counted as
invalid, with the reason, and the run continues. A file that cannot be read at
all (for example a JSON file that is not valid JSON) stops the run with exit `3`.

## Outputs

All outputs go in the `--out` folder.

| File | Contents |
|---|---|
| `valid.csv`, `valid.json` or `valid.jsonl` | Valid records, in the input's format. CSV keeps the input's columns, in the same order. |
| `invalid.jsonl` | One JSON object per invalid record: `row`, `errors` (each with `field` and `reason`), and the `record` as read. |
| `errors.txt` | The error report, one block per invalid record. |
| `summary.json` | Counts, the schema and policy versions, and the input's SHA-256. Contains no timestamp, so the same run gives the same file. |

Valid records are written exactly as read, so text values stay text. A policy
decision does not change the record.

Records that pass the schema and the duplicate check are valid. If a policy is
given, the decision for each valid record is counted in `summary.json` under
`decisions`. The records themselves are not changed or moved.

### `summary.json`

```json
{
  "tool": "tpt-data",
  "tool_version": "2026.1.0",
  "schema": { "name": "customer", "version": "1.0.0", "engine_version": "2026.1.0" },
  "policy": { "name": "credit-review", "version": "1.0.0" },
  "input": { "file": "customers.csv", "format": "csv", "sha256": "..." },
  "result": {
    "processed": 7,
    "valid": 2,
    "invalid": 5,
    "errors_by_field": { "email": 2, "country": 1, "customer_id": 1, "credit_limit": 1 },
    "decisions": { "approved": 1, "review": 1 }
  }
}
```

`errors_by_field` counts errors, not records. One record can have several errors.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Every record is valid. |
| `1` | A file could not be read or written. |
| `2` | Invalid schema or policy, or a bad command line. |
| `3` | The input could not be read as a whole, for example a broken JSON array. |
| `10` | One or more records are invalid. The outputs are still written. |

These codes are specific to `tpt-data`. They are not the same as `tpt-policy`'s
codes, which use `10` for `approval_required`.

## Privacy

The console summary shows counts and the first 20 error blocks. It does not
print record values beyond what the error reason quotes, such as the value that
failed a pattern. The output files do contain the records, so treat them the same
way as the input.

Nothing is sent over the network.

## Limits in this version

- Input is CSV, JSON or JSON Lines. XML is not supported yet.
- The HTML report is not built yet. The text report and `summary.json` are.
- The JSON array format loads the whole file. Use JSON Lines for large files.
- Only the `unique` schema rule works across records.
- A policy is one file, and it is evaluated per record. Policies that need several records at once are not supported.
