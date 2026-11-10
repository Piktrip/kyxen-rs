//! `kyxen` — settings app for kyxend: profiles, G-key macros, lighting.

mod app;
mod labels;

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

    if let Some(shot) = parse_shot() {
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

    window.run()
}
