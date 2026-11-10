//! Freedesktop integration files: app launcher entry and tray autostart.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from)
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map_or_else(|| home().join(fallback), PathBuf::from)
}

/// `~/.local/share/applications/kyxen.desktop`
pub fn launcher_path() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("applications/kyxen.desktop")
}

/// `~/.local/share/icons/hicolor/256x256/apps/kyxen.png`
pub fn icon_path() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("icons/hicolor/256x256/apps/kyxen.png")
}

/// `~/.config/autostart/kyxen-tray.desktop`
pub fn autostart_path() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("autostart/kyxen-tray.desktop")
}

/// Desktop entry for the app; `tray` = the hidden login entry (`--tray`).
pub fn entry(gui: &Path, tray: bool) -> String {
    let (name, exec, extra) = if tray {
        (
            "Kyxen (Tray)",
            format!("{} --tray", gui.display()),
            "X-GNOME-Autostart-enabled=true\nNoDisplay=true\n",
        )
    } else {
        ("Kyxen", gui.display().to_string(), "")
    };
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={name}\n\
         GenericName=G-Tasten und Beleuchtung\n\
         Comment=Makros und RGB-Beleuchtung für Logitech-Tastaturen\n\
         Exec={exec}\n\
         Icon=kyxen\n\
         Terminal=false\n\
         Categories=Settings;HardwareSettings;\n\
         Keywords=logitech;keyboard;gkey;macro;rgb;tastatur;\n\
         StartupWMClass=kyxen\n\
         {extra}"
    )
}

/// Whether the tray starts at login.
pub fn tray_autostart() -> bool {
    autostart_path().exists()
}

/// Create or remove the tray login entry for the app at `gui`.
pub fn set_tray_autostart(gui: &Path, on: bool) -> io::Result<()> {
    let path = autostart_path();
    if on {
        fs::create_dir_all(path.parent().unwrap_or(&path))?;
        fs::write(&path, entry(gui, true))
    } else {
        match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries() {
        let app = entry(Path::new("/u/.local/bin/kyxen"), false);
        assert!(app.contains("Exec=/u/.local/bin/kyxen\n"));
        assert!(!app.contains("NoDisplay"));
        let tray = entry(Path::new("/u/.local/bin/kyxen"), true);
        assert!(tray.contains("Exec=/u/.local/bin/kyxen --tray\n"));
        assert!(tray.contains("NoDisplay=true"));
    }
}
