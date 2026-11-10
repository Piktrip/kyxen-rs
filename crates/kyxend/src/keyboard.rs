//! The thread that owns the keyboard's HID++ connection.
//!
//! One file descriptor carries everything: GKEY software control (which is
//! tied to the open fd and reverts when it closes), G/M-key notifications,
//! M-key LEDs and lighting frames.

use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use kyxen_core::config::LightingConfig;
use kyxen_core::effects::{self, Frame};
use kyxen_core::keymap::Keymap;
use kyxen_hidpp::hidraw::Hidraw;
use kyxen_hidpp::lighting::frame_reports;
use kyxen_hidpp::notify::{self, KeyEvent, KeyTracker};
use kyxen_hidpp::onboard::{Mode, OnboardProfiles};
use kyxen_hidpp::{FeatureId, HidppDevice, Transport};
use tracing::{debug, info, warn};

use crate::daemon::Event;
use crate::device::{GKeyMode, Keyboard};

const FRAME: Duration = Duration::from_millis(50);
const IDLE: Duration = Duration::from_millis(250);

const GKEY_FN_SOFTWARE_CONTROL: u8 = 2;

/// G Hub's lighting init on the G815, addressed by fixed feature index.
const G815_LIGHTING_INIT: [[u8; 4]; 2] = [[0x11, 0xff, 0x09, 0x0f], [0x11, 0xff, 0x03, 0x2e]];

#[derive(Debug)]
pub enum Command {
    /// Lighting and the key map (ISO / ANSI) to draw it on.
    Lighting(LightingConfig, &'static Keymap),
    /// Show M-key slot `n` (1-based) as active. The keyboard lights M-LEDs
    /// only for the active *onboard* profile (MKEYS `setLED` is ignored in
    /// onboard mode), so this activates that onboard profile and redraws.
    MKeyLed(Option<u8>),
    /// Restore the onboard profile that was active before we started, then stop.
    Shutdown,
}

/// Runtime feature indices.
#[derive(Debug, Clone, Copy)]
struct Features {
    gkey: Option<u8>,
    mkeys: Option<u8>,
    per_key: Option<u8>,
    onboard: Option<OnboardProfiles>,
    /// Onboard profile active before the daemon took over.
    initial_profile: u8,
}

pub fn spawn(kb: &Keyboard, events: Sender<Event>) -> Result<(Sender<Command>, JoinHandle<()>)> {
    let transport =
        Hidraw::open(&kb.hidpp).with_context(|| format!("open {}", kb.hidpp.display()))?;
    let mut dev = HidppDevice::new(transport).with_timeout(Duration::from_secs(2));
    let features = setup(&mut dev, kb)?;
    let (tx, rx) = crossbeam_channel::unbounded();
    let handle = thread::Builder::new().name("hidpp".into()).spawn(move || {
        let mut worker = Worker::new(dev, features, events.clone());
        if let Err(e) = worker.run(&rx) {
            warn!("keyboard connection lost: {e:#}");
            let _ = events.send(Event::KeyboardLost);
        }
    })?;
    Ok((tx, handle))
}

fn optional<T: Transport>(dev: &mut HidppDevice<T>, id: FeatureId) -> Option<u8> {
    dev.feature_index(id)
        .inspect_err(|e| warn!("feature 0x{:04x} unavailable: {e}", id.0))
        .ok()
}

fn setup<T: Transport>(dev: &mut HidppDevice<T>, kb: &Keyboard) -> Result<Features> {
    let onboard = OnboardProfiles::open(dev)
        .inspect_err(|e| warn!("onboard profiles unavailable: {e}"))
        .ok();
    let mut f = Features {
        gkey: optional(dev, FeatureId::GKEY),
        mkeys: optional(dev, FeatureId::MKEYS),
        per_key: optional(dev, FeatureId::PER_KEY_LIGHTING_V2),
        onboard,
        initial_profile: 1,
    };
    info!(gkey = ?f.gkey, mkeys = ?f.mkeys, per_key = ?f.per_key, onboard = ?f.onboard.map(|o| o.index), "HID++ features");

    if let Some(op) = f.onboard {
        if kb.model.gkeys == GKeyMode::BootProtocol && op.ensure_gkey_remap(dev)? {
            info!("waiting for the keyboard to settle after the flash write");
            thread::sleep(Duration::from_millis(1500));
        }
        if op.mode(dev)? == Mode::Onboard {
            f.initial_profile = op.current_profile(dev)?.max(1);
        } else {
            op.set_mode(dev, Mode::Onboard)?;
        }
    }
    if let Some(idx) = f.gkey {
        // Diverts G/M-keys to notifications on this fd.
        dev.request(idx, GKEY_FN_SOFTWARE_CONTROL, &[0x01])
            .context("enable G-key software control")?;
    }
    if kb.model.g815_lighting_init {
        for pkt in G815_LIGHTING_INIT {
            let mut report = pkt.to_vec();
            report.resize(20, 0);
            dev.send_raw(&report)?;
        }
    }
    Ok(f)
}

struct Worker<T> {
    dev: HidppDevice<T>,
    f: Features,
    events: Sender<Event>,
    gkeys: KeyTracker,
    mkeys: KeyTracker,
    lighting: Option<(LightingConfig, &'static Keymap)>,
    onboard_slot: Option<u8>,
    started: Instant,
    last_frame: Option<Frame>,
    next_frame: Instant,
}

impl<T: Transport> Worker<T> {
    fn new(dev: HidppDevice<T>, f: Features, events: Sender<Event>) -> Self {
        Self {
            dev,
            f,
            events,
            gkeys: KeyTracker::default(),
            mkeys: KeyTracker::default(),
            lighting: None,
            onboard_slot: None,
            started: Instant::now(),
            last_frame: None,
            next_frame: Instant::now(),
        }
    }

