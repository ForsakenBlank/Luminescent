//! Luminescent: small tools a hotkey away.
//!
//!     luminescent                       start (or, if it is running already, ask it to open)
//!     luminescent --smoke               start, check every part once, then quit (0 when all is well)
//!     luminescent --bench-startup       start, print how long it took to be ready, then quit
//!     luminescent --safe-mode           start without modules and with automations paused
//!     luminescent ctl <command>         talk to the running app (ping, status, overlay, action <id>, quit)
//!     luminescent --strip-metadata <paths>   clean copies into the Metadata save folder (Dolphin / Explorer menu)

mod ctl;
mod launcher;
mod smoke;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use lum_core::Paths;
use lum_platform::ipc::CtlEndpoint;

#[derive(Parser)]
#[command(name = "luminescent", version, about = "Small tools a hotkey away.")]
struct Cli {
    /// Start, check every part once, then quit with 0 when all is well (used to test a build)
    #[arg(long)]
    smoke: bool,

    /// Start, print the time from start to ready, then quit
    #[arg(long)]
    bench_startup: bool,

    /// Start without modules and with automations paused
    #[arg(long)]
    safe_mode: bool,

    /// Make cleaned copies of these files or folders in the Metadata save folder
    #[arg(long, num_args = 1.., value_name = "PATHS", allow_hyphen_values = true)]
    strip_metadata: Option<Vec<PathBuf>>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Send a command to the running Luminescent: ping, status, overlay, palette, settings, action <id>, quit
    Ctl {
        #[arg(required = true, num_args = 1.., allow_hyphen_values = true)]
        command: Vec<String>,
    },
}

fn main() -> ExitCode {
    let started = Instant::now();
    let cli = Cli::parse();

    if let Some(paths) = cli.strip_metadata {
        // the Dolphin / Explorer menu: no window and no lock needed
        eprintln!(
            "The Metadata Stripper is not in this build yet, so nothing was changed ({} items).",
            paths.len()
        );
        return ExitCode::from(1);
    }
    if let Some(Command::Ctl { command }) = cli.command {
        return send_ctl(&command.join(" "));
    }

    let options = launcher::Options { smoke: cli.smoke, bench: cli.bench_startup, safe_mode: cli.safe_mode };
    match launcher::run(options, started) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            tracing::error!(target: "unhandled", "Luminescent could not start: {e:#}");
            eprintln!(
                "Luminescent could not start: {e:#}\nStart it again; after two failed starts it starts in safe mode."
            );
            ExitCode::from(1)
        }
    }
}

fn send_ctl(line: &str) -> ExitCode {
    let endpoint = CtlEndpoint::for_home(&Paths::from_env());
    match endpoint.request(line, Duration::from_secs(10)) {
        Ok(answer) => {
            println!("{answer}");
            if answer.starts_with("error:") { ExitCode::from(1) } else { ExitCode::SUCCESS }
        }
        Err(e) => {
            eprintln!("Luminescent is not running ({}: {e}).", endpoint.describe());
            ExitCode::from(2)
        }
    }
}
