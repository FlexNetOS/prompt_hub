# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial workspace with 3 crates: prompt-hub, prompthub, prompthub-server
- 49 library modules covering MVP through Tier 5 automation
- libsql database backend with 9 migrations
- FAST/SMART/Hybrid search with FTS5 and vector similarity
- RBAC auth with argon2id
- Vibe Coding engine for natural-language-to-deliverable
- 12 new feature modules: circuit_breaker, budget, quota, moderation, retention, garbage_collector, load_balancer, provider_health, satisfaction, analytics, diff, lineage
- 36 CLI commands with real implementations
- HTTP API server with OpenAPI, rate limiting, structured responses
- 595 test functions across 49 modules
- Docker multi-stage build with distroless
- CI/CD with 10 GitHub Actions jobs
