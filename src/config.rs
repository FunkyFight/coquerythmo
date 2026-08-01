#![allow(clippy::items_after_test_module)]

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};

const MAX_RECENT: usize = 10;
const DEFAULT_SERVER_IP: &str = "38.87.117.194";
const PREVIOUS_DEFAULT_SERVER_IP: &str = "46.225.214.44";
const DEFAULT_SERVER_PORT: u16 = 9050;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentProject {
    pub video_path: PathBuf,
    pub br_path: PathBuf,
}

static INSTANCE: OnceLock<RwLock<Config>> = OnceLock::new();
static PROJECT_SCROLL_SPEED: AtomicU32 = AtomicU32::new(1.0f32.to_bits());
static PROJECT_READING_BAR_OFFSET_SECONDS: AtomicU64 = AtomicU64::new(0.0f64.to_bits());
static DEV_MODE: bool = false;

pub fn dev_mode() -> bool {
    DEV_MODE
}

const APP_NAME: &str = "coquerythmo";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub window: WindowConfig,
    pub ui: UiConfig,
    pub lang: String,
    pub network: NetworkConfig,
    pub accessibility: AccessibilityConfig,
    /// Base directory for runtime temporary files. `None` means the OS temp
    /// directory (for example `%TEMP%` on Windows).
    pub temporary_directory: Option<PathBuf>,
    pub recording_input_device: Option<String>,
    pub last_whats_new_version: Option<String>,
    #[serde(default)]
    pub recent_projects: Vec<RecentProject>,
    #[serde(default)]
    pub license_key: String,
    #[serde(default)]
    pub license_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AccessibilityConfig {
    pub screen_reader_enabled: bool,
    pub voice_volume: f32,
    pub media_ducking: f32,
}

