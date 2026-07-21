# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/INiNiDS/nothypridle/compare/v0.1.0...v0.1.1) - 2026-07-21

### Fixed

- remove cargo-deb/cargo-rpm, keep binary archive + PKGBUILD only
- use explicit version in nhidle Cargo.toml for cargo-rpm compat
- move rpm metadata to package, cd into crate dir for cargo rpm

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
