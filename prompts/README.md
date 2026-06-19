# GitHub Models Prompts for prompt_hub

This directory contains reusable prompts stored in GitHub's standardized `.prompt.yml` format. These prompts are optimized for use with GitHub Models and other AI development tools integrated into GitHub.

## Benefits

- **Organized UI**: View and manage prompts directly in GitHub
- **Easy Sharing**: Share prompts with team members
- **Iterative Development**: Compare and test prompt variations
- **Integration**: Works seamlessly with GitHub's AI tooling
- **Version Control**: Track prompt changes through git history

## Available Prompts

### [code-review-rust.prompt.yml](code-review-rust.prompt.yml)
**Purpose**: Comprehensive code review for Rust changes

**Use when**:
- Submitting a pull request with Rust code
- Need to verify safety constraints (#![forbid(unsafe_code)])
- Want to check compilation, tests, and clippy compliance

**Input**: Your Rust code change
**Output**: Detailed review with pass/fail checks, issues, and suggested fixes

---

### [implement-feature.prompt.yml](implement-feature.prompt.yml)
**Purpose**: Guided workflow for implementing new features

**Use when**:
- Starting work on a new feature from the spec
- Need step-by-step implementation guidance
- Want to follow the 7-step pattern (types → storage → module → hub → API → CLI → tests)

**Input**: Feature description from SPEC.md
**Output**: Complete implementation plan with code examples

---

### [debug-compilation.prompt.yml](debug-compilation.prompt.yml)
**Purpose**: Systematic debugging of Rust compilation errors

**Use when**:
- Getting a compilation error you don't understand
- Need to identify the root cause quickly
- Want targeted fixes with verification steps

**Input**: Error message and code snippet
**Output**: Error analysis, solution, and verification commands

---

### [design-migration.prompt.yml](design-migration.prompt.yml)
**Purpose**: Design database schemas and SQL migrations

**Use when**:
- Need to add/modify database tables
- Want to ensure backward compatibility
- Planning a schema change with new Prompt features

**Input**: Schema requirement or feature needing database changes
**Output**: Migration SQL file, type definitions, and testing plan

---

### [design-api-endpoint.prompt.yml](design-api-endpoint.prompt.yml)
**Purpose**: Design and implement REST API endpoints

**Use when**:
- Creating a new HTTP endpoint for prompthub-server
- Need request/response types, handlers, and tests
- Want proper error handling and OpenAPI documentation

**Input**: Endpoint requirements (resources, operations, auth)
**Output**: Endpoint design, implementation code, and tests

---

### [env-state-convergence.prompt.yml](env-state-convergence.prompt.yml)
**Purpose**: Handle any task touching environment state in a declaratively-managed workspace

**Use when**:
- A task touches host config, the agent env (`.claude`/`.codex`), a plugin/marketplace cache,
  dotfiles, a daemon, a toolchain, or any path under a real home dir
- You're tempted to either hand-edit the runtime or punt env state as "off-limits host config"
- You need to decide whether a signal is real drift or stale/cosmetic

**Input**: The environment-state situation
**Output**: Verdict, declared source of truth, detect→declare→sync→lock convergence plan, and a
means-vs-outcome check. Doctrine companion: [`prompt-hub/templates/env_state_convergence.md`](../prompt-hub/templates/env_state_convergence.md)

---

## How to Use These Prompts

### Option 1: Via GitHub UI
1. Open this file in GitHub
2. Click on a `.prompt.yml` file
3. Use GitHub's "Ask GitHub Models" feature
4. Follow the prompt's structure and examples

### Option 2: Via GitHub CLI
```bash
gh models prompt -f prompts/code-review-rust.prompt.yml <your-code>
```

### Option 3: Via IDE Integration
If using an IDE with GitHub Models support, reference the prompt file path when asking for help.

### Option 4: Copy & Paste
Copy the `messages` section into your AI tool of choice (Claude, ChatGPT, etc.)

---

## Prompt Format

All prompts follow GitHub's standardized `.prompt.yml` format:

```yaml
name: Human-readable name
description: What this prompt does
model: openai/gpt-4o          # Model to use
modelParameters:
  temperature: 0.3             # 0=deterministic, 1=creative
  top_p: 0.9                   # Diversity of response
messages:
  - role: system               # System instructions
    content: |
      Detailed guidance...
  - role: user                 # User prompt template
    content: |
      Input description with {{placeholder}} variables
testData:
  - {{placeholder}}: "example value"
    expected: "expected output"
evaluators:
  - name: "Evaluation criteria"
    string:
      contains: "expected substring"
```

### Key Fields

- **name**: Display name in GitHub UI
- **description**: Brief explanation of purpose
- **model**: Which AI model to use (e.g., openai/gpt-4o)
- **modelParameters**:
  - `temperature`: Lower (0.2) = focused/deterministic, Higher (0.8) = creative
  - `top_p`: Nucleus sampling (0.9 = 90% most likely tokens)
- **messages**: Array of conversation turns
  - `system`: Instructions for the AI
  - `user`: The actual prompt with `{{variable}}` placeholders
- **testData**: Examples with inputs and expected outputs
- **evaluators**: Criteria to evaluate response quality

---

## Writing New Prompts

To create a new prompt:

1. **File naming**: `description.prompt.yml` in this directory
2. **Structure**: Copy an existing prompt as template
3. **Key sections**:
   - Clear system message explaining the task
   - User message with placeholders
   - Test examples showing inputs and outputs
   - Evaluators that verify response quality
4. **Commit**: Add to git with descriptive message

Example structure:
```yaml
name: Your Feature
description: What it does
model: openai/gpt-4o
modelParameters:
  temperature: 0.3
messages:
  - role: system
    content: Instructions for the AI
  - role: user
    content: |
      The actual prompt with {{variable}} placeholders
testData:
  - variable: "example input"
    expected: "expected output format"
evaluators:
  - name: "Checks for required content"
    string:
      contains: "key phrase"
```

---

## Limitations

These prompts cannot use:
- Complex templating languages (Jinja, Handlebars, etc.)
- Proprietary file formats
- Interactive dialogs
- File uploads or attachments

They support:
- Simple `{{variable}}` substitution
- Standard YAML syntax
- JSON in responses
- Multi-turn conversations

---

## Best Practices

1. **Be specific in system instructions** — The more detailed, the better the results
2. **Include examples in testData** — Shows the AI what good output looks like
3. **Use evaluators for quality gates** — Ensures the response meets your requirements
4. **Keep prompts focused** — One task per prompt, not multiple unrelated tasks
5. **Version control** — Update prompts in git when they change

---

## Contributing

To improve these prompts:

1. Identify what's not working well
2. Create a new version or update the existing one
3. Test with examples from testData
4. Commit with a clear message: `prompt: improve code-review for better error detection`
5. Document the change in commit message

---

## See Also

- [GitHub Models Documentation](https://docs.github.com/en/github-models)
- [Storing Prompts in GitHub Repositories](https://docs.github.com/en/github-models/use-github-models/storing-prompts-in-github-repositories)
- [Project .instructions.md](../.instructions.md) — Coding standards
- [SPEC.md](../SPEC.md) — Complete feature specification
