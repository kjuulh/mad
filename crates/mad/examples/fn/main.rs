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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::TRACE)
        .init();

    mad::Mad::builder()
        .add(WaitServer {})
        .add_fn(|cancel| async move {
            let millis_wait = 50;

            tracing::debug!("waiting: {}ms", millis_wait);

            // Simulates a server running for some time. Is normally supposed to be futures blocking indefinitely
            tokio::time::sleep(std::time::Duration::from_millis(millis_wait)).await;

            Ok(())
        })
        .run()
        .await?;

    Ok(())
}
