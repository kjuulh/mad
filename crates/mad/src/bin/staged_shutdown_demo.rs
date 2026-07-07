//! Real-process demo of staged graceful shutdown, for exercising the actual OS
//! signal path (SIGTERM/SIGKILL), process exit codes, and that drain delays are
//! honored — things the in-process tests can't cover.
//!
//! It runs a staged app under concurrent load:
//!   stage 0 — ingress (drains first, after a pre-stop delay),
//!   stage 1 — worker,
//!   stage 2 — publisher (the dependency; torn down last).
//!
//! It prints `READY pid=<pid>` once serving, then blocks until a signal. On
//! SIGTERM it drains stage-by-stage (honoring the ingress pre-stop delay), then
//! prints a machine-parseable `RESULT ...` line and exits 0. Output lines are
//! flushed so a parent process can read them live.
//!
//! Manual use:
//!   cargo run -p notmad --bin staged_shutdown_demo &
//!   kill -TERM %1     # watch the ordered drain
//!
//! It's driven by `tests/signal.rs` (spawn, send real signals, assert).

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use notmad::{Component, ComponentInfo, Mad, MadError, stage};
use tokio_util::sync::CancellationToken;

/// Print a line and flush immediately (stdout is block-buffered when piped).
fn emit(line: impl AsRef<str>) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", line.as_ref());
    let _ = out.flush();
}

fn drain_ms() -> u64 {
    std::env::var("DEMO_DRAIN_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(250)
}

#[derive(Clone)]
struct Publisher {
    alive: Arc<AtomicBool>,
}
impl Publisher {
    async fn publish(&self) -> bool {
        if !self.alive.load(Ordering::SeqCst) {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(15)).await;
        true
    }
}
impl Component for Publisher {
    fn info(&self) -> ComponentInfo {
        "publisher".into()
    }
    async fn run(&self, cancel: CancellationToken) -> Result<(), MadError> {
        cancel.cancelled().await;
        self.alive.store(false, Ordering::SeqCst);
        emit("PUBLISHER closed");
        Ok(())
    }
}

struct Worker;
impl Component for Worker {
    fn info(&self) -> ComponentInfo {
        "worker".into()
    }
    async fn run(&self, cancel: CancellationToken) -> Result<(), MadError> {
        cancel.cancelled().await;
        emit("WORKER stopped");
        Ok(())
    }
}

#[derive(Clone)]
struct Ingress {
    publisher: Publisher,
    accepting: Arc<AtomicBool>,
    inflight: Arc<AtomicU64>,
    served: Arc<AtomicU64>,
    dropped: Arc<AtomicU64>,
}
impl Ingress {
    async fn handle(&self) {
        if !self.accepting.load(Ordering::SeqCst) {
            return;
        }
        self.inflight.fetch_add(1, Ordering::SeqCst);
        if self.publisher.publish().await {
            self.served.fetch_add(1, Ordering::SeqCst);
        } else {
            self.dropped.fetch_add(1, Ordering::SeqCst);
        }
        self.inflight.fetch_sub(1, Ordering::SeqCst);
    }
}
impl Component for Ingress {
    fn info(&self) -> ComponentInfo {
        "ingress".into()
    }
    async fn run(&self, cancel: CancellationToken) -> Result<(), MadError> {
        cancel.cancelled().await;
        emit("INGRESS draining");
        // Pre-stop delay: keep accepting while the LB deregisters.
        tokio::time::sleep(Duration::from_millis(drain_ms())).await;
        self.accepting.store(false, Ordering::SeqCst);
        while self.inflight.load(Ordering::SeqCst) > 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        emit("INGRESS drained");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let publisher = Publisher {
        alive: Arc::new(AtomicBool::new(true)),
    };
    let ingress = Ingress {
        publisher: publisher.clone(),
        accepting: Arc::new(AtomicBool::new(true)),
        inflight: Arc::new(AtomicU64::new(0)),
        served: Arc::new(AtomicU64::new(0)),
        dropped: Arc::new(AtomicU64::new(0)),
    };

    // Background load — concurrent clients hammering ingress through shutdown.
    let stop = Arc::new(AtomicBool::new(false));
    let mut clients = Vec::new();
    for _ in 0..16 {
        let ing = ingress.clone();
        let stop = stop.clone();
        clients.push(tokio::spawn(async move {
            while !stop.load(Ordering::SeqCst) {
                ing.handle().await;
                tokio::time::sleep(Duration::from_millis(3)).await;
            }
        }));
    }

    let mut app = Mad::builder();
    app.add(stage(ingress.clone()).and(ingress.clone())) // 2 ingress "listeners" in one stage
        .add(Worker)
        .add(publisher.clone())
        .cancellation(Some(Duration::from_secs(30)));

    emit(format!("READY pid={}", std::process::id()));

    // Blocks until SIGTERM/SIGINT; notmad drives the ordered drain.
    app.run().await?;

    stop.store(true, Ordering::SeqCst);
    for c in clients {
        let _ = c.await;
    }

    emit(format!(
        "RESULT served={} dropped={}",
        ingress.served.load(Ordering::SeqCst),
        ingress.dropped.load(Ordering::SeqCst)
    ));
    emit("DONE");
    Ok(())
}
