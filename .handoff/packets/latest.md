# Handoff Packet (latest) — handoff.packet.v2

> Compiled by `hf fleet render prompt_hub` from the FLEET ledger (meta/.handoff) + this repo's git-text capsule/cards. Not rendered from a per-repo ledger (ADR-0004 §3).

## 1. North Star (prompt_hub)
A non-technical user makes any request; prompt_hub transforms, communicates, and delivers it as intended (SwarmBundle -> handoff.task.v1).

## 2. State Precedence
Git > FLEET ledger (meta/.handoff/ledger.db) > tasks/*.task.json > this packet.

## 3. Progress
Done: 53/71.  FLEET tamper-evident events verified: 732.

## 4. Remaining
- [P2] **PHTASK-0048** — Complete search ONNX embedder real inference + model fetch/verify
- [P2] **PHTASK-0055** — Complete voice STT/TTS backends behind the FSM
- [P1] **PHTASK-0056** — EPIC — expose remaining ~125 hub capabilities over HTTP
- [P1] **PHTASK-0057** — Routes: prompt lifecycle (11 capabilities)
- [P2] **PHTASK-0058** — Routes: context gathering + lineage (11 capabilities)
- [P1] **PHTASK-0059** — Routes: provider health/multi-provider + rollout/deploy/rollback (18)
- [P2] **PHTASK-0060** — Routes: cost-limits + beta + quota + moderation (14)
- [P2] **PHTASK-0061** — Routes: audit/SOC2/diff + retention/GC + auto-purge (23)
- [P3] **PHTASK-0062** — Routes: voice + local-llm + analytics + accessibility (16)
- [P3] **PHTASK-0063** — Routes: sandbox + malware + offline/sync/mobile + chaos + multimodal/i18n + preview + pollination + quality-gate (remaining ~32)
- [P1] **PHTASK-0064** — CI: add Default-Features Build + Test-Compile to branch-protection required checks
- [P3] **PHTASK-0065** — Hygiene: retire Python drift (generate_cards.py) + resolve unused libloading dep
- [P1] **PHTASK-0066** — Complete LockManager TTL sweep (expired locks never deleted)
- [P2] **PHTASK-0067** — Complete CLI JunieCommand::Task execution (print-only stub)
- [P3] **PHTASK-0068** — Complete or formally retire the TUI (run_tui is a stub)
- [P2] **PHTASK-0069** — Wire TLS/mTLS server path or retire the hollow `tls` feature
- [P3] **PHTASK-0070** — Wire HF tokenizers counting backend or retire the hollow `tokenizers` feature
- [P3] **PHTASK-0071** — Reconcile SPEC-promised but absent feature flags: sqlcipher + ffi

