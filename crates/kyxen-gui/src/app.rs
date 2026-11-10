//! Connects the Slint UI to the config files and the daemon.

// Slint models index with i32; list positions here are always tiny.
#![allow(clippy::cast_possible_wrap)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::{OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use kyxen_core::Rgb;
use kyxen_core::colour::Colour;
use kyxen_core::config::{
    ActionKind, AnimationConfig, Direction, GlobalConfig, LightingConfig, LightingMode,
    MacroAction, MouseButton, MouseMode, PresetConfig, PresetName, Profile, Slide, Store,
    Transition,
};
use kyxen_core::effects;
use kyxen_core::ipc::{self, Request, Status};
use kyxen_core::keymap::{Form, Keymap};
use kyxen_core::layout::{self, TextLayout};
use slint::{Color, ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::labels;
use crate::{AppWindow, GKeyInfo, KeyCap, ProfileItem, Thumb, Tr, Ui};

/// Text layout choices; the names are `Tr.layouts` in the UI.
const LAYOUTS: [Option<&str>; 5] = [None, Some("at"), Some("de"), Some("us"), Some("gb")];
/// Key shape choices: automatic, ISO, ANSI.
const FORMS: [Option<&str>; 3] = [None, Some("iso"), Some("ansi")];
/// App language choices: system, German, English.
const LANGUAGES: [Option<&str>; 3] = [None, Some("de"), Some("en")];

/// Switch the UI language: `Some("de")`, `Some("en")` or the system language.
/// Translations are compiled in; English is the source language.
fn apply_language(lang: Option<&str>) {
    let code = lang.map_or_else(system_language, str::to_owned);
    let bundle = if code == "de" { "de" } else { "" };
    if let Err(e) = slint::select_bundled_translation(bundle) {
        tracing::warn!("language {code}: {e}");
    }
}

/// Language part of the locale (`de_AT.UTF-8` → `de`), as gettext picks it.
fn system_language() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.is_empty())
        .map(|v| {
            v.split(['_', '.', '@'])
                .next()
                .unwrap_or_default()
                .to_owned()
        })
        .unwrap_or_default()
}
const PALETTE: [&str; 8] = [
    "#3ddc84", "#00d1ff", "#0a84ff", "#bf5af2", "#ff2d95", "#ff9500", "#ffd60a", "#ff3b30",
];
const THUMB_COLUMNS: usize = 16;

fn rgb_to_slint(Rgb(r, g, b): Rgb) -> Color {
    Color::from_rgb_u8(r, g, b)
}

fn slint_to_rgb(c: Color) -> Rgb {
    Rgb(c.red(), c.green(), c.blue())
}

/// RGB → (hue degrees, saturation, value).
fn to_hsv(Rgb(r, g, b): Rgb) -> (f32, f32, f32) {
    let c = Color::from_rgb_u8(r, g, b).to_hsva();
    (c.hue, c.saturation, c.value)
}

fn from_hsv(h: f32, s: f32, v: f32) -> Rgb {
    slint_to_rgb(Color::from_hsva(h, s, v, 1.0))
}

fn rate_to_slider(ms: f64) -> f32 {
    ((ms.clamp(10.0, 1000.0) / 10.0).log(100.0)) as f32
}

fn slider_to_rate(v: f32) -> f64 {
    (10.0 * 100f64.powf(f64::from(v.clamp(0.0, 1.0)))).round()
}

fn speed_to_slider(s: f64) -> f32 {
    ((s.clamp(0.1, 5.0) / 0.1).log(50.0)) as f32
}

fn slider_to_speed(v: f32) -> f64 {
    (0.1 * 50f64.powf(f64::from(v.clamp(0.0, 1.0))) * 20.0).round() / 20.0
}

fn kind_index(a: ActionKind) -> i32 {
    match a {
        ActionKind::None | ActionKind::Unknown => 0,
        ActionKind::Type => 1,
        ActionKind::Combo => 2,
        ActionKind::Command => 3,
        ActionKind::HoldToggle => 4,
        ActionKind::MouseButton => 5,
    }
}

fn kind_from(i: i32) -> ActionKind {
    match i {
        1 => ActionKind::Type,
        2 => ActionKind::Combo,
        3 => ActionKind::Command,
        4 => ActionKind::HoldToggle,
        5 => ActionKind::MouseButton,
        _ => ActionKind::None,
    }
}

const MOUSE_BUTTONS: [MouseButton; 5] = [
    MouseButton::Left,
    MouseButton::Right,
    MouseButton::Middle,
    MouseButton::Button4,
    MouseButton::Button5,
];
const MOUSE_MODES: [MouseMode; 3] = [MouseMode::Click, MouseMode::DoubleClick, MouseMode::Hold];
const DIRECTIONS: [Direction; 5] = [
    Direction::LeftRight,
    Direction::RightLeft,
    Direction::TopBottom,
    Direction::BottomTop,
    Direction::Radial,
];

fn effect_index(l: &LightingConfig) -> i32 {
    match (l.mode, l.preset.as_ref().map(|p| p.name)) {
        (LightingMode::Preset, Some(PresetName::Breathing)) => 1,
        (LightingMode::Preset, Some(PresetName::Wave)) => 2,
        (LightingMode::Preset, Some(PresetName::RainbowWave)) => 3,
        (LightingMode::Preset, Some(PresetName::ColourCycle)) => 4,
        (LightingMode::Animation, _) => 5,
        _ => 0,
    }
}

/// Lighting config showing effect `i`, keeping colours/speed from `base`.
fn with_effect(base: &LightingConfig, i: i32) -> LightingConfig {
    let mut l = base.clone();
    let mut preset = l.preset.clone().unwrap_or_else(|| PresetConfig {
        colours: vec![base.base_colour],
        ..PresetConfig::default()
    });
    match i {
        0 => l.mode = LightingMode::Static,
        1..=4 => {
            l.mode = LightingMode::Preset;
            preset.name = [
                PresetName::Breathing,
                PresetName::Wave,
                PresetName::RainbowWave,
                PresetName::ColourCycle,
            ][(i - 1) as usize];
            if preset.colours.is_empty() {
                preset.colours.push(base.base_colour);
            }
            if i == 2 && preset.colours.len() < 2 {
                preset.colours.push(Colour(Rgb(0xff, 0x00, 0xbd)));
            }
            l.preset = Some(preset);
        }
        _ => {
            l.mode = LightingMode::Animation;
            if l.animation.as_ref().is_none_or(|a| a.slides.is_empty()) {
                l.animation = Some(demo_animation());
            }
        }
    }
    l
}

