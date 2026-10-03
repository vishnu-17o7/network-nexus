//! Render the real widgets using explicitly labeled documentation fixtures.
use nexus_net::{
    app::{App, Page},
    config::{Config, History},
    diagnosis::{Finding, Severity},
    model::*,
    ui,
};
use ratatui::{backend::TestBackend, Terminal};
use serde_json::json;
use std::{
    path::Path,
    time::{Duration, Instant},
};

fn render(path: &Path, app: &App, width: u16, height: u16) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    let b = terminal.backend().buffer();
    let mut cells = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let c = &b[(x, y)];
            cells.push(json!({"x":x,"y":y,"symbol":c.symbol(),"fg":format!("{:?}",c.fg),"bg":format!("{:?}",c.bg),"bold":c.modifier.contains(ratatui::style::Modifier::BOLD)}));
        }
    }
    let graphics = app.graphics.borrow().preview_assets(path).unwrap();
    std::fs::write(
        path,
        serde_json::to_vec(
            &json!({"width":width,"height":height,"cells":cells,"graphics":graphics}),
        )
        .unwrap(),
    )
    .unwrap();
}
fn main() {
    let output = std::env::args().nth(1).expect("Output directory");
    let dir = Path::new(&output);
    std::fs::create_dir_all(dir).unwrap();
    let mut app = App::new(Config::default(), History::default());
    app.graphics.borrow_mut().mode = nexus_net::graphics::Mode::Preview;
    app.started = Instant::now() - Duration::from_secs(2);
    app.notification = "PREVIEW FIXTURE · example data, not a live network measurement".into();
    app.snapshot = Snapshot {
        timestamp: chrono::Utc::now(),
        primary: Some("wlan0".into()),
        interfaces: vec![Interface {
            name: "wlan0".into(),
            kind: "Wi-Fi".into(),
            state: "up".into(),
            addresses: vec!["192.0.2.24/24".into(), "2001:db8::24/64".into()],
            rx_rate: 825000.0,
            tx_rate: 94000.0,
            ..Default::default()
        }],
        routes: vec![Route {
            destination: "default".into(),
            gateway: "192.0.2.1".into(),
            interface: "wlan0".into(),
            ..Default::default()
        }],
        dns: vec!["192.0.2.1".into()],
        dns_source: "systemd-resolved".into(),
        ..Default::default()
    };
    let findings = vec![
        Finding::new(
            Severity::Error,
            "DoT TLS handshake / verification failed",
            "Example: resolver certificate expired. TCP port 853 accepted a connection.",
            "Check certificate expiry, TLS hostname and system clock. Inspect DNS-over-TLS.",
        ),
        nexus_net::diagnosis::http_status(404),
        Finding::new(
            Severity::Warning,
            "DNS response is slow",
            "Example: system resolver answered in 486 ms.",
            "Compare resolvers and inspect VPN, DoT and upstream DNS latency.",
        ),
        Finding::new(
            Severity::Pass,
            "Internet endpoint reached",
            "Example: both direct TCP tests connected on port 443.",
            "Internet transport worked in this example; investigate DNS and the application.",
        ),
    ];
    let diagnostic = ToolResult {
        title: "Network diagnostics".into(),
        at: chrono::Utc::now(),
        columns: vec![
            "Status".into(),
            "Finding".into(),
            "Evidence".into(),
            "Next step".into(),
        ],
        rows: findings.iter().map(Finding::row).collect(),
        findings,
        metrics: [("Internet reachability".into(), "Reachable".into())].into(),
        ..Default::default()
    };
    app.assessment = Some(diagnostic.clone());
    app.result = Some(diagnostic);
    let mut probe = Probe {
        target: "1.1.1.1".into(),
        ..Default::default()
    };
    for n in 0..90 {
        let latency = 14.0 + (n as f64 * 0.19).sin() * 3.0 + if n == 61 { 28.0 } else { 0.0 };
        probe.record(if (73..=75).contains(&n) {
            None
        } else {
            Some(latency)
        });
        app.history.samples.push(Sample {
            at: chrono::Utc::now() - chrono::Duration::seconds((89 - n) * 2),
            rx: 400000.0 + (n as f64 * 0.21).sin() * 220000.0 + if n > 67 { 350000.0 } else { 0.0 },
            tx: 85000.0 + (n as f64 * 0.37).sin() * 40000.0,
            latency: Some(latency),
            loss: Some(0.0),
            dns_ms: Some(486.0),
        });
    }
    app.probes.insert("1.1.1.1".into(), probe);
    app.dns_ms = Some(486.0);
    render(&dir.join("overview.json"), &app, 140, 42);
    render(&dir.join("overview-compact.json"), &app, 80, 24);
    render(&dir.join("overview-tall.json"), &app, 60, 48);
    render(&dir.join("overview-wide.json"), &app, 200, 48);
    app.graphics.borrow_mut().mode = nexus_net::graphics::Mode::Text;
    render(&dir.join("overview-text.json"), &app, 140, 42);
    app.graphics.borrow_mut().mode = nexus_net::graphics::Mode::Preview;
    app.config.theme = "light".into();
    render(&dir.join("overview-light.json"), &app, 140, 42);
    app.config.theme = "dark".into();
    app.dispatch("chart-pause");
    render(&dir.join("overview-paused.json"), &app, 140, 42);
    app.dispatch("chart-pause");
    let assessment = app.assessment.take();
    let probes = std::mem::take(&mut app.probes);
    let dns_ms = app.dns_ms.take();
    render(&dir.join("overview-unmeasured.json"), &app, 140, 42);
    app.graphics.borrow_mut().mode = nexus_net::graphics::Mode::Text;
    render(&dir.join("overview-unmeasured-text.json"), &app, 140, 42);
    app.graphics.borrow_mut().mode = nexus_net::graphics::Mode::Preview;
    app.assessment = assessment;
    app.probes = probes;
    app.dns_ms = dns_ms;
    app.notification = "PREVIEW FIXTURE · example data, not a live network measurement".into();
    app.page = Page::Tools;
    render(&dir.join("diagnostics.json"), &app, 140, 36);
    app.selected = 1;
    render(&dir.join("http404.json"), &app, 140, 36);
    render(&dir.join("diagnostics-compact.json"), &app, 60, 32);
    app.page = Page::Tailscale;
    app.tailscale_result = Some(nexus_net::integrations::tailnet_result(
        nexus_net::integrations::Tailnet {
            state: "Running".into(),
            name: "example tailnet".into(),
            self_name: "devbox.example.ts.net".into(),
            ips: vec!["100.64.0.24".into()],
            exit_node: "gateway.example.ts.net".into(),
            dns_suffix: "example.ts.net".into(),
            peers: vec![
                nexus_net::integrations::Peer {
                    name: "gateway.example.ts.net".into(),
                    ips: vec!["100.64.0.1".into()],
                    os: "linux".into(),
                    online: Some(true),
                    path: "direct 192.0.2.1:41641".into(),
                    exit_node: true,
                    exit_available: true,
                    rx: 38101982,
                    tx: 9055423,
                    ..Default::default()
                },
                nexus_net::integrations::Peer {
                    name: "laptop.example.ts.net".into(),
                    ips: vec!["100.64.0.42".into()],
                    os: "macOS".into(),
                    online: Some(true),
                    path: "DERP fra".into(),
                    rx: 810101,
                    tx: 954002,
                    ..Default::default()
                },
                nexus_net::integrations::Peer {
                    name: "phone.example.ts.net".into(),
                    ips: vec!["100.64.0.73".into()],
                    os: "android".into(),
                    online: Some(false),
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    ));
    render(&dir.join("tailscale.json"), &app, 140, 36);
    app.page = Page::Pihole;
    let now = chrono::Utc::now().timestamp() as f64;
    app.pihole_result = Some(nexus_net::integrations::pihole_result(
        nexus_net::integrations::PiholeStatus {
            endpoint: "https://pi.hole".into(),
            blocking: "enabled".into(),
            queries: 48729,
            blocked: 12891,
            percent: 26.5,
            clients: 18,
            domains: 214902,
            frequency: Some(2.1),
            history: (0..96)
                .map(|n| {
                    let total =
                        350.0 + (n as f64 * 0.23).sin() * 150.0 + if n > 67 { 200.0 } else { 0.0 };
                    nexus_net::integrations::PiholePoint {
                        timestamp: now - (95 - n) as f64 * 600.0,
                        total,
                        blocked: total * 0.27,
                    }
                })
                .collect(),
            ..Default::default()
        },
    ));
    render(&dir.join("pihole.json"), &app, 140, 42);
    render(&dir.join("pihole-compact.json"), &app, 60, 32);
    app.page = Page::Dashboard;
    app.selected = 0;
    app.busy = Some("Network diagnostics · checking DNS, TCP and HTTPS".into());
    for n in 0..20 {
        app.tick = n * 3;
        render(&dir.join(format!("animation-{n:02}.json")), &app, 120, 32);
    }
}
