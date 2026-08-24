# BA Agent plugin and shared configuration

The repository contains a self-contained local marketplace at
`plugins/personal-marketplace`. Its `personal` catalog exposes the
`ba-agent-control` plugin without publishing or copying it outside the checkout.

## Bundle contents

- `.codex-plugin/plugin.json` — Codex manifest for BA Agent `0.1.0-alpha.1`.
- `skills/` — 11 workflow Skills: define, design, plan, workstream, dispatch,
  quality gate, review, handoff, health, release, and initialization.
- `agents/` — eight canonical role templates used by the shared generator.
- `hooks/hooks.json` and `scripts/hook_guard.py` — cross-platform lifecycle
  context and defense-in-depth checks.
- `.mcp.json` — local stdio server descriptor for
  `ba-agent-control mcp serve`.

Hooks are not the authorization boundary. The server, path canonicalization,
worktree isolation, approval records, Codex sandbox, and quality-gate state
remain authoritative.

For managed runs, the server sets `BA_AGENT_MANAGED_RUN=1`, declares write
scopes through `BA_AGENT_ALLOWED_PATHS`, reports gate state through
`BA_AGENT_GATE_STATUS`, and exposes `BA_AGENT_OPERATION_APPROVED=1` only while
executing an operation backed by a current approval record. The hook guard
fails closed for managed push, merge, deploy, delete, or privilege-elevation
commands without that signal.

## Add the local marketplace

Build `ba-agent-control` and make the binary available on `PATH`. Then add the
non-default local marketplace root with an absolute path:

```powershell
$marketplaceRoot = (Resolve-Path plugins/personal-marketplace).Path
codex plugin marketplace add "$marketplaceRoot"
```

The marketplace entry resolves
`./plugins/ba-agent-control` relative to that marketplace root. In the Codex
app, open the plugin directory and install **BA Agent**. Start a new task after
install or upgrade so its Skills, hooks, and MCP server are loaded into fresh
context.

This repository-local marketplace is intentionally separate from the default
personal file at `~/.agents/plugins/marketplace.json`; do not copy or hand-edit
the user's default marketplace as part of development.

## Shared Codex and Claude generation

`ba-config` loads the canonical roles, Skills, hooks, and hook guard from the
plugin bundle into one `ProjectConfig`. `ProjectConfig::generate` produces the
following plans:

| Shared source | Codex target | Claude target |
| --- | --- | --- |
| Project instructions | managed block in `AGENTS.md` | managed block in `CLAUDE.md` |
| Eight role templates | `.codex/agents/*.toml` | `.claude/agents/*.md` |
| 11 Skills | `.agents/skills/*/SKILL.md` | `.claude/skills/*/SKILL.md` |
| Hook descriptor | `.codex/hooks.json` | `.claude/settings.json` |
| Hook guard | `.ba-agent/generated/hooks/hook_guard.py` | same shared local file |

The hook source retains Codex's `commandWindows` and context-limit fields. The
Claude renderer selects the host-specific command and removes those Codex-only
keys before merging `.claude/settings.json`.

Generation is a plan, not an implicit write. The caller reads the current file,
passes it with an `Artifact` to `merge_artifact`, reviews the returned outcome,
and performs an authorized atomic write only after a successful merge.

Managed blocks include their identifier and SHA-256 checksum. A subsequent
generation replaces a block only when the checksum still matches its current
body. BA Agent-owned TOML, Markdown, and script files use an embedded file
checksum and reject updates after user edits. JSON settings are recursively
additive: absent keys are inserted, equal keys are retained, and an unequal
existing value produces a conflict with its JSON Pointer. No merge path silently
overwrites user-owned content.

Claude support here is configuration compatibility only. The Alpha does not
claim or provide live Claude dispatch. Inspect the explicit capability matrix:

```powershell
ba-agent-control --json adapter compatibility claude `
  --plugin-root plugins/personal-marketplace/plugins/ba-agent-control
```

External Codex or Claude state can be imported as immutable evidence with
`migrate runtime-state`. The command defaults to dry-run, validates the v1
schema and status vocabulary, writes canonical JSON under
`.ba-agent/imports/runtime-state/<runtime>/` only with `--apply`, and records the
Artifact plus permanent Receipt in one SQLite transaction. It never advances a
BA Agent Task or Run state. See [Runtime compatibility](runtime-compatibility.md).

## Validation

Run the BA Agent bundle and hook tests:

```powershell
python plugins/personal-marketplace/plugins/ba-agent-control/scripts/validate_bundle.py
python plugins/personal-marketplace/plugins/ba-agent-control/scripts/test_hook_guard.py
```

The installed `plugin-creator` Skill currently provides scaffolding but no
separate plugin validator. `validate_bundle.py` therefore validates the manifest,
required plugin paths, role count, Skill count, identifiers, and local
marketplace entry. Do not document or invoke the removed
`validate_plugin.py` path.

Run the official Skill validator once for every directory under `skills/`:

```powershell
Get-ChildItem plugins/personal-marketplace/plugins/ba-agent-control/skills -Directory |
  ForEach-Object {
    python "$env:USERPROFILE/.codex/skills/.system/skill-creator/scripts/quick_validate.py" $_.FullName
  }
```

After the crate is a workspace member, verify the shared generator and merge
logic with:

```powershell
cargo test -p ba-config
cargo clippy -p ba-config --all-targets -- -D warnings
cargo fmt --all -- --check
```

The plugin structure and lifecycle hook formats track the current official
[Codex plugin packaging](https://developers.openai.com/plugins/build/plugins)
and [Codex hooks](https://developers.openai.com/codex/hooks/) documentation.
