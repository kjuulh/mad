use notmad::ComponentInfo;
use rand::Rng;
use tokio_util::sync::CancellationToken;
use tracing::Level;

struct WaitServer {}
impl notmad::Component for WaitServer {
    fn info(&self) -> ComponentInfo {
        "WaitServer".into()
    }

    async fn run(&self, _cancellation: CancellationToken) -> Result<(), notmad::MadError> {
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

    let item = "some item".to_string();

    notmad::Mad::builder()
        .add(WaitServer {})
        .add_fn(|_cancel| async move {
            let millis_wait = 50;

            tracing::debug!("waiting: {}ms", millis_wait);

            // Simulates a server running for some time. Is normally supposed to be futures blocking indefinitely
            tokio::time::sleep(std::time::Duration::from_millis(millis_wait)).await;

            Ok(())
        })
        .add_fn(move |_cancel| {
            // I am an actual closure

            let item = item.clone();

            async move {
                let _item = item;

                let millis_wait = 50;

                tracing::debug!("waiting: {}ms", millis_wait);

                // Simulates a server running for some time. Is normally supposed to be futures blocking indefinitely
                tokio::time::sleep(std::time::Duration::from_millis(millis_wait)).await;

                Ok(())
            }
        })
        .run()
        .await?;

    Ok(())
}
