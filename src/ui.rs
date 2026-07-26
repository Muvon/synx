use owo_colors::OwoColorize;
use std::path::Path;

use crate::protocol::SyncMode;
use crate::transport::Remote;

/// Initialize tracing. The agent must never write logs to stdout (that's the
/// protocol channel), so we always route to stderr.
pub fn init(verbose: u8, agent: bool) {
    let default = default_filter(verbose, agent);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .compact()
        .init();
}

fn default_filter(verbose: u8, agent: bool) -> &'static str {
    match verbose {
        0 if agent => "synx=warn",
        0 => "synx=info",
        1 => "synx=debug",
        _ => "synx=trace",
    }
}

pub fn banner(local: &Path, remote: &Remote, mode: SyncMode) {
    let arrow = match mode {
        SyncMode::Push => "──▶",
        SyncMode::Pull => "◀──",
        SyncMode::Both => "◀─▶",
    };
    let user = remote.user.as_deref().unwrap_or("");
    let at = if user.is_empty() { "" } else { "@" };
    eprintln!(
        "{}  {} {} {}{}{}:{}",
        "synx".bright_cyan().bold(),
        local.display().bright_white(),
        arrow.bright_yellow().bold(),
        user.dimmed(),
        at,
        remote.host.bright_white(),
        remote.path.bright_white(),
    );
}

pub fn ok(s: &str) {
    eprintln!("{} {}", "✓".bright_green(), s);
}
pub fn info(s: &str) {
    eprintln!("{} {}", "•".bright_blue(), s);
}
pub fn warn(s: &str) {
    eprintln!("{} {}", "!".bright_yellow(), s.bright_yellow());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_filters_and_renders_all_status_variants() {
        assert_eq!(default_filter(0, true), "synx=warn");
        assert_eq!(default_filter(0, false), "synx=info");
        assert_eq!(default_filter(1, false), "synx=debug");
        assert_eq!(default_filter(2, false), "synx=trace");

        let with_user = Remote {
            user: Some("dev".into()),
            host: "host".into(),
            path: "/remote".into(),
        };
        let without_user = Remote {
            user: None,
            host: "host".into(),
            path: "/remote".into(),
        };
        banner(Path::new("/local"), &with_user, SyncMode::Push);
        banner(Path::new("/local"), &without_user, SyncMode::Pull);
        banner(Path::new("/local"), &without_user, SyncMode::Both);
        ok("ready");
        info("working");
        warn("careful");
    }
}
