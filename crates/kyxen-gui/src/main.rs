//! `kyxen` — settings app for kyxend: profiles, G-key macros, lighting.
//!
//! `kyxen` opens the window, `kyxen --tray` starts hidden in the tray.
//! Closing the window keeps the app in the tray; "Beenden" in the tray menu
//! quits. Starting it again shows the running window.

mod activation;
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

/// `--screenshot <file.png> [--tab N] [--gkey N] [--effect N] [--lang N] [--form N] [--layout N]`:
/// render the window after it settled, save it and exit (for visual checks).
/// `--lang` / `--form` / `--layout` pick the settings choices (0 = automatic).
struct Shot {
    path: String,
    tab: i32,
    gkey: i32,
    effect: Option<i32>,
    lang: Option<i32>,
    form: Option<i32>,
    layout: Option<i32>,
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
        lang: value("--lang").and_then(|v| v.parse().ok()),
        layout: value("--layout").and_then(|v| v.parse().ok()),
        form: value("--form").and_then(|v| v.parse().ok()),
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

/// Show the window and bring it to the front (tray click, "Kyxen öffnen",
/// a second `kyxen`).
///
/// On Wayland:
/// - Slint/winit never maps a window whose surface was created while it was
///   hidden (`kyxen --tray`): `show()` reports success, but nothing appears.
///   Hiding destroys the surface on Wayland, so show → hide → show gets a
///   fresh one that does map.
/// - A window is only raised (and the launcher's feedback only ends) when it
///   presents the activation token of the click or launch (see [`activation`]).
pub fn present(w: &AppWindow) {
    let token = activation::take();
    if w.window().is_visible() {
        w.window().set_minimized(false);
    } else {
        let _ = w.show();
        let _ = w.hide();
        let _ = w.show();
    }
    if let Some(token) = token {
        activation::raise(w.as_weak(), token);
    }
}

/// Slint creates every window's surface when the event loop starts, even one
/// that was never shown. On Wayland that is an empty toplevel, which Plasma
/// lists as an open "Kyxen" task that does nothing when clicked. Once it
/// exists, destroy it (hiding does that on Wayland; nothing is drawn between
/// the two calls). [`present`] creates a fresh one later.
fn drop_hidden_surface(window: slint::Weak<AppWindow>, tries: u32) {
    use slint::winit_030::WinitWindowAccessor;
    Timer::single_shot(Duration::from_millis(25), move || {
        let Some(w) = window.upgrade() else { return };
        if w.window().is_visible() {
            return; // opened in the meantime
        }
        if w.window().has_winit_window() {
            let _ = w.show();
            let _ = w.hide();
        } else if tries > 0 {
            drop_hidden_surface(window, tries - 1);
        }
    });
}

fn main() -> Result<(), slint::PlatformError> {
    // before anything else: removing it from the environment is only sound
    // while we are single-threaded
    let launch_token = activation::from_env();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let shot = parse_shot();
    let start_hidden = std::env::args().any(|a| a == "--tray");
    if shot.is_none() && instance::signal_running(launch_token.as_deref()) {
        // an instance is already running; it shows its window
        return Ok(());
    }

    let window = AppWindow::new()?;
    // Claim the instance name first: registering the tray icon can take a
    // while (D-Bus), and a second `kyxen` started meanwhile must find us.
    if shot.is_none() && !instance::listen(window.as_weak()) {
        // lost the race against another instance starting right now
        if !start_hidden {
            instance::signal_running(launch_token.as_deref());
        }
        return Ok(());
    }
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

    // Tray icon (setting "Tray-Icon"); without a tray host the window closes as usual.
    let tray_wanted = shot.is_none() && app.borrow().tray_wanted();
    let has_tray = tray_wanted && app.borrow_mut().spawn_tray();
    if start_hidden && !has_tray {
        // started for the tray at login, but the tray is switched off
        return Ok(());
    }

    if let Some(shot) = shot {
        app.borrow_mut().read_only = true;
        let g = window.global::<Ui>();
        g.set_tab(shot.tab);
        g.invoke_select_gkey(shot.gkey);
        if let Some(e) = shot.effect {
            g.invoke_set_effect(e);
        }
        if let Some(l) = shot.lang {
            g.invoke_set_language(l);
        }
        if let Some(f) = shot.form {
            g.invoke_set_form(f);
        }
        if let Some(l) = shot.layout {
            g.invoke_set_layout(l);
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

    // With a tray, closing only hides the window; the tray can be switched
    // on and off at runtime, so decide when the window is closed.
    {
        let weak = std::rc::Rc::downgrade(&app);
        window.window().on_close_requested(move || {
            if !weak.upgrade().is_some_and(|a| a.borrow().has_tray()) {
                let _ = slint::quit_event_loop();
            }
            slint::CloseRequestResponse::HideWindow
        });
    }
    if has_tray && start_hidden {
        drop_hidden_surface(window.as_weak(), 80);
    } else {
        window.show()?;
        if let Some(token) = launch_token {
            // ends the launcher's feedback and gives the window focus
            activation::raise(window.as_weak(), token);
        }
    }
    slint::run_event_loop_until_quit()
}
