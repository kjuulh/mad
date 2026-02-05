# MAD - Lifecycle Manager for Rust Applications

[![Crates.io](https://img.shields.io/crates/v/notmad.svg)](https://crates.io/crates/notmad)
[![Documentation](https://docs.rs/notmad/badge.svg)](https://docs.rs/notmad)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A simple lifecycle manager for long-running Rust applications. Run multiple services concurrently with graceful shutdown handling.

## Installation

```toml
[dependencies]
notmad = "0.10.0"
tokio = { version = "1", features = ["full"] }
```

## Quick Start

```rust
use notmad::{Component, Mad};
use tokio_util::sync::CancellationToken;

struct MyService;

impl Component for MyService {
    async fn run(&self, cancellation: CancellationToken) -> Result<(), notmad::MadError> {
        println!("Service running...");
        cancellation.cancelled().await;
        println!("Service stopped");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Mad::builder()
        .add(MyService)
        .run()
        .await?;
    Ok(())
}
```

## Basic Usage

### Axum Web Server with Graceful Shutdown

Here's how to run an Axum server with MAD's graceful shutdown:

```rust
use axum::{Router, routing::get};
use notmad::{Component, ComponentInfo};
use tokio_util::sync::CancellationToken;

struct WebServer {
    port: u16,
}

impl Component for WebServer {
    fn info(&self) -> ComponentInfo {
        format!("WebServer:{}", self.port).into()
    }

    async fn run(&self, cancel: CancellationToken) -> Result<(), notmad::MadError> {
        let app = Router::new().route("/", get(|| async { "Hello, World!" }));

        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", self.port))
            .await?;

        println!("Listening on http://0.0.0.0:{}", self.port);

        // Run server with graceful shutdown
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                cancel.cancelled().await;
                println!("Shutting down server...");
            })
            .await?;

        Ok(())
    }
}
```

### Run Multiple Services

```rust
Mad::builder()
    .add(WebServer { port: 8080 })
    .add(WebServer { port: 8081 })
    .run()
    .await?;
```

### Use Functions as Components

```rust
Mad::builder()
    .add_fn(|cancel| async move {
        println!("Running...");
        cancel.cancelled().await;
        Ok(())
    })
    .run()
    .await?;
```

## Lifecycle Hooks

Components support optional setup and cleanup phases:

```rust
impl Component for DatabaseService {
    async fn setup(&self) -> Result<(), notmad::MadError> {
        println!("Connecting to database...");
        Ok(())
    }

    async fn run(&self, cancel: CancellationToken) -> Result<(), notmad::MadError> {
        cancel.cancelled().await;
        Ok(())
    }

    async fn close(&self) -> Result<(), notmad::MadError> {
        println!("Closing database connection...");
        Ok(())
    }
}
```

## Migration from v0.9

### Breaking Changes

1. **`name()` → `info()`**: Returns `ComponentInfo` instead of `Option<String>`
   ```rust
   // Before
   fn name(&self) -> Option<String> { Some("my-service".into()) }

   // After
   fn info(&self) -> ComponentInfo { "my-service".into() }
   ```

2. **No more `async-trait`**: Remove the dependency and `#[async_trait]` attribute
   ```rust
   // Before
   #[async_trait]
   impl Component for MyService { }

   // After
   impl Component for MyService { }
   ```

## Examples

See [examples directory](crates/mad/examples) for complete working examples.

## License

MIT - see [LICENSE](LICENSE)

## Links

- [Documentation](https://docs.rs/notmad)
- [Repository](https://github.com/kjuulh/mad)
- [Crates.io](https://crates.io/crates/notmad)
