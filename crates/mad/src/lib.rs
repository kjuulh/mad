use futures::stream::FuturesUnordered;
use futures_util::StreamExt;
use std::{fmt::Display, sync::Arc};
use tokio::signal::unix::{signal, SignalKind};

use tokio_util::sync::CancellationToken;

#[derive(thiserror::Error, Debug)]
pub enum MadError {
    #[error("component failed: {0}")]
    Inner(#[source] anyhow::Error),

    #[error("component(s) failed: {run}")]
    RunError { run: anyhow::Error },

    #[error("component(s) failed: {close}")]
    CloseError { close: anyhow::Error },

    #[error("component(s) failed: {0}")]
    AggregateError(AggregateError),

    #[error("setup not defined")]
    SetupNotDefined,

    #[error("close not defined")]
    CloseNotDefined,
}

#[derive(Debug)]
pub struct AggregateError {
    errors: Vec<MadError>,
}

impl Display for AggregateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MadError::AggregateError: (")?;

        for error in &self.errors {
            f.write_str(&error.to_string())?;
            f.write_str(", ")?;
        }

        f.write_str(")")
    }
}

pub struct Mad {
    components: Vec<Arc<dyn Component + Send + Sync + 'static>>,

    should_cancel: Option<std::time::Duration>,
}

struct CompletionResult {
    res: Result<(), MadError>,
    name: Option<String>,
}

impl Mad {
    pub fn builder() -> Self {
        Self {
            components: Vec::default(),

            should_cancel: Some(std::time::Duration::from_millis(100)),
        }
    }

    pub fn add(&mut self, component: impl IntoComponent) -> &mut Self {
        self.components.push(component.into_component());

        self
    }

    pub fn add_fn<F, Fut>(&mut self, f: F) -> &mut Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: futures::Future<Output = Result<(), MadError>> + Send + 'static,
    {
        let comp = ClosureComponent { inner: Box::new(f) };

        self.add(comp)
    }

    pub fn cancellation(&mut self, should_cancel: Option<std::time::Duration>) -> &mut Self {
        self.should_cancel = should_cancel;

        self
    }

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
                }))
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

        for comp in &self.components {
            let comp = comp.clone();
            let cancellation_token = cancellation_token.child_token();
            let job_cancellation = job_cancellation.child_token();

            let (error_tx, error_rx) = tokio::sync::mpsc::channel::<CompletionResult>(1);
            channels.push(error_rx);

            tokio::spawn(async move {
                let name = comp.name().clone();

                tracing::debug!(component = name, "mad running");

                tokio::select! {
                    _ = cancellation_token.cancelled() => {
                        error_tx.send(CompletionResult { res: Ok(()) , name  }).await
                    }
                    res = comp.run(job_cancellation) => {
                        error_tx.send(CompletionResult { res , name  }).await
                    }
                    _ = tokio::signal::ctrl_c() => {
                        error_tx.send(CompletionResult { res: Ok(()) , name }).await
                    }
                    _ = signal_unix_terminate() => {
                        error_tx.send(CompletionResult { res: Ok(()) , name }).await
                    }
                }
            });
        }

        let mut futures = FuturesUnordered::new();
        for channel in channels.iter_mut() {
            futures.push(channel.recv());
        }

        while let Some(Some(msg)) = futures.next().await {
            match msg.res {
                Err(e) => {
                    tracing::debug!(
                        error = e.to_string(),
                        component = msg.name,
                        "component ran to completion with error"
                    );
                }
                Ok(_) => {
                    tracing::debug!(component = msg.name, "component ran to completion");
                }
            }

            job_cancellation.cancel();
            if let Some(cancel_wait) = self.should_cancel {
                tokio::time::sleep(cancel_wait).await;

                cancellation_token.cancel();
            }
        }

        tracing::debug!("ran components");

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

#[async_trait::async_trait]
pub trait Component {
    fn name(&self) -> Option<String> {
        None
    }

    async fn setup(&self) -> Result<(), MadError> {
        Err(MadError::SetupNotDefined)
    }

    async fn run(&self, cancellation_token: CancellationToken) -> Result<(), MadError>;

    async fn close(&self) -> Result<(), MadError> {
        Err(MadError::CloseNotDefined)
    }
}

pub trait IntoComponent {
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
