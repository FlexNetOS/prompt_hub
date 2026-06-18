# Skill: session-resume

prompt_hub (FLEET member) resume skill, adapted from the kernel reference.
MEMBER MODEL: the packet is compiled centrally by `hf fleet render prompt_hub`.

## Purpose

Rehydrate current project state for a new agent session with minimal context load.

## Trigger phrases

- resume
- continue
- pick up
- what next
- recover session

## Steps

1. Run `cd <meta-root> && hf fleet render prompt_hub` then read `.handoff/packets/latest.md` (or `hf resume`).
2. Read `.handoff/active.md`.
3. Read `.handoff/context/capsule.json`.
4. Read the top unblocked card under `.handoff/tasks/` (status:backlog, lowest priority number).
5. Read the latest ADR(s) under `.handoff/decisions/`.
6. Check the latest drift report (when `hf drift` lands — HFTASK-0005).
7. Print the exact next command.

## Hard rule

Do not edit files during this skill.
