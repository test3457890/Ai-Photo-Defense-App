//! AI Photo Defense desktop workstation.
//!
//! One application state shared by three interface modes with genuinely different layouts:
//! Essential (a compact processing list), Professional (dockable photographic workspace) and
//! Technical (diagnostics, validation and research tools). All modes edit the same collection,
//! edit stacks, settings and project, so switching mode never loses work.
//!
//! Heavy work (decode, render, scan, export) runs on blocking worker threads. Interactive
//! previews render a proxy (<= 1600 px) of the photograph; exports always render full size.
// Release builds are GUI-only on Windows: no console window opens beside the application.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod theme;
mod views;
mod widgets;

fn main() -> iced::Result {
    app::run()
}
