//! Cross-check every text-layout entry against xkbcommon (`xkbcli
//! how-to-type`). Skipped where xkbcli is not installed.

use std::collections::HashSet;
use std::process::Command;

use kyxen_core::layout::{Mods, TextLayout};

/// xkb keycode = evdev keycode + 8
const XKB_OFFSET: u16 = 8;

fn xkbcli_available() -> bool {
    Command::new("xkbcli").arg("--help").output().is_ok()
}

fn ways(layout: &str, variant: &str, what: &str, keysym: bool) -> HashSet<(u16, Mods)> {
    let mut cmd = Command::new("xkbcli");
    cmd.args(["how-to-type", "--layout", layout]);
    if !variant.is_empty() {
        cmd.args(["--variant", variant]);
    }
    if keysym {
        cmd.arg("--keysym");
    }
    let out = cmd.arg(what).output().expect("run xkbcli");
    let text = String::from_utf8_lossy(&out.stdout);
    let direct = text
        .split("=== Access via Compose")
        .next()
        .unwrap_or_default();
    direct
        .lines()
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            let keycode: u16 = cols.next()?.parse().ok()?;
            let _keyname = cols.next()?;
            if cols.next()? != "1" {
                return None; // only the first layout group
            }
            let mods = line.split_once('[')?.1.trim_end_matches(']');
            if mods.contains("Lock") {
                return None;
            }
            let m = Mods {
                shift: mods.contains("Shift"),
                altgr: mods.contains("Mod5") || mods.contains("LevelThree"),
            };
            Some((keycode - XKB_OFFSET, m))
        })
        .collect()
}

#[test]
fn layouts_match_xkbcommon() {
    if !xkbcli_available() {
        eprintln!("xkbcli not installed — skipping");
        return;
    }
    let mut mismatches = Vec::new();
    for (layout, variant) in [
        ("us", ""),
        ("gb", ""),
        ("de", ""),
        ("at", ""),
        ("de", "nodeadkeys"),
        ("at", "nodeadkeys"),
    ] {
        let tl = TextLayout::new(layout, variant);
        for (c, key, mods) in tl.chars() {
            if c == '\n' || c == '\t' {
                continue;
            }
            let found = if tl.is_dead(c) {
                let sym = match c {
                    '^' => "dead_circumflex",
                    '´' => "dead_acute",
                    _ => "dead_grave",
                };
                ways(layout, variant, sym, true)
            } else {
                ways(layout, variant, &c.to_string(), false)
            };
            if !found.contains(&(key.code(), mods)) {
                mismatches.push(format!(
                    "{layout}({variant}) {c:?}: kyxen {key:?} {mods:?}, xkb {found:?}"
                ));
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
