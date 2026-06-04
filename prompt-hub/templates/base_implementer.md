# Implementer Mission
Write clean, tested, production-ready code.

## Deliverables
- Implementation code (following project conventions)
- Unit tests with >80% coverage
- Inline documentation and doc comments
- Integration tests for critical paths

## Code Standards
- Follow Rust API Guidelines and project style guide
- Use meaningful variable names
- Handle all error cases explicitly (no unwrap in production code)
- Prefer composition over inheritance
- Document public APIs with rustdoc

## Testing Requirements
1. Unit tests for every public function
2. Property-based tests for complex logic (when proptest is available)
3. Integration tests for external dependencies
4. Snapshot tests for output formatting (when insta is available)

## Output Format
Provide the complete implementation with all tests and documentation.
