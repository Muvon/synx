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
    #[arg(long, value_name = "OPTS")]
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
