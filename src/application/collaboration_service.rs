//! Collaboration session state and the network client it owns.

use std::sync::{Arc, Mutex};

use crate::network::NetworkClient;

/// Result of a background server discovery/ping operation.
pub struct PingResult {
    pub ip: String,
    pub port: u16,
    pub name: String,
    pub motd: String,
    pub online: u32,
    pub max_slots: u32,
    pub success: bool,
}

pub struct CollaborationSession {
    pub network: NetworkClient,
    pub ping_results: Arc<Mutex<Vec<PingResult>>>,
}

impl CollaborationSession {
    pub fn new() -> Self {
        Self {
            network: NetworkClient::new(),
            ping_results: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Default for CollaborationSession {
    fn default() -> Self {
        Self::new()
    }
}
