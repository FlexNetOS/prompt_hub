---
id: 019ea7b9-50da-7540-af8c-8d833b5a79cc
slug: tasks/lane-loop-handoff
title: "Wire session-handoff into prompt-loop harness"
type: task
status: active
priority: high
tags: [harness, handoff, session-relay, session-handoff]
---

## Overview
Copy `meta/weave/sessions-handoff` schemas/templates/docs into the prompt-loop skill directory. Then upgrade the harness to wire session-handoff at end-of-loops via session-relay, and refactor wiring into `/lane-loop`.

## Goals
1. Add `handoff/` subdirectory under `.claude/skills/prompt-loop/` with all sessions-handoff content (schemas, templates, docs) as harness reference material
2. Upgrade `session-relay/SKILL.md` to use the handoff packet schema and weave session events at loop end
3. Wire everything into a cohesive `/lane-loop` command path

## Acceptance Criteria
- [ ] `.claude/skills/prompt-loop/handoff/` exists with all schemas, templates, docs from weave/sessions-handoff
- [ ] `session-relay/SKILL.md` references handoff packet schema for checkpoint state
- [ ] End-of-loop session-relay produces a handoff packet (per `handoff.packet.v2` schema)
- [ ] All changes compile/gate clean (no prompt_hub code affected — harness files only)

## Progress Log
### 2026-06-08
- Task created per AGENTS.md discipline
- Working through all three sub-tasks in sequence
