//! Real Rust ↔ Node interoperability, run with:
//! cargo test --lib network::integration_tests -- --ignored --test-threads=1

use super::*;
use serde_json::json;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Instant;

struct Server(Child);
impl Server {
    fn start() -> (Self, u16) {
        let mut child = Command::new("node")
            .arg("test/rust_client_server.js")
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("server"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("Node and server dependencies are required");
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let port = loop {
            let mut line = String::new();
            assert!(
                reader.read_line(&mut line).unwrap() > 0,
                "server did not start"
            );
            if let Some(port) = line.trim().strip_prefix("TEST_PORT:") {
                break port.parse().unwrap();
            }
        };
        thread::spawn(move || {
            for line in reader.lines() {
                if line.is_err() {
                    break;
                }
            }
        });
        (Self(child), port)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.0.stdin.take();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if self.0.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_message(
    network: &NetworkClient,
    predicate: impl Fn(&IncomingMessage) -> bool,
) -> IncomingMessage {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        while let Some(message) = network.try_recv() {
            if let IncomingMessage::Error(error) | IncomingMessage::ConnectionFailed(error) =
                &message
            {
                // Engine.IO reports the deliberate transport cut as an error
                // before delivering Close and reconnecting.
                if error != "EngineIO Error" {
                    panic!("network error: {error}");
                }
            }
            if predicate(&message) {
                return message;
            }
        }
        assert!(Instant::now() < deadline, "network event timed out");
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_state(network: &NetworkClient) -> replication::ReceivedState {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let replication = network.replication.as_ref().unwrap();
        if let Some(error) = replication.take_error() {
            panic!("replication error: {error}");
        }
        if let Some(received) = replication.take_received() {
            return received;
        }
        assert!(Instant::now() < deadline, "replication timed out");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "requires Node and npm install in server/"]
fn mute_and_solo_reach_an_actor_before_the_first_recording() {
    use crate::recording::RecordingOperation;

    let (_server, port) = Server::start();
    let mut director = NetworkClient::new();
    director.connect_and_send(
        "127.0.0.1",
        port,
        "",
        Packet::CreateRoom {
            username: "DA".into(),
            project_huuid: "empty-recording".into(),
        },
    );
    let IncomingMessage::Packet(Packet::RoomCreated { code }) =
        wait_message(&director, |message| {
            matches!(message, IncomingMessage::Packet(Packet::RoomCreated { .. }))
        })
    else {
        unreachable!()
    };

    let session = crate::application::project_service::ProjectSession::new();
    let mut project = session.recording_project;
    let mut log = session.recording_transactions;
    let track_id = project.tracks().next().unwrap().id;
    let mut replica = project.clone();
    let mut replica_log = log.clone();
    director.initialize_created_room(
        &code,
        &RecordingPreparePayload {
            project: project.clone(),
            transactions: log.clone(),
            current_frame: 0,
            capture_target: None,
        },
    );

    let mut actor = NetworkClient::new();
    actor.connect_and_send(
        "127.0.0.1",
        port,
        "",
        Packet::JoinRoom {
            code,
            username: "Actor".into(),
            project_huuid: Some("empty-recording".into()),
            project_mode: crate::protocol::InvitationProjectMode::None,
        },
    );
    wait_message(&actor, |message| {
        matches!(message, IncomingMessage::Packet(Packet::RoomJoined { .. }))
    });

    // The actor has the same saved project; no capture or resync initializes the chain.
    for operation in [
        RecordingOperation::SetTrackMuted {
            track_id,
            muted: true,
        },
        RecordingOperation::SetTrackSolo {
            track_id,
            solo: true,
        },
        RecordingOperation::SetTrackMuted {
            track_id,
            muted: false,
        },
        RecordingOperation::SetTrackSolo {
            track_id,
            solo: false,
        },
    ] {
        let transaction = log.append_and_apply(&mut project, operation).unwrap();
        director.send_recording_transaction(transaction);
        let IncomingMessage::RecordingTransaction(received) = wait_message(&actor, |message| {
            matches!(message, IncomingMessage::RecordingTransaction(_))
        }) else {
            unreachable!()
        };
        assert_eq!(&received, transaction);
        replica_log
            .append_received_and_apply(&mut replica, received)
            .unwrap();
        assert_eq!(replica, project);
        assert_eq!(project.clips().len(), 0);
    }
    director.disconnect();
    actor.disconnect();
}

#[test]
#[ignore = "requires Node and npm install in server/"]
fn rust_clients_transfer_projects_replicate_edits_and_rejoin_the_same_room() {
    let (_server, port) = Server::start();
    let root = std::env::temp_dir().join(format!(
        "coquerythmo-network-test-{:032x}",
        rand::random::<u128>()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let mut director = NetworkClient::new();
    director.connect_and_send(
        "127.0.0.1",
        port,
        "",
        Packet::CreateRoom {
            username: "DA".into(),
            project_huuid: "project".into(),
        },
    );
    let IncomingMessage::Packet(Packet::RoomCreated { code }) =
        wait_message(&director, |message| {
            matches!(message, IncomingMessage::Packet(Packet::RoomCreated { .. }))
        })
    else {
        unreachable!()
    };
    let mut actor = NetworkClient::new();
    actor.connect_and_send(
        "127.0.0.1",
        port,
        "",
        Packet::JoinRoom {
            code: code.clone(),
            username: "Actor".into(),
            project_huuid: None,
            project_mode: crate::protocol::InvitationProjectMode::None,
        },
    );
    wait_message(&actor, |message| {
        matches!(message, IncomingMessage::Packet(Packet::RoomJoined { .. }))
    });

    let mut project = crate::project::Project::new();
    let line_id = project.add_line(5, 30, 0.25);
    project.get_line_mut(line_id).unwrap().text = "premier texte".into();
    // Exercise chunked state publication and snapshot reads, not only tiny deltas.
    for index in 0..400 {
        let id = project.add_line(index * 100, 30, 0.5);
        project.get_line_mut(id).unwrap().text = "texte long ".repeat(400);
    }
    let transport = rythmo::Transport {
        frame: 25,
        playing: false,
        fps: 24.0,
        instrumental: false,
        rythmo: true,
    };
    let view = rythmo::DirectorView {
        selection: None,
        compact_empty_tracks: true,
        active_stroke: None,
        font_family: "sans-serif".into(),
    };
    director.replication.as_mut().unwrap().offer(
        &project,
        "project",
        transport.clone(),
        view.clone(),
    );
    let first = wait_state(&actor);
    let mut remote_project = crate::project::Project::new();
    first.document.apply(&mut remote_project).unwrap();
    assert_eq!(
        remote_project.get_line(line_id).unwrap().text,
        "premier texte"
    );
    assert_eq!(first.frame(), 25);
    assert_eq!(remote_project.lines().count(), 401);
    assert_eq!(first.view.font_family, "sans-serif");

    let source = root.join("project.coquerythmo");
    let bytes = vec![0x5a; crate::file_transfer::FILE_CHUNK_BYTES * 20 + 17];
    std::fs::write(&source, &bytes).unwrap();
    let file =
        crate::file_transfer::FileTransferMetadata::from_path("test_project", &source).unwrap();
    let metadata = ProjectTransferMetadata {
        request_id: file.transfer_id,
        project_huuid: "project".into(),
        file_name: file.file_name,
        total_bytes: file.total_bytes,
        total_chunks: file.total_chunks,
        chunk_size: file.chunk_size,
        sha1: file.sha1,
    };
    director.request_project_transfer(&metadata);
    wait_message(&director, |message| {
        matches!(message, IncomingMessage::ProjectTransferReady(_))
    });
    // The server writes the block, loses its acknowledgement, then drops the
    // socket. Retrying must resume without appending this block a second time.
    director
        ._client
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .emit(
            "test_drop_reply",
            json!({"method":"project_write", "index":3}),
        )
        .unwrap();
    director
        .send_project_file(source, metadata.clone())
        .recv_timeout(Duration::from_secs(15))
        .unwrap()
        .unwrap();
    wait_message(&actor, |message| {
        matches!(message, IncomingMessage::ProjectTransferRequest(_))
    });
    actor
        ._client
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .emit(
            "test_drop_reply",
            json!({"method":"project_read", "index":2}),
        )
        .unwrap();
    actor.download_project(metadata, root.join("download"));
    let IncomingMessage::ProjectDownloadFinished { result, .. } = wait_message(&actor, |message| {
        matches!(message, IncomingMessage::ProjectDownloadFinished { .. })
    }) else {
        unreachable!()
    };
    assert_eq!(std::fs::read(result.unwrap()).unwrap(), bytes);

    // Drain the upload reconnection events before testing a fresh transport cut.
    while director.try_recv().is_some() {}

    director
        ._client
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .emit("test_drop_reply", json!({"method":"event"}))
        .unwrap();
    director
        .rpc
        .call(
            "event",
            json!({"event":"recording_playback", "payload":{"frame":77, "playing":false}}),
        )
        .unwrap();
    wait_message(
        &actor,
        |message| matches!(message, IncomingMessage::RecordingPlayback(value) if value.frame == 77),
    );
    while let Some(message) = actor.try_recv() {
        assert!(
            !matches!(message, IncomingMessage::RecordingPlayback(value) if value.frame == 77),
            "a lost ACK replayed the command"
        );
    }
    wait_message(&director, |message| {
        matches!(message, IncomingMessage::Disconnected(_))
    });
    let IncomingMessage::Packet(Packet::RoomJoined {
        code: rejoined,
        role,
        ..
    }) = wait_message(&director, |message| {
        matches!(message, IncomingMessage::Packet(Packet::RoomJoined { .. }))
    })
    else {
        unreachable!()
    };
    assert_eq!(rejoined, code);
    assert_eq!(role, "admin");
    project.get_line_mut(line_id).unwrap().text = "après reconnexion".into();
    director
        .replication
        .as_mut()
        .unwrap()
        .offer(&project, "project", transport, view);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let updated = wait_state(&actor);
        updated.document.apply(&mut remote_project).unwrap();
        if remote_project.get_line(line_id).unwrap().text == "après reconnexion" {
            break;
        }
        assert!(Instant::now() < deadline);
    }
    actor.disconnect();
    director.disconnect();
    std::fs::remove_dir_all(root).unwrap();
}
