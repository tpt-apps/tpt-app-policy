# Schema reference

A schema describes the records in a file: which fields exist, what type each
must have, and what rules apply. TPT Data Validator checks each record against
the schema.

Written for: people writing schemas for `tpt-data validate`.

## Example

`customer.schema.yaml`:

```yaml
schema: customer
version: "1.0.0"
fields:
  customer_id:
    type: string
    required: true
    unique: true
  email:
    type: string
    required: true
    pattern: '^[^@\s]+@[^@\s]+\.[^@\s]+$'
  country:
    type: string
    required: true
    enum: [NZ, AU, US, GB]
  credit_limit:
    type: number
    min: 0
    max: 100000
  address.postcode:
    type: string
```

## Top-level keys

| Key | Required | Meaning |
|---|---|---|
| `schema` | Yes | Name of the schema. Appears in the summary report. |
| `version` | No | Schema version, such as `"1.0.0"`. Defaults to `unversioned`. Recorded in the summary. |
| `fields` | Yes | Map of field name to field rules. At least one field. |

Keys are read in the order they appear, and errors are reported in that order.

## Field names

A field name is a path into the record, with dots between levels:

- `email` is the top-level `email` column or key.
- `address.postcode` is `postcode` inside `address`.

For CSV, a column header with dots becomes the same nested path. So the CSV
column `address.postcode` matches the schema field `address.postcode`. For JSON,
the record has to be nested.

A field name cannot be empty, and it cannot have an empty part, such as `a..b`
or `a.`.

## Field keys

| Key | Applies to | Meaning |
|---|---|---|
| `type` | All | Required. One of `string`, `integer`, `number`, `boolean`. |
| `required` | All | `true` means the field must be present and not null. An empty CSV cell counts as missing. Default `false`. |
| `unique` | All | Each value may appear only once across the run. Empty and missing values are not compared. Default `false`. |
| `enum` | All | List of allowed values. The value must be one of them. Must not be empty. |
| `pattern` | `string` only | Regular expression the text must match. Uses Rust's `regex` syntax, which has no backreferences or lookaround. |
| `min` | `integer`, `number` | Smallest allowed value, inclusive. |
| `max` | `integer`, `number` | Largest allowed value, inclusive. `min` must not be greater than `max`. |

Any other key is an error. This catches typos such as `require: true`.

## Types

| Type | Accepts (JSON input) | Accepts (CSV input) |
|---|---|---|
| `string` | Text | Any text |
| `integer` | Whole JSON numbers | Text of a whole number, such as `42` or ` -7 ` |
| `number` | JSON numbers | Text of a number, such as `2.5` or `1e3`. `NaN` and infinity are refused. |
| `boolean` | `true` or `false` | `true` or `false`, in any case |

JSON input must already have the right JSON type. A JSON `"42"` fails an
`integer` field. CSV cells are always text, so they are read as the field's type.

## Errors in the schema file

A bad schema stops the run before any record is read. The message names the
field and the key, and says how to fix it:

```text
error: unknown type at fields.email.type
  why: 'date' is not a type
  fix: use one of: string, integer, number, boolean
  file: customer.schema.yaml
```

Checks made when the schema is read:

- the YAML is valid, with the line and column of any error
- `schema` is not empty, and `fields` has at least one entry
- every field name is a valid path
- every `type` is one of the four types
- `pattern` is only used on `string` fields, and it compiles
- `min` and `max` are only used on numeric fields, and `min` is not greater than `max`
- `enum` is not empty, and holds only plain values, not lists or objects
- no unknown keys

## Reading the results

Each invalid record gets one or more errors. An error has a field and a reason:

```text
Row 182:
  email: 'bad' does not match the pattern ^[^@\s]+@[^@\s]+\.[^@\s]+$
  country: 'FR' is not one of ['NZ', 'AU', 'US', 'GB']
```

Row numbers start at 1 and do not count the CSV header.

A record that cannot be read at all (a broken CSV row, a JSON Lines line that is
not JSON) is reported against the field `(record)`.

## Limits

- `pattern` uses the `regex` crate. Lookahead and backreferences are not
  supported, and patterns are matched anywhere in the text unless anchored with
  `^` and `$`.
- `unique` keeps every value seen so far, in memory. For a very large file with
  many unique values, that can use a lot of memory.
- Schemas describe one record at a time. Checks across records, other than
  `unique`, are not supported yet.
