# AGENTS.md

## Overview

This repository is a Rust workspace for `nothypridle`, a Wayland idle-management daemon with D-Bus integration, AAM-based configuration, and smart inhibitor support.

Workspace members:

- `crates/nhidle` — the `nhidle` daemon binary. It handles configuration loading, Wayland, D-Bus, and listener/rule orchestration.
- `crates/nh-hal` — reusable hardware/system integrations for audio, resources, media, and GPU-related checks.

## Development commands

Run commands from the repository root:

```sh
cargo check --workspace
cargo build --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Use `cargo fmt --all` to format code. Run the narrowest relevant package command when iterating, for example `cargo check -p nothypridle` or `cargo check -p nh-hal`.

## Code conventions

- Follow standard Rust formatting and idioms; keep changes focused and avoid unrelated rewrites.
- Prefer existing workspace dependencies and preserve the workspace dependency versions in the root `Cargo.toml`.
- Use `tracing` for application diagnostics rather than ad-hoc output.
- Propagate errors with `Result` and `?` where practical; preserve useful context when crossing subsystem boundaries.
- Keep Wayland, D-Bus, configuration, and hardware integration logic in their existing modules rather than coupling unrelated concerns.
- Update `Cargo.lock` when dependency resolution changes.

## Configuration and runtime notes

The daemon reads its configuration from the platform config directory under `nothypridle/`:

- `config.aam` contains optional global sleep/lock hooks.
- `rules/` contains listener rule files loaded by the configuration loader.

Changes affecting configuration parsing, listener behavior, Wayland protocols, or D-Bus behavior should be checked against the corresponding module and documented in the README when user-facing.

## Change verification

Before submitting a change, run formatting and at least `cargo check --workspace`. For behavioral changes, also run the relevant tests or `cargo test --workspace`; use Clippy for changes touching public APIs, async code, or error handling.

