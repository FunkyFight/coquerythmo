//! Correlated, cancellable request/reply transport. A successful socket write
//! is not a successful operation; only a server reply completes a request.

use rust_socketio::RawClient;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc, Arc, Mutex,
};
use std::time::{Duration, Instant};

pub const PROTOCOL_VERSION: u32 = 2;

struct Shared {
    pending: Mutex<HashMap<String, mpsc::Sender<Value>>>,
    client: Mutex<Option<RawClient>>,
    stopped: AtomicBool,
    sequence: AtomicU64,
    nonce: u128,
}

#[derive(Clone)]
pub struct RpcClient {
    shared: Arc<Shared>,
}

impl RpcClient {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared {
                pending: Mutex::new(HashMap::new()),
                client: Mutex::new(None),
                stopped: AtomicBool::new(false),
                sequence: AtomicU64::new(0),
                nonce: rand::random(),
            }),
        }
    }

    /// Called only after room admission, never just after WebSocket connect.
    pub fn set_client(&self, client: Option<RawClient>) {
        let mut current = self.shared.client.lock().unwrap();
        if client.is_some() && self.is_stopped() {
            return;
        }
        *current = client;
    }

    pub fn stop(&self) {
        self.shared.stopped.store(true, Ordering::Release);
        self.set_client(None);
    }

    pub fn is_stopped(&self) -> bool {
        self.shared.stopped.load(Ordering::Acquire)
    }

    pub fn is_ready(&self) -> bool {
        self.shared.client.lock().unwrap().is_some()
    }

    pub fn receive(&self, reply: Value) {
        let Some(id) = reply.get("id").and_then(Value::as_str) else {
            return;
        };
        if let Some(sender) = self.shared.pending.lock().unwrap().remove(id) {
            let _ = sender.send(reply);
        }
    }

    pub fn call(&self, method: &str, body: Value) -> Result<Value, String> {
        self.call_cancellable(method, body, &AtomicBool::new(false))
    }

    pub fn call_cancellable(
        &self,
        method: &str,
        body: Value,
        cancelled: &AtomicBool,
    ) -> Result<Value, String> {
        let id = format!(
            "{:032x}_{}",
            self.shared.nonce,
            self.shared.sequence.fetch_add(1, Ordering::Relaxed)
        );
        let request = serde_json::json!({ "id": id, "method": method, "body": body });
        let result = self.wait_for_reply(&id, request, cancelled);
        self.shared.pending.lock().unwrap().remove(&id);
        result
    }

    fn wait_for_reply(
        &self,
        id: &str,
        request: Value,
        cancelled: &AtomicBool,
    ) -> Result<Value, String> {
        let (tx, rx) = mpsc::channel();
        let deadline = Instant::now() + Duration::from_secs(300);
        let mut next_send = Instant::now();
        loop {
            if self.is_stopped() {
                return Err("network session closed".into());
            }
            if cancelled.load(Ordering::Acquire) {
                return Err("network operation cancelled".into());
            }
            if Instant::now() >= deadline {
                return Err("server acknowledgement timed out".into());
            }
            if Instant::now() >= next_send {
                let client = self.shared.client.lock().unwrap().clone();
                if let Some(client) = client {
                    self.shared
                        .pending
                        .lock()
                        .unwrap()
                        .insert(id.to_owned(), tx.clone());
                    match client.emit("protocol_request", request.clone()) {
                        Ok(()) => next_send = Instant::now() + Duration::from_secs(5),
                        Err(_) => next_send = Instant::now() + Duration::from_millis(250),
                    }
                }
            }
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(reply) if reply["ok"] == true => return Ok(reply["body"].clone()),
                Ok(reply) if reply["retryable"] == true => {
                    next_send = Instant::now() + Duration::from_millis(500)
                }
                Ok(reply) => {
                    return Err(reply["error"]
                        .as_str()
                        .unwrap_or("invalid server reply")
                        .to_owned())
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("reply channel closed".into())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_are_correlated_and_late_duplicates_are_ignored() {
        let rpc = RpcClient::new();
        let (one_tx, one_rx) = mpsc::channel();
        let (two_tx, two_rx) = mpsc::channel();
        rpc.shared
            .pending
            .lock()
            .unwrap()
            .insert("one".into(), one_tx);
        rpc.shared
            .pending
            .lock()
            .unwrap()
            .insert("two".into(), two_tx);
        rpc.receive(serde_json::json!({"id":"two", "ok":true, "body":{"index":2}}));
        assert_eq!(two_rx.try_recv().unwrap()["body"]["index"], 2);
        assert!(one_rx.try_recv().is_err());
        rpc.receive(serde_json::json!({"id":"missing", "ok":true}));
        rpc.receive(serde_json::json!({"id":"two", "ok":true}));
        assert!(two_rx.try_recv().is_err());
    }

    #[test]
    fn stopping_a_session_cancels_requests_waiting_for_reconnection() {
        let rpc = RpcClient::new();
        let worker = rpc.clone();
        let thread = std::thread::spawn(move || worker.call("ping", serde_json::json!({})));
        rpc.stop();
        assert_eq!(
            thread.join().unwrap().unwrap_err(),
            "network session closed"
        );
        assert!(rpc.shared.pending.lock().unwrap().is_empty());
    }
}
