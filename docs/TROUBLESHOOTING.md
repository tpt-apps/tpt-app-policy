# Troubleshooting

Start with `tpt-policy doctor`. It checks the install and prints what failed.

## Install and download

### Checksum does not match

Compare the archive with the `SHA256SUMS` file. The commands are in
[FAQ.md](FAQ.md#downloads-and-installation).

- Download the archive again. A partial download is the most common cause.
- Make sure you are comparing the archive you downloaded, not an older one with
  the same name.
- If the hash still differs after a fresh download, email support with the
  hash you got and the one in `SHA256SUMS`. Do not run the binary.

### Windows shows a SmartScreen warning

Windows says "Windows protected your PC" or "unknown publisher". The program is
not digitally signed yet, so Windows warns about it. Only continue if the
checksum matches.

1. Check the checksum first (see above).
2. Unpack the archive. Right-click `bin\tpt-policy.exe`, choose **Properties**,
   and tick **Unblock** if it is shown. Click **OK**.
3. Run `tpt-policy --version` in a terminal. If SmartScreen appears, click
   **More info**, then **Run anyway**.

If the warning does not go away, or you are not sure about the file, email
support before you run it.

### `tpt-policy` is not recognised

The folder that holds `tpt-policy` is not on your `PATH`.

- Windows: run the program by its full path first, for example
  `C:\Tools\tpt-policy\bin\tpt-policy.exe --version`. Then add that folder
  to your `PATH` in **Environment Variables**, and open a new terminal.
- Linux: run `echo $PATH` to see the folders, then copy the binary into one of
  them, such as `~/.local/bin`, and open a new shell.

### Linux says "Permission denied"

The binary is not marked as executable after unpacking:

```sh
chmod +x bin/tpt-policy
```

## Policy does not load

**`error: ... at <path>`** from `validate`, `check`, `test` or `serve`.

- Read the **what**, **where**, **why** and **fix** lines. The **where** is a
  key path such as `rules[0].when.amount.greater_than`. Line numbers are given
  only for YAML syntax errors.
- Operators are spelled `gt`, `gte`, `lt`, `lte`, not `greater_than`. The full list is in
  [POLICY_REFERENCE.md](POLICY_REFERENCE.md#operators).
- Indentation errors in YAML are the most common cause. Use spaces, not tabs.

Exit code `2` means the policy is invalid.

## Input is rejected

**`error: input is not valid JSON`**, exit code `3`.

- The message gives the line of the JSON error. Check for trailing commas and
  unquoted keys.
- If you pipe data in, check what the sender actually produced:
  `cat input.json | head -c 200`.

**`error: cannot read input`**, exit code `1`: the file path is wrong, or
the file is locked.

## A decision is not what I expected

1. Run `tpt-policy explain` instead of `check`. It lists the matched rules and
   the failed rules with the first failed check.
2. Check the field path. A missing field makes its check fail. In `explain`
   output this appears as `<field> is missing`.
3. Check the type. `"1000"` (text) does not equal `1000` (number) when
   compared with `equals`. Send the number as a JSON number.
4. Check the default. A result of `review` with no `matched_rules` means no
   rule matched and `default_decision` applied.

## Tests fail

**`FAIL <name>`** from `tpt-policy test`, exit code `4`.

- The message shows the expected and actual values.
- If you changed a rule, check whether the test's `expect` still describes the
  behaviour you want. The test is the record of intent, so change it only on
  purpose.

## Server will not start

**`error: cannot serve on <address>`**

- The port may be in use. Pick another with `--listen 127.0.0.1:8081`.
- The address must be `host:port`, for example `127.0.0.1:8080`.

**`warning: listening on ... without a token`**

- Set `TPT_POLICY_TOKEN` and send `Authorization: Bearer <token>`, or listen on
  `127.0.0.1` only.

## Server returns an error

| Status | Cause | Fix |
|---|---|---|
| `401` | Missing or wrong bearer token. | Check the `Authorization` header and `TPT_POLICY_TOKEN`. |
| `400` | Body is not valid JSON. | Send a JSON object with `Content-Type: application/json`. |
| `413` | Body over 1 MB. | Send less data, or split the record. |
| `404` | Unknown path. | The evaluation endpoint is `POST /v1/evaluate`. |
| `405` | Wrong method, such as `GET /v1/evaluate`. | Use `POST`. |

Error bodies look like `{"error": {"what": ..., "why": ..., "fix": ...}}`.

## `doctor` fails

- `working directory ... is not writable`: run from a folder you can write to, or
  fix the permissions.
- `self-test`: this means the binary itself is broken. Reinstall from a clean
  build (`cargo build --release`) and report the output.

## Still stuck

Collect this and send it to support (see [SECURITY.md](SECURITY.md) for what is
safe to share):

- `tpt-policy --version` and `tpt-policy doctor` output
- the command you ran, and its full output
- the policy file, with any customer data removed

Input records may contain personal or financial data. Remove or replace them
before sharing.
