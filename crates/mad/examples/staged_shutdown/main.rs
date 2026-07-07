//! Ordered (staged) graceful shutdown under load.
//!
//! This models the exact production shape that motivated [`Mad::stage`]: a
//! service behind an AWS ALB. The load balancer keeps sending requests to a
//! task for a short window *after* ECS starts stopping it (target-group
//! deregistration isn't instant), and every request depends on a downstream
//! resource (here a "publisher"). If the whole process tears down at once, the
//! publisher can close while the ingress is still draining in-flight requests —
//! and those requests fail even though healthy tasks were ready to serve.
//!
//! Stages fix that. We declare:
//!   * stage 0 — ingress (`http` + `grpc`), drains first, in parallel;
//!   * stage 1 — a background `worker`;
//!   * stage 2 — the `publisher` resource the ingress depends on.
//!
//! On shutdown MAD cancels stage 0 and waits for it to fully drain (the
//! publisher in stage 2 stays alive the whole time), then stage 1, then stage 2.
//! A background "ALB" hammers the ingress throughout. The invariant we assert:
//! **zero requests fail because a dependency shut down first.**
//!
//! Try collapsing everything into one stage (delete the `.stage()` calls) and
//! re-run: you'll see `FAILED (dependency gone mid-flight)` climb above zero.
//!
//! Run with: `cargo run -p notmad --example staged_shutdown`

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use notmad::{Component, ComponentInfo, Mad, MadError, stage};
use tokio_util::sync::CancellationToken;

/// Downstream resource every request depends on (stage 2). Publishing after it
/// has shut down is a failure — exactly what ordered shutdown must prevent.
#[derive(Clone)]
struct Publisher {
    alive: Arc<AtomicBool>,
}

impl Publisher {
    fn new() -> Self {
        Self {
            alive: Arc::new(AtomicBool::new(true)),
        }
    }

    async fn publish(&self) -> Result<(), &'static str> {
        if !self.alive.load(Ordering::SeqCst) {
            return Err("publisher already shut down");
        }
        // Simulate upstream work (Kafka produce, DB write, ...).
        tokio::time::sleep(Duration::from_millis(20)).await;
        Ok(())
    }
}

impl Component for Publisher {
    fn info(&self) -> ComponentInfo {
        "publisher".into()
    }

    async fn run(&self, cancel: CancellationToken) -> Result<(), MadError> {
        cancel.cancelled().await;
        println!("  [publisher] shutting down (closing connections)");
        self.alive.store(false, Ordering::SeqCst);
        Ok(())
    }
}

/// Metrics shared between the ingress components and the load generator.
#[derive(Clone, Default)]
struct Stats {
    served: Arc<AtomicU64>,
    // Refused because we already stopped accepting — fine: a real ALB reroutes
    // these to a healthy task.
    rejected: Arc<AtomicU64>,
    // Failed because the publisher was gone mid-flight — the bug staged
    // shutdown exists to make impossible.
    failed_dependency: Arc<AtomicU64>,
}

/// The ALB-facing server (stage 0). Accepts requests, each of which uses the
/// publisher. On shutdown: a short pre-stop delay (keep accepting while ALB
/// deregistration propagates), then stop accepting and drain in-flight.
#[derive(Clone)]
struct Ingress {
    name: &'static str,
    publisher: Publisher,
    accepting: Arc<AtomicBool>,
    inflight: Arc<AtomicU64>,
    stats: Stats,
}

impl Ingress {
    fn new(name: &'static str, publisher: Publisher, stats: Stats) -> Self {
        Self {
            name,
            publisher,
            accepting: Arc::new(AtomicBool::new(true)),
            inflight: Arc::new(AtomicU64::new(0)),
            stats,
        }
    }