fn demo_animation() -> AnimationConfig {
    let slide = |c: Rgb| Slide {
        keys: Keymap::g815()
            .lit()
            .chain(Keymap::g815_ansi().lit())
            .map(|(n, _)| (n.to_owned(), Colour(c)))
            .collect(),
        hold: 0.6,
        transition: Transition::Hsv,
        transition_duration: 1.2,
    };
    AnimationConfig {
        looped: true,
        slides: vec![
            slide(Rgb(0x3d, 0xdc, 0x84)),
            slide(Rgb(0x00, 0xd1, 0xff)),
            slide(Rgb(0xbf, 0x5a, 0xf2)),
        ],
    }
}

/// Send a request to the daemon off the UI thread.
///
/// One worker sends them in order: with a thread per request, quick clicks
/// A → B → A could reach the daemon as A, A, B and leave it on another
/// profile than the UI shows.
fn send_daemon(req: Request) {
    static QUEUE: OnceLock<mpsc::Sender<Request>> = OnceLock::new();
    let tx = QUEUE.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Request>();
        thread::spawn(move || {
            while let Ok(first) = rx.recv() {
                let mut batch = vec![first];
                batch.extend(rx.try_iter());
                for req in coalesce(batch) {
                    ipc::command(&req);
                }
            }
        });
        tx
    });
    let _ = tx.send(req);
}

/// Drop requests a later one in the same burst makes moot: only the last
/// profile switch and the last reload count. Order is kept otherwise.
fn coalesce(batch: Vec<Request>) -> Vec<Request> {
    let last_switch = batch
        .iter()
        .rposition(|r| matches!(r, Request::Switch { .. }));
    let last_reload = batch.iter().rposition(|r| matches!(r, Request::Reload));
    batch
        .into_iter()
        .enumerate()
        .filter(|(i, r)| match r {
            Request::Switch { .. } => Some(*i) == last_switch,
            Request::Reload => Some(*i) == last_reload,
            _ => true,
        })
        .map(|(_, r)| r)
        .collect()
}

#[derive(Default)]
struct Recording {
    held: Vec<String>,
    captured: Vec<String>,
}

pub struct App {
    ui: slint::Weak<AppWindow>,
    store: Store,
    global: GlobalConfig,
    profiles: Vec<Profile>,
    cur: usize,
    gkey: u8,
    brush: Rgb,
    stop: usize,
    layout: String,
    text_layout: TextLayout,
    keys: Rc<VecModel<KeyCap>>,
    thumbs: Rc<VecModel<Thumb>>,
    key_x: BTreeMap<u8, f32>,
    effect_started: Instant,
    tick: u32,
    save_timer: Timer,
    recording: Option<Recording>,
    /// Never write files (screenshot mode).
    pub read_only: bool,
    tray: Option<ksni::blocking::Handle<crate::tray::KyxenTray>>,
    keyboard: Option<String>,
    /// The daemon's active profile in the last status; the UI follows
    /// changes of it (M1–M3), not mere differences from its own selection.
    daemon_profile: Option<String>,
    /// Last profile change made in the UI (switch, new, delete).
    local_change: Option<Instant>,
}

pub type Shared = Rc<RefCell<App>>;

impl App {
    pub fn new(ui: &AppWindow) -> Shared {
        let store = Store::default();
        let global = store.load_global().unwrap_or_default();
        let (mut profiles, failed) = store.load_profiles();
        for e in failed {
            tracing::warn!("skipping profile: {e}");
        }
        if profiles.is_empty() {
            profiles.push(store.ensure_default_profile().unwrap_or_default());
        }
        let cur = profiles
            .iter()
            .position(|p| p.name == global.active_profile)
            .unwrap_or(0);
        let (layout, variant) = layout::detect(
            global.keyboard_layout.as_deref(),
            global.keyboard_variant.as_deref(),
        );
        apply_language(global.language.as_deref());
        let app = Rc::new(RefCell::new(Self {
            ui: ui.as_weak(),
            store,
            global,
            profiles,
            cur,
            gkey: 1,
            brush: Rgb(0x3d, 0xdc, 0x84),
            stop: 0,
            text_layout: TextLayout::new(&layout, &variant),
            layout,
            keys: Rc::new(VecModel::default()),
            thumbs: Rc::new(VecModel::from(vec![
                Thumb {
                    colors: ModelRc::default()
                };
                6
            ])),
            key_x: BTreeMap::new(),
            effect_started: Instant::now(),
            tick: 0,
            save_timer: Timer::default(),
            recording: None,
            read_only: false,
            tray: None,
            keyboard: None,
            daemon_profile: None,
            local_change: None,
        }));
        {
            let mut a = app.borrow_mut();
            a.build_keys();
            let g = ui.global::<Ui>();
            g.set_keys(ModelRc::from(a.keys.clone()));
            g.set_thumbs(ModelRc::from(a.thumbs.clone()));
            g.set_version(env!("CARGO_PKG_VERSION").into());
            g.set_palette(ModelRc::from(Rc::new(VecModel::from(
                PALETTE
                    .iter()
                    .map(|p| rgb_to_slint(p.parse::<Colour>().map(|c| c.0).unwrap_or_default()))
                    .collect::<Vec<_>>(),
            ))));
            g.set_save_state(ui.global::<Tr>().get_saved());
            a.push_all();
        }
        app
    }

    /// Show the tray icon; `false` if there is no tray host.
    pub fn spawn_tray(&mut self) -> bool {
        use ksni::blocking::TrayMethods;
        if self.tray.is_some() {
            return true;
        }
        let started = Instant::now();
        let spawned = crate::tray::KyxenTray::new(self.ui.clone()).spawn();
        tracing::debug!("tray registration took {:?}", started.elapsed());
        match spawned {
            Ok(handle) => {
                self.tray = Some(handle);
                self.sync_tray();
                true
            }
            Err(e) => {
                tracing::warn!("no tray icon: {e}");
                false
            }
        }
    }

