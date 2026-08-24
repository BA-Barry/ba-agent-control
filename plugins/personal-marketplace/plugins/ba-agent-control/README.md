# BA Agent Codex plugin

This package exposes BA Agent's 11 focused workflow Skills, lifecycle guard
hooks, eight canonical role templates, and local stdio MCP server to Codex. It
stores no model credentials and does not enable telemetry.

The MCP entry expects `ba-agent-control` on `PATH` and launches
`ba-agent-control mcp serve`. Codex discovers `hooks/hooks.json` at the standard
plugin path; every hook invokes the plugin-relative Python guard with Unix and
Windows command variants.

The eight files under `agents/` are canonical role templates consumed by BA
Agent's configuration generator. Codex does not discover plugin-root agent
files directly. `ba-config` produces `.codex/agents/` and `.claude/agents/`
artifacts from the same templates and refuses to replace modified managed
content.

Hooks are defense in depth. They do not replace the Codex sandbox, BA Agent's
approval queue, path canonicalization, or server-side authorization.
Managed runs set `BA_AGENT_OPERATION_APPROVED=1` only while executing a
recorded approval for push, merge, deploy, deletion, or privilege elevation.

See `docs/plugin.md` in the repository for local marketplace setup, generated
project paths, update behavior, and validation commands.
