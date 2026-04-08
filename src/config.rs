use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

const MAX_RECENT: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentProject {
    pub video_path: PathBuf,
    pub br_path: PathBuf,
}

static INSTANCE: OnceLock<RwLock<Config>> = OnceLock::new();

const APP_NAME: &str = "coquerythmo";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub window: WindowConfig,
    pub ui: UiConfig,
    pub lang: String,
    pub network: NetworkConfig,
    #[serde(default)]
    pub recent_projects: Vec<RecentProject>,
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
    pub scroll_speed: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    pub server_ip: String,
    pub server_port: u16,
    #[serde(skip)]
    pub password: String,
    pub username: String,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            server_ip: "127.0.0.1".into(),
            server_port: 9050,
            password: String::new(),
            username: "User".into(),
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
            recent_projects: Vec::new(),
        }
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
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
            scroll_speed: 1.0,
        }
    }
}

impl Config {
    fn config_path() -> PathBuf {
        let dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(APP_NAME);
        dir.join(CONFIG_FILE)
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(contents) => match toml::from_str(&contents) {
                    Ok(config) => {
                        log::info!("Config loaded from {}", path.display());
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
        let config = Config::default();
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
}

pub fn init() {
    INSTANCE.get_or_init(|| RwLock::new(Config::load()));
}

pub fn save_settings(lang: String, rythmo_font: Option<String>, scroll_speed: f32) {
    let lock = INSTANCE.get().expect("config not initialized");
    let mut cfg = lock.write().unwrap();
    cfg.lang = lang;
    cfg.ui.rythmo_font = rythmo_font;
    cfg.ui.scroll_speed = scroll_speed;
    cfg.save();
}

pub fn add_recent_project(video_path: PathBuf, br_path: PathBuf) {
    let lock = INSTANCE.get().expect("config not initialized");
    let mut cfg = lock.write().unwrap();
    // Remove existing entry with same paths
    cfg.recent_projects.retain(|r| r.video_path != video_path || r.br_path != br_path);
    // Insert at front
    cfg.recent_projects.insert(0, RecentProject { video_path, br_path });
    // Keep only MAX_RECENT
    cfg.recent_projects.truncate(MAX_RECENT);
    cfg.save();
}

pub fn recent_projects() -> Vec<RecentProject> {
    get().recent_projects.clone()
}

pub fn scroll_speed() -> f32 {
    get().ui.scroll_speed
}

pub fn get() -> std::sync::RwLockReadGuard<'static, Config> {
    INSTANCE
        .get()
        .expect("config not initialized, call config::init() first")
        .read()
        .unwrap()
}
