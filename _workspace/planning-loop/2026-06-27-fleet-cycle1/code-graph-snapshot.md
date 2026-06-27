# Code-graph snapshot + diff · cycle 2026-06-27-fleet-cycle1

## Status: FAIL-CLOSED — the fleet code graph is NOT indexed
`git kb code symbols --json` returns `{"count":0}` for the spine repos (verified in meta_cli, exit 0, 0 symbols);
`lifeos/.git/gitkb/code.db` exists with 0 indexed symbols (baseline `[L3]`, still true). The semantic call-graph
(`callers`/`callees`/`flows`/`impact`) is therefore **unavailable** for this fleet at this HEAD.

## Fallback used (declared, not hidden)
Structural edges in `docs/plans/meta-fleet-integration.md` are derived from:
1. `Cargo.toml [dependencies]` **path-deps** (compile edges),
2. `.meta.yaml depends_on` (declared edges, cross-checked vs #1),
3. grepped `run_plugin` / `Command::new` / `ExecutionPlan` **call-sites** (runtime dispatch edges).

→ Wiring confidence HIGH; call-graph-internal (who-calls-whom *within* a crate) confidence MEDIUM.

## Diff vs previous
No prior fleet-level graph snapshot exists (this is cycle 1 of the fleet loop; the rusty-idd baseline mapped a 16-component
subset without a code graph). Baseline → this cycle delta: +harness-agent-rs (execution layer), +shimmy, +n8n,
+flexnetos_runner/github_app rows; meta_plugin_api flagged dead; network-control "composes ruvector" corrected to "lane only";
LifeOS upgraded from "skeleton" to "real app + durable AI runtime + chosen ruvector mirror".

## Recommendation (for the next cycle)
Run `git kb code index` across the in-scope Rust repos before I2 so the map gets real call-graph edges and `kb_impact`
blast-radius instead of dependency-edge fallback. This is the single biggest fidelity upgrade available to the loop.
