# Contributing

BA Agent is currently developed as a local-first personal Alpha. Contributions should preserve its auditability, explicit approval boundaries, and clean-room implementation policy.

## Local checks

1. Format Rust with `cargo fmt --all`.
2. Run `cargo clippy --workspace --all-targets -- -D warnings`.
3. Run `cargo test --workspace`.
4. Run `pnpm validate` and the relevant Playwright tests for console changes.
5. Update architecture decisions and user-facing documentation when behavior changes.

Do not commit credentials, runtime databases, worktrees, detailed run logs, generated secrets, or third-party source copied from TheAgency.

All contributions are accepted under Apache-2.0.
