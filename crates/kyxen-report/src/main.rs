//! `kyxen-report` — read-only hardware report for Logitech keyboards.
//!
//! For people who want to help add support for their keyboard without
//! writing code: run it, look at the Markdown file it writes, attach it to
//! an issue. It only asks the keyboard questions (HID++ getters) and changes
//! nothing on it.

mod probe;
mod system;

use std::fmt::Write as _;
use std::io::{self, BufRead, IsTerminal, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use kyxen_hidpp::descriptor;
use kyxen_hidpp::hidraw::Hidraw;

use probe::HidppInfo;
use system::{Node, System};

const ISSUE_URL: &str =
    "https://github.com/Piktrip/kyxen-rs/issues/new?template=keyboard-report.yml";

struct Text {
    intro: &'static str,
    confirm: &'static str,
    scanning: &'static str,
    no_access: &'static str,
    none_found: &'static str,
    written: &'static str,
    next: &'static str,
}

const DE: Text = Text {
    intro: "Kyxen-Tastaturbericht\n\n\
        Dieses Programm liest Informationen über deine Logitech-Geräte aus und schreibt\n\
        sie in eine Textdatei. Es stellt der Tastatur nur Fragen und ändert nichts an ihr:\n\
        keine Einstellungen, kein Speicher, keine Beleuchtung. Der Bericht enthält keine\n\
        Seriennummern, Benutzer- oder Rechnernamen; du kannst ihn vor dem Teilen lesen.\n\n\
        Benutzung auf eigene Gefahr. Kyxen ist ein inoffizielles Hobby-Projekt ohne\n\
        Verbindung zu Logitech und kommt ohne jede Gewährleistung (GPL-3.0).",
    confirm: "Weiter mit Enter, abbrechen mit Strg+C … ",
    scanning: "Suche Logitech-Geräte …",
    no_access: "Kein Zugriff auf {node}. Starte das Programm mit sudo, dann kann es auch\ndie Tastatur selbst abfragen:  sudo ./kyxen-report",
    none_found: "Keine Logitech-Geräte gefunden. Ist die Tastatur angesteckt?",
    written: "Fertig! Der Bericht liegt hier:",
    next: "Schau ihn dir gern an und häng ihn an ein neues Issue (GitHub-Konto nötig):",
};

const EN: Text = Text {
    intro: "Kyxen keyboard report\n\n\
        This program reads information about your Logitech devices and writes it to a\n\
        text file. It only asks the keyboard questions and changes nothing on it: no\n\
        settings, no memory, no lighting. The report contains no serial numbers, user or\n\
        host names; you can read it before sharing.\n\n\
        Use at your own risk. Kyxen is an unofficial hobby project, not affiliated with\n\
        Logitech, and comes without any warranty (GPL-3.0).",
    confirm: "Press Enter to continue, Ctrl+C to cancel … ",
    scanning: "Looking for Logitech devices …",
    no_access: "No access to {node}. Run the program with sudo so it can query the\nkeyboard itself:  sudo ./kyxen-report",
    none_found: "No Logitech devices found. Is the keyboard plugged in?",
    written: "Done! The report is here:",
    next: "Have a look and attach it to a new issue (needs a GitHub account):",
};

fn text() -> &'static Text {
    let lang = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.is_empty())
        .unwrap_or_default();
    if lang.starts_with("de") { &DE } else { &EN }
}

/// A probed node: sysfs facts plus what HID++ told us.
struct Probed {
    node: Node,
    hidpp_interface: bool,
    boot_keyboard: bool,
    access: Access,
    devices: Vec<HidppInfo>,
}

