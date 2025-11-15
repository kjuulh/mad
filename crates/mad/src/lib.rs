//! # MAD - Lifecycle Manager for Rust Applications
//!
//! MAD is a robust lifecycle manager designed for long-running Rust operations. It provides
//! a simple, composable way to manage multiple concurrent services within your application,
//! handling graceful startup and shutdown automatically.
//!
//! ## Overview
//!
//! MAD helps you build applications composed of multiple long-running components that need
//! to be orchestrated together. It handles:
//!
//! - **Concurrent execution** of multiple components
//! - **Graceful shutdown** with cancellation tokens
//! - **Error aggregation** from multiple components
//! - **Lifecycle management** with setup, run, and close phases
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use notmad::{Component, Mad};
//! use async_trait::async_trait;
//! use tokio_util::sync::CancellationToken;
//!
//! struct MyService {
//!     name: String,
//! }
//!
//! #[async_trait]
//! impl Component for MyService {
//!     fn name(&self) -> Option<String> {
//!         Some(self.name.clone())
//!     }
//!
//!     async fn run(&self, cancellation: CancellationToken) -> Result<(), notmad::MadError> {
//!         // Your service logic here
//!         while !cancellation.is_cancelled() {
//!             // Do work...
//!             tokio::time::sleep(std::time::Duration::from_secs(1)).await;
//!         }
//!         Ok(())
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     Mad::builder()
//!         .add(MyService { name: "service-1".into() })
//!         .add(MyService { name: "service-2".into() })
//!         .run()
//!         .await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Component Lifecycle
//!
//! Components go through three phases:
//!
//! 1. **Setup**: Optional initialization phase before components start running
//! 2. **Run**: Main execution phase where components perform their work
//! 3. **Close**: Optional cleanup phase after components stop
//!
//! ## Error Handling
//!
//! MAD provides comprehensive error handling through [`MadError`], which can:
//! - Wrap errors from individual components
//! - Aggregate multiple errors when several components fail
//! - Automatically convert from `anyhow::Error`
//!
//! ## Shutdown Behavior
//!
//! MAD handles shutdown gracefully:
//! - Responds to SIGTERM and Ctrl+C signals
//! - Propagates cancellation tokens to all components
//! - Waits for components to finish cleanup
//! - Configurable cancellation timeout

use futures::stream::FuturesUnordered;
use futures_util::StreamExt;
use std::{error::Error, fmt::Display, sync::Arc};
use tokio::{
    signal::unix::{SignalKind, signal},
    task::JoinError,
};

use tokio_util::sync::CancellationToken;

use crate::waiter::Waiter;

mod waiter;

/// Error type for MAD operations.
///
/// This enum represents all possible errors that can occur during
/// the lifecycle of MAD components.
#[derive(thiserror::Error, Debug)]
pub enum MadError {
    /// Generic error wrapper for anyhow errors.
    ///
    /// This variant is used when components return errors via the `?` operator
    /// or when converting from `anyhow::Error`.
    #[error(transparent)]
    Inner(anyhow::Error),

    /// Error that occurred during the run phase of a component.
    #[error(transparent)]
    RunError { run: anyhow::Error },

    /// Error that occurred during the close phase of a component.
    #[error("component(s) failed during close")]
    CloseError {
        #[source]
        close: anyhow::Error,
    },

    /// Multiple errors from different components.
    ///
    /// This is used when multiple components fail simultaneously,
    /// allowing all errors to be reported rather than just the first one.
    #[error("{0}")]
    AggregateError(AggregateError),

    /// Returned when a component doesn't implement the optional setup method.
    ///
    /// This is not typically an error condition as setup is optional.
    #[error("setup not defined")]
    SetupNotDefined,

    /// Returned when a component doesn't implement the optional close method.
    ///
    /// This is not typically an error condition as close is optional.
    #[error("close not defined")]
    CloseNotDefined,
}

impl From<anyhow::Error> for MadError {
    fn from(value: anyhow::Error) -> Self {
        Self::Inner(value)
    }
}

/// Container for multiple errors from different components.
///
/// When multiple components fail, their errors are collected
/// into this struct to provide complete error reporting.
#[derive(Debug, thiserror::Error)]
pub struct AggregateError {
    errors: Vec<MadError>,
}

