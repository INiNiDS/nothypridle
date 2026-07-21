use crate::config::ListenerLoader;
use crate::dbus::{self, GlobalHooks};
use crate::helpers::{CommandRunner, ShellRunner};
use crate::wayland;
use std::path::PathBuf;
use std::sync::Arc;

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("Initializing nothypridle...");

    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nothypridle");

    let rules_dir = config_dir.join("rules");
    let global_config_path = config_dir.join("config.aam");
    let mut hooks = GlobalHooks::default();

    if global_config_path.exists() {
        if let Ok(model) = aam_rs::aam::AAM::load(&global_config_path) {
            hooks.before_sleep = model.get("before_sleep_cmd").map(|v| v.to_string());
            hooks.after_sleep = model.get("after_sleep_cmd").map(|v| v.to_string());
            hooks.lock = model.get("lock_cmd").map(|v| v.to_string());
            hooks.unlock = model.get("unlock_cmd").map(|v| v.to_string());
            tracing::info!("Main: Loaded global config 'config.aam'.");
        } else {
            tracing::warn!(
                "Main: Failed to parse 'config.aam'. Global sleep/lock hooks are inactive."
            );
        }
    } else {
        tracing::warn!("Main: 'config.aam' not found. Global hooks will be skipped.");
    }

    let mut loader = ListenerLoader::new();
    if let Err(err) = loader.load_dir_with_schema(&rules_dir) {
        return Err(format!(
            "Failed to load rules directory '{}': {:?}",
            rules_dir.display(),
            err
        )
        .into());
    }

    let command_runner: Arc<dyn CommandRunner> = Arc::new(ShellRunner);

    let wayland_runner = Arc::clone(&command_runner);
    std::thread::spawn(move || {
        if let Err(err) = wayland::run_wayland_client(loader, wayland_runner) {
            tracing::error!("Wayland: Thread exited with error: {}", err);
        }
    });

    if let Err(err) = dbus::run_dbus_listener(hooks, command_runner).await {
        tracing::error!("D-Bus: Listener crashed: {}", err);
        return Err(format!("D-Bus listener crashed: {err}").into());
    }

    Ok(())
}
