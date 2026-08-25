#!/usr/bin/env sh
set -eu

executable=${1:-target/release/ba-agent-control}
samples=${2:-100}
case "$samples" in *[!0-9]*|'') echo "samples must be a positive integer" >&2; exit 2;; esac
[ "$samples" -gt 0 ] || { echo "samples must be a positive integer" >&2; exit 2; }
[ -f "$executable" ] || { echo "executable does not exist: $executable" >&2; exit 2; }
[ -x "$executable" ] || { echo "executable is not runnable: $executable" >&2; exit 2; }

executable=$(CDPATH= cd -- "$(dirname -- "$executable")" && printf '%s/%s\n' "$(pwd -P)" "$(basename -- "$executable")")
benchmark_root=$(mktemp -d "${TMPDIR:-/tmp}/ba-agent-bench.XXXXXX")
cleanup() {
  case "$benchmark_root" in
    "${TMPDIR:-/tmp}"/ba-agent-bench.*) rm -rf -- "$benchmark_root" ;;
    *) echo "unsafe benchmark path: $benchmark_root" >&2 ;;
  esac
}
trap cleanup EXIT INT TERM

python3 - "$executable" "$benchmark_root" "$samples" <<'PY'
import json
from pathlib import Path
import re
import subprocess
import sys
import time
from urllib.request import urlopen

executable = Path(sys.argv[1])
benchmark_root = Path(sys.argv[2])
samples = int(sys.argv[3])
data_root = benchmark_root / "data"
stdout_path = benchmark_root / "stdout.log"
stderr_path = benchmark_root / "stderr.log"
data_root.mkdir()

started = time.perf_counter()
with stdout_path.open("w+", encoding="utf-8") as stdout, stderr_path.open("w", encoding="utf-8") as stderr:
    process = subprocess.Popen(
        [str(executable), "--data-dir", str(data_root), "ui", "--no-open"],
        stdout=stdout,
        stderr=stderr,
        text=True,
    )
    try:
        port = None
        for _ in range(1000):
            stdout.flush()
            stdout.seek(0)
            match = re.search(r"BA Agent: http://127\.0\.0\.1:(\d+)/", stdout.read())
            if match:
                port = int(match.group(1))
                break
            if process.poll() is not None:
                raise RuntimeError(f"server exited before listening; see {stderr_path}")
            time.sleep(0.01)
        cold_start_ms = (time.perf_counter() - started) * 1000
        if port is None:
            raise RuntimeError(f"server did not report a listening address; see {stderr_path}")

        durations = []
        for _ in range(samples):
            request_started = time.perf_counter()
            with urlopen(f"http://127.0.0.1:{port}/api/v1/health", timeout=5) as response:
                response.read()
            durations.append((time.perf_counter() - request_started) * 1000)
        durations.sort()

        rss_kib = None
        proc_status = Path(f"/proc/{process.pid}/status")
        if proc_status.exists():
            match = re.search(r"^VmRSS:\s+(\d+)\s+kB$", proc_status.read_text(), re.MULTILINE)
            if match:
                rss_kib = int(match.group(1))
        if rss_kib is None:
            rss_kib = int(subprocess.check_output(
                ["ps", "-o", "rss=", "-p", str(process.pid)], text=True,
            ).strip())

        percentile = lambda fraction: durations[max(0, int((samples * fraction) + 0.999999) - 1)]
        print(json.dumps({
            "coldStartMs": round(cold_start_ms, 3),
            "apiSamples": samples,
            "apiP50Ms": round(percentile(0.50), 3),
            "apiP95Ms": round(percentile(0.95), 3),
            "apiMaxMs": round(durations[-1], 3),
            "workingSetMiB": round(rss_kib / 1024, 2),
        }, separators=(",", ":")))
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
PY
