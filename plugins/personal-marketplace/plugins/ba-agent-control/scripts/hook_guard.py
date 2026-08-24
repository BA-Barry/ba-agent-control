#!/usr/bin/env python3
"""Codex hook policy for BA Agent managed runs.

The guard is intentionally conservative and stateless. Server-side policy,
path canonicalization, approval records, and the Codex sandbox remain the
authoritative controls.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import re
import sys
from typing import Any


DANGEROUS_GIT = (
    r"\bgit\b[^\r\n;&|]*\breset\s+--hard\b",
    r"\bgit\b[^\r\n;&|]*\bclean\s+-(?:\w*f\w*|\w*x\w*)\b",
    r"\bgit\b[^\r\n;&|]*\bpush\b[^\r\n;&|]*(?:--force(?:-with-lease)?|-f(?:\s|$))",
    r"\bgit\b[^\r\n;&|]*\b(?:checkout|restore)\s+--\s+(?:\.|/|\\)",
)
DELETE_COMMAND = (
    r"(?:\brm\s+-[^\r\n]*r|\b(?:Remove-Item|ri)\b[^\r\n]*(?:-Recurse|-r\b)|"
    r"\b(?:rmdir|rd)\s+/s\b|\bdel\s+/[sq]\b)"
)
SHELL_WRITE = (
    r"(?:\b(?:Set-Content|Add-Content|Out-File|New-Item|Remove-Item|Move-Item|Copy-Item)\b|"
    r"(?:^|[;&|]\s*)(?:rm|mv|cp|mkdir|touch|truncate|install)\b|"
    r"(?:^|[^<])(?:>>|>)\s*[^&])"
)
PROTECTED_PARTS = (
    ".git/",
    ".ba-agent/runtime",
    ".ba-agent/state",
    ".ba-agent/receipts",
    "ba-agent-control.sqlite",
)
APPROVAL_REQUIRED = (
    ("push", r"\bgit\b[^\r\n;&|]*\bpush\b"),
    ("merge", r"\bgit\b[^\r\n;&|]*\bmerge\b"),
    (
        "deploy",
        r"\b(?:kubectl\s+apply|helm\s+(?:install|upgrade)|terraform\s+apply|"
        r"(?:npm|pnpm|yarn)\s+(?:run\s+)?deploy)\b",
    ),
    (
        "privilege elevation",
        r"\b(?:sudo|doas|runas|pkexec)\b|\bStart-Process\b[^\r\n]*-Verb\s+RunAs\b",
    ),
)


def _read_event() -> dict[str, Any]:
    try:
        value = json.load(sys.stdin)
    except (json.JSONDecodeError, OSError):
        return {}
    return value if isinstance(value, dict) else {}


def _tool_text(event: dict[str, Any]) -> str:
    tool_input = event.get("tool_input", {})
    if isinstance(tool_input, dict):
        command = tool_input.get("command")
        if isinstance(command, str):
            return command
        try:
            return json.dumps(tool_input, ensure_ascii=False, sort_keys=True)
        except TypeError:
            return str(tool_input)
    return str(tool_input)


def _normalized(value: str) -> str:
    return value.replace("\\", "/").lower()


def _deny(event_name: str, reason: str) -> dict[str, Any]:
    if event_name == "PermissionRequest":
        return {
            "hookSpecificOutput": {
                "hookEventName": event_name,
                "decision": {"behavior": "deny", "message": reason},
            }
        }
    return {
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }
    }


def _managed_run() -> bool:
    return os.environ.get("BA_AGENT_MANAGED_RUN", "").lower() in {"1", "true", "yes"}


def _operation_approved() -> bool:
    return os.environ.get("BA_AGENT_OPERATION_APPROVED", "").lower() in {
        "1",
        "true",
        "yes",
    }


def _extract_candidate_paths(event: dict[str, Any], text: str) -> list[str]:
    tool_input = event.get("tool_input", {})
    if isinstance(tool_input, dict):
        explicit = [
            value
            for key in (
                "path",
                "file_path",
                "target_path",
                "destination",
                "notebook_path",
            )
            for value in (tool_input.get(key),)
            if isinstance(value, str) and value.strip()
        ]
        if explicit:
            return explicit

    patch_paths = re.findall(r"^\*\*\* (?:Add|Update|Delete) File:\s*(.+)$", text, re.MULTILINE)
    if patch_paths:
        return [item.strip() for item in patch_paths if item.strip()]
    parameter_paths = re.findall(
        r"(?:-LiteralPath|-Path|-Destination)\s+['\"]([^'\"\r\n]+)['\"]",
        text,
        re.IGNORECASE,
    )
    redirect_paths = re.findall(r">{1,2}\s*['\"]([^'\"\r\n]+)['\"]", text)
    if parameter_paths or redirect_paths:
        return [item.strip() for item in parameter_paths + redirect_paths if item.strip()]
    quoted = re.findall(r"['\"]([^'\"\r\n]+)['\"]", text)
    if re.search(r"\b(?:Set-Content|Add-Content|Out-File|New-Item)\b", text, re.IGNORECASE):
        quoted = quoted[:1]
    return [item.strip() for item in quoted if item.strip()]


def _write_intent(event: dict[str, Any], text: str) -> bool:
    tool_name = str(event.get("tool_name") or "")
    if tool_name in {"apply_patch", "Edit", "Write"}:
        return True
    if tool_name.startswith("mcp__"):
        lowered = tool_name.lower()
        return any(
            token in lowered
            for token in ("write", "edit", "create", "update", "delete", "move", "copy")
        )
    return bool(re.search(SHELL_WRITE, text, re.IGNORECASE))


def _outside_scope(candidate: str, cwd: Path) -> bool:
    if any(token in candidate for token in ("*", "$", "%", "${", "$(")):
        return True
    path = Path(candidate)
    if not path.is_absolute():
        path = cwd / path
    try:
        resolved = path.resolve(strict=False)
        resolved.relative_to(cwd.resolve(strict=False))
    except (OSError, ValueError):
        return True

    scopes = [item for item in os.environ.get("BA_AGENT_ALLOWED_PATHS", "").split(os.pathsep) if item]
    if not scopes:
        return False
    for scope in scopes:
        allowed = Path(scope)
        if not allowed.is_absolute():
            allowed = cwd / allowed
        try:
            resolved.relative_to(allowed.resolve(strict=False))
            return False
        except (OSError, ValueError):
            continue
    return True


def _policy_reason(event: dict[str, Any]) -> str | None:
    text = _tool_text(event)
    lowered = _normalized(text)
    tool_name = str(event.get("tool_name") or "")
    write_intent = _write_intent(event, text)
    for pattern in DANGEROUS_GIT:
        if re.search(pattern, text, re.IGNORECASE):
            return "BA Agent blocked a destructive or forceful Git command."

    cwd = Path(str(event.get("cwd") or os.getcwd()))
    if re.search(DELETE_COMMAND, text, re.IGNORECASE):
        candidates = _extract_candidate_paths(event, text)
        if not candidates or any(_outside_scope(item, cwd) for item in candidates):
            return "BA Agent blocked a recursive or out-of-scope delete."

    if not _managed_run():
        return None

    if not _operation_approved():
        if re.search(DELETE_COMMAND, text, re.IGNORECASE):
            return "BA Agent requires recorded human approval before deletion."
        for operation, pattern in APPROVAL_REQUIRED:
            if re.search(pattern, text, re.IGNORECASE):
                return f"BA Agent requires recorded human approval before {operation}."
        lowered_tool = tool_name.lower()
        for operation in ("push", "merge", "deploy", "delete", "elevate"):
            if tool_name.startswith("mcp__") and operation in lowered_tool:
                return f"BA Agent requires recorded human approval before {operation}."

    if write_intent and any(part in lowered for part in PROTECTED_PARTS):
        return "BA Agent protected state and receipts must be changed through the control plane."

    if write_intent and not tool_name.startswith("mcp__"):
        candidates = _extract_candidate_paths(event, text)
        if not candidates:
            return "BA Agent could not verify the managed write target against its path scope."
        for candidate in candidates:
            if _outside_scope(candidate, cwd):
                return "BA Agent blocked a write outside the task's declared path scope."

    if re.search(r"\bgit\s+commit\b", text, re.IGNORECASE):
        if os.environ.get("BA_AGENT_GATE_STATUS", "").lower() != "passed":
            return "BA Agent blocked commit creation until required quality gates pass."
    return None


def _context(event_name: str) -> dict[str, Any] | None:
    if event_name in {"SessionStart", "SubagentStart", "PostCompact"}:
        return {
            "hookSpecificOutput": {
                "hookEventName": event_name,
                "additionalContext": (
                    "For BA Agent managed runs, honor the declared write scope, "
                    "route destructive or remote operations to approval, and do not "
                    "commit before quality gates pass. Hooks are not the security boundary."
                ),
            }
        }
    if event_name in {"PreCompact", "SubagentStop", "Stop"}:
        return {"continue": True}
    return None


def main() -> int:
    event = _read_event()
    event_name = str(event.get("hook_event_name") or "")
    if event_name in {"PreToolUse", "PermissionRequest"}:
        reason = _policy_reason(event)
        if reason:
            json.dump(_deny(event_name, reason), sys.stdout)
        return 0
    payload = _context(event_name)
    if payload is not None:
        json.dump(payload, sys.stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
