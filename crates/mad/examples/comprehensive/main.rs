//! Comprehensive example demonstrating MAD's full capabilities.
//!
//! This example shows:
//! - Multiple component types (struct, closure, conditional)
//! - Component lifecycle (setup, run, close)
//! - Error handling and propagation
//! - Graceful shutdown with cancellation tokens
//! - Concurrent component execution

use async_trait::async_trait;
use notmad::{Component, Mad, MadError};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::time::{Duration, interval};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

/// A web server component that simulates handling HTTP requests.
struct WebServer {
    port: u16,
    request_count: Arc<AtomicUsize>,
}

#[async_trait]
impl Component for WebServer {
    fn name(&self) -> Option<String> {
        Some(format!("web-server-{}", self.port))
    }

    async fn setup(&self) -> Result<(), MadError> {
        info!("Setting up web server on port {}", self.port);
        // In a real application, you might:
        // - Bind to the port
        // - Set up TLS certificates
        // - Initialize middleware
        tokio::time::sleep(Duration::from_millis(100)).await;
        info!("Web server on port {} is ready", self.port);
        Ok(())
    }

    async fn run(&self, cancellation: CancellationToken) -> Result<(), MadError> {
        info!("Web server on port {} started", self.port);
        let mut interval = interval(Duration::from_secs(1));

        while !cancellation.is_cancelled() {
            tokio::select! {
                _ = cancellation.cancelled() => {
                    info!("Web server on port {} received shutdown signal", self.port);
                    break;
                }
                _ = interval.tick() => {
                    // Simulate handling requests
                    let count = self.request_count.fetch_add(1, Ordering::Relaxed);
                    info!("Server on port {} handled request #{}", self.port, count + 1);
                }
            }
        }

        Ok(())
    }

    async fn close(&self) -> Result<(), MadError> {
        info!("Shutting down web server on port {}", self.port);
        // In a real application, you might:
        // - Drain existing connections
        // - Save server state
        // - Close database connections
        tokio::time::sleep(Duration::from_millis(200)).await;
        let total = self.request_count.load(Ordering::Relaxed);
        info!(
            "Web server on port {} shut down. Total requests handled: {}",
            self.port, total
        );
        Ok(())
    }
}

/// A background job processor that simulates processing tasks from a queue.
struct JobProcessor {
    queue_name: String,
    processing_interval: Duration,
}

#[async_trait]
impl Component for JobProcessor {
    fn name(&self) -> Option<String> {
        Some(format!("job-processor-{}", self.queue_name))
    }

    async fn setup(&self) -> Result<(), MadError> {
        info!("Connecting to queue: {}", self.queue_name);
        // Simulate connecting to a message queue
        tokio::time::sleep(Duration::from_millis(150)).await;
        Ok(())
    }

    async fn run(&self, cancellation: CancellationToken) -> Result<(), MadError> {
        info!("Job processor for {} started", self.queue_name);
        let mut interval = interval(self.processing_interval);
        let mut job_count = 0;

        loop {
            tokio::select! {
                _ = cancellation.cancelled() => {
                    info!("Job processor for {} stopping...", self.queue_name);
                    break;
                }
                _ = interval.tick() => {
                    job_count += 1;
                    info!("Processing job #{} from {}", job_count, self.queue_name);

                    // Simulate job processing
                    tokio::time::sleep(Duration::from_millis(100)).await;

                    // Simulate occasional errors (but don't fail the component)
                    if job_count % 10 == 0 {
                        warn!("Job #{} from {} required retry", job_count, self.queue_name);
                    }
                }
            }
        }

        info!(
            "Job processor for {} processed {} jobs",
            self.queue_name, job_count
        );
        Ok(())
    }

    async fn close(&self) -> Result<(), MadError> {
        info!("Disconnecting from queue: {}", self.queue_name);
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok(())
    }
}

/// A health check component that monitors system health.
struct HealthChecker {
    check_interval: Duration,
}

#[async_trait]
impl Component for HealthChecker {
    fn name(&self) -> Option<String> {
        Some("health-checker".to_string())
    }

