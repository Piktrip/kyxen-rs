//! `kyxen` — settings app for kyxend: profiles, G-key macros, lighting.
//!
//! `kyxen` opens the window, `kyxen --tray` starts hidden in the tray.
//! Closing the window keeps the app in the tray; "Beenden" in the tray menu
//! quits. Starting it again shows the running window.

mod app;
mod instance;
mod labels;
mod tray;

mod ui {
    #![allow(missing_debug_implementations, clippy::all, clippy::pedantic)]
    slint::include_modules!();
}

use std::time::Duration;

use slint::{ComponentHandle, Timer, TimerMode};
use tracing_subscriber::EnvFilter;
pub use ui::*;

/// `--screenshot <file.png> [--tab N] [--gkey N] [--effect N]`: render the
/// window after it settled, save it and exit (for visual checks).
struct Shot {
    path: String,
    tab: i32,
    gkey: i32,
    effect: Option<i32>,
}

fn parse_shot() -> Option<Shot> {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    Some(Shot {
        path: value("--screenshot")?,
        tab: value("--tab").and_then(|v| v.parse().ok()).unwrap_or(0),
        gkey: value("--gkey").and_then(|v| v.parse().ok()).unwrap_or(1),
        effect: value("--effect").and_then(|v| v.parse().ok()),
    })
}

fn save_png(window: &AppWindow, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let shot = window.window().take_snapshot()?;
    let (w, h) = (shot.width(), shot.height());
    // PAM (P7) is trivial to write; convert with ImageMagick if a .png was asked for.
    let pam = format!("{path}.pam");
    let mut data =
        format!("P7\nWIDTH {w}\nHEIGHT {h}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n")
            .into_bytes();
    data.extend_from_slice(shot.as_bytes());
    std::fs::write(&pam, data)?;
    let status = std::process::Command::new("magick")
        .args([pam.as_str(), path])
        .status()?;
    if status.success() {
        std::fs::remove_file(pam)?;
    }
    Ok(())
}

fn main() -> Result<(), slint::PlatformError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let shot = parse_shot();
    let start_hidden = std::env::args().any(|a| a == "--tray");
    if shot.is_none() && instance::signal_running() {
        // an instance is already running; it shows its window
        return Ok(());
    }

    let window = AppWindow::new()?;
    let app = app::App::new(&window);
    app::connect(&window, &app);
    let _status = app::start_status_polling(&app);

    let animation = Timer::default();
    {
        let weak = std::rc::Rc::downgrade(&app);
        animation.start(TimerMode::Repeated, Duration::from_millis(33), move || {
            if let Some(app) = weak.upgrade() {
                app.borrow_mut().tick();
            }
        });
    }

    // Tray icon; without a tray host the window just closes as usual.
    let tray = if shot.is_none() {
        use ksni::blocking::TrayMethods;
        tray::KyxenTray::new(window.as_weak())
            .spawn()
            .inspect_err(|e| tracing::warn!("no tray icon: {e}"))
            .ok()
    } else {
        None
    };
    let has_tray = tray.is_some();
    app.borrow_mut().set_tray(tray);
    if shot.is_none() {
        instance::listen(window.as_weak());
    }

    if let Some(shot) = shot {
        app.borrow_mut().read_only = true;
        let g = window.global::<Ui>();
        g.set_tab(shot.tab);
        g.invoke_select_gkey(shot.gkey);
        if let Some(e) = shot.effect {
            g.invoke_set_effect(e);
        }
        let weak = window.as_weak();
        Timer::single_shot(Duration::from_millis(1500), move || {
            if let Some(w) = weak.upgrade() {
                if let Err(e) = save_png(&w, &shot.path) {
                    eprintln!("screenshot failed: {e}");
                }
                let _ = w.hide();
                let _ = slint::quit_event_loop();
            }
        });
    }

    if has_tray {
        window
            .window()
            .on_close_requested(|| slint::CloseRequestResponse::HideWindow);
    }
    if !(has_tray && start_hidden) {
        window.show()?;
    }
    if has_tray {
        slint::run_event_loop_until_quit()
    } else {
        slint::run_event_loop()
    }
}
