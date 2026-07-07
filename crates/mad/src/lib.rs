//! # MAD - Lifecycle Manager for Rust Applications
//!
//! A simple lifecycle manager for long-running Rust applications. Run multiple services
//! concurrently with graceful shutdown handling.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use notmad::{Component, Mad};
//! use tokio_util::sync::CancellationToken;
//!
//! struct MyService;
//!
//! impl Component for MyService {
//!     async fn run(&self, cancel: CancellationToken) -> Result<(), notmad::MadError> {
//!         println!("Running...");
//!         cancel.cancelled().await;
//!         println!("Stopped");
//!         Ok(())
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     Mad::builder()
//!         .add(MyService)
//!         .run()
//!         .await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Features
//!
//! - Run multiple components concurrently
//! - Graceful shutdown with cancellation tokens
//! - Optional lifecycle hooks: `setup()`, `run()`, `close()`
//! - Automatic error aggregation
//! - SIGTERM and Ctrl+C signal handling

use std::{error::Error, fmt::Display, pin::Pin, sync::Arc};
use tokio::signal::unix::{SignalKind, signal};

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
/// use tokio_util::sync::CancellationToken;
///
/// struct MyComponent;
///
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
    /// Components grouped into ordered shutdown stages — one per [`Mad::add`]
    /// call. Startup is concurrent across all stages; shutdown drains stages in
    /// declaration order — the first (outermost / ingress) stage first —
    /// advancing only once a stage has fully stopped. Group several components
    /// into a single parallel stage with [`stage`] + [`Stage::and`].
    stages: Vec<Vec<SharedComponent>>,

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
            stages: Vec::new(),

            should_cancel: Some(std::time::Duration::from_millis(100)),
        }
    }

    /// Adds a **shutdown stage** to the application.
    ///
    /// `add` takes anything that is [`IntoStage`]: a single component (the
    /// common case — a component is itself a one-member stage), or several
    /// components grouped into one stage with [`stage`] + [`Stage::and`] /
    /// [`Stage::and_fn`]. **Each `add` call is its own stage.**
    ///
    /// All components across all stages **start concurrently**. On shutdown,
    /// stages are drained **in the order they were added**: the first stage is
    /// cancelled and fully drained — while every later stage is still running,
    /// so in-flight work isn't starved of its dependencies — then the next
    /// stage, and so on. Components within one stage are cancelled together and
    /// drain in parallel. Each stage gets its own drain grace (see
    /// [`Mad::cancellation`]) before it is force-stopped.
    ///
    /// So declare your outermost / ingress layer first and its dependencies
    /// later. (This is the inverse of go-garden's `Add`, which sets up in add
    /// order and tears down in reverse — here add order *is* drain order.)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use notmad::{Mad, Component, stage};
    /// # use tokio_util::sync::CancellationToken;
    /// # #[derive(Clone)] struct S;
    /// # impl Component for S { async fn run(&self, _: CancellationToken) -> Result<(), notmad::MadError> { Ok(()) } }
    /// # async fn example(http: S, grpc: S, consumer: S, publisher: S) -> Result<(), notmad::MadError> {
    /// Mad::builder()
    ///     .add(stage(http).and(grpc))   // stage 0 — ingress (http + grpc), drains first, in parallel
    ///     .add(consumer)                // stage 1 — a lone component is a one-member stage
    ///     .add(publisher)               // stage 2 — drains last
    ///     .run()
    ///     .await
    /// # }
    /// ```
    pub fn add(&mut self, stage: impl IntoStage) -> &mut Self {
        self.stages.push(stage.into_stage().components);

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
    /// # use tokio_util::sync::CancellationToken;
    /// # struct DebugService;
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
            self.add(component)
        } else {
            self.add(Waiter::new(component.into_component()))
        }
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
        self.add(Waiter::default())
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
    ///   Pass `None` to wait indefinitely.
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

    /// Describe the shutdown [`Topology`]: the stages in drain order and the
    /// components in each. Handy to log at startup so the shutdown ordering is
    /// visible — `println!("{}", app.topology())` prints a nested diagram, and
    /// [`Topology::to_json`] emits JSON.
    pub fn topology(&self) -> Topology {
        Topology {
            stages: self
                .stages
                .iter()
                .enumerate()
                .map(|(index, components)| TopologyStage {
                    index,
                    components: components.iter().map(|c| c.info().to_string()).collect(),
                })
                .collect(),
        }
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
        tracing::debug!("shutdown topology:\n{}", self.topology());

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

        for comp in self.stages.iter().flatten() {
            tracing::trace!(component = %comp.info(), "mad setting up");

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

        let stage_count = self.stages.len();
        let total: usize = self.stages.iter().map(|s| s.len()).sum();

        if total == 0 {
            tracing::debug!("no components to run");
            return Ok(());
        }

        // Per-stage cancellation. `graceful[i]` is handed to stage `i`'s
        // components as their run cancellation token (asking them to drain);
        // `force[i]` hard-stops stage `i` once its drain grace has elapsed.
        let graceful: Vec<CancellationToken> =
            (0..stage_count).map(|_| CancellationToken::new()).collect();
        let force: Vec<CancellationToken> =
            (0..stage_count).map(|_| CancellationToken::new()).collect();

        // Completions arrive tagged with their stage index.
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(usize, CompletionResult)>(total);
        let mut remaining: Vec<usize> = self.stages.iter().map(|s| s.len()).collect();
        let mut outstanding = total;

        for (stage_idx, stage) in self.stages.iter().enumerate() {
            for comp in stage {
                let comp = comp.clone();
                let graceful = graceful[stage_idx].child_token();
                let force = force[stage_idx].child_token();
                let tx = tx.clone();

                tokio::spawn(async move {
                    let info = comp.info().clone();

                    tracing::debug!(component = %info, stage = stage_idx, "mad running");

                    let handle = tokio::spawn(async move { comp.run(graceful).await });

                    let res = tokio::select! {
                        _ = force.cancelled() => Ok(()),
                        res = handle => match res {
                            Ok(res) => res,
                            Err(join) => Err(match join.source() {
                                Some(error) => MadError::RunError {
                                    run: anyhow::anyhow!("component aborted: {:?}", error),
                                },
                                None => {
                                    if join.is_panic() {
                                        MadError::RunError {
                                            run: anyhow::anyhow!("component panicked: {}", join),
                                        }
                                    } else {
                                        MadError::RunError {
                                            run: anyhow::anyhow!(
                                                "component faced unknown error: {}",
                                                join
                                            ),
                                        }
                                    }
                                }
                            }),
                        },
                    };

                    let _ = tx.send((stage_idx, CompletionResult { res, name: info.name })).await;
                });
            }
        }
        drop(tx);

        // Shutdown is triggered by an OS signal or by any component returning on
        // its own (mirrors the previous "first done stops the rest").
        let shutdown = CancellationToken::new();
        tokio::spawn({
            let shutdown = shutdown.clone();
            async move {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = signal_unix_terminate() => {}
                }
                tracing::debug!("shutdown signal received");
                shutdown.cancel();
            }
        });

        let mut errors = Vec::new();

        fn record_completion(
            errors: &mut Vec<MadError>,
            remaining: &mut [usize],
            outstanding: &mut usize,
            stage: usize,
            completion: CompletionResult,
        ) {
            let CompletionResult { res, name } = completion;
            *outstanding -= 1;
            remaining[stage] -= 1;
            match res {
                Err(e) => {
                    tracing::debug!(error = e.to_string(), component = ?name, stage, "component completed with error");
                    errors.push(e);
                }
                Ok(_) => tracing::debug!(component = ?name, stage, "component completed"),
            }
        }

        // Wait for the first trigger: a signal, or a component finishing.
        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::debug!("beginning ordered shutdown (signal)");
            }
            Some((stage, completion)) = rx.recv() => {
                record_completion(&mut errors, &mut remaining, &mut outstanding, stage, completion);
                tracing::debug!("beginning ordered shutdown (component completed)");
            }
        }

        // Drain stages in declaration order. Only advance to the next stage once
        // the current one has fully stopped, so a stage's dependencies (later
        // stages) stay alive while it drains its in-flight work.
        let grace = self.should_cancel;
        for stage_idx in 0..stage_count {
            if remaining[stage_idx] == 0 {
                continue;
            }

            tracing::debug!(stage = stage_idx, remaining = remaining[stage_idx], "draining stage");
            graceful[stage_idx].cancel();

            let deadline = async {
                match grace {
                    Some(d) => tokio::time::sleep(d).await,
                    None => std::future::pending::<()>().await,
                }
            };
            tokio::pin!(deadline);
            let mut forced = false;

            while remaining[stage_idx] > 0 {
                tokio::select! {
                    maybe = rx.recv() => match maybe {
                        Some((stage, completion)) => {
                            record_completion(&mut errors, &mut remaining, &mut outstanding, stage, completion);
                        }
                        None => break,
                    },
                    _ = &mut deadline, if !forced => {
                        tracing::warn!(stage = stage_idx, "stage drain grace elapsed; forcing stop");
                        forced = true;
                        force[stage_idx].cancel();
                    }
                }
            }
        }

        // Any remaining completions (later-stage components that had already
        // returned on their own) — collect their errors too.
        while outstanding > 0 {
            match rx.recv().await {
                Some((stage, completion)) => {
                    record_completion(&mut errors, &mut remaining, &mut outstanding, stage, completion);
                }
                None => break,
            }
        }

        tracing::debug!("ran components");
        if !errors.is_empty() {
            return Err(MadError::AggregateError(AggregateError { errors }));
        }

        Ok(())
    }

    async fn close_components(&mut self) -> Result<(), MadError> {
        tracing::debug!("closing components");

        for comp in self.stages.iter().flatten() {
            tracing::trace!(component = %comp.info(), "mad closing");
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

#[derive(Default, Clone)]
pub struct ComponentInfo {
    name: Option<String>,
}

impl Display for ComponentInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            self.name
                .as_ref()
                .map(|n| n.as_str())
                .unwrap_or_else(|| "unknown"),
        )
    }
}

