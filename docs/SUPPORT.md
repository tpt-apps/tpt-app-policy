# Get help

Most questions are answered by the docs. Start here, then contact us if you
are still stuck.

Written for: buyers and users of TPT App Policy.

## Find your answer

| If you want to... | Read |
|---|---|
| Install it on Windows or Linux | [INSTALL.md](INSTALL.md) |
| Run your first rule in five minutes | [QUICKSTART.md](QUICKSTART.md) |
| Understand how decisions are made | [CONCEPTS.md](CONCEPTS.md) |
| Look up a command or exit code | [CLI_REFERENCE.md](CLI_REFERENCE.md) |
| Write a rule, or find an operator | [POLICY_REFERENCE.md](POLICY_REFERENCE.md) |
| Call it from another system | [INTEGRATION.md](INTEGRATION.md) |
| Fix an error message or unexpected result | [TROUBLESHOOTING.md](TROUBLESHOOTING.md) |
| Check the questions people ask most | [FAQ.md](FAQ.md) |
| Know what data leaves your machine (none) | [SECURITY.md](SECURITY.md) |

Every error message also says what to do. Read its **why** and **fix** lines
first.

## Try these before you write in

1. Run `tpt-policy --version` to confirm the version you have.
2. Run `tpt-policy doctor`. It checks the install and a built-in example.
3. Run `tpt-policy validate <policy>` on the file that gives you trouble.
4. Run `tpt-policy explain <policy> <input>`. It shows every rule that matched
   and every rule that failed, and why.
5. Search the [FAQ](FAQ.md) and [TROUBLESHOOTING](TROUBLESHOOTING.md) for the
   exact error text.

Most problems are solved by steps 2 to 4.

## Contact support

Email **support@tptsolutions.co.nz**.

Support is provided on a best-effort basis. We read every message, but we do
not promise a response time or a fix date.
Support covers the product as released. It does not cover building your
business rules for you, or changing the software for your systems.

Before you email, read the scope in [What support covers](#what-support-covers).

### What to send

Send these together. Requests without them take longer to answer, and often
need a reply to ask for them.

1. Your product version, from `tpt-policy --version`.
2. Your operating system and version (for example Windows 11 or Ubuntu 24.04).
3. The output of `tpt-policy doctor`.
4. The command you ran, exactly as typed.
5. The full output, including any error with its **what**, **why** and **fix**
   lines.
6. The smallest policy file and input that show the problem. Replace any real
   names, amounts or personal details with made-up values first.
7. What you expected to happen, and what happened instead.

Use the template below. Copy it into the email.

```text
Subject: [TPT App Policy] short description of the problem

Version:          (output of tpt-policy --version)
Operating system: (for example Windows 11, Ubuntu 24.04)
Purchase:         (optional: Gumroad order number)

Command:
    (the exact command)

Output:
    (the full output)

doctor output:
    (output of tpt-policy doctor)

Minimal policy and input:
    (sanitised files, or attach them)

Expected:
    (what you expected)

Actual:
    (what happened)
```

Do not send real customer, employee or payment data. Use made-up values that
produce the same behaviour.

### Bugs

A bug is a case where the software does not do what the documentation says. If
you can reproduce it, use the template above and add "Steps to reproduce". The
steps should start from a clean folder and end at the failure.

If you are not sure whether something is a bug or a misunderstanding, send it
anyway. We will tell you which it is.

## What support covers

| Covered | Not covered |
|---|---|
| Installing and running the released version | Writing your business rules for you |
| Errors that the docs describe | Changing the software for your systems |
| Bugs in the released tool | Integrating with ERP, accounting or other systems |
| Questions about the docs | Compliance, legal or audit advice |
| Checksum and download problems | Custom features or forks |

If something is outside the scope, we will say so. Where we can, we will point
you to the part of the docs that helps.

## Source code and issues

The licence terms in the bundle say what you may do with the software. If you
build or change it yourself, you do so without support from us.

Bug reports about the released version are welcome by email. Do not put
customer data in a public issue.

## Security problems

Do not describe a security problem in a public place. Email
**support@tptsolutions.co.nz** with the subject line "SECURITY" and wait for a
reply before you share the details. See [SECURITY.md](SECURITY.md).
