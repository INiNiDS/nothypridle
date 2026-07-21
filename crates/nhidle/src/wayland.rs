use std::sync::{Arc, Mutex};

use crate::nh_hal::resources::ResourceMonitor;
use wayland_client::event_created_child;
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    globals::{GlobalListContents, registry_queue_init},
};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1::{Event as IdleNotificationEvent, ExtIdleNotificationV1},
    ext_idle_notifier_v1::{Event as IdleNotifierEvent, ExtIdleNotifierV1},
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::{
    Event as ToplevelHandleEvent, ZwlrForeignToplevelHandleV1,
};
#[allow(unused_imports)]
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE;
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::{
    Event as ToplevelManagerEvent, ZwlrForeignToplevelManagerV1,
};

use crate::config::ListenerLoader;
use crate::helpers::CommandRunner;
use crate::monitor::{
    FullscreenState, IdleCheckOptions, RuleConfigSource, RuleRuntime, spawn_idle_check,
};

pub fn handle_rule_resumed<C: RuleConfigSource + ?Sized>(
    config: &C,
    rule: &RuleRuntime,
    runner: &dyn CommandRunner,
) {
    if rule.fired() {
        rule.set_fired(false);
        match config.on_resume(rule.id()) {
            Ok(Some(cmd)) => runner.run(&cmd),
            Ok(None) => {
                tracing::debug!("Wayland: rule '{}' has no 'on_resume' command.", rule.id());
            }
            Err(err) => {
                tracing::warn!(
                    "Wayland: rule '{}' has unparseable 'on_resume': {}",
                    rule.id(),
                    err
                );
            }
        }
    }
}

/// Wayland-client dispatch state shared with all [`Dispatch`] implementations.
struct AppState {
    loader: Arc<ListenerLoader>,
    resource_monitor: Arc<Mutex<ResourceMonitor>>,
    fullscreen_states: Arc<Mutex<FullscreenState>>,
    command_runner: Arc<dyn CommandRunner>,
    // Kept alive so the server-side objects outlive `run_wayland_client`'s setup phase.
    _seat: Option<wl_seat::WlSeat>,
    _idle_notifier: Option<ExtIdleNotifierV1>,
    _toplevel_manager: Option<ZwlrForeignToplevelManagerV1>,
    // Keeps `ExtIdleNotificationV1` proxies alive for the lifetime of `state`.
    #[allow(dead_code)]
    notifications: Vec<ExtIdleNotificationV1>,
}