impl ComponentInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_name(&mut self, name: impl Into<String>) -> &mut Self {
        self.name = Some(name.into());
        self
    }
}

impl From<String> for ComponentInfo {
    fn from(value: String) -> Self {
        Self { name: Some(value) }
    }
}
impl From<&str> for ComponentInfo {
    fn from(value: &str) -> Self {
        Self {
            name: Some(value.into()),
        }
    }
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
/// use notmad::{Component, ComponentInfo, MadError};
/// use tokio_util::sync::CancellationToken;
///
/// struct DatabaseConnection {
///     url: String,
/// }
///
/// impl Component for DatabaseConnection {
///     fn info(&self) -> ComponentInfo {
///         "database".into()
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
pub trait Component: Send + Sync + 'static {
    /// Returns an optional name for the component.
    ///
    /// This name is used in logging and error messages to identify
    /// which component is being processed.
    ///
    /// # Default
    ///
    /// Returns `None` if not overridden.
    fn info(&self) -> ComponentInfo {
        ComponentInfo::default()
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
    fn setup(&self) -> impl Future<Output = Result<(), MadError>> + Send + '_ {
        async { Err(MadError::SetupNotDefined) }
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
    fn run(
        &self,
        cancellation_token: CancellationToken,
    ) -> impl Future<Output = Result<(), MadError>> + Send + '_;

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
    fn close(&self) -> impl Future<Output = Result<(), MadError>> + Send + '_ {
        async { Err(MadError::CloseNotDefined) }
    }
}

