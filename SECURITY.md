# Security policy

BA Agent is pre-release software. Do not expose its local HTTP service to a network interface or use it to authorize production deployment without independent review.

## Security boundaries

- The server binds only to `127.0.0.1` and validates the session token, Origin, CSRF token, CSP, and filesystem paths.
- Codex sandboxing remains the primary execution boundary. Hooks add policy guardrails but are not a sandbox.
- Model credentials are never stored by BA Agent.
- Push, merge, deploy, deletion, and privilege elevation require explicit approval.

Report vulnerabilities privately to the repository owner until a public security contact is established. Do not include secrets, personal data, or active exploit material in a public issue.
