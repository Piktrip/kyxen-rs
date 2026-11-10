//! `kyxend` — G-key macros, M-key profiles and per-key RGB for Logitech
//! G815 / G915 keyboards on Linux.

mod actions;
mod daemon;
mod device;
mod install;
mod ipc;
mod keyboard;
mod passthrough;

use std::thread;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use kyxen_core::config::Store;
use kyxen_hidpp::HidppDevice;
use kyxen_hidpp::hidraw::Hidraw;
use kyxen_hidpp::onboard::OnboardProfiles;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the daemon (default).
    Run,
    /// Show the detected keyboard and its HID++ features (read-only).
    Probe,
    /// Put the keyboard back into onboard mode on a profile slot.
    Restore {
        #[arg(default_value_t = 1)]
        slot: u8,
    },
    /// Ask the running daemon for its status.
    Status,
    /// Switch the running daemon to a profile.
    Switch { profile: String },
    /// Make the running daemon reload its config.
    Reload,
    /// Install for this user: binaries to ~/.local/bin, systemd user service,
    /// app launcher entry and tray autostart.
    Install {
        /// Don't start the tray icon at login.
        #[arg(long)]
        no_tray: bool,
    },
    /// Remove everything `install` created (config files are kept).
    Uninstall,
    /// Write a read-only hardware report of your Logitech devices (Markdown),
    /// to attach to an issue when asking for support of a keyboard.
    Report {
        /// Where to write it (default: current folder).
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
        /// Don't wait for Enter after the notice.
        #[arg(short, long)]
        yes: bool,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .init();

    match Cli::parse().command.unwrap_or(Cmd::Run) {
        Cmd::Run => run(),
        Cmd::Probe => probe(),
        Cmd::Restore { slot } => restore(slot),
        Cmd::Status => client(&ipc::Request::Status),
        Cmd::Switch { profile } => client(&ipc::Request::Switch { profile }),
        Cmd::Reload => client(&ipc::Request::Reload),
        Cmd::Install { no_tray } => install::install(!no_tray),
        Cmd::Uninstall => install::uninstall(),
        Cmd::Report { output, yes } => report(output, yes),
    }
}

fn run() -> Result<()> {
    let store = Store::default();
    let socket = ipc::socket_path();
    let listener = ipc::bind(&socket)?;
    let (events_tx, events_rx) = crossbeam_channel::unbounded();

    let mut signals = Signals::new([SIGINT, SIGTERM])?;
    let tx = events_tx.clone();
    thread::spawn(move || {
        if signals.forever().next().is_some() {
            let _ = tx.send(daemon::Event::Shutdown);
        }
    });

    let global = store.load_global().unwrap_or_default();
    let layout = daemon::Daemon::text_layout(&global);
    info!(layout = layout.name, "text macro layout");
    let (actions_tx, actions_handle) = actions::spawn(actions::Uinput::new()?, layout)?;
    ipc::spawn(listener, events_tx.clone())?;

    daemon::Daemon::new(store, actions_tx.clone(), events_tx).run(&events_rx);

    let _ = actions_tx.send(actions::Job::Shutdown);
    let _ = actions_handle.join();
    let _ = std::fs::remove_file(&socket);
    info!("stopped");
    Ok(())
}

fn open() -> Result<(device::Keyboard, HidppDevice<Hidraw>)> {
    let kb = device::detect().context("no supported keyboard found")?;
    let dev = HidppDevice::new(
        Hidraw::open(&kb.hidpp).with_context(|| format!("open {}", kb.hidpp.display()))?,
    );
    Ok((kb, dev))
}

fn probe() -> Result<()> {
    let (kb, mut dev) = open()?;
    println!("{} — HID++ on {}", kb.model.name, kb.hidpp.display());
    if let Some(p) = &kb.boot_event {
        println!("boot keyboard: {}", p.display());
    }
    println!("\nfeatures:");
    for (idx, id) in dev.feature_table()? {
        println!("  0x{idx:02x}  {:04x}  {}", id.0, id.name().unwrap_or(""));
    }
    if let Ok(op) = OnboardProfiles::open(&mut dev) {
        println!(
            "\nonboard profiles: mode {:?}, profile {}, {}-byte sectors",
            op.mode(&mut dev)?,
            op.current_profile(&mut dev)?,
            op.sector_size
        );
    }
    Ok(())
}

fn report(output: Option<std::path::PathBuf>, yes: bool) -> Result<()> {
    use std::io::{BufRead, IsTerminal, Write};

    let german = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.is_empty())
        .is_some_and(|v| v.starts_with("de"));
    let say = |de: &str, en: &str| if german { de.to_owned() } else { en.to_owned() };

    println!("{}\n", kyxen_report::notice(german));
    if !yes && std::io::stdin().is_terminal() {
        print!(
            "{}",
            say(
                "Weiter mit Enter, abbrechen mit Strg+C … ",
                "Press Enter to continue, Ctrl+C to cancel … "
            )
        );
        std::io::stdout().flush()?;
        std::io::stdin().lock().read_line(&mut String::new())?;
    }
    println!(
        "\n{}",
        say("Suche Logitech-Geräte …", "Looking for Logitech devices …")
    );

    let report = kyxen_report::collect();
    for line in report.summary() {
        println!("{line}");
    }
    if report.is_empty() {
        println!(
            "{}",
            say(
                "Keine Logitech-Geräte gefunden. Ist die Tastatur angesteckt?",
                "No Logitech devices found. Is the keyboard plugged in?"
            )
        );
    }
    for node in report.denied() {
        println!(
            "{}",
            say(
                &format!(
                    "Kein Zugriff auf {}: für alle Details mit sudo starten (sudo kyxend report).",
                    node.display()
                ),
                &format!(
                    "No access to {}: run with sudo for all details (sudo kyxend report).",
                    node.display()
                ),
            )
        );
    }

    let name = kyxen_report::file_name();
    let candidates = output.map_or_else(
        || vec![name.clone().into(), std::env::temp_dir().join(&name)],
        |p| vec![p],
    );
    let path = kyxen_report::write(&report.markdown(), &candidates)?;
    println!(
        "\n{}\n  {}\n\n{}\n  {}",
        say(
            "Fertig! Der Bericht liegt hier:",
            "Done! The report is here:"
        ),
        path.display(),
        say(
            "Schau ihn dir gern an und häng ihn an ein neues Issue (GitHub-Konto nötig):",
            "Have a look and attach it to a new issue (needs a GitHub account):"
        ),
        kyxen_report::ISSUE_URL
    );
    Ok(())
}

fn restore(slot: u8) -> Result<()> {
    let (_, mut dev) = open()?;
    let op = OnboardProfiles::open(&mut dev)?;
    op.restore(&mut dev, slot)?;
    println!("onboard mode, profile {}", op.current_profile(&mut dev)?);
    Ok(())
}

fn client(req: &ipc::Request) -> Result<()> {
    let resp = ipc::request(&ipc::socket_path(), req)?;
    println!("{}", serde_json::to_string_pretty(&resp)?);
    Ok(())
}
