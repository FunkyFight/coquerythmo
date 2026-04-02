use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

static INSTANCE: OnceLock<Config> = OnceLock::new();

const APP_NAME: &str = "coquerythmo";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub window: WindowConfig,
    pub ui: UiConfig,
    pub lang: String,
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            window: WindowConfig::default(),
            ui: UiConfig::default(),
            lang: "fr-fr".into(),
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
    INSTANCE.get_or_init(Config::load);
}

pub fn get() -> &'static Config {
    INSTANCE
        .get()
        .expect("config not initialized, call config::init() first")
}
