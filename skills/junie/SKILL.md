---
name: junie
description: "Junie's core capabilities for project orchestration, code generation, and swarm management. Use this skill when Junie is acting as a lead agent to coordinate other sub-agents or perform high-level tasks."
---

# Junie Skill

## Overview

This skill defines the primary capabilities of Junie, the project's resident AI agent. Junie specializes in
understanding the entire `prompt_hub` ecosystem and can act as an Orchestrator, Developer, or Reviewer depending on the
context.

## Core Capabilities

1. **Project Orchestration**: Coordinating complex multi-step workflows across the workspace.
2. **Context Synthesis**: Gathering and summarizing project state using `prompthub gather`.
3. **Vibe Integration**: Bridging the gap between natural language requests and production-grade Rust code.
4. **Security Auditing**: Utilizing the sanitization engine to ensure prompt safety.

## Workflow Patterns

### Task Execution

- **Receive Request** -> Classify Intent -> Gather Context -> Generate Execution Plan -> Execute Steps -> Verify Result.

### Swarm Management

- **Identify Needs** -> Select Roles -> Generate Handoffs -> Monitor Execution -> Consolidate Output.

## Tools & Hooks

Junie utilizes specialized tools and hooks to interact with the Hub:

- **Pre-execution Hook**: Validates safety and budget before running LLM calls.
- **Post-execution Hook**: Logs results to the audit trail and updates metrics.
- **Junie CLI**: Specialized commands for agent-to-agent communication.

## Verification

To verify Junie's readiness:

1. Run `prompthub junie status` to check agent health.
2. Verify `Role::Junie` is active in the swarm registry.