    /// Handle one incoming request (as if from the ALB).
    async fn handle(&self) {
        if !self.accepting.load(Ordering::SeqCst) {
            self.stats.rejected.fetch_add(1, Ordering::SeqCst);
            return;
        }
        self.inflight.fetch_add(1, Ordering::SeqCst);
        match self.publisher.publish().await {
            Ok(()) => {
                self.stats.served.fetch_add(1, Ordering::SeqCst);
            }
            Err(_) => {
                self.stats.failed_dependency.fetch_add(1, Ordering::SeqCst);
            }
        }
        self.inflight.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Component for Ingress {
    fn info(&self) -> ComponentInfo {
        self.name.into()
    }

    async fn run(&self, cancel: CancellationToken) -> Result<(), MadError> {
        cancel.cancelled().await;

        // Pre-stop delay: keep accepting so requests the ALB already routed here
        // (before deregistration propagated) still land on a live listener.
        println!(
            "  [{}] SIGTERM -> pre-stop delay (still accepting; LB deregistering)",
            self.name
        );
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Stop accepting new work, then drain what's in-flight. The publisher
        // (stage 2) is still alive here — that's the whole point.
        self.accepting.store(false, Ordering::SeqCst);
        println!(
            "  [{}] stopped accepting; draining {} in-flight",
            self.name,
            self.inflight.load(Ordering::SeqCst)
        );
        while self.inflight.load(Ordering::SeqCst) > 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        println!("  [{}] drained, exiting", self.name);
        Ok(())
    }
}

/// Example scaffolding: begins shutdown after a delay (stands in for an OS
/// SIGTERM so the example is self-contained). Named so it reads cleanly in the
/// topology diagram.
struct ShutdownAfter(Duration);

impl Component for ShutdownAfter {
    fn info(&self) -> ComponentInfo {
        "shutdown-timer".into()
    }

    async fn run(&self, _cancel: CancellationToken) -> Result<(), MadError> {
        tokio::time::sleep(self.0).await;
        println!("--- shutdown initiated (ordered, stage by stage) ---");
        Ok(())
    }
}

/// A middle-tier background worker (stage 1).
struct Worker;

impl Component for Worker {
    fn info(&self) -> ComponentInfo {
        "worker".into()
    }

    async fn run(&self, cancel: CancellationToken) -> Result<(), MadError> {
        cancel.cancelled().await;
        println!("  [worker] shutting down");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let publisher = Publisher::new();
    let stats = Stats::default();
    let http = Ingress::new("http", publisher.clone(), stats.clone());
    let grpc = Ingress::new("grpc", publisher.clone(), stats.clone());

    // The "ALB": several concurrent clients hammering the ingress the whole time,
    // including right through shutdown.
    let load_cancel = CancellationToken::new();
    let mut generators = Vec::new();
    for _ in 0..16 {
        let http = http.clone();
        let lc = load_cancel.clone();
        generators.push(tokio::spawn(async move {
            while !lc.is_cancelled() {
                http.handle().await;
                // Pace each client so the reject path (no internal await) can't
                // busy-spin and starve the runtime once we stop accepting.
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }));
    }

    let mut app = Mad::builder();
    app
        // stage 0 — ingress; http + grpc drain first, in parallel.
        .add(stage(http.clone()).and(grpc.clone()))
        // stage 1 — background worker.
        .add(Worker)
        // stage 2 — the resource the ingress depends on; torn down last.
        .add(publisher.clone())
        // stage 3 — begins shutdown after 1s of load (stands in for SIGTERM).
        .add(ShutdownAfter(Duration::from_secs(1)))
        .cancellation(Some(Duration::from_secs(10)));

    // The shutdown topology — the "middleware diagram" of drain order.
    print!("{}", app.topology());
    println!("json: {}\n", app.topology().to_json());

    println!("serving under load; SIGTERM simulated in 1s...\n");

    app.run().await?;

    // Stop the load generator now that the service is down.
    load_cancel.cancel();
    for g in generators {
        let _ = g.await;
    }

    let served = stats.served.load(Ordering::SeqCst);
    let rejected = stats.rejected.load(Ordering::SeqCst);
    let failed = stats.failed_dependency.load(Ordering::SeqCst);

    println!("\n=== summary ===");
    println!("served ok:                              {served}");
    println!("rejected after draining (LB reroutes):  {rejected}");
    println!("FAILED (dependency gone mid-flight):    {failed}");

    assert_eq!(
        failed, 0,
        "ordered shutdown must keep the publisher alive until ingress has drained"
    );
    println!(
        "\nOrdered shutdown kept the publisher alive until ingress drained -> zero dependency failures."
    );

    Ok(())
}
