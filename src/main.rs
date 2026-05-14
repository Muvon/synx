mod agent;
mod cache;
mod cli;
mod ignores;
mod peer;
mod protocol;
mod sync;
mod transport;
mod ui;
mod walker;
mod watcher;

use anyhow::Result;
use clap::Parser;
use cli::Cli;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    ui::init(cli.verbose, cli.agent);

    if cli.agent {
        let path = cli
            .local
            .ok_or_else(|| anyhow::anyhow!("--agent requires a path argument"))?;
        return agent::run(std::path::PathBuf::from(path)).await;
    }

    let local = cli
        .local
        .clone()
        .ok_or_else(|| anyhow::anyhow!("missing <LOCAL> path (try: synx --help)"))?;
    let remote = cli
        .remote
        .clone()
        .ok_or_else(|| anyhow::anyhow!("missing <REMOTE> target (try: synx --help)"))?;

    sync::run(cli::ClientArgs {
        local,
        remote,
        mode: cli.mode,
        ssh_opts: cli.ssh_opts.clone(),
        no_compress: cli.no_compress,
        once: cli.once,
        dry_run: cli.dry_run,
        remote_synx: cli.remote_synx.clone(),
    })
    .await
}
