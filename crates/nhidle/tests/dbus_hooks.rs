use nothypridle::dbus::{DbusHookEvent, GlobalHooks, handle_hook_event};
use nothypridle::helpers::RecordingRunner;
use std::sync::{Arc, Mutex};

#[test]
fn test_dbus_hooks() {
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let runner = RecordingRunner(recorded.clone());

    let hooks = GlobalHooks {
        before_sleep: Some("before_sleep".to_string()),
        after_sleep: Some("after_sleep".to_string()),
        lock: Some("lock".to_string()),
        unlock: Some("unlock".to_string()),
    };

    handle_hook_event(&hooks, DbusHookEvent::BeforeSleep, &runner);
    assert_eq!(recorded.lock().unwrap().len(), 1);
    assert_eq!(recorded.lock().unwrap()[0], "before_sleep");

    handle_hook_event(&hooks, DbusHookEvent::AfterSleep, &runner);
    assert_eq!(recorded.lock().unwrap().len(), 2);
    assert_eq!(recorded.lock().unwrap()[1], "after_sleep");

    handle_hook_event(&hooks, DbusHookEvent::Lock, &runner);
    assert_eq!(recorded.lock().unwrap().len(), 3);
    assert_eq!(recorded.lock().unwrap()[2], "lock");

    handle_hook_event(&hooks, DbusHookEvent::Unlock, &runner);
    assert_eq!(recorded.lock().unwrap().len(), 4);
    assert_eq!(recorded.lock().unwrap()[3], "unlock");
}

#[test]
fn test_dbus_hooks_no_commands() {
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let runner = RecordingRunner(recorded.clone());

    let hooks = GlobalHooks {
        before_sleep: None,
        after_sleep: None,
        lock: None,
        unlock: None,
    };

    handle_hook_event(&hooks, DbusHookEvent::BeforeSleep, &runner);
    handle_hook_event(&hooks, DbusHookEvent::AfterSleep, &runner);
    handle_hook_event(&hooks, DbusHookEvent::Lock, &runner);
    handle_hook_event(&hooks, DbusHookEvent::Unlock, &runner);

    assert_eq!(recorded.lock().unwrap().len(), 0);
}
