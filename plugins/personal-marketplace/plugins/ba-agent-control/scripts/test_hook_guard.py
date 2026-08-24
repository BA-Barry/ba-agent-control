#!/usr/bin/env python3
"""Fixture-level tests for hook_guard.py using only the Python standard library."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("hook_guard.py")


def invoke(event: dict, env: dict[str, str] | None = None) -> dict:
    full_env = os.environ.copy()
    full_env.update(env or {})
    result = subprocess.run(
        [sys.executable, str(SCRIPT)],
        input=json.dumps(event),
        text=True,
        capture_output=True,
        check=True,
        env=full_env,
    )
    return json.loads(result.stdout) if result.stdout else {}


class HookGuardTests(unittest.TestCase):
    def test_blocks_force_push(self) -> None:
        output = invoke(
            {
                "hook_event_name": "PreToolUse",
                "cwd": os.getcwd(),
                "tool_input": {"command": "git push --force origin main"},
            }
        )
        self.assertEqual(
            output["hookSpecificOutput"]["permissionDecision"], "deny"
        )

    def test_blocks_force_push_with_git_global_options(self) -> None:
        output = invoke(
            {
                "hook_event_name": "PreToolUse",
                "cwd": os.getcwd(),
                "tool_input": {"command": "git -C . push origin main --force-with-lease"},
            }
        )
        self.assertEqual(
            output["hookSpecificOutput"]["permissionDecision"], "deny"
        )

    def test_permission_request_uses_permission_shape(self) -> None:
        output = invoke(
            {
                "hook_event_name": "PermissionRequest",
                "cwd": os.getcwd(),
                "tool_input": {"command": "git reset --hard HEAD~1"},
            }
        )
        self.assertEqual(
            output["hookSpecificOutput"]["decision"]["behavior"], "deny"
        )

    def test_managed_commit_needs_passed_gate(self) -> None:
        output = invoke(
            {
                "hook_event_name": "PreToolUse",
                "cwd": os.getcwd(),
                "tool_input": {"command": "git commit -m safe"},
            },
            {"BA_AGENT_MANAGED_RUN": "1", "BA_AGENT_GATE_STATUS": "failed"},
        )
        self.assertEqual(
            output["hookSpecificOutput"]["permissionDecision"], "deny"
        )

    def test_unmanaged_commit_is_not_intercepted(self) -> None:
        output = invoke(
            {
                "hook_event_name": "PreToolUse",
                "cwd": os.getcwd(),
                "tool_input": {"command": "git commit -m ordinary"},
            },
            {"BA_AGENT_MANAGED_RUN": "0"},
        )
        self.assertEqual(output, {})

    def test_managed_patch_outside_scope_is_blocked(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            output = invoke(
                {
                    "hook_event_name": "PreToolUse",
                    "cwd": folder,
                    "tool_input": {
                        "command": "*** Begin Patch\n*** Update File: docs/out.md\n"
                    },
                    "tool_name": "apply_patch",
                },
                {
                    "BA_AGENT_MANAGED_RUN": "1",
                    "BA_AGENT_ALLOWED_PATHS": "src",
                },
            )
        self.assertEqual(
            output["hookSpecificOutput"]["permissionDecision"], "deny"
        )

    def test_managed_patch_ignores_quoted_source_content(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            output = invoke(
                {
                    "hook_event_name": "PreToolUse",
                    "cwd": folder,
                    "tool_name": "apply_patch",
                    "tool_input": {
                        "command": (
                            "*** Begin Patch\n*** Add File: src/out.py\n"
                            "+print(\"not-a-path\")\n*** End Patch\n"
                        )
                    },
                },
                {
                    "BA_AGENT_MANAGED_RUN": "1",
                    "BA_AGENT_ALLOWED_PATHS": "src",
                },
            )
        self.assertEqual(output, {})

    def test_dynamic_write_path_is_blocked(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            output = invoke(
                {
                    "hook_event_name": "PreToolUse",
                    "cwd": folder,
                    "tool_name": "Bash",
                    "tool_input": {"command": "Set-Content '../$target' 'value'"},
                },
                {
                    "BA_AGENT_MANAGED_RUN": "1",
                    "BA_AGENT_ALLOWED_PATHS": "src",
                },
            )
        self.assertEqual(
            output["hookSpecificOutput"]["permissionDecision"], "deny"
        )

    def test_write_tool_uses_path_field_not_content_as_a_path(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            output = invoke(
                {
                    "hook_event_name": "PreToolUse",
                    "cwd": folder,
                    "tool_name": "Write",
                    "tool_input": {
                        "file_path": "src/out.txt",
                        "content": "quoted content outside every path scope",
                    },
                },
                {
                    "BA_AGENT_MANAGED_RUN": "1",
                    "BA_AGENT_ALLOWED_PATHS": "src",
                },
            )
        self.assertEqual(output, {})

    def test_unverifiable_managed_write_target_is_blocked(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            output = invoke(
                {
                    "hook_event_name": "PreToolUse",
                    "cwd": folder,
                    "tool_name": "Bash",
                    "tool_input": {"command": "Set-Content ../out.txt value"},
                },
                {
                    "BA_AGENT_MANAGED_RUN": "1",
                    "BA_AGENT_ALLOWED_PATHS": "src",
                },
            )
        self.assertEqual(
            output["hookSpecificOutput"]["permissionDecision"], "deny"
        )

    def test_managed_read_outside_scope_is_allowed(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            output = invoke(
                {
                    "hook_event_name": "PreToolUse",
                    "cwd": folder,
                    "tool_name": "Bash",
                    "tool_input": {"command": "Get-Content '../notes.txt'"},
                },
                {
                    "BA_AGENT_MANAGED_RUN": "1",
                    "BA_AGENT_ALLOWED_PATHS": "src",
                },
            )
        self.assertEqual(output, {})

    def test_managed_protected_read_is_allowed(self) -> None:
        output = invoke(
            {
                "hook_event_name": "PreToolUse",
                "cwd": os.getcwd(),
                "tool_name": "Bash",
                "tool_input": {"command": "Get-Content '.ba-agent/receipts/latest.json'"},
            },
            {"BA_AGENT_MANAGED_RUN": "1"},
        )
        self.assertEqual(output, {})

    def test_managed_delete_needs_recorded_approval(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            output = invoke(
                {
                    "hook_event_name": "PreToolUse",
                    "cwd": folder,
                    "tool_name": "Bash",
                    "tool_input": {"command": "Remove-Item -Recurse 'src/tmp'"},
                },
                {
                    "BA_AGENT_MANAGED_RUN": "1",
                    "BA_AGENT_ALLOWED_PATHS": "src",
                },
            )
        self.assertEqual(
            output["hookSpecificOutput"]["permissionDecision"], "deny"
        )

    def test_managed_in_scope_delete_allows_recorded_approval(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            output = invoke(
                {
                    "hook_event_name": "PreToolUse",
                    "cwd": folder,
                    "tool_name": "Bash",
                    "tool_input": {"command": "Remove-Item -Recurse 'src/tmp'"},
                },
                {
                    "BA_AGENT_MANAGED_RUN": "1",
                    "BA_AGENT_ALLOWED_PATHS": "src",
                    "BA_AGENT_OPERATION_APPROVED": "1",
                },
            )
        self.assertEqual(output, {})

    def test_session_start_adds_policy_context(self) -> None:
        output = invoke({"hook_event_name": "SessionStart"})
        self.assertIn(
            "declared write scope",
            output["hookSpecificOutput"]["additionalContext"],
        )


if __name__ == "__main__":
    unittest.main()
