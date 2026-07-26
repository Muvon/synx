use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn synx() -> Command {
    Command::new(env!("CARGO_BIN_EXE_synx"))
}

#[test]
fn help_and_version_exit_successfully() {
    let help = synx().arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Fast, real-time bidirectional"));

    let version = synx().arg("--version").output().unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).contains("synx"));
}

#[test]
fn main_reports_missing_arguments_and_bad_agent_invocation() {
    let missing_local = synx().output().unwrap();
    assert!(!missing_local.status.success());
    assert!(String::from_utf8_lossy(&missing_local.stderr).contains("missing <LOCAL>"));

    let missing_remote = synx().arg(".").output().unwrap();
    assert!(!missing_remote.status.success());
    assert!(String::from_utf8_lossy(&missing_remote.stderr).contains("missing <REMOTE>"));

    let bad_agent = synx().arg("--agent").output().unwrap();
    assert!(!bad_agent.status.success());
    assert!(String::from_utf8_lossy(&bad_agent.stderr).contains("--agent requires a path"));
}

#[test]
fn client_rejects_an_invalid_remote_before_starting_ssh() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("synx-cli-process-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();

    let output = synx().arg(&root).arg("not-a-remote").output().unwrap();
    let _ = std::fs::remove_dir_all(&root);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("remote must be"));
}