/// Run the raw Wayland idle + foreign-toplevel client until the connection is closed.
pub fn run_wayland_client(
    loader: ListenerLoader,
    command_runner: Arc<dyn CommandRunner>,
) -> anyhow::Result<()> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut event_queue) = registry_queue_init::<AppState>(&conn)?;
    let qh = event_queue.handle();

    let resource_monitor = Arc::new(Mutex::new(ResourceMonitor::new()));
    let fullscreen_states: Arc<Mutex<FullscreenState>> =
        Arc::new(Mutex::new(FullscreenState::default()));
    let loader = Arc::new(loader);

    let seat: wl_seat::WlSeat = globals
        .bind(&qh, 1..=9, ())
        .map_err(|e| anyhow::anyhow!("Failed to bind wl_seat: {e}"))?;
    let idle_notifier: ExtIdleNotifierV1 = globals
        .bind(&qh, 1..=2, ())
        .map_err(|e| anyhow::anyhow!("Failed to bind ext_idle_notifier_v1: {e}"))?;

    let toplevel_manager = match globals.bind::<ZwlrForeignToplevelManagerV1, AppState, ()>(
        &qh,
        1..=3,
        (),
    ) {
        Ok(proxy) => {
            tracing::info!(
                "Wayland: bound zwlr_foreign_toplevel_manager_v1 for fullscreen tracking."
            );
            Some(proxy)
        }
        Err(e) => {
            tracing::warn!(
                "Wayland: wlr-foreign-toplevel-management-v1 unsupported by compositor ({e}); fullscreen inhibit will be inert."
            );
            None
        }
    };

    let loader_ref = &loader;
    let mut notifications: Vec<ExtIdleNotificationV1> = Vec::new();
    for id in loader_ref.get_all_ids() {
        let timeout_secs = match loader_ref.timeout(&id) {
            Ok(t) => t,
            Err(err) => {
                tracing::warn!(
                    "Wayland: rule '{}' has unparseable/missing 'timeout'; skipping: {}",
                    id,
                    err
                );
                continue;
            }
        };
        // Compute milliseconds in u64 to avoid `u32 * u32` overflow on
        // pathologically large (>~49.7 day) timeouts, then clamp into the
        // protocol's u32 millisecond range.
        let timeout_ms = (timeout_secs as u64).saturating_mul(1000);
        if timeout_ms > u32::MAX as u64 {
            tracing::warn!(
                "Wayland: rule '{}' timeout {}s ({} ms) exceeds the u32 millisecond range; clamping to {} ms.",
                id,
                timeout_secs,
                timeout_ms,
                u32::MAX
            );
        }
        let timeout_ms = timeout_ms.min(u32::MAX as u64) as u32;
        let rule = Arc::new(RuleRuntime::new(id.clone()));
        let notification =
            idle_notifier.get_idle_notification(timeout_ms, &seat, &qh, rule.clone());
        tracing::info!(
            "Wayland: registered idle notification for rule '{}' (timeout {}s).",
            id,
            timeout_secs
        );
        notifications.push(notification);
    }

    let mut state = AppState {
        loader,
        resource_monitor,
        fullscreen_states,
        command_runner,
        _seat: Some(seat),
        _idle_notifier: Some(idle_notifier),
        _toplevel_manager: toplevel_manager,
        notifications,
    };

    tracing::info!("Wayland: client running; dispatching events.");
    loop {
        if let Err(err) = event_queue.blocking_dispatch(&mut state) {
            tracing::error!("Wayland: blocking_dispatch failed: {}", err);
            return Err(err.into());
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        // Global lifecycle is managed by `GlobalListContents`; nothing to do.
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_seat::WlSeat,
        _event: wl_seat::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        // `wl_seat` capabilities/name changes are irrelevant to idle detection.
    }
}

impl Dispatch<ExtIdleNotifierV1, ()> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtIdleNotifierV1,
        _event: IdleNotifierEvent,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        // `ext_idle_notifier_v1` exposes no events.
    }
}

impl Dispatch<ExtIdleNotificationV1, Arc<RuleRuntime>> for AppState {
    fn event(
        state: &mut Self,
        _proxy: &ExtIdleNotificationV1,
        event: IdleNotificationEvent,
        data: &Arc<RuleRuntime>,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        match event {
            IdleNotificationEvent::Idled => {
                tracing::info!("Wayland: rule '{}' entered idle.", data.id());
                spawn_idle_check(
                    state.loader.clone(),
                    state.resource_monitor.clone(),
                    state.fullscreen_states.clone(),
                    data.clone(),
                    state.command_runner.clone(),
                    IdleCheckOptions::default(),
                );
            }
            IdleNotificationEvent::Resumed => {
                tracing::info!("Wayland: rule '{}' resumed.", data.id());
                data.bump_epoch();
                handle_rule_resumed(state.loader.as_ref(), data, state.command_runner.as_ref());
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrForeignToplevelManagerV1,
        _event: ToplevelManagerEvent,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }

    event_created_child!(AppState, ZwlrForeignToplevelManagerV1, [
        EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for AppState {
    fn event(
        state: &mut Self,
        proxy: &ZwlrForeignToplevelHandleV1,
        event: ToplevelHandleEvent,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        match event {
            ToplevelHandleEvent::State { state: bytes } => {
                let states: Vec<u32> = bytes
                    .chunks_exact(4)
                    .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect();
                let is_fullscreen = states.contains(&3);
                if let Ok(mut map) = state.fullscreen_states.lock() {
                    map.set(proxy.id(), is_fullscreen);
                }
            }
            ToplevelHandleEvent::Closed => {
                if let Ok(mut map) = state.fullscreen_states.lock() {
                    map.remove(&proxy.id());
                }
            }
            _ => {}
        }
    }
}
