//! G815 G-key path (as in Python Kyxen): G1–G5 are remapped to F13–F17 in
//! flash, read from the boot-protocol hidraw node, and suppressed in a
//! grabbed passthrough of the keyboard's evdev node so applications never
//! see them. Not used for the G915, whose G-keys come via HID++.

use std::io::Read;
use std::path::Path;
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use evdev::uinput::VirtualDevice;
use evdev::{Device, EventType, InputEvent, KeyCode};
use tracing::{info, warn};

use crate::daemon::Event;

/// HID usages F13–F17 → G1–G5.
const GKEY_USAGES: [u8; 5] = [0x68, 0x69, 0x6a, 0x6b, 0x6c];
const SUPPRESSED: [KeyCode; 5] = [
    KeyCode::KEY_F13,
    KeyCode::KEY_F14,
    KeyCode::KEY_F15,
    KeyCode::KEY_F16,
    KeyCode::KEY_F17,
];

/// Forward everything from `event_node` except F13–F17 through a virtual keyboard.
pub fn spawn_passthrough(event_node: &Path) -> Result<JoinHandle<()>> {
    let mut dev =
        Device::open(event_node).with_context(|| format!("open {}", event_node.display()))?;
    let keys = dev
        .supported_keys()
        .context("keyboard reports no keys")?
        .to_owned();
    let mut out = VirtualDevice::builder()?
        .name("Kyxen passthrough")
        .with_keys(keys)?
        .build()?;
    dev.grab().context("grab keyboard")?;
    info!(node = %event_node.display(), "keyboard grabbed for G-key suppression");
    Ok(thread::Builder::new()
        .name("passthrough".into())
        .spawn(move || {
            loop {
                let batch: Vec<InputEvent> = match dev.fetch_events() {
                    Ok(events) => events
                        .filter(|e| e.event_type() != EventType::SYNCHRONIZATION)
                        .filter(|e| {
                            !(e.event_type() == EventType::KEY
                                && SUPPRESSED.contains(&KeyCode::new(e.code())))
                        })
                        .collect(),
                    Err(e) => {
                        warn!("keyboard input ended: {e}");
                        return;
                    }
                };
                if !batch.is_empty()
                    && let Err(e) = out.emit(&batch)
                {
                    warn!("passthrough write failed: {e}");
                }
            }
        })?)
}

/// G-key edges from 8-byte boot keyboard reports `[mods, 0, k1..k6]`.
pub fn boot_report_gkeys(prev: &[u8], report: &[u8]) -> Vec<(u8, bool)> {
    let held = |r: &[u8]| -> Vec<u8> {
        r.iter()
            .skip(2)
            .filter_map(|u| GKEY_USAGES.iter().position(|g| g == u))
            .map(|i| i as u8 + 1)
            .collect()
    };
    let (before, now) = (held(prev), held(report));
    let pressed = now
        .iter()
        .filter(|g| !before.contains(g))
        .map(|&g| (g, true));
    let released = before
        .iter()
        .filter(|g| !now.contains(g))
        .map(|&g| (g, false));
    pressed.chain(released).collect()
}

/// Read G-keys from the boot-protocol hidraw node.
pub fn spawn_boot_reader(hidraw: &Path, events: Sender<Event>) -> Result<JoinHandle<()>> {
    let mut file =
        std::fs::File::open(hidraw).with_context(|| format!("open {}", hidraw.display()))?;
    Ok(thread::Builder::new()
        .name("boot-gkeys".into())
        .spawn(move || {
            let mut prev = [0u8; 8];
            let mut buf = [0u8; 64];
            loop {
                let n = match file.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => n,
                };
                for (g, pressed) in boot_report_gkeys(&prev, &buf[..n]) {
                    let _ = events.send(Event::GKey { n: g, pressed });
                }
                let len = n.min(8);
                prev = [0; 8];
                prev[..len].copy_from_slice(&buf[..len]);
            }
        })?)
}

#[cfg(test)]
mod tests {
    use super::boot_report_gkeys;

    #[test]
    fn boot_reports_to_gkey_edges() {
        let idle = [0u8; 8];
        let g1 = [0, 0, 0x68, 0, 0, 0, 0, 0];
        let g1_g5 = [0, 0, 0x68, 0x6c, 0, 0, 0, 0];
        assert_eq!(boot_report_gkeys(&idle, &g1), [(1, true)]);
        assert_eq!(boot_report_gkeys(&g1, &g1_g5), [(5, true)]);
        assert_eq!(boot_report_gkeys(&g1_g5, &idle), [(1, false), (5, false)]);
        // a normal key (A = 0x04) is not a G-key
        assert!(boot_report_gkeys(&idle, &[0, 0, 0x04, 0, 0, 0, 0, 0]).is_empty());
    }
}
