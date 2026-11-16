#![windows_subsystem = "windows"]
mod a11y;
mod config;
mod engine;
mod interval_provider;
mod log;
mod numeric_edit;
mod style;
mod system;
mod ui;
fn main() {
    ui::run();
}
