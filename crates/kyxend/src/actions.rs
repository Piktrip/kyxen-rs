//! Macro execution through virtual uinput devices.
//!
//! One worker thread runs every action in order, so a repeating "type text"
//! macro never interleaves with itself (Python Kyxen started a thread per
//! repetition and typed overlapping text).

use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode, RelativeAxisCode};
use kyxen_core::config::{ActionKind, MacroAction, MouseButton, MouseMode};
use kyxen_core::layout::TextLayout;
use kyxen_core::macros::{self, KeyStroke};
use tracing::{info, warn};

const KEY_DELAY: Duration = Duration::from_millis(18);
const MOD_DELAY: Duration = Duration::from_millis(8);
const MIN_REPEAT: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub enum Job {
    Run(MacroAction),
    /// Toggle a hold action for G-key `gkey`: first call presses, second releases.
    ToggleHold {
        gkey: u8,
        action: MacroAction,
    },
    RepeatStart {
        gkey: u8,
        action: MacroAction,
    },
    RepeatStop {
        gkey: u8,
    },
    /// Release held toggles and stop repeats (profile switch).
    ReleaseAll,
    SetLayout(TextLayout),
    Shutdown,
}

/// Where key and mouse events go. A trait so tests can record them.
pub trait Output {
    fn keys(&mut self, strokes: &[KeyStroke]) -> Result<()>;
    fn mouse(&mut self, button: KeyCode, pressed: bool) -> Result<()>;
}

/// Virtual keyboard and mouse.
#[derive(Debug)]
pub struct Uinput {
    keyboard: VirtualDevice,
    mouse: VirtualDevice,
}

impl Uinput {
    pub fn new() -> Result<Self> {
        let mut keys = AttributeSet::<KeyCode>::new();
        for code in 1..=248 {
            keys.insert(KeyCode::new(code));
        }
        let keyboard = VirtualDevice::builder()
            .context("open /dev/uinput")?
            .name("Kyxen virtual keyboard")
            .with_keys(&keys)?
            .build()?;
        let mut buttons = AttributeSet::<KeyCode>::new();
        for b in [
            KeyCode::BTN_LEFT,
            KeyCode::BTN_RIGHT,
            KeyCode::BTN_MIDDLE,
            KeyCode::BTN_SIDE,
            KeyCode::BTN_EXTRA,
        ] {
            buttons.insert(b);
        }
        let mut axes = AttributeSet::<RelativeAxisCode>::new();
        axes.insert(RelativeAxisCode::REL_X);
        axes.insert(RelativeAxisCode::REL_Y);
        // Relative axes make libinput treat it as a pointer.
        let mouse = VirtualDevice::builder()?
            .name("Kyxen virtual mouse")
            .with_keys(&buttons)?
            .with_relative_axes(&axes)?
            .build()?;
        Ok(Self { keyboard, mouse })
    }
}

impl Output for Uinput {
    fn keys(&mut self, strokes: &[KeyStroke]) -> Result<()> {
        for &(key, pressed) in strokes {
            self.keyboard.emit(&[InputEvent::new(
                EventType::KEY.0,
                key.code(),
                i32::from(pressed),
            )])?;
            let is_mod = matches!(
                key,
                KeyCode::KEY_LEFTSHIFT | KeyCode::KEY_RIGHTALT | KeyCode::KEY_LEFTCTRL
            );
            thread::sleep(if is_mod { MOD_DELAY } else { KEY_DELAY });
        }
        Ok(())
    }

    fn mouse(&mut self, button: KeyCode, pressed: bool) -> Result<()> {
        self.mouse.emit(&[InputEvent::new(
            EventType::KEY.0,
            button.code(),
            i32::from(pressed),
        )])?;
        Ok(())
    }
}

fn mouse_code(b: MouseButton) -> KeyCode {
    match b {
        MouseButton::Left => KeyCode::BTN_LEFT,
        MouseButton::Right => KeyCode::BTN_RIGHT,
        MouseButton::Middle => KeyCode::BTN_MIDDLE,
        MouseButton::Button4 => KeyCode::BTN_SIDE,
        MouseButton::Button5 => KeyCode::BTN_EXTRA,
    }
}

