#![cfg(target_os = "linux")]

use nexus_net::{
    backend::{linux::LinuxBackend, NetworkBackend},
    config::Config,
    model::Snapshot,
    tools::{self, Tool},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn live_snapshot_reads_real_loopback_and_counters() {
    let snapshot = LinuxBackend.snapshot().await.unwrap();
    let lo = snapshot
        .interfaces
        .iter()
        .find(|i| i.name == "lo")
        .expect("Linux loopback");
    assert!(
        lo.addresses.iter().any(|a| a.starts_with("127.0.0.1/")),
        "Snapshot: {snapshot:?}"
    );
    assert!(snapshot.timestamp.timestamp() > 0);
    assert!(snapshot.capabilities.iter().any(|c| c.command == "ip"));
}

#[tokio::test]
async fn tcp_connects_to_local_test_server() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        listener.accept().await.unwrap();
    });
    let result = tools::execute(
        Tool::Tcp {
            host: "127.0.0.1".into(),
            port,
        },
        Snapshot::default(),
        Config::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.rows.len(), 1);
    assert!(result.rows[0][2].contains("ms"));
    task.await.unwrap();
}

#[tokio::test]
async fn udp_distinguishes_response_from_no_reply() {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let port = socket.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        let mut buf = [0u8; 256];
        let (n, peer) = socket.recv_from(&mut buf).await.unwrap();
        socket.send_to(&buf[..n], peer).await.unwrap();
    });
    let result = tools::execute(
        Tool::Udp {
            host: "127.0.0.1".into(),
            port,
        },
        Snapshot::default(),
        Config::default(),
    )
    .await
    .unwrap();
    assert!(result.rows[0][1].contains("bytes returned"));
    task.await.unwrap();
}

#[tokio::test]
async fn http_inspector_uses_real_local_head_request_and_redacts_secrets() {
    if !nexus_net::command::available("curl") {
        return;
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 2048];
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        assert!(String::from_utf8_lossy(&buf[..n]).starts_with("HEAD /"));
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nSet-Cookie: session=private123\r\nX-API-Key: supersecret\r\nServer: Nexus-Test\r\nConnection: close\r\n\r\n").await.unwrap();
    });
    let r = tools::execute(
        Tool::Http {
            url: format!("http://127.0.0.1:{port}/"),
        },
        Snapshot::default(),
        Config::default(),
    )
    .await
    .unwrap();
    server.await.unwrap();
    assert_eq!(r.metrics["http_code"], "200");
    assert!(r.metrics.contains_key("TCP connect"));
    let json = serde_json::to_string(&r).unwrap();
    assert!(!json.contains("private123"));
    assert!(!json.contains("supersecret"));
}

#[test]
fn nethogs_trace_normalizes_final_refresh_only() {
    let out="Refreshing:\n/usr/bin/old/123/1000 1.0 2.0\nRefreshing:\n/usr/bin/browser/456/1000 10.0 20.0\n";
    let rows = tools::parse_process_traffic(out);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][1], "456");
    assert_eq!(rows[0][3], "20.0 KiB/s");
}

#[tokio::test]
async fn http_404_is_a_reachable_application_error() {
    if !nexus_net::command::available("curl") {
        return;
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0; 2048];
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        socket
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
    });
    let r = tools::execute(
        Tool::Http {
            url: format!("http://127.0.0.1:{port}/missing"),
        },
        Snapshot::default(),
        Config::default(),
    )
    .await
    .unwrap();
    task.await.unwrap();
    assert_eq!(r.metrics["http_code"], "404");
    assert!(r.findings[0].evidence.contains("connection worked"));
    assert!(!r.findings[0].title.contains("outage"));
}
