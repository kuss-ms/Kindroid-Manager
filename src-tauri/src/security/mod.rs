// `exists` and `clear` are only called via the Tauri command layer
// in non-test builds; suppress the unused-in-tests warning.
#![allow(dead_code)]

pub mod secrets;
