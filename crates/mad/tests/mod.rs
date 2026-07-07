use std::sync::Arc;

use notmad::{Component, ComponentInfo, Mad, MadError, stage};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing_test::traced_test;

struct NeverEndingRun {}

impl Component for NeverEndingRun {
    fn info(&self) -> ComponentInfo {
        "NeverEndingRun".into()
    }

    async fn run(&self, _cancellation: CancellationToken) -> Result<(), notmad::MadError> {
        let millis_wait = rand::random_range(50..1000);

        tokio::time::sleep(std::time::Duration::from_millis(millis_wait)).await;

        Ok(())
    }
}

#[tokio::test]
#[traced_test]
async fn test_can_run_components() -> anyhow::Result<()> {
    Mad::builder()
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .add(NeverEndingRun {})
        .run()
        .await?;

    Ok(())
}

#[tokio::test]
#[traced_test]
async fn test_can_shutdown_gracefully() -> anyhow::Result<()> {
    let check = Arc::new(Mutex::new(None));

    Mad::builder()
        .add_fn({
            let check = check.clone();

            move |cancel| {
                let check = check.clone();

                async move {
                    let start = std::time::SystemTime::now();
                    tracing::info!("waiting for cancel");
                    cancel.cancelled().await;
                    tracing::info!("submitting check");
                    let mut check = check.lock().await;
                    let elapsed = start.elapsed().expect("to be able to get elapsed");
                    *check = Some(elapsed);
                    tracing::info!("check submitted");

                    Ok(())
                }
            }
        })
        .add_fn(|_| async move {
            tracing::info!("starting sleep");
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            tracing::info!("sleep ended");

            Ok(())
        })
        .run()
        .await?;

    let check = check
        .lock()
        .await
        .expect("to be able to get a duration from cancel");

    // We default wait 100 ms for graceful shutdown, and we explicitly wait 100ms in the sleep routine
    tracing::info!("check millis: {}", check.as_millis());
    assert!(check.as_millis() < 250);

    Ok(())
}

#[tokio::test]
#[traced_test]
async fn test_component_panics_shutdowns_cleanly() -> anyhow::Result<()> {
    let res = Mad::builder()
        .add_fn({
            move |_cancel| async move {
                panic!("my inner panic");
            }
        })
        .add_fn(|cancel| async move {
            cancel.cancelled().await;

            Ok(())
        })
        .run()
        .await;

    let err_content = res.unwrap_err().to_string();
    assert!(err_content.contains("component panicked"));
    assert!(err_content.contains("my inner panic"));

    Ok(())
}

/// Components declared in earlier stages must fully drain before later stages
/// are cancelled — and a later stage (a dependency) must stay alive while an
/// earlier stage drains its in-flight work.
#[tokio::test]
#[traced_test]
async fn shuts_down_stages_in_declaration_order() -> anyhow::Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};

    let order = Arc::new(Mutex::new(Vec::<String>::new()));
    // True while the later-stage "resource" is still running.
    let resource_alive = Arc::new(AtomicBool::new(false));
    // Set by ingress: was the resource still alive when ingress finished draining?
    let saw_resource_during_drain = Arc::new(AtomicBool::new(false));

    // Stage 0 (ingress): on cancel, drain for a bit (still "serving"), then check
    // its dependency is still up and record its stop.
    struct Ingress {
        order: Arc<Mutex<Vec<String>>>,
        resource_alive: Arc<AtomicBool>,
        saw_resource: Arc<AtomicBool>,
    }
    impl Component for Ingress {
        fn info(&self) -> ComponentInfo {
            "ingress".into()
        }
        async fn run(&self, cancel: CancellationToken) -> Result<(), MadError> {
            cancel.cancelled().await;
            // Pre-stop drain window — dependencies must remain available here.
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            self.saw_resource
                .store(self.resource_alive.load(Ordering::SeqCst), Ordering::SeqCst);
            self.order.lock().await.push("ingress".to_string());
            Ok(())
        }
    }

    // Stage 1 (resource): alive from start; torn down only after ingress stops.
    struct Resource {
        order: Arc<Mutex<Vec<String>>>,
        alive: Arc<AtomicBool>,
    }
    impl Component for Resource {
        fn info(&self) -> ComponentInfo {
            "resource".into()
        }
        async fn run(&self, cancel: CancellationToken) -> Result<(), MadError> {
            self.alive.store(true, Ordering::SeqCst);
            cancel.cancelled().await;
            self.alive.store(false, Ordering::SeqCst);
            self.order.lock().await.push("resource".to_string());
            Ok(())
        }
    }

    Mad::builder()
        // stage 0 — ingress.
        .add(Ingress {
            order: order.clone(),
            resource_alive: resource_alive.clone(),
            saw_resource: saw_resource_during_drain.clone(),
        })
        // stage 1 — the dependency, torn down only after ingress drains.
        .add(Resource {
            order: order.clone(),
            alive: resource_alive.clone(),
        })
        // stage 2 — trigger: returns shortly after start, beginning shutdown.
        .add_fn(|_cancel| async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            Ok(())
        })
        // Generous per-stage grace so nothing is force-stopped.
        .cancellation(Some(std::time::Duration::from_secs(5)))
        .run()
        .await?;

    let order = order.lock().await;
    let ingress_idx = order
        .iter()
        .position(|s| s == "ingress")
        .expect("ingress stopped");
    let resource_idx = order
        .iter()
        .position(|s| s == "resource")
        .expect("resource stopped");

    assert!(
        ingress_idx < resource_idx,
        "ingress (stage 0) must stop before its resource (stage 1): {:?}",
        *order
    );
    assert!(
        saw_resource_during_drain.load(Ordering::SeqCst),
        "the resource must stay alive while ingress drains its in-flight work"
    );

    Ok(())
}

