//! Daemon state machine: profiles, key events, IPC and reconnects.

use std::collections::BTreeMap;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use kyxen_core::config::{GlobalConfig, Profile, Store};
use kyxen_core::keymap::{Form, Keymap};
use kyxen_core::layout::{self, TextLayout};
use serde_json::{Value, json};
use tracing::{error, info, warn};

use crate::actions::Job;
use crate::device::{self, GKeyMode, Keyboard};
use crate::ipc::Request;
use crate::{keyboard, passthrough};

const TICK: Duration = Duration::from_millis(1500);
const RETRY: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub enum Event {
    GKey {
        n: u8,
        pressed: bool,
    },
    /// M-key slot pressed (1-based).
    MKey(u8),
    KeyboardLost,
    Ipc(Request, Sender<Value>),
    Shutdown,
}

struct Connection {
    keyboard: Keyboard,
    commands: Sender<keyboard::Command>,
    threads: Vec<JoinHandle<()>>,
}

pub struct Daemon {
    store: Store,
    global: GlobalConfig,
    profiles: BTreeMap<String, Profile>,
    active: String,
    actions: Sender<Job>,
    events_tx: Sender<Event>,
    conn: Option<Connection>,
    config_stamp: (Option<SystemTime>, Option<SystemTime>),
    /// ISO / ANSI key map, from the config.
    keymap: &'static Keymap,
}

impl Daemon {
    pub fn new(store: Store, actions: Sender<Job>, events_tx: Sender<Event>) -> Self {
        let mut d = Self {
            store,
            global: GlobalConfig::default(),
            profiles: BTreeMap::new(),
            active: String::new(),
            actions,
            events_tx,
            conn: None,
            config_stamp: (None, None),
            keymap: Keymap::g815(),
        };
        d.reload();
        d
    }

    /// Text layout for the current config (override or system).
    pub fn text_layout(global: &GlobalConfig) -> TextLayout {
        let (l, v) = layout::detect(
            global.keyboard_layout.as_deref(),
            global.keyboard_variant.as_deref(),
        );
        TextLayout::new(&l, &v)
    }