pub fn spawn(
    output: impl Output + Send + 'static,
    layout: TextLayout,
) -> Result<(Sender<Job>, JoinHandle<()>)> {
    let (tx, rx) = crossbeam_channel::unbounded();
    let handle = thread::Builder::new()
        .name("actions".into())
        .spawn(move || {
            Executor {
                out: output,
                layout,
                held: BTreeMap::new(),
                repeats: BTreeMap::new(),
            }
            .run(&rx);
        })?;
    Ok((tx, handle))
}

struct Executor<O> {
    out: O,
    layout: TextLayout,
    held: BTreeMap<u8, MacroAction>,
    repeats: BTreeMap<u8, (MacroAction, Instant)>,
}

impl<O: Output> Executor<O> {
    fn run(&mut self, rx: &Receiver<Job>) {
        loop {
            let next_due = self.repeats.values().map(|(_, at)| *at).min();
            let job = match next_due {
                Some(at) => rx.recv_timeout(at.saturating_duration_since(Instant::now())),
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            match job {
                Ok(Job::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                    self.release_all();
                    return;
                }
                Ok(job) => self.handle(job),
                Err(RecvTimeoutError::Timeout) => self.fire_due_repeats(),
            }
        }
    }

    fn handle(&mut self, job: Job) {
        match job {
            Job::Run(action) => self.execute(&action),
            Job::ToggleHold { gkey, action } => {
                let release = self.held.remove(&gkey);
                let press = release.is_none();
                let action = release.unwrap_or(action);
                self.hold(&action, press);
                if press {
                    self.held.insert(gkey, action);
                }
            }
            Job::RepeatStart { gkey, action } => {
                self.execute(&action);
                let every =
                    Duration::from_secs_f64(action.repeat_rate.max(0.0) / 1000.0).max(MIN_REPEAT);
                self.repeats.insert(gkey, (action, Instant::now() + every));
            }
            Job::RepeatStop { gkey } => {
                self.repeats.remove(&gkey);
            }
            Job::ReleaseAll => self.release_all(),
            Job::SetLayout(layout) => {
                info!(layout = layout.name, "text macro layout");
                self.layout = layout;
            }
            Job::Shutdown => unreachable!("handled in run"),
        }
    }

    fn fire_due_repeats(&mut self) {
        let now = Instant::now();
        let due: BTreeSet<u8> = self
            .repeats
            .iter()
            .filter(|(_, (_, at))| *at <= now)
            .map(|(g, _)| *g)
            .collect();
        for gkey in due {
            let Some((action, _)) = self.repeats.get(&gkey).cloned() else {
                continue;
            };
            self.execute(&action);
            let every =
                Duration::from_secs_f64(action.repeat_rate.max(0.0) / 1000.0).max(MIN_REPEAT);
            // Measured from the end of this run, so slow actions never pile up.
            if let Some(entry) = self.repeats.get_mut(&gkey) {
                entry.1 = Instant::now() + every;
            }
        }
    }

    fn release_all(&mut self) {
        self.repeats.clear();
        for (_, action) in std::mem::take(&mut self.held) {
            self.hold(&action, false);
        }
    }

    fn execute(&mut self, action: &MacroAction) {
        let result = match action.action {
            ActionKind::Type => {
                let (strokes, skipped) = macros::text_strokes(&action.text, &self.layout);
                if !skipped.is_empty() {
                    warn!(
                        ?skipped,
                        layout = self.layout.name,
                        "characters not on the keyboard layout"
                    );
                }
                self.out.keys(&strokes)
            }
            ActionKind::Combo => {
                let keys = self.resolve(&action.keys);
                let mut strokes = macros::chord(&keys, true);
                strokes.extend(macros::chord(&keys, false));
                self.out.keys(&strokes)
            }
            ActionKind::Command => spawn_command(&action.cmd),
            ActionKind::MouseButton => {
                let b = mouse_code(action.mouse_btn);
                let clicks = if action.mouse_mode == MouseMode::DoubleClick {
                    2
                } else {
                    1
                };
                (0..clicks).try_for_each(|i| {
                    if i > 0 {
                        thread::sleep(Duration::from_millis(100));
                    }
                    self.out.mouse(b, true)?;
                    thread::sleep(Duration::from_millis(5));
                    self.out.mouse(b, false)
                })
            }
            ActionKind::HoldToggle => {
                self.hold(action, true);
                Ok(())
            }
            ActionKind::None | ActionKind::Unknown => Ok(()),
        };
        if let Err(e) = result {
            warn!("macro failed: {e:#}");
        }
    }

    fn hold(&mut self, action: &MacroAction, press: bool) {
        let result = if action.action == ActionKind::MouseButton {
            self.out.mouse(mouse_code(action.mouse_btn), press)
        } else {
            let keys = self.resolve(&action.keys);
            self.out.keys(&macros::chord(&keys, press))
        };
        if let Err(e) = result {
            warn!("hold failed: {e:#}");
        }
    }

    fn resolve(&self, names: &[String]) -> Vec<KeyCode> {
        let (keys, unknown) = macros::resolve_keys(names, &self.layout);
        if !unknown.is_empty() {
            warn!(?unknown, "unknown key names in combo");
        }
        keys
    }
}

/// Run a shell command detached from the daemon (own process group).
fn spawn_command(cmd: &str) -> Result<()> {
    if cmd.trim().is_empty() {
        return Ok(());
    }
    let mut child = Command::new("sh")
        .args(["-c", cmd])
        .stdin(Stdio::null())
        .process_group(0)
        .spawn()
        .with_context(|| format!("run {cmd:?}"))?;
    // Reap it in the background so it never becomes a zombie.
    thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Recorder(Arc<Mutex<Vec<String>>>);

    impl Output for Recorder {
        fn keys(&mut self, strokes: &[KeyStroke]) -> Result<()> {
            let mut log = self.0.lock().unwrap();
            log.extend(
                strokes
                    .iter()
                    .map(|(k, p)| format!("{}{k:?}", if *p { "+" } else { "-" })),
            );
            Ok(())
        }
        fn mouse(&mut self, b: KeyCode, pressed: bool) -> Result<()> {
            self.0
                .lock()
                .unwrap()
                .push(format!("{}{b:?}", if pressed { "+" } else { "-" }));
            Ok(())
        }
    }

    fn exec() -> (Executor<Recorder>, Recorder) {
        let rec = Recorder::default();
        (
            Executor {
                out: rec.clone(),
                layout: TextLayout::new("at", ""),
                held: BTreeMap::new(),
                repeats: BTreeMap::new(),
            },
            rec,
        )
    }

    fn combo(keys: &[&str]) -> MacroAction {
        MacroAction {
            action: ActionKind::Combo,
            keys: keys.iter().map(|s| (*s).into()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn combo_presses_then_releases_in_reverse() {
        let (mut e, rec) = exec();
        e.execute(&combo(&["ctrl", "z"]));
        assert_eq!(
            *rec.0.lock().unwrap(),
            ["+KEY_LEFTCTRL", "+KEY_Y", "-KEY_Y", "-KEY_LEFTCTRL"]
        );
    }

    #[test]
    fn hold_toggle_and_release_all() {
        let (mut e, rec) = exec();
        let hold = MacroAction {
            action: ActionKind::HoldToggle,
            keys: vec!["shift".into()],
            ..Default::default()
        };
        e.handle(Job::ToggleHold {
            gkey: 1,
            action: hold.clone(),
        });
        e.handle(Job::ToggleHold {
            gkey: 1,
            action: hold.clone(),
        });
        e.handle(Job::ToggleHold {
            gkey: 2,
            action: hold,
        });
        e.handle(Job::ReleaseAll);
        assert_eq!(
            *rec.0.lock().unwrap(),
            [
                "+KEY_LEFTSHIFT",
                "-KEY_LEFTSHIFT",
                "+KEY_LEFTSHIFT",
                "-KEY_LEFTSHIFT"
            ]
        );
    }

    #[test]
    fn repeats_run_sequentially_until_stopped() {
        let rec = Recorder::default();
        let (tx, handle) = spawn(rec.clone(), TextLayout::new("us", "")).unwrap();
        let action = MacroAction {
            action: ActionKind::Type,
            text: "ab".into(),
            repeat: true,
            repeat_rate: 10.0,
            ..Default::default()
        };
        tx.send(Job::RepeatStart { gkey: 1, action }).unwrap();
        thread::sleep(Duration::from_millis(400));
        tx.send(Job::RepeatStop { gkey: 1 }).unwrap();
        tx.send(Job::Shutdown).unwrap();
        handle.join().unwrap();
        let log = rec.0.lock().unwrap();
        assert!(log.len() >= 8, "repeated at least twice: {log:?}");
        // never interleaved: always a, a, b, b, a, a, b, b …
        for chunk in log.chunks(4) {
            if chunk.len() == 4 {
                assert_eq!(chunk, ["+KEY_A", "-KEY_A", "+KEY_B", "-KEY_B"]);
            }
        }
    }
}
