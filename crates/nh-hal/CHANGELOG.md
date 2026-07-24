# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/INiNiDS/nothypridle/releases/tag/v0.2.0) - 2026-07-24

### Added

- *(nh-hal)* add hardware abstraction library

### Fixed

- add version to nh-hal dependency and enable publishing
- disable package publishing in Cargo.toml
- use available_memory() instead of free_memory() for RAM monitoring
- move publish=false from Cargo.toml to release-plz.toml
- disable crates.io publishing, release-plz only creates GitHub Release

### Other

- extract nh-hal into a separate workspace crate
- merge nh-hal crate into nothypridle as nh_hal module
- release v0.0.0
- add README and BSD-3-Clause license
- configure workspace and project structure
