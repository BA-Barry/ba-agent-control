#!/usr/bin/env sh
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: test-install-security.sh <release-archive>" >&2
  exit 2
fi

archive=$(CDPATH= cd -- "$(dirname -- "$1")" && printf '%s/%s\n' "$(pwd -P)" "$(basename -- "$1")")
installer=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)/install.sh
test_root=$(mktemp -d "${TMPDIR:-/tmp}/ba-agent-install-security.XXXXXX")
cleanup() {
  case "$test_root" in "${TMPDIR:-/tmp}"/ba-agent-install-security.*) rm -rf -- "$test_root";; *) echo "unsafe installer test path: $test_root" >&2;; esac
}
trap cleanup EXIT INT TERM

expect_failure() {
  pattern=$1
  shift
  if "$@" >"$test_root/stdout" 2>"$test_root/stderr"; then
    echo "expected installer failure matching: $pattern" >&2
    exit 1
  fi
  grep -E "$pattern" "$test_root/stderr" >/dev/null || {
    echo "installer failed for an unexpected reason:" >&2
    cat "$test_root/stderr" >&2
    exit 1
  }
}

mismatch="$test_root/checksum-mismatch.tar.gz"
cp -- "$archive" "$mismatch"
printf '%064d  checksum-mismatch.tar.gz\n' 0 > "$mismatch.sha256"
expect_failure 'checksum mismatch' sh "$installer" install "$mismatch" "$test_root/mismatch-install"

unsafe="$test_root/unsafe-entry.tar.gz"
python3 - "$archive" "$unsafe" <<'PY'
from io import BytesIO
from pathlib import Path
import tarfile
import sys

source_path, target_path = map(Path, sys.argv[1:])
with tarfile.open(source_path, "r:gz") as source, tarfile.open(target_path, "w:gz") as target:
    for member in source.getmembers():
        stream = source.extractfile(member) if member.isfile() else None
        target.addfile(member, stream)
    payload = b"must never be extracted"
    entry = tarfile.TarInfo("../escape.txt")
    entry.size = len(payload)
    target.addfile(entry, BytesIO(payload))
PY
if command -v sha256sum >/dev/null 2>&1; then
  unsafe_hash=$(sha256sum "$unsafe" | awk '{print $1}')
else
  unsafe_hash=$(shasum -a 256 "$unsafe" | awk '{print $1}')
fi
printf '%s  unsafe-entry.tar.gz\n' "$unsafe_hash" > "$unsafe.sha256"
expect_failure 'unsafe or unexpected entry' sh "$installer" install "$unsafe" "$test_root/unsafe-install"
[ ! -e "$test_root/escape.txt" ] || { echo "unsafe archive escaped the staging directory" >&2; exit 1; }

mkdir "$test_root/real-install"
ln -s "$test_root/real-install" "$test_root/symlink-install"
expect_failure 'install directory must not be a symlink' sh "$installer" install "$archive" "$test_root/symlink-install"

echo "Unix installer security tests passed: checksum, archive entries, and symlinks"
