#![allow(clippy::field_reassign_with_default)]
use nothypridle::helpers::RecordingRunner;
use nothypridle::monitor::{RuleConfigSource, RuleRuntime};
use nothypridle::wayland::handle_rule_resumed;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct FakeConfig {
    on_resume_cmd: Option<String>,
}

impl RuleConfigSource for FakeConfig {
    fn max_cpu_usage(&self, _id: &str) -> Result<f32, String> {
        Ok(0.0)
    }
    fn max_gpu_usage(&self, _id: &str) -> Result<f32, String> {
        Ok(0.0)
    }
    fn min_ram_mb(&self, _id: &str) -> Result<u32, String> {
        Ok(0)
    }
    fn min_vram_mb(&self, _id: &str) -> Result<u32, String> {
        Ok(0)
    }
    fn keep_alive_processes(&self, _id: &str) -> Result<Vec<String>, String> {
        Ok(vec![])
    }
    fn music_playing(&self, _id: &str) -> Result<bool, String> {
        Ok(false)
    }
    fn music_process_name(&self, _id: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn fullscreen(&self, _id: &str) -> Result<bool, String> {
        Ok(false)
    }
    fn on_timeout(&self, _id: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn on_resume(&self, _id: &str) -> Result<Option<String>, String> {
        Ok(self.on_resume_cmd.clone())
    }
    fn timeout(&self, _id: &str) -> Result<u32, String> {
        Ok(0)
    }
}

#[test]
fn test_handle_rule_resumed_not_fired() {
    let mut config = FakeConfig::default();
    config.on_resume_cmd = Some("echo resume".to_string());

    let rule = RuleRuntime::new("test".to_string());
    rule.set_fired(false);

    let recorded = Arc::new(Mutex::new(Vec::new()));
    let runner = RecordingRunner(recorded.clone());

    handle_rule_resumed(&config, &rule, &runner);

    assert_eq!(recorded.lock().unwrap().len(), 0);
    assert!(!rule.fired());
}

#[test]
fn test_handle_rule_resumed_fired() {
    let mut config = FakeConfig::default();
    config.on_resume_cmd = Some("echo resume".to_string());

    let rule = RuleRuntime::new("test".to_string());
    rule.set_fired(true);

    let recorded = Arc::new(Mutex::new(Vec::new()));
    let runner = RecordingRunner(recorded.clone());

    handle_rule_resumed(&config, &rule, &runner);

    assert_eq!(recorded.lock().unwrap().len(), 1);
    assert_eq!(recorded.lock().unwrap()[0], "echo resume");
    assert!(!rule.fired());
}

#[test]
fn test_handle_rule_resumed_fired_no_cmd() {
    let config = FakeConfig::default();

    let rule = RuleRuntime::new("test".to_string());
    rule.set_fired(true);

    let recorded = Arc::new(Mutex::new(Vec::new()));
    let runner = RecordingRunner(recorded.clone());

    handle_rule_resumed(&config, &rule, &runner);

    assert_eq!(recorded.lock().unwrap().len(), 0);
    assert!(!rule.fired()); // resets even if no command
}