#[derive(PartialEq)]
enum Access {
    NotTried,
    Denied,
    Error(String),
    Ok,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!(
            "kyxen-report [--yes] [--output FILE]\n\n\
             Writes a read-only hardware report of your Logitech devices as Markdown.\n\
             --yes      don't wait for Enter after the notice\n\
             --output   where to write the report (default: current folder)"
        );
        return Ok(());
    }
    let yes = args.iter().any(|a| a == "-y" || a == "--yes");
    let output = args
        .iter()
        .position(|a| a == "-o" || a == "--output")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);

    let t = text();
    println!("{}\n", t.intro);
    if !yes && io::stdin().is_terminal() {
        print!("{}", t.confirm);
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
    }

    println!("\n{}", t.scanning);
    let sys = system::system();
    let mut probed = Vec::new();
    for node in system::nodes(Path::new("/sys/class/hidraw")) {
        let hidpp_interface = descriptor::is_hidpp(&node.descriptor);
        let mut p = Probed {
            boot_keyboard: descriptor::is_boot_keyboard(&node.descriptor),
            hidpp_interface,
            access: Access::NotTried,
            devices: Vec::new(),
            node,
        };
        if hidpp_interface {
            match Hidraw::open(&p.node.hidraw) {
                Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                    println!(
                        "{}",
                        t.no_access
                            .replace("{node}", &p.node.hidraw.display().to_string())
                    );
                    p.access = Access::Denied;
                }
                Err(e) => p.access = Access::Error(e.to_string()),
                Ok(first) => {
                    let path = p.node.hidraw.clone();
                    let mut first = Some(first);
                    p.devices = probe::probe_node(|| match first.take() {
                        Some(h) => Ok(h),
                        None => Hidraw::open(&path),
                    });
                    p.access = Access::Ok;
                }
            }
        }
        print_summary(&p);
        probed.push(p);
    }
    if probed.is_empty() {
        println!("{}", t.none_found);
    }

    let report = render(&sys, &probed);
    let path = write_report(output, &report)?;
    println!(
        "\n{}\n  {}\n\n{}\n  {ISSUE_URL}",
        t.written,
        path.display(),
        t.next
    );
    Ok(())
}

fn print_summary(p: &Probed) {
    let mut line = format!(
        "  {}  046d:{:04x}  {}",
        p.node.hidraw.display(),
        p.node.product,
        p.node.name
    );
    for d in &p.devices {
        if let Some(name) = &d.name {
            let _ = write!(line, "  →  {name} ({} features)", d.features.len());
        }
    }
    println!("{line}");
}

// ── report ───────────────────────────────────────────────────────────────

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn feature_name(id: u16) -> &'static str {
    kyxen_hidpp::FeatureId(id).name().unwrap_or(match id {
        0x0002 => "FEATURE_INFO",
        0x0007 => "DEVICE_FRIENDLY_NAME",
        0x0021 => "UNIQUE_RANDOM_ID",
        0x00c2 => "DFU_CONTROL_SIGNED",
        0x00d0 => "DFU",
        0x1000 => "BATTERY_STATUS",
        0x1004 => "UNIFIED_BATTERY",
        0x1300 => "LED_CONTROL",
        0x18a1 => "LED_SW_CONTROL",
        0x1b00 => "REPROG_CONTROLS",
        0x40a3 => "FN_INVERSION_3",
        0x4100 => "ENCRYPTION",
        0x4521 => "DISABLE_KEYS",
        0x8070 => "COLOR_LED_EFFECTS",
        0x8080 => "PER_KEY_LIGHTING",
        0x8110 => "MOUSE_BUTTON_SPY",
        _ => "",
    })
}

fn feature_flags(flags: u8) -> String {
    let names = [
        (0x80, "obsolete"),
        (0x40, "hidden"),
        (0x20, "engineering"),
        (0x10, "mfg-deactivatable"),
        (0x08, "compliance-deactivatable"),
    ];
    names
        .iter()
        .filter(|(bit, _)| flags & bit != 0)
        .map(|(_, n)| *n)
        .collect::<Vec<_>>()
        .join(", ")
}

fn device_type(t: u8) -> &'static str {
    [
        "keyboard",
        "remote control",
        "numpad",
        "mouse",
        "trackpad",
        "trackball",
        "presenter",
        "receiver",
        "headset",
        "webcam",
        "steering wheel",
        "joystick",
        "gamepad",
        "dock",
        "speaker",
        "microphone",
        "light",
        "programmable controller",
        "pedals",
        "adapter",
    ]
    .get(usize::from(t))
    .copied()
    .unwrap_or("unknown")
}