    fn stop_tray(&mut self) {
        if let Some(tray) = self.tray.take() {
            let _ = tray.shutdown();
        }
    }

    pub fn has_tray(&self) -> bool {
        self.tray.is_some()
    }

    pub fn tray_wanted(&self) -> bool {
        self.global.tray
    }

    fn sync_tray(&self) {
        let Some(tray) = &self.tray else { return };
        let profiles: Vec<(String, String)> = self
            .profiles
            .iter()
            .map(|p| (p.name.clone(), p.display_name().to_owned()))
            .collect();
        let (active, keyboard) = (self.cur, self.keyboard.clone());
        let ui = self.ui();
        let tr = ui.global::<Tr>();
        let texts = crate::tray::TrayTexts {
            profile: tr.get_tray_profile().into(),
            open: tr.get_tray_open().into(),
            quit: tr.get_tray_quit().into(),
            no_keyboard: tr.get_tray_no_keyboard().into(),
        };
        tray.update(move |t| {
            t.profiles = profiles;
            t.active = active;
            t.keyboard = keyboard;
            t.texts = texts;
        });
    }

    fn ui(&self) -> AppWindow {
        self.ui.upgrade().expect("window alive")
    }

    fn profile(&self) -> &Profile {
        &self.profiles[self.cur]
    }

    fn profile_mut(&mut self) -> &mut Profile {
        &mut self.profiles[self.cur]
    }

    fn action_mut(&mut self) -> &mut MacroAction {
        let n = self.gkey;
        let p = self.profile_mut();
        match n {
            1 => &mut p.g1,
            2 => &mut p.g2,
            3 => &mut p.g3,
            4 => &mut p.g4,
            _ => &mut p.g5,
        }
    }

    fn action(&self) -> MacroAction {
        self.profile().gkey(self.gkey).cloned().unwrap_or_default()
    }

    fn lighting_mut(&mut self) -> &mut LightingConfig {
        &mut self.profile_mut().lighting
    }

    // ── keyboard model ──────────────────────────────────────────────────