impl AggregateError {
    /// Returns a slice of all contained errors.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// match result {
    ///     Err(notmad::MadError::AggregateError(agg)) => {
    ///         for error in agg.get_errors() {
    ///             eprintln!("Component error: {}", error);
    ///         }
    ///     }
    ///     _ => {}
    /// }
    /// ```
    pub fn get_errors(&self) -> &[MadError] {
        &self.errors
    }

    pub fn take_errors(self) -> Vec<MadError> {
        self.errors
    }
}

impl Display for AggregateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.errors.is_empty() {
            return Ok(());
        }

        if self.errors.len() == 1 {
            return write!(f, "{}", self.errors[0]);
        }

        writeln!(f, "{} component errors occurred:", self.errors.len())?;
        for (i, error) in self.errors.iter().enumerate() {
            write!(f, "\n[Component {}] {}", i + 1, error)?;

            // Print the error chain for each component error
            let mut source = error.source();
            let mut level = 1;
            while let Some(err) = source {
                write!(f, "\n  {}. {}", level, err)?;
                source = err.source();
                level += 1;
            }
        }
        Ok(())
    }
}

/// The main lifecycle manager for running multiple components.
///
/// `Mad` orchestrates the lifecycle of multiple components, ensuring they
/// start up in order, run concurrently, and shut down gracefully.
///
/// # Example
///
/// ```rust
/// use notmad::{Component, Mad};
/// use async_trait::async_trait;
/// use tokio_util::sync::CancellationToken;
///
/// struct MyComponent;
///
/// #[async_trait]
/// impl Component for MyComponent {
///     async fn run(&self, _cancel: CancellationToken) -> Result<(), notmad::MadError> {
///         Ok(())
///     }
/// }
///
/// # async fn example() -> Result<(), notmad::MadError> {
/// Mad::builder()
///     .add(MyComponent)
///     .run()
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct Mad {
    components: Vec<Arc<dyn Component + Send + Sync + 'static>>,

    should_cancel: Option<std::time::Duration>,
}

struct CompletionResult {
    res: Result<(), MadError>,
    name: Option<String>,
}

impl Mad {
    /// Creates a new `Mad` builder.
    ///
    /// This is the entry point for constructing a MAD application.
    /// Components are added using the builder pattern before calling `run()`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use notmad::Mad;
    ///
    /// let mut app = Mad::builder();
    /// ```
    pub fn builder() -> Self {
        Self {
            components: Vec::default(),

            should_cancel: Some(std::time::Duration::from_millis(100)),
        }
    }

    /// Adds a component to the MAD application.
    ///
    /// Components will be set up in the order they are added,
    /// run concurrently, and closed in the order they were added.
    ///
    /// # Arguments
    ///
    /// * `component` - Any type that implements `Component` or `IntoComponent`
    ///
    /// # Example
    ///
    /// ```rust
    /// use notmad::{Component, Mad};
    /// # use async_trait::async_trait;
    /// # use tokio_util::sync::CancellationToken;
    /// # struct MyService;
    /// # #[async_trait]
    /// # impl Component for MyService {
    /// #     async fn run(&self, _: CancellationToken) -> Result<(), notmad::MadError> { Ok(()) }
    /// # }
    ///
    /// Mad::builder()
    ///     .add(MyService)
    ///     .add(MyService);
    /// ```
    pub fn add(&mut self, component: impl IntoComponent) -> &mut Self {
        self.components.push(component.into_component());

        self
    }

    /// Conditionally adds a component based on a boolean condition.
    ///
    /// If the condition is false, a waiter component is added instead,
    /// which simply waits for cancellation without doing any work.
    ///
    /// # Arguments
    ///
    /// * `condition` - If true, adds the component; if false, adds a waiter
    /// * `component` - The component to add if condition is true
    ///
    /// # Example
    ///
    /// ```rust
    /// use notmad::Mad;
    /// # use notmad::Component;
    /// # use async_trait::async_trait;
    /// # use tokio_util::sync::CancellationToken;
    /// # struct DebugService;
    /// # #[async_trait]
    /// # impl Component for DebugService {
    /// #     async fn run(&self, _: CancellationToken) -> Result<(), notmad::MadError> { Ok(()) }
    /// # }
    ///
    /// let enable_debug = std::env::var("DEBUG").is_ok();
    ///
    /// Mad::builder()
    ///     .add_conditional(enable_debug, DebugService);
    /// ```
    pub fn add_conditional(&mut self, condition: bool, component: impl IntoComponent) -> &mut Self {
        if condition {
            self.components.push(component.into_component());
        } else {
            self.components
                .push(Waiter::new(component.into_component()).into_component())
        }

        self
    }

