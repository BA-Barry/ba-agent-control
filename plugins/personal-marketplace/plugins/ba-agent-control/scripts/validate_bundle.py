#!/usr/bin/env python3
"""Validate BA Agent-specific plugin invariants with the standard library."""

from __future__ import annotations

import json
from pathlib import Path
import sys
import tomllib


PLUGIN_ROOT = Path(__file__).resolve().parents[1]
MARKETPLACE_ROOT = PLUGIN_ROOT.parents[1]
MARKETPLACE_PATH = MARKETPLACE_ROOT / ".agents" / "plugins" / "marketplace.json"
EXPECTED_ROLES = 8
EXPECTED_SKILLS = 11
SUPPORTED_HOOK_EVENTS = {
    "SessionStart",
    "SubagentStart",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "PreCompact",
    "PostCompact",
    "UserPromptSubmit",
    "SubagentStop",
    "Stop",
    "SessionEnd",
}


def load_json(path: Path) -> dict:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path} must contain a JSON object")
    return value


def validate() -> list[str]:
    errors: list[str] = []
    manifest = load_json(PLUGIN_ROOT / ".codex-plugin" / "plugin.json")
    marketplace = load_json(MARKETPLACE_PATH)
    mcp = load_json(PLUGIN_ROOT / ".mcp.json")
    hooks = load_json(PLUGIN_ROOT / "hooks" / "hooks.json")

    if manifest.get("name") != PLUGIN_ROOT.name:
        errors.append("plugin folder and manifest name differ")
    if "hooks" in manifest:
        errors.append("default hooks/hooks.json discovery should not be duplicated in plugin.json")
    if "[TODO:" in json.dumps(manifest):
        errors.append("plugin manifest contains a scaffold TODO")

    entries = marketplace.get("plugins")
    if not isinstance(entries, list):
        errors.append("marketplace plugins must be an array")
    else:
        matching = [
            entry
            for entry in entries
            if isinstance(entry, dict) and entry.get("name") == PLUGIN_ROOT.name
        ]
        if len(matching) != 1:
            errors.append("marketplace must contain exactly one BA Agent entry")
        else:
            source = matching[0].get("source", {})
            source_path = source.get("path")
            if source.get("source") != "local" or not isinstance(source_path, str):
                errors.append("marketplace BA Agent source must be local")
            elif (MARKETPLACE_ROOT / source_path).resolve() != PLUGIN_ROOT:
                errors.append("marketplace source path does not resolve to the plugin")
            policy = matching[0].get("policy", {})
            if not policy.get("installation") or not policy.get("authentication"):
                errors.append("marketplace entry is missing installation/authentication policy")

    servers = mcp.get("mcpServers")
    server = servers.get("ba-agent-control") if isinstance(servers, dict) else None
    if not isinstance(server, dict):
        errors.append("MCP descriptor is missing the ba-agent-control server")
    elif server.get("command") != "ba-agent-control" or server.get("args") != ["mcp", "serve"]:
        errors.append("MCP descriptor must run `ba-agent-control mcp serve`")

    hook_map = hooks.get("hooks")
    if not isinstance(hook_map, dict):
        errors.append("hook descriptor must contain a hooks object")
    else:
        unknown_events = set(hook_map) - SUPPORTED_HOOK_EVENTS
        if unknown_events:
            errors.append(f"unsupported hook events: {', '.join(sorted(unknown_events))}")
        for event, groups in hook_map.items():
            if not isinstance(groups, list) or not groups:
                errors.append(f"hook event {event} must have at least one matcher group")
                continue
            for group in groups:
                handlers = group.get("hooks") if isinstance(group, dict) else None
                if not isinstance(handlers, list) or not handlers:
                    errors.append(f"hook event {event} has no command handlers")
                    continue
                for handler in handlers:
                    if not isinstance(handler, dict):
                        errors.append(f"hook event {event} has a malformed handler")
                        continue
                    if handler.get("type") != "command":
                        errors.append(f"hook event {event} has a non-command handler")
                    command = handler.get("command", "")
                    windows = handler.get("commandWindows", "")
                    if "hook_guard.py" not in command or "hook_guard.py" not in windows:
                        errors.append(f"hook event {event} does not invoke the guard cross-platform")

    role_paths = sorted((PLUGIN_ROOT / "agents").glob("*.toml"))
    if len(role_paths) != EXPECTED_ROLES:
        errors.append(f"expected {EXPECTED_ROLES} roles, found {len(role_paths)}")
    for path in role_paths:
        role = tomllib.loads(path.read_text(encoding="utf-8"))
        for field in ("name", "description", "developer_instructions"):
            if not isinstance(role.get(field), str) or not role[field].strip():
                errors.append(f"{path.name} has no non-empty {field}")
        if role.get("sandbox_mode") not in {"read-only", "workspace-write"}:
            errors.append(f"{path.name} must declare a bounded sandbox_mode")

    skill_roots = sorted(path for path in (PLUGIN_ROOT / "skills").iterdir() if path.is_dir())
    if len(skill_roots) != EXPECTED_SKILLS:
        errors.append(f"expected {EXPECTED_SKILLS} skills, found {len(skill_roots)}")
    for root in skill_roots:
        skill_path = root / "SKILL.md"
        agent_path = root / "agents" / "openai.yaml"
        if not skill_path.is_file() or not skill_path.read_text(encoding="utf-8").startswith("---\n"):
            errors.append(f"{root.name} has no valid SKILL.md entrypoint")
        if not agent_path.is_file():
            errors.append(f"{root.name} has no agents/openai.yaml")
        elif f"${root.name}" not in agent_path.read_text(encoding="utf-8"):
            errors.append(f"{root.name} default prompt must name ${root.name}")

    return errors


def main() -> int:
    try:
        errors = validate()
    except (OSError, ValueError, json.JSONDecodeError, tomllib.TOMLDecodeError) as error:
        print(f"bundle validation failed: {error}", file=sys.stderr)
        return 1
    if errors:
        print("bundle validation failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print(
        f"bundle validation passed: {PLUGIN_ROOT.name} "
        f"({EXPECTED_ROLES} roles, {EXPECTED_SKILLS} skills)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
