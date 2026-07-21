# nothypridle

A Wayland idle-management daemon with D-Bus integration, [AAM](https://aam.ininids.in.rs/)-based
configuration, and smart inhibitor support. Written in Rust as a drop-in replacement
for [hypridle](https://github.com/hyprwm/hypridle).

`nothypridle` watches per-rule idle timers through the Wayland `ext-idle-notification-v1`
protocol, and before firing `on_timeout` it checks a set of configurable inhibitors
(CPU/GPU/RAM/VRAM usage, running keep-alive processes, active media playback, fullscreen
toplevels). It also listens to `systemd-logind` D-Bus signals so global `before_sleep` /
`after_sleep` / `lock` / `unlock` hooks can be run independently of any rule.

## Features

- **Rule-based idle management.** Each rule file in `rules/` declares its own
  `timeout` plus the conditions under which it should be inhibited and the
  commands to run on idle/resume.
- **Smart inhibitors.** A rule is skipped when the system is busy: high CPU/GPU
  usage, low free RAM/VRAM, a configured keep-alive process is running, audio is
  playing, or a toplevel is fullscreen.
- **Multi-vendor GPU monitoring.** Free VRAM and GPU busy percent are sampled
  from AMD (`amdgpu` sysfs), NVIDIA (NVML) and Intel (`i915`/`xe` sysfs) devices,
  taking the busiest GPU / most-constrained VRAM across cards.
- **systemd-logind hooks.** `PrepareForSleep(true/false)` and session
  `Lock`/`Unlock` signals trigger optional global commands.
- **Wayland-native.** Uses `ext-idle-notification-v1` for idle notifications and
  `wlr-foreign-toplevel-management-v1` (when supported by the compositor) for
  fullscreen inhibit.
- **Crash-safe error handling.** Configuration / D-Bus / Wayland failures are
  propagated as `Result`s instead of calling `process::exit`, so the daemon can
  be supervised and restarted cleanly by systemd.

## Requirements

- A Wayland compositor that advertises `ext_idle_notifier_v1` (Hyprland, KDE
  Plasma 6, sway-latest, etc.).
- Optional: `zwlr_foreign_toplevel_manager_v1` for fullscreen-aware inhibit.
  Without it the fullscreen inhibitor is inert (all other checks still work).
- `systemd` (provides `org.freedesktop.login1` on the system bus).
- Rust 1.85+ (edition 2024) to build from source.

## Building

```sh
cargo build --release --workspace
```

The daemon binary is `target/release/nhidle`.

For a strict clean-code gate (treats quality violations as hard errors):

```sh
DEPLOY=1 cargo build --release --workspace
```

## Installation

### From source

```sh
cargo install --path crates/nhidle --locked
```

This places `nhidle` in `~/.cargo/bin`. Make sure that directory is on `$PATH`.

### Manual install

```sh
cargo build --release --workspace
install -Dm755 target/release/nhidle /usr/local/bin/nhidle
install -Dm644 dist/nhidle-cargo.service /usr/lib/systemd/user/nhidle-cargo.service
install -Dm644 dist/schema.aam /usr/share/doc/nothypridle/schema.aam
install -Dm644 dist/config.example.aam /usr/share/doc/nothypridle/config.example.aam
install -Dm644 dist/rules.example.aam /usr/share/doc/nothypridle/rules.example.aam
```

### Packaging

A systemd user unit and example configuration files are shipped under
[`dist/`](./dist). Distribution packagers can install them to the standard
locations shown above. The Cargo manifest already carries the metadata required
by `cargo install` (`description`, `license`, `repository`, `keywords`,
`categories`).

## Running

```sh
nhidle
```

The daemon reads its configuration from the platform config directory
(`$XDG_CONFIG_HOME/nothypridle/`, defaulting to `~/.config/nothypridle/`):

```
~/.config/nothypridle/
├── config.aam         # optional global hooks
└── rules/             # one .aam file per idle rule
    ├── schema.aam     # @schema Rule definition (shared by all rules)
    ├── dim.aam
    └── lock.aam
```

### Enable as a systemd user service

```sh
systemctl --user enable --now nhidle-cargo.service
```

## Configuration

`nothypridle` uses the [AAM](https://aam.ininids.in.rs/) configuration format
(`key = value`).

### Global hooks — `config.aam`

All keys are optional. Missing keys simply skip the corresponding hook.

```aam
before_sleep_cmd = "systemctl --user lock && loginctl lock-session"
after_sleep_cmd  = "killall -SIGUSR1 waybar"
lock_cmd         = "hyprlock"
unlock_cmd       = "pkill hyprlock"
```

### Per-rule files — `rules/<id>.aam`

Each rule file must contain an `id` field and a `@derive schema.aam::Rule`
directive that links it to the shared schema (see below). The `timeout`
(in seconds) and every required field below are mandatory; optional fields
may be omitted.

| Field                   | Required | AAM type      | Description                                                          |
| ----------------------- | -------- | ------------- | -------------------------------------------------------------------- |
| `id`                    | yes      | `string`      | Rule identifier (must be unique across all rule files).              |
| `timeout`               | yes      | `i32`         | Idle timeout in seconds before `on_timeout` fires.                   |
| `max_cpu_usage`         | yes      | `f64`         | Inhibit when global CPU usage (percent) exceeds this.                |
| `max_gpu_usage`         | yes      | `f64`         | Inhibit when GPU busy percent exceeds this.                          |
| `min_ram_mb`            | yes      | `i32`         | Inhibit when free RAM drops below this (MB).                         |
| `min_vram_mb`           | yes      | `i32`         | Inhibit when free VRAM drops below this (MB).                        |
| `music_playing`         | yes      | `bool`        | Whether to apply the music-playback inhibitor at all.                |
| `fullscreen`            | yes      | `bool`        | Whether to apply the fullscreen-toplevel inhibitor at all.          |
| `on_timeout`            | no       | `string`      | Command to run when the idle timer fires.                            |
| `on_resume`             | no       | `string`      | Command to run when activity resumes after the rule fired.           |
| `music_process_name`    | no       | `string`      | MPRIS player identity to watch; if unset, any playing player inhibits. |
| `keep_alive_processes`  | no       | `list<string>` | Process-name substrings that, if running, keep the rule from firing. |

### Rule schema — `rules/schema.aam`

A shared `@schema Rule { … }` definition lives in `rules/schema.aam`. Every
rule file starts with `@derive schema.aam::Rule` to declare that it follows
this schema. At load time the daemon injects the schema definition into each
rule file before parsing, so AAM validates:

- **Required fields** — all non-optional fields (without `*`) must be present.
- **Type checking** — each field value is checked against its declared AAM type
  (`i32`, `f64`, `bool`, `string`, `list<string>`).

If a rule fails validation, the daemon reports the error and refuses to start,
preventing silently misconfigured rules from running at runtime.

```aam
@schema Rule {
    id: string
    timeout: i32
    max_cpu_usage: f64
    max_gpu_usage: f64
    min_ram_mb: i32
    min_vram_mb: i32
    music_playing: bool
    fullscreen: bool
    on_timeout*: string
    on_resume*: string
    music_process_name*: string
    keep_alive_processes*: list<string>
}
```

Fields marked with `*` are optional; all others are required. The daemon
falls back to plain loading (no schema validation) if `schema.aam` is absent,
preserving backward compatibility.

#### Example — dim the screen after 2 minutes, lock after 5

`rules/dim.aam`:

```aam
@derive schema.aam::Rule

id                = dim
timeout           = 120
max_cpu_usage     = 30.0
max_gpu_usage     = 30.0
min_ram_mb        = 1024
min_vram_mb       = 256
music_playing     = true
fullscreen        = true
on_timeout        = brightnessctl -s set 20%
on_resume         = brightnessctl -r
keep_alive_processes = [ffmpeg, HandBrake]
```

`rules/lock.aam`:

```aam
@derive schema.aam::Rule

id                = lock
timeout           = 300
max_cpu_usage     = 10.0
max_gpu_usage     = 10.0
min_ram_mb        = 1024
min_vram_mb       = 256
music_playing     = true
fullscreen        = true
on_timeout        = loginctl lock-session
on_resume         = pkill hyprlock
```

Unparseable or missing optional values at runtime are reported once per rule
via `tracing` and the corresponding check is skipped, so a single bad rule
never breaks the whole daemon.

## Architecture

The workspace is split into two crates:

- `crates/nhidle` — the `nhidle` daemon. Owns configuration loading
  (`config.rs`), the Wayland client (`wayland.rs`), the D-Bus listener
  (`dbus.rs`), idle/inhibitor orchestration (`monitor.rs`), and the entry point
  (`app.rs`, `main.rs`).
- `crates/nh-hal` — reusable hardware/system integrations: MPRIS audio
  detection (`audio.rs`) and a multi-vendor GPU/CPU/RAM resource monitor
  (`resources.rs`).

Idle handling is structured around three small abstractions in `monitor.rs`:

- `ResourceSource` — read-only access to system metrics (mocked in tests).
- `RuleConfigSource` — typed access to a rule's config values (mocked in tests).
- `RuleRuntime` — per-rule mutable state (epoch counter for resume detection,
  fired flag, deduplicated warning log).

When the Wayland client reports a rule as idle, `spawn_idle_check` starts a
polling loop that calls `should_inhibit`; only when no inhibitor matches and the
user has not resumed (epoch unchanged) does it run `on_timeout`.

## Development

```sh
cargo check --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

The crate ships both integration tests (under `crates/nhidle/tests/`) and inline
unit tests (`#[cfg(test)] mod tests` in each module). The `test-util` Cargo
feature exposes the `RecordingRunner` test scaffolding used by the integration
tests; it is intentionally not part of the public library API.

## Comparison with hypridle

| Aspect                       | hypridle                          | nothypridle                                            |
| ---------------------------- | --------------------------------- | ------------------------------------------------------ |
| Config format                | TOML                              | AAM (`.aam`)                                           |
| Idle protocol                | `ext-idle-notification-v1`        | `ext-idle-notification-v1`                             |
| Inhibitors                   | D-Bus `Inhibit` only              | CPU/GPU/RAM/VRAM, processes, MPRIS, fullscreen, D-Bus |
| Global sleep/lock hooks      | Yes                               | Yes (via `config.aam`)                                 |
| GPU monitoring               | No                                | AMD + NVIDIA + Intel                                   |
| Language                     | C++                               | Rust                                                   |

`nothypridle` is intended as a feature-compatible superset of hypridle with
additional hardware-aware inhibitors; existing hypridle users can migrate by
translating their `hypridle.conf` into `config.aam` plus one `rules/*.aam` file
per listener.

## License

BSD-3-Clause. See [`LICENSE`](./LICENSE).
