#!/usr/bin/env sh
# Builds a release bundle in the layout from spec §23 and writes checksums (§40).
#
# Usage: scripts/package.sh
# Output: dist/tpt-app-policy-<version>-<platform>.tar.gz and dist/SHA256SUMS
#
# Runs on Linux, macOS and Git Bash on Windows.
set -eu

cd "$(dirname "$0")/.."

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
NAME="tpt-app-policy-$VERSION"
BUNDLE="dist/$NAME"

# The licence is a legal decision, not something to invent here. Refuse to
# ship without one unless a test build explicitly opts in.
if [ ! -f LICENSE.txt ]; then
    if [ "${TPT_ALLOW_UNLICENSED:-}" = "1" ]; then
        echo "warning: LICENSE.txt is missing; this bundle is for internal testing only" >&2
    else
        echo "error: LICENSE.txt is missing. Add the commercial licence before packaging." >&2
        echo "  fix: add LICENSE.txt, or set TPT_ALLOW_UNLICENSED=1 for an internal test build" >&2
        exit 1
    fi
fi

echo "building $NAME for $PLATFORM"
cargo build --release -p tpt-app-policy

rm -rf "dist"
mkdir -p "$BUNDLE/bin" "$BUNDLE/examples" "$BUNDLE/policies" "$BUNDLE/schemas" "$BUNDLE/docs"

cp "target/release/tpt-policy$EXT" "$BUNDLE/bin/"
cp -R examples/. "$BUNDLE/examples/"
cp examples/expense/expense.yaml examples/purchasing/purchasing.yaml "$BUNDLE/policies/"
cp docs/ARCHITECTURE.md "$BUNDLE/docs/"
cp README.md CHANGELOG.md "$BUNDLE/"
if [ -f LICENSE.txt ]; then
    cp LICENSE.txt "$BUNDLE/"
else
    echo "UNLICENSED INTERNAL TEST BUILD. Do not distribute." > "$BUNDLE/LICENSE.txt"
fi

# Version and platform in the bundle itself, so a loose binary can be identified.
"$BUNDLE/bin/tpt-policy$EXT" --version > "$BUNDLE/VERSION.txt"

ARCHIVE="$NAME-$PLATFORM.tar.gz"
tar -czf "dist/$ARCHIVE" -C dist "$NAME"
(cd dist && sha256sum "$ARCHIVE" > SHA256SUMS)

echo "wrote dist/$ARCHIVE"
echo "wrote dist/SHA256SUMS"
cat dist/SHA256SUMS
