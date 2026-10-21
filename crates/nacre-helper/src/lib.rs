//! Native desktop helper boundaries. Orient remains an independent MIT dependency.
#![deny(unsafe_code)]
pub mod ai_core;
pub mod ai_rpc;
pub mod arguments;
pub mod aur_review;
mod brain_arguments;
pub mod brain_data;
pub mod branding;
pub mod classic_state;
pub mod colors;
pub mod colors_launch;
pub mod compositor;
pub mod compositor_compat;
pub mod compositor_config;
pub mod daemon;
pub mod dashboard_data;
pub mod dashboard_media;
pub mod dashboard_system;
pub mod demo_wallpapers;
pub mod diagnostics;
pub mod diagnostics_native;
pub mod file_icons;
pub mod files;
pub mod i18n;
pub mod image_paint;
pub mod image_text;
pub mod json;
pub mod launcher_preferences;
pub mod launcher_workflows;
pub mod launchers;
pub mod light_devices;
pub mod lock_config;
pub mod lock_dashboard;
pub mod menu;
pub mod menu_actions;
pub mod menu_arguments;
pub mod menu_cli;
pub mod menu_registry;
pub mod network_state;
pub mod niri_acceptance_monitor;
pub mod niri_acceptance_systemd;
pub mod niri_hover_install;
mod niri_hover_pointer;
pub mod niri_hover_probe;
pub mod niri_preview;
pub mod niri_session_gate;
pub mod niri_session_runtime;
pub mod niri_session_stage;
pub mod niri_widths;
pub mod notification_history;
pub mod orient;
pub mod palette_api;
pub mod palette_cli;
pub mod palette_engine;
pub mod paths;
pub mod pearl_state;
pub mod pearl_window;
pub mod process;
pub mod resource_state;
pub mod service;
pub mod session_watch;
pub mod shell_supervisor;
pub mod signals;
pub mod software_actions;
pub mod software_ai;
pub mod software_environments;
pub mod software_hardware;
pub mod software_input;
pub mod software_library;
pub mod software_setup;
pub mod software_speed;
pub mod software_tui;
pub mod state;
pub mod systemd;
#[cfg(test)]
mod test_support;
pub mod unicode;
pub mod updates;
pub mod wallpaper_media;
pub mod wallpaper_watch;
pub mod weather;
pub mod window_chat_title;
pub mod window_hide;
pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

pub mod kde_palette;

pub mod template_export;
pub mod terminal_branding;
pub mod terminal_font;
pub mod theme_bundles;

pub mod login_appearance;

pub mod lock_info;

pub mod lock_prepare;

pub mod desktop_settings;

pub mod owned_settings;
