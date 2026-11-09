//! REA_ShareX — fast pre-configured region screenshot utility.
//!
//! Phase A scaffold: every module compiles against the shared `model` contract.
//! The real top-level orchestration (tray + hotkey loop -> capture -> save ->
//! toast) is wired by the integrator once the modules are implemented.

#![allow(dead_code)] // Phase A: stubs are not all wired up yet.

mod capture;
mod config_ui;
mod hotkey;
mod model;
mod naming;
mod overlay;
mod save;
mod settings;
mod toast;
mod tray;

fn main() {
    println!("REA_ShareX scaffold — modules compiled. Orchestration TBD.");
}
