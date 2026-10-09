# TPT Compliance Evidence Processor

Process, check and organise compliance evidence on your own machine. It reads
your evidence files, maps them to the controls they support, checks each file,
and writes an inventory, a manifest of hashes, and reports you can share.

**It does not certify compliance.** Its reports say what evidence exists and
whether it passes the checks you set. Whether an organisation meets a framework,
standard or law is a decision for people, auditors and regulators. Every report
carries this statement.

## Quick start

```text
tpt-evidence process --evidence evidence.yaml --controls controls.yaml --as-of 2026-10-09 --out evidence-out
tpt-evidence verify --manifest evidence-out/manifest.json --root .
tpt-evidence doctor
```

The example in `examples/evidence/` runs as shown:

```text
tpt-evidence process --evidence examples/evidence/evidence.yaml --controls examples/evidence/controls.yaml --as-of 2026-10-09 --out evidence-out
```

## Commands

### `process --evidence FILE --controls FILE --as-of DATE [--out DIR]`

| Flag | Meaning |
|---|---|
| `--evidence FILE` | Required. The evidence index (YAML). File paths in it are relative to this file. |
| `--controls FILE` | Required. The controls to check against (YAML). |
| `--as-of DATE` | Required. The date to measure ages from, as `YYYY-MM-DD`. The tool never reads the clock, so the same inputs give the same output. |
| `--out DIR` | Optional. Folder for the outputs. Created if missing. Default `evidence-out`. |

It writes three files:

| File | Contents |
|---|---|
| `report.json` | The full result: controls, items, findings, summary and manifest. Keys are sorted, so the output is stable. |
| `report.html` | A self-contained page for people to read. No scripts, no external links. |
| `manifest.json` | The SHA-256 and size of every evidence file, and a hash over the entries. |

The console shows a short summary, including every control that is not covered
and every finding.

### `verify --manifest FILE --root DIR`

Checks that the files under `--root` still match the manifest. `--root` is the
folder the manifest paths are relative to: the folder that holds the evidence
index. Prints one line per file, `ok`, `changed` or `missing`, and whether the
manifest's own hash still matches its entries.

### `doctor`

Prints the version and platform, checks that the working folder is writable, and
runs a built-in evidence set through the processor.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | `process`: complete. Every control is covered and no evidence has an error. `verify`: everything matches. |
| `1` | A file could not be read or written. |
| `2` | Invalid index, controls file, manifest or date, or a bad command line. |
| `5` | `doctor` found a problem. |
| `10` | `verify`: a file is missing or changed, or the manifest was edited. |
| `20` | `process`: incomplete. A control has no usable evidence, or an item has an error. |

## Evidence index

```yaml
name: internal-controls-q3-2026
items:
  - id: access-review-q3
    file: files/access-review.csv
    controls: [AC-2]
    collected: 2026-09-30
    owner: IT Security
    sha256: 7f1627...           # optional: the hash when the evidence was collected
    description: Quarterly access review
```

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | Unique within the index. Shown in every report. |
| `file` | yes | Path relative to the index file. It cannot leave that folder (no `..`, no absolute paths). |
| `collected` | yes | When the evidence was collected, as `YYYY-MM-DD`. Must be a real date. |
| `controls` | no | The control IDs this evidence supports. |
| `type` | no | `json`, `csv`, `text`, `log`, `policy` or `document`. Inferred from the extension if omitted: `.json`, `.csv`, `.xml` (document), `.yaml`/`.yml` (policy), `.log`, anything else (text). |
| `owner`, `description` | no | Shown in the reports. |
| `sha256` | no | If given, the file must still have this hash. A different file is an error. |

Unknown fields are refused, so a typo such as `control:` fails instead of being
ignored.

## Controls

```yaml
framework: Internal Controls Baseline
controls:
  - id: AC-2
    title: Account management
    max_age_days: 120
    min_items: 1
```

- `max_age_days` (optional): evidence older than this, measured from `collected`
  to `--as-of`, does not count as usable. A control whose listed evidence is all
  too old is `stale`. See the status list below.
- `min_items` (optional, default 1): how many usable evidence items the control
  needs.

## What is checked

For each evidence file:

| Check | Result when it fails |
|---|---|
| The file can be read | error: cannot read the file |
| The hash matches `sha256` | error: the file has changed since its hash was recorded |
| JSON is valid | error |
| CSV parses (counts data rows) | error |
| Policy files are valid `tpt-policy` policies | error |
| Documents (`.xml`, `.json`) are valid, using the same rules as `tpt-document` | error |
| Text and logs are UTF-8 (counts lines) | error |
| Controls named exist in the controls file | warning |
| `collected` is not after `--as-of` | warning |

An item with an error does not count toward any control. An item with only
warnings still counts.

Controls get one of three statuses:

- `covered`: at least `min_items` usable evidence items.
- `stale`: not enough usable evidence, and every listed item is too old for `max_age_days`.
- `gap`: not enough usable evidence, for any other reason.

The result is `complete` only when every control is covered and no item has an
error. Otherwise it is `incomplete`.

## Hashes and manifest

Every readable evidence file gets a SHA-256 hash and a byte count. The manifest
lists them sorted by path, with a `manifest_sha256` over the entries. Anyone with
the evidence folder and the manifest can run `verify` to see whether anything
changed since the report was made.

## Not in this version

- **Signatures.** Signed reports need `tpt-crypto`, which is not in this
  repository yet. The manifest makes changes visible, but it does not prove who
  made the report.
- **Extra files.** `verify` checks the files the manifest lists. It does not
  report files that were added to the folder.
- **Log analysis.** Logs are checked as text and counted. Their contents are not
  interpreted.
- **Control libraries.** You write the controls file. The tool ships no framework
  text.

## Privacy

Nothing leaves the machine. The reports contain file names, hashes and the
metadata you wrote, not the contents of the evidence. Keep the output folder
where you would keep the evidence.
