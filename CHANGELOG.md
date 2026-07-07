# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
