#!/usr/bin/env sh
set -eu

action=${1:-}
case "$action" in
  install) prefix=${3:-"${HOME}/.local/bin"} ;;
  rollback|uninstall) prefix=${2:-"${HOME}/.local/bin"} ;;
  *) prefix=${2:-"${HOME}/.local/bin"} ;;
esac
executable="$prefix/ba-agent-control"
previous="$prefix/ba-agent-control.previous"
[ ! -L "$prefix" ] || { echo "install directory must not be a symlink: $prefix" >&2; exit 1; }
[ ! -L "$executable" ] || { echo "installed binary must not be a symlink: $executable" >&2; exit 1; }
[ ! -L "$previous" ] || { echo "previous binary must not be a symlink: $previous" >&2; exit 1; }

case "$action" in
  install)
    archive=${2:-}
    [ -n "$archive" ] || { echo "usage: install.sh install <archive> [prefix]" >&2; exit 2; }
    archive=$(CDPATH= cd -- "$(dirname -- "$archive")" && printf '%s/%s\n' "$(pwd -P)" "$(basename -- "$archive")")
    staging=$(mktemp -d "${TMPDIR:-/tmp}/ba-agent-install.XXXXXX")
    cleanup() { case "$staging" in "${TMPDIR:-/tmp}"/ba-agent-install.*) rm -rf -- "$staging";; esac; }
    trap cleanup EXIT INT TERM
    tar -tzf "$archive" > "$staging/entries"
    : > "$staging/normalized"
    while IFS= read -r entry; do
      normalized=${entry#./}
      case "$normalized" in
        '') ;;
        ba-agent-control|install.sh|installation.md|LICENSE|NOTICE|README.md|THIRD_PARTY_LICENSES.md)
          printf '%s\n' "$normalized" >> "$staging/normalized"
          ;;
        *) echo "archive contains an unsafe or unexpected entry: $entry" >&2; exit 1 ;;
      esac
    done < "$staging/entries"
    printf '%s\n' ba-agent-control install.sh installation.md LICENSE NOTICE README.md THIRD_PARTY_LICENSES.md |
      LC_ALL=C sort > "$staging/expected"
    LC_ALL=C sort "$staging/normalized" > "$staging/actual"
    cmp -s "$staging/expected" "$staging/actual" || {
      echo "archive must contain each expected file exactly once" >&2
      exit 1
    }
    tar -C "$staging" -xzf "$archive"
    candidate="$staging/ba-agent-control"
    [ -f "$candidate" ] || { echo "archive does not contain ba-agent-control" >&2; exit 1; }
    [ ! -L "$candidate" ] || { echo "candidate binary must not be a symlink" >&2; exit 1; }
    chmod 0755 "$candidate"
    "$candidate" --help >/dev/null
    mkdir -p -- "$prefix"
    incoming="$prefix/ba-agent-control.incoming.$$"
    install -m 0755 -- "$candidate" "$incoming"
    rm -f -- "$previous"
    if [ -f "$executable" ]; then mv -- "$executable" "$previous"; fi
    if ! mv -- "$incoming" "$executable"; then
      if [ -f "$previous" ] && [ ! -e "$executable" ]; then mv -- "$previous" "$executable"; fi
      rm -f -- "$incoming"
      exit 1
    fi
    echo "Installed BA Agent at $executable. Runtime data was retained."
    ;;
  rollback)
    [ -f "$previous" ] || { echo "no previous BA Agent binary is available" >&2; exit 1; }
    swap="$prefix/ba-agent-control.swap.$$"
    if [ -f "$executable" ]; then mv -- "$executable" "$swap"; fi
    if ! mv -- "$previous" "$executable"; then
      if [ -f "$swap" ] && [ ! -e "$executable" ]; then mv -- "$swap" "$executable"; fi
      exit 1
    fi
    if [ -f "$swap" ]; then mv -- "$swap" "$previous"; fi
    echo "Rolled back BA Agent. Runtime data was retained."
    ;;
  uninstall)
    rm -f -- "$executable" "$previous"
    echo "Removed BA Agent binaries. Runtime data was retained."
    ;;
  *)
    echo "usage: install.sh <install ARCHIVE [PREFIX] | rollback [PREFIX] | uninstall [PREFIX]>" >&2
    exit 2
    ;;
esac