    fn run(&mut self, commands: &Receiver<Command>) -> Result<()> {
        loop {
            loop {
                match commands.try_recv() {
                    Ok(Command::Shutdown) | Err(TryRecvError::Disconnected) => {
                        return self.shutdown();
                    }
                    Ok(cmd) => self.handle(cmd),
                    Err(TryRecvError::Empty) => break,
                }
            }
            if Instant::now() >= self.next_frame {
                self.render()?;
            }
            let wait = self
                .next_frame
                .saturating_duration_since(Instant::now())
                .min(IDLE);
            if let Some(note) = self.dev.next_notification(wait)? {
                self.on_notification(&note);
            }
        }
    }

    fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::Lighting(cfg, map) => {
                self.lighting = Some((cfg, map));
                self.started = Instant::now();
                self.last_frame = None;
                self.next_frame = Instant::now();
            }
            Command::MKeyLed(slot) => self.set_mkey_led(slot),
            Command::Shutdown => unreachable!("handled in run"),
        }
    }

    fn on_notification(&mut self, note: &kyxen_hidpp::Notification) {
        if let Some(mask) = self.f.gkey.and_then(|i| notify::gkey_mask(note, i)) {
            for ev in self.gkeys.update(mask) {
                let (n, pressed) = match ev {
                    KeyEvent::Pressed(n) => (n, true),
                    KeyEvent::Released(n) => (n, false),
                };
                let _ = self.events.send(Event::GKey { n, pressed });
            }
        } else if let Some(mask) = self.f.mkeys.and_then(|i| notify::mkey_mask(note, i)) {
            for ev in self.mkeys.update(u16::from(mask)) {
                if let KeyEvent::Pressed(slot) = ev {
                    let _ = self.events.send(Event::MKey(slot));
                }
            }
        } else {
            debug!(?note, "unhandled notification");
        }
    }

    fn set_mkey_led(&mut self, slot: Option<u8>) {
        let (Some(op), Some(slot)) = (self.f.onboard, slot) else {
            return;
        };
        if self.onboard_slot == Some(slot) {
            return;
        }
        match op.set_current_profile(&mut self.dev, slot.clamp(1, 3)) {
            Ok(()) => {
                self.onboard_slot = Some(slot);
                // The onboard profile brings its own lighting; draw ours again.
                self.last_frame = None;
                self.next_frame = Instant::now();
            }
            Err(e) => warn!("could not activate onboard profile {slot} for the M-key LED: {e}"),
        }
    }

    fn render(&mut self) -> Result<()> {
        let (Some((cfg, map)), Some(idx)) = (&self.lighting, self.f.per_key) else {
            self.next_frame = Instant::now() + IDLE;
            return Ok(());
        };
        let animated = effects::is_animated(cfg);
        if animated || self.last_frame.is_none() {
            let frame = effects::frame(cfg, map, self.started.elapsed().as_secs_f64());
            let changed = effects::diff(self.last_frame.as_ref(), &frame);
            for report in frame_reports(idx, &changed) {
                self.dev.send_raw(&report)?;
            }
            self.last_frame = Some(frame);
        }
        self.next_frame = Instant::now() + if animated { FRAME } else { IDLE };
        Ok(())
    }

