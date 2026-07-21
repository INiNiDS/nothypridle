use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::nh_hal::resources::ResourceMonitor;
use wayland_client::backend::ObjectId;

use crate::config::ListenerLoader;
use crate::helpers::CommandRunner;

pub trait ResourceSource: Send {
    fn update(&mut self);
    fn get_cpu(&self) -> f32;
    fn get_gpu(&self) -> f32;
    fn get_ram(&self) -> u64;
    fn get_vram(&self) -> u64;
    fn check_any_process_running(&mut self, targets: &[&str]) -> bool;
    fn is_audio_playing(&self, name: &str) -> bool;
    fn is_any_audio_playing(&self) -> bool;
}

impl ResourceSource for ResourceMonitor {
    fn update(&mut self) {
        ResourceMonitor::update(self)
    }
    fn get_cpu(&self) -> f32 {
        ResourceMonitor::get_cpu(self)
    }
    fn get_gpu(&self) -> f32 {
        ResourceMonitor::get_gpu(self)
    }
    fn get_ram(&self) -> u64 {
        ResourceMonitor::get_ram(self)
    }
    fn get_vram(&self) -> u64 {
        ResourceMonitor::get_vram(self)
    }
    fn check_any_process_running(&mut self, targets: &[&str]) -> bool {
        ResourceMonitor::check_any_process_running(self, targets)
    }
    fn is_audio_playing(&self, name: &str) -> bool {
        crate::nh_hal::audio::is_audio_playing(name)
    }
    fn is_any_audio_playing(&self) -> bool {
        crate::nh_hal::audio::is_any_audio_playing()
    }
}

pub trait RuleConfigSource: Send + Sync {
    fn max_cpu_usage(&self, id: &str) -> Result<f32, String>;
    fn max_gpu_usage(&self, id: &str) -> Result<f32, String>;
    fn min_ram_mb(&self, id: &str) -> Result<u32, String>;
    fn min_vram_mb(&self, id: &str) -> Result<u32, String>;
    fn keep_alive_processes(&self, id: &str) -> Result<Vec<String>, String>;
    fn music_playing(&self, id: &str) -> Result<bool, String>;
    fn music_process_name(&self, id: &str) -> Result<Option<String>, String>;
    fn fullscreen(&self, id: &str) -> Result<bool, String>;
    fn on_timeout(&self, id: &str) -> Result<Option<String>, String>;
    fn on_resume(&self, id: &str) -> Result<Option<String>, String>;
    fn timeout(&self, id: &str) -> Result<u32, String>;
}

impl RuleConfigSource for ListenerLoader {
    fn max_cpu_usage(&self, id: &str) -> Result<f32, String> {
        self.max_cpu_usage(id).map_err(|e| e.to_string())
    }
    fn max_gpu_usage(&self, id: &str) -> Result<f32, String> {
        self.max_gpu_usage(id).map_err(|e| e.to_string())
    }
    fn min_ram_mb(&self, id: &str) -> Result<u32, String> {
        self.min_ram_mb(id).map_err(|e| e.to_string())
    }
    fn min_vram_mb(&self, id: &str) -> Result<u32, String> {
        self.min_vram_mb(id).map_err(|e| e.to_string())
    }
    fn keep_alive_processes(&self, id: &str) -> Result<Vec<String>, String> {
        self.keep_alive_processes(id).map_err(|e| e.to_string())
    }
    fn music_playing(&self, id: &str) -> Result<bool, String> {
        self.music_playing(id).map_err(|e| e.to_string())
    }
    fn music_process_name(&self, id: &str) -> Result<Option<String>, String> {
        self.music_process_name(id).map_err(|e| e.to_string())
    }
    fn fullscreen(&self, id: &str) -> Result<bool, String> {
        self.fullscreen(id).map_err(|e| e.to_string())
    }
    fn on_timeout(&self, id: &str) -> Result<Option<String>, String> {
        self.on_timeout(id).map_err(|e| e.to_string())
    }
    fn on_resume(&self, id: &str) -> Result<Option<String>, String> {
        self.on_resume(id).map_err(|e| e.to_string())
    }
    fn timeout(&self, id: &str) -> Result<u32, String> {
        self.timeout(id).map_err(|e| e.to_string())
    }
}

#[derive(Default)]
pub struct FullscreenState {
    states: HashMap<ObjectId, bool>,
    #[cfg(any(test, feature = "test-util"))]
    pub test_any: bool,
}

impl FullscreenState {
    pub fn set(&mut self, id: ObjectId, is_fullscreen: bool) {
        self.states.insert(id, is_fullscreen);
    }

