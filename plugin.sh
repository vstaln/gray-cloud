#!/bin/sh
here="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
sys="$(uname -s)"
case "$sys" in
MINGW* | MSYS* | CYGWIN*) rel="x86_64-pc-windows-msvc/gray-cloud.exe" ;;
Darwin*)
    case "$(uname -m)" in
    arm64) rel="aarch64-apple-darwin/gray-cloud" ;;
    *) rel="x86_64-apple-darwin/gray-cloud" ;;
    esac ;;
*)
    case "$(uname -m)" in
    aarch64 | arm64) rel="aarch64-unknown-linux-musl/gray-cloud" ;;
    *) rel="x86_64-unknown-linux-musl/gray-cloud" ;;
    esac ;;
esac
if [ -x "$here/$rel" ]; then exec "$here/$rel" "$@"; fi
for cand in "$here/target/release/gray-cloud.exe" "$here/target/release/gray-cloud" "$here/target/debug/gray-cloud.exe" "$here/target/debug/gray-cloud"; do
    if [ -x "$cand" ]; then exec "$cand" "$@"; fi
done
echo "gray-cloud binary not found in $here" >&2
exit 1
