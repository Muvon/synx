use super::*;
use clap::{CommandFactory, Parser};

#[test]
fn cli_definition_is_internally_consistent() {
    Cli::command().debug_assert();
}

#[test]
fn parses_defaults_and_full_client_options() {
    let defaults = Cli::try_parse_from(["synx", "local", "host:/remote"]).unwrap();
    assert_eq!(defaults.local.as_deref(), Some("local"));
    assert_eq!(defaults.remote.as_deref(), Some("host:/remote"));
    assert_eq!(defaults.mode, SyncMode::Both);
    assert_eq!(defaults.remote_synx, "synx");
    assert_eq!(defaults.verbose, 0);
    assert!(!defaults.once);
    assert!(!defaults.dry_run);
    assert!(!defaults.allow_repo_mismatch);

    let full = Cli::try_parse_from([
        "synx",
        "local",
        "host:/remote",
        "--mode",
        "push",
        "-vv",
        "--ssh-opts",
        "-p 2222",
        "--no-compress",
        "--once",
        "--dry-run",
        "--allow-repo-mismatch",
        "--remote-synx",
        "/opt/synx",
    ])
    .unwrap();
    assert_eq!(full.mode, SyncMode::Push);
    assert_eq!(full.verbose, 2);
    assert_eq!(full.ssh_opts.as_deref(), Some("-p 2222"));
    assert!(full.no_compress && full.once && full.dry_run && full.allow_repo_mismatch);
    assert_eq!(full.remote_synx, "/opt/synx");
}

#[test]
fn parses_agent_and_rejects_invalid_mode() {
    let agent = Cli::try_parse_from(["synx", "--agent", "/root"]).unwrap();
    assert!(agent.agent);
    assert_eq!(agent.local.as_deref(), Some("/root"));
    assert!(agent.remote.is_none());
    assert!(Cli::try_parse_from(["synx", "local", "host:/remote", "--mode", "sideways"]).is_err());
}
