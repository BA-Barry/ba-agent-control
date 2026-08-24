# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Rust 2024 Edition for the local control plane and single binary; React, TypeScript, Vite, Tailwind CSS v4, Radix primitives, TanStack Router/Query/Table, and Phosphor icons for the embedded web console. This stack was explicitly selected in the approved implementation plan.

## Users

BA Agent serves advanced individual developers who run several local repositories and need to plan, dispatch, inspect, review, and recover AI-assisted development work without handing project state to a cloud service. The primary working environment is a desktop browser opened by a local CLI on Windows, WSL2, Linux, or macOS.

## Product Purpose

BA Agent is a local-first research-and-development control plane. It turns project goals into workstreams and tasks, dispatches isolated Codex runs, keeps approvals and quality gates visible, and preserves verifiable receipts so a developer can understand what happened and safely continue after a restart. Success means that one person can coordinate four concurrent agents across three repositories, isolate writes, approve risky actions, pass quality gates, commit locally, and recover the complete state without data loss.

## Positioning

BA Agent combines an auditable local domain model, worktree-enforced write isolation, human approval boundaries, and hash-chained quality receipts in one control plane while reusing the developer's authenticated Codex CLI. It is not a chat wrapper and does not require model credentials or cloud synchronization.

## Operating Context

Developers register existing Git repositories, create milestones and dependent tasks, assign narrow agent roles, and dispatch work. Read work may share a checkout; write work starts in a detached worktree and declares a path scope. Runs stream structured events into the console, pause for approvals, execute project-defined quality gates, and create a local branch and commit only after gates pass. Project-owned configuration and durable workflow artifacts live under `.ba-agent/`; operational data lives in the OS data directory.

## Capabilities and Constraints

- Chinese is the default interface language, with complete English resources.
- The Alpha reuses Codex CLI 0.130.0 or newer. App Server is preferred, with a visible `codex exec --json` capability fallback.
- Claude support in Alpha is limited to shared configuration generation, compatibility reports, and state import; live Claude dispatch is out of scope.
- The service binds only to `127.0.0.1`, emits no telemetry by default, stores no model credentials, and provides no embedded interactive terminal.
- Push, merge, deploy, deletion, and privilege elevation always require explicit human approval.
- Runtime events are append-only and sequenced. Detailed logs default to 90-day retention; quality receipts are permanent and hash-chained.
- Team accounts, cloud synchronization, direct model API access, and public hosting are out of scope for the personal Alpha.
- Public GitHub repository creation, credential registration, and public release require separate user confirmation.

## Brand Commitments

The display name is **BA Agent**. Repository, crate, binary, and plugin identifiers use `ba-agent-control`. The voice is quiet, precise, and operational: it names state, risk, and the next action without hype. The project is an Apache-2.0 clean-room implementation inspired by TheAgency's architectural ideas; it does not reuse TheAgency code, trademarks, or bundled applications.

## Evidence on Hand

The approved implementation plan is the sole product authority at project start. There are no customer testimonials, production benchmarks, logos, screenshots, or commercial claims; future surfaces must not fabricate them. Demonstration records may be synthetic only when clearly labeled.

## Product Principles

- Local state is the source of truth and remains inspectable, portable, and recoverable.
- Automation may advance routine work, but risk boundaries stay explicit and human-controlled.
- Every completed change carries evidence: events, gate results, artifacts, and a verifiable receipt.
- Familiar developer-tool conventions outrank decorative novelty; density serves decisions.
- Runtime differences are represented as capabilities, never hidden behind false parity.

## Accessibility & Inclusion

The console must meet WCAG 2.2 AA for the Public Beta, including keyboard navigation, managed focus, screen-reader semantics, sufficient contrast, reduced motion, responsive layouts, and equivalent Chinese and English task flows.
