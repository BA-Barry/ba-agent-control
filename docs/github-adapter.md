# Optional GitHub adapter

BA Agent's GitHub integration is local-first and read-only in the personal Alpha. It can report authentication capability and inspect repository metadata, but it cannot create repositories, push, merge, publish releases, register credentials, or perform another remote mutation.

## Credential precedence

The adapter tries these sources in strict order:

1. An installed and authenticated `gh` session for `github.com`.
2. A PAT already stored in the operating-system credential store under service `ba-agent-control/github` and user `github.com`.
3. `GITHUB_TOKEN` from the current BA Agent process.

When probing `gh`, BA Agent removes GitHub token environment variables from that child process. This prevents `gh` from silently turning the third-priority environment source into the first-priority result. BA Agent never calls `gh auth status --show-token`.

The adapter has no credential-write API. Adding or replacing a PAT remains a separately approved user action. The primary temporary token buffer is zeroized on drop; short-lived authorization headers are never logged. Credentials are never included in `Debug`, JSON, database records, logs, or control descriptors.

## Commands

```powershell
ba-agent-control --json adapter probe github
ba-agent-control --json adapter repository openai/codex
```

The equivalent protected loopback endpoints are:

- `GET /api/v1/adapters`
- `GET /api/v1/adapters/github/repository?repository=owner/name`

Repository identifiers accept exactly two conservative path segments (`owner/name`) and reject path traversal, query strings, and additional URL components.

## Remote-write boundary

The capability report always sets `remoteWrites` to `false` and `remoteWritesRequireApproval` to `true`. A future push, merge, repository-creation, or release workflow must add an Approval entity and an explicit remote-mutation implementation; this Alpha adapter cannot bypass that design boundary.
