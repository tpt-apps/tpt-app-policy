# Gumroad listing: TPT App Policy

Everything needed to set up and run the Gumroad product. **CHECK** marks facts
about Gumroad or the law that change or depend on where you sell, so confirm
them before you rely on them.

Written for: the seller (TPT Solutions) setting up the listing. Sections 3 to 5
are the buyer-facing copy.

## 0. The model

- **One Gumroad product per release year.** The first is **TPT App Policy 2026**.
  The 2027 release becomes **TPT App Policy 2027**, a new listing with its own
  price. The 2026 listing stays up for people who buy it.
- **Each purchase covers that year's release** (2026.x). A new year is a new
  purchase. No updates are promised within a year.
- **14-day refund, no questions asked.** Any buyer can ask for a refund within
  14 days of purchase, and you give it. This matches the EU and UK 14-day
  withdrawal right, so one policy works in every market. You accept some abuse as
  the cost.
- **The Gumroad product is the release bundle.** It is the tested, checksummed
  binary, the documentation and the help. The listing describes that product.

On licensing, the listing says nothing beyond section 4. The licence texts ship
in the bundle, and the repository is public, so buyers can see that the software
is open source. Do not describe the product as proprietary, closed source, or
"not open source", in the listing, the docs, or any reply to a buyer.

## 1. Product settings

| Field | Value |
|---|---|
| Seller | TPT Solutions |
| Name | TPT App Policy 2026 |
| Type | Digital product (download) |
| URL slug | `tpt-app-policy-2026` |
| Support email | `support@tptsolutions.co.nz` |
| Cover image | 1280 x 720 px. Show the expense example and a `decision` JSON output. No third-party logos. |
| Thumbnail | 600 x 600 px. The product name with the decision words. |
| Tags | `business rules`, `policy engine`, `approvals`, `yaml`, `cli`, `offline`, `self-hosted` |
| Category | Developer tools |
| Summary (one line, on cards) | Offline business rules in YAML. Check JSON, get a decision and the rules that made it. |
| Files | `tpt-app-policy-2026.1.0-windows-x64.tar.gz`, `tpt-app-policy-2026.1.0-linux-x64.tar.gz`, `SHA256SUMS` |
| Versioning | Upload only the 2026 release (2026.1.0). Later 2026 releases stay out of this listing. |
| Refunds | 14 days, section 7. Set the same in Gumroad's refund settings. **CHECK** that Gumroad allows this window. |

### Files to upload

Build the bundles with the release workflow (push a tag, see section 9), then
download the artifacts from the GitHub release. For a Windows-only first sale,
you can run `scripts/package.sh` in Git Bash instead.

Check the checksums before you upload:

```sh
sha256sum -c SHA256SUMS
```

Each bundle contains `bin/`, `docs/`, `examples/`, `policies/`, `README.md`,
`CHANGELOG.md`, `LICENSE-MIT`, `LICENSE-APACHE` and `VERSION.txt`.

## 2. Price

| Product | Price | Includes |
|---|---|---|
| TPT App Policy 2026 | $49 one-time (suggested) | Windows and Linux bundles for the 2026 release, all docs, examples and self-service help |

Set the price for each year separately. A new release year is a new paid
purchase, so the 2027 price is its own decision.

## 3. Listing title and short description

**Title:** TPT App Policy 2026: offline business rules in YAML

**Short description:**

> Write business rules in a YAML file. Check JSON data against them. Get a
> decision (approved, review, approval required, or rejected) and the rules
> that produced it. Runs on your machine, offline, with no account.

## 4. Listing body

Paste this as the product description.

```markdown
## What it does

TPT App Policy checks data against business rules you write yourself.

You write a policy file:

    policy: expense
    rules:
      - id: manager-approval
        description: Expenses over $1,000 need manager approval
        when:
          expense.amount:
            gt: 1000
        then:
          decision: approval_required
          approver: manager

You send it data:

    { "expense": { "amount": 6200 } }

You get a decision and the rules that produced it:

    {
      "decision": "approval_required",
      "approvers": ["manager"],
      "matched_rules": ["manager-approval"]
    }

## Why people buy it

- **Runs locally.** No cloud, no account, no licence server, no phone-home.
- **Explains itself.** Every result lists the rules that matched and the checks
  that failed.
- **Repeatable.** The same policy and the same data always give byte-identical
  output, so results can be tested and audited.
- **Testable.** Put test cases in the policy file and run them in CI.
- **Scriptable.** Command line, stdin/stdout, and a local HTTP endpoint.
- **Stable exit codes.** Scripts can branch on the decision without parsing text.

## What you get

- `tpt-policy`, the command-line tool, for Windows x64 and Linux x64
- The tested 2026 release bundle, with SHA-256 checksums
- Documentation: install, quickstart, concepts, CLI and policy references,
  integration, troubleshooting, FAQ, and how to get help
- Example policies, including a purchasing policy with tests
- The licence texts that apply to the software

## What it does not do

- It does not connect to ERP, accounting or workflow systems. You export data
  as JSON and act on the decision.
- It does not certify compliance with any standard or law. It applies the rules
  you write.
- It has no graphical interface.

## Refunds

Not happy within 14 days of purchase? Email support@tptsolutions.co.nz and you
get a full refund, no questions asked.

## Support

Read the docs first. The download includes a FAQ, a troubleshooting guide and a
help page with a bug report template. Questions and bug reports are welcome at
support@tptsolutions.co.nz. Support is best-effort, with no guaranteed response
time and no help with writing your own rules.

## Requirements

- Windows 10 or 11, 64-bit, or Linux 64-bit
- No runtime to install. The binary is self-contained.
- The Windows program is not digitally signed yet. The bundled help explains how
  to check the checksum and run it.
```

