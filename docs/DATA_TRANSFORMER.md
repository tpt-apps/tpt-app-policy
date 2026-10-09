# TPT Data Transformer

Reshape CSV, JSON and JSON Lines records with a pipeline you write once and run
many times. Clean names and emails, convert money columns, combine or split
fields, fill defaults, filter rows, and add calculated fields. Runs offline.

## Quick start

```text
tpt-transform run customers.csv --pipeline customers.pipeline.yaml --out results
tpt-transform check --pipeline customers.pipeline.yaml
tpt-transform doctor
```

The examples in `examples/transform/` run as shown:

```text
tpt-transform run examples/transform/customers.csv --pipeline examples/transform/customers.pipeline.yaml --out results
```

## Commands

### `run <input> --pipeline FILE [--format FORMAT] [--to FORMAT] [--out DIR]`

| Argument or flag | Meaning |
|---|---|
| `<input>` | Input file: `.csv`, `.json` (an array) or `.jsonl`. |
| `--pipeline FILE` | Required. The pipeline, in YAML. |
| `--format FORMAT` | Optional. `csv`, `json` or `jsonl`. Defaults to the file extension. |
| `--to FORMAT` | Optional. Output format. Defaults to the input format. |
| `--out DIR` | Optional. Output folder, created if missing. Default `tpt-transform-out`. |

Each run writes three files into the output folder:

| File | Contents |
|---|---|
| `transformed.<format>` | Every record that made it through the pipeline. |
| `rejected.jsonl` | One line per rejected record: `row`, `reason` and the original `input`. |
| `summary.json` | Counts, the pipeline name and version, and the SHA-256 of the input. |

The console shows the counts, and the first 20 rejections.

### `check --pipeline FILE`

Checks the pipeline file and prints `pipeline ok: <name> (<steps> steps)`. Use it
before a run, or in a build, to catch a mistake early.

### `doctor`

Prints the version and platform, checks that the working folder is writable, and
runs a built-in pipeline over one record.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Every record was transformed. Filtered records are not errors. |
| `1` | A file could not be read or written. |
| `2` | Invalid pipeline, unknown file type, or a bad command line. |
| `3` | The input could not be read as a whole, for example a broken JSON array. |
| `10` | One or more records were rejected. The outputs were still written. |

## Records, rejections and filters

- A **rejected** record is one a step could not process, such as text that is
  not a number in a `to_number` step. It goes to `rejected.jsonl`, and the run
  carries on.
- A **filtered** record is one a `filter` step does not keep. It is left out of
  the output and counted in the summary, but it is not an error.
- A line in a file that cannot be read at all is also rejected, with the reason
  from the reader.

Each step runs in order over each record. The reason for a rejection names the
step, for example `step 5 (to_number): field 'amount': 'abc' is not a number`.

## Pipeline file

A pipeline file has an optional `name` and `version`, and a `pipeline` list.
Each step is one key naming the step, followed by its settings:

```yaml
name: customer-clean
version: 1.0.0
pipeline:
  - trim:
      fields: ["*"]
  - lowercase:
      fields: [email]
  - rename:
      first_name: firstName
      last_name: lastName
  - combine:
      fields: [firstName, lastName]
      into: fullName
      separator: " "
```

Unknown step names and unknown settings are errors. The file is checked before
any record is read.

### Field names

A field name is a path. `address.city` is the `city` key inside `address`. CSV
columns with dots, such as `address.city`, are read the same way. A field that is
not in a record is skipped by most steps. Empty CSV cells are null.

The name `*` means every text value in the record. It is allowed only in `trim`,
`lowercase`, `uppercase` and `title_case`.

## Steps

### Text

| Step | Settings | What it does |
|---|---|---|
| `trim` | `fields` | Removes spaces at both ends of text. |
| `lowercase` | `fields` | Lower case text. |
| `uppercase` | `fields` | Upper case text. |
| `title_case` | `fields` | Capitalises each word, and lower cases the rest of it. |

### Fields

| Step | Settings | What it does |
|---|---|---|
| `rename` | `old: new` pairs | Moves each value to its new name. A missing field is skipped. |
| `delete` | `fields` | Removes the fields. A missing field is skipped. |
| `select` | `fields` | Keeps only these fields. Everything else is removed. |
| `defaults` | `field: value` pairs | Sets a value when the field is missing, null or empty text. |

### Types

| Step | Settings | What it does |
|---|---|---|
| `to_number` | `fields` | Converts text to a number. Text that is not a number rejects the record. Blank text becomes null. Thousands separators such as `1,200` are not accepted. |
| `to_text` | `fields` | Converts numbers and booleans to text. |
| `to_bool` | `fields` | Converts `true`, `yes`, `y`, `1` to true and `false`, `no`, `n`, `0` to false (not case sensitive). Anything else rejects the record. |

### Values

| Step | Settings | What it does |
|---|---|---|
| `map` | `field`, `values` | Replaces a value with one from the table, matched as text. Values not in the table are left alone. |
| `conditional` | `when`, `set` | Sets the `set` fields when the `when` condition holds. |

### Combining and splitting

| Step | Settings | What it does |
|---|---|---|
| `combine` | `fields`, `into`, `separator` (default one space), `keep_sources` (default false) | Joins text values into the `into` field. Missing and null fields count as empty text. The source fields are removed unless `keep_sources` is true. |
| `split` | `field`, `into`, `separator` | Splits one text value into the `into` fields. Parts are trimmed, and empty parts become null. Missing parts are null. More parts than `into` names rejects the record. The source field is removed unless it is one of the `into` names. |

### Filtering

| Step | Settings | What it does |
|---|---|---|
| `filter` | a condition | Keeps records that match. Other records are filtered out (counted, not rejected). |

### Calculated fields

| Step | Settings | What it does |
|---|---|---|
| `calculate` | `into`, `operation`, `fields`, `decimals` (optional, up to 10) | Arithmetic on numeric fields, into the `into` field. |

Operations:

- `add` and `multiply` take one or more fields.
- `subtract` and `divide` take exactly two fields: the first minus or divided by the second.
- Division by zero rejects the record.
- The fields must be numbers. A text field is rejected with a message to add a
  `to_number` step before it.
- Whole-number results are written as whole numbers. `decimals` rounds the result.

## Conditions

A condition names a `field` and exactly one of:

- `equals`: the field has this value.
- `not_equals`: the field does not have this value.
- `one_of`: the field is one of these values.

Values are compared as text. A field that is missing or null is empty text. This
means the number `0` in a YAML file matches a CSV cell `0`.

## Limits in this version

- The whole input is read into memory before the output is written. Large files
  need enough memory for their records. Streaming is planned for a later version.
- JSON input must be a JSON array, or JSON Lines. Input is not streamed as an
  array.
- Joins, lookups and reference tables are not in this version.
- Field names are paths through objects only. Items inside a list cannot be
  addressed by position, so a list is kept or changed as a whole.
- Text is compared and converted as text. Locale-specific number formats, such as
  `1.200,50`, are not understood.

## Privacy

Nothing leaves the machine. The output files, including `rejected.jsonl`, contain
your records, so keep them where you would keep the input.
