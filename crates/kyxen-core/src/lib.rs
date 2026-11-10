//! Keyboard-independent parts of Kyxen: configuration files, text layouts,
//! macro translation, key geometry and lighting effects.

pub mod colour;
pub mod config;
pub mod desktop;
pub mod effects;
pub mod ipc;
pub mod keymap;
pub mod layout;
pub mod macros;

pub use colour::Colour;
pub use kyxen_hidpp::lighting::Rgb;