impl Default for AccessibilityConfig {
    fn default() -> Self {
        Self {
            screen_reader_enabled: false,
            voice_volume: 1.0,
            media_ducking: 0.35,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    pub width: u32,
    pub height: u32,
    pub title: String,
    pub vsync: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub font_size: f32,
    pub border_radius: f32,
    pub rythmo_font: Option<String>,
    /// Fraction of the free area (screen height minus topbar and toolbar)
    /// allocated to the video preview. The bande rythmo gets the remainder.
    pub video_split: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    pub server_ip: String,
    pub server_port: u16,
    #[serde(skip)]
    pub password: String,
    pub username: String,
    #[serde(default = "default_servers")]
    pub saved_servers: Vec<SavedServer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedServer {
    pub ip: String,
    pub port: u16,
}

fn default_servers() -> Vec<SavedServer> {
    vec![SavedServer {
        ip: DEFAULT_SERVER_IP.into(),
        port: DEFAULT_SERVER_PORT,
    }]
}

pub fn is_default_server(ip: &str, port: u16) -> bool {
    ip == DEFAULT_SERVER_IP && port == DEFAULT_SERVER_PORT
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            server_ip: "127.0.0.1".into(),
            server_port: DEFAULT_SERVER_PORT,
            password: String::new(),
            username: "User".into(),
            saved_servers: default_servers(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            window: WindowConfig::default(),
            ui: UiConfig::default(),
            lang: "fr-fr".into(),
            network: NetworkConfig::default(),
            accessibility: AccessibilityConfig::default(),
            temporary_directory: None,
            recording_input_device: None,
            last_whats_new_version: None,
            recent_projects: Vec::new(),
            license_key: String::new(),
            license_type: String::new(),
        }
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 1024,
            height: 720,
            title: "coquerythmo".into(),
            vsync: true,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            font_size: 18.0,
            border_radius: 8.0,
            rythmo_font: None,
            video_split: 0.48,
        }
    }
}

impl Config {
    fn config_path() -> PathBuf {
        let dir = dirs::config_dir()
            .or_else(dirs::data_local_dir)
            .or_else(dirs::data_dir)
            .unwrap_or_else(std::env::temp_dir)
            .join(APP_NAME);
        dir.join(CONFIG_FILE)
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(contents) => match toml::from_str::<Config>(&contents) {
                    Ok(mut config) => {
                        log::info!("Config loaded from {}", path.display());
                        if config.migrate() {
                            log::info!("Config migrated to latest defaults");
                            config.save();
                        }
                        return config;
                    }
                    Err(e) => {
                        log::warn!("Invalid config file, using defaults: {e}");
                    }
                },
                Err(e) => {
                    log::warn!("Could not read config file, using defaults: {e}");
                }
            }
        } else {
            log::info!("No config file found, using defaults");
        }
        let config = Config {
            last_whats_new_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            ..Config::default()
        };
        config.save();
        config
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match toml::to_string_pretty(self) {
            Ok(contents) => {
                if let Err(e) = fs::write(&path, contents) {
                    log::warn!("Could not write config: {e}");
                }
            }
            Err(e) => {
                log::warn!("Could not serialize config: {e}");
            }
        }
    }

    fn migrate(&mut self) -> bool {
        let migrated = self.migrate_default_server_ip();
        self.ensure_default_server() || migrated
    }

    fn migrate_default_server_ip(&mut self) -> bool {
        let mut changed = false;

        if self.network.server_ip == PREVIOUS_DEFAULT_SERVER_IP
            && self.network.server_port == DEFAULT_SERVER_PORT
        {
            self.network.server_ip = DEFAULT_SERVER_IP.into();
            changed = true;
        }

        let mut has_current_default = self
            .network
            .saved_servers
            .iter()
            .any(|s| s.ip == DEFAULT_SERVER_IP && s.port == DEFAULT_SERVER_PORT);

        for server in &mut self.network.saved_servers {
            if server.ip == PREVIOUS_DEFAULT_SERVER_IP && server.port == DEFAULT_SERVER_PORT {
                changed = true;
                if !has_current_default {
                    server.ip = DEFAULT_SERVER_IP.into();
                    has_current_default = true;
                }
            }
        }

        if changed {
            self.network
                .saved_servers
                .retain(|s| s.ip != PREVIOUS_DEFAULT_SERVER_IP || s.port != DEFAULT_SERVER_PORT);
        }

        changed
    }

    fn ensure_default_server(&mut self) -> bool {
        if self
            .network
            .saved_servers
            .iter()
            .any(|server| is_default_server(&server.ip, server.port))
        {
            return false;
        }
        self.network.saved_servers.insert(
            0,
            SavedServer {
                ip: DEFAULT_SERVER_IP.into(),
                port: DEFAULT_SERVER_PORT,
            },
        );
        true
    }

    fn remove_saved_server(&mut self, index: usize) -> bool {
        let Some(server) = self.network.saved_servers.get(index) else {
            return false;
        };
        if is_default_server(&server.ip, server.port) {
            return false;
        }
        self.network.saved_servers.remove(index);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_default_server_rewrites_active_and_saved_ip() {
        let mut config = Config::default();
        config.network.server_ip = PREVIOUS_DEFAULT_SERVER_IP.into();
        config.network.server_port = DEFAULT_SERVER_PORT;
        config.network.saved_servers = vec![SavedServer {
            ip: PREVIOUS_DEFAULT_SERVER_IP.into(),
            port: DEFAULT_SERVER_PORT,
        }];

        assert!(config.migrate());
        assert_eq!(config.network.server_ip, DEFAULT_SERVER_IP);
        assert_eq!(config.network.saved_servers.len(), 1);
        assert_eq!(config.network.saved_servers[0].ip, DEFAULT_SERVER_IP);
        assert_eq!(config.network.saved_servers[0].port, DEFAULT_SERVER_PORT);
    }

    #[test]
    fn migrate_default_server_removes_duplicate_old_default() {
        let mut config = Config::default();
        config.network.saved_servers = vec![
            SavedServer {
                ip: PREVIOUS_DEFAULT_SERVER_IP.into(),
                port: DEFAULT_SERVER_PORT,
            },
            SavedServer {
                ip: DEFAULT_SERVER_IP.into(),
                port: DEFAULT_SERVER_PORT,
            },
        ];

        assert!(config.migrate());
        assert_eq!(config.network.saved_servers.len(), 1);
        assert_eq!(config.network.saved_servers[0].ip, DEFAULT_SERVER_IP);
        assert_eq!(config.network.saved_servers[0].port, DEFAULT_SERVER_PORT);
    }

    #[test]
    fn migrate_default_server_preserves_custom_port() {
        let mut config = Config::default();
        config.network.server_ip = PREVIOUS_DEFAULT_SERVER_IP.into();
        config.network.server_port = DEFAULT_SERVER_PORT + 1;
        config.network.saved_servers = vec![SavedServer {
            ip: PREVIOUS_DEFAULT_SERVER_IP.into(),
            port: DEFAULT_SERVER_PORT + 1,
        }];

        assert!(config.migrate());
        assert_eq!(config.network.server_ip, PREVIOUS_DEFAULT_SERVER_IP);
        assert!(config
            .network
            .saved_servers
            .iter()
            .any(|server| server.ip == PREVIOUS_DEFAULT_SERVER_IP
                && server.port == DEFAULT_SERVER_PORT + 1));
        assert!(config
            .network
            .saved_servers
            .iter()
            .any(|server| is_default_server(&server.ip, server.port)));
    }

    #[test]
    fn migrate_restores_a_missing_default_server() {
        let mut config = Config::default();
        config.network.saved_servers.clear();

        assert!(config.migrate());
        assert_eq!(config.network.saved_servers.len(), 1);
        assert!(is_default_server(
            &config.network.saved_servers[0].ip,
            config.network.saved_servers[0].port
        ));
    }

    #[test]
    fn default_server_cannot_be_removed() {
        let mut config = Config::default();

        assert!(!config.remove_saved_server(0));
        assert_eq!(config.network.saved_servers.len(), 1);
    }

    #[test]
    fn microphone_selection_survives_config_round_trip() {
        let mut config = Config::default();
        config.recording_input_device = Some("Studio microphone".into());

        let encoded = toml::to_string(&config).unwrap();
        let decoded: Config = toml::from_str(&encoded).unwrap();

        assert_eq!(
            decoded.recording_input_device.as_deref(),
            Some("Studio microphone")
        );
    }

    #[test]
    fn temporary_directory_survives_config_round_trip() {
        let mut config = Config::default();
        config.temporary_directory = Some(PathBuf::from(r"D:\CoquerythmoTemp"));

        let encoded = toml::to_string(&config).unwrap();
        let decoded: Config = toml::from_str(&encoded).unwrap();

        assert_eq!(decoded.temporary_directory, config.temporary_directory);
    }
}