fn firmware_kind(t: u8) -> &'static str {
    [
        "application",
        "bootloader",
        "hardware",
        "touchpad",
        "optical sensor",
        "softdevice",
        "RF companion",
        "factory app",
        "RGB effect",
        "motor drive",
    ]
    .get(usize::from(t))
    .copied()
    .unwrap_or("other")
}

fn render(sys: &System, probed: &[Probed]) -> String {
    let mut s = String::new();
    let w = &mut s;
    let _ = writeln!(w, "# Kyxen keyboard report\n");
    let _ = writeln!(
        w,
        "Generated by kyxen-report {} on {} (UTC). Read-only: the program only sent the \
         HID++ getters listed at the end and changed nothing on the devices. No serial \
         numbers, unit ids, user or host names are included.\n",
        env!("CARGO_PKG_VERSION"),
        today()
    );
    let _ = writeln!(w, "## System\n");
    let _ = writeln!(w, "| | |\n|---|---|");
    let _ = writeln!(w, "| OS | {} |", sys.os);
    let _ = writeln!(w, "| Kernel | {} |", sys.kernel);
    let _ = writeln!(w, "| Desktop | {} ({}) |", sys.desktop, sys.session);
    let _ = writeln!(w, "| Keyboard layout | {} |", sys.layout);
    let _ = writeln!(
        w,
        "| Kyxen udev rule | {} |",
        if sys.udev_rule { "installed" } else { "no" }
    );
    let _ = writeln!(
        w,
        "| kyxend | {} |\n",
        sys.daemon.as_deref().unwrap_or("not running")
    );

    let _ = writeln!(w, "## Logitech HID interfaces\n");
    if probed.is_empty() {
        let _ = writeln!(w, "None found.\n");
    }
    let _ = writeln!(
        w,
        "| Node | USB id | Name | Bus | Interface | HID++ | Boot keyboard |"
    );
    let _ = writeln!(w, "|---|---|---|---|---|---|---|");
    for p in probed {
        let _ = writeln!(
            w,
            "| {} | 046d:{:04x} | {} | {} | {} | {} | {} |",
            p.node.hidraw.display(),
            p.node.product,
            p.node.name,
            p.node.bus,
            p.node.interface.map_or("–".into(), |i| i.to_string()),
            if p.hidpp_interface { "yes" } else { "no" },
            if p.boot_keyboard { "yes" } else { "no" },
        );
    }
    let _ = writeln!(w);

    for p in probed.iter().filter(|p| p.hidpp_interface) {
        render_node(w, p);
    }

    let _ = writeln!(w, "## Report descriptors\n");
    for p in probed {
        let _ = writeln!(
            w,
            "<details><summary>{} ({} bytes)</summary>\n\n```\n{}\n```\n</details>\n",
            p.node.hidraw.display(),
            p.node.descriptor.len(),
            hex(&p.node.descriptor)
        );
    }

    let _ = writeln!(w, "## Requests sent\n");
    for r in probe::REQUESTS {
        let _ = writeln!(w, "- {r}");
    }
    let _ = writeln!(
        w,
        "\nDevice indices tried: 0xff, and 1–6 behind a receiver. Software id {:#04x}.",
        probe::SW_ID
    );
    s
}

/// The HID++ section of one interface.
fn render_node(w: &mut String, p: &Probed) {
    let _ = writeln!(
        w,
        "## {} (046d:{:04x}, interface {})\n",
        p.node.hidraw.display(),
        p.node.product,
        p.node.interface.map_or("?".into(), |i| i.to_string())
    );
    match &p.access {
        Access::Denied => {
            let _ = writeln!(
                w,
                "No permission to open the node; run with sudo for HID++ details.\n"
            );
        }
        Access::Error(e) => {
            let _ = writeln!(w, "Could not open the node: {e}\n");
        }
        Access::NotTried => {}
        Access::Ok if p.devices.is_empty() => {
            let _ = writeln!(w, "No HID++ answer on this interface.\n");
        }
        Access::Ok => {
            for d in &p.devices {
                render_device(w, d);
            }
            if p.devices.len() == 1 && p.devices[0].protocol.is_none() {
                let _ = writeln!(
                    w,
                    "No paired device answered on indices 1–6 (switched off, asleep, \
                     or connected by cable right now).\n"
                );
            }
        }
    }
}

