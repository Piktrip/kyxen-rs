//! Tray icon (StatusNotifierItem, shown natively by KDE Plasma).

use std::thread;

use ksni::menu::{RadioGroup, RadioItem, StandardItem};
use ksni::{Icon, MenuItem, ToolTip};
use kyxen_core::ipc::{self, Request};

use crate::AppWindow;

const ICON_32: &[u8] = include_bytes!("../assets/icon-32.png");
const ICON_64: &[u8] = include_bytes!("../assets/icon-64.png");

/// RGBA PNG → ARGB32 (network byte order) as the SNI spec wants it.
fn decode(png_bytes: &[u8]) -> Option<Icon> {
    let mut reader = png::Decoder::new(png_bytes).read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let data = buf[..info.buffer_size()]
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[r, g, b, a]| [a, r, g, b])
        .collect();
    Some(Icon {
        width: i32::try_from(info.width).ok()?,
        height: i32::try_from(info.height).ok()?,
        data,
    })
}

/// Menu texts in the app's language (set from the UI thread).
#[derive(Debug, Default, Clone)]
pub struct TrayTexts {
    pub profile: String,
    pub open: String,
    pub quit: String,
    pub no_keyboard: String,
}

pub struct KyxenTray {
    ui: slint::Weak<AppWindow>,
    icons: Vec<Icon>,
    /// `(profile id, display name)`
    pub profiles: Vec<(String, String)>,
    pub active: usize,
    pub keyboard: Option<String>,
    pub texts: TrayTexts,
}

impl std::fmt::Debug for KyxenTray {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KyxenTray")
            .field("profiles", &self.profiles)
            .field("active", &self.active)
            .finish_non_exhaustive()
    }
}

impl KyxenTray {
    pub fn new(ui: slint::Weak<AppWindow>) -> Self {
        Self {
            ui,
            icons: [ICON_32, ICON_64]
                .iter()
                .filter_map(|b| decode(b))
                .collect(),
            profiles: Vec::new(),
            active: 0,
            keyboard: None,
            texts: TrayTexts::default(),
        }
    }

    fn show_window(&self) {
        // The host sends the click's activation token separately and may
        // deliver it just after the click itself (KDE does).
        for _ in 0..20 {
            if let Some(token) = ksni::current_xdg_activation_token() {
                crate::activation::set(token);
                break;
            }
            thread::sleep(std::time::Duration::from_millis(5));
        }
        let _ = self.ui.upgrade_in_event_loop(|w| crate::present(&w));
    }
}

impl ksni::Tray for KyxenTray {
    fn id(&self) -> String {
        "kyxen".into()
    }

    fn title(&self) -> String {
        "Kyxen".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        self.icons.clone()
    }

    fn tool_tip(&self) -> ToolTip {
        let profile = self
            .profiles
            .get(self.active)
            .map_or("–", |(_, n)| n.as_str());
        let device = self
            .keyboard
            .as_deref()
            .unwrap_or(&self.texts.no_keyboard);
        ToolTip {
            title: "Kyxen".into(),
            description: format!("{}: {profile}\n{device}", self.texts.profile),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.show_window();
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items: Vec<MenuItem<Self>> = vec![
            StandardItem {
                label: self.texts.profile.clone(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            RadioGroup {
                selected: self.active,
                select: Box::new(|tray: &mut Self, i| {
                    if let Some((id, _)) = tray.profiles.get(i) {
                        tray.active = i;
                        let profile = id.clone();
                        // never block the menu on the daemon
                        thread::spawn(move || ipc::command(&Request::Switch { profile }));
                    }
                }),
                options: self
                    .profiles
                    .iter()
                    .map(|(_, name)| RadioItem {
                        label: name.clone(),
                        ..Default::default()
                    })
                    .collect(),
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: self.texts.open.clone(),
                icon_name: "configure".into(),
                activate: Box::new(|tray: &mut Self| tray.show_window()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: self.texts.quit.clone(),
                icon_name: "application-exit".into(),
                activate: Box::new(|_| {
                    let _ = slint::invoke_from_event_loop(|| {
                        let _ = slint::quit_event_loop();
                    });
                }),
                ..Default::default()
            }
            .into(),
        ];
        if self.profiles.is_empty() {
            items.remove(1);
        }
        items
    }
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn icons_decode_to_argb() {
        let icon = decode(super::ICON_32).expect("RGBA icon");
        assert_eq!((icon.width, icon.height), (32, 32));
        assert_eq!(icon.data.len(), 32 * 32 * 4);
    }
}
