## Pull Request: [Brief Title]

### Description
<!-- Describe the changes in this PR -->

### Related Issues
<!-- Reference any related issues: Fixes #123, Related to #456 -->

### Type of Change
- [ ] Bug fix (non-breaking)
- [ ] New feature (non-breaking)
- [ ] Breaking change
- [ ] Documentation update
- [ ] Refactoring (no functional change)
- [ ] Performance improvement

### Changes Made
- 
- 
- 

---

## Testing

### Test Coverage
- [ ] Unit tests added/updated
- [ ] Integration tests added/updated
- [ ] Manual testing completed

### Test Results
```
<!-- Paste output of: cargo test --all-features -->
```

### Browser/Environment Testing (if applicable)
- [ ] Tested on Linux
- [ ] Tested on macOS
- [ ] Tested on Windows

---

## Code Quality

### Pre-Submission Checklist
- [ ] Code compiles: `cargo check --all-features`
- [ ] All tests pass: `cargo test --all-features`
- [ ] Clippy passes: `cargo clippy --all-targets --all-features`
- [ ] Code is formatted: `cargo fmt --all`
- [ ] No `unsafe` code added (or justified in comments)
- [ ] No new compiler warnings
- [ ] MSRV (1.91.1) verified: `cargo +1.91.1 check`

### Documentation
- [ ] Comments added for complex logic
- [ ] Public API documented (doc comments)
- [ ] Updated relevant .md files
- [ ] Updated CHANGELOG.md

### Security & Performance
- [ ] No hardcoded secrets
- [ ] No SQL injection vulnerabilities
- [ ] No data loss on panic/crash
- [ ] Performance impact assessed (if applicable)

---

## Dependencies
- [ ] No new external dependencies added
- [ ] All dependencies are up-to-date
- [ ] Feature flags documented

---

## Review Checklist (Maintainers)

- [ ] Code follows style guidelines
- [ ] Changes are backward compatible
- [ ] Tests adequately cover changes
- [ ] Documentation is clear and complete
- [ ] No circular dependencies introduced
- [ ] Error handling is appropriate
- [ ] All edge cases considered

---

## Additional Notes
<!-- Any other information reviewers should know -->

---

## Deployment Considerations
- [ ] Database migrations (if applicable) are backward compatible
- [ ] Feature can be safely rolled back
- [ ] No downtime required
- [ ] Feature flags properly implemented (if needed)
