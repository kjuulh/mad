use async_trait::async_trait;
use rand::Rng;
use tokio_util::sync::CancellationToken;
use tracing::Level;

struct ErrorServer {}
#[async_trait]
impl notmad::Component for ErrorServer {
    fn name(&self) -> Option<String> {
        Some("ErrorServer".into())
    }

    async fn run(&self, cancellation: CancellationToken) -> Result<(), notmad::MadError> {
        let millis_wait = rand::thread_rng().gen_range(500..3000);

        tracing::debug!("waiting: {}ms", millis_wait);

        // Simulates a server running for some time. Is normally supposed to be futures blocking indefinitely
        tokio::time::sleep(std::time::Duration::from_millis(millis_wait)).await;

        Err(notmad::MadError::Inner(anyhow::anyhow!("expected error")))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::TRACE)
        .init();

    // Do note that only the first server which returns an error is guaranteed to be handled. This is because if servers don't respect cancellation, they will be dropped

    notmad::Mad::builder()
        .add(ErrorServer {})
        .add(ErrorServer {})
        .add(ErrorServer {})
        .add(ErrorServer {})
        .run()
        .await?;

    Ok(())
}
