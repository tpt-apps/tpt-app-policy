#!/usr/bin/env sh
# Smoke-tests the release bundles in dist/ (run scripts/package.sh first).
#
# 1. Checks every archive against dist/SHA256SUMS.
# 2. Confirms each of the nine products has an archive for this platform.
# 3. Unpacks each archive into a clean folder and runs its binary with
#    --version and doctor. Each must exit 0.
#
# Exits 1 on the first failure. No code signing is involved: the checksums
# are the integrity check.
#
# Runs on Linux, macOS and Git Bash on Windows.
set -eu

cd "$(dirname "$0")/.."

PRODUCTS="tpt-app-policy tpt-data tpt-document tpt-invoice tpt-transform tpt-approve tpt-evidence tpt-ai-guard tpt-secure-run"

if [ ! -f dist/SHA256SUMS ]; then
    echo "error: dist/SHA256SUMS is missing" >&2
    echo "  fix: run scripts/package.sh for each product first" >&2
    exit 1
fi

echo "checking checksums"
(cd dist && sha256sum -c SHA256SUMS)

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

for product in $PRODUCTS; do
    set -- dist/"$product"-*.tar.gz
    if [ ! -f "$1" ]; then
        echo "error: no bundle for $product in dist/" >&2
        echo "  fix: run scripts/package.sh $product" >&2
        exit 1
    fi
    archive=$1
    folder="$WORK/$product"
    mkdir -p "$folder"
    tar -xzf "$archive" -C "$folder"

    bundle=$(find "$folder" -mindepth 1 -maxdepth 1 -type d | head -n 1)
    binary=$(ls "$bundle/bin" | head -n 1)
    if [ ! -f "$bundle/VERSION.txt" ]; then
        echo "error: $archive has no VERSION.txt" >&2
        exit 1
    fi

    echo "== $(basename "$archive"): $binary"
    "$bundle/bin/$binary" --version
    "$bundle/bin/$binary" doctor > "$folder/doctor.txt" || {
        echo "error: $binary doctor failed:" >&2
        cat "$folder/doctor.txt" >&2
        exit 1
    }
    if grep -q "^\[fail\]" "$folder/doctor.txt"; then
        echo "error: $binary doctor reported a failed check" >&2
        cat "$folder/doctor.txt" >&2
        exit 1
    fi
done

echo "all bundles smoke-tested"
