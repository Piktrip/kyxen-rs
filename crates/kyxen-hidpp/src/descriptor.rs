//! HID report descriptor inspection.

const VENDOR_PAGE: u32 = 0xff00;
const HIDPP_LONG_ID: u32 = 0x11;

/// Short items of a report descriptor as `(tag, value)`; the tag keeps the
/// type bits (e.g. `0x04` Usage Page, `0x08` Usage, `0x84` Report ID).
fn items(desc: &[u8]) -> impl Iterator<Item = (u8, u32)> + '_ {
    let mut i = 0;
    std::iter::from_fn(move || {
        while i < desc.len() {
            let prefix = desc[i];
            if prefix == 0xfe {
                // long item: fe <size> <tag> <data…>
                i += 3 + usize::from(*desc.get(i + 1)?);
                continue;
            }
            let size = [0, 1, 2, 4][usize::from(prefix & 0x03)];
            let data = desc.get(i + 1..i + 1 + size).unwrap_or_default();
            let value = data
                .iter()
                .rev()
                .fold(0u32, |acc, &b| (acc << 8) | u32::from(b));
            i += 1 + size;
            return Some((prefix & 0xfc, value));
        }
        None
    })
}

/// True if the descriptor declares the HID++ long report (report id 0x11)
/// on Logitech's vendor usage page 0xFF00 — i.e. this hidraw node carries
/// HID++. The G815 has it on USB interface 1, the G915 on interface 2.
pub fn is_hidpp(desc: &[u8]) -> bool {
    let mut usage_page = 0;
    items(desc).any(|(tag, value)| {
        if tag == 0x04 {
            usage_page = value;
        }
        tag == 0x84 && value == HIDPP_LONG_ID && usage_page == VENDOR_PAGE
    })
}

/// True for a boot-protocol keyboard interface: starts with Generic Desktop
/// / Keyboard and declares no report ids.
pub fn is_boot_keyboard(desc: &[u8]) -> bool {
    let mut it = items(desc);
    it.next() == Some((0x04, 0x01))
        && it.next() == Some((0x08, 0x06))
        && !items(desc).any(|(tag, _)| tag == 0x84)
}

#[cfg(test)]
mod tests {
    use super::is_hidpp;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    // Report descriptors read from a G915 (046d:c33e) on USB cable.
    const IFACE0_BOOT_KBD: &str = "05010906a101050719e029e7150025017501950881029505050819012905910295017503\
        910395707501050719042973810295051987298b81029503199029928102c0";
    const IFACE1_MOUSE_CC: &str = "05010902a10185020901a100951075011500250105091901291081029502751016018026\
        ff7f0501093009318106950175081581257f093881069501050c0a38028106c0c0050c09\
        01a101850395027510150126ff0219012aff028100c005010980a1018504950175021501\
        2503098209810983810075068103c0";
    const IFACE2_HIDPP: &str = "0600ff0901a101851095067508150026ff000901810009019100c00600ff0902a1018511\
        95137508150026ff000902810009029100c0";

    #[test]
    fn finds_hidpp_on_g915_interface_2_only() {
        assert!(!is_hidpp(&hex(IFACE0_BOOT_KBD)));
        assert!(!is_hidpp(&hex(IFACE1_MOUSE_CC)));
        assert!(is_hidpp(&hex(IFACE2_HIDPP)));
    }

    #[test]
    fn recognises_boot_keyboard() {
        assert!(super::is_boot_keyboard(&hex(IFACE0_BOOT_KBD)));
        assert!(!super::is_boot_keyboard(&hex(IFACE1_MOUSE_CC)));
        assert!(!super::is_boot_keyboard(&hex(IFACE2_HIDPP)));
    }

    #[test]
    fn report_0x11_outside_vendor_page_is_not_hidpp() {
        assert!(!is_hidpp(&hex("05010906a1018511c0")));
    }
}