/// Three stages must drain strictly in declaration order (0, then 1, then 2),
/// each fully before the next.
#[tokio::test]
#[traced_test]
async fn drains_stages_in_strict_declaration_order() -> anyhow::Result<()> {
    let order = Arc::new(Mutex::new(Vec::<usize>::new()));

    struct Recorder {
        idx: usize,
        order: Arc<Mutex<Vec<usize>>>,
    }
    impl Component for Recorder {
        async fn run(&self, cancel: CancellationToken) -> Result<(), MadError> {
            cancel.cancelled().await;
            self.order.lock().await.push(self.idx);
            Ok(())
        }
    }

    Mad::builder()
        .add(Recorder { idx: 0, order: order.clone() })
        .add(Recorder { idx: 1, order: order.clone() })
        .add(Recorder { idx: 2, order: order.clone() })
        // trigger begins shutdown shortly after start
        .add_fn(|_| async {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            Ok(())
        })
        .cancellation(Some(std::time::Duration::from_secs(5)))
        .run()
        .await?;

    assert_eq!(
        *order.lock().await,
        vec![0, 1, 2],
        "stages must drain in declaration order"
    );
    Ok(())
}

/// Members of one stage are cancelled together (parallel), and the next stage is
/// not cancelled until ALL members of the current stage have finished draining —
/// even the slowest.
#[tokio::test]
#[traced_test]
async fn stage_members_drain_in_parallel_and_next_waits_for_all() -> anyhow::Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};

    let slow_done = Arc::new(AtomicBool::new(false));
    let next_saw_all_done = Arc::new(AtomicBool::new(false));

    // Slow stage-0 member: drains for 200ms.
    struct Slow {
        done: Arc<AtomicBool>,
    }
    impl Component for Slow {
        async fn run(&self, cancel: CancellationToken) -> Result<(), MadError> {
            cancel.cancelled().await;
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            self.done.store(true, Ordering::SeqCst);
            Ok(())
        }
    }
    // Fast stage-0 member: drains immediately.
    struct Fast;
    impl Component for Fast {
        async fn run(&self, cancel: CancellationToken) -> Result<(), MadError> {
            cancel.cancelled().await;
            Ok(())
        }
    }
    // Stage-1 member: when IT is cancelled, the slow stage-0 member must already
    // be done — proving stage 0 fully drained before stage 1 was touched.
    struct Next {
        slow_done: Arc<AtomicBool>,
        saw: Arc<AtomicBool>,
    }
    impl Component for Next {
        async fn run(&self, cancel: CancellationToken) -> Result<(), MadError> {
            cancel.cancelled().await;
            self.saw
                .store(self.slow_done.load(Ordering::SeqCst), Ordering::SeqCst);
            Ok(())
        }
    }

    Mad::builder()
        .add(stage(Slow { done: slow_done.clone() }).and(Fast)) // stage 0 (parallel)
        .add(Next {
            slow_done: slow_done.clone(),
            saw: next_saw_all_done.clone(),
        }) // stage 1
        .add_fn(|_| async {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            Ok(())
        })
        .cancellation(Some(std::time::Duration::from_secs(5)))
        .run()
        .await?;

    assert!(
        next_saw_all_done.load(Ordering::SeqCst),
        "stage 1 must not be cancelled until every member of stage 0 (incl. the slow one) has drained"
    );
    Ok(())
}