    pub fn remove(&mut self, id: &ObjectId) {
        self.states.remove(id);
    }

    pub fn any_fullscreen(&self) -> bool {
        #[cfg(any(test, feature = "test-util"))]
        if self.test_any {
            return true;
        }
        self.states.values().any(|is_fs| *is_fs)
    }
}

pub struct RuleRuntime {
    id: String,
    epoch: AtomicU64,
    fired: AtomicBool,
    warned: Mutex<HashSet<&'static str>>,
}

impl RuleRuntime {
    pub fn new(id: String) -> Self {
        Self {
            id,
            epoch: AtomicU64::new(0),
            fired: AtomicBool::new(false),
            warned: Mutex::new(HashSet::new()),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// Emits `msg` as a `tracing::warn!` at most once per rule for the given
    /// `key`, so persistently unparseable config values don't spam the log on
    /// every poll tick.
    fn warn_once(&self, key: &'static str, msg: impl std::fmt::Display) {
        match self.warned.lock() {
            Ok(mut guard) => {
                if guard.insert(key) {
                    tracing::warn!("{}", msg);
                }
            }
            Err(_) => {
                tracing::warn!("{}", msg);
            }
        }
    }

    pub fn bump_epoch(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
    }

    pub fn current_epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    pub fn fired(&self) -> bool {
        self.fired.load(Ordering::SeqCst)
    }

    pub fn set_fired(&self, value: bool) {
        self.fired.store(value, Ordering::SeqCst);
    }
}

/// Returns `Some(true)` if the rule should be inhibited by this check,
/// `Some(false)` if the check definitively does *not* inhibit, and `None` if
/// the configured value is unparseable (already reported via `warn_once`).
fn inhibit_if_exceeds_max(
    rule: &RuleRuntime,
    key: &'static str,
    field: &str,
    getter: impl FnOnce() -> Result<f32, String>,
    current: f32,
) -> Option<bool> {
    match getter() {
        Ok(max) if current > max => {
            tracing::debug!(
                "Monitor: rule '{}' inhibited by {} usage ({} > {})",
                rule.id(),
                field,
                current,
                max
            );
            Some(true)
        }
        Ok(_) => Some(false),
        Err(_) => {
            rule.warn_once(
                key,
                format_args!(
                    "Monitor: rule '{}' has invalid/missing '{}'; skipping {} check.",
                    rule.id(),
                    field,
                    field
                ),
            );
            None
        }
    }
}

fn inhibit_if_below_min(
    rule: &RuleRuntime,
    key: &'static str,
    field: &str,
    getter: impl FnOnce() -> Result<u32, String>,
    current_mb: u64,
) -> Option<bool> {
    match getter() {
        Ok(min) if current_mb < u64::from(min) => {
            tracing::debug!(
                "Monitor: rule '{}' inhibited by free {} ({} MB < {} MB)",
                rule.id(),
                field,
                current_mb,
                min
            );
            Some(true)
        }
        Ok(_) => Some(false),
        Err(_) => {
            rule.warn_once(
                key,
                format_args!(
                    "Monitor: rule '{}' has invalid/missing '{}'; skipping {} check.",
                    rule.id(),
                    field,
                    field
                ),
            );
            None
        }
    }
}

fn inhibit_by_keep_alive<S: ResourceSource + ?Sized, C: RuleConfigSource + ?Sized>(
    loader: &C,
    monitor: &mut S,
    rule: &RuleRuntime,
) -> bool {
    let id = rule.id();
    match loader.keep_alive_processes(id) {
        Ok(targets) if !targets.is_empty() => {
            let targets_str: Vec<&str> = targets.iter().map(|s| s.as_str()).collect();
            if monitor.check_any_process_running(&targets_str) {
                tracing::debug!("Monitor: rule '{}' inhibited by a keep-alive process.", id);
                return true;
            }
        }
        Ok(_) => {}
        Err(err) => {
            rule.warn_once(
                "keep_alive_processes",
                format_args!(
                    "Monitor: rule '{}' has unparseable 'keep_alive_processes'; skipping process check: {}",
                    id, err
                ),
            );
        }
    }
    false
}

fn inhibit_by_music<S: ResourceSource + ?Sized, C: RuleConfigSource + ?Sized>(
    loader: &C,
    monitor: &S,
    rule: &RuleRuntime,
) -> bool {
    let id = rule.id();
    if !matches!(loader.music_playing(id), Ok(true)) {
        return false;
    }

    match loader.music_process_name(id) {
        Ok(Some(name)) => {
            if monitor.is_audio_playing(&name) {
                tracing::debug!(
                    "Monitor: rule '{}' inhibited by music playback ({}).",
                    id,
                    name
                );
                return true;
            }
        }
        Ok(None) => {
            if monitor.is_any_audio_playing() {
                tracing::debug!("Monitor: rule '{}' inhibited by any music playback.", id);
                return true;
            }
        }
        Err(err) => {
            rule.warn_once(
                "music_process_name",
                format_args!(
                    "Monitor: rule '{}' has unparseable 'music_process_name'; skipping music check: {}",
                    id, err
                ),
            );
        }
    }
    false
}

fn inhibit_by_fullscreen<C: RuleConfigSource + ?Sized>(
    loader: &C,
    fullscreen: &Mutex<FullscreenState>,
    rule: &RuleRuntime,
) -> bool {
    if !matches!(loader.fullscreen(rule.id()), Ok(true)) {
        return false;
    }
    if let Ok(states) = fullscreen.lock()
        && states.any_fullscreen()
    {
        tracing::debug!(
            "Monitor: rule '{}' inhibited by a fullscreen toplevel.",
            rule.id()
        );
        return true;
    }
    false
}

pub fn should_inhibit<S: ResourceSource + ?Sized, C: RuleConfigSource + ?Sized>(
    loader: &C,
    resource_monitor: &Mutex<S>,
    fullscreen: &Mutex<FullscreenState>,
    rule: &RuleRuntime,
) -> bool {
    let id = rule.id();
    let mut monitor = match resource_monitor.lock() {
        Ok(guard) => guard,
        Err(err) => {
            tracing::warn!(
                "Monitor: ResourceMonitor lock poisoned for rule '{}': {}",
                id,
                err
            );
            return false;
        }
    };
    monitor.update();

    if let Some(true) = inhibit_if_exceeds_max(
        rule,
        "max_cpu_usage",
        "max_cpu_usage",
        || loader.max_cpu_usage(id),
        monitor.get_cpu(),
    ) {
        return true;
    }

    if let Some(true) = inhibit_if_exceeds_max(
        rule,
        "max_gpu_usage",
        "max_gpu_usage",
        || loader.max_gpu_usage(id),
        monitor.get_gpu(),
    ) {
        return true;
    }

    let free_ram_mb = monitor.get_ram() / (1024 * 1024);
    if let Some(true) = inhibit_if_below_min(
        rule,
        "min_ram_mb",
        "min_ram_mb",
        || loader.min_ram_mb(id),
        free_ram_mb,
    ) {
        return true;
    }

    let free_vram_mb = monitor.get_vram();
    if free_vram_mb != u64::MAX
        && let Some(true) = inhibit_if_below_min(
            rule,
            "min_vram_mb",
            "min_vram_mb",
            || loader.min_vram_mb(id),
            free_vram_mb,
        )
    {
        return true;
    }

    if inhibit_by_keep_alive(loader, &mut *monitor, rule) {
        return true;
    }

    if inhibit_by_music(loader, &*monitor, rule) {
        return true;
    }

    if inhibit_by_fullscreen(loader, fullscreen, rule) {
        return true;
    }

    false
}

#[derive(Clone, Copy)]
pub struct IdleCheckOptions {
    pub poll_interval: Duration,
}

impl Default for IdleCheckOptions {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(5),
        }
    }
}

