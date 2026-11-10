//! XDG activation tokens (Wayland).
//!
//! A launcher (Plasma's menu, the tray host) hands out a token with every
//! launch or click. A window has to present it, otherwise KWin neither
//! raises it nor ends the launch feedback, which then sits in the task
//! manager as an empty entry that does nothing when clicked.
//!
//! winit only presents a token when it creates a window, and Slint recreates
//! ours without it, so [`raise`] presents it on the live surface itself via
//! `xdg_activation_v1`, sharing winit's Wayland connection.

use std::sync::Mutex;
use std::time::Duration;

use slint::ComponentHandle;
use slint::winit_030::WinitWindowAccessor;
use slint::winit_030::winit::raw_window_handle::{
    HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};

use crate::AppWindow;

static PENDING: Mutex<Option<String>> = Mutex::new(None);

/// Park a token (from another thread) for the next [`crate::present`].
pub fn set(token: String) {
    if !token.is_empty() {
        *PENDING.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(token);
    }
}

/// The parked token, if any.
pub fn take() -> Option<String> {
    PENDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
}

/// The token we were launched with, removed from the environment.
pub fn from_env() -> Option<String> {
    let token = std::env::var("XDG_ACTIVATION_TOKEN").ok()?;
    // SAFETY: called first thing in `main`, before any other thread exists.
    #[allow(unsafe_code)]
    unsafe {
        std::env::remove_var("XDG_ACTIVATION_TOKEN");
        std::env::remove_var("DESKTOP_STARTUP_ID");
    }
    Some(token).filter(|t| !t.is_empty())
}

/// Present `token` for the window as soon as its surface exists (it may
/// still be in the making after `show()`). Gives up after about a second.
pub fn raise(window: slint::Weak<AppWindow>, token: String) {
    fn attempt(window: slint::Weak<AppWindow>, token: String, tries: u32) {
        let Some(w) = window.upgrade() else { return };
        let done = w
            .window()
            .with_winit_window(|ww| {
                let (Ok(d), Ok(h)) = (ww.display_handle(), ww.window_handle()) else {
                    return true; // not Wayland-capable: nothing to do
                };
                let (RawDisplayHandle::Wayland(d), RawWindowHandle::Wayland(h)) =
                    (d.as_raw(), h.as_raw())
                else {
                    return true; // X11 raises without a token
                };
                match wayland::activate(d.display.as_ptr(), h.surface.as_ptr(), &token) {
                    Ok(()) => tracing::debug!("presented activation token {token}"),
                    Err(e) => tracing::warn!("xdg activation failed: {e}"),
                }
                true
            })
            .unwrap_or(false);
        if !done && tries > 0 {
            slint::Timer::single_shot(Duration::from_millis(25), move || {
                attempt(window, token, tries - 1);
            });
        }
    }
    attempt(window, token, 40);
}

mod wayland {
    use std::ffi::c_void;

    use wayland_client::backend::{Backend, ObjectId};
    use wayland_client::globals::{GlobalListContents, registry_queue_init};
    use wayland_client::protocol::wl_registry::{self, WlRegistry};
    use wayland_client::protocol::wl_surface::WlSurface;
    use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
    use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1::{
        self, XdgActivationV1,
    };

    struct State;

    impl Dispatch<WlRegistry, GlobalListContents> for State {
        fn event(
            _: &mut Self,
            _: &WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<XdgActivationV1, ()> for State {
        fn event(
            _: &mut Self,
            _: &XdgActivationV1,
            _: xdg_activation_v1::Event,
            &(): &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    /// `xdg_activation_v1.activate(token, surface)` on winit's connection.
    #[allow(unsafe_code)]
    pub fn activate(
        display: *mut c_void,
        surface: *mut c_void,
        token: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // SAFETY: both pointers come from winit's live window, used on the UI
        // thread that owns it; a foreign backend does not take ownership of
        // the display and lives only for this call.
        let backend = unsafe { Backend::from_foreign_display(display.cast()) };
        let conn = Connection::from_backend(backend);
        let (globals, queue) = registry_queue_init::<State>(&conn)?;
        let activation: XdgActivationV1 = globals.bind(&queue.handle(), 1..=1, ())?;
        // SAFETY: `surface` is winit's wl_surface proxy, alive for this call.
        let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.cast()) }?;
        let surface = WlSurface::from_id(&conn, id)?;
        activation.activate(token.to_owned(), &surface);
        activation.destroy();
        conn.flush()?;
        Ok(())
    }
}