trait AsyncComponent: Send + Sync + 'static {
    fn info_async(&self) -> ComponentInfo;

    fn setup_async(&self) -> Pin<Box<dyn Future<Output = Result<(), MadError>> + Send + '_>>;

    fn run_async(
        &self,
        cancellation_token: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<(), MadError>> + Send + '_>>;

    fn close_async(&self) -> Pin<Box<dyn Future<Output = Result<(), MadError>> + Send + '_>>;
}

impl<E: Component> AsyncComponent for E {
    #[inline(always)]
    fn info_async(&self) -> ComponentInfo {
        self.info()
    }

    #[inline(always)]
    fn setup_async(&self) -> Pin<Box<dyn Future<Output = Result<(), MadError>> + Send + '_>> {
        Box::pin(self.setup())
    }

    #[inline(always)]
    fn run_async(
        &self,
        cancellation_token: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<(), MadError>> + Send + '_>> {
        Box::pin(self.run(cancellation_token))
    }

    #[inline(always)]
    fn close_async(&self) -> Pin<Box<dyn Future<Output = Result<(), MadError>> + Send + '_>> {
        Box::pin(self.close())
    }
}

#[derive(Clone)]
pub struct SharedComponent {
    component: Arc<dyn AsyncComponent + Send + Sync + 'static>,
}

