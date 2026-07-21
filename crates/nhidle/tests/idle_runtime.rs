#![allow(clippy::field_reassign_with_default)]
use nothypridle::helpers::RecordingRunner;
use nothypridle::monitor::{
    FullscreenState, IdleCheckOptions, IdleCheckOutcome, ResourceSource, RuleConfigSource,
    RuleRuntime, run_idle_check_loop,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct FakeResourceSource {
    inhibit: bool,
}

impl ResourceSource for FakeResourceSource {
    fn update(&mut self) {}
    fn get_cpu(&self) -> f32 {
        if self.inhibit { 100.0 } else { 0.0 }
    }
    fn get_gpu(&self) -> f32 {
        0.0
    }
    fn get_ram(&self) -> u64 {
        1000 * 1024 * 1024
    }
    fn get_vram(&self) -> u64 {
        1000
    }
    fn check_any_process_running(&mut self, _targets: &[&str]) -> bool {
        false
    }
    fn is_audio_playing(&self, _name: &str) -> bool {
        false
    }
    fn is_any_audio_playing(&self) -> bool {
        false
    }
}

#[derive(Default)]
struct FakeConfig {
    on_timeout_cmd: Option<String>,
}

impl RuleConfigSource for FakeConfig {
    fn max_cpu_usage(&self, _id: &str) -> Result<f32, String> {
        Ok(50.0)
    } // Inhibit if cpu > 50
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
        Err("".into())
    }
    fn music_playing(&self, _id: &str) -> Result<bool, String> {
        Err("".into())
    }
    fn music_process_name(&self, _id: &str) -> Result<Option<String>, String> {
        Err("".into())
    }
    fn fullscreen(&self, _id: &str) -> Result<bool, String> {
        Err("".into())
    }
    fn on_timeout(&self, _id: &str) -> Result<Option<String>, String> {
        Ok(self.on_timeout_cmd.clone())
    }
    fn on_resume(&self, _id: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn timeout(&self, _id: &str) -> Result<u32, String> {
        Ok(0)
    }
}

#[test]
fn test_run_idle_check_loop_fires() {
    let mut config = FakeConfig::default();
    config.on_timeout_cmd = Some("echo fired".to_string());

    let resources = FakeResourceSource::default(); // inhibit = false
    let monitor = Mutex::new(resources);
    let fullscreen = Mutex::new(FullscreenState::default());
    let rule = RuleRuntime::new("test".to_string());

    let recorded = Arc::new(Mutex::new(Vec::new()));
    let runner = RecordingRunner(recorded.clone());

    let options = IdleCheckOptions {
        poll_interval: Duration::ZERO,
    };

    let outcome = run_idle_check_loop(&config, &monitor, &fullscreen, &rule, &runner, options);

    assert!(matches!(outcome, IdleCheckOutcome::FiredOnTimeout));
    assert!(rule.fired());
    assert_eq!(recorded.lock().unwrap().len(), 1);
    assert_eq!(recorded.lock().unwrap()[0], "echo fired");
}

#[test]
fn test_run_idle_check_loop_no_command() {
    let config = FakeConfig::default();

    let resources = FakeResourceSource::default(); // inhibit = false
    let monitor = Mutex::new(resources);
    let fullscreen = Mutex::new(FullscreenState::default());
    let rule = RuleRuntime::new("test".to_string());

    let recorded = Arc::new(Mutex::new(Vec::new()));
    let runner = RecordingRunner(recorded.clone());

    let options = IdleCheckOptions {
        poll_interval: Duration::ZERO,
    };

    let outcome = run_idle_check_loop(&config, &monitor, &fullscreen, &rule, &runner, options);

    assert!(matches!(outcome, IdleCheckOutcome::NoCommandToFire));
    assert!(rule.fired());
    assert_eq!(recorded.lock().unwrap().len(), 0);
}

struct AbortingResourceSource {
    rule: Arc<RuleRuntime>,
}

impl ResourceSource for AbortingResourceSource {
    fn update(&mut self) {}
    fn get_cpu(&self) -> f32 {
        self.rule.bump_epoch();
        0.0 // no inhibit
    }
    fn get_gpu(&self) -> f32 {
        0.0
    }
    fn get_ram(&self) -> u64 {
        1000 * 1024 * 1024
    }
    fn get_vram(&self) -> u64 {
        1000
    }
    fn check_any_process_running(&mut self, _targets: &[&str]) -> bool {
        false
    }
    fn is_audio_playing(&self, _name: &str) -> bool {
        false
    }
    fn is_any_audio_playing(&self) -> bool {
        false
    }
}

#[test]
fn test_run_idle_check_loop_aborted() {
    let config = FakeConfig::default();

    let rule = Arc::new(RuleRuntime::new("test".to_string()));
    let resources = AbortingResourceSource { rule: rule.clone() };

    let monitor = Mutex::new(resources);
    let fullscreen = Mutex::new(FullscreenState::default());

    let recorded = Arc::new(Mutex::new(Vec::new()));
    let runner = RecordingRunner(recorded.clone());

    let options = IdleCheckOptions {
        poll_interval: Duration::ZERO,
    };

    let outcome = run_idle_check_loop(&config, &monitor, &fullscreen, &rule, &runner, options);

    assert!(matches!(outcome, IdleCheckOutcome::AbortedByUserResume));
    assert!(!rule.fired());
    assert_eq!(recorded.lock().unwrap().len(), 0);
}
