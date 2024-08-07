use anyhow::anyhow;
use async_trait::async_trait;
use mad::{Component, Mad};
use rand::Rng;
use tokio_util::sync::CancellationToken;
use tracing_test::traced_test;

struct NeverEndingRun {}

#[async_trait]
impl Component for NeverEndingRun {
    fn name(&self) -> Option<String> {
        Some("NeverEndingRun".into())
    }

    async fn run(&self, cancellation: CancellationToken) -> Result<(), mad::MadError> {
        let millis_wait = rand::thread_rng().gen_range(50..1000);

        tokio::time::sleep(std::time::Duration::from_millis(millis_wait)).await;

        return Err(mad::MadError::Inner(anyhow!("failed to run stuff")));

        Ok(())
    }
}

#[tokio::test]
#[traced_test]
async fn test_can_run_components() -> anyhow::Result<()> {
    Mad::builder()
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .run()
        .await?;

    anyhow::bail!("stuff");

    Ok(())
}
