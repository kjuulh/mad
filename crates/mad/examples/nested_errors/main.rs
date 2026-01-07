use tokio_util::sync::CancellationToken;

struct NestedErrorComponent {
    name: String,
}

impl notmad::Component for NestedErrorComponent {
    fn name(&self) -> Option<String> {
        Some(self.name.clone())
    }

    async fn run(&self, _cancellation: CancellationToken) -> Result<(), notmad::MadError> {
        // Simulate a deeply nested error
        let io_error = std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "access denied to /etc/secret",
        );

        Err(anyhow::Error::from(io_error)
            .context("failed to read configuration file")
            .context("unable to initialize database connection pool")
            .context(format!("component '{}' startup failed", self.name))
            .into())
    }
}

struct AnotherFailingComponent;

impl notmad::Component for AnotherFailingComponent {
    fn name(&self) -> Option<String> {
        Some("another-component".into())
    }

    async fn run(&self, _cancellation: CancellationToken) -> Result<(), notmad::MadError> {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        Err(anyhow::anyhow!("network timeout after 30s")
            .context("failed to connect to external API")
            .context("service health check failed")
            .into())
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("mad=debug")
        .init();

    let result = notmad::Mad::builder()
        .add(NestedErrorComponent {
            name: "database-service".into(),
        })
        .add(AnotherFailingComponent)
        .run()
        .await;

    match result {
        Ok(()) => println!("Success!"),
        Err(e) => {
            eprintln!("\n=== Error occurred ===");
            eprintln!("{}", e);

            // Also demonstrate how to walk the error chain manually
            if let notmad::MadError::AggregateError(ref agg) = e {
                eprintln!("\n=== Detailed error chains ===");
                for (i, error) in agg.get_errors().iter().enumerate() {
                    eprintln!("\nComponent {} error chain:", i + 1);
                    if let notmad::MadError::Inner(inner) = error {
                        for (j, cause) in inner.chain().enumerate() {
                            eprintln!("  {}. {}", j + 1, cause);
                        }
                    }
                }
            } else if let notmad::MadError::Inner(ref inner) = e {
                eprintln!("\n=== Error chain ===");
                for (i, cause) in inner.chain().enumerate() {
                    eprintln!("  {}. {}", i + 1, cause);
                }
            }
        }
    }
}
