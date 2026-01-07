//! Example demonstrating running multiple services with MAD.
//!
//! This example shows how to run a web server, queue processor, and
//! scheduled task together, with graceful shutdown on Ctrl+C.

use notmad::{Component, Mad, MadError};
use tokio::time::{Duration, interval};
use tokio_util::sync::CancellationToken;

/// A simple web server component.
///
/// In a real application, this would bind to a port and handle HTTP requests.
/// Here we simulate it by periodically logging that we're "handling" requests.
struct WebServer {
    port: u16,
}

impl Component for WebServer {
    fn name(&self) -> Option<String> {
        Some(format!("WebServer:{}", self.port))
    }

    async fn setup(&self) -> Result<(), MadError> {
        println!("[{}] Binding to port...", self.name().unwrap());
        // Simulate server setup time
        tokio::time::sleep(Duration::from_millis(100)).await;
        println!("[{}] Ready to accept connections", self.name().unwrap());
        Ok(())
    }

    async fn run(&self, cancellation: CancellationToken) -> Result<(), MadError> {
        println!("[{}] Server started", self.name().unwrap());

        // Simulate handling requests until shutdown
        let mut request_id = 0;
        let mut interval = interval(Duration::from_secs(2));

        while !cancellation.is_cancelled() {
            tokio::select! {
                _ = cancellation.cancelled() => {
                    println!("[{}] Shutdown signal received", self.name().unwrap());
                    break;
                }
                _ = interval.tick() => {
                    request_id += 1;
                    println!("[{}] Handling request #{}", self.name().unwrap(), request_id);
                }
            }
        }

        Ok(())
    }

    async fn close(&self) -> Result<(), MadError> {
        println!("[{}] Closing connections...", self.name().unwrap());
        // Simulate graceful connection drain
        tokio::time::sleep(Duration::from_millis(200)).await;
        println!("[{}] Server stopped", self.name().unwrap());
        Ok(())
    }
}

/// A queue processor that consumes messages from a queue.
///
/// This simulates processing messages from a message queue like
/// RabbitMQ, Kafka, or AWS SQS.
struct QueueProcessor {
    queue_name: String,
}

impl Component for QueueProcessor {
    fn name(&self) -> Option<String> {
        Some(format!("QueueProcessor:{}", self.queue_name))
    }

    async fn run(&self, cancellation: CancellationToken) -> Result<(), MadError> {
        println!("[{}] Started processing", self.name().unwrap());

        let mut message_count = 0;

        // Process messages until shutdown
        while !cancellation.is_cancelled() {
            // Simulate waiting for and processing a message
            tokio::select! {
                _ = cancellation.cancelled() => {
                    println!("[{}] Stopping message processing", self.name().unwrap());
                    break;
                }
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    message_count += 1;
                    println!("[{}] Processed message #{}", self.name().unwrap(), message_count);
                }
            }
        }

        println!(
            "[{}] Processed {} messages total",
            self.name().unwrap(),
            message_count
        );
        Ok(())
    }
}

/// A scheduled task that runs periodically.
///
/// This could be used for tasks like:
/// - Cleaning up old data
/// - Generating reports
/// - Syncing with external systems
struct ScheduledTask {
    task_name: String,
    interval_secs: u64,
}

impl Component for ScheduledTask {
    fn name(&self) -> Option<String> {
        Some(format!("ScheduledTask:{}", self.task_name))
    }

    async fn run(&self, cancellation: CancellationToken) -> Result<(), MadError> {
        println!(
            "[{}] Scheduled to run every {} seconds",
            self.name().unwrap(),
            self.interval_secs
        );

        let mut interval = interval(Duration::from_secs(self.interval_secs));
        let mut run_count = 0;

        while !cancellation.is_cancelled() {
            tokio::select! {
                _ = cancellation.cancelled() => {
                    println!("[{}] Scheduler stopping", self.name().unwrap());
                    break;
                }
                _ = interval.tick() => {
                    run_count += 1;
                    println!("[{}] Executing run #{}", self.name().unwrap(), run_count);

                    // Simulate task execution
                    tokio::time::sleep(Duration::from_millis(500)).await;

                    println!("[{}] Run #{} completed", self.name().unwrap(), run_count);
                }
            }
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Starting multi-service application");
    println!("Press Ctrl+C to trigger graceful shutdown");
    println!("----------------------------------------");

    // Build the application with multiple services
    Mad::builder()
        // Add a web server on port 8080
        .add(WebServer { port: 8080 })
        // Add another web server on port 8081 (e.g., admin interface)
        .add(WebServer { port: 8081 })
        // Add queue processors for different queues
        .add(QueueProcessor {
            queue_name: "orders".to_string(),
        })
        .add(QueueProcessor {
            queue_name: "notifications".to_string(),
        })
        // Add scheduled tasks
        .add(ScheduledTask {
            task_name: "cleanup".to_string(),
            interval_secs: 5,
        })
        .add(ScheduledTask {
            task_name: "report_generator".to_string(),
            interval_secs: 10,
        })
        // Add a monitoring component using a closure
        .add_fn(|cancel| async move {
            println!("[Monitor] Starting system monitor");
            let mut interval = interval(Duration::from_secs(15));

            while !cancel.is_cancelled() {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        println!("[Monitor] Stopping monitor");
                        break;
                    }
                    _ = interval.tick() => {
                        println!("[Monitor] All systems operational");
                    }
                }
            }

            Ok(())
        })
        // Set graceful shutdown timeout to 3 seconds
        .cancellation(Some(Duration::from_secs(3)))
        // Run all components
        .run()
        .await?;

    println!("----------------------------------------");
    println!("All services shut down successfully");

    Ok(())
}
