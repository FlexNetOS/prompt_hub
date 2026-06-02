use prompt_hub::multimodal_input::MultiModalInput;
use prompt_hub::models::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let processor = MultiModalInput;
    let voice_input = UserInput {
        input_type: InputType::Voice,
        raw_data: vec![],
        extracted_text: "Build a login page with dark mode".to_string(),
    };
    let intent = processor.process(voice_input).await?;
    println!("Voice -> Intent: {:?}", intent.task_type);
    Ok(())
}