pub fn init() {
    INSTANCE.get_or_init(|| RwLock::new(Config::load()));
}

pub fn default_temporary_directory() -> PathBuf {
    std::env::temp_dir()
}

pub fn temporary_directory() -> PathBuf {
    INSTANCE
        .get()
        .and_then(|lock| lock.read().ok())
        .and_then(|config| config.temporary_directory.clone())
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(default_temporary_directory)
}

/// Return the configured UI language without requiring the global config to
/// have been initialized (notably useful for domain-only tests).
pub fn language_or_default() -> String {
    INSTANCE
        .get()
        .and_then(|lock| lock.read().ok().map(|config| config.lang.clone()))
        .unwrap_or_else(|| Config::default().lang)
}

pub fn save_settings(lang: String, rythmo_font: Option<String>, temporary_directory: PathBuf) {
    let lock = INSTANCE.get().expect("config not initialized");
    let mut cfg = lock.write().unwrap();
    cfg.lang = lang;
    cfg.ui.rythmo_font = rythmo_font;
    cfg.temporary_directory = if temporary_directory.as_os_str().is_empty()
        || temporary_directory == default_temporary_directory()
    {
        None
    } else {
        Some(temporary_directory)
    };
    cfg.save();
}

pub(crate) fn set_project_view_settings(
    scroll_speed: f32,
    reading_bar_offset_percent: f32,
    viewport_width: f32,
    fps: f64,
) {
    let scroll_speed = scroll_speed.clamp(0.25, 4.0);
    let pixels_per_frame = crate::constants::PIXELS_PER_FRAME * scroll_speed;
    let offset_seconds = crate::rythmo_layout::reading_bar_offset_seconds(
        reading_bar_offset_percent.clamp(-50.0, 50.0),
        viewport_width,
        fps,
        pixels_per_frame,
    );
    PROJECT_SCROLL_SPEED.store(scroll_speed.to_bits(), Ordering::Relaxed);
    PROJECT_READING_BAR_OFFSET_SECONDS.store(offset_seconds.to_bits(), Ordering::Relaxed);
}

pub fn scroll_speed() -> f32 {
    f32::from_bits(PROJECT_SCROLL_SPEED.load(Ordering::Relaxed))
}

pub fn reading_bar_offset_seconds() -> f64 {
    f64::from_bits(PROJECT_READING_BAR_OFFSET_SECONDS.load(Ordering::Relaxed))
}

