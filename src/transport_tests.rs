use super::*;

#[test]
fn parses_remote_targets_and_rejects_malformed_inputs() {
    let remote = parse_remote("alice@example.com:/srv/code:copy").unwrap();
    assert_eq!(remote.user.as_deref(), Some("alice"));
    assert_eq!(remote.host, "example.com");
    assert_eq!(remote.path, "/srv/code:copy");
    assert_eq!(remote.ssh_target(), "alice@example.com");

    let host_only = parse_remote("example.com:~/code").unwrap();
    assert_eq!(host_only.user, None);
    assert_eq!(host_only.ssh_target(), "example.com");

    for invalid in ["missing-colon", ":/path", "host:", "@host:/path"] {
        assert!(parse_remote(invalid).is_err(), "accepted {invalid:?}");
    }
}

#[test]
fn shell_quotes_commands_and_home_paths_without_injection() {
    assert_eq!(shell_quote("safe/path-1"), "safe/path-1");
    assert_eq!(shell_quote(""), "''");
    assert_eq!(shell_quote("a b"), "'a b'");
    assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    assert_eq!(shell_quote_path("~"), "~");
    assert_eq!(shell_quote_path("~/a b"), "~/'a b'");
    assert_eq!(shell_quote_path("~alice/a b"), "~alice/'a b'");
    assert_eq!(shell_quote_path("~bad$user/path"), "'~bad$user/path'");
}

#[test]
fn builds_ssh_arguments_with_quoted_user_options() {
    let remote = parse_remote("alice@host:/remote path").unwrap();
    let command = build_ssh_command(
        &remote,
        Some("-p 2222 -i '/key with spaces'"),
        "/opt/synx current",
    )
    .unwrap();
    let args: Vec<String> = command
        .as_std()
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert!(args.windows(2).any(|pair| pair == ["-p", "2222"]));
    assert!(args
        .windows(2)
        .any(|pair| pair == ["-i", "/key with spaces"]));
    assert_eq!(args[args.len() - 2], "alice@host");
    assert_eq!(
        args.last().unwrap(),
        "'/opt/synx current' --agent '/remote path'"
    );
    assert!(build_ssh_command(&remote, Some("'unterminated"), "synx").is_err());
}