impl SharedComponent {
    #[inline(always)]
    pub fn info(&self) -> ComponentInfo {
        self.component.info_async()
    }

    #[inline(always)]
    async fn setup(&self) -> Result<(), MadError> {
        self.component.setup_async().await
    }

    #[inline(always)]
    async fn run(&self, cancellation_token: CancellationToken) -> Result<(), MadError> {
        self.component.run_async(cancellation_token).await
    }

    #[inline(always)]
    async fn close(&self) -> Result<(), MadError> {
        self.component.close_async().await
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
/// # use tokio_util::sync::CancellationToken;
///
/// struct MyService;
///
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
    fn into_component(self) -> SharedComponent;
}

impl<T: Component> IntoComponent for T {
    fn into_component(self) -> SharedComponent {
        SharedComponent {
            component: Arc::new(self),
        }
    }
}

/// A shutdown stage: one or more components that start and drain together (in
/// parallel). Create one with [`stage`], chain more components with
/// [`Stage::and`], and pass it to [`Mad::add`].
pub struct Stage {
    components: Vec<SharedComponent>,
}

impl Stage {
    /// Add another component to this stage. Members of a stage run and drain in
    /// parallel.
    pub fn and(mut self, component: impl IntoComponent) -> Self {
        self.components.push(component.into_component());
        self
    }

