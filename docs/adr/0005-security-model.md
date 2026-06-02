# ADR-0005: Security Model

## Status
Accepted

## Decision
RBAC with AgentIdentity, argon2id tokens, SHA-256 audit chain.

## Rationale
- Role-based access: Read, Write, Admin, SwarmOnly
- Token hashing never stores plaintext
- Tamper-evident audit logs for compliance
- 5+ sanitizer heuristics for prompt injection detection
