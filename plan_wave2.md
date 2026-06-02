# Wave 2 Plan: Remaining 7% + Optional Features

## Loop 1: Core Gaps (4 agents parallel)
- Agent 1: audit_trail() storage-backed implementation + storage.rs methods
- Agent 2: UserProfile type + A/B testing canary engine + git-cliff in CI
- Agent 3: Tests for error.rs, lib.rs, models.rs + feature flag stub fills
- Agent 4: OpenAPI generation + CI/CD completion + Docker hardening

## Loop 2: Feature Completeness (4 agents parallel)
- Agent 5: Benchmark implementations + example quality
- Agent 6: Feature-gated module implementations (tiktoken, tokenizers, plugins)
- Agent 7: Security hardening + TLS + sqlcipher stub fills
- Agent 8: Documentation completion + runbooks + C4 diagrams

## Loop 3: Verification Swarm (5 agents parallel)
- Same 5 verifier types from wave 1, report honest results
- If any FAIL, dispatch fix agents and loop again
