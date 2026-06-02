use prompt_hub::confidence::ConfidenceScorer;

fn main() {
    let scorer = ConfidenceScorer {
        intent_clarity: 0.95,
        context_completeness: 0.80,
        skill_match: 0.90,
        historical_success: 0.85,
    };
    let score = scorer.score();
    println!("Confidence Scoring Demo:");
    println!("  Overall: {:.0}%", score.overall * 100.0);
    println!("  Requires confirmation: {}", score.requires_confirmation);
}
