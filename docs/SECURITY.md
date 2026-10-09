# Security

This document describes how TPT App Policy handles data and what you need to
configure. It is not a certification or an audit.

## What it does with your data

- **Nothing leaves the machine.** The CLI and the HTTP service make no outbound
  network calls. There is no telemetry, no account, and no licence check.
- **Input is not stored.** Each evaluation reads the input, returns the decision,
  and discards it. There is no database.
- **Results can show input values.** The `failed_rules` reasons include the
  actual value that failed a check, for example `(actual: 500)`. The output
  goes only to whoever runs the command or calls the service. Treat it as
  sensitive if your input is. The service writes no request logs of its own.

Confirm the first point yourself: run the service and watch its network traffic
with your usual tools. The binary has no code that opens an outbound connection.

## HTTP service

The `serve` command is for internal use on a trusted machine.

- **Bind to localhost** (the default, `127.0.0.1`) unless another machine needs
  to reach it.
- **Set a bearer token** with the `TPT_POLICY_TOKEN` environment variable. Using
  the environment variable keeps the value out of shell history and the process
  list. The token is compared on each `POST /v1/evaluate` request.
- **Use TLS in front of it** if traffic crosses a network. The service speaks
  plain HTTP. Put it behind a reverse proxy that terminates TLS, or keep it on
  localhost.
- **Tokens are compared in constant time**, so the check does not leak how much
  of a token matched.
- **Expect one request at a time.** The server is single-threaded. Do not expose
  it directly to the internet.
- **Requests are limited to 1 MB**, which limits the cost of one request.

## Policy files

- A policy file is code. Anyone who can edit it can change the decisions. Keep it
  under version control and review changes like code.
- A policy cannot run code, read files, or make network calls. Rules only compare
  JSON values.

## Reporting a vulnerability

Email **support@tptsolutions.co.nz** with the subject line `SECURITY`. Do not
put the details in a public issue. See [SUPPORT.md](SUPPORT.md).

## Known limits

- No signed release bundles yet. Verify the archive against `SHA256SUMS`.
- The HTTP server has no rate limiting.
- There is one bearer token per server. There are no per-user accounts.
