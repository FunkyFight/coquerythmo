//! Custom URL protocol (`coquerythmo://`) used for quick room setup.
//!
//! Two link flavours are supported, both packaging their fields as a
//! base64 (URL-safe, no padding) encoded JSON document:
//!
//! - **Host** (`{"k":"h"}`): server address (`ip:port`, port defaulting to
//!   `9050`), username, password and the local `.coquerythmo` project to load.
//!   Opening the link closes the current project (prompting to save when
//!   dirty), loads the target project and creates a fresh room as director.
//! - **Join** (`{"k":"j"}`): server address, password and room code. The app
//!   asks for a username, then connects as an actor.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};

/// Scheme registered in the Windows registry (HKCU) so browsers and file
/// explorers can hand `coquerythmo://...` URLs to the executable.
pub const PROTOCOL_SCHEME: &str = "coquerythmo";
pub const PROTOCOL_PREFIX: &str = "coquerythmo://";
const PROTOCOL_PATH_PREFIX: &str = "coquerythmo://link/";
pub const DEFAULT_SERVER_PORT: u16 = 9050;

const KIND_HOST: &str = "h";
const KIND_JOIN: &str = "j";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolKind {
    Host,
    Join,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolPayload {
    /// `h` = host (create room), `j` = join room. Short keys keep the URL
    /// compact since it is hand-copied into chats.
    #[serde(rename = "k")]
    kind: String,
    /// `ip:port` (the `:port` suffix is optional and defaults to
    /// `DEFAULT_SERVER_PORT`).
    #[serde(rename = "s")]
    pub server: String,
    /// Room code — `join` only.
    #[serde(rename = "c", default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Password — always serialized, but may be empty when the server has
    /// none.
    #[serde(rename = "p", default)]
    pub password: String,
    /// Username — required for `host`, optional for `join` (prompted later
    /// by the app when missing).
    #[serde(rename = "u", default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Local path of the `.coquerythmo` project to open — `host` only.
    #[serde(rename = "j", default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

impl ProtocolPayload {
    pub fn host(
        server: &str,
        username: impl Into<String>,
        password: impl Into<String>,
        project: impl Into<String>,
    ) -> Self {
        Self {
            kind: KIND_HOST.to_string(),
            server: server.trim().to_string(),
            code: None,
            password: password.into(),
            username: Some(username.into()).filter(|u| !u.trim().is_empty()),
            project: Some(project.into()).filter(|p| !p.trim().is_empty()),
        }
    }

    pub fn join(server: &str, password: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            kind: KIND_JOIN.to_string(),
            server: server.trim().to_string(),
            code: Some(code.into()).filter(|c| !c.trim().is_empty()),
            password: password.into(),
            username: None,
            project: None,
        }
    }

    pub fn kind(&self) -> Option<ProtocolKind> {
        match self.kind.as_str() {
            KIND_HOST => Some(ProtocolKind::Host),
            KIND_JOIN => Some(ProtocolKind::Join),
            _ => None,
        }
    }

    /// Serialize to a URI whose case-sensitive payload is in the path.
    ///
    /// Keeping the payload out of the host is important: URI clients are
    /// allowed to lowercase hosts, while base64 is case-sensitive.
    pub fn to_url(&self) -> String {
        let json = serde_json::to_string(self).expect("ProtocolPayload must serialize");
        format!("{PROTOCOL_PATH_PREFIX}{}", URL_SAFE_NO_PAD.encode(json))
    }

    /// Decode a protocol URI back into its payload. The old
    /// `coquerythmo://base64url` host form remains accepted for links that
    /// arrive without being normalized by another application.
    pub fn from_url(url: &str) -> Option<Self> {
        let url = url.trim();
        let encoded = url
            .strip_prefix(PROTOCOL_PATH_PREFIX)
            .or_else(|| url.strip_prefix(PROTOCOL_PREFIX))?;
        if encoded.is_empty() {
            return None;
        }
        let bytes = URL_SAFE_NO_PAD.decode(encoded).ok()?;
        let payload: ProtocolPayload = serde_json::from_slice(&bytes).ok()?;
        // Reject documents whose discriminator is not one of ours.
        payload.kind()?;
        Some(payload)
    }

    /// Extract `(host, port)`, defaulting the port to `DEFAULT_SERVER_PORT`
    /// when omitted. Supports `host`, `host:port`, `[v6]`, `[v6]:port` and
    /// bare IPv6 addresses (no bracket → no port parsing).
    pub fn server_endpoint(&self) -> (String, u16) {
        split_host_port(self.server.trim())
    }

    /// True when the mandatory fields for the declared kind are present.
    /// For `join`, the username is not required (prompted interactively).
    pub fn is_valid(&self) -> bool {
        if self.server.trim().is_empty() {
            return false;
        }
        match self.kind() {
            Some(ProtocolKind::Host) => {
                self.username
                    .as_deref()
                    .is_some_and(|u| !u.trim().is_empty())
                    && self
                        .project
                        .as_deref()
                        .is_some_and(|p| !p.trim().is_empty())
            }
            Some(ProtocolKind::Join) => self.code.as_deref().is_some_and(|c| !c.trim().is_empty()),
            None => false,
        }
    }
}

fn split_host_port(input: &str) -> (String, u16) {
    // Bracketed IPv6: `[addr]` optionally followed by `:port`.
    if let Some(rest) = input.strip_prefix('[') {
        if let Some(closing) = rest.find(']') {
            let host = rest[..closing].to_string();
            let port = rest[closing + 1..]
                .strip_prefix(':')
                .and_then(|port_str| port_str.parse::<u16>().ok())
                .unwrap_or(DEFAULT_SERVER_PORT);
            return (host, port);
        }
    }
    // Bare IPv6 (multiple `:`) — no port can be attached without brackets.
    if input.matches(':').count() != 1 {
        return (input.to_string(), DEFAULT_SERVER_PORT);
    }
    if let Some((host, port_str)) = input.rsplit_once(':') {
        if !host.is_empty() {
            if let Ok(port) = port_str.parse::<u16>() {
                return (host.to_string(), port);
            }
        }
    }
    (input.to_string(), DEFAULT_SERVER_PORT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_payload_roundtrips() {
        let payload = ProtocolPayload::host(
            "38.87.117.194:9050",
            "Alice",
            "s3cret",
            r"C:\projects\ep01.coquerythmo",
        );
        assert!(payload.is_valid());
        let url = payload.to_url();
        assert!(url.starts_with(PROTOCOL_PREFIX));
        assert!(url.starts_with(PROTOCOL_PATH_PREFIX));
        let decoded = ProtocolPayload::from_url(&url).expect("valid host URI");
        assert_eq!(decoded.kind(), Some(ProtocolKind::Host));
        assert_eq!(
            decoded.server_endpoint(),
            ("38.87.117.194".to_string(), 9050)
        );
        assert_eq!(decoded.username.as_deref(), Some("Alice"));
        assert_eq!(decoded.password, "s3cret");
        assert_eq!(
            decoded.project.as_deref(),
            Some(r"C:\projects\ep01.coquerythmo")
        );
    }

    #[test]
    fn join_payload_roundtrips_and_defaults_to_default_port() {
        let payload = ProtocolPayload::join("example.com", "password", "XKCD42");
        assert!(payload.is_valid());
        let decoded = ProtocolPayload::from_url(&payload.to_url()).expect("valid join URI");
        assert_eq!(decoded.kind(), Some(ProtocolKind::Join));
        assert_eq!(
            decoded.server_endpoint(),
            ("example.com".to_string(), DEFAULT_SERVER_PORT)
        );
        assert_eq!(decoded.code.as_deref(), Some("XKCD42"));
        assert!(decoded.username.is_none());
    }

    #[test]
    fn custom_port_is_preserved() {
        let payload = ProtocolPayload::host("example.com:7000", "u", "p", "p");
        assert_eq!(payload.server_endpoint(), ("example.com".to_string(), 7000));
    }

    #[test]
    fn bracketed_ipv6_with_port() {
        let payload = ProtocolPayload::host("[2001:db8::1]:9050", "u", "p", "p");
        assert_eq!(payload.server_endpoint(), ("2001:db8::1".to_string(), 9050));
    }

    #[test]
    fn bare_ipv6_defaults_to_default_port() {
        let payload = ProtocolPayload::host("2001:db8::1", "u", "p", "p");
        assert_eq!(
            payload.server_endpoint(),
            ("2001:db8::1".to_string(), DEFAULT_SERVER_PORT)
        );
    }

    #[test]
    fn join_requires_room_code() {
        let mut payload = ProtocolPayload::join("example.com", "p", "ABCD");
        payload.code = None;
        assert!(!payload.is_valid());
    }

    #[test]
    fn host_requires_username_and_project() {
        let payload = ProtocolPayload::host("example.com", "", "p", "proj");
        assert!(!payload.is_valid());
        let payload = ProtocolPayload::host("example.com", "u", "p", "");
        assert!(!payload.is_valid());
        let payload = ProtocolPayload::host("example.com", "u", "p", "proj");
        assert!(payload.is_valid());
    }

    #[test]
    fn unknown_kind_is_invalid() {
        let payload = ProtocolPayload {
            kind: "x".into(),
            server: "1.2.3.4".into(),
            code: None,
            password: "p".into(),
            username: None,
            project: None,
        };
        assert!(payload.kind().is_none());
        assert!(!payload.is_valid());
    }

    #[test]
    fn malformed_urls_are_rejected() {
        assert!(ProtocolPayload::from_url("").is_none());
        assert!(ProtocolPayload::from_url("coquerythmo://").is_none());
        assert!(ProtocolPayload::from_url("coquerythmo:// ").is_none());
        assert!(ProtocolPayload::from_url("https://example.com").is_none());
        // Bad base64
        assert!(ProtocolPayload::from_url("coquerythmo://!!!").is_none());
        // Valid base64, but not JSON ("john" encoded).
        let not_json = format!("{}{}", PROTOCOL_PREFIX, URL_SAFE_NO_PAD.encode("john"));
        assert!(ProtocolPayload::from_url(&not_json).is_none());
        // Valid JSON but unknown kind.
        let bad_kind = format!(
            "{}{}",
            PROTOCOL_PREFIX,
            URL_SAFE_NO_PAD.encode(r#"{"k":"x","s":"1.2.3.4","c":"ABCD","p":"pw"}"#)
        );
        assert!(ProtocolPayload::from_url(&bad_kind).is_none());
    }

    #[test]
    fn join_links_do_not_embed_username() {
        let payload = ProtocolPayload::join("example.com", "p", "ABCD");
        assert!(payload.is_valid());
        let decoded = ProtocolPayload::from_url(&payload.to_url()).expect("valid join URI");
        assert!(decoded.username.is_none());
    }

    #[test]
    fn reported_join_link_is_valid() {
        let url = "coquerythmo://eyJrIjoiaiIsInMiOiIzOC44Ny4xMTcuMTk0OjkwNTAiLCJjIjoiUTBLSlVJIiwicCI6IiIsInUiOiJGdW5reUZpZ2h0In0";
        let payload = ProtocolPayload::from_url(url).expect("reported link should decode");
        assert!(payload.is_valid());
        assert_eq!(payload.kind(), Some(ProtocolKind::Join));
        assert_eq!(payload.code.as_deref(), Some("Q0KJUI"));
    }
}
