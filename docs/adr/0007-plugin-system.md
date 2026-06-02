# ADR-0007: Plugin System

## Status
Accepted

## Decision
Dual loading: libloading (dynamic .so/.dll) + inventory (static registration).

## Plugin Types
- SearchBackend, Sanitizer, TemplateEngine, MetricExporter
- Fault isolation: catch_unwind per plugin
