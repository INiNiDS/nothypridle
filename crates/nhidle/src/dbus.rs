use crate::helpers::CommandRunner;
use futures_util::StreamExt;
use std::sync::Arc;
use zbus::message::Type;
use zbus::{Connection, MatchRule, MessageStream};

#[derive(Default)]
pub struct GlobalHooks {
    pub before_sleep: Option<String>,
    pub after_sleep: Option<String>,
    pub lock: Option<String>,
    pub unlock: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum DbusHookEvent {
    BeforeSleep,
    AfterSleep,
    Lock,
    Unlock,
}

pub fn handle_hook_event(hooks: &GlobalHooks, event: DbusHookEvent, runner: &dyn CommandRunner) {
    match event {
        DbusHookEvent::BeforeSleep => {
            tracing::info!("D-Bus: Preparing for sleep. Spawning before_sleep_cmd.");
            if let Some(ref cmd) = hooks.before_sleep {
                runner.run(cmd);
            }
        }
        DbusHookEvent::AfterSleep => {
            tracing::info!("D-Bus: System woke up. Spawning after_sleep_cmd.");
            if let Some(ref cmd) = hooks.after_sleep {
                runner.run(cmd);
            }
        }
        DbusHookEvent::Lock => {
            tracing::info!("D-Bus: Session locked. Spawning lock_cmd.");
            if let Some(ref cmd) = hooks.lock {
                runner.run(cmd);
            }
        }
        DbusHookEvent::Unlock => {
            tracing::info!("D-Bus: Session unlocked. Spawning unlock_cmd.");
            if let Some(ref cmd) = hooks.unlock {
                runner.run(cmd);
            }
        }
    }
}

/// Classify a `systemd-logind` D-Bus signal into a hook event, if any.
fn classify_signal(interface: &str, member: &str, msg: &zbus::Message) -> Option<DbusHookEvent> {
    if interface == "org.freedesktop.login1.Manager" && member == "PrepareForSleep" {
        let active = msg.body().deserialize::<bool>().ok()?;
        return Some(if active {
            DbusHookEvent::BeforeSleep
        } else {
            DbusHookEvent::AfterSleep
        });
    }

    if interface == "org.freedesktop.login1.Session" {
        return match member {
            "Lock" => Some(DbusHookEvent::Lock),
            "Unlock" => Some(DbusHookEvent::Unlock),
            _ => None,
        };
    }

    None
}

pub async fn run_dbus_listener(
    hooks: GlobalHooks,
    runner: Arc<dyn CommandRunner>,
) -> Result<(), Box<dyn std::error::Error>> {
    let connection = Connection::system().await?;
    let dbus_proxy = zbus::fdo::DBusProxy::new(&connection).await?;

    let rule = MatchRule::builder()
        .sender("org.freedesktop.login1")?
        .msg_type(Type::Signal)
        .build();

    dbus_proxy.add_match_rule(rule).await?;

    let mut stream = MessageStream::from(connection);

    tracing::info!("D-Bus: Listening for systemd-logind signals...");

    while let Some(msg_result) = stream.next().await {
        let Ok(msg) = msg_result else {
            continue;
        };
        let header = msg.header();
        let member = header.member().map(|m| m.as_str()).unwrap_or("");
        let interface = header.interface().map(|i| i.as_str()).unwrap_or("");

        if let Some(event) = classify_signal(interface, member, &msg) {
            handle_hook_event(&hooks, event, runner.as_ref());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_signal(
        interface: &str,
        member: &str,
        body: &(impl zbus::export::serde::Serialize + zbus::zvariant::Type),
    ) -> zbus::Message {
        zbus::Message::signal("/test", interface, member)
            .unwrap()
            .build(body)
            .unwrap()
    }

    #[test]
    fn classify_signal_prepare_for_sleep_active_is_before_sleep() {
        let msg = build_signal("org.freedesktop.login1.Manager", "PrepareForSleep", &true);
        assert!(matches!(
            classify_signal("org.freedesktop.login1.Manager", "PrepareForSleep", &msg),
            Some(DbusHookEvent::BeforeSleep)
        ));
    }

    #[test]
    fn classify_signal_prepare_for_sleep_inactive_is_after_sleep() {
        let msg = build_signal("org.freedesktop.login1.Manager", "PrepareForSleep", &false);
        assert!(matches!(
            classify_signal("org.freedesktop.login1.Manager", "PrepareForSleep", &msg),
            Some(DbusHookEvent::AfterSleep)
        ));
    }

    #[test]
    fn classify_signal_lock_member_maps_to_lock_event() {
        let msg = build_signal("org.freedesktop.login1.Session", "Lock", &());
        assert!(matches!(
            classify_signal("org.freedesktop.login1.Session", "Lock", &msg),
            Some(DbusHookEvent::Lock)
        ));
    }

    #[test]
    fn classify_signal_unlock_member_maps_to_unlock_event() {
        let msg = build_signal("org.freedesktop.login1.Session", "Unlock", &());
        assert!(matches!(
            classify_signal("org.freedesktop.login1.Session", "Unlock", &msg),
            Some(DbusHookEvent::Unlock)
        ));
    }

    #[test]
    fn classify_signal_unknown_member_returns_none() {
        let msg = build_signal("org.freedesktop.login1.Session", "SomethingElse", &());
        assert_eq!(
            classify_signal("org.freedesktop.login1.Session", "SomethingElse", &msg),
            None
        );
    }

    #[test]
    fn classify_signal_unknown_interface_returns_none() {
        let msg = build_signal("org.freedesktop.Other.Manager", "PrepareForSleep", &true);
        assert_eq!(
            classify_signal("org.freedesktop.Other.Manager", "PrepareForSleep", &msg),
            None
        );
    }

    #[test]
    fn classify_signal_prepare_for_sleep_unparseable_body_returns_none() {
        // `PrepareForSleep` expects a `bool`; a `u32` cannot be deserialized
        // into one, so classification must yield `None` rather than panic.
        let msg = build_signal("org.freedesktop.login1.Manager", "PrepareForSleep", &7u32);
        assert_eq!(
            classify_signal("org.freedesktop.login1.Manager", "PrepareForSleep", &msg),
            None
        );
    }

    #[test]
    fn global_hooks_default_is_all_none() {
        let hooks = GlobalHooks::default();
        assert!(matches!(
            hooks,
            GlobalHooks {
                before_sleep: None,
                after_sleep: None,
                lock: None,
                unlock: None
            }
        ));
    }
}