    /// ISO or ANSI key map, from the "key shape" setting or the layout.
    fn keymap(&self) -> &'static Keymap {
        Keymap::g815_for(Form::resolve(
            self.global.keyboard_form.as_deref(),
            &self.layout,
        ))
    }

    fn build_keys(&mut self) {
        let map = self.keymap();
        self.key_x = map
            .keys
            .iter()
            .filter_map(|k| Some((k.led?, map.centre(k.name).0)))
            .collect();
        let x0 = map.keys.iter().map(|k| k.x).fold(f32::INFINITY, f32::min);
        let y0 = map.keys.iter().map(|k| k.y).fold(f32::INFINITY, f32::min);
        let caps: Vec<KeyCap> = map
            .keys
            .iter()
            .map(|k| {
                let (label, sub) = labels::legend(k.name, &self.layout);
                KeyCap {
                    name: k.name.into(),
                    label: label.into(),
                    sub: sub.into(),
                    x: k.x - x0,
                    y: k.y - y0,
                    w: k.w,
                    h: k.h,
                    color: Color::from_rgb_u8(0, 0, 0),
                    lit: k.led.is_some(),
                    gkey: k
                        .name
                        .strip_prefix('G')
                        .and_then(|n| n.parse().ok())
                        .filter(|n| (1..=5).contains(n))
                        .unwrap_or(0),
                }
            })
            .collect();
        let w = map.keys.iter().map(|k| k.x + k.w - x0).fold(0.0, f32::max);
        let h = map.keys.iter().map(|k| k.y + k.h - y0).fold(0.0, f32::max);
        self.keys.set_vec(caps);
        if let Some(ui) = self.ui.upgrade() {
            ui.global::<Ui>().set_kb_w(w);
            ui.global::<Ui>().set_kb_h(h);
        }
    }

    pub fn key_at(&self, x: f32, y: f32) -> i32 {
        self.keys
            .iter()
            .position(|k| x >= k.x && x < k.x + k.w && y >= k.y && y < k.y + k.h)
            .map_or(-1, |i| i as i32)
    }

    /// Recolour the on-screen keyboard (called ~30×/s).
    pub fn tick(&mut self) {
        // nothing to draw while the window sits in the tray
        if !self.ui.upgrade().is_some_and(|w| w.window().is_visible()) {
            return;
        }
        self.tick = self.tick.wrapping_add(1);
        let t = self.effect_started.elapsed().as_secs_f64();
        let cfg = self.profile().lighting.clone();
        let map = self.keymap();
        let frame: BTreeMap<u8, Rgb> = effects::frame(&cfg, map, t).into_iter().collect();
        let (mut sr, mut sg, mut sb, mut n) = (0u32, 0u32, 0u32, 0u32);
        for i in 0..self.keys.row_count() {
            let Some(mut cap) = self.keys.row_data(i) else {
                continue;
            };
            let led = map.get(cap.name.as_str()).and_then(|k| k.led);
            let rgb = led.and_then(|l| frame.get(&l).copied()).unwrap_or_default();
            let c = rgb_to_slint(rgb);
            if cap.color != c {
                cap.color = c;
                self.keys.set_row_data(i, cap);
            }
            if led.is_some() {
                sr += u32::from(rgb.0);
                sg += u32::from(rgb.1);
                sb += u32::from(rgb.2);
                n += 1;
            }
        }
        if let (Some(ui), true) = (self.ui.upgrade(), n > 0) {
            ui.global::<Ui>().set_glow(Color::from_rgb_u8(
                (sr / n) as u8,
                (sg / n) as u8,
                (sb / n) as u8,
            ));
        }
        if self.tick.is_multiple_of(3) {
            self.update_thumbs(t);
        }
    }

    fn update_thumbs(&self, t: f64) {
        let base = &self.profile().lighting;
        for i in 0..6 {
            let cfg = with_effect(base, i);
            let mut cols = vec![(0u32, 0u32, 0u32, 0u32); THUMB_COLUMNS];
            for (led, Rgb(r, g, b)) in effects::frame(&cfg, self.keymap(), t) {
                let x = self.key_x.get(&led).copied().unwrap_or(0.5);
                let c = ((x * THUMB_COLUMNS as f32) as usize).min(THUMB_COLUMNS - 1);
                cols[c].0 += u32::from(r);
                cols[c].1 += u32::from(g);
                cols[c].2 += u32::from(b);
                cols[c].3 += 1;
            }
            let colors: Vec<Color> = cols
                .iter()
                .map(|&(r, g, b, n)| {
                    let n = n.max(1);
                    Color::from_rgb_u8((r / n) as u8, (g / n) as u8, (b / n) as u8)
                })
                .collect();
            self.thumbs.set_row_data(
                i as usize,
                Thumb {
                    colors: ModelRc::from(Rc::new(VecModel::from(colors))),
                },
            );
        }
    }

    // ── pushing state to the UI ─────────────────────────────────────────

    fn push_all(&mut self) {
        self.push_profiles();
        self.push_macro();
        self.push_lighting();
        self.push_settings();
    }

    fn push_profiles(&self) {
        let ui = self.ui();
        let items: Vec<ProfileItem> = self
            .profiles
            .iter()
            .map(|p| ProfileItem {
                id: p.name.clone().into(),
                name: p.display_name().into(),
                color: rgb_to_slint(p.tray_colour.0),
                slot: self.global.slot_of(&p.name).map_or(0, i32::from),
            })
            .collect();
        let g = ui.global::<Ui>();
        g.set_profiles(ModelRc::from(Rc::new(VecModel::from(items))));
        g.set_current_profile(self.cur as i32);
        g.set_layout_name(self.layout.to_uppercase().into());
        self.sync_tray();
    }

    fn summary(&self, a: &MacroAction) -> String {
        let ui = self.ui();
        let tr = ui.global::<Tr>();
        match a.action {
            ActionKind::Type => {
                let t: String = a.text.chars().take(28).collect();
                if t.is_empty() {
                    tr.get_no_text().into()
                } else {
                    format!("„{t}“")
                }
            }
            ActionKind::Combo | ActionKind::HoldToggle => {
                if a.keys.is_empty() {
                    tr.get_no_keys().into()
                } else {
                    a.keys
                        .iter()
                        .map(|k| labels::chip(k, &self.layout))
                        .collect::<Vec<_>>()
                        .join(" + ")
                }
            }
            ActionKind::Command => {
                if a.cmd.is_empty() {
                    tr.get_no_command().into()
                } else {
                    a.cmd.clone()
                }
            }
            ActionKind::MouseButton => {
                let pick = |names: ModelRc<SharedString>, i: Option<usize>| {
                    names.row_data(i.unwrap_or(0)).unwrap_or_default()
                };
                let b = pick(
                    tr.get_mouse_buttons(),
                    MOUSE_BUTTONS.iter().position(|m| *m == a.mouse_btn),
                );
                let m = pick(
                    tr.get_mouse_modes(),
                    MOUSE_MODES.iter().position(|m| *m == a.mouse_mode),
                );
                format!("{b} · {m}")
            }
            ActionKind::None | ActionKind::Unknown => tr.get_unassigned().into(),
        }
    }

    fn push_macro(&self) {
        let ui = self.ui();
        let g = ui.global::<Ui>();
        let infos: Vec<GKeyInfo> = (1..=5u8)
            .map(|n| {
                let a = self.profile().gkey(n).cloned().unwrap_or_default();
                GKeyInfo {
                    n: i32::from(n),
                    kind: kind_index(a.action),
                    summary: self.summary(&a).into(),
                }
            })
            .collect();
        g.set_gkeys(ModelRc::from(Rc::new(VecModel::from(infos))));
        let a = self.action();
        g.set_sel_gkey(i32::from(self.gkey));
        g.set_m_kind(kind_index(a.action));
        if g.get_m_text() != a.text.as_str() {
            g.set_m_text(a.text.clone().into());
        }
        if g.get_m_cmd() != a.cmd.as_str() {
            g.set_m_cmd(a.cmd.clone().into());
        }
        let chips: Vec<SharedString> = a
            .keys
            .iter()
            .map(|k| labels::chip(k, &self.layout).into())
            .collect();
        let shown: Vec<SharedString> = match &self.recording {
            Some(r) => r
                .captured
                .iter()
                .map(|k| labels::chip(k, &self.layout).into())
                .collect(),
            None => chips,
        };
        g.set_m_keys(ModelRc::from(Rc::new(VecModel::from(shown))));
        g.set_m_repeat(a.repeat);
        g.set_m_rate(rate_to_slider(a.repeat_rate));
        g.set_m_rate_text(format!("{} ms", a.repeat_rate.round()).into());
        g.set_m_mouse_btn(
            MOUSE_BUTTONS
                .iter()
                .position(|m| *m == a.mouse_btn)
                .unwrap_or(0) as i32,
        );
        g.set_m_mouse_mode(
            MOUSE_MODES
                .iter()
                .position(|m| *m == a.mouse_mode)
                .unwrap_or(0) as i32,
        );
        g.set_recording(self.recording.is_some());
    }

    fn picked_colour(&self) -> Rgb {
        let l = &self.profile().lighting;
        if effect_index(l) == 0 {
            self.brush
        } else {
            l.preset
                .as_ref()
                .and_then(|p| p.colours.get(self.stop))
                .map_or(self.brush, |c| c.0)
        }
    }

    #[allow(clippy::many_single_char_names)]
    fn push_picker(&self) {
        let ui = self.ui();
        let g = ui.global::<Ui>();
        let c = self.picked_colour();
        let (h, s, v) = to_hsv(c);
        // keep the hue when the colour is grey/black
        if s > 0.001 && v > 0.001 {
            g.set_pick_h(h);
        }
        if v > 0.001 {
            g.set_pick_s(s);
        }
        g.set_pick_v(v);
        g.set_pick_hex(Colour(c).to_string().into());
        g.set_brush(rgb_to_slint(self.brush));
    }

    fn push_lighting(&self) {
        let ui = self.ui();
        let g = ui.global::<Ui>();
        let l = &self.profile().lighting;
        let effect = effect_index(l);
        g.set_l_effect(effect);
        let preset = l.preset.clone().unwrap_or_default();
        g.set_l_speed(speed_to_slider(preset.speed));
        g.set_l_speed_text(format!("{:.2}×", preset.speed).into());
        g.set_l_brightness(l.brightness as f32);
        g.set_l_direction(
            DIRECTIONS
                .iter()
                .position(|d| *d == preset.direction)
                .unwrap_or(0) as i32,
        );
        let stops: Vec<Color> = match effect {
            1 => preset
                .colours
                .iter()
                .take(1)
                .map(|c| rgb_to_slint(c.0))
                .collect(),
            2 => preset
                .colours
                .iter()
                .take(2)
                .map(|c| rgb_to_slint(c.0))
                .collect(),
            _ => Vec::new(),
        };
        g.set_l_stops(ModelRc::from(Rc::new(VecModel::from(stops))));
        g.set_l_stop(self.stop as i32);
        self.push_picker();
    }

    fn push_settings(&self) {
        let ui = self.ui();
        let g = ui.global::<Ui>();
        let p = self.profile();
        if g.get_p_name() != p.display_name() {
            g.set_p_name(p.display_name().into());
        }
        g.set_p_slot(self.global.slot_of(&p.name).map_or(0, i32::from));
        let choice = |options: &[Option<&str>], value: Option<&String>| {
            options
                .iter()
                .position(|o| *o == value.map(String::as_str))
                .unwrap_or(0) as i32
        };
        g.set_layout_choice(choice(&LAYOUTS, self.global.keyboard_layout.as_ref()));
        g.set_form_choice(choice(&FORMS, self.global.keyboard_form.as_ref()));
        g.set_language_choice(choice(&LANGUAGES, self.global.language.as_ref()));
        g.set_tray_enabled(self.global.tray);
        g.set_tray_autostart(kyxen_core::desktop::tray_autostart());
    }

    // ── saving ──────────────────────────────────────────────────────────

    fn mark_dirty(this: &Shared, global_too: bool) {
        let a = this.borrow();
        if a.read_only {
            return;
        }
        let ui = a.ui();
        ui.global::<Ui>()
            .set_save_state(ui.global::<Tr>().get_saving());
        let weak = Rc::downgrade(this);
        a.save_timer.start(
            TimerMode::SingleShot,
            Duration::from_millis(350),
            move || {
                if let Some(app) = weak.upgrade() {
                    app.borrow().save(global_too);
                }
            },
        );
    }

    fn save(&self, global_too: bool) {
        let mut ok = self
            .store
            .save_profile(self.profile())
            .inspect_err(|e| tracing::error!("{e}"))
            .is_ok();
        if global_too {
            ok &= self
                .store
                .save_global(&self.global)
                .inspect_err(|e| tracing::error!("{e}"))
                .is_ok();
        }
        let ui = self.ui();
        let tr = ui.global::<Tr>();
        ui.global::<Ui>().set_save_state(if ok {
            tr.get_saved()
        } else {
            tr.get_save_failed()
        });
        send_daemon(Request::Reload);
    }

    fn flush(&self) {
        if self.save_timer.running() {
            self.save_timer.stop();
            self.save(true);
        }
    }

    fn toast(&self, text: &str) {
        let ui = self.ui();
        ui.global::<Ui>().set_toast(text.into());
        let weak = self.ui.clone();
        Timer::single_shot(Duration::from_secs(3), move || {
            if let Some(ui) = weak.upgrade() {
                ui.global::<Ui>().set_toast("".into());
            }
        });
    }

    // ── daemon status ───────────────────────────────────────────────────

    pub fn on_status(&mut self, status: Option<Status>) {
        let ui = self.ui();
        let g = ui.global::<Ui>();
        g.set_connected(status.is_some());
        let keyboard = status.as_ref().and_then(|s| s.keyboard.clone());
        if keyboard != self.keyboard {
            self.keyboard = keyboard;
            self.sync_tray();
        }
        g.set_keyboard_name(
            status
                .as_ref()
                .and_then(|s| s.keyboard.clone())
                .unwrap_or_default()
                .into(),
        );
        // Follow profile switches made with M1–M3 on the keyboard. Only a
        // change on the daemon's side counts: a status that was requested
        // before a click in the UI arrives with the old profile, and comparing
        // it with our selection would switch the UI back.
        let Some(s) = status else { return };
        // Statuses requested around our own changes may still show the old
        // profile; look again once the daemon has caught up.
        let settling = self.save_timer.running()
            || self
                .local_change
                .is_some_and(|t| t.elapsed() < Duration::from_secs(2));
        if settling || self.daemon_profile.as_deref() == Some(s.active_profile.as_str()) {
            return;
        }
        self.daemon_profile = Some(s.active_profile.clone());
        if let Some(i) = self
            .profiles
            .iter()
            .position(|p| p.name == s.active_profile)
            && i != self.cur
        {
            tracing::debug!(
                profile = s.active_profile,
                "following a profile switch on the keyboard"
            );
            self.cur = i;
            self.global.active_profile.clone_from(&s.active_profile);
            self.stop = 0;
            self.effect_started = Instant::now();
            self.push_all();
        }
    }
}