Before publishing, check every claim against the release you upload. The
Windows and Linux install tests in TODO.md are not done, so the requirements line
is not yet verified (section 8).

## 5. Price card description

**TPT App Policy 2026:** The tested Windows and Linux bundle for the 2026
release, with the full documentation, help and examples. One-time purchase.
Full refund within 14 days.

## 6. Receipt and download message

```text
Thanks for buying TPT App Policy 2026.

Download the archive for your system (Windows or Linux) and the SHA256SUMS file.
Check the archive before you unpack it. The commands are in docs/FAQ.md.

Start with docs/QUICKSTART.md. If something does not work, docs/TROUBLESHOOTING.md
and docs/FAQ.md answer most questions. For anything else, email
support@tptsolutions.co.nz. The details to include are listed in docs/SUPPORT.md.

Changed your mind? Email support@tptsolutions.co.nz within 14 days of purchase
for a full refund.
```

## 7. Refunds, support and legal

**Refund policy: 14 days, no questions asked.** A full refund on request within
14 days of purchase. Use this wording in the listing and in Gumroad's refund
settings:

> Refunds: full refund within 14 days of purchase, no questions asked. Email
> support@tptsolutions.co.nz.

Why this policy:

- It matches the 14-day withdrawal right that EU and UK consumers have for
  digital content, so one policy works in each market.
- It is simple to state and simple to run. A refund is one email.
- It can be abused: someone could buy, use it, and ask for a refund. That is the
  cost you have accepted. Gumroad may flag repeat buyers, which is worth checking
  if refunds start to add up (**CHECK**).

After 14 days, refunds are at your discretion. Do not promise anything beyond
14 days in the listing.

Before you publish, have a New Zealand adviser look at the wording. The
Consumer Guarantees Act may apply to TPT Solutions as a seller. A 14-day refund
does not by itself cover a faulty product after that period, so the adviser
should say what else you owe buyers.

**Support.** Best-effort, through `support@tptsolutions.co.nz`. No purchase
check is needed to ask a question.

**Tracking and buyers.** Gumroad records each sale and buyer email, because it
is the seller of record. The software has no licence keys, no activation and no
buyer identifier, so nothing in the binary tracks buyers. Do not add licence keys
to the Gumroad product, because nothing in the software would check them.

**Terms and privacy (CHECK):**

- Gumroad's terms apply to the sale. Read them once.
- Gumroad handles payment and, in many regions, sales tax or VAT as seller of
  record. Confirm this for each country you sell into on Gumroad's tax pages.
- The product sends no data anywhere (see `docs/SECURITY.md`).
- Gumroad collects buyer data for the sale. Your privacy notice should say what
  TPT Solutions does with buyer emails.

**Claims and names:**

- Do not describe the product as certified, approved by a regulator, or
  compliant with a named standard. It is a rules engine, not a compliance tool.
- Check that the TPT Solutions name and the "TPT App Policy" name do not clash
  with other trademarks in the markets you sell into.

## 8. Before you publish: checklist

Items 1 to 3 block a sale.

1. [ ] Windows install tested on a clean Windows machine: unzip, check the
   checksum, run `tpt-policy doctor`, run the quickstart. Include the SmartScreen
   steps from `docs/TROUBLESHOOTING.md`.
2. [ ] Linux install tested on a clean Linux machine. The release workflow builds
   the Linux bundle on Ubuntu, but it has not run on a real tag yet.
3. [ ] Release tagged, CI green on both OSes, bundles and `SHA256SUMS` attached
   to the GitHub release.
4. [ ] Refund wording reviewed by an adviser, and the 14-day window set in
   Gumroad (section 7 CHECK items).
5. [ ] Support email `support@tptsolutions.co.nz` working, and checked at least
   weekly.
6. [ ] Each help doc read once by someone who has not seen the code. Their
   questions should be answered by the docs. Add any that are not.
7. [ ] The listing and price card say nothing about licensing beyond section 4,
   and nothing calls the product proprietary or closed source.
8. [ ] Docker image: either tested and listed, or left out. The listing does not
   mention Docker.
9. [ ] Every sentence in section 4 is still true for the release you upload.

**Code signing: unsigned.** The Windows program will not be code-signed for now.
The FAQ and troubleshooting guide explain the SmartScreen warning and how to
check the checksum first. Revisit this if buyer complaints justify a certificate.

## 9. Versioning and each release

Releases are named by the year they ship in, then a counter for that year:

| Release | Version | Tag | Gumroad |
|---|---|---|---|
| First release, 2026 | `2026.1.0` | `v2026.1.0` | TPT App Policy 2026 |
| Fix release in 2026 | `2026.1.1` | `v2026.1.1` | Published on GitHub, not added to the listing |
| Feature release in 2026 | `2026.2.0` | `v2026.2.0` | Published on GitHub, not added to the listing |
| First release, 2027 | `2027.1.0` | `v2027.1.0` | New listing: TPT App Policy 2027 |

The version is set in `Cargo.toml` (`[workspace.package] version`). The product
reports it in `tpt-policy --version`, and every evaluation records the engine
version.

Your policy files have their own `version` field. That is separate, and belongs
to the person who writes the policy.

For each annual release:

1. Update `CHANGELOG.md`. Note any change to the policy format.
2. Set the version in `Cargo.toml`, and update the expected output in
   `examples/expense/expense.expected.json` if the engine version is shown there.
3. Run `cargo test --workspace`, clippy and fmt. CI does this too.
4. Tag the release: `git tag v2027.1.0 && git push origin v2027.1.0`. The release
   workflow builds the bundles and attaches them to a GitHub release.
5. Create a new Gumroad product for the year, with its own price, and upload the
   bundles and `SHA256SUMS`.
6. Leave the previous year's listing up, and say in its description that a newer
   release exists.
