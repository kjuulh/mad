# MAD 

Mad is a life-cycle manager for long running rust operations.

- Webservers
- Queue bindings
- gRPC servers etc
- Cron runners

It is supposed to be the main thing the application runs, and everything from it is spawned and managed by it.

```rust
struct WaitServer {}

#[async_trait]
impl Component for WaitServer {
    fn name(&self) -> Option<String> {
        Some("NeverEndingRun".into())
    }

    async fn run(&self, cancellation: CancellationToken) -> Result<(), mad::MadError> {
        let millis_wait = rand::thread_rng().gen_range(50..1000);

        // Simulates a server running for some time. Is normally supposed to be futures blocking indefinitely
        tokio::time::sleep(std::time::Duration::from_millis(millis_wait)).await;

        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Mad::builder()
        .add(WaitServer {})
        .add(WaitServer {})
        .add(WaitServer {})
        .run()
        .await?;

    Ok(())
}
```

## Examples

Can be found (here)[crates/mad/examples]

- basic
- fn
- signals
- error_log