/// Map a Slint key event text to a combo key name.
fn key_name(text: &str, layout: &TextLayout) -> Option<String> {
    let mut chars = text.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    let named = match c {
        '\u{10}' | '\u{15}' => "shift",
        '\u{11}' | '\u{16}' => "ctrl",
        '\u{12}' => "alt",
        '\u{13}' => "altgr",
        '\u{17}' | '\u{18}' => "super",
        '\u{08}' => "backspace",
        '\t' | '\u{19}' => "tab",
        '\n' | '\r' => "enter",
        '\u{7f}' => "delete",
        ' ' => "space",
        '\u{F700}' => "up",
        '\u{F701}' => "down",
        '\u{F702}' => "left",
        '\u{F703}' => "right",
        '\u{F727}' => "insert",
        '\u{F729}' => "home",
        '\u{F72B}' => "end",
        '\u{F72C}' => "pageup",
        '\u{F72D}' => "pagedown",
        '\u{F730}' => "pause",
        '\u{F731}' => "printscreen",
        '\u{F735}' => "menu",
        c @ '\u{F704}'..='\u{F71B}' => return Some(format!("f{}", c as u32 - 0xF704 + 1)),
        c => {
            // Shifted characters (e.g. '!' on Shift+1) → the key's plain character.
            let lower = c.to_lowercase().next().unwrap_or(c);
            let key = layout.lookup(lower).map(|(k, _)| k);
            return key
                .and_then(|k| {
                    layout
                        .chars()
                        .find(|(_, kk, m)| *kk == k && m.is_none())
                        .map(|(ch, _, _)| ch)
                })
                .or(Some(lower))
                .map(|ch| ch.to_string());
        }
    };
    Some(named.to_owned())
}