pub enum IdleCheckOutcome {
    AbortedByUserResume,
    FiredOnTimeout,
    NoCommandToFire,
    ParseError,
}

pub fn run_idle_check_loop<S: ResourceSource + ?Sized, C: RuleConfigSource + ?Sized>(
    loader: &C,
    resource_monitor: &Mutex<S>,
    fullscreen: &Mutex<FullscreenState>,
    rule: &RuleRuntime,
    runner: &dyn CommandRunner,
    options: IdleCheckOptions,
) -> IdleCheckOutcome {
    let start_epoch = rule.current_epoch();
    loop {
        if !should_inhibit(loader, resource_monitor, fullscreen, rule) {
            break;
        }

        if options.poll_interval > Duration::ZERO {
            thread::sleep(options.poll_interval);
        }

        if rule.current_epoch() != start_epoch {
            tracing::info!(
                "Monitor: rule '{}' poll loop aborted; user resumed (epoch advanced).",
                rule.id()
            );
            return IdleCheckOutcome::AbortedByUserResume;
        }
    }

    if rule.current_epoch() != start_epoch {
        tracing::info!(
            "Monitor: rule '{}' fired-on-resume avoided; user resumed before timeout execution.",
            rule.id()
        );
        return IdleCheckOutcome::AbortedByUserResume;
    }

    let id = rule.id();
    let res = match loader.on_timeout(id) {
        Ok(Some(cmd)) => {
            runner.run(&cmd);
            IdleCheckOutcome::FiredOnTimeout
        }
        Ok(None) => {
            tracing::debug!("Monitor: rule '{}' has no 'on_timeout' command.", id);
            IdleCheckOutcome::NoCommandToFire
        }
        Err(err) => {
            tracing::warn!(
                "Monitor: rule '{}' has unparseable 'on_timeout': {}",
                id,
                err
            );
            IdleCheckOutcome::ParseError
        }
    };

    rule.set_fired(true);
    if matches!(res, IdleCheckOutcome::FiredOnTimeout) {
        tracing::info!("Monitor: rule '{}' fired 'on_timeout'.", id);
    }
    res
}

