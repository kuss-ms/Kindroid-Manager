// Commands are consumed by `tauri_wrappers` (non-test only). In test
// builds they're "unused" but still part of the public API surface.
#![allow(dead_code)]

pub mod characters;
pub mod chat_history;
pub mod history;
pub mod push;
pub mod settings;
pub mod share_code;
pub mod sync_loop;
pub mod sync_registry;
pub mod targets;
pub mod tauri_wrappers;

#[cfg(not(test))]
pub mod sync_loop_impl;
