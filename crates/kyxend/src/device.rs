//! Find a supported keyboard through sysfs.

use std::fs;
use std::path::{Path, PathBuf};

use kyxen_hidpp::descriptor;

pub const LOGITECH: u16 = 0x046d;

/// How a model delivers G-keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GKeyMode {
    /// GKEY software control diverts them to HID++ notifications (G915).
    /// No flash writes, no input grab.
    Hidpp,
    /// Flash remap to F13–F17, read from the boot-protocol interface and
    /// suppressed in a grabbed passthrough (G815, as in Python Kyxen).
    BootProtocol,
}

#[derive(Debug, Clone, Copy)]
pub struct Model {
    pub product_id: u16,
    pub name: &'static str,
    pub gkeys: GKeyMode,
    /// Send the G815's G Hub lighting init (fixed feature indices).
    pub g815_lighting_init: bool,
}

pub static MODELS: &[Model] = &[
    Model {
        product_id: 0xc33f,
        name: "Logitech G815",
        gkeys: GKeyMode::BootProtocol,
        g815_lighting_init: true,
    },
    Model {
        product_id: 0xc33e,
        name: "Logitech G915 (wired)",
        gkeys: GKeyMode::Hidpp,
        g815_lighting_init: false,
    },
];

/// A detected keyboard and its device nodes.
#[derive(Debug, Clone)]
pub struct Keyboard {
    pub model: Model,
    /// hidraw node that speaks HID++.
    pub hidpp: PathBuf,
    /// hidraw node of the boot-keyboard interface (G815 G-keys).
    pub boot_hidraw: Option<PathBuf>,
    /// evdev node of the boot-keyboard interface (grabbed on the G815).
    pub boot_event: Option<PathBuf>,
}

#[derive(Debug)]
struct Node {
    hidraw: PathBuf,
    product: u16,
    descriptor: Vec<u8>,
    sysdir: PathBuf,
}

fn hid_ids(uevent: &str) -> Option<(u16, u16)> {
    let id = uevent.lines().find_map(|l| l.strip_prefix("HID_ID="))?;
    let mut parts = id.split(':').skip(1);
    let vendor = u32::from_str_radix(parts.next()?, 16).ok()?;
    let product = u32::from_str_radix(parts.next()?, 16).ok()?;
    Some((u16::try_from(vendor).ok()?, u16::try_from(product).ok()?))
}

fn scan(sys_class: &Path) -> Vec<Node> {
    let Ok(entries) = fs::read_dir(sys_class) else {
        return Vec::new();
    };
    let mut nodes: Vec<Node> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let sysdir = e.path().join("device");
            let (vendor, product) = hid_ids(&fs::read_to_string(sysdir.join("uevent")).ok()?)?;
            (vendor == LOGITECH).then(|| Node {
                hidraw: Path::new("/dev").join(e.file_name()),
                product,
                descriptor: fs::read(sysdir.join("report_descriptor")).unwrap_or_default(),
                sysdir,
            })
        })
        .collect();
    nodes.sort_by(|a, b| a.hidraw.cmp(&b.hidraw));
    nodes
}

fn event_node(sysdir: &Path) -> Option<PathBuf> {
    for input in fs::read_dir(sysdir.join("input"))
        .ok()?
        .filter_map(Result::ok)
    {
        for ev in fs::read_dir(input.path()).ok()?.filter_map(Result::ok) {
            let name = ev.file_name();
            if name.to_string_lossy().starts_with("event") {
                return Some(Path::new("/dev/input").join(name));
            }
        }
    }
    None
}

/// Detect the first supported keyboard.
pub fn detect() -> Option<Keyboard> {
    detect_in(Path::new("/sys/class/hidraw"))
}

pub fn detect_in(sys_class: &Path) -> Option<Keyboard> {
    let nodes = scan(sys_class);
    MODELS.iter().find_map(|model| {
        let mine: Vec<&Node> = nodes
            .iter()
            .filter(|n| n.product == model.product_id)
            .collect();
        let hidpp = mine.iter().find(|n| descriptor::is_hidpp(&n.descriptor))?;
        let boot = mine
            .iter()
            .find(|n| descriptor::is_boot_keyboard(&n.descriptor));
        Some(Keyboard {
            model: *model,
            hidpp: hidpp.hidraw.clone(),
            boot_hidraw: boot.map(|n| n.hidraw.clone()),
            boot_event: boot.and_then(|n| event_node(&n.sysdir)),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Build a fake /sys/class/hidraw tree with the G915's three interfaces.
    #[test]
    fn detects_g915_interfaces() {
        let root = std::env::temp_dir().join(format!("kyxen-sysfs-{}", std::process::id()));
        let descs = [
            (
                "hidraw3",
                "05010906a101050719e029e7150025017501950881029505c0",
                true,
            ), // boot kbd
            ("hidraw4", "05010902a1018502c0", false),
            (
                "hidraw5",
                "0600ff0901a101851095067508150026ff00c00600ff0902a1018511c0",
                false,
            ),
        ];
        for (name, desc, with_input) in descs {
            let dev = root.join(name).join("device");
            fs::create_dir_all(&dev).unwrap();
            fs::write(
                dev.join("uevent"),
                "DRIVER=hid-generic\nHID_ID=0003:0000046D:0000C33E\n",
            )
            .unwrap();
            fs::write(dev.join("report_descriptor"), hex(desc)).unwrap();
            if with_input {
                fs::create_dir_all(dev.join("input/input4/event4")).unwrap();
            }
        }
        let kb = detect_in(&root).expect("G915 detected");
        assert_eq!(kb.model.product_id, 0xc33e);
        assert_eq!(kb.hidpp, Path::new("/dev/hidraw5"));
        assert_eq!(kb.boot_hidraw.as_deref(), Some(Path::new("/dev/hidraw3")));
        assert_eq!(
            kb.boot_event.as_deref(),
            Some(Path::new("/dev/input/event4"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_hid_id() {
        assert_eq!(
            hid_ids("HID_ID=0003:0000046D:0000C33F\n"),
            Some((0x046d, 0xc33f))
        );
        assert_eq!(hid_ids("nothing"), None);
    }
}
