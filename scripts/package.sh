#!/usr/bin/env sh
# Builds a release bundle in the layout from spec §23 and writes checksums (§40).
#
# Usage: scripts/package.sh [policy|data|document|invoice|transform|approve|evidence]
#   policy (default): TPT App Policy, binary tpt-policy
#   data:             TPT Data Validator, binary tpt-data
#   document:         TPT Document Validator, binary tpt-document
#   invoice:          TPT Invoice Validator, binary tpt-invoice
#   transform:        TPT Data Transformer, binary tpt-transform
#   approve:          TPT Approval Engine, binary tpt-approve
#   evidence:         TPT Compliance Evidence Processor, binary tpt-evidence
#
# Output: dist/<name>-<version>-<platform>.tar.gz, and dist/SHA256SUMS listing
# every archive in dist/.
#
# Runs on Linux, macOS and Git Bash on Windows.
set -eu

cd "$(dirname "$0")/.."

PRODUCT=${1:-policy}
case "$PRODUCT" in
    policy)
        CRATE=tpt-app-policy
        BINARY=tpt-policy
        NAME_PREFIX=tpt-app-policy
        ;;
    data)
        CRATE=tpt-data
        BINARY=tpt-data
        NAME_PREFIX=tpt-data
        ;;
    invoice)
        CRATE=tpt-invoice
        BINARY=tpt-invoice
        NAME_PREFIX=tpt-invoice
        ;;
    document)
        CRATE=tpt-document
        BINARY=tpt-document
        NAME_PREFIX=tpt-document
        ;;
    transform)
        CRATE=tpt-transform
        BINARY=tpt-transform
        NAME_PREFIX=tpt-transform
        ;;
    evidence)
        CRATE=tpt-evidence
        BINARY=tpt-evidence
        NAME_PREFIX=tpt-evidence
        ;;
    approve)
        CRATE=tpt-approve
        BINARY=tpt-approve
        NAME_PREFIX=tpt-approve
        ;;
    *)
        echo "error: unknown product '$PRODUCT'" >&2
        echo "  fix: use 'policy', 'data', 'document', 'invoice', 'transform', 'approve' or 'evidence'" >&2
        exit 1
        ;;
esac

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
ARCH=$(uname -m)
case "$ARCH" in
    x86_64 | amd64) ARCH=x64 ;;
    aarch64 | arm64) ARCH=arm64 ;;
esac
case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*) OS=windows; EXT=.exe ;;
    Linux) OS=linux; EXT= ;;
    Darwin) OS=macos; EXT= ;;
    *) echo "error: unsupported OS $(uname -s)" >&2; exit 1 ;;
esac
PLATFORM="$OS-$ARCH"
NAME="$NAME_PREFIX-$VERSION"
BUNDLE="dist/$NAME"
ARCHIVE="$NAME-$PLATFORM.tar.gz"

# Both licence texts ship in every bundle. Refuse to package without them.
for f in LICENSE-MIT LICENSE-APACHE; do
    if [ ! -f "$f" ]; then
        echo "error: $f is missing" >&2
        exit 1
    fi
done

echo "building $NAME for $PLATFORM"
cargo build --release -p "$CRATE"

rm -rf "$BUNDLE" "dist/$ARCHIVE"
mkdir -p "$BUNDLE/bin" "$BUNDLE/docs"

cp "target/release/$BINARY$EXT" "$BUNDLE/bin/"
cp LICENSE-MIT LICENSE-APACHE "$BUNDLE/"

if [ "$PRODUCT" = policy ]; then
    mkdir -p "$BUNDLE/examples" "$BUNDLE/policies"
    cp -R examples/. "$BUNDLE/examples/"
    cp examples/expense/expense.yaml examples/purchasing/purchasing.yaml "$BUNDLE/policies/"
    # User-facing docs only. ARCHITECTURE.md is internal planning and stays out.
    for doc in INSTALL QUICKSTART CONCEPTS CLI_REFERENCE POLICY_REFERENCE INTEGRATION TROUBLESHOOTING SECURITY SUPPORT FAQ; do
        cp "docs/$doc.md" "$BUNDLE/docs/"
    done
    cp README.md CHANGELOG.md "$BUNDLE/"
elif [ "$PRODUCT" = data ]; then
    mkdir -p "$BUNDLE/examples"
    cp -R examples/data/. "$BUNDLE/examples/"
    for doc in DATA_VALIDATOR SCHEMA_REFERENCE SECURITY; do
        cp "docs/$doc.md" "$BUNDLE/docs/"
    done
    cp CHANGELOG.md "$BUNDLE/"
elif [ "$PRODUCT" = invoice ]; then
    mkdir -p "$BUNDLE/examples"
    cp -R examples/invoices/. "$BUNDLE/examples/"
    for doc in INVOICE_VALIDATOR DOCUMENT_VALIDATOR SCHEMA_REFERENCE POLICY_REFERENCE SECURITY; do
        cp "docs/$doc.md" "$BUNDLE/docs/"
    done
    cp CHANGELOG.md "$BUNDLE/"
elif [ "$PRODUCT" = evidence ]; then
    mkdir -p "$BUNDLE/examples"
    cp -R examples/evidence/. "$BUNDLE/examples/"
    for doc in COMPLIANCE_EVIDENCE SECURITY; do
        cp "docs/$doc.md" "$BUNDLE/docs/"
    done
    cp CHANGELOG.md "$BUNDLE/"
elif [ "$PRODUCT" = approve ]; then
    mkdir -p "$BUNDLE/examples"
    cp -R examples/approval/. "$BUNDLE/examples/"
    for doc in APPROVAL_ENGINE POLICY_REFERENCE SECURITY; do
        cp "docs/$doc.md" "$BUNDLE/docs/"
    done
    cp CHANGELOG.md "$BUNDLE/"
elif [ "$PRODUCT" = transform ]; then
    mkdir -p "$BUNDLE/examples"
    cp -R examples/transform/. "$BUNDLE/examples/"
    for doc in DATA_TRANSFORMER SECURITY; do
        cp "docs/$doc.md" "$BUNDLE/docs/"
    done
    cp CHANGELOG.md "$BUNDLE/"
else
    mkdir -p "$BUNDLE/examples"
    cp -R examples/documents/. "$BUNDLE/examples/"
    for doc in DOCUMENT_VALIDATOR SCHEMA_REFERENCE POLICY_REFERENCE SECURITY; do
        cp "docs/$doc.md" "$BUNDLE/docs/"
    done
    cp CHANGELOG.md "$BUNDLE/"
fi

# Version and platform in the bundle itself, so a loose binary can be identified.
"$BUNDLE/bin/$BINARY$EXT" --version > "$BUNDLE/VERSION.txt"

tar -czf "dist/$ARCHIVE" -C dist "$NAME"
(cd dist && sha256sum ./*.tar.gz | sed 's|\./||' > SHA256SUMS)

echo "wrote dist/$ARCHIVE"
echo "wrote dist/SHA256SUMS"
cat dist/SHA256SUMS
