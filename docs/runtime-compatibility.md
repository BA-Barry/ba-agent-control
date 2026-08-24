# Runtime compatibility

BA Agent uses one canonical source for project instructions, eight narrow roles, 11 Skills, Hooks, and the local hook guard. Renderers produce runtime-specific configuration while the BA Agent domain state and receipt chain remain runtime-neutral.

## Capability report

```powershell
ba-agent-control --json adapter compatibility claude `
  --plugin-root plugins/personal-marketplace/plugins/ba-agent-control
```

The Claude Alpha report marks project instructions, roles, Skills, Hooks, and structured status import as supported. It explicitly marks live dispatch, realtime BA Agent events, and interactive BA Agent approvals as unsupported. Generated configuration is not presented as runtime parity.

## Runtime state snapshot v1

The fixture [claude-runtime-state-v1.json](fixtures/claude-runtime-state-v1.json) is a synthetic reproducible example. The top-level schema is:

```json
{
  "schemaVersion": 1,
  "runtime": "claude",
  "capturedAt": "2026-08-24T09:00:00Z",
  "tasks": [
    { "externalId": "task-1", "title": "Review", "status": "completed" }
  ],
  "runs": [
    { "externalId": "run-1", "taskExternalId": "task-1", "status": "succeeded" }
  ]
}
```

Task status values use BA Agent's documented Task vocabulary; Run values use its Run vocabulary. External identifiers must be non-empty, unique within their entity type, no longer than 256 characters, and free of control characters. Every referenced external task must be present. `capturedAt` must be UTC-compatible RFC3339 and the input is limited to 10 MiB.

## Dry-run and apply

```powershell
ba-agent-control --json migrate runtime-state `
  docs/fixtures/claude-runtime-state-v1.json <PROJECT_ID>

ba-agent-control --json migrate runtime-state `
  docs/fixtures/claude-runtime-state-v1.json <PROJECT_ID> --apply
```

Dry-run performs all parsing, status, reference, project, path, and digest validation without writing. Apply canonicalizes the JSON, addresses the repository file by SHA-256, and creates an Artifact and forward-hash-chained Receipt in one SQLite transaction. Reimporting the same digest is idempotent.

Imported status is evidence, not authority: it never changes an existing BA Agent Task or Run. Reconciliation into an authoritative workflow requires a later explicit design and approval rather than silently manufacturing lifecycle transitions.
