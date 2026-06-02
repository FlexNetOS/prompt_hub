# ADR-0008: Vibe Coding Architecture

## Status
Accepted

## Decision
Intent-to-deliverable pipeline with confidence scoring and auto-confirmation.

## Pipeline
Intent -> Classify -> Skill Select -> Extract Vars -> Inject Defaults -> Generate -> Test -> Deploy -> Summarize

## Key Features
- Auto-confirmation at >=80% confidence
- Fallback chain on failure
- Plain-English summaries for non-technical users
