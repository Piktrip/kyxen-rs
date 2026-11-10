//! `kyxend` — G-key macros, M-key profiles and per-key RGB for Logitech
//! G815 / G915 keyboards on Linux.

mod actions;
mod daemon;
mod device;
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
