use nexus_net::{
    app::{App, Effect, Modal, Page},
    config::{Config, History},
    control::{self, Change},
    integrations::{self, Secret},
    tools::Tool,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[derive(Clone, Debug)]
struct Request {
    method: String,
    path: String,
    headers: String,
    body: Value,
}
struct Api {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
    blocking: Arc<Mutex<bool>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Api {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn api(redirect: bool) -> Api {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = requests.clone();
    let blocking = Arc::new(Mutex::new(true));
    let state = blocking.clone();
    let task = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let recorded = recorded.clone();
            let state = state.clone();
            tokio::spawn(async move {
                let mut bytes = Vec::new();
                let mut chunk = [0; 4096];
                let header_end;
                loop {
                    let n = socket.read(&mut chunk).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&chunk[..n]);
                    if let Some(i) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                        header_end = i + 4;
                        break;
                    }
                    assert!(bytes.len() < 65536);
                }
                let headers = String::from_utf8_lossy(&bytes[..header_end]).to_string();
                let first = headers.lines().next().unwrap();
                let mut words = first.split_whitespace();
                let method = words.next().unwrap().to_string();
                let path = words.next().unwrap().to_string();
                let len = headers
                    .lines()
                    .find_map(|s| {
                        let (k, v) = s.split_once(':')?;
                        (k.eq_ignore_ascii_case("content-length"))
                            .then(|| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                while bytes.len() < header_end + len {
                    let n = socket.read(&mut chunk).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&chunk[..n]);
                }
                let body: Value = serde_json::from_slice(&bytes[header_end..header_end + len])
                    .unwrap_or(Value::Null);
                recorded.lock().unwrap().push(Request {
                    method: method.clone(),
                    path: path.clone(),
                    headers: headers.clone(),
                    body: body.clone(),
                });
                let (status, value) = if redirect {
                    (302, json!({}))
                } else if path == "/api/auth" {
                    if method == "DELETE" {
                        (200, json!({}))
                    } else if body["password"] == "wrong" {
                        (401, json!({"error":{"message":"do not echo secrets"}}))
                    } else {
                        (
                            200,
                            json!({"session":{"valid":true,"sid":"test-session-secret","validity":300}}),
                        )
                    }
                } else if !headers
                    .to_lowercase()
                    .contains("x-ftl-sid: test-session-secret")
                {
                    (401, json!({}))
                } else {
                    match (method.as_str(), path.as_str()) {
                        ("GET", "/api/stats/summary") => (
                            200,
                            json!({"queries":{"total":100,"blocked":23,"percent_blocked":23.0,"frequency":1.2},"clients":{"active":3},"gravity":{"domains_being_blocked":9000}}),
                        ),
                        ("GET", "/api/history") => (
                            200,
                            json!({"history":[{"timestamp":1000,"total":10,"blocked":2},{"timestamp":1600,"total":20,"blocked":5}]}),
                        ),
                        ("GET", "/api/dns/blocking") => (
                            200,
                            json!({"blocking":if *state.lock().unwrap(){"enabled"}else{"disabled"},"timer":null}),
                        ),
                        ("POST", "/api/dns/blocking") => {
                            *state.lock().unwrap() = body["blocking"].as_bool().unwrap();
                            (
                                200,
                                json!({"blocking":if *state.lock().unwrap(){"enabled"}else{"disabled"},"timer":body["timer"]}),
                            )
                        }
                        _ => (404, json!({})),
                    }
                };
                let bytes = serde_json::to_vec(&value).unwrap();
                let location = if status == 302 {
                    "Location: http://127.0.0.1:1/stolen\r\n"
                } else {
                    ""
                };
                let response=format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{location}Connection: close\r\n\r\n",bytes.len());
                socket.write_all(response.as_bytes()).await.unwrap();
                socket.write_all(&bytes).await.unwrap();
            });
        }
    });
    Api {
        url,
        requests,
        blocking,
        task,
    }
}
#[test]
fn structured_tailnet_preserves_unknown_and_normalizes_paths() {
    let t=integrations::parse_tailnet(&json!({"BackendState":"Running","CurrentTailnet":{"Name":"example","MagicDNSSuffix":"example.ts.net"},"TailscaleIPs":["100.64.0.1"],"Peer":{"a":{"DNSName":"relay.example.","TailscaleIPs":["100.64.0.2"],"Relay":"fra","Online":true,"ExitNode":true,"ExitNodeOption":true},"b":{"DNSName":"direct.example.","CurAddr":"192.0.2.3:1234"}}})).unwrap();
    assert_eq!(t.exit_node, "relay.example");
    assert!(t.peers[0].path.contains("DERP"));
    assert_eq!(t.peers[1].online, None);
    assert!(t.peers[1].path.starts_with("direct"));
    let r = integrations::tailnet_result(t);
    assert_eq!(r.rows.len(), 2);
    assert!(r.tailscale.is_some());
    assert!(integrations::parse_tailnet(&json!({"Peer":{}})).is_err());
}
#[test]
fn service_urls_reject_embedded_credentials_and_ambiguous_paths() {
    assert!(integrations::endpoint("https://user:secret@pi.hole").is_err());
    assert!(integrations::endpoint("file:///etc/passwd").is_err());
    assert!(integrations::endpoint("https://pi.hole?key=secret").is_err());
    assert_eq!(
        integrations::endpoint("https://pi.hole/api/")
            .unwrap()
            .as_str(),
        "https://pi.hole/api/"
    );
}
#[test]
fn credentials_never_serialize_or_debug() {
    let tool = Tool::Pihole {
        url: "https://pi.hole".into(),
        password: Secret::new("super-secret-password".into()),
    };
    assert!(!serde_json::to_string(&tool)
        .unwrap()
        .contains("super-secret"));
    assert!(!format!("{tool:?}").contains("super-secret"));
}
#[tokio::test]
async fn authenticated_status_normalizes_and_logs_out() {
    let api = api(false).await;
    let r = integrations::pihole_status(&api.url, &Secret::new("test-password".into()))
        .await
        .unwrap();
    let p = r.pihole.as_ref().unwrap();
    assert_eq!((p.queries, p.blocked, p.clients), (100, 23, 3));
    assert_eq!(p.history.len(), 2);
    let serialized = serde_json::to_string(&r).unwrap();
    assert!(!serialized.contains("test-password"));
    assert!(!serialized.contains("test-session-secret"));
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if api
                .requests
                .lock()
                .unwrap()
                .iter()
                .any(|r| r.method == "DELETE")
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let requests = api.requests.lock().unwrap();
    assert_eq!(requests[0].body["password"], "test-password");
    assert!(requests
        .iter()
        .all(|r| !r.path.contains("secret") && !r.path.contains('?')));
    assert!(requests
        .iter()
        .filter(|r| r.path != "/api/auth")
        .all(|r| r.headers.to_lowercase().contains("x-ftl-sid")));
}
#[tokio::test]
async fn api_redirects_and_authentication_failures_are_not_followed() {
    let redirect = api(true).await;
    let error = integrations::pihole_status(&redirect.url, &Secret::new("secret".into()))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("302"));
    assert!(!error.contains("secret"));
    assert_eq!(redirect.requests.lock().unwrap().len(), 1);
    let auth = api(false).await;
    let error = integrations::pihole_status(&auth.url, &Secret::new("wrong".into()))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("401"));
    assert!(!error.contains("wrong"));
}
#[tokio::test]
async fn blocking_controls_only_apply_after_preview_and_check_for_races() {
    let api = api(false).await;
    let change = Change::Pihole {
        url: api.url.clone(),
        password: Secret::new("test-password".into()),
        enabled: false,
    };
    let plan = control::prepare(change, &Default::default()).await.unwrap();
    assert!(*api.blocking.lock().unwrap());
    assert!(!api
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|r| r.method == "POST" && r.path == "/api/dns/blocking"));
    assert!(plan.summary.join(" ").contains("60"));
    control::apply(&plan).await.unwrap();
    assert!(!*api.blocking.lock().unwrap());
    assert!(api
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|r| r.method == "POST"
            && r.path == "/api/dns/blocking"
            && r.body == json!({"blocking":false,"timer":60})));
    control::apply(&plan.revert().unwrap()).await.unwrap();
    assert!(*api.blocking.lock().unwrap());
    *api.blocking.lock().unwrap() = false;
    let before = api
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r.path == "/api/dns/blocking" && r.method == "POST")
        .count();
    let result = control::apply(&plan).await.unwrap();
    assert!(result.notes.join(" ").contains("changed since preview"));
    assert_eq!(
        before,
        api.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.path == "/api/dns/blocking" && r.method == "POST")
            .count()
    );
}
#[test]
fn service_pages_keep_selection_and_do_not_replace_internet_diagnosis() {
    let mut app = App::new(Config::default(), History::default());
    app.page = Page::Tailscale;
    app.selected = 1;
    app.filter = "gateway".into();
    app.finish_result(integrations::tailnet_result(integrations::Tailnet {
        state: "Stopped".into(),
        ..Default::default()
    }));
    assert_eq!(app.page, Page::Tailscale);
    assert_eq!(app.selected, 1);
    assert_eq!(app.filter, "gateway");
    assert!(app.assessment.is_none());
    assert!(matches!(app.dispatch("pihole-connect"), Effect::None));
    assert!(matches!(app.modal, Some(Modal::Form(_))));
}
#[test]
fn navigation_wraps_across_all_seventeen_pages() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = App::new(Config::default(), History::default());
    app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE));
    assert_eq!(app.page, Page::Pihole);
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.page, Page::Dashboard);
}
