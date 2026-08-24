# ADR 0001: local-first clean-room architecture

- Status: accepted
- Date: 2026-08-23

## Context

The product needs the orchestration behavior of a multi-agent development control plane while preserving local ownership, inspectability, and compatibility with already authenticated developer tools. TheAgency is an architectural inspiration, but its source, product identity, and bundled applications are outside this implementation.

## Decision

Build an independent Apache-2.0 Rust and React monorepo. Keep durable operational state in a global SQLite database and project-authored artifacts under `.ba-agent/`. Reuse Codex CLI authentication through a runtime adapter. Treat runtime capabilities as negotiated data. Require worktree isolation for writes and explicit approval for remote, destructive, deployment, and privilege-elevating actions.

## Consequences

- The initial implementation invests more in domain boundaries, migrations, compatibility fixtures, and receipts than a thin wrapper would.
- The console can remain a single local binary with no cloud dependency.
- Claude and future runtime support can share the domain model without claiming unsupported live capabilities.
- Clean-room records and NOTICE attribution must stay current.