    /// Key map (ISO / ANSI) for the current config.
    fn keymap(global: &GlobalConfig) -> &'static Keymap {
        let (l, _) = layout::detect(global.keyboard_layout.as_deref(), None);
        Keymap::g815_for(Form::resolve(global.keyboard_form.as_deref(), &l))
    }

    fn stamp(&self) -> (Option<SystemTime>, Option<SystemTime>) {
        let global = std::fs::metadata(self.store.global_path())
            .and_then(|m| m.modified())
            .ok();
        (self.store.profiles_mtime(), global)
    }

    /// (Re)load config.toml and all profiles.
    fn reload(&mut self) {
        let layout_before = (
            self.global.keyboard_layout.clone(),
            self.global.keyboard_variant.clone(),
        );
        self.global = self.store.load_global().unwrap_or_else(|e| {
            warn!("{e}; using defaults");
            GlobalConfig::default()
        });
        let (mut profiles, failed) = self.store.load_profiles();
        for e in failed {
            warn!("skipping profile: {e}");
        }
        if profiles.is_empty() {
            match self.store.ensure_default_profile() {
                Ok(p) => profiles.push(p),
                Err(e) => error!("cannot create default profile: {e}"),
            }
        }
        self.profiles = profiles.into_iter().map(|p| (p.name.clone(), p)).collect();
        self.active = if self.profiles.contains_key(&self.global.active_profile) {
            self.global.active_profile.clone()
        } else {
            self.profiles.keys().next().cloned().unwrap_or_default()
        };
        let layout_now = (
            self.global.keyboard_layout.clone(),
            self.global.keyboard_variant.clone(),
        );
        if layout_now != layout_before {
            let _ = self
                .actions
                .send(Job::SetLayout(Self::text_layout(&self.global)));
        }
        self.keymap = Self::keymap(&self.global);
        self.config_stamp = self.stamp();
        self.apply_active();
        info!(
            active = self.active,
            profiles = self.profiles.len(),
            "config loaded"
        );
    }

    fn active_profile(&self) -> Option<&Profile> {
        self.profiles.get(&self.active)
    }

    /// Push the active profile's lighting and M-key LED to the keyboard.
    fn apply_active(&self) {
        let Some(conn) = &self.conn else { return };
        if let Some(p) = self.active_profile() {
            let _ = conn
                .commands
                .send(keyboard::Command::Lighting(p.lighting.clone(), self.keymap));
        }
        let _ = conn.commands.send(keyboard::Command::MKeyLed(
            self.global.slot_of(&self.active),
        ));
    }

    fn switch(&mut self, name: &str) -> bool {
        if !self.profiles.contains_key(name) {
            warn!(profile = name, "no such profile");
            return false;
        }
        let _ = self.actions.send(Job::ReleaseAll);
        name.clone_into(&mut self.active);
        name.clone_into(&mut self.global.active_profile);
        if let Err(e) = self.store.save_global(&self.global) {
            warn!("could not save active profile: {e}");
        }
        self.config_stamp = self.stamp();
        self.apply_active();
        info!(profile = name, "profile switched");
        true
    }

    fn on_gkey(&self, n: u8, pressed: bool) {
        let Some(action) = self.active_profile().and_then(|p| p.gkey(n)).cloned() else {
            return;
        };
        let job = if action.repeats() {
            if pressed {
                Job::RepeatStart { gkey: n, action }
            } else {
                Job::RepeatStop { gkey: n }
            }
        } else if !pressed {
            return;
        } else if action.is_hold() {
            Job::ToggleHold { gkey: n, action }
        } else {
            Job::Run(action)
        };
        let _ = self.actions.send(job);
    }

    fn on_ipc(&mut self, req: Request) -> Value {
        match req {
            Request::Status => json!({
                "active_profile": self.active,
                "profiles": self.profiles.keys().collect::<Vec<_>>(),
                "keyboard": self.conn.as_ref().map(|c| c.keyboard.model.name),
            }),
            Request::Switch { profile } => json!({ "ok": self.switch(&profile) }),
            Request::Reload => {
                self.reload();
                json!({ "ok": true })
            }
            Request::Run { gkey } => {
                let action = self.active_profile().and_then(|p| p.gkey(gkey)).cloned();
                let ok = action.is_some_and(|a| {
                    // a single run, even for repeat/hold macros
                    self.actions.send(Job::Run(a)).is_ok()
                });
                json!({ "ok": ok })
            }
        }
    }

    fn connect(&mut self) -> Result<bool> {
        let Some(kb) = device::detect() else {
            return Ok(false);
        };
        info!(model = kb.model.name, hidpp = %kb.hidpp.display(), "keyboard found");
        let (commands, handle) = keyboard::spawn(&kb, self.events_tx.clone())?;
        let mut threads = vec![handle];
        if kb.model.gkeys == GKeyMode::BootProtocol {
            if let Some(node) = &kb.boot_event {
                threads.push(passthrough::spawn_passthrough(node)?);
            }
            if let Some(hidraw) = &kb.boot_hidraw {
                threads.push(passthrough::spawn_boot_reader(
                    hidraw,
                    self.events_tx.clone(),
                )?);
            }
        }
        self.conn = Some(Connection {
            keyboard: kb,
            commands,
            threads,
        });
        self.apply_active();
        Ok(true)
    }

    fn disconnect(&mut self, restore: bool) {
        let Some(conn) = self.conn.take() else { return };
        let _ = self.actions.send(Job::ReleaseAll);
        if restore {
            let _ = conn.commands.send(keyboard::Command::Shutdown);
        }
        drop(conn.commands);
        for t in conn.threads {
            if t.is_finished() || restore {
                // passthrough/boot readers end when the device goes away
                let _ = t.join();
            }
        }
    }

    pub fn run(&mut self, events: &Receiver<Event>) {
        let mut waiting_logged = false;
        loop {
            if self.conn.is_none() {
                match self.connect() {
                    Ok(true) => waiting_logged = false,
                    Ok(false) if !waiting_logged => {
                        info!(
                            "no supported keyboard found, retrying every {}s",
                            RETRY.as_secs()
                        );
                        waiting_logged = true;
                    }
                    Ok(false) => {}
                    Err(e) => warn!("keyboard setup failed: {e:#}"),
                }
            }
            let timeout = if self.conn.is_some() { TICK } else { RETRY };
            match events.recv_timeout(timeout) {
                Ok(Event::GKey { n, pressed }) => self.on_gkey(n, pressed),
                Ok(Event::MKey(slot)) => {
                    if let Some(name) = self.global.slot(slot).map(str::to_owned) {
                        self.switch(&name);
                    } else {
                        info!("M{slot} pressed but no profile_slots.m{slot} in config.toml");
                    }
                }
                Ok(Event::Ipc(req, reply)) => {
                    let _ = reply.send(self.on_ipc(req));
                }
                Ok(Event::KeyboardLost) => {
                    warn!("keyboard disconnected; waiting for it to come back");
                    self.disconnect(false);
                }
                Ok(Event::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                    info!("shutting down");
                    self.disconnect(true);
                    return;
                }
                Err(RecvTimeoutError::Timeout) => {
                    if self.stamp() != self.config_stamp {
                        info!("config changed on disk");
                        self.reload();
                    }
                }
            }
        }
    }
}
