use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
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

const APP_NAME: &str = "coquerythmo";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub window: WindowConfig,
    pub ui: UiConfig,
    pub lang: String,
    pub network: NetworkConfig,
    pub last_whats_new_version: Option<String>,
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
            last_whats_new_version: None,
            recent_projects: Vec::new(),
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
        let mut config = Config::default();
        config.last_whats_new_version = Some(env!("CARGO_PKG_VERSION").to_string());
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
        self.migrate_default_server_ip()
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

        assert!(!config.migrate());
        assert_eq!(config.network.server_ip, PREVIOUS_DEFAULT_SERVER_IP);
        assert_eq!(
            config.network.saved_servers[0].ip,
            PREVIOUS_DEFAULT_SERVER_IP
        );
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

pub fn should_show_whats_new(version: &str) -> bool {
    get().last_whats_new_version.as_deref() != Some(version)
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
    if index < cfg.network.saved_servers.len() {
        cfg.network.saved_servers.remove(index);
        cfg.save();
    }
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
