use std::sync::Arc;

use notmad::{Component, Mad, MadError};
use rand::Rng;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing_test::traced_test;

struct NeverEndingRun {}

impl Component for NeverEndingRun {
    fn name(&self) -> Option<String> {
        Some("NeverEndingRun".into())
    }

    async fn run(&self, _cancellation: CancellationToken) -> Result<(), notmad::MadError> {
        let millis_wait = rand::thread_rng().gen_range(50..1000);

        tokio::time::sleep(std::time::Duration::from_millis(millis_wait)).await;

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

    Ok(())
}

#[tokio::test]
#[traced_test]
async fn test_can_shutdown_gracefully() -> anyhow::Result<()> {
    let check = Arc::new(Mutex::new(None));

    Mad::builder()
        .add_fn({
            let check = check.clone();

            move |cancel| {
                let check = check.clone();

                async move {
                    let start = std::time::SystemTime::now();
                    tracing::info!("waiting for cancel");
                    cancel.cancelled().await;
                    tracing::info!("submitting check");
                    let mut check = check.lock().await;
                    let elapsed = start.elapsed().expect("to be able to get elapsed");
                    *check = Some(elapsed);
                    tracing::info!("check submitted");

                    Ok(())
                }
            }
        })
        .add_fn(|_| async move {
            tracing::info!("starting sleep");
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            tracing::info!("sleep ended");

            Ok(())
        })
        .run()
        .await?;

    let check = check
        .lock()
        .await
        .expect("to be able to get a duration from cancel");

    // We default wait 100 ms for graceful shutdown, and we explicitly wait 100ms in the sleep routine
    tracing::info!("check millis: {}", check.as_millis());
    assert!(check.as_millis() < 250);

    Ok(())
}

#[tokio::test]
#[traced_test]
async fn test_component_panics_shutdowns_cleanly() -> anyhow::Result<()> {
    let res = Mad::builder()
        .add_fn({
            move |_cancel| async move {
                panic!("my inner panic");
            }
        })
        .add_fn(|cancel| async move {
            cancel.cancelled().await;

            Ok(())
        })
        .run()
        .await;

    let err_content = res.unwrap_err().to_string();
    assert!(err_content.contains("component panicked"));
    assert!(err_content.contains("my inner panic"));

    Ok(())
}

#[test]
fn test_can_easily_transform_error() -> anyhow::Result<()> {
    fn fallible() -> anyhow::Result<()> {
        Ok(())
    }

    fn inner() -> Result<(), MadError> {
        fallible()?;

        Ok(())
    }

    inner()?;

    Ok(())
}
