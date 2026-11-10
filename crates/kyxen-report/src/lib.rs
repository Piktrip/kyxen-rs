//! Read-only hardware report for Logitech keyboards.
//!
//! For people who want to help add support for their keyboard without
//! writing code: Kyxen writes a Markdown file (app: Settings → Help, or
//! `kyxend report`) to attach to an issue. It only asks the keyboard
//! questions (HID++ getters) and changes nothing on it.

mod probe;
mod system;

use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use kyxen_hidpp::descriptor;
use kyxen_hidpp::hidraw::Hidraw;

use probe::HidppInfo;
use system::{Node, System};

/// Where reports go: a new issue with the keyboard-report template.
pub const ISSUE_URL: &str =
    "https://github.com/Piktrip/kyxen-rs/issues/new?template=keyboard-report.yml";

/// What the report does and the disclaimer, for the command line.
pub fn notice(german: bool) -> &'static str {
    if german {
        "Kyxen-Tastaturbericht\n\n\
         Liest Informationen über deine Logitech-Geräte aus und schreibt sie in eine\n\
         Textdatei. Es stellt der Tastatur nur Fragen und ändert nichts an ihr: keine\n\
         Einstellungen, kein Speicher, keine Beleuchtung. Der Bericht enthält keine\n\
         Seriennummern, Benutzer- oder Rechnernamen; du kannst ihn vor dem Teilen lesen.\n\n\
         Benutzung auf eigene Gefahr. Kyxen ist ein inoffizielles Hobby-Projekt ohne\n\
         Verbindung zu Logitech und kommt ohne jede Gewährleistung (GPL-3.0)."
    } else {
        "Kyxen keyboard report\n\n\
         Reads information about your Logitech devices and writes it to a text file.\n\
         It only asks the keyboard questions and changes nothing on it: no settings, no\n\
         memory, no lighting. The report contains no serial numbers, user or host\n\
         names; you can read it before sharing.\n\n\
         Use at your own risk. Kyxen is an unofficial hobby project, not affiliated with\n\
         Logitech, and comes without any warranty (GPL-3.0)."
    }
}

/// Everything collected, ready to render.
#[derive(Debug)]
pub struct Report {
    sys: System,
    probed: Vec<Probed>,
}

/// Collect the report: sysfs facts for every Logitech HID node and HID++
/// answers from every interface we may open. Takes a few seconds when a
/// receiver is present (it asks each of its six slots).
pub fn collect() -> Report {
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
        probed.push(p);
    }
    Report { sys, probed }
}

impl Report {
    /// The report as Markdown.
    pub fn markdown(&self) -> String {
        render(&self.sys, &self.probed)
    }

    /// No Logitech device found at all.
    pub fn is_empty(&self) -> bool {
        self.probed.is_empty()
    }

    /// HID++ nodes we were not allowed to open (no udev rule; `sudo` helps).
    pub fn denied(&self) -> Vec<PathBuf> {
        self.probed
            .iter()
            .filter(|p| p.access == Access::Denied)
            .map(|p| p.node.hidraw.clone())
            .collect()
    }

    /// One line per node: `/dev/hidrawN  046d:pppp  name  →  device (n features)`.
    pub fn summary(&self) -> Vec<String> {
        self.probed.iter().map(summary_line).collect()
    }
}

/// `kyxen-report-YYYY-MM-DD.md`
pub fn file_name() -> String {
    format!("kyxen-report-{}.md", today())
}

/// Write `report` to the first of `candidates` that works and hand the file
/// to the real user when running under sudo. Returns the absolute path.
pub fn write(report: &str, candidates: &[PathBuf]) -> Result<PathBuf> {
    let mut last_err = None;
    for path in candidates {
        match std::fs::write(path, report) {
            Ok(()) => {
                give_to_sudo_user(path);
                return std::path::absolute(path).context("resolve report path");
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.map_or_else(
        || anyhow::anyhow!("no place to write the report"),
        Into::into,
    ))
}

/// A probed node: sysfs facts plus what HID++ told us.
#[derive(Debug)]
struct Probed {
    node: Node,
    hidpp_interface: bool,
    boot_keyboard: bool,
    access: Access,
    devices: Vec<HidppInfo>,
}

#[derive(Debug, PartialEq)]
enum Access {
    NotTried,
    Denied,
    Error(String),
    Ok,
}

fn summary_line(p: &Probed) -> String {
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
    line
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
        "Generated by Kyxen {} on {} (UTC). Read-only: the program only sent the \
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
