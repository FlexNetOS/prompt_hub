# GitHub Actions with AI Models

This directory contains GitHub Actions workflows that integrate multiple AI models for code review, testing, documentation, and deployment safety checks.

## Enabling the AI workflows (opt-in)

The AI workflows (`ai-code-review`, `ai-safety-deployment`, `ai-test-doc-generation`,
`external-ai-apis`, `multi-model-evaluation`) are **disabled by default**. Every job
is gated on the repository variable `ENABLE_AI_WORKFLOWS`, so without it they **skip
cleanly (green)** instead of failing CI when AI access/credentials are unavailable.

To enable:

1. Set the repository **variable** `ENABLE_AI_WORKFLOWS` to `true`
   (`Settings → Secrets and variables → Actions → Variables`, or
   `gh variable set ENABLE_AI_WORKFLOWS --body true`).
2. Provide the credentials each workflow needs:

   | Workflow | Requires | Notes |
   |----------|----------|-------|
   | `ai-code-review`, `ai-safety-deployment`, `ai-test-doc-generation`, `multi-model-evaluation` | GitHub Models | Uses the built-in `GITHUB_TOKEN` + `models: read`; the org/repo must have **GitHub Models enabled**. |
   | `external-ai-apis` (Claude job) | secret `ANTHROPIC_API_KEY` | Anthropic API. |
   | `external-ai-apis` (Devin job) | secret `DEVIN_API_KEY` | Devin API. |

   Set secrets with `gh secret set ANTHROPIC_API_KEY` / `gh secret set DEVIN_API_KEY`.

To disable again, unset the variable (or set it to anything other than `true`).

## Available Workflows

### 1. **Code Review with GitHub Models** (`ai-code-review.yml`)

**When it triggers**: 
- On pull request (opened, synchronize, reopened)
- Manual trigger via workflow_dispatch

**What it does**:
- Reviews Rust code changes with GPT-4o
- Checks for safety, types, error handling, and tests
- Posts review as PR comment

**Models used**:
- `openai/gpt-4o` - Code review
- `claude/claude-opus` - Feature suggestions

**Permissions needed**: `models: read`

---

### 2. **Multi-Model Evaluation** (`multi-model-evaluation.yml`)

**When it triggers**:
- On pull request (opened, synchronize)
- Manual trigger with model selection

**What it does**:
- Evaluates code with 4 different models in parallel
- GitHub Models (GPT-4o) - General review
- Claude (via GitHub Models) - Architectural analysis
- OpenAI (via GitHub Models) - Code completion suggestions
- DeepSeek (via GitHub Models) - Performance analysis

**Models used**:
- `openai/gpt-4o`
- `claude/claude-opus`
- `deepseek/deepseek-coder`

**Output**: Workflow logs contain detailed analysis from each model

---

### 3. **External AI APIs** (`external-ai-apis.yml`)

**When it triggers**:
- Manual trigger with model choice (Claude, Devin, or both)
- PR labeled with `ai-review`

**What it does**:
- Claude (Anthropic) - Deep code review via Anthropic API
- Devin.ai - Automated test generation

**Models used**:
- `claude-opus-4-1` (Anthropic API)
- Devin.ai API (code generation)

**Secrets required**:
- `ANTHROPIC_API_KEY` - Anthropic API key
- `DEVIN_API_KEY` - Devin.ai API key

**Output**: PR comments and generated test artifacts

---

### 4. **Test & Doc Generation** (`ai-test-doc-generation.yml`)

**When it triggers**:
- On pull request (opened, synchronize)
- Manual trigger via workflow_dispatch

**What it does**:
- Generates unit tests with GPT-4o
- Generates documentation with Claude
- Generates examples with DeepSeek
- Uploads artifacts for review

**Models used**:
- `openai/gpt-4o` - Unit tests
- `claude/claude-opus` - Documentation
- `deepseek/deepseek-coder` - Examples

**Output**: 
- Artifacts: generated tests, docs, and examples
- PR comment with links to artifacts

---

### 5. **Safety & Deployment Check** (`ai-safety-deployment.yml`)

**When it triggers**:
- On PR ready for review
- Manual trigger with severity level (lenient/normal/strict)

