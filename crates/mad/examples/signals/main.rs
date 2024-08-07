use async_trait::async_trait;
use rand::Rng;
use tokio_util::sync::CancellationToken;
use tracing::Level;

struct WaitServer {}
#[async_trait]
impl mad::Component for WaitServer {
    fn name(&self) -> Option<String> {
        Some("WaitServer".into())
    }

    async fn run(&self, cancellation: CancellationToken) -> Result<(), mad::MadError> {
        let millis_wait = rand::thread_rng().gen_range(500..3000);

        tracing::debug!("waiting: {}ms", millis_wait);

        // Simulates a server running for some time. Is normally supposed to be futures blocking indefinitely
        tokio::time::sleep(std::time::Duration::from_millis(millis_wait)).await;

        Ok(())
    }
}

struct RespectCancel {}
#[async_trait]
impl mad::Component for RespectCancel {
    fn name(&self) -> Option<String> {
        Some("RespectCancel".into())
    }

    async fn run(&self, cancellation: CancellationToken) -> Result<(), mad::MadError> {
        cancellation.cancelled().await;
        tracing::debug!("stopping because job is cancelled");

        Ok(())
    }
}

struct NeverStopServer {}
#[async_trait]
impl mad::Component for NeverStopServer {
    fn name(&self) -> Option<String> {
        Some("NeverStopServer".into())
    }

    async fn run(&self, cancellation: CancellationToken) -> Result<(), mad::MadError> {
        // Simulates a server running for some time. Is normally supposed to be futures blocking indefinitely
        tokio::time::sleep(std::time::Duration::from_millis(999999999)).await;

        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::TRACE)
        .init();

    mad::Mad::builder()
        .add(WaitServer {})
        .add(NeverStopServer {})
        .add(RespectCancel {})
        .run()
        .await?;

    Ok(())
}
