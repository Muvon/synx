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
        // Naive whitespace split; the user is expected to quote properly.
        for arg in opts.split_whitespace() {
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

    cmd.spawn()
        .context("failed to spawn ssh (is it installed?)")
}
