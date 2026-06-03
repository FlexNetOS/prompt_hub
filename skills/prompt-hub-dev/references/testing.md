# Testing Patterns

- **Unit Tests**: In-module `mod tests`. Use `tokio::test` for async.
- **Integration Tests**: In `tests/`.
- **Mocks**: Avoid complex mocking; use in-memory `libsql`.