    /// Adds a waiter component that does nothing but wait for cancellation.
    ///
    /// This is useful when you need a placeholder component or want
    /// the application to keep running without any specific work.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # async fn example() {
    /// use notmad::Mad;
    ///
    /// Mad::builder()
    ///     .add_wait()  // Keeps the app running until shutdown signal
    ///     .run()
    ///     .await;
    /// # }
    /// ```
    pub fn add_wait(&mut self) -> &mut Self {
        self.components.push(Waiter::default().into_component());

        self
    }

    /// Adds a closure or function as a component.
    ///
    /// This is a convenient way to add simple components without
    /// creating a full struct that implements `Component`.
    ///
    /// # Arguments
    ///
    /// * `f` - A closure that takes a `CancellationToken` and returns a future
    ///
    /// # Example
    ///
    /// ```rust
    /// use notmad::Mad;
    /// use tokio_util::sync::CancellationToken;
    ///
    /// Mad::builder()
    ///     .add_fn(|cancel: CancellationToken| async move {
    ///         while !cancel.is_cancelled() {
    ///             println!("Working...");
    ///             tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    ///         }
    ///         Ok(())
    ///     });
    /// ```
    pub fn add_fn<F, Fut>(&mut self, f: F) -> &mut Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: futures::Future<Output = Result<(), MadError>> + Send + 'static,
    {
        let comp = ClosureComponent { inner: Box::new(f) };

        self.add(comp)
    }

    /// Configures the cancellation timeout behavior.
    ///
    /// When a shutdown signal is received, MAD will:
    /// 1. Send cancellation tokens to all components
    /// 2. Wait for the specified duration
    /// 3. Force shutdown if components haven't stopped
    ///
    /// # Arguments
    ///
    /// * `should_cancel` - Duration to wait after cancellation before forcing shutdown.
    ///                     Pass `None` to wait indefinitely.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # async fn example() {
    /// use notmad::Mad;
    /// use std::time::Duration;
    ///
    /// Mad::builder()
    ///     .cancellation(Some(Duration::from_secs(30)))  // 30 second grace period
    ///     .run()
    ///     .await;
    /// # }
    /// ```
    pub fn cancellation(&mut self, should_cancel: Option<std::time::Duration>) -> &mut Self {
        self.should_cancel = should_cancel;

        self
    }

    /// Runs all components until completion or shutdown.
    ///
    /// This method:
    /// 1. Calls `setup()` on all components (in order)
    /// 2. Starts all components concurrently
    /// 3. Waits for shutdown signal (SIGTERM, Ctrl+C) or component failure
    /// 4. Sends cancellation to all components
    /// 5. Calls `close()` on all components (in order)
    ///
    /// # Returns
    ///
    /// * `Ok(())` if all components shut down cleanly
    /// * `Err(MadError)` if any component fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use notmad::Mad;
    /// # async fn example() -> Result<(), notmad::MadError> {
    /// Mad::builder()
    ///     .add_wait()
    ///     .run()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn run(&mut self) -> Result<(), MadError> {
        tracing::info!("running mad setup");

        self.setup_components().await?;

        let run_result = self.run_components().await;

        let close_result = self.close_components().await;

        tracing::info!("mad is closed down");
        match (run_result, close_result) {
            (Err(run), Err(close)) => {
                return Err(MadError::AggregateError(AggregateError {
                    errors: vec![run, close],
                }));
            }
            (Ok(_), Ok(_)) => {}
            (Ok(_), Err(close)) => return Err(close),
            (Err(run), Ok(_)) => return Err(run),
        }