/// Wire all UI callbacks to the shared app state.
#[allow(clippy::too_many_lines)]
pub fn connect(ui: &AppWindow, app: &Shared) {
    let g = ui.global::<Ui>();

    macro_rules! with {
        ($app:ident, |$a:ident $(, $arg:ident : $ty:ty)*| $body:block) => {{
            let weak = Rc::downgrade($app);
            move |$($arg: $ty),*| {
                if let Some(rc) = weak.upgrade() {
                    #[allow(unused_mut)]
                    let mut $a = rc.borrow_mut();
                    let _ = &rc;
                    $body
                }
            }
        }};
    }

    // ── profiles ────────────────────────────────────────────────────────
    g.on_select_profile(with!(app, |a, i: i32| {
        a.flush();
        a.local_change = Some(Instant::now());
        a.cur = (i as usize).min(a.profiles.len() - 1);
        let name = a.profile().name.clone();
        a.global.active_profile.clone_from(&name);
        a.stop = 0;
        a.effect_started = Instant::now();
        a.push_all();
        send_daemon(Request::Switch { profile: name });
    }));

    {
        let weak = Rc::downgrade(app);
        g.on_new_profile(move || {
            let Some(rc) = weak.upgrade() else { return };
            {
                let mut a = rc.borrow_mut();
                a.flush();
                a.local_change = Some(Instant::now());
                let mut n = a.profiles.len() + 1;
                while a.profiles.iter().any(|p| p.name == format!("profile-{n}")) {
                    n += 1;
                }
                let mut p = Profile::new(&format!("profile-{n}"));
                p.display_name = a.ui().global::<Tr>().invoke_new_profile(n as i32).into();
                p.tray_colour = PALETTE[n % PALETTE.len()].parse().unwrap_or_default();
                p.lighting.base_colour = p.tray_colour;
                a.profiles.push(p);
                a.cur = a.profiles.len() - 1;
                a.gkey = 1;
                a.push_all();
                a.ui().global::<Ui>().set_tab(2);
            }
            App::mark_dirty(&rc, false);
        });
    }

    // ── keyboard ────────────────────────────────────────────────────────
    {
        let weak = Rc::downgrade(app);
        g.on_key_at(move |x, y| weak.upgrade().map_or(-1, |rc| rc.borrow().key_at(x, y)));
    }

    let paint = |rc: &Shared, i: i32, drag: bool| {
        let changed = {
            let mut a = rc.borrow_mut();
            let Some(cap) = a.keys.row_data(i as usize) else {
                return;
            };
            let ui = a.ui();
            let tab = ui.global::<Ui>().get_tab();
            if tab == 0 {
                if cap.gkey > 0 && !drag {
                    a.flush();
                    a.gkey = cap.gkey as u8;
                    a.recording = None;
                    a.push_macro();
                }
                return;
            }
            if tab != 1 || effect_index(&a.profile().lighting) != 0 || !cap.lit {
                return;
            }
            let name = cap.name.to_string();
            let brush = a.brush;
            match ui.global::<Ui>().get_tool() {
                0 => {
                    let l = a.lighting_mut();
                    if l.keys.get(&name).map(|c| c.0) == Some(brush) {
                        return;
                    }
                    l.keys.insert(name, Colour(brush));
                }
                1 if !drag => {
                    let l = a.lighting_mut();
                    l.base_colour = Colour(brush);
                    l.keys.clear();
                }
                2 if !drag => {
                    a.brush = slint_to_rgb(cap.color);
                    ui.global::<Ui>().set_tool(0);
                    a.push_picker();
                    return;
                }
                3 => {
                    if a.lighting_mut().keys.remove(&name).is_none() {
                        return;
                    }
                }
                _ => return,
            }
            true
        };
        if changed {
            App::mark_dirty(rc, false);
        }
    };
    {
        let weak = Rc::downgrade(app);
        g.on_key_clicked(move |i| {
            if let Some(rc) = weak.upgrade() {
                paint(&rc, i, false);
            }
        });
    }
    {
        let weak = Rc::downgrade(app);
        g.on_key_dragged(move |i| {
            if let Some(rc) = weak.upgrade() {
                paint(&rc, i, true);
            }
        });
    }

    // ── macros ──────────────────────────────────────────────────────────
    g.on_select_gkey(with!(app, |a, n: i32| {
        a.flush();
        a.gkey = n.clamp(1, 5) as u8;
        a.recording = None;
        a.push_macro();
    }));

    macro_rules! edit_macro {
        ($name:ident, |$a:ident $(, $arg:ident : $ty:ty)*| $body:block) => {{
            let weak = Rc::downgrade(app);
            g.$name(move |$($arg: $ty),*| {
                let Some(rc) = weak.upgrade() else { return };
                {
                    let mut $a = rc.borrow_mut();
                    $body
                    $a.push_macro();
                }
                App::mark_dirty(&rc, false);
            });
        }};
    }

    edit_macro!(on_set_kind, |a, i: i32| {
        a.recording = None;
        a.action_mut().action = kind_from(i);
    });
    edit_macro!(on_set_text, |a, t: SharedString| {
        a.action_mut().text = t.to_string();
    });
    edit_macro!(on_set_cmd, |a, t: SharedString| {
        a.action_mut().cmd = t.to_string();
    });
    edit_macro!(on_clear_keys, |a| {
        a.action_mut().keys.clear();
    });
    edit_macro!(on_set_repeat, |a, on: bool| {
        a.action_mut().repeat = on;
    });
    edit_macro!(on_set_rate, |a, v: f32| {
        a.action_mut().repeat_rate = slider_to_rate(v);
    });
    edit_macro!(on_set_mouse, |a, b: i32, m: i32| {
        let act = a.action_mut();
        act.mouse_btn = MOUSE_BUTTONS[(b as usize).min(4)];
        act.mouse_mode = MOUSE_MODES[(m as usize).min(2)];
    });

    g.on_start_record(with!(app, |a| {
        a.recording = if a.recording.is_some() {
            None
        } else {
            Some(Recording::default())
        };
        a.push_macro();
    }));

    {
        let weak = Rc::downgrade(app);
        g.on_record_key(move |text, pressed| {
            let Some(rc) = weak.upgrade() else { return };
            let finished = {
                let mut a = rc.borrow_mut();
                if text.as_str() == "\u{1b}" {
                    a.recording = None;
                    a.push_macro();
                    return;
                }
                let Some(name) = key_name(text.as_str(), &a.text_layout) else {
                    return;
                };
                let Some(rec) = a.recording.as_mut() else {
                    return;
                };
                if pressed {
                    if !rec.held.contains(&name) {
                        rec.held.push(name.clone());
                    }
                    if !rec.captured.contains(&name) {
                        rec.captured.push(name);
                    }
                    a.push_macro();
                    false
                } else {
                    rec.held.retain(|k| *k != name);
                    if rec.held.is_empty() && !rec.captured.is_empty() {
                        let keys = std::mem::take(&mut rec.captured);
                        a.recording = None;
                        a.action_mut().keys = keys;
                        a.push_macro();
                        true
                    } else {
                        false
                    }
                }
            };
            if finished {
                App::mark_dirty(&rc, false);
            }
        });
    }

    g.on_test_macro(with!(app, |a| {
        a.flush();
        let n = a.gkey;
        a.toast(&a.ui().global::<Tr>().invoke_triggered(i32::from(n)));
        // give the daemon a moment to reload the just-saved profile
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            ipc::command(&Request::Run { gkey: n });
        });
    }));

    // ── lighting ────────────────────────────────────────────────────────
    macro_rules! edit_light {
        ($name:ident, |$a:ident $(, $arg:ident : $ty:ty)*| $body:block) => {{
            let weak = Rc::downgrade(app);
            g.$name(move |$($arg: $ty),*| {
                let Some(rc) = weak.upgrade() else { return };
                {
                    let mut $a = rc.borrow_mut();
                    $body
                    $a.push_lighting();
                }
                App::mark_dirty(&rc, false);
            });
        }};
    }

    edit_light!(on_set_effect, |a, i: i32| {
        let l = with_effect(&a.profile().lighting, i);
        a.profile_mut().lighting = l;
        a.stop = 0;
        a.effect_started = Instant::now();
    });
    edit_light!(on_set_speed, |a, v: f32| {
        if let Some(p) = a.lighting_mut().preset.as_mut() {
            p.speed = slider_to_speed(v);
        }
    });
    edit_light!(on_set_brightness, |a, v: f32| {
        a.lighting_mut().brightness = (f64::from(v) * 100.0).round() / 100.0;
    });
    edit_light!(on_set_direction, |a, i: i32| {
        if let Some(p) = a.lighting_mut().preset.as_mut() {
            p.direction = DIRECTIONS[(i as usize).min(4)];
        }
    });
    g.on_select_stop(with!(app, |a, i: i32| {
        a.stop = i.max(0) as usize;
        a.push_lighting();
    }));

    let apply_colour = |rc: &Shared, c: Rgb| {
        let dirty = {
            let mut a = rc.borrow_mut();
            let stop = a.stop;
            if effect_index(&a.profile().lighting) == 0 {
                a.brush = c;
                a.push_picker();
                false
            } else {
                if let Some(slot) = a
                    .lighting_mut()
                    .preset
                    .as_mut()
                    .and_then(|p| p.colours.get_mut(stop))
                {
                    *slot = Colour(c);
                }
                a.push_lighting();
                true
            }
        };
        if dirty {
            App::mark_dirty(rc, false);
        }
    };
    {
        let weak = Rc::downgrade(app);
        g.on_colour_changed(move |h, s, v| {
            if let Some(rc) = weak.upgrade() {
                apply_colour(&rc, from_hsv(h, s, v));
            }
        });
    }
    {
        let weak = Rc::downgrade(app);
        g.on_set_hex(move |t| {
            if let (Some(rc), Ok(c)) = (weak.upgrade(), t.parse::<Colour>()) {
                apply_colour(&rc, c.0);
            }
        });
    }
    {
        let weak = Rc::downgrade(app);
        g.on_set_colour(move |c| {
            if let Some(rc) = weak.upgrade() {
                apply_colour(&rc, slint_to_rgb(c));
            }
        });
    }
    edit_light!(on_fill_all, |a| {
        let brush = a.brush;
        let l = a.lighting_mut();
        l.base_colour = Colour(brush);
        l.keys.clear();
    });
    edit_light!(on_clear_paint, |a| {
        a.lighting_mut().keys.clear();
    });

    // ── settings ────────────────────────────────────────────────────────
    {
        let weak = Rc::downgrade(app);
        g.on_rename(move |t| {
            let Some(rc) = weak.upgrade() else { return };
            {
                let mut a = rc.borrow_mut();
                t.trim().clone_into(&mut a.profile_mut().display_name);
                a.push_profiles();
            }
            App::mark_dirty(&rc, false);
        });
    }
    {
        let weak = Rc::downgrade(app);
        g.on_set_profile_color(move |c| {
            let Some(rc) = weak.upgrade() else { return };
            {
                let mut a = rc.borrow_mut();
                a.profile_mut().tray_colour = Colour(slint_to_rgb(c));
                a.push_profiles();
            }
            App::mark_dirty(&rc, false);
        });
    }
    {
        let weak = Rc::downgrade(app);
        g.on_set_slot(move |s| {
            let Some(rc) = weak.upgrade() else { return };
            {
                let mut a = rc.borrow_mut();
                let name = a.profile().name.clone();
                a.global.profile_slots.retain(|_, p| *p != name);
                if (1..=3).contains(&s) {
                    a.global.profile_slots.insert(format!("m{s}"), name);
                }
                a.push_profiles();
                a.push_settings();
            }
            App::mark_dirty(&rc, true);
        });
    }
    {
        let weak = Rc::downgrade(app);
        g.on_delete_profile(move || {
            let Some(rc) = weak.upgrade() else { return };
            let mut a = rc.borrow_mut();
            if a.profiles.len() <= 1 || a.read_only {
                return;
            }
            a.save_timer.stop();
            a.local_change = Some(Instant::now());
            let cur = a.cur;
            let removed = a.profiles.remove(cur);
            let path = a
                .store
                .profiles_dir()
                .join(format!("{}.toml", removed.name));
            if let Err(e) = std::fs::remove_file(&path) {
                tracing::warn!("could not delete {}: {e}", path.display());
            }
            a.global.profile_slots.retain(|_, p| *p != removed.name);
            a.cur = 0;
            let first = a.profile().name.clone();
            a.global.active_profile.clone_from(&first);
            if let Err(e) = a.store.save_global(&a.global) {
                tracing::warn!("{e}");
            }
            a.push_all();
            a.toast(
                &a.ui()
                    .global::<Tr>()
                    .invoke_deleted(removed.display_name().into()),
            );
            send_daemon(Request::Reload);
        });
    }
    {
        let weak = Rc::downgrade(app);
        g.on_set_tray(move |on| {
            let Some(rc) = weak.upgrade() else { return };
            {
                let mut a = rc.borrow_mut();
                if a.read_only {
                    return;
                }
                a.global.tray = on;
                if on {
                    if !a.spawn_tray() {
                        a.toast(&a.ui().global::<Tr>().get_no_tray());
                    }
                } else {
                    a.stop_tray();
                    if let Err(e) = kyxen_core::desktop::set_tray_autostart(Path::new(""), false) {
                        tracing::warn!("could not remove autostart entry: {e}");
                    }
                }
                a.push_settings();
            }
            App::mark_dirty(&rc, true);
        });
    }
    g.on_set_autostart(with!(app, |a, on: bool| {
        if a.read_only {
            return;
        }
        let exe = std::env::current_exe().unwrap_or_default();
        if let Err(e) = kyxen_core::desktop::set_tray_autostart(&exe, on) {
            a.toast(&format!("Autostart: {e}"));
        }
        a.push_settings();
    }));
    {
        let weak = Rc::downgrade(app);
        g.on_set_layout(move |i| {
            let Some(rc) = weak.upgrade() else { return };
            {
                let mut a = rc.borrow_mut();
                let code = LAYOUTS.get(i as usize).copied().flatten();
                a.global.keyboard_layout = code.map(str::to_owned);
                a.global.keyboard_variant = None;
                let (l, v) = layout::detect(a.global.keyboard_layout.as_deref(), None);
                a.text_layout = TextLayout::new(&l, &v);
                a.layout = l;
                a.build_keys();
                a.push_all();
            }
            App::mark_dirty(&rc, true);
        });
    }
    {
        let weak = Rc::downgrade(app);
        g.on_set_form(move |i| {
            let Some(rc) = weak.upgrade() else { return };
            {
                let mut a = rc.borrow_mut();
                a.global.keyboard_form =
                    FORMS.get(i as usize).copied().flatten().map(str::to_owned);
                a.build_keys();
                a.push_settings();
            }
            App::mark_dirty(&rc, true);
        });
    }
    {
        let weak = Rc::downgrade(app);
        g.on_set_language(move |i| {
            let Some(rc) = weak.upgrade() else { return };
            {
                let mut a = rc.borrow_mut();
                a.global.language = LANGUAGES
                    .get(i as usize)
                    .copied()
                    .flatten()
                    .map(str::to_owned);
                apply_language(a.global.language.as_deref());
                // texts built in Rust: summaries, tray menu
                a.push_all();
            }
            App::mark_dirty(&rc, true);
        });
    }
}