fn render_device(w: &mut String, d: &HidppInfo) {
    let Some((major, minor)) = d.protocol else {
        let _ = writeln!(w, "### Device index 0xff: HID++ 1.0 (receiver)\n");
        return;
    };
    let _ = writeln!(
        w,
        "### Device index {:#04x}: {}\n",
        d.device_index,
        d.name.as_deref().unwrap_or("(no name)")
    );
    let _ = writeln!(w, "- HID++ {major}.{minor}");
    if let Some(t) = d.device_type {
        let _ = writeln!(w, "- Type: {} ({t})", device_type(t));
    }
    if let Some(m) = &d.model_ids {
        let _ = writeln!(w, "- Model ids: `{}`", hex(m));
    }
    for f in &d.firmware {
        let _ = writeln!(
            w,
            "- Firmware ({}): {} {:02x}.{:02x} build {:04x}",
            firmware_kind(f.kind),
            f.prefix,
            f.number,
            f.revision,
            f.build
        );
    }
    if let Some(l) = d.keyboard_layout {
        let _ = writeln!(w, "- Keyboard layout id (0x4540): {l}");
    }
    if let Some(g) = d.gkeys {
        let _ = writeln!(w, "- G-keys (0x8010): {g}");
    }
    if let Some(m) = d.mkeys {
        let _ = writeln!(w, "- M-keys (0x8020): {m}");
    }
    if let Some(o) = &d.onboard {
        let desc = &o.description;
        let byte = |i: usize| desc.get(i).copied().unwrap_or(0);
        let _ = writeln!(
            w,
            "- Onboard profiles (0x8100): {} profiles, {} sectors of {} bytes, mode {}, current profile {}",
            byte(3),
            byte(6),
            u16::from_be_bytes([byte(7), byte(8)]),
            o.mode.map_or("?".into(), |m| match m {
                1 => "onboard (1)".into(),
                2 => "host (2)".into(),
                other => other.to_string(),
            }),
            o.current_profile.map_or("?".into(), |p| p.to_string()),
        );
        let _ = writeln!(w, "  - getDescription: `{}`", hex(desc));
    }
    for e in &d.errors {
        let _ = writeln!(w, "- Failed: {e}");
    }
    let _ = writeln!(
        w,
        "\n| Index | Feature | Name | Version | Flags |\n|---|---|---|---|---|"
    );
    for f in &d.features {
        let _ = writeln!(
            w,
            "| 0x{:02x} | 0x{:04x} | {} | {} | {} |",
            f.index,
            f.id,
            feature_name(f.id),
            f.version,
            feature_flags(f.flags)
        );
    }
    let _ = writeln!(w);
}

/// Today's date (UTC) as `YYYY-MM-DD`.
fn today() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (y, m, d) = civil(secs.cast_signed() / 86_400);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since 1970-01-01 → (year, month, day). Howard Hinnant's algorithm.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Write the report next to the user (current folder, else /tmp) and hand
/// it to the real user when run with sudo.
fn write_report(output: Option<PathBuf>, report: &str) -> Result<PathBuf> {
    let name = format!("kyxen-report-{}.md", today());
    let candidates = match output {
        Some(p) => vec![p],
        None => vec![PathBuf::from(&name), std::env::temp_dir().join(&name)],
    };
    let mut last_err = None;
    for path in candidates {
        match std::fs::write(&path, report) {
            Ok(()) => {
                give_to_sudo_user(&path);
                return std::path::absolute(&path).context("resolve report path");
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.map_or_else(
        || anyhow::anyhow!("no place to write the report"),
        Into::into,
    ))
}

fn give_to_sudo_user(path: &Path) {
    let id = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<u32>().ok());
    if let (Some(uid), Some(gid)) = (id("SUDO_UID"), id("SUDO_GID")) {
        let _ = std::os::unix::fs::chown(path, Some(uid), Some(gid));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_402), (2025, 11, 10));
        assert_eq!(civil(11_016), (2000, 2, 29));
    }
}
