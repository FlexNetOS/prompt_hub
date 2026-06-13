---
name: lane-loop
description: "Unified harness entry point for prompt-loop dev loop + session-relay handoff. Combines autonomous feature building with durable session-to-session handoff via Handoff Packet V2. ALWAYS use as the single /lane-loop slash command — delegates to prompt-loop for building and session-relay for handoff. Triggers: '/lane-loop', 'harness loop', 'dev loop'. Defaults to APPLY mode."
---

# Lane-Loop — Unified Harness Entry Point

`/lane-loop` is the **single entry point** for all autonomous prompt_hub development work. It wires together three components into one cohesive harness:

| Component | Skill | Role |
|-----------|-------|------|
| **prompt-loop** | `prompt-loop` | The construction crew — discovers backlog, builds features, verifies across boundaries |
| **session-relay** | `session-relay` | Durable handoff between sessions — produces Handoff Packet V2 at each cycle boundary |
| **handoff** | `prompt-loop/handoff/` | Reference schemas & templates from weave/sessions-handoff — session events, packet schema, task cards, policy gates |

## Invocation

```
Skill(skill: "lane-loop")      # invoke the harness
Skill(skill: "lane-loop", args: "safe")     # local-only mode
Skill(skill: "lane-loop", args: "resume")   # resume from HANDOFF.md
```

## Delegation Model

`/lane-loop` does NOT reimplement prompt-loop or session-relay. It delegates:

1. **Build work** → `prompt-loop` skill (Phase 0 DISCOVER → Phase 2 cycle → Phase 3 DONE)
2. **Handoff at budget** → `session-relay` skill HAND OFF mode
3. **Resume from checkpoint** → `session-relay` skill RESUME mode

## End-of-Loop Handoff Flow (applied via session-relay)

At the end of each cycle, before HAND OFF:

```
[Build cycle complete]
    │  all _workspace artifacts persist
    ▼
[session-relay HAND OFF trigger]
    ├── Emit session_stopped event (handoff.session_event.v1)
    ├── Compile Handoff Packet V2 from Git state + gate results
    ├── Spawn continuity-steward → write HANDOFF.md (packet as markdown block)
    ├── git add/commit _workspace/HANDOFF.md backlog.md loop_state.md
    ├── Best-effort mesh heartbeat relay:handoff
    └── Write sentinel (HANDOFF/DONE/NEEDS-HUMAN)
```

### Handoff Packet V2 compilation

At handoff time, compile from current state:

```bash
# Gather inputs
git diff --name-only HEAD~1     # changed_files
cargo test --workspace 2>&1 | tail -5   # tests + commands results
git log -1 --format="%H"       # git_sha (proof of commit)
cat _workspace/loop_state.md  # cycles_total, last_item
```

Then produce the packet JSON per `.claude/skills/prompt-loop/handoff/schemas/packet.schema.json` and embed it in `HANDOFF.md`.

### Session Event emission

At handoff boundary, emit to mesh (best-effort):

```bash
# Session stopped event
notify_peer("all", "relay:handoff — cycle N completed, sentinel=HANDOFF")

# Or if DONE:
notify_peer("all", "relay:handoff — loop DONE, all gates green")
```

## Policy Gates (from handoff/policies/rules.toml)

Applied at each cycle boundary:

| Gate | Default | Effect |
|------|---------|--------|
| `require_checkpoint` | true | Handoff fails without compiled packet |
| `require_test_evidence` | true | At least one gate result must show pass |
| `require_drift_audit` | true | drift_report.status always recorded |
| `require_next_command` | true | next_command non-null (except in DONE) |
| `default_write_mode` | deny_without_claim | Agents write only within claimed paths |

## Lane-Loop vs Prompt-Loop

| | `/lane-loop` | `/prompt-loop` |
|--|-------------|----------------|
| Entry point | Unified harness | Feature builder only |
| Handoff | Built-in (v2 packet) | Text checkpoint |
| Session events | Auto-emitted | N/A |
| Policy gates | Enforced at boundary | Manual |
| Delegation | prompt-loop + session-relay | Direct crew management |

**Use `/lane-loop`** when you want the full harness with durable handoff.  
**Use `/prompt-loop` directly** when you're managing a single continuous session and don't need handoff.

---

*Wired into harness via session-handoff from weave/sessions-handoff.*
*Handoff packet schema: .claude/skills/prompt-loop/handoff/schemas/packet.schema.json*
*Session event schema: .claude/skills/prompt-loop/handoff/schemas/session.schema.json*