        Ok(())
    }

    async fn setup_components(&mut self) -> Result<(), MadError> {
        tracing::debug!("setting up components");

        for comp in &self.components {
            tracing::trace!(component = &comp.name(), "mad setting up");

            match comp.setup().await {
                Ok(_) | Err(MadError::SetupNotDefined) => {}
                Err(e) => return Err(e),
            };
        }

        tracing::debug!("finished setting up components");

        Ok(())
    }

    async fn run_components(&mut self) -> Result<(), MadError> {
        tracing::debug!("running components");

        let mut channels = Vec::new();
        let cancellation_token = CancellationToken::new();
        let job_cancellation = CancellationToken::new();
        let job_done = CancellationToken::new();

        for comp in &self.components {
            let comp = comp.clone();
            let cancellation_token = cancellation_token.child_token();
            let job_cancellation = job_cancellation.child_token();

            let (error_tx, error_rx) = tokio::sync::mpsc::channel::<CompletionResult>(1);
            channels.push(error_rx);

            tokio::spawn(async move {
                let name = comp.name().clone();

                tracing::debug!(component = name, "mad running");

                let handle = tokio::spawn(async move { comp.run(job_cancellation).await });

                tokio::select! {
                    _ = cancellation_token.cancelled() => {
                        error_tx.send(CompletionResult { res: Ok(()) , name  }).await
                    }
                    res = handle => {
                     let res = match res {
                         Ok(res) => res,
                         Err(join) => {
                             match join.source() {
                                 Some(error) => {
                                     Err(MadError::RunError{run: anyhow::anyhow!("component aborted: {:?}", error)})
                                 },
                                 None => {
                                     if join.is_panic(){
                                         Err(MadError::RunError { run: anyhow::anyhow!("component panicked: {}", join) })
                                     } else {
                                         Err(MadError::RunError { run: anyhow::anyhow!("component faced unknown error: {}", join) })
                                     }
                                 },
                             }
                         },
                     };


                        error_tx.send(CompletionResult { res , name  }).await
                    }
                }
            });
        }

        tokio::spawn({
            let cancellation_token = cancellation_token;
            let job_done = job_done.child_token();

            let wait_cancel = self.should_cancel;

            async move {
                let should_cancel =
                    |cancel: CancellationToken,
                     global_cancel: CancellationToken,
                     wait: Option<std::time::Duration>| async move {
                        if let Some(cancel_wait) = wait {
                            cancel.cancel();
                            tokio::time::sleep(cancel_wait).await;
                            global_cancel.cancel();
                        }
                    };

                tokio::select! {
                    _ = cancellation_token.cancelled() => {
                        job_cancellation.cancel();
                    }
                    _ = job_done.cancelled() => {
                        should_cancel(job_cancellation, cancellation_token, wait_cancel).await;
                    }
                    _ = tokio::signal::ctrl_c() => {
                        should_cancel(job_cancellation,  cancellation_token,wait_cancel).await;
                    }
                    _ = signal_unix_terminate() => {
                        should_cancel(job_cancellation, cancellation_token, wait_cancel).await;
                    }
                }
            }
        });

        let mut futures = FuturesUnordered::new();
        for channel in channels.iter_mut() {
            futures.push(channel.recv());
        }

        let mut errors = Vec::new();
        while let Some(Some(msg)) = futures.next().await {
            match msg.res {
                Err(e) => {
                    tracing::debug!(
                        error = e.to_string(),
                        component = msg.name,
                        "component ran to completion with error"
                    );
                    errors.push(e);
                }
                Ok(_) => {
                    tracing::debug!(component = msg.name, "component ran to completion");
                }
            }

            job_done.cancel();
        }

        tracing::debug!("ran components");
        if !errors.is_empty() {
            return Err(MadError::AggregateError(AggregateError { errors }));
        }

        Ok(())
    }

    async fn close_components(&mut self) -> Result<(), MadError> {
        tracing::debug!("closing components");

        for comp in &self.components {
            tracing::trace!(component = &comp.name(), "mad closing");
            match comp.close().await {
                Ok(_) | Err(MadError::CloseNotDefined) => {}
                Err(e) => return Err(e),
            };
        }

        tracing::debug!("closed components");

        Ok(())
    }
}

async fn signal_unix_terminate() {
    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to bind SIGTERM handler");
    sigterm.recv().await;
}

