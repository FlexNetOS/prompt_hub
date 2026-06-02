use prompt_hub::models::*;

#[tokio::main]
async fn main() {
    println!("╔══════════════════════════════════════════╗");
    println!("║     Vibe Coding Demo — PromptHub         ║");
    println!("╚══════════════════════════════════════════╝\n");

    // Simulate a non-technical user request
    let user_request = "Build me a blog with user auth and comments, dark mode, deploy to Vercel";

    println!("User Request: '{}'\n", user_request);

    // Step 1: Intent classification (simulated)
    println!("[Step 1] Classifying intent...");
    let intent = Intent {
        raw_text: user_request.to_string(),
        domain: Domain::DevOps,
        role: Role::Architect,
        task_type: TaskType::Create,
        complexity: Complexity::Moderate,
        urgency: Urgency::Medium,
        extracted_entities: [
            ("feature".to_string(), "blog, auth, comments, dark mode".to_string()),
            ("deploy_target".to_string(), "vercel".to_string()),
        ]
        .into(),
    };
    println!("  → Domain: {:?}", intent.domain);
    println!("  → Role: {:?}", intent.role);
    println!("  → Task Type: {:?}", intent.task_type);
    println!("  → Complexity: {:?}", intent.complexity);

    // Step 2: Skill recommendation (simulated)
    println!("\n[Step 2] Recommending skill...");
    println!("  → Best skill: 'fullstack-blog-scaffold'");
    println!("  → Confidence: 94%");
    println!("  → Used 42 times for similar requests");

    // Step 3: Generate execution plan
    println!("\n[Step 3] Generating execution plan...");
    let plan = ExecutionPlan {
        title: "Blog with Auth + Comments + Dark Mode".to_string(),
        description: "Full-stack blog application".to_string(),
        steps: vec![
            ExecutionStep {
                id: 1,
                description: "Set up Next.js project with TypeScript".to_string(),
                action: "create_project".to_string(),
                dependencies: vec![],
                estimated_duration_secs: 30,
            },
            ExecutionStep {
                id: 2,
                description: "Install and configure Auth.js for authentication".to_string(),
                action: "setup_auth".to_string(),
                dependencies: vec![1],
                estimated_duration_secs: 60,
            },
            ExecutionStep {
                id: 3,
                description: "Create blog post model and API routes".to_string(),
                action: "create_blog_api".to_string(),
                dependencies: vec![1],
                estimated_duration_secs: 90,
            },
            ExecutionStep {
                id: 4,
                description: "Build comment system with nested replies".to_string(),
                action: "create_comments".to_string(),
                dependencies: vec![3],
                estimated_duration_secs: 120,
            },
            ExecutionStep {
                id: 5,
                description: "Implement dark mode with next-themes".to_string(),
                action: "setup_dark_mode".to_string(),
                dependencies: vec![1],
                estimated_duration_secs: 45,
            },
            ExecutionStep {
                id: 6,
                description: "Create blog UI components (list, post, editor)".to_string(),
                action: "create_ui".to_string(),
                dependencies: vec![3, 5],
                estimated_duration_secs: 120,
            },
            ExecutionStep {
                id: 7,
                description: "Deploy to Vercel".to_string(),
                action: "deploy".to_string(),
                dependencies: vec![2, 4, 6],
                estimated_duration_secs: 60,
            },
        ],
        total_estimated_duration_secs: 525,
    };

    println!("  → Plan: {}", plan.title);
    println!("  → Steps: {}", plan.steps.len());
    println!("  → Estimated time: {}s", plan.total_estimated_duration_secs);

    for step in &plan.steps {
        println!(
            "     [{}] {} (deps: {:?}, ~{}s)",
            step.id, step.description, step.dependencies, step.estimated_duration_secs
        );
    }

    // Step 4: Cost estimation
    println!("\n[Step 4] Estimating cost...");
    let estimate = CostEstimate {
        tokens_input: 15000,
        tokens_output: 8000,
        cost_usd: 0.15,
        time_seconds: 525,
        confidence: 0.85,
    };
    println!("  → Input tokens: {}", estimate.tokens_input);
    println!("  → Output tokens: {}", estimate.tokens_output);
    println!("  → Cost: ${:.2}", estimate.cost_usd);
    println!("  → Time: ~{} min", estimate.time_seconds / 60);
    println!("  → Confidence: {:.0}%", estimate.confidence * 100.0);

    // Step 5: Show what would happen
    println!("\n[Step 5] Preview:");
    println!("  ╔══════════════════════════════════════════════════════════╗");
    println!("  ║  I'll create a Next.js blog with:                       ║");
    println!("  ║    ✓ TypeScript + Tailwind CSS                         ║");
    println!("  ║    ✓ Auth.js authentication (OAuth + email)            ║");
    println!("  ║    ✓ Blog posts with Markdown editor                    ║");
    println!("  ║    ✓ Nested comment system                             ║");
    println!("  ║    ✓ Dark mode toggle                                   ║");
    println!("  ║    ✓ Deployed to Vercel                                 ║");
    println!("  ║                                                         ║");
    println!("  ║  Estimated: 9 min  •  7 files  •  ~400 lines of code   ║");
    println!("  ║  Cost: ~$0.15                                           ║");
    println!("  ╚══════════════════════════════════════════════════════════╝");

    println!("\n✨ Demo complete! In full mode, this would execute automatically.");
}
