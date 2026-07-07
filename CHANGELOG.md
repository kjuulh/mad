# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.12.0] - 2026-07-07

> Released as 0.12.0 rather than 0.11.0: crates.io already had a `notmad 0.11.0`
> (published ahead of the git tags), so this staged-shutdown release takes the
> next free version. There is no 0.11.0 on crates.io that contains this feature.

### Added
- ordered staged graceful shutdown (add + IntoStage) (#62)
  Shutdown is now staged and ordered. `Mad::add` takes an `IntoStage` — a single
  component (a component is itself a one-member stage), or several grouped into one
  parallel stage with `stage(a).and(b).and_fn(..)`. Each `add` call is its own
  stage; stages drain in declaration order (outermost/ingress first), each fully
  before the next is cancelled, so an ingress stage finishes its in-flight work
  while the resources it depends on stay alive. Startup stays concurrent.
  
  Additive, not a breaking change: existing `add(component)` calls keep compiling
  and behaving as before, since a lone component is treated as a one-member stage.
  `and` lives on `Stage` (reached via `stage(..)`), not on components, so a lone
  component and a stage-of-components stay distinct. This is the inverse of
  go-garden's `Add` (setup add-order, teardown reverse): here add order *is* drain
  order. The combinator style mirrors mire-sagas.
  
  Components that must shut down together (the old all-at-once behaviour) go in one
  stage — `add(stage(a).and(b).and(c))`.
  
  - new `Stage` + `stage()` + `IntoStage`; `add` takes `impl IntoStage`.
  - `Mad::topology()` -> `Topology`: stages in drain order; render as a nested
  diagram (Display) or JSON (to_json), also logged at debug in run(). Mirrors
  go-garden's service-flow graphic.
  - pressure tests (ordering proven): strict N-stage order; parallel-in-stage +
  next-waits-for-all; dependency-stays-alive-during-drain; unresponsive stage
  force-stopped then next proceeds; topology shape/JSON; and an in-process
  ALB/ECS load test (24 clients, 0 dropped + correct order).
  - `staged_shutdown_demo` bin + `tests/signal.rs`: a real process driven with
  real SIGTERM / SIGKILL, asserting the OS signal path, ordered drain, honored
  pre-stop delays, zero dropped requests, and exit codes.
  - `staged_shutdown` example prints the topology and hammers ingress under load.
  - drive-by: fix existing examples/tests for rand 0.10.
  - bump 0.11 -> 0.12.
- bump v0.11
- replace async-trait with erased box type

### Fixed
- *(deps)* update rust crate rand to v0.10.2 (#63)
- *(deps)* update rust crate rand to v0.10.1 (#56)
- *(deps)* update rust-futures monorepo to v0.3.32 (#50)
- *(deps)* update rust crate rand to 0.10.0 (#49)
- *(deps)* update rust crate thiserror to v2.0.18 (#46)
- *(deps)* update rust crate tokio-util to v0.7.18 (#45)

### Other
- cuddle-please release + crates.io publish pipelines (#65)
- add Woodpecker pipeline (fmt, clippy, test, build) (#64)
- *(deps)* update rust crate anyhow to v1.0.103 (#61)
- *(deps)* update rust crate tokio to v1.52.3 (#60)
- *(deps)* update rust crate tokio to v1.52.2 (#59)
- *(deps)* update rust crate tokio to v1.52.1 (#58)
- *(deps)* update rust crate tokio to v1.52.0 (#57)
- *(deps)* update rust crate tokio to v1.51.1 (#55)
- *(deps)* update rust crate tokio to v1.51.0 (#54)
- *(deps)* update rust crate tracing-subscriber to v0.3.23 (#53)
- *(deps)* update rust crate tokio to v1.50.0 (#52)
- *(deps)* update rust crate anyhow to v1.0.102 (#51)
- *(deps)* update rust crate tracing-test to v0.2.6 (#48)
- *(deps)* update rust crate anyhow to v1.0.101 (#47)
- name() -> info() and removed async_trait
- *(deps)* update rust crate tokio to v1.49.0 (#44)
- *(deps)* update rust crate tracing to v0.1.44 (#43)
- *(deps)* update tokio-tracing monorepo (#41)

### Added
- `Mad::topology()` -> `Topology`: the stages in drain order with their
  components. Render it as a nested "middleware" diagram (its `Display` impl) or
  as JSON (`Topology::to_json`) to make the shutdown ordering obvious; it's also
  logged at `debug` in `run()`.
- `staged_shutdown_demo` binary — a real-process staged-shutdown demo under
  concurrent load, driven by process-level tests (`tests/signal.rs`) that send
  real SIGTERM / SIGKILL to verify the OS signal path, ordered drain, honored
  pre-stop delays, zero dropped requests, and exit codes.

### Changed (BREAKING)
- Ordered, staged graceful shutdown. `Mad::add` now takes an `IntoStage`: a
  single component (a component is itself a one-member stage), or several
  components grouped into one parallel stage with `stage(a).and(b).and_fn(..)`.
  **Each `add` call is its own stage.** On shutdown, stages drain in declaration
  order — the first (outermost / ingress) stage first, each drained fully before
  the next is cancelled — so an ingress stage finishes its in-flight work while
  the resources it depends on (queues, publishers, pools) are still alive.
  Startup stays concurrent across all stages.

  `and` lives on `Stage` (reached via `stage(..)`), not on components, so a lone
  component and a stage-of-components stay distinct. This is the inverse of
  go-garden's `Add` (set up in add order, tear down in reverse); here **add
  order is drain order**.

  Migration: the previous "all components shut down together" behaviour is now a
  single stage — `.add(stage(a).and(b).and(c))`. Independent, ordered layers use
  separate `.add(..)` calls. See the new `staged_shutdown` example.

## [0.10.0] - 2025-11-15

### Added
- implement take errors

## [0.9.0] - 2025-11-15

### Added
- mad not properly surfaces panics
- add publish
- add readme

### Fixed
- *(deps)* update all dependencies (#38)

### Other
- *(deps)* update rust crate tracing-subscriber to v0.3.20 (#37)

## [0.8.1] - 2025-08-09

### Other
- error logging

## [0.8.0] - 2025-08-08

### Added
- add docs
- update readme

## [0.7.5] - 2025-07-24

### Added
- print big inner

### Other
- more error correction
- correct error test to not be as verbose

## [0.7.4] - 2025-07-24

### Added
- cleanup aggregate error for single error

## [0.7.3] - 2025-07-24

### Added
- automatic conversion from anyhow::Error and access to aggregate errors

### Fixed
- *(deps)* update all dependencies (#30)

## [0.7.2] - 2025-06-25

### Added
- add wait
- add conditional, allows adding or waiting for close

### Fixed
- *(deps)* update rust crate async-trait to v0.1.86 (#28)
- *(deps)* update rust crate rand to 0.9.0 (#27)
- *(deps)* update rust crate thiserror to v2.0.11 (#26)
- *(deps)* update all dependencies (#25)
- *(deps)* update rust crate async-trait to v0.1.84 (#24)
- *(deps)* update rust crate thiserror to v2.0.9 (#22)
- *(deps)* update rust crate thiserror to v2.0.8 (#21)
- *(deps)* update rust crate thiserror to v2.0.7 (#20)
- *(deps)* update rust crate thiserror to v2.0.6 (#19)
- *(deps)* update rust crate thiserror to v2.0.5 (#18)
- *(deps)* update rust crate tokio-util to v0.7.13 (#17)

### Other
- chore

- *(deps)* update all dependencies (#29)
- *(deps)* update rust crate anyhow to v1.0.95 (#23)
- *(deps)* update all dependencies (#16)
- *(deps)* update rust crate tracing-subscriber to v0.3.19 (#15)
- *(deps)* update rust crate tracing to v0.1.41 (#13)

## [0.7.1] - 2024-11-24

### Fixed
- make sure to close on final

## [0.7.0] - 2024-11-24

### Added
- actually bubble up errors

### Fixed
- *(deps)* update rust crate thiserror to v2 (#9)

## [0.6.0] - 2024-11-23

### Added
- adding test to make sure we can gracefully shutdown
- make sure to close down properly

## [0.5.0] - 2024-11-19

### Added
- update name
- respect sigterm
- include author
- update with rename

### Docs
- add examples

## [0.4.0] - 2024-08-07

### Added
- add correction
- add small docs

## [0.3.0] - 2024-08-07

### Added
- add add_fn to execute immediate lambdas

## [0.2.1] - 2024-08-07

### Docs
- add a small readme

## [0.2.0] - 2024-08-07

### Added
- with ctrl-c signal

## [0.1.0] - 2024-08-07

### Added
- workspace package
- workspace true
- add cancellation
- add basic

### Fixed
- tests
