mod config;
mod logging;
mod monitor;
mod ntfy;
mod state;

use anyhow::Result;
use chrono::Utc;
use clap::{Parser, Subcommand};
use config::Config;
use state::State;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

/// Poll web pages on a schedule, diff them against the last snapshot, and
/// push a plain-text diff to ntfy.sh when they change.
#[derive(Parser)]
#[command(name = "webmon", version, about)]
struct Cli {
    /// Path to the TOML config file.
    #[arg(short, long, default_value = "config.toml", global = true)]
    config: PathBuf,

    /// Also mirror log output to stderr (logging to the configured log file
    /// always happens regardless of this flag).
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Stay running and check targets on a loop (self-scheduling).
    Daemon {
        /// How often (seconds) the daemon wakes up to check whether any
        /// target is due. This is just the daemon's internal tick — each
        /// target is still only actually fetched once its own
        /// `interval_secs` has elapsed.
        #[arg(long, default_value_t = 30)]
        tick_secs: u64,
    },
    /// Run exactly one pass and exit: check every target, fetch only the
    /// ones whose interval has elapsed, then exit. Intended to be invoked
    /// periodically by an external scheduler, e.g. a system cron job that
    /// runs this once a minute.
    Cron,
    /// Force-fetch every target right now, ignoring schedules. Useful for
    /// testing a config or a regex before trusting it to the schedule.
    RunNow,
}

fn main() {
    let cli = Cli::parse();

    // Config must load before we can set up file logging (it names the log
    // file), so failures here go to stderr directly.
    let cfg = match Config::load(&cli.config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error loading config {}: {:#}", cli.config.display(), e);
            std::process::exit(1);
        }
    };

    if let Err(e) = logging::init(&cfg.general.log_file, cli.verbose, cli.verbose) {
        eprintln!("error setting up logging: {:#}", e);
        std::process::exit(1);
    }

    log::info!(
        "webmon starting up: config={}, {} target(s)",
        cli.config.display(),
        cfg.targets.len()
    );

    let result = match cli.command {
        Commands::Daemon { tick_secs } => run_daemon(&cfg, tick_secs),
        Commands::Cron => run_pass(&cfg, false),
        Commands::RunNow => run_pass(&cfg, true),
    };

    if let Err(e) = result {
        log::error!("fatal error: {:#}", e);
        eprintln!("fatal error: {:#}", e);
        std::process::exit(1);
    }
}

/// Runs one pass over all targets: due (or forced) targets get fetched,
/// diffed, and notified; the state file is updated and saved at the end.
fn run_pass(cfg: &Config, force: bool) -> Result<()> {
    let mut state = State::load(&cfg.general.state_file)?;
    let client = monitor::build_client(cfg)?;
    let now = Utc::now().timestamp();

    let mut checked = 0u32;
    let mut changed = 0u32;

    for target in &cfg.targets {
        let interval = cfg.interval_for(target);
        let due = force || state.is_due(&target.name, interval, now);

        if !due {
            log::debug!(
                "target '{}' not due yet ({}s interval)",
                target.name,
                interval
            );
            continue;
        }

        checked += 1;
        match monitor::process_target(&client, cfg, target) {
            Ok(outcome) => {
                if outcome.changed {
                    changed += 1;
                }
                state.mark_run(&target.name, now);
            }
            Err(e) => {
                log::error!("target '{}' failed: {:#}", target.name, e);
                // Still mark it as run so a persistently broken target
                // doesn't get hammered every tick; it'll be retried after
                // its normal interval.
                state.mark_run(&target.name, now);
            }
        }
    }

    state.save(&cfg.general.state_file)?;
    log::info!(
        "pass complete: {} target(s) checked, {} changed",
        checked,
        changed
    );
    Ok(())
}

fn run_daemon(cfg: &Config, tick_secs: u64) -> Result<()> {
    log::info!("daemon mode: tick every {}s", tick_secs);
    loop {
        if let Err(e) = run_pass(cfg, false) {
            log::error!("error during daemon pass: {:#}", e);
        }
        thread::sleep(Duration::from_secs(tick_secs));
    }
}
