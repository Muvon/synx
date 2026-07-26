use clap::Parser;

use crate::protocol::SyncMode;

const LONG_ABOUT: &str = "\
Fast, real-time bidirectional file sync over SSH.

EXAMPLES:
  synx ./src dev@beefy:/srv/app/src
  synx ~/proj devbox:/work --mode push
  synx /var/log host:/backup --mode pull --once
  synx ~/notes box:~/notes -v
";

#[derive(Parser, Debug)]
#[command(
    name = "synx",
    version,
    about = "Fast real-time file sync over SSH",
    long_about = LONG_ABOUT,
)]
pub struct Cli {
    /// Local directory to sync (or, with --agent, the remote-side path).
    #[arg(value_name = "LOCAL")]
    pub local: Option<String>,

    /// Remote target as [user@]host:/path
    #[arg(value_name = "REMOTE")]
    pub remote: Option<String>,

    /// Sync direction
    #[arg(short, long, value_enum, default_value_t = SyncMode::Both)]
    pub mode: SyncMode,

    /// Increase logging verbosity (-v, -vv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Internal: run as the remote-side agent (invoked over SSH).
    #[arg(long, hide = true)]
    pub agent: bool,

    /// Extra arguments to pass to the ssh client, e.g. "-p 2222 -i ~/.ssh/key"
    #[arg(long, value_name = "OPTS", allow_hyphen_values = true)]
    pub ssh_opts: Option<String>,

    /// Disable on-the-wire zstd compression.
    #[arg(long)]
    pub no_compress: bool,

    /// Perform the initial sync and exit, without entering live-watch mode.
    #[arg(long)]
    pub once: bool,

    /// Print the planned operations and exit without applying them.
    #[arg(long)]
    pub dry_run: bool,

    /// Command used to invoke synx on the remote (must be in PATH).
    #[arg(long, default_value = "synx", value_name = "CMD")]
    pub remote_synx: String,
}

#[derive(Debug, Clone)]
pub struct ClientArgs {
    pub local: String,
    pub remote: String,
    pub mode: SyncMode,
    pub ssh_opts: Option<String>,
    pub no_compress: bool,
    pub once: bool,
    pub dry_run: bool,
    pub remote_synx: String,
}

#[cfg(test)]
mod tests {
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
            "--remote-synx",
            "/opt/synx",
        ])
        .unwrap();
        assert_eq!(full.mode, SyncMode::Push);
        assert_eq!(full.verbose, 2);
        assert_eq!(full.ssh_opts.as_deref(), Some("-p 2222"));
        assert!(full.no_compress && full.once && full.dry_run);
        assert_eq!(full.remote_synx, "/opt/synx");
    }

    #[test]
    fn parses_agent_and_rejects_invalid_mode() {
        let agent = Cli::try_parse_from(["synx", "--agent", "/root"]).unwrap();
        assert!(agent.agent);
        assert_eq!(agent.local.as_deref(), Some("/root"));
        assert!(agent.remote.is_none());
        assert!(
            Cli::try_parse_from(["synx", "local", "host:/remote", "--mode", "sideways"]).is_err()
        );
    }
}
