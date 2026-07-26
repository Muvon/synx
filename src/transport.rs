use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::process::{Child, Command};

/// Parsed remote target: `[user@]host:/path`
#[derive(Debug, Clone)]
pub struct Remote {
    pub user: Option<String>,
    pub host: String,
    pub path: String,
}

impl Remote {
    pub fn ssh_target(&self) -> String {
        match &self.user {
            Some(u) => format!("{u}@{}", self.host),
            None => self.host.clone(),
        }
    }
}

pub fn parse_remote(s: &str) -> Result<Remote> {
    let (left, path) = s
        .split_once(':')
        .with_context(|| format!("remote must be [user@]host:/path, got {s:?}"))?;
    let (user, host) = match left.split_once('@') {
        Some((u, h)) => (Some(u.to_string()), h.to_string()),
        None => (None, left.to_string()),
    };
    if user.as_deref() == Some("") {
        anyhow::bail!("empty user in remote target");
    }
    if host.is_empty() {
        anyhow::bail!("empty host in remote target");
    }
    if path.is_empty() {
        anyhow::bail!("empty path in remote target");
    }
    Ok(Remote {
        user,
        host,
        path: path.to_string(),
    })
}

/// Shell-quote `s` for safe interpolation into a single ssh remote command line.
fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "/_.-+=:@%".contains(c))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Like `shell_quote`, but preserves a leading `~` (or `~user`) so the
/// remote shell can expand it to the home directory.
fn shell_quote_path(s: &str) -> String {
    if s == "~" {
        return "~".to_string();
    }
    if let Some(rest) = s.strip_prefix("~/") {
        return format!("~/{}", shell_quote(rest));
    }
    // ~username/path
    if let Some(stripped) = s.strip_prefix('~') {
        if let Some(idx) = stripped.find('/') {
            let (user, rest) = stripped.split_at(idx);
            // ~user portion: only allow safe chars unquoted
            if user
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                && !user.is_empty()
            {
                return format!("~{}/{}", user, shell_quote(&rest[1..]));
            }
        }
    }
    shell_quote(s)
}

/// Spawn `ssh <opts> <target> <remote_synx> --agent <remote_path>`.
/// stdin/stdout are piped (protocol channel); stderr is inherited so the user
/// sees SSH auth prompts and agent diagnostics.
pub fn spawn_ssh(remote: &Remote, ssh_opts: Option<&str>, remote_synx: &str) -> Result<Child> {
    let mut cmd = build_ssh_command(remote, ssh_opts, remote_synx)?;
    cmd.spawn()
        .context("failed to spawn ssh (is it installed?)")
}

fn build_ssh_command(
    remote: &Remote,
    ssh_opts: Option<&str>,
    remote_synx: &str,
) -> Result<Command> {
    let mut cmd = Command::new("ssh");
    cmd.arg("-T")
        .arg("-o")
        .arg("Compression=no")
        .arg("-o")
        .arg("ServerAliveInterval=30")
        .arg("-o")
        .arg("ServerAliveCountMax=3")
        .arg("-o")
        .arg("ControlMaster=auto")
        .arg("-o")
        .arg("ControlPersist=60")
        .arg("-o")
        .arg("ControlPath=~/.ssh/synx-%C");

    if let Some(opts) = ssh_opts {
        let args = shlex::split(opts).context("invalid quoting in --ssh-opts")?;
        for arg in args {
            cmd.arg(arg);
        }
    }

    cmd.arg(remote.ssh_target());

    let remote_cmd = format!(
        "{} --agent {}",
        shell_quote(remote_synx),
        shell_quote_path(&remote.path),
    );
    cmd.arg(remote_cmd);

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);

    Ok(cmd)
}

#[cfg(test)]
mod tests {
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
}