    fn shutdown(&mut self) -> Result<()> {
        let slot = self.f.initial_profile.clamp(1, 3);
        if let Some(op) = self.f.onboard {
            // Back to the stored profile so the keyboard lights up on its own.
            op.restore(&mut self.dev, slot)
                .context("restore onboard mode")?;
        }
        info!("keyboard handed back to onboard profile {slot}");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kyxen_hidpp::mock::MockKeyboard;

    fn worker(kb: MockKeyboard) -> (Worker<MockKeyboard>, Receiver<Event>) {
        let mut dev = HidppDevice::new(kb);
        let model = crate::device::MODELS[1];
        let kb = Keyboard {
            model,
            hidpp: "/dev/null".into(),
            boot_hidraw: None,
            boot_event: None,
        };
        let f = setup(&mut dev, &kb).unwrap();
        let (tx, rx) = crossbeam_channel::unbounded();
        (Worker::new(dev, f, tx), rx)
    }

    #[test]
    fn setup_enables_software_control_without_flash_writes() {
        let (mut w, _) = worker(MockKeyboard::g915());
        let sent = &w.dev.transport_mut().sent;
        assert!(
            sent.iter()
                .any(|r| r[2] == 0x10 && r[3] >> 4 == 2 && r[4] == 1),
            "GKEY software control"
        );
        assert!(w.dev.transport_mut().writes.is_empty());
    }

    #[test]
    fn gkey_notifications_become_events() {
        let (mut w, rx) = worker(MockKeyboard::g915());
        for mask in [0x01, 0x00, 0x10] {
            let mut r = vec![0x11, 0xff, 0x10, 0x00, mask];
            r.resize(20, 0);
            w.dev.transport_mut().push(r);
        }
        while let Some(n) = w.dev.next_notification(Duration::ZERO).unwrap() {
            w.on_notification(&n);
        }
        let got: Vec<_> = rx.try_iter().collect();
        assert!(matches!(
            got.as_slice(),
            [
                Event::GKey {
                    n: 1,
                    pressed: true
                },
                Event::GKey {
                    n: 1,
                    pressed: false
                },
                Event::GKey {
                    n: 5,
                    pressed: true
                }
            ]
        ));
    }

    #[test]
    fn static_lighting_is_sent_once() {
        let (mut w, _) = worker(MockKeyboard::g915());
        w.handle(Command::Lighting(LightingConfig::default(), Keymap::g815()));
        w.render().unwrap();
        let first = w.dev.transport_mut().sent.len();
        assert!(
            w.dev
                .transport_mut()
                .sent
                .iter()
                .any(|r| r[2] == 0x0a && r[3] == 0x7e),
            "frame committed"
        );
        w.render().unwrap();
        assert_eq!(
            w.dev.transport_mut().sent.len(),
            first,
            "unchanged static frame is not resent"
        );
    }

    #[test]
    fn shutdown_restores_mode_and_initial_profile() {
        let mut kb = MockKeyboard::g915();
        kb.current_profile = 2; // the user was on onboard profile 2
        let (mut w, _) = worker(kb);
        w.set_mkey_led(Some(3));
        w.dev.transport_mut().mode = 2; // something put it into host mode
        w.shutdown().unwrap();
        assert_eq!(w.dev.transport_mut().mode, 1);
        assert_eq!(w.dev.transport_mut().current_profile, 2);
    }

    #[test]
    fn mkey_led_activates_onboard_profile_and_redraws() {
        let (mut w, _) = worker(MockKeyboard::g915());
        w.handle(Command::Lighting(LightingConfig::default(), Keymap::g815()));
        w.render().unwrap();
        let before = w.dev.transport_mut().sent.len();
        w.set_mkey_led(Some(2));
        assert_eq!(w.dev.transport_mut().current_profile, 2);
        assert_eq!(w.dev.transport_mut().mode, 1, "stays in onboard mode");
        w.render().unwrap();
        let commits = w.dev.transport_mut().sent[before..]
            .iter()
            .filter(|r| r[2] == 0x0a && r[3] == 0x7e)
            .count();
        assert_eq!(
            commits, 1,
            "lighting redrawn after the onboard profile switch"
        );
        let n = w.dev.transport_mut().sent.len();
        w.set_mkey_led(Some(2));
        assert_eq!(
            w.dev.transport_mut().sent.len(),
            n,
            "same slot is not switched again"
        );
    }
}
