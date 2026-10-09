# FAQ and tips

Short answers to the questions people ask most, plus tips that save time. For
errors, see [TROUBLESHOOTING.md](TROUBLESHOOTING.md).

Written for: buyers and users of TPT App Policy.

## About the product

**Does it need internet access or an account?**
No. It runs on your machine, offline. There is no licence key, no activation,
and no phone-home. See [SECURITY.md](SECURITY.md).

**What am I paying for?**
The tested release bundle for the 2026 release, the documentation, the examples
and the help. The bundle includes the licence texts that apply to the software.

**Can I use it commercially?**
The licence terms are in `LICENSE-MIT` and `LICENSE-APACHE` in the bundle. Read
them for what you may do with the software.

**Can I put it inside my own product?**
Check the licence terms in the bundle. Keep the licence notices in anything you
distribute.

**Does it certify that my process complies with a law or standard?**
No. It applies the rules you write, and reports what they say. Whether those
rules meet a regulation is your decision, with your advisers.

**Does it have a graphical interface?**
No. It is a command-line tool and a local HTTP service. Your own system or
a script calls it.

**What operating systems does it run on?**
Windows 10 and 11 (64-bit), and Linux (64-bit). It is a single binary, with no
runtime to install.

**Which version is this?**
Versions are named by year: `2026.1.0` is the first release of 2026. Later
releases in the same year get `2026.1.1`, `2026.2.0` and so on. A new year
starts a new line, such as `2027.1.0`. Run `tpt-policy --version` to see yours.

**Does a new version change my existing policies?**
Your policy files are yours. Each release notes any change to the policy
format in [CHANGELOG.md](../CHANGELOG.md). Read that before you upgrade.

## Using it

**Where do I start?**
[QUICKSTART.md](QUICKSTART.md). It takes about five minutes.

**Why did I get `review` when I expected `approved`?**
No rule matched, so the policy's default decision applied. The default is
`review` on purpose: a gap in the rules goes to a person. Run
`tpt-policy explain` to see which rules were checked and why each one failed.
Add a rule for the case, or set `default_decision` if you want a different
default.

**Can one input match several rules?**
Yes. The most severe decision wins, in this order: `rejected`,
`approval_required`, `review`, `approved`. Approvers and requirements from every
matching rule are combined.

**How do I check a rule before I rely on it?**
Add a `tests:` block to the policy file, then run `tpt-policy test`. Keep the
tests next to the rules, and run them in your version control or CI.

**Why does a missing field fail my rule?**
A check on a missing or null field fails, so a rule cannot pass by accident
because data is absent. To match on absence, use `exists: false`.

**Is `"1000"` the same as `1000`?**
No. Text and numbers are different. Send numbers as JSON numbers, without
quotes.

**Can I use regular expressions?**
Not yet. The `matches` operator is planned.

**Can I read the policy from a database or a web page?**
No. Policies are local YAML files. Your own system can load them and pass them
to the tool.

## Tips

**Keep one policy per decision.** A file called `purchase.yaml` that answers
one question is easier to test and review than one large file for everything.

**Use descriptive rule IDs and descriptions.** The ID appears in
`matched_rules`, and the description appears in `explain`. Both show up in
audit trails, so write them for the person who reads the record later.

**Write a test for every rule.** Two tests per rule is a good start: one input
that should match, and one that should not. Run `tpt-policy test` after every
change.

**Put the default at the end, deliberately.** If you want a catch-all,
write a rule that matches anything and sets the decision you want. Do not
depend on the default for cases you care about.

**Check the exit code in scripts.** Exit codes `0`, `10`, `20` and `30` are
decisions. Use them to branch in shell scripts, and read the JSON for detail.

**Use `run` for pipelines.** `cat record.json | tpt-policy run policy.yaml`
reads from stdin and writes JSON to stdout. It is the simplest way to
connect it to other tools.

**Keep the service on localhost.** Start `serve` on `127.0.0.1` unless another
machine must reach it. If it must, set `TPT_POLICY_TOKEN`, and put a
TLS-terminating proxy in front.

**Set the token in the environment, not on the command line.**
`export TPT_POLICY_TOKEN=...` keeps it out of shell history.

**Save the whole decision next to the record.** It includes the policy and
engine versions, so you can explain an old decision after the policy has
changed.

**Check the download before you unpack it.** See the next section.

## Downloads and installation

**How do I check the download is complete and unchanged?**
Compare the archive with the `SHA256SUMS` file.

On Linux:

```sh
sha256sum -c SHA256SUMS
```

On Windows (PowerShell), compare the hash of the archive with the line for
it in `SHA256SUMS`:

```powershell
Get-FileHash .\tpt-app-policy-2026.1.0-windows-x64.tar.gz -Algorithm SHA256
```

The two hashes must match exactly. If they do not, download again. If they
still differ, contact support.

**Why does Windows say the app is from an unknown publisher?**
The Windows program is not digitally signed yet. Windows SmartScreen shows a
warning for unsigned programs. This is expected, and it does not mean the
program is harmful. Check the checksum first, then follow the steps in
[TROUBLESHOOTING.md](TROUBLESHOOTING.md#windows-shows-a-smartscreen-warning).

**Where should I put the program?**
Anywhere on your `PATH`. On Linux, `/usr/local/bin` or `~/.local/bin` work
well. On Windows, a folder such as `C:\Tools\tpt-policy` that you add to your
`PATH` works well.

**Do I get new versions?**
Your purchase covers the 2026 release. A new release each year is a new purchase. If you use a new version, replace the
`tpt-policy` binary, keep your policy files, run `tpt-policy doctor` and
`tpt-policy test`, then read the changelog for format changes.

**How do I remove it?**
Delete the binary and the folder you unpacked it into. The tool keeps no
database, no settings file and no registry entries. `doctor` creates a small
test file in the current folder to check it can write there, then deletes it.

## Money and access

**Can I get a receipt or invoice?**
Gumroad sends a receipt for each purchase. If you need a tax invoice in a
particular name, email support with your order number and say what it is for.

**How many machines can I install it on?**
The licence terms in the bundle decide what you may do with the software. Your
purchase covers the 2026 bundle and its docs.

**Can I share the program with someone else?**
The licence terms in the bundle say what you may share. Nothing in the program
checks who is running it.

**Do you track who bought it?**
Gumroad keeps the sale record, because it is the seller. The software does not
identify buyers. It has no licence keys and no activation.

**Can I get a refund?**
Yes. Email support@tptsolutions.co.nz within 14 days of purchase for a full
refund, no questions asked.

**My download link stopped working.**
Log in to Gumroad and open your purchase from there. If it still does not work,
email support. Your order number helps, but it is not required.
