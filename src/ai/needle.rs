//! Process bridge to the Needle 3 engine.
//!
//! The engine ships as `needle.exe` next to its model `needle3.cact`. Each
//! query starts the engine in `--serve` mode with the tools chosen for that
//! query, posts the text over HTTP on the loopback and stops it: the engine
//! cannot change its tool set once started, and passing the text on the
//! command line would lose every accented character (the engine reads its
//! arguments in the ANSI code page). A request takes about 1.5 s on a
//! laptop: one second to load the model, the rest to answer.

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::Deserialize;

#[cfg(windows)]
const EXECUTABLE: &str = "needle.exe";
#[cfg(not(windows))]
const EXECUTABLE: &str = "needle";
const MODEL: &str = "needle3.cact";
/// Loading the model on a slow disk can take a while; past this the engine is
/// considered dead.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(40);
const ANSWER_TIMEOUT: Duration = Duration::from_secs(60);

/// One function call chosen by the engine.
#[derive(Clone, Debug, Deserialize)]
pub struct ToolCall {
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

/// The engine's own checks on the arguments it produced.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Validation {
    /// `tool.argument` paths whose value does not come from the query.
    #[serde(default)]
    pub ungrounded: Vec<String>,
    /// The query negates the action ("ne sauvegarde pas").
    #[serde(default)]
    pub negation: bool,
}

/// The JSON envelope of an answer.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct NeedleResponse {
    #[serde(default)]
    pub validation: Validation,
    /// `call` when tools were dispatched, `respond` when the engine had no
    /// tool for the request.
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub function_calls: Vec<ToolCall>,
    /// Calls the engine judged too uncertain to dispatch.
    #[serde(default)]
    pub suppressed_calls: Vec<ToolCall>,
    #[serde(default)]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub confidence: Option<f64>,
}

/// Folder holding `needle.exe` and `needle3.cact`.
///
/// Looked up next to the application (`needle3/` subfolder first), in the
/// working directory and, for development builds, in the source tree.
/// `COQUERYTHMO_NEEDLE_DIR` overrides everything.
pub fn needle_dir() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::var_os("COQUERYTHMO_NEEDLE_DIR") {
        candidates.push(PathBuf::from(dir));
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        candidates.push(exe_dir.join("needle3"));
        candidates.push(exe_dir.clone());
        candidates.push(exe_dir.join("src").join("ai").join("needle3"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("needle3"));
        candidates.push(cwd.join("src").join("ai").join("needle3"));
    }
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("ai")
            .join("needle3"),
    );
    candidates
        .into_iter()
        .find(|dir| dir.join(EXECUTABLE).is_file() && dir.join(MODEL).is_file())
}

pub fn is_available() -> bool {
    needle_dir().is_some()
}

/// Kills the engine when the request ends, whatever the path taken.
struct Engine(Child);

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Scratch files of one request (tools and session facts), removed with it.
struct RequestFiles {
    tools: PathBuf,
    system: PathBuf,
}

impl Drop for RequestFiles {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.tools);
        let _ = std::fs::remove_file(&self.system);
    }
}

fn free_port() -> Result<u16, String> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|error| format!("no free port: {error}"))
}

fn write_request_files(tools_json: &str, system: &str) -> Result<RequestFiles, String> {
    let dir = crate::media_binary::installation_temp_dir().join("needle3");
    std::fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tools = dir.join(format!("tools-{stamp}-{}.json", std::process::id()));
    let system_path = dir.join(format!("system-{stamp}-{}.txt", std::process::id()));
    std::fs::write(&tools, tools_json).map_err(|error| format!("{}: {error}", tools.display()))?;
    std::fs::write(&system_path, system)
        .map_err(|error| format!("{}: {error}", system_path.display()))?;
    Ok(RequestFiles {
        tools,
        system: system_path,
    })
}

fn spawn_engine(dir: &Path, files: &RequestFiles, port: u16) -> Result<Engine, String> {
    let mut command = Command::new(dir.join(EXECUTABLE));
    command
        .arg("--model")
        .arg(dir.join(MODEL))
        .arg("--tools")
        .arg(&files.tools)
        .arg("--system")
        .arg(&files.system)
        .arg("--serve")
        .arg("--port")
        .arg(port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flashes behind the application.
        command.creation_flags(0x0800_0000);
    }
    command
        .spawn()
        .map(Engine)
        .map_err(|error| format!("{}: {error}", dir.join(EXECUTABLE).display()))
}

/// Waits for the "serving" line the engine prints once the model is loaded.
fn wait_until_serving(engine: &mut Engine) -> Result<(), String> {
    let Some(stderr) = engine.0.stderr.take() else {
        return Err("engine output unavailable".into());
    };
    let (sender, receiver) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    let mut last_lines: Vec<String> = Vec::new();
    loop {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => {
                if line.contains("serving on") {
                    return Ok(());
                }
                last_lines.push(line);
                if last_lines.len() > 6 {
                    last_lines.remove(0);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(if last_lines.is_empty() {
                    "the engine stopped before serving".to_string()
                } else {
                    last_lines.join(" ")
                });
            }
        }
        if let Ok(Some(status)) = engine.0.try_wait() {
            return Err(format!(
                "the engine exited ({status}): {}",
                last_lines.join(" ")
            ));
        }
        if Instant::now() > deadline {
            return Err("the engine took too long to start".into());
        }
    }
}