pub fn spawn_idle_check<S: ResourceSource + 'static, C: RuleConfigSource + 'static>(
    loader: Arc<C>,
    resource_monitor: Arc<Mutex<S>>,
    fullscreen: Arc<Mutex<FullscreenState>>,
    rule: Arc<RuleRuntime>,
    runner: Arc<dyn CommandRunner>,
    options: IdleCheckOptions,
) {
    let id = rule.id().to_string();
    tracing::info!(
        "Monitor: rule '{}' entered idle; starting inhibitor polling.",
        id
    );

    thread::spawn(move || {
        run_idle_check_loop(
            loader.as_ref(),
            resource_monitor.as_ref(),
            fullscreen.as_ref(),
            rule.as_ref(),
            runner.as_ref(),
            options,
        );
    });
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use wayland_client::backend::ObjectId;

    #[test]
    fn rule_runtime_defaults_are_sane() {
        let rule = RuleRuntime::new("rule-1".to_string());
        assert_eq!(rule.id(), "rule-1");
        assert_eq!(rule.current_epoch(), 0);
        assert!(!rule.fired());
    }

    #[test]
    fn bump_epoch_advances_monotonically() {
        let rule = RuleRuntime::new("r".to_string());
        assert_eq!(rule.current_epoch(), 0);
        rule.bump_epoch();
        assert_eq!(rule.current_epoch(), 1);
        rule.bump_epoch();
        rule.bump_epoch();
        assert_eq!(rule.current_epoch(), 3);
    }

    #[test]
    fn fired_flag_round_trips() {
        let rule = RuleRuntime::new("r".to_string());
        rule.set_fired(true);
        assert!(rule.fired());
        rule.set_fired(false);
        assert!(!rule.fired());
    }

    #[test]
    fn warn_once_does_not_panic_on_contended_mutex() {
        let rule = RuleRuntime::new("r".to_string());
        // Two distinct keys should both be recorded without panicking; the
        // exact log output is not asserted (no subscriber is installed).
        rule.warn_once("k1", "first");
        rule.warn_once("k2", "second");
        // Repeating the same key should not panic either.
        rule.warn_once("k1", "first-repeat");
    }

    #[test]
    fn fullscreen_state_tracks_entries() {
        let mut state = FullscreenState::default();
        assert!(!state.any_fullscreen());

        let id_a = ObjectId::null();
        let id_b = ObjectId::null();

        state.set(id_a.clone(), false);
        assert!(!state.any_fullscreen());

        state.set(id_b.clone(), true);
        assert!(state.any_fullscreen());

        state.set(id_b.clone(), false);
        assert!(!state.any_fullscreen());

        state.set(id_a.clone(), true);
        assert!(state.any_fullscreen());

        state.remove(&id_a);
        assert!(!state.any_fullscreen());
    }

    #[test]
    fn fullscreen_state_test_any_short_circuits() {
        let mut state = FullscreenState::default();
        state.test_any = true;
        assert!(state.any_fullscreen());

        // Even after removing every tracked toplevel, `test_any` wins.
        let id = ObjectId::null();
        state.set(id.clone(), false);
        assert!(state.any_fullscreen());
    }

    #[test]
    fn idle_check_options_default_is_five_seconds() {
        let opts = IdleCheckOptions::default();
        assert_eq!(opts.poll_interval, Duration::from_secs(5));
    }

    #[test]
    fn inhibit_if_exceeds_max_inhibits_when_over() {
        let rule = RuleRuntime::new("r".into());
        let r = inhibit_if_exceeds_max(&rule, "k", "f", || Ok(50.0), 60.0);
        assert_eq!(r, Some(true));
    }

    #[test]
    fn inhibit_if_exceeds_max_passes_when_under() {
        let rule = RuleRuntime::new("r".into());
        let r = inhibit_if_exceeds_max(&rule, "k", "f", || Ok(50.0), 10.0);
        assert_eq!(r, Some(false));
    }

    #[test]
    fn inhibit_if_exceeds_max_reports_none_on_error() {
        let rule = RuleRuntime::new("r".into());
        let r = inhibit_if_exceeds_max(&rule, "k", "f", || Err("bad".into()), 10.0);
        assert_eq!(r, None);
    }

    #[test]
    fn inhibit_if_below_min_inhibits_when_under() {
        let rule = RuleRuntime::new("r".into());
        let r = inhibit_if_below_min(&rule, "k", "f", || Ok(1000u32), 500u64);
        assert_eq!(r, Some(true));
    }

    #[test]
    fn inhibit_if_below_min_passes_when_over() {
        let rule = RuleRuntime::new("r".into());
        let r = inhibit_if_below_min(&rule, "k", "f", || Ok(100u32), 5_000u64);
        assert_eq!(r, Some(false));
    }

    #[test]
    fn inhibit_if_below_min_reports_none_on_error() {
        let rule = RuleRuntime::new("r".into());
        let r = inhibit_if_below_min(&rule, "k", "f", || Err("bad".into()), 5_000u64);
        assert_eq!(r, None);
    }

    #[test]
    fn inhibit_by_fullscreen_respects_config_flag() {
        // Config returns `Ok(false)`: even if a toplevel is fullscreen, the
        // rule must not inhibit.
        #[derive(Default)]
        struct Falsy;
        impl RuleConfigSource for Falsy {
            fn max_cpu_usage(&self, _id: &str) -> Result<f32, String> {
                Err("".into())
            }
            fn max_gpu_usage(&self, _id: &str) -> Result<f32, String> {
                Err("".into())
            }
            fn min_ram_mb(&self, _id: &str) -> Result<u32, String> {
                Err("".into())
            }
            fn min_vram_mb(&self, _id: &str) -> Result<u32, String> {
                Err("".into())
            }
            fn keep_alive_processes(&self, _id: &str) -> Result<Vec<String>, String> {
                Ok(vec![])
            }
            fn music_playing(&self, _id: &str) -> Result<bool, String> {
                Err("".into())
            }
            fn music_process_name(&self, _id: &str) -> Result<Option<String>, String> {
                Err("".into())
            }
            fn fullscreen(&self, _id: &str) -> Result<bool, String> {
                Ok(false)
            }
            fn on_timeout(&self, _id: &str) -> Result<Option<String>, String> {
                Ok(None)
            }
            fn on_resume(&self, _id: &str) -> Result<Option<String>, String> {
                Ok(None)
            }
            fn timeout(&self, _id: &str) -> Result<u32, String> {
                Ok(0)
            }
        }

        let mut fs = FullscreenState::default();
        fs.test_any = true;
        let fullscreen = Mutex::new(fs);
        let rule = RuleRuntime::new("r".into());
        assert!(!inhibit_by_fullscreen(&Falsy, &fullscreen, &rule));
    }

    #[test]
    fn inhibit_by_fullscreen_inhibits_when_flag_and_state_agree() {
        #[derive(Default)]
        struct Truthy;
        impl RuleConfigSource for Truthy {
            fn max_cpu_usage(&self, _id: &str) -> Result<f32, String> {
                Err("".into())
            }
            fn max_gpu_usage(&self, _id: &str) -> Result<f32, String> {
                Err("".into())
            }
            fn min_ram_mb(&self, _id: &str) -> Result<u32, String> {
                Err("".into())
            }
            fn min_vram_mb(&self, _id: &str) -> Result<u32, String> {
                Err("".into())
            }
            fn keep_alive_processes(&self, _id: &str) -> Result<Vec<String>, String> {
                Ok(vec![])
            }
            fn music_playing(&self, _id: &str) -> Result<bool, String> {
                Err("".into())
            }
            fn music_process_name(&self, _id: &str) -> Result<Option<String>, String> {
                Err("".into())
            }
            fn fullscreen(&self, _id: &str) -> Result<bool, String> {
                Ok(true)
            }
            fn on_timeout(&self, _id: &str) -> Result<Option<String>, String> {
                Ok(None)
            }
            fn on_resume(&self, _id: &str) -> Result<Option<String>, String> {
                Ok(None)
            }
            fn timeout(&self, _id: &str) -> Result<u32, String> {
                Ok(0)
            }
        }

        let mut fs = FullscreenState::default();
        fs.test_any = true;
        let fullscreen = Mutex::new(fs);
        let rule = RuleRuntime::new("r".into());
        assert!(inhibit_by_fullscreen(&Truthy, &fullscreen, &rule));
    }
}