/// A stage that ignores its cancellation token is force-stopped after the
/// per-stage grace, and shutdown still advances to the next stage.
#[tokio::test]
#[traced_test]
async fn unresponsive_stage_is_forced_then_next_proceeds() -> anyhow::Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};

    let next_ran = Arc::new(AtomicBool::new(false));

    // Ignores cancellation entirely; would block for 10s if not force-stopped.
    struct Stuck;
    impl Component for Stuck {
        async fn run(&self, _cancel: CancellationToken) -> Result<(), MadError> {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            Ok(())
        }
    }
    struct After {
        ran: Arc<AtomicBool>,
    }
    impl Component for After {
        async fn run(&self, cancel: CancellationToken) -> Result<(), MadError> {
            cancel.cancelled().await;
            self.ran.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    let start = std::time::Instant::now();
    Mad::builder()
        .add(Stuck) // stage 0 — never responds to cancel
        .add(After { ran: next_ran.clone() }) // stage 1
        .add_fn(|_| async {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            Ok(())
        })
        .cancellation(Some(std::time::Duration::from_millis(200))) // per-stage grace
        .run()
        .await?;
    let elapsed = start.elapsed();

    assert!(
        next_ran.load(Ordering::SeqCst),
        "shutdown must advance past a force-stopped stage"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(3),
        "stuck stage must be force-stopped after grace, not waited out (took {elapsed:?})"
    );
    Ok(())
}

/// `topology()` reflects the stages in drain order, marks the parallel stage,
/// and renders both a diagram and JSON.
#[tokio::test]
async fn topology_reports_stages_in_drain_order() -> anyhow::Result<()> {
    struct Named(&'static str);
    impl Component for Named {
        fn info(&self) -> ComponentInfo {
            self.0.into()
        }
        async fn run(&self, cancel: CancellationToken) -> Result<(), MadError> {
            cancel.cancelled().await;
            Ok(())
        }
    }

    let mut app = Mad::builder();
    app.add(stage(Named("http")).and(Named("grpc"))) // stage 0 — parallel
        .add(Named("publisher")); // stage 1

    let topo = app.topology();
    assert_eq!(topo.stages.len(), 2);
    assert_eq!(topo.stages[0].components, vec!["http", "grpc"]);
    assert_eq!(topo.stages[1].components, vec!["publisher"]);

    let json = topo.to_json();
    assert!(json.contains("\"parallel\":true"), "json: {json}");
    assert!(json.contains("\"http\""));

    let diagram = topo.to_string();
    assert!(diagram.contains("drains 1st"), "diagram:\n{diagram}");
    assert!(diagram.contains("drains last"), "diagram:\n{diagram}");
    assert!(diagram.contains("(parallel)"), "diagram:\n{diagram}");

    Ok(())
}

/// End-to-end ALB/ECS scenario under concurrent load: an ingress stage serving
/// requests that each use a downstream `publisher` in a *later* stage. While the
/// load balancer keeps hammering us through shutdown, staged draining must keep
/// the publisher alive until ingress has drained — so **no in-flight request
/// fails because its dependency shut down first** (the deploy-rollover 502).
///
/// This would fail if all components were cancelled at once (single stage): the
/// publisher could close while ingress is still draining in-flight requests.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[traced_test]
async fn alb_load_ordered_shutdown_drops_no_inflight_request() -> anyhow::Result<()> {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    // A later-stage dependency. Publishing after it shuts down is a failure.
    #[derive(Clone)]
    struct Publisher {
        alive: Arc<AtomicBool>,
        // Set from `ingress_drained` at the moment the publisher is cancelled —
        // must be true (ingress drained before we tore the publisher down).
        ingress_drained: Arc<AtomicBool>,
        closed_after_ingress: Arc<AtomicBool>,
    }
    impl Publisher {
        async fn publish(&self) -> bool {
            if !self.alive.load(Ordering::SeqCst) {
                return false;
            }
            tokio::time::sleep(std::time::Duration::from_millis(15)).await;
            true
        }
    }
    impl Component for Publisher {
        fn info(&self) -> ComponentInfo {
            "publisher".into()
        }
        async fn run(&self, cancel: CancellationToken) -> Result<(), MadError> {
            cancel.cancelled().await;
            self.closed_after_ingress
                .store(self.ingress_drained.load(Ordering::SeqCst), Ordering::SeqCst);
            self.alive.store(false, Ordering::SeqCst);
            Ok(())
        }
    }

    // The ALB-facing ingress. Pre-stop delay keeps accepting while the LB
    // deregisters, then it stops accepting and drains in-flight.
    #[derive(Clone)]
    struct Ingress {
        publisher: Publisher,
        accepting: Arc<AtomicBool>,
        inflight: Arc<AtomicU64>,
        served: Arc<AtomicU64>,
        failed: Arc<AtomicU64>,
        drained: Arc<AtomicBool>,
    }
    impl Ingress {
        async fn handle(&self) {
            if !self.accepting.load(Ordering::SeqCst) {
                return; // like a 503 — the ALB reroutes to a healthy task
            }
            self.inflight.fetch_add(1, Ordering::SeqCst);
            if self.publisher.publish().await {
                self.served.fetch_add(1, Ordering::SeqCst);
            } else {
                self.failed.fetch_add(1, Ordering::SeqCst);
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
            tokio::time::sleep(std::time::Duration::from_millis(80)).await; // pre-stop
            self.accepting.store(false, Ordering::SeqCst);
            while self.inflight.load(Ordering::SeqCst) > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            self.drained.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    let ingress_drained = Arc::new(AtomicBool::new(false));
    let publisher_closed_after_ingress = Arc::new(AtomicBool::new(false));
    let publisher = Publisher {
        alive: Arc::new(AtomicBool::new(true)),
        ingress_drained: ingress_drained.clone(),
        closed_after_ingress: publisher_closed_after_ingress.clone(),
    };
    let ingress = Ingress {
        publisher: publisher.clone(),
        accepting: Arc::new(AtomicBool::new(true)),
        inflight: Arc::new(AtomicU64::new(0)),
        served: Arc::new(AtomicU64::new(0)),
        failed: Arc::new(AtomicU64::new(0)),
        drained: ingress_drained.clone(),
    };

    // The "ALB": 24 concurrent clients hammering ingress right through shutdown.
    let stop = Arc::new(AtomicBool::new(false));
    let mut clients = Vec::new();
    for _ in 0..24 {
        let ing = ingress.clone();
        let stop = stop.clone();
        clients.push(tokio::spawn(async move {
            while !stop.load(Ordering::SeqCst) {
                ing.handle().await;
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
        }));
    }

    Mad::builder()
        .add(ingress.clone()) // stage 0 — ingress, drains first
        .add(publisher.clone()) // stage 1 — dependency, torn down after ingress
        // trigger shutdown after some load has built up
        .add_fn(|_| async {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            Ok(())
        })
        .cancellation(Some(std::time::Duration::from_secs(5)))
        .run()
        .await?;

    stop.store(true, Ordering::SeqCst);
    for c in clients {
        let _ = c.await;
    }

    let served = ingress.served.load(Ordering::SeqCst);
    let failed = ingress.failed.load(Ordering::SeqCst);
    assert!(served > 0, "the load generator should have served real requests");
    // 0 dropped connections: nothing failed mid-flight from a torn-down dependency.
    assert_eq!(
        failed, 0,
        "no in-flight request may fail because the publisher (a later stage) shut down first; \
         served={served}, failed={failed}"
    );
    // Proper shutdown order: the publisher was cancelled only after ingress had
    // fully drained.
    assert!(
        publisher_closed_after_ingress.load(Ordering::SeqCst),
        "publisher (stage 1) must shut down only after ingress (stage 0) has fully drained"
    );
    Ok(())
}

#[test]
fn test_can_easily_transform_error() -> anyhow::Result<()> {
    fn fallible() -> anyhow::Result<()> {
        Ok(())
    }

    fn inner() -> Result<(), MadError> {
        fallible()?;

        Ok(())
    }

    inner()?;

    Ok(())
}