/// Answers one query with the given tool schemas (a JSON array) and session
/// facts. Blocking: call it from a worker thread.
pub fn ask(dir: &Path, system: &str, tools_json: &str, query: &str) -> Result<NeedleResponse, String> {
    let files = write_request_files(tools_json, system)?;
    let port = free_port()?;
    let mut engine = spawn_engine(dir, &files, port)?;
    wait_until_serving(&mut engine)?;

    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(ANSWER_TIMEOUT))
            .build(),
    );
    // serde keeps UTF-8 as is (no `\u` escapes), which the engine expects.
    let body = serde_json::json!({ "input": query }).to_string();
    let mut response = agent
        .post(&format!("http://127.0.0.1:{port}/complete"))
        .header("Content-Type", "application/json; charset=utf-8")
        .send(body.as_str())
        .map_err(|error| format!("engine request failed: {error}"))?;
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|error| format!("engine answer unreadable: {error}"))?;
    parse_response(&text)
}

pub fn parse_response(text: &str) -> Result<NeedleResponse, String> {
    let response: NeedleResponse = serde_json::from_str(text.trim())
        .map_err(|error| format!("engine answer malformed ({error}): {text}"))?;
    if !response.success {
        return Err(response
            .error
            .clone()
            .unwrap_or_else(|| "engine error".to_string()));
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_call_envelope() {
        let text = r#"{"type":"call","success":true,"error":null,"function_calls":[{"name":"seek_to_time","arguments":{"time":"00:01:30"}}],"suppressed_calls":[],"reasoning":"x","confidence":0.44,"prefill_tps":1.0,"validation":{"ungrounded":["seek_to_time.time"],"negation":false}}"#;
        let response = parse_response(text).unwrap();
        assert_eq!(response.kind, "call");
        assert_eq!(response.function_calls.len(), 1);
        assert_eq!(response.function_calls[0].name, "seek_to_time");
        assert_eq!(response.function_calls[0].arguments["time"], "00:01:30");
        assert_eq!(response.validation.ungrounded, vec!["seek_to_time.time"]);
        assert!(!response.validation.negation);
    }

    /// Round trip through the real engine: `cargo test --lib ai::needle -- --ignored`.
    #[test]
    #[ignore = "needs needle.exe and needle3.cact"]
    fn engine_round_trip() {
        let dir = needle_dir().expect("engine folder");
        let tools = super::super::retrieval::select_for_query("va à 1 minute 30");
        let tools_json = super::super::tools::schema_json(&tools);
        let response = ask(&dir, "date: 2026-10-15\nuser language: fr", &tools_json, "va à 1 minute 30")
            .expect("engine answer");
        assert_eq!(response.kind, "call", "{response:?}");
        assert_eq!(response.function_calls[0].name, "seek_to_time", "{response:?}");
        // The model sometimes rewrites "1 minute 30" as "130s"; the bridge
        // only guarantees that an accented query survives and that the
        // argument is a position the app can read.
        let time = response.function_calls[0].arguments["time"]
            .as_str()
            .expect("time argument");
        assert!(
            super::super::timecode::parse_frames(time, 25.0).is_some(),
            "{response:?}"
        );
        assert!(
            response.reasoning.as_deref().unwrap_or("").contains("à"),
            "accents must reach the engine intact: {response:?}"
        );
    }

    /// Diagnostic: `NEEDLE_QUERY="va à 1:30" cargo test --lib ai::needle::tests::engine_query -- --ignored --nocapture`
    /// prints the tools chosen for the query and the engine's answer.
    #[test]
    #[ignore = "diagnostic, needs the engine and NEEDLE_QUERY"]
    fn engine_query() {
        let Ok(query) = std::env::var("NEEDLE_QUERY") else {
            return;
        };
        let dir = needle_dir().expect("engine folder");
        for query in query.split("||") {
            let tools = super::super::retrieval::select_for_query(query);
            let names: Vec<&str> = tools.iter().map(|tool| tool.name).collect();
            let response = ask(
                &dir,
                "date: 2026-10-15\nuser language: fr\ncharacters: Chanson, Marie\nselected line: none",
                &super::super::tools::schema_json(&tools),
                query,
            );
            println!("QUERY {query:?}\n  tools {names:?}\n  -> {response:?}");
        }
    }

    #[test]
    fn reports_engine_failures() {
        let text = r#"{"type":"call","success":false,"error":"input exceeds the context window","function_calls":[]}"#;
        assert_eq!(
            parse_response(text).unwrap_err(),
            "input exceeds the context window"
        );
        assert!(parse_response("not json").is_err());
    }
}
