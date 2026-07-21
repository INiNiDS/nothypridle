# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.0](https://github.com/INiNiDS/nothypridle/releases/tag/v0.0.0) - 2026-07-21

### Added

- *(nhidle)* add idle monitor with smart inhibitors
- *(nhidle)* add D-Bus systemd-logind listener
- *(nhidle)* add Wayland protocol client
- *(nhidle)* add shell command runner
- *(nhidle)* add AAM-based configuration loader
- *(nhidle)* add daemon entry point and app orchestration

### Fixed

- update tag references to v{version} format
- use path+version for nh-hal dep instead of workspace inheritance
- specify nh-hal version explicitly in nhidle dependency
- add tracing dep, binstall metadata, and CI check+clippy workflow
- add missing tracing dep to nhidle, add cargo check to CI
- move publish=false from Cargo.toml to release-plz.toml
- disable crates.io publishing, release-plz only creates GitHub Release

### Other

- merge nh-hal crate into nothypridle as nh_hal module
- release v0.0.0
- add README and BSD-3-Clause license
- *(nhidle)* add integration tests
- configure workspace and project structure

## [0.1.0](https://github.com/INiNiDS/nothypridle/releases/tag/nothypridle-v0.1.0) - 2026-07-21

### Added

- *(nhidle)* add idle monitor with smart inhibitors
- *(nhidle)* add D-Bus systemd-logind listener
- *(nhidle)* add Wayland protocol client
- *(nhidle)* add shell command runner
- *(nhidle)* add AAM-based configuration loader
- *(nhidle)* add daemon entry point and app orchestration

### Fixed

- move publish=false from Cargo.toml to release-plz.toml
- disable crates.io publishing, release-plz only creates GitHub Release

### Other

- add README and BSD-3-Clause license
- *(nhidle)* add integration tests
- configure workspace and project structure
