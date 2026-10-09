# Integration

TPT App Policy has no connectors to ERP or accounting systems. That is by
design. Your system exports data as JSON, sends it to the policy, and acts on
the decision. This page shows the pattern.

Written for: engineers wiring the policy into an existing system.

## The pattern

```text
your system  --(JSON)-->  tpt-policy  --(decision JSON)-->  your system
```

1. Build a JSON object from the record you want to check.
2. Send it to the policy, as a CLI call or an HTTP request.
3. Read `decision`, `approvers` and `requirements`.
4. Act on them in your system.

Use the shape of your own records as the input. The policy reads whatever
fields its rules name, so keep the field names in one place.

## Option A: HTTP

Start the service on the machine that has the policy file:

```sh
export TPT_POLICY_TOKEN="$(openssl rand -hex 24)"
tpt-policy serve policies/purchase.yaml --listen 127.0.0.1:8080
```

Send a record:

```sh
curl -sS -X POST http://127.0.0.1:8080/v1/evaluate \
  -H "Authorization: Bearer $TPT_POLICY_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"purchase": {"amount": 6000, "vendor": "Acme"}}'
```

The response is the decision JSON. Decide in your code what to do with it:

```python
import json, os, urllib.request

def check_purchase(record: dict) -> dict:
    req = urllib.request.Request(
        "http://127.0.0.1:8080/v1/evaluate",
        data=json.dumps(record).encode(),
        headers={
            "Authorization": "Bearer " + os.environ["TPT_POLICY_TOKEN"],
            "Content-Type": "application/json",
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=5) as resp:
        return json.load(resp)

result = check_purchase({"purchase": {"amount": 6000, "vendor": "Acme"}})
if result["decision"] == "approval_required":
    print("needs", ", ".join(result["approvers"]))
```

Notes:

- The service is single-threaded. Send requests one at a time, or run it
  behind a queue. It is for internal use, not the public internet.
- Keep it on `127.0.0.1` unless you need another machine to reach it. If you
  bind to another address, set a token.
- Request bodies over 1 MB are refused with `413`, and a body that is not JSON
  gets `400`.

## Option B: command line

For batch jobs, pipe JSON through `run`:

```sh
cat purchases.json | tpt-policy run policies/purchase.yaml > decisions.json
```

Or check one file and branch on the exit code:

```sh
tpt-policy check policies/purchase.yaml purchase-123.json > decision.json
case $? in
  0)  echo "approved" ;;
  10) echo "approval required" ;;
  20) echo "review" ;;
  30) echo "rejected" ;;
  *)  echo "policy error" ;;
esac
```

Exit codes `0`, `10`, `20` and `30` are decisions. Anything else is an error
(see [CLI_REFERENCE.md](CLI_REFERENCE.md)).

## Export and import files

If your system exports CSV, convert each row to a JSON object first, then
evaluate it. The policy engine itself takes JSON only. Spreadsheet and CSV
handling is planned for the Data Validator product.

## Handling the result

| Decision | Suggested handling |
|---|---|
| `approved` | Proceed. |
| `review` | Queue for a person. Show `failed_rules` so they know why. |
| `approval_required` | Route to `approvers`. Show `requirements`. |
| `rejected` | Stop. Keep the record and the `matched_rules`. |

Store the whole decision JSON next to the record. It includes the policy and
engine versions, so you can explain the decision later.

## Errors

- A policy that does not load fails at start-up, so bad policies are caught
  before any record is checked.
- In HTTP, errors come back as `{"error": {"what", "why", "fix"}}`.
- Treat any non-`200` response as a failure to decide, not as an approval.
