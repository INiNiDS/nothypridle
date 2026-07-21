#[cfg(any(test, feature = "test-util"))]
use std::sync::{Arc, Mutex};

pub trait CommandRunner: Send + Sync + 'static {
    fn run(&self, cmd: &str);
}

pub struct ShellRunner;
impl CommandRunner for ShellRunner {
    fn run(&self, cmd: &str) {
        let cmd = strip_surrounding_quotes(cmd.trim()).to_string();
        std::thread::spawn(move || {
            match std::process::Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .status()
            {
                Ok(status) => {
                    if status.success() {
                        tracing::debug!("Helper: Command executed successfully: '{}'", cmd);
                    } else {
                        tracing::warn!(
                            "Helper: Command '{}' exited with error status: {}",
                            cmd,
                            status
                        );
                    }
                }
                Err(e) => {
                    tracing::error!("Helper: Failed to run command '{}': {:?}", cmd, e);
                }
            }
        });
    }
}

/// Removes a single layer of surrounding double or single quotes from `s`.
/// AAM stores quoted values verbatim (e.g. `"foo bar"` stays `"foo bar"` in
/// the value map), so commands written with quotes in `.aam` files need them
/// stripped before being handed to `sh -c`.
fn strip_surrounding_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

#[cfg(any(test, feature = "test-util"))]
pub struct RecordingRunner(pub Arc<Mutex<Vec<String>>>);
#[cfg(any(test, feature = "test-util"))]
impl CommandRunner for RecordingRunner {
    fn run(&self, cmd: &str) {
        self.0.lock().unwrap().push(cmd.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_runner_captures_commands_in_order() {
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let runner = RecordingRunner(recorded.clone());

        runner.run("echo first");
        runner.run("echo second");

        let guard = recorded.lock().unwrap();
        assert_eq!(guard.len(), 2);
        assert_eq!(guard[0], "echo first");
        assert_eq!(guard[1], "echo second");
    }

    #[test]
    fn shell_runner_run_does_not_panic_on_invalid_command() {
        // The runner spawns a thread and never blocks; an invalid command
        // surfaces as a non-zero exit status logged via `tracing`, not a panic.
        // We simply ensure the call returns without incident.
        ShellRunner.run("this_command_does_not_exist_xyz_123");
        // Give the spawned thread a moment to finish and report.
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    #[test]
    fn strip_surrounding_quotes_removes_double_quotes() {
        assert_eq!(strip_surrounding_quotes("\"hello world\""), "hello world");
    }

    #[test]
    fn strip_surrounding_quotes_removes_single_quotes() {
        assert_eq!(strip_surrounding_quotes("'hello world'"), "hello world");
    }

    #[test]
    fn strip_surrounding_quotes_leaves_unquoted_untouched() {
        assert_eq!(strip_surrounding_quotes("hyprlock"), "hyprlock");
        assert_eq!(
            strip_surrounding_quotes("systemctl suspend"),
            "systemctl suspend"
        );
    }

    #[test]
    fn strip_surrounding_quotes_keeps_inner_quotes() {
        assert_eq!(
            strip_surrounding_quotes("\"echo \\\"hi\\\"\""),
            "echo \\\"hi\\\""
        );
    }

    #[test]
    fn strip_surrounding_quotes_keeps_mismatched_quotes() {
        assert_eq!(strip_surrounding_quotes("\"hello'"), "\"hello'");
        assert_eq!(strip_surrounding_quotes("'hello\""), "'hello\"");
    }

    #[test]
    fn strip_surrounding_quotes_handles_short_strings() {
        assert_eq!(strip_surrounding_quotes(""), "");
        assert_eq!(strip_surrounding_quotes("a"), "a");
        assert_eq!(strip_surrounding_quotes("\"\""), "");
        assert_eq!(strip_surrounding_quotes("''"), "");
    }
}
