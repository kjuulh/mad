//! Waiter components for MAD.
//!
//! This module provides waiter components that simply wait for cancellation
//! without performing any work. Useful for keeping the application alive
//! or as placeholders in conditional component loading.

use tokio_util::sync::CancellationToken;

use crate::{Component, ComponentInfo, IntoComponent, MadError, SharedComponent};

/// A default waiter component that panics if run.
///
/// This is used internally as a placeholder that should never
/// actually be executed.
pub struct DefaultWaiter;
impl Component for DefaultWaiter {
    async fn run(&self, _cancellation_token: CancellationToken) -> Result<(), MadError> {
        panic!("should never be called");
    }
}

/// A wrapper component that waits for cancellation.
///
/// Instead of running the wrapped component's logic, this simply
/// waits for the cancellation token. This is useful for conditionally
/// disabling components while keeping the same structure.
///
/// # Example
///
/// ```rust,ignore
/// use mad::Waiter;
///
/// // Instead of running the service, just wait
/// let waiter = Waiter::new(service.into_component());
/// ```
pub struct Waiter {
    comp: SharedComponent,
}

impl Default for Waiter {
    fn default() -> Self {
        Self {
            comp: DefaultWaiter {}.into_component(),
        }
    }
}

impl Waiter {
    /// Creates a new waiter that wraps the given component.
    ///
    /// The wrapped component's name will be used (prefixed with "waiter/"),
    /// but its run method will not be called.
    pub fn new(c: SharedComponent) -> Self {
        Self { comp: c }
    }
}

impl Component for Waiter {
    /// Returns the name of the waiter, prefixed with "waiter/".
    ///
    /// If the wrapped component has a name, it will be "waiter/{name}".
    /// Otherwise, returns "waiter".
    fn info(&self) -> ComponentInfo {
        match &self.comp.info().name {
            Some(name) => format!("waiter/{name}").into(),
            None => "waiter".into(),
        }
    }

    /// Waits for cancellation without performing any work.
    ///
    /// This method simply waits for the cancellation token to be triggered,
    /// then returns successfully.
    async fn run(&self, cancellation_token: CancellationToken) -> Result<(), MadError> {
        cancellation_token.cancelled().await;

        Ok(())
    }
}
