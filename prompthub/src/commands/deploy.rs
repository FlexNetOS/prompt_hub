#![forbid(unsafe_code)]
use anyhow::Result;
use uuid::Uuid;

pub async fn run(artifact_id: Uuid, safe: bool) -> Result<()> {
    println!("Deploying {} (safe={})...", artifact_id, safe);
    println!("  (Deploy requires artifact generation first — use 'vibe')");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn deploy_run_succeeds() {
        // `deploy` is a placeholder pending artifact generation; it must
        // succeed for any artifact id / safe flag without touching the store.
        run(Uuid::nil(), true).await.expect("deploy run");
    }
}
