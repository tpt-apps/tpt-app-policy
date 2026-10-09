# Privacy

Written for: buyers and users of TPT App Policy, and anyone checking whether it
can be used with customer or financial data.

Your data stays on your infrastructure. TPT App Policy runs entirely on the
machine or container you install it on.

## What the software does

- **No data is sent to TPT.** The command-line tool and the `serve` HTTP service
  make no outbound network calls. The software has no telemetry, no crash
  reporting, no update check, and no licence or activation check.
- **No account is needed.** Nothing asks you to sign in or register.
- **Input is not stored.** Each evaluation reads your input, returns a decision,
  and discards the input. There is no database and no request log.
- **Output stays where you send it.** Results go to the terminal, a file you name
  with `--output`, or the caller of the HTTP service.

How we know: the code has no outbound client library, and its only network code
accepts connections on the address you give it (`127.0.0.1` by default). You can
check this yourself by running `serve` and watching its network traffic with
your usual tools. See [SECURITY.md](SECURITY.md).

## What you should handle with care

- **Results can contain your input values.** A failed rule can show the value
  that failed a check, for example `(actual: 500)`. Treat the output as sensitive
  if the input is.
- **Policy files are code.** They hold your business rules. Keep them under
  version control and limit who can edit them.
- **Your own systems send the data.** If you call the HTTP service from an ERP,
  an accounting system or a script, that system is what moves the data. TPT App
  Policy does not see it.

## Buyer information

TPT App Policy is sold through Gumroad. Gumroad collects buyer details (such as
name, email and payment information) to process the sale. That is handled under
Gumroad's own terms and privacy policy. The software does not receive or store
any of it.

## Support requests

When you email **support@tptsolutions.co.nz**, the message is the only thing we
receive. Do not include real customer, employee or payment data. Use made-up
values that show the same behaviour. See [SUPPORT.md](SUPPORT.md).

## Changes

If a future version adds any network feature, such as updates or telemetry, this
page will change, and the release notes will say so.
