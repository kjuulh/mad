use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::{Component, MadError};

pub struct Waiter {
    comp: Arc<dyn Component + Send + Sync + 'static>,
}

impl Waiter {
    pub fn new(c: Arc<dyn Component + Send + Sync + 'static>) -> Self {
        Self { comp: c }
    }
}

#[async_trait]
impl Component for Waiter {
    fn name(&self) -> Option<String> {
        match self.comp.name() {
            Some(name) => Some(format!("waiter/{name}")),
            None => Some("waiter".into()),
        }
    }

    async fn run(&self, cancellation_token: CancellationToken) -> Result<(), MadError> {
        cancellation_token.cancelled().await;

        Ok(())
    }
}
