#![allow(clippy::field_reassign_with_default)]
use nothypridle::monitor::{
    FullscreenState, ResourceSource, RuleConfigSource, RuleRuntime, should_inhibit,
};
use std::sync::Mutex;

#[derive(Default)]
struct FakeResourceSource {
    cpu: f32,
    gpu: f32,
    ram: u64,
    vram: u64,
    process_running: bool,
    audio_playing_name: bool,
    any_audio_playing: bool,
}

impl ResourceSource for FakeResourceSource {
    fn update(&mut self) {}
    fn get_cpu(&self) -> f32 {
        self.cpu
    }
    fn get_gpu(&self) -> f32 {
        self.gpu
    }
    fn get_ram(&self) -> u64 {
        self.ram
    }
    fn get_vram(&self) -> u64 {
        self.vram
    }
    fn check_any_process_running(&mut self, _targets: &[&str]) -> bool {
        self.process_running
    }
    fn is_audio_playing(&self, _name: &str) -> bool {
        self.audio_playing_name
    }
    fn is_any_audio_playing(&self) -> bool {
        self.any_audio_playing
    }
}

#[derive(Default)]
struct FakeConfig {
    max_cpu: Option<f32>,
    max_gpu: Option<f32>,
    min_ram: Option<u32>,
    min_vram: Option<u32>,
    keep_alive: Vec<String>,
    music_playing: Option<bool>,
    music_name: Option<Option<String>>,
    fullscreen: Option<bool>,
}

impl RuleConfigSource for FakeConfig {
    fn max_cpu_usage(&self, _id: &str) -> Result<f32, String> {
        self.max_cpu.ok_or("".into())
    }
    fn max_gpu_usage(&self, _id: &str) -> Result<f32, String> {
        self.max_gpu.ok_or("".into())
    }
    fn min_ram_mb(&self, _id: &str) -> Result<u32, String> {
        self.min_ram.ok_or("".into())
    }
    fn min_vram_mb(&self, _id: &str) -> Result<u32, String> {
        self.min_vram.ok_or("".into())
    }
    fn keep_alive_processes(&self, _id: &str) -> Result<Vec<String>, String> {
        Ok(self.keep_alive.clone())
    }
    fn music_playing(&self, _id: &str) -> Result<bool, String> {
        self.music_playing.ok_or("".into())
    }
    fn music_process_name(&self, _id: &str) -> Result<Option<String>, String> {
        self.music_name.clone().ok_or("".into())
    }
    fn fullscreen(&self, _id: &str) -> Result<bool, String> {
        self.fullscreen.ok_or("".into())
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

#[test]
fn test_inhibit_cpu() {
    let mut config = FakeConfig::default();
    config.max_cpu = Some(50.0);

    let mut resources = FakeResourceSource::default();
    resources.cpu = 60.0;

    let monitor = Mutex::new(resources);
    let fullscreen = Mutex::new(FullscreenState::default());
    let rule = RuleRuntime::new("test".to_string());

    assert!(should_inhibit(&config, &monitor, &fullscreen, &rule));

    monitor.lock().unwrap().cpu = 40.0;
    assert!(!should_inhibit(&config, &monitor, &fullscreen, &rule));
}

#[test]
fn test_inhibit_gpu() {
    let mut config = FakeConfig::default();
    config.max_gpu = Some(50.0);

    let mut resources = FakeResourceSource::default();
    resources.gpu = 60.0;

    let monitor = Mutex::new(resources);
    let fullscreen = Mutex::new(FullscreenState::default());
    let rule = RuleRuntime::new("test".to_string());

    assert!(should_inhibit(&config, &monitor, &fullscreen, &rule));
}

#[test]
fn test_inhibit_ram() {
    let mut config = FakeConfig::default();
    config.min_ram = Some(1000);

    let mut resources = FakeResourceSource::default();
    resources.ram = 500 * 1024 * 1024;

    let monitor = Mutex::new(resources);
    let fullscreen = Mutex::new(FullscreenState::default());
    let rule = RuleRuntime::new("test".to_string());

    assert!(should_inhibit(&config, &monitor, &fullscreen, &rule));
}

#[test]
fn test_inhibit_vram() {
    let mut config = FakeConfig::default();
    config.min_vram = Some(1000);

    let mut resources = FakeResourceSource::default();
    resources.vram = 500;

    let monitor = Mutex::new(resources);
    let fullscreen = Mutex::new(FullscreenState::default());
    let rule = RuleRuntime::new("test".to_string());

    assert!(should_inhibit(&config, &monitor, &fullscreen, &rule));
}

#[test]
fn test_inhibit_process() {
    let mut config = FakeConfig::default();
    config.keep_alive = vec!["firefox".to_string()];

    let mut resources = FakeResourceSource::default();
    resources.process_running = true;

    let monitor = Mutex::new(resources);
    let fullscreen = Mutex::new(FullscreenState::default());
    let rule = RuleRuntime::new("test".to_string());

    assert!(should_inhibit(&config, &monitor, &fullscreen, &rule));
}

#[test]
fn test_inhibit_music_any() {
    let mut config = FakeConfig::default();
    config.music_playing = Some(true);
    config.music_name = Some(None);

    let mut resources = FakeResourceSource::default();
    resources.any_audio_playing = true;

    let monitor = Mutex::new(resources);
    let fullscreen = Mutex::new(FullscreenState::default());
    let rule = RuleRuntime::new("test".to_string());

    assert!(should_inhibit(&config, &monitor, &fullscreen, &rule));
}

#[test]
fn test_inhibit_music_named() {
    let mut config = FakeConfig::default();
    config.music_playing = Some(true);
    config.music_name = Some(Some("spotify".to_string()));

    let mut resources = FakeResourceSource::default();
    resources.audio_playing_name = true;

    let monitor = Mutex::new(resources);
    let fullscreen = Mutex::new(FullscreenState::default());
    let rule = RuleRuntime::new("test".to_string());

    assert!(should_inhibit(&config, &monitor, &fullscreen, &rule));
}

#[test]
fn test_inhibit_fullscreen() {
    let mut config = FakeConfig::default();
    config.fullscreen = Some(true);

    let monitor = Mutex::new(FakeResourceSource::default());

    let mut fs = FullscreenState::default();
    fs.test_any = true;
    let fullscreen = Mutex::new(fs);
    let rule = RuleRuntime::new("test".to_string());

    assert!(should_inhibit(&config, &monitor, &fullscreen, &rule));
}
