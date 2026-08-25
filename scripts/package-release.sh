#!/usr/bin/env sh
set -eu

if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
  echo "usage: package-release.sh <target> <binary> <output-dir> [version]" >&2
  exit 2
fi

target=$1
binary=$2
output_dir=$3
version=${4:-$(node -p "require('./package.json').version")}
case "$target" in *[!a-z0-9-]*|'') echo "invalid release target: $target" >&2; exit 2;; esac
case "$version" in *[!A-Za-z0-9.-]*|'') echo "invalid release version: $version" >&2; exit 2;; esac

repository_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
binary=$(CDPATH= cd -- "$(dirname -- "$binary")" && printf '%s/%s\n' "$(pwd -P)" "$(basename -- "$binary")")
mkdir -p -- "$output_dir"
output_root=$(CDPATH= cd -- "$output_dir" && pwd -P)
archive_name="ba-agent-control-v${version}-${target}"
archive="$output_root/$archive_name.tar.gz"
checksum="$archive.sha256"
if [ -e "$archive" ] || [ -e "$checksum" ]; then
  echo "release output already exists; refusing to overwrite: $archive" >&2
  exit 1
fi

staging=$(mktemp -d "$output_root/.staging.XXXXXX")
cleanup() {
  case "$staging" in "$output_root"/.staging.*) rm -rf -- "$staging";; *) echo "unsafe staging path: $staging" >&2;; esac
}
trap cleanup EXIT INT TERM

install -m 0755 -- "$binary" "$staging/ba-agent-control"
for file in README.md LICENSE NOTICE THIRD_PARTY_LICENSES.md THIRD_PARTY_NOTICES.txt; do
  install -m 0644 -- "$repository_root/$file" "$staging/$file"
done
install -m 0644 -- "$repository_root/docs/installation.md" "$staging/installation.md"
install -m 0755 -- "$repository_root/scripts/install.sh" "$staging/install.sh"
tar -C "$staging" -czf "$archive" .
if command -v sha256sum >/dev/null 2>&1; then
  hash=$(sha256sum "$archive" | awk '{print $1}')
else
  hash=$(shasum -a 256 "$archive" | awk '{print $1}')
fi
printf '%s  %s\n' "$hash" "$(basename -- "$archive")" > "$checksum"
printf '{"archive":"%s","checksum":"%s","sha256":"%s"}\n' "$archive" "$checksum" "$hash"