#[cfg(test)]
pub fn set_reading_bar_offset_seconds(offset: f64) {
    PROJECT_READING_BAR_OFFSET_SECONDS.store(offset.to_bits(), Ordering::Relaxed);
}

pub fn add_recent_project(video_path: PathBuf, br_path: PathBuf) {
    let lock = INSTANCE.get().expect("config not initialized");
    let mut cfg = lock.write().unwrap();
    // Remove existing entry with same paths
    cfg.recent_projects
        .retain(|r| r.video_path != video_path || r.br_path != br_path);
    // Insert at front
    cfg.recent_projects.insert(
        0,
        RecentProject {
            video_path,
            br_path,
        },
    );
    // Keep only MAX_RECENT
    cfg.recent_projects.truncate(MAX_RECENT);
    cfg.save();
}

pub fn recent_projects() -> Vec<RecentProject> {
    get().recent_projects.clone()
}

pub fn set_screen_reader_enabled(enabled: bool) {
    let lock = INSTANCE.get().expect("config not initialized");
    let mut cfg = lock.write().unwrap();
    cfg.accessibility.screen_reader_enabled = enabled;
    cfg.save();
}

pub fn remove_recent_project(video_path: &PathBuf, br_path: &PathBuf) {
    let lock = INSTANCE.get().expect("config not initialized");
    let mut cfg = lock.write().unwrap();
    cfg.recent_projects
        .retain(|recent| &recent.video_path != video_path || &recent.br_path != br_path);
    cfg.save();
}

pub fn should_show_whats_new(version: &str) -> bool {
    DEV_MODE || get().last_whats_new_version.as_deref() != Some(version)
}

pub fn mark_whats_new_seen(version: &str) {
    let lock = INSTANCE.get().expect("config not initialized");
    let mut cfg = lock.write().unwrap();
    cfg.last_whats_new_version = Some(version.to_string());
    cfg.save();
}

pub fn saved_servers() -> Vec<SavedServer> {
    get().network.saved_servers.clone()
}

pub fn add_server(ip: String, port: u16) {
    let lock = INSTANCE.get().expect("config not initialized");
    let mut cfg = lock.write().unwrap();
    if !cfg
        .network
        .saved_servers
        .iter()
        .any(|s| s.ip == ip && s.port == port)
    {
        cfg.network.saved_servers.push(SavedServer { ip, port });
        cfg.save();
    }
}

pub fn remove_server(index: usize) {
    let lock = INSTANCE.get().expect("config not initialized");
    let mut cfg = lock.write().unwrap();
    if cfg.remove_saved_server(index) {
        cfg.save();
    }
}

pub fn video_split() -> f32 {
    get().ui.video_split
}

pub fn recording_input_device() -> Option<String> {
    get().recording_input_device.clone()
}

pub fn set_recording_input_device(device: Option<String>) {
    let lock = INSTANCE.get().expect("config not initialized");
    let mut cfg = lock.write().unwrap();
    cfg.recording_input_device = device;
    cfg.save();
}

pub fn set_video_split(split: f32) {
    let lock = INSTANCE.get().expect("config not initialized");
    let mut cfg = lock.write().unwrap();
    cfg.ui.video_split = split;
    cfg.save();
}

pub fn get() -> std::sync::RwLockReadGuard<'static, Config> {
    INSTANCE
        .get()
        .expect("config not initialized, call config::init() first")
        .read()
        .unwrap()
}

pub fn license_key() -> String {
    get().license_key.clone()
}

pub fn license_type() -> String {
    get().license_type.clone()
}

pub fn set_license(key: String, lic_type: String) {
    let lock = INSTANCE.get().expect("config not initialized");
    let mut cfg = lock.write().unwrap();
    cfg.license_key = key;
    cfg.license_type = lic_type;
    cfg.save();
}

pub fn license_display_label() -> String {
    let t = license_type();
    let lower = t.to_lowercase();
    if lower.starts_with("licence") || lower.starts_with("license") {
        t
    } else if lower.contains("école")
        || lower.contains("school")
        || lower.contains("escuela")
        || lower.contains("organisme")
        || lower.contains("organization")
        || lower.contains("organización")
        || lower.contains("structure")
        || lower.contains("enterprise")
    {
        "License organisme".into()
    } else if lower.contains("professionnel") || lower.contains("professional") {
        "License professionnelle".into()
    } else {
        format!("License {}", t)
    }
}
