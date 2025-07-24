# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
