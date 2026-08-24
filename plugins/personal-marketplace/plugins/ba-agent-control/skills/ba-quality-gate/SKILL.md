---
name: ba-quality-gate
description: Evaluate BA Agent quality gates when a completed change needs reproducible checks and a permanent tamper-evident receipt before commit or release.
---

# Evaluate quality gates

Load the project's configured gates and run only commands authorized for the task stage. The ordinary sequence is format, lint, type check, changed-scope tests, secret scan, and diff review; release candidates also require the full suite.

Record command, exit status, duration, relevant output, tool version when available, and artifact hashes. Never weaken a check, edit results, or treat a skipped gate as passed.

Create a receipt only through BA Agent after all required gates pass. On failure, return exact remediation evidence and leave commit eligibility blocked.