    /// Add a closure component to this stage — sugar for [`Stage::and`] over an
    /// `add_fn`-style closure.
    pub fn and_fn<F, Fut>(self, f: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: futures::Future<Output = Result<(), MadError>> + Send + 'static,
    {
        self.and(ClosureComponent { inner: Box::new(f) })
    }
}

/// Start a [`Stage`] from a single component; chain more with [`Stage::and`]:
///
/// ```rust
/// # use notmad::{Component, Mad, stage};
/// # use tokio_util::sync::CancellationToken;
/// # #[derive(Clone)] struct S;
/// # impl Component for S { async fn run(&self, _: CancellationToken) -> Result<(), notmad::MadError> { Ok(()) } }
/// # let (http, grpc) = (S, S);
/// Mad::builder().add(stage(http).and(grpc));
/// ```
pub fn stage(component: impl IntoComponent) -> Stage {
    Stage {
        components: vec![component.into_component()],
    }
}

/// Anything [`Mad::add`] accepts: a single component (a one-member stage), or a
/// [`Stage`] built with [`stage`] + [`Stage::and`].
///
/// `and` lives on [`Stage`], not on components, so a lone component and "a stage
/// of components" stay distinct — you opt into a stage explicitly via [`stage`].
pub trait IntoStage {
    /// Convert into a [`Stage`].
    fn into_stage(self) -> Stage;
}

impl<T: IntoComponent> IntoStage for T {
    fn into_stage(self) -> Stage {
        Stage {
            components: vec![self.into_component()],
        }
    }
}

impl IntoStage for Stage {
    fn into_stage(self) -> Stage {
        self
    }
}

/// A snapshot of the app's shutdown topology: the stages in **drain order**
/// (stage 0 drains first, each fully before the next) and the components in
/// each. Obtain it with [`Mad::topology`] and render it as a nested diagram via
/// the [`Display`](std::fmt::Display) impl, or as JSON with [`Topology::to_json`].
///
/// ```rust
/// # use notmad::{Component, Mad, stage};
/// # use tokio_util::sync::CancellationToken;
/// # #[derive(Clone)] struct S;
/// # impl Component for S {
/// #     fn info(&self) -> notmad::ComponentInfo { "svc".into() }
/// #     async fn run(&self, _: CancellationToken) -> Result<(), notmad::MadError> { Ok(()) }
/// # }
/// # let (http, grpc, publisher) = (S, S, S);
/// let mut app = Mad::builder();
/// app.add(stage(http).and(grpc)).add(publisher);
/// println!("{}", app.topology());       // pretty nested diagram
/// println!("{}", app.topology().to_json());
/// ```
#[derive(Debug, Clone)]
pub struct Topology {
    /// Stages in drain order — index 0 drains first, each fully before the next.
    pub stages: Vec<TopologyStage>,
}

/// One stage within a [`Topology`].
#[derive(Debug, Clone)]
pub struct TopologyStage {
    /// Declaration index, which is also the drain order (0 drains first).
    pub index: usize,
    /// Names of the components in this stage. They start and drain in parallel.
    pub components: Vec<String>,
}

impl Topology {
    /// Render as compact JSON (dependency-free).
    pub fn to_json(&self) -> String {
        fn esc(s: &str) -> String {
            s.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
        }
        let mut out = String::from("{\"stages\":[");
        for (i, st) in self.stages.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"index\":{},\"drain_order\":{},\"parallel\":{},\"components\":[",
                st.index,
                st.index + 1,
                st.components.len() > 1
            ));
            for (j, c) in st.components.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                out.push('"');
                out.push_str(&esc(c));
                out.push('"');
            }
            out.push_str("]}");
        }
        out.push_str("]}");
        out
    }
}

impl Display for Topology {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "notmad shutdown topology — {} stage(s)", self.stages.len())?;
        writeln!(f, "  startup:  all stages start concurrently")?;
        writeln!(
            f,
            "  shutdown: stages drain top-to-bottom, each fully before the next"
        )?;
        if self.stages.is_empty() {
            return writeln!(f, "  (no components)");
        }
        let last = self.stages.len() - 1;
        for st in &self.stages {
            let indent = "   ".repeat(st.index);
            let order = match st.index {
                0 => "drains 1st".to_string(),
                i if i == last => "drains last".to_string(),
                i => format!("drains #{}", i + 1),
            };
            let parallel = if st.components.len() > 1 {
                "  (parallel)"
            } else {
                ""
            };
            writeln!(f, "{indent}└─ stage {} — {order}{parallel}", st.index)?;
            let ci = format!("{indent}   ");
            if st.components.is_empty() {
                writeln!(f, "{ci}• (empty)")?;
            } else {
                for c in &st.components {
                    writeln!(f, "{ci}• {c}")?;
                }
            }
            if st.index != last {
                writeln!(f, "{ci}▼")?;
            }
        }
        Ok(())
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

        impl Component for FailingComponent {
            fn info(&self) -> ComponentInfo {
                "test-component".into()
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
