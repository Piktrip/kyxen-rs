//! System facts and Logitech HID nodes from sysfs (no device access needed).
//!
//! Left out on purpose: user and host names, serial numbers (`HID_UNIQ`),
//! USB bus paths.

use std::fs;
use std::path::{Path, PathBuf};

pub const LOGITECH: u16 = 0x046d;

#[derive(Debug, Clone)]
pub struct System {
    pub os: String,
    pub kernel: String,
    pub desktop: String,
    pub session: String,
    pub layout: String,
    pub udev_rule: bool,
    pub daemon: Option<String>,
}

pub fn system() -> System {
    let os = fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                .map(|v| v.trim_matches('"').to_owned())
        })
        .unwrap_or_else(|| "unknown".into());
    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease")
        .map_or_else(|_| "unknown".into(), |s| s.trim().to_owned());
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let (layout, variant) = kyxen_core::layout::detect(None, None);
    let udev_rule = fs::read_dir("/etc/udev/rules.d").is_ok_and(|d| {
        d.filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().contains("kyxen"))
    });
    let daemon = kyxen_core::ipc::status()
        .map(|s| s.keyboard.unwrap_or_else(|| "running, no keyboard".into()));
    System {
        os,
        kernel,
        desktop: env("XDG_CURRENT_DESKTOP").unwrap_or_else(|| "unknown (sudo?)".into()),
        session: env("XDG_SESSION_TYPE").unwrap_or_else(|| "unknown (sudo?)".into()),
        layout: if variant.is_empty() {
            layout
        } else {
            format!("{layout} ({variant})")
        },
        udev_rule,
        daemon,
    }
}

/// One Logitech hidraw node.
#[derive(Debug, Clone)]
pub struct Node {
    pub hidraw: PathBuf,
    pub product: u16,
    pub name: String,
    pub bus: String,
    /// USB interface number, if the node sits on a USB interface.
    pub interface: Option<u8>,
    pub descriptor: Vec<u8>,
}

fn hid_id(uevent: &str) -> Option<(u16, u16, u16)> {
    let id = uevent.lines().find_map(|l| l.strip_prefix("HID_ID="))?;
    let mut parts = id.split(':');
    let bus = u16::from_str_radix(parts.next()?, 16).ok()?;
    let vendor = u32::from_str_radix(parts.next()?, 16).ok()?;
    let product = u32::from_str_radix(parts.next()?, 16).ok()?;
    Some((
        bus,
        u16::try_from(vendor).ok()?,
        u16::try_from(product).ok()?,
    ))
}

fn bus_name(bus: u16) -> String {
    match bus {
        0x03 => "USB".into(),
        0x05 => "Bluetooth".into(),
        other => format!("bus {other:#04x}"),
    }
}

/// USB interface number from the resolved sysfs path (`…/1-3:1.2/…`).
fn interface_of(sysdir: &Path) -> Option<u8> {
    let real = fs::canonicalize(sysdir).ok()?;
    real.components().rev().find_map(|c| {
        let s = c.as_os_str().to_string_lossy();
        let (_, cfg) = s.split_once(':')?;
        let (_, iface) = cfg.split_once('.')?;
        iface.parse().ok()
    })
}

/// All Logitech hidraw nodes, sorted by node name.
pub fn nodes(sys_class: &Path) -> Vec<Node> {
    let Ok(entries) = fs::read_dir(sys_class) else {
        return Vec::new();
    };
    let mut out: Vec<Node> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let sysdir = e.path().join("device");
            let uevent = fs::read_to_string(sysdir.join("uevent")).ok()?;
            let (bus, vendor, product) = hid_id(&uevent)?;
            (vendor == LOGITECH).then(|| Node {
                hidraw: Path::new("/dev").join(e.file_name()),
                product,
                name: uevent
                    .lines()
                    .find_map(|l| l.strip_prefix("HID_NAME="))
                    .unwrap_or("")
                    .to_owned(),
                bus: bus_name(bus),
                interface: interface_of(&sysdir),
                descriptor: fs::read(sysdir.join("report_descriptor")).unwrap_or_default(),
            })
        })
        .collect();
    out.sort_by_key(|n| {
        let s = n.hidraw.to_string_lossy();
        s.trim_start_matches("/dev/hidraw")
            .parse::<u32>()
            .unwrap_or(u32::MAX)
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hid_id() {
        let ev = "DRIVER=hid-generic\nHID_ID=0003:0000046D:0000C33E\nHID_NAME=Logitech G915\n";
        assert_eq!(hid_id(ev), Some((3, 0x046d, 0xc33e)));
    }
}