    async fn run(&self, cancellation: CancellationToken) -> Result<(), MadError> {
        info!("Health checker started");
        let mut interval = interval(self.check_interval);

        while !cancellation.is_cancelled() {
            tokio::select! {
                _ = cancellation.cancelled() => {
                    info!("Health checker stopping...");
                    break;
                }
                _ = interval.tick() => {
                    // Simulate health checks
                    let cpu_usage = rand::random::<f32>() * 100.0;
                    let memory_usage = rand::random::<f32>() * 100.0;

                    info!("System health: CPU={:.1}%, Memory={:.1}%", cpu_usage, memory_usage);

                    if cpu_usage > 90.0 {
                        warn!("High CPU usage detected: {:.1}%", cpu_usage);
                    }
                    if memory_usage > 90.0 {
                        warn!("High memory usage detected: {:.1}%", memory_usage);
                    }
                }
            }
        }

        Ok(())
    }
}

/// A component that will fail after some time to demonstrate error handling.
struct FailingComponent {
    fail_after: Duration,
}

#[async_trait]
impl Component for FailingComponent {
    fn name(&self) -> Option<String> {
        Some("failing-component".to_string())
    }

    async fn run(&self, cancellation: CancellationToken) -> Result<(), MadError> {
        info!(
            "Failing component started (will fail after {:?})",
            self.fail_after
        );

        tokio::select! {
            _ = cancellation.cancelled() => {
                info!("Failing component cancelled before failure");
                Ok(())
            }
            _ = tokio::time::sleep(self.fail_after) => {
                error!("Failing component encountered an error!");
                Err(anyhow::anyhow!("Simulated component failure").into())
            }
        }
    }
}

/// Debug component that logs system status periodically.
struct DebugComponent;

#[async_trait]
impl Component for DebugComponent {
    fn name(&self) -> Option<String> {
        Some("debug-component".to_string())
    }

    async fn run(&self, cancel: CancellationToken) -> Result<(), MadError> {
        info!("Debug mode enabled - verbose logging active");
        let mut interval = interval(Duration::from_secs(5));

        while !cancel.is_cancelled() {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = interval.tick() => {
                    info!("DEBUG: System running normally");
                }
            }
        }

        info!("Debug component shutting down");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing for logging
    tracing_subscriber::fmt()
        .with_target(false)
        .without_time()
        .init();

    info!("Starting comprehensive MAD example application");

    // Check if we should enable the failing component
    let enable_failure_demo = std::env::var("ENABLE_FAILURE").is_ok();

    // Check if we should enable debug mode
    let debug_mode = std::env::var("DEBUG").is_ok();

    // Shared state for demonstration
    let request_count = Arc::new(AtomicUsize::new(0));

    // Build and run the application
    let result = Mad::builder()
        // Add web servers
        .add(WebServer {
            port: 8080,
            request_count: request_count.clone(),
        })
        .add(WebServer {
            port: 8081,
            request_count: request_count.clone(),
        })
        // Add job processors
        .add(JobProcessor {
            queue_name: "high-priority".to_string(),
            processing_interval: Duration::from_secs(2),
        })
        .add(JobProcessor {
            queue_name: "low-priority".to_string(),
            processing_interval: Duration::from_secs(5),
        })
        // Add health checker
        .add(HealthChecker {
            check_interval: Duration::from_secs(3),
        })
        // Conditionally add a debug component using add_fn
        .add_conditional(debug_mode, DebugComponent)
        // Conditionally add failing component to demonstrate error handling
        .add_conditional(
            enable_failure_demo,
            FailingComponent {
                fail_after: Duration::from_secs(10),
            },
        )
        // Add a simple metrics reporter using add_fn
        .add_fn(|cancel: CancellationToken| async move {
            info!("Metrics reporter started");
            let mut interval = interval(Duration::from_secs(10));
            let start = std::time::Instant::now();

            while !cancel.is_cancelled() {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = interval.tick() => {
                        let uptime = start.elapsed();
                        info!("Application uptime: {:?}", uptime);
                    }
                }
            }

            info!("Metrics reporter stopped");
            Ok(())
        })
        // Configure graceful shutdown timeout
        .cancellation(Some(Duration::from_secs(5)))
        // Run the application
        .run()
        .await;

    match result {
        Ok(()) => {
            info!("Application shut down successfully");
            Ok(())
        }
        Err(e) => {
            error!("Application failed: {}", e);

            // Check if it's an aggregate error with multiple failures
            if let MadError::AggregateError(ref agg) = e {
                error!("Multiple component failures detected:");
                for (i, err) in agg.get_errors().iter().enumerate() {
                    error!("  {}. {}", i + 1, err);
                }
            }

            Err(e.into())
        }
    }
}
