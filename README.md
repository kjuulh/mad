# MAD - Lifecycle Manager for Rust Applications

[![Crates.io](https://img.shields.io/crates/v/notmad.svg)](https://crates.io/crates/notmad)
[![Documentation](https://docs.rs/notmad/badge.svg)](https://docs.rs/notmad)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Overview

MAD is a robust lifecycle manager designed for long-running Rust operations. It provides a simple, composable way to manage multiple concurrent services within your application, handling graceful startup and shutdown automatically.

### Perfect for:
- 🌐 Web servers
- 📨 Queue consumers and message processors  
- 🔌 gRPC servers
- ⏰ Cron job runners
- 🔄 Background workers
- 📡 Any long-running async operations

## Features

- **Component-based architecture** - Build your application from composable, reusable components
- **Graceful shutdown** - Automatic handling of shutdown signals with proper cleanup
- **Concurrent execution** - Run multiple components in parallel with tokio
- **Error handling** - Built-in error propagation and logging
- **Cancellation tokens** - Coordinate shutdown across all components
- **Minimal boilerplate** - Focus on your business logic, not lifecycle management

## Installation

Add MAD to your `Cargo.toml`:

```toml
[dependencies]
notmad = "0.7.5"
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
```

## Quick Start

Here's a simple example of a component that simulates a long-running server:

```rust
use mad::{Component, Mad};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

// Define your component
struct WebServer {
    port: u16,
}

#[async_trait]
impl Component for WebServer {
    fn name(&self) -> Option<String> {
        Some(format!("WebServer on port {}", self.port))
    }

    async fn run(&self, cancellation: CancellationToken) -> Result<(), mad::MadError> {
        println!("Starting web server on port {}", self.port);
        
        // Your server logic here
        // The cancellation token will be triggered on shutdown
        tokio::select! {
            _ = cancellation.cancelled() => {
                println!("Shutting down web server");
            }
            _ = self.serve() => {
                println!("Server stopped");
            }
        }
        
        Ok(())
    }
}

impl WebServer {
    async fn serve(&self) {
        // Simulate a running server
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Build and run your application
    Mad::builder()
        .add(WebServer { port: 8080 })
        .add(WebServer { port: 8081 })  // You can add multiple instances
        .run()
        .await?;

    Ok(())
}
```

## Advanced Usage

### Custom Components

Components can be anything that implements the `Component` trait:

```rust
use mad::{Component, Mad};
use async_trait::async_trait;

struct QueueProcessor {
    queue_name: String,
}

#[async_trait]
impl Component for QueueProcessor {
    fn name(&self) -> Option<String> {
        Some(format!("QueueProcessor-{}", self.queue_name))
    }

    async fn run(&self, cancellation: CancellationToken) -> Result<(), mad::MadError> {
        while !cancellation.is_cancelled() {
            // Process messages from queue
            self.process_next_message().await?;
        }
        Ok(())
    }
}
```

### Error Handling

MAD provides comprehensive error handling through the `MadError` type with automatic conversion from `anyhow::Error`:

```rust
async fn run(&self, cancellation: CancellationToken) -> Result<(), mad::MadError> {
    // Errors automatically convert from anyhow::Error to MadError
    database_operation().await?;
    
    // Or return explicit errors
    if some_condition {
        return Err(anyhow::anyhow!("Something went wrong").into());
    }
    
    Ok(())
}
```

## Examples

Check out the [examples directory](crates/mad/examples) for more detailed examples:

- **basic** - Simple component lifecycle
- **fn** - Using functions as components
- **signals** - Handling system signals
- **error_log** - Error handling and logging

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request. For major changes, please open an issue first to discuss what you would like to change.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Author

Created and maintained by [kjuulh](https://github.com/kjuulh)

## Repository

Find the source code at [https://github.com/kjuulh/mad](https://github.com/kjuulh/mad)
