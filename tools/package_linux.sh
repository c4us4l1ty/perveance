#!/usr/bin/env bash
# Create a fresh, unsigned evaluation bundle from trusted local Linux binaries.
set -eu
if [ "$#" -ne 2 ]; then
    echo "Usage: $0 BINARY_DIRECTORY NEW_OUTPUT_DIRECTORY" >&2
    exit 2
fi
bin_dir=$1
output=$2
if [ "$(uname -s)" != Linux ] || [ "$(uname -m)" != x86_64 ]; then
    echo "This packaging script supports only x86-64 Linux." >&2
    exit 1
fi
for binary in perveance perveance-cli; do
    if [ ! -f "$bin_dir/$binary" ] || [ ! -x "$bin_dir/$binary" ]; then
        echo "Missing executable: $bin_dir/$binary" >&2
        exit 1
    fi
done
root=$(cd "$(dirname "$0")/.." && pwd)
# mkdir fails if the destination exists, so a retry cannot overwrite artifacts.
mkdir "$output"
output=$(cd "$output" && pwd)
stage=$(mktemp -d "${TMPDIR:-/tmp}/perveance-package.XXXXXXXX")
trap 'rm -rf "$stage"' EXIT
bundle="$stage/perveance-linux-x86_64"
mkdir -p "$bundle/bin" "$bundle/examples"
install -m 755 "$bin_dir/perveance" "$bundle/bin/perveance"
install -m 755 "$bin_dir/perveance-cli" "$bundle/bin/perveance-cli"
install -m 644 "$root/packaging/README-LINUX.txt" "$bundle/README.txt"
install -m 644 "$root/examples/potion-operations.json" "$root/examples/rocket-operations.json" "$root/examples/opaque-document-v2.json" "$bundle/examples/"
{
    printf 'Perveance experimental evaluation build\n'
    printf 'Source revision: '
    git -C "$root" rev-parse HEAD
    printf 'Working tree dirty: '
    if [ -n "$(git -C "$root" status --porcelain --untracked-files=normal)" ]; then echo yes; else echo no; fi
    printf 'Built UTC: '; date -u '+%Y-%m-%dT%H:%M:%SZ'
    printf 'Architecture: '; uname -m
    printf 'Kernel: '; uname -sr
    printf 'Build glibc: '; getconf GNU_LIBC_VERSION 2>/dev/null || echo unknown
    printf 'Rust compiler: '; rustc --version 2>/dev/null || echo unknown
} > "$bundle/BUILD-INFO.txt"
archive=perveance-linux-x86_64.tar.gz
tar -C "$stage" -czf "$output/$archive" perveance-linux-x86_64
(cd "$output" && sha256sum "$archive" > SHA256SUMS)
echo "Created $output/$archive and $output/SHA256SUMS"