/// Poll the daemon in the background and feed results to the UI thread.
pub fn start_status_polling(app: &Shared) -> Timer {
    let (tx, rx) = mpsc::channel::<Option<Status>>();
    thread::spawn(move || {
        loop {
            if tx.send(ipc::status()).is_err() {
                return;
            }
            thread::sleep(Duration::from_millis(1500));
        }
    });
    let timer = Timer::default();
    let weak = Rc::downgrade(app);
    timer.start(TimerMode::Repeated, Duration::from_millis(400), move || {
        let Some(rc) = weak.upgrade() else { return };
        if let Some(status) = rx.try_iter().last() {
            rc.borrow_mut().on_status(status);
        }
    });
    timer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bursts_keep_the_last_switch_in_order() {
        let sw = |p: &str| Request::Switch { profile: p.into() };
        let out = coalesce(vec![
            sw("a"),
            Request::Reload,
            sw("b"),
            Request::Run { gkey: 1 },
            Request::Reload,
            sw("a"),
        ]);
        let names: Vec<String> = out
            .iter()
            .map(|r| match r {
                Request::Switch { profile } => format!("switch {profile}"),
                Request::Reload => "reload".into(),
                Request::Run { gkey } => format!("run {gkey}"),
                Request::Status => "status".into(),
            })
            .collect();
        assert_eq!(names, ["run 1", "reload", "switch a"]);
    }

    #[test]
    fn slider_mappings_round_trip() {
        assert!((slider_to_rate(rate_to_slider(100.0)) - 100.0).abs() < 1.0);
        assert!((slider_to_speed(speed_to_slider(1.5)) - 1.5).abs() < 0.06);
        assert_eq!(slider_to_rate(0.0), 10.0);
    }

    #[test]
    fn recorded_keys_use_plain_characters() {
        let at = TextLayout::new("at", "");
        assert_eq!(key_name("\u{11}", &at).as_deref(), Some("ctrl"));
        assert_eq!(key_name("Z", &at).as_deref(), Some("z"));
        assert_eq!(
            key_name("!", &at).as_deref(),
            Some("1"),
            "Shift+1 records the 1 key"
        );
        assert_eq!(key_name("\u{F708}", &at).as_deref(), Some("f5"));
    }

    #[test]
    fn effects_keep_colours() {
        let base = LightingConfig {
            base_colour: "#123456".parse().unwrap(),
            ..Default::default()
        };
        let wave = with_effect(&base, 2);
        let p = wave.preset.unwrap();
        assert_eq!(p.name, PresetName::Wave);
        assert_eq!(p.colours.len(), 2);
        assert_eq!(p.colours[0], base.base_colour);
        assert_eq!(effect_index(&with_effect(&base, 5)), 5);
    }
}
