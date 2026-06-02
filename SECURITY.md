# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

Please report security vulnerabilities to security@prompthub.dev.

Do NOT open public issues for security bugs.

We will respond within 48 hours and release a patch within 7 days for critical issues.

## Security Features

- `#![forbid(unsafe_code)]` — zero unsafe code
- argon2id password hashing
- SHA-256 audit tamper evidence
- Prompt injection detection (5+ heuristics)
- RBAC with AgentIdentity
- sqlcipher encryption at rest (optional)
- TLS/mTLS support (optional)
