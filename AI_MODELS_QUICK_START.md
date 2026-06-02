# 🤖 Quick Setup: GitHub Actions with AI Models

Get started with AI-assisted development in 5 minutes.

## ⚡ Quick Start

### 1️⃣ GitHub Models (Automatic - No Setup!)

✅ **Already works!** Your workflows can use GitHub Models with just your `GITHUB_TOKEN`.

No additional secrets or configuration needed.

```yaml
permissions:
  models: read

steps:
  - run: |
      curl -X POST https://models.github.ai/inference/chat/completions \
        -H "Authorization: Bearer $GITHUB_TOKEN" \
        -d '{"model":"openai/gpt-4o","messages":[...]}'
```

**Available models**: GPT-4o, Claude, DeepSeek, Llama 2, and more.

---

### 2️⃣ Claude (Anthropic API) - Optional

To enable Claude code reviews:

1. Sign up: https://console.anthropic.com
2. Get API key from https://console.anthropic.com/account/billing
3. Add to GitHub:
   - Repo settings → Secrets and variables → Actions
   - New secret: `ANTHROPIC_API_KEY` = your key
4. Done! Workflows can now use Claude

**Cost**: ~$0.01-0.10 per code review

---

### 3️⃣ Devin.ai (Optional)

To enable Devin.ai code generation:

1. Sign up: https://devin.ai
2. Get API key from settings
3. Add to GitHub:
   - Repo settings → Secrets and variables → Actions  
   - New secret: `DEVIN_API_KEY` = your key
4. Done!

**Cost**: Check https://devin.ai/pricing (beta)

---

## 🎬 Run Your First Workflow

### Option A: GitHub Models (Free, Immediate)

1. Go to **Actions** tab in your repo
2. Select **Code Review with GitHub Models**
3. Click **Run workflow**
4. Push a PR or trigger manually
5. Check PR comments for AI review ✨

### Option B: Multi-Model Comparison (Free, Interesting)

1. Go to **Actions** → **Multi-Model Evaluation**
2. Click **Run workflow**
3. See outputs from GPT-4o, Claude, DeepSeek in one place
4. Compare and choose best suggestions

### Option C: Claude Deep Review (Optional, Paid)

1. Setup ANTHROPIC_API_KEY (see above)
2. Go to **Actions** → **External AI APIs**
3. Click **Run workflow** → Choose `claude`
4. Get detailed architectural analysis

---

## 📊 Available Workflows

| Workflow | Trigger | Models | Cost |
|----------|---------|--------|------|
| **ai-code-review.yml** | On PR | GPT-4o, Claude | FREE |
| **multi-model-evaluation.yml** | Manual | GPT-4o, Claude, DeepSeek | FREE |
| **external-ai-apis.yml** | Manual or `ai-review` label | Claude API, Devin | Paid† |
| **ai-test-doc-generation.yml** | On PR | GPT-4o, Claude, DeepSeek | FREE |
| **ai-safety-deployment.yml** | PR ready for review | GPT-4o, Claude | FREE |

† Requires API keys configured in GitHub Secrets

---

## 🛠️ Configuration Checklist

- [ ] ✅ GitHub Models ready (automatic)
- [ ] Optional: Claude API key added to secrets
- [ ] Optional: Devin.ai API key added to secrets
- [ ] Workflows file downloaded and visible in `.github/workflows/`
- [ ] Ready to run workflows!

---

## 🚀 What Happens Next

When you open a PR:

1. **ai-code-review.yml** runs → Posts code review as comment
2. **ai-test-doc-generation.yml** runs → Generates test/doc artifacts
3. **ai-safety-deployment.yml** runs → Security & performance check

When you manually trigger:

1. **multi-model-evaluation.yml** → Compare 4 AI models
2. **external-ai-apis.yml** → Use Claude API or Devin

---

## 💡 Pro Tips

### Monitor API Usage
- GitHub Models: Free, included with GitHub
- Claude (Anthropic): Monitor at https://console.anthropic.com/account/billing
- Set budget alerts to avoid surprises

### Review AI Output
- Always review generated code before using
- AI is a tool, not a replacement for human review
- Run tests: `cargo test --all-features`
- Check clippy: `cargo clippy`

### Optimize Costs
- Only run on specific triggers (not every commit)
- Limit code size sent to APIs
- Use cheaper models for simple tasks
- Cache results in artifacts

### Troubleshooting
- Check workflow logs: **Actions** tab → workflow → logs
- Verify secrets spelling: must match exactly in code
- Ensure branch has access to secrets (only default branch)
- Run with `workflow_dispatch` to debug

---

## 📚 Learn More

- [GitHub Actions README](.github/workflows/README.md) - Detailed docs
- [GitHub Models Quickstart](https://docs.github.com/en/github-models/quickstart)
- [GitHub Models API Reference](https://docs.github.com/en/rest/models/inference)
- [Project Prompts](prompts/README.md) - Reusable prompt library
- [Agent Configuration](.instructions.md) - Coding standards

---

## ✨ What You Get

✅ **Automated code reviews** from AI  
✅ **Test generation** - Save time writing tests  
✅ **Documentation generation** - Auto doc comments  
✅ **Security analysis** - Detect vulnerabilities  
✅ **Performance analysis** - Find bottlenecks  
✅ **Multi-model evaluation** - Choose best AI for task  
✅ **Deployment safety** - Pre-merge checks  

All integrated into your existing GitHub workflow!

---

## 🤝 Contributing

Found an issue? Want to improve workflows?

1. Edit the workflow file
2. Test with `workflow_dispatch`
3. Create a PR with improvements
4. Document changes in this file

---

**Questions?** See [.github/workflows/README.md](.github/workflows/README.md) for complete documentation.