**What it does**:
- Security analysis (GPT-4o)
- Unsafe code detection
- Secret pattern detection
- Performance impact analysis (Claude)
- Deployment readiness check

**Models used**:
- `openai/gpt-4o` - Security analysis
- `claude/claude-opus` - Performance analysis

**Output**:
- Security report with issues and fixes
- Performance recommendations
- Deployment readiness checklist

---

## Setup Instructions

### GitHub Models (Built-in, No Setup Required)

GitHub Models works out-of-the-box with your `GITHUB_TOKEN`:

1. Workflows automatically use `${{ secrets.GITHUB_TOKEN }}`
2. Add `permissions: models: read` to your workflow
3. Call endpoint: `https://models.github.ai/inference/chat/completions`

**No additional setup needed!** Your existing GitHub token has access to all GitHub Models.

---

### Claude (Anthropic API)

To use Claude models:

1. **Get API Key**:
   - Go to [https://console.anthropic.com](https://console.anthropic.com)
   - Create an account or sign in
   - Navigate to API keys
   - Create a new API key

2. **Add Secret to GitHub**:
   - Go to your repository settings
   - Navigate to **Secrets and variables** → **Actions**
   - Click **New repository secret**
   - Name: `ANTHROPIC_API_KEY`
   - Paste your Claude API key

3. **Trigger workflow**:
   - Go to **Actions** tab
   - Select **External AI APIs**
   - Click **Run workflow**
   - Choose `claude` or `both`

**Cost**: Anthropic charges per token. Monitor usage at https://console.anthropic.com/account/billing

---

### Devin.ai

To use Devin.ai for code generation:

1. **Get API Key**:
   - Go to [https://devin.ai](https://devin.ai)
   - Sign up and authenticate
   - Navigate to API settings
   - Generate API key

2. **Add Secret to GitHub**:
   - Repository settings → **Secrets and variables** → **Actions**
   - Click **New repository secret**
   - Name: `DEVIN_API_KEY`
   - Paste your Devin API key

3. **Trigger workflow**:
   - Go to **Actions** tab
   - Select **External AI APIs**
   - Click **Run workflow**
   - Choose `devin` or `both`

**Note**: Devin.ai API is still in development. Update endpoint and payload based on their current API documentation.

---

## Available Models (GitHub Models)

GitHub Models provides free API access to:

| Model | Provider | Use Case |
|-------|----------|----------|
| `openai/gpt-4o` | OpenAI | General purpose, code review |
| `openai/gpt-4-turbo` | OpenAI | Complex analysis |
| `openai/gpt-3.5-turbo` | OpenAI | Fast, simple tasks |
| `claude/claude-opus` | Anthropic | Detailed analysis, architecture |
| `claude/claude-haiku` | Anthropic | Quick tasks, cheap |
| `deepseek/deepseek-coder` | DeepSeek | Code generation, analysis |
| `meta/llama-2` | Meta | Text generation |
| `mistral/mistral-large` | Mistral | Multi-lingual, reasoning |
| `cohere/command-r-plus` | Cohere | Long-form text generation |

See [GitHub Marketplace: Models](https://github.com/marketplace?type=models) for complete list.

---

## API Reference

### GitHub Models Endpoint

```bash
curl -X POST https://models.github.ai/inference/chat/completions \
  -H "Authorization: Bearer YOUR_GITHUB_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "openai/gpt-4o",
    "messages": [
      {"role": "system", "content": "..."},
      {"role": "user", "content": "..."}
    ],
    "temperature": 0.3,
    "max_tokens": 1000
  }'
```

### Parameters

- **model**: Model identifier (e.g., `openai/gpt-4o`)
- **messages**: Array of message objects with `role` and `content`
- **temperature**: 0-1, higher = more creative (default 0.7)
- **max_tokens**: Maximum response length
- **top_p**: Nucleus sampling (0-1)

### Response Format

```json
{
  "choices": [
    {
      "message": {
        "role": "assistant",
        "content": "Response text..."
      }
    }
  ],
  "usage": {
    "prompt_tokens": 123,
    "completion_tokens": 456
  }
}
```

---

## Workflow Variables & Secrets

### Environment Variables

Add to `.github/workflows/`:
```yaml
env:
  GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
  ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}  # Only for Claude workflows
  DEVIN_API_KEY: ${{ secrets.DEVIN_API_KEY }}          # Only for Devin workflows
```

### GitHub Secrets to Configure

| Secret | Used For | How to Get |
|--------|----------|-----------|
| `GITHUB_TOKEN` | GitHub Models (built-in) | Automatic with every GitHub repo |
| `ANTHROPIC_API_KEY` | Claude via Anthropic API | https://console.anthropic.com/account/billing |
| `DEVIN_API_KEY` | Devin.ai code generation | https://devin.ai (still in beta) |

---

## Cost Estimation

### GitHub Models
- **Cost**: FREE for GitHub users
- No additional fees, API calls included
- Call limits apply (check GitHub Models documentation)

### Anthropic Claude
- **Cost**: Pay-per-token
- Opus 4.1: $15/MTok input, $60/MTok output
- Monitor at: https://console.anthropic.com/account/billing
- **Estimate**: $0.01-$0.10 per code review

### Devin.ai
- **Cost**: Check https://devin.ai/pricing
- Still in beta, pricing may change

---

## Best Practices

### 1. Limit API Calls
```yaml
# Only run on PR opens, not every push
on:
  pull_request:
    types: [opened]  # Not: [opened, synchronize]
```

### 2. Batch Operations
```yaml
# Generate multiple artifacts in parallel
jobs:
  test-generation:
    # ...
  doc-generation:
    # ...
  example-generation:
    # ...
```

### 3. Cache Results
```yaml
# Store generated content as artifacts
- uses: actions/upload-artifact@v4
  with:
    name: generated-tests
    retention-days: 7
```

### 4. Review Before Using
- Always manually review AI-generated code
- Run tests before merging
- Check for security issues
- Verify documentation accuracy

### 5. Token Limits
- Limit request size: `head -100` code snippets
- Set reasonable `max_tokens` (1000-2000)
- Handle API timeouts gracefully

---

## Troubleshooting

### Workflow doesn't trigger
- Check `on:` conditions in workflow file
- Verify branches and paths match
- Check "Actions" tab for logs

### API returns 401 Unauthorized
- Verify `GITHUB_TOKEN` is being passed correctly
- Check `permissions:` block in workflow
- For external APIs, verify secret name spelling

### Generated content is poor quality
- Adjust `temperature` (lower = focused, higher = creative)
- Improve system prompt instructions
- Add examples in the prompt
- Try different models

### Artifacts not created
- Check job succeeded in Actions tab
- Verify `upload-artifact` action syntax
- Check file paths exist before uploading

---

## Examples

### Run a quick code review
```bash
# Manually trigger workflow
gh workflow run ai-code-review.yml
```

### View workflow logs
```bash
# See latest run
gh run list --workflow ai-code-review.yml --limit 1

# View detailed output
gh run view <RUN_ID> --log
```

### Download generated artifacts
```bash
# List artifacts from a run
gh run download <RUN_ID> -p generated-unit-tests
```

---

## Related Documentation

- [GitHub Models Quickstart](https://docs.github.com/en/github-models/quickstart)
- [GitHub Models API Reference](https://docs.github.com/en/rest/models/inference)
- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [Storing Prompts in GitHub](https://docs.github.com/en/github-models/use-github-models/storing-prompts-in-github-repositories)
- [Project Prompts](../prompts/README.md)
- [Agent Configuration](.instructions.md)

---

## Contributing

To add new workflows:

1. Create a new YAML file in `.github/workflows/`
2. Use existing workflows as templates
3. Test with `workflow_dispatch` first
4. Document in this README
5. Add to git and commit

---

## Security Notes

- ✅ GitHub tokens are automatically scoped to repository
- ✅ External API keys are stored as repository secrets (encrypted)
- ✅ API calls use HTTPS encryption
- ✅ No code is sent to external services unless workflow explicitly does
- ⚠️ Always review AI-generated code before merging
- ⚠️ Monitor API spending on external services
- ⚠️ Verify API keys are never logged or committed

---

## License

These workflows are part of the prompt_hub project and follow the same license (Apache 2.0 / MIT).
