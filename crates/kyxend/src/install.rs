//! `kyxend install` / `uninstall`: per-user installation without root.
//!
//! ```text
//! ~/.local/bin/kyxend, ~/.local/bin/kyxen          binaries (copied from the build)
//! ~/.config/systemd/user/kyxend.service            daemon, started with the desktop session
//! ~/.local/share/applications/kyxen.desktop        app launcher entry
//! ~/.local/share/icons/hicolor/256x256/apps/kyxen.png
//! ~/.config/autostart/kyxen-tray.desktop           tray icon at login (optional)
//! ```

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

const UNIT: &str = "kyxend.service";
const ICON: &[u8] = include_bytes!("../../kyxen-gui/assets/icon-256.png");

fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("$HOME is not set")
}

fn xdg(var: &str, fallback: &str) -> Result<PathBuf> {
    match std::env::var_os(var).filter(|v| !v.is_empty()) {
        Some(v) => Ok(PathBuf::from(v)),
        None => Ok(home()?.join(fallback)),
    }
}

struct Paths {
    bin_dir: PathBuf,
    unit: PathBuf,
    desktop: PathBuf,
    icon: PathBuf,
    autostart: PathBuf,
}

fn paths() -> Result<Paths> {
    let config = xdg("XDG_CONFIG_HOME", ".config")?;
    let data = xdg("XDG_DATA_HOME", ".local/share")?;
    Ok(Paths {
        bin_dir: home()?.join(".local/bin"),
        unit: config.join("systemd/user").join(UNIT),
        desktop: data.join("applications/kyxen.desktop"),
        icon: data.join("icons/hicolor/256x256/apps/kyxen.png"),
        autostart: config.join("autostart/kyxen-tray.desktop"),
    })
}

/// Write via temp file + rename (safe even if the target is a running binary).
fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    fs::create_dir_all(path.parent().context("no parent dir")?)?;
    let tmp = path.with_extension("kyxen-tmp");
    fs::write(&tmp, data).with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

fn copy_binary(src: &Path, dst: &Path) -> Result<()> {
    if src == dst {
        return Ok(());
    }
    let data = fs::read(src).with_context(|| format!("read {}", src.display()))?;
    write_atomic(dst, &data)?;
    fs::set_permissions(dst, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

pub fn unit_file(daemon: &Path) -> String {
    format!(
        "[Unit]\n\
         Description=Kyxen daemon — G-key macros and lighting for Logitech keyboards\n\
         Documentation=https://github.com/Piktrip/kyxen-rs\n\
         PartOf=graphical-session.target\n\
         After=graphical-session.target\n\
         \n\
         [Service]\n\
         ExecStart={}\n\
         Restart=on-failure\n\
         RestartSec=2\n\
         \n\
         [Install]\n\
         WantedBy=graphical-session.target\n",
        daemon.display()
    )
}

pub fn desktop_file(gui: &Path, tray: bool) -> String {
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

fn systemctl(args: &[&str]) -> Result<()> {
    let status = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()
        .context("run systemctl")?;
    if !status.success() {
        bail!("systemctl --user {} failed", args.join(" "));
    }
    Ok(())
}

pub fn install(tray_autostart: bool) -> Result<()> {
    let p = paths()?;
    let exe = std::env::current_exe().context("locate kyxend")?;
    let daemon = p.bin_dir.join("kyxend");
    copy_binary(&exe, &daemon)?;
    println!("✔ {}", daemon.display());

    let gui_src = exe.with_file_name("kyxen");
    let gui = p.bin_dir.join("kyxen");
    let has_gui = gui_src.exists();
    if has_gui {
        copy_binary(&gui_src, &gui)?;
        println!("✔ {}", gui.display());
    } else {
        println!(
            "• GUI not found next to kyxend ({}), skipping app entry",
            gui_src.display()
        );
    }

    write_atomic(&p.unit, unit_file(&daemon).as_bytes())?;
    println!("✔ {}", p.unit.display());

    if has_gui {
        write_atomic(&p.icon, ICON)?;
        write_atomic(&p.desktop, desktop_file(&gui, false).as_bytes())?;
        println!("✔ {}", p.desktop.display());
        if tray_autostart {
            write_atomic(&p.autostart, desktop_file(&gui, true).as_bytes())?;
            println!("✔ {} (tray at login)", p.autostart.display());
        }
        let _ = Command::new("update-desktop-database")
            .arg(p.desktop.parent().unwrap_or(&p.desktop))
            .status();
    }

    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", "--now", UNIT])?;
    systemctl(&["restart", UNIT])?;
    println!("✔ kyxend is running and starts with your desktop session");
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let p = paths()?;
    let _ = systemctl(&["disable", "--now", UNIT]);
    for f in [
        &p.unit,
        &p.desktop,
        &p.icon,
        &p.autostart,
        &p.bin_dir.join("kyxen"),
        &p.bin_dir.join("kyxend"),
    ] {
        match fs::remove_file(f) {
            Ok(()) => println!("✘ {}", f.display()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => eprintln!("could not remove {}: {e}", f.display()),
        }
    }
    let _ = systemctl(&["daemon-reload"]);
    println!("Config files in ~/.config/kyxen were kept.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_and_desktop_entries() {
        let unit = unit_file(Path::new("/home/u/.local/bin/kyxend"));
        assert!(unit.contains("ExecStart=/home/u/.local/bin/kyxend\n"));
        assert!(unit.contains("WantedBy=graphical-session.target"));
        let app = desktop_file(Path::new("/home/u/.local/bin/kyxen"), false);
        assert!(app.contains("Exec=/home/u/.local/bin/kyxen\n"));
        assert!(!app.contains("NoDisplay"));
        let tray = desktop_file(Path::new("/home/u/.local/bin/kyxen"), true);
        assert!(tray.contains("Exec=/home/u/.local/bin/kyxen --tray\n"));
    }
}