/// Trait for implementing MAD components.
///
/// Components represent individual services or tasks that run as part
/// of your application. Each component has its own lifecycle with
/// optional setup and cleanup phases.
///
/// # Example
///
/// ```rust
/// use notmad::{Component, MadError};
/// use async_trait::async_trait;
/// use tokio_util::sync::CancellationToken;
///
/// struct DatabaseConnection {
///     url: String,
/// }
///
/// #[async_trait]
/// impl Component for DatabaseConnection {
///     fn name(&self) -> Option<String> {
///         Some("database".to_string())
///     }
///
///     async fn setup(&self) -> Result<(), MadError> {
///         println!("Connecting to database...");
///         // Initialize connection pool
///         Ok(())
///     }
///
///     async fn run(&self, cancel: CancellationToken) -> Result<(), MadError> {
///         // Keep connection alive, handle queries
///         cancel.cancelled().await;
///         Ok(())
///     }
///
///     async fn close(&self) -> Result<(), MadError> {
///         println!("Closing database connection...");
///         // Clean up resources
///         Ok(())
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait Component {
    /// Returns an optional name for the component.
    ///
    /// This name is used in logging and error messages to identify
    /// which component is being processed.
    ///
    /// # Default
    ///
    /// Returns `None` if not overridden.
    fn name(&self) -> Option<String> {
        None
    }

    /// Optional setup phase called before the component starts running.
    ///
    /// Use this for initialization tasks like:
    /// - Establishing database connections
    /// - Loading configuration
    /// - Preparing resources
    ///
    /// # Default
    ///
    /// Returns `MadError::SetupNotDefined` which is handled gracefully.
    ///
    /// # Errors
    ///
    /// If setup fails with an error other than `SetupNotDefined`,
    /// the entire application will stop before any components start running.
    async fn setup(&self) -> Result<(), MadError> {
        Err(MadError::SetupNotDefined)
    }

    /// Main execution phase of the component.
    ///
    /// This method should contain the primary logic of your component.
    /// It should respect the cancellation token and shut down gracefully
    /// when cancellation is requested.
    ///
    /// # Arguments
    ///
    /// * `cancellation_token` - Signal for graceful shutdown
    ///
    /// # Implementation Guidelines
    ///
    /// - Check `cancellation_token.is_cancelled()` periodically
    /// - Use `tokio::select!` with `cancellation_token.cancelled()` for async operations
    /// - Clean up resources before returning
    ///
    /// # Errors
    ///
    /// Any error returned will trigger shutdown of all other components.
    async fn run(&self, cancellation_token: CancellationToken) -> Result<(), MadError>;

    /// Optional cleanup phase called after the component stops.
    ///
    /// Use this for cleanup tasks like:
    /// - Flushing buffers
    /// - Closing connections
    /// - Saving state
    ///
    /// # Default
    ///
    /// Returns `MadError::CloseNotDefined` which is handled gracefully.
    ///
    /// # Errors
    ///
    /// Errors during close are logged but don't prevent other components
    /// from closing.
    async fn close(&self) -> Result<(), MadError> {
        Err(MadError::CloseNotDefined)
    }
}

/// Trait for converting types into components.
///
/// This trait is automatically implemented for all types that implement
/// `Component + Send + Sync + 'static`, allowing them to be added to MAD
/// directly without manual conversion.
///
/// # Example
///
/// ```rust
/// use notmad::{Component, IntoComponent, Mad};
/// # use async_trait::async_trait;
/// # use tokio_util::sync::CancellationToken;
///
/// struct MyService;
///
/// # #[async_trait]
/// # impl Component for MyService {
/// #     async fn run(&self, _: CancellationToken) -> Result<(), notmad::MadError> { Ok(()) }
/// # }
///
/// // MyService automatically implements IntoComponent
/// Mad::builder()
///     .add(MyService);  // Works directly
/// ```
pub trait IntoComponent {
    /// Converts self into an Arc-wrapped component.
    fn into_component(self) -> Arc<dyn Component + Send + Sync + 'static>;
}

impl<T: Component + Send + Sync + 'static> IntoComponent for T {
    fn into_component(self) -> Arc<dyn Component + Send + Sync + 'static> {
        Arc::new(self)
    }
}

struct ClosureComponent<F, Fut>
where
    F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
    Fut: futures::Future<Output = Result<(), MadError>> + Send + 'static,
{
    inner: Box<F>,
}

