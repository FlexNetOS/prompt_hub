#![forbid(unsafe_code)]
use anyhow::Result;

pub async fn run(request: &str) -> Result<()> {
    println!("Preview: '{}'...", request);
    println!("  (Preview generates a plan without executing)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn preview_run_succeeds() {
        // `preview` is a non-executing placeholder; it must succeed for any
        // request string without touching the store.
        run("build a CLI").await.expect("preview run");
    }
}