impl<F, Fut> ClosureComponent<F, Fut>
where
    F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
    Fut: futures::Future<Output = Result<(), MadError>> + Send + 'static,
{
    pub async fn execute(&self, cancellation_token: CancellationToken) -> Result<(), MadError> {
        (*self.inner)(cancellation_token).await?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl<F, Fut> Component for ClosureComponent<F, Fut>
where
    F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
    Fut: futures::Future<Output = Result<(), MadError>> + Send + 'static,
{
    async fn run(&self, cancellation_token: CancellationToken) -> Result<(), MadError> {
        self.execute(cancellation_token).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;

    #[test]
    fn test_error_chaining_display() {
        // Test single error with context chain
        let base_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let error = anyhow::Error::from(base_error)
            .context("failed to read configuration")
            .context("unable to initialize database")
            .context("service startup failed");

        let mad_error = MadError::Inner(error);
        let display = format!("{}", mad_error);

        // Should display the top-level error message
        assert!(display.contains("service startup failed"));

        // Test error chain iteration
        if let MadError::Inner(ref e) = mad_error {
            let chain: Vec<String> = e.chain().map(|c| c.to_string()).collect();
            assert_eq!(chain.len(), 4);
            assert_eq!(chain[0], "service startup failed");
            assert_eq!(chain[1], "unable to initialize database");
            assert_eq!(chain[2], "failed to read configuration");
            assert_eq!(chain[3], "file not found");
        }
    }

    #[test]
    fn test_aggregate_error_display() {
        let error1 = MadError::Inner(
            anyhow::anyhow!("database connection failed")
                .context("failed to connect to PostgreSQL"),
        );

        let error2 = MadError::Inner(
            anyhow::anyhow!("port already in use")
                .context("failed to bind to port 8080")
                .context("web server initialization failed"),
        );

        let aggregate = MadError::AggregateError(AggregateError {
            errors: vec![error1, error2],
        });

        let display = format!("{}", aggregate);

        // Check that it shows multiple errors
        assert!(display.contains("2 component errors occurred"));
        assert!(display.contains("[Component 1]"));
        assert!(display.contains("[Component 2]"));

        // Check that context chains are displayed
        assert!(display.contains("failed to connect to PostgreSQL"));
        assert!(display.contains("database connection failed"));
        assert!(display.contains("web server initialization failed"));
        assert!(display.contains("failed to bind to port 8080"));
        assert!(display.contains("port already in use"));
    }

    #[test]
    fn test_single_error_aggregate() {
        let error = MadError::Inner(anyhow::anyhow!("single error"));
        let aggregate = AggregateError {
            errors: vec![error],
        };

        let display = format!("{}", aggregate);
        // Single error should be displayed directly
        assert!(display.contains("single error"));
        assert!(!display.contains("component errors occurred"));
    }

    #[test]
    fn test_error_source_chain() {
        let error = MadError::Inner(
            anyhow::anyhow!("root cause")
                .context("middle layer")
                .context("top layer"),
        );

        // Test that we can access the error chain
        if let MadError::Inner(ref e) = error {
            let chain: Vec<String> = e.chain().map(|c| c.to_string()).collect();
            assert_eq!(chain.len(), 3);
            assert_eq!(chain[0], "top layer");
            assert_eq!(chain[1], "middle layer");
            assert_eq!(chain[2], "root cause");
        } else {
            panic!("Expected MadError::Inner");
        }
    }

    #[tokio::test]
    async fn test_component_error_propagation() {
        struct FailingComponent;

        #[async_trait::async_trait]
        impl Component for FailingComponent {
            fn name(&self) -> Option<String> {
                Some("test-component".to_string())
            }

            async fn run(&self, _cancel: CancellationToken) -> Result<(), MadError> {
                Err(anyhow::anyhow!("IO error")
                    .context("failed to open file")
                    .context("component initialization failed")
                    .into())
            }
        }

        let result = Mad::builder()
            .add(FailingComponent)
            .cancellation(Some(std::time::Duration::from_millis(100)))
            .run()
            .await;

        assert!(result.is_err());
        let error = result.unwrap_err();

        // Check error display
        let display = format!("{}", error);
        assert!(display.contains("component initialization failed"));
    }
}
