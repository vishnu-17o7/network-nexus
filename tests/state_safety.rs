use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nexus_net::{
    app::{App, Effect, Modal, Page},
    config::{private_atomic, Config, History},
    control::{self, Change},
    model::{Interface, Sample, Snapshot},
};

#[tokio::test]
async fn escape_from_change_preview_never_applies_the_plan() {
    let snapshot = Snapshot {
        interfaces: vec![Interface {
            name: "eth0".into(),
            state: "up".into(),
            mtu: 1500,
            ..Default::default()
        }],
        ..Default::default()
    };
    let plan = control::prepare(
        Change::Mtu {
            interface: "eth0".into(),
            value: 1400,
        },
        &snapshot,
    )
    .await
    .unwrap();
    let mut app = App::new(Config::default(), History::default());
    app.snapshot = snapshot;
    app.modal = Some(Modal::ConfirmPlan(plan));
    assert!(matches!(
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        Effect::None
    ));
    assert_eq!(app.snapshot.interfaces[0].mtu, 1500);
    assert!(app.modal.is_none());
}

#[test]
fn interface_counter_reset_does_not_create_an_enormous_rate() {
    let mut app = App::new(Config::default(), History::default());
    let first = Snapshot {
        timestamp: Utc::now(),
        interfaces: vec![Interface {
            name: "eth0".into(),
            rx_bytes: 10000,
            tx_bytes: 20000,
            ..Default::default()
        }],
        ..Default::default()
    };
    app.update_snapshot(first);
    let next = Snapshot {
        timestamp: Utc::now(),
        interfaces: vec![Interface {
            name: "eth0".into(),
            rx_bytes: 1,
            tx_bytes: 1,
            ..Default::default()
        }],
        ..Default::default()
    };
    app.update_snapshot(next);
    assert_eq!(app.snapshot.interfaces[0].rx_rate, 0.0);
    assert_eq!(app.snapshot.interfaces[0].tx_rate, 0.0);
}

#[test]
fn history_retention_keeps_newest_samples() {
    let sample = |rx| Sample {
        at: Utc::now(),
        rx,
        tx: 0.0,
        latency: None,
        loss: None,
        dns_ms: None,
    };
    let history = History {
        samples: vec![sample(1.0), sample(2.0), sample(3.0)],
        ..Default::default()
    };
    let app = App::new(
        Config {
            retention_samples: 2,
            ..Default::default()
        },
        history,
    );
    assert_eq!(app.history.samples[0].rx, 2.0);
    assert_eq!(app.history.samples[1].rx, 3.0);
}

#[test]
fn filter_form_applies_without_network_effects() {
    let mut app = App::new(Config::default(), History::default());
    app.navigate(Page::Interfaces);
    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
    assert!(matches!(
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Effect::None
    ));
    assert_eq!(app.filter, "e");
}

#[cfg(unix)]
#[test]
fn atomic_save_is_private_and_does_not_chmod_existing_parent() {
    use std::os::unix::fs::PermissionsExt;
    let parent = std::env::temp_dir().join(format!("nexus-save-test-{}", std::process::id()));
    std::fs::create_dir_all(&parent).unwrap();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
    let file = parent.join("test.json");
    private_atomic(&file, b"test").unwrap();
    assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(
        parent.metadata().unwrap().permissions().mode() & 0o777,
        0o755
    );
    std::fs::remove_dir_all(parent).unwrap();
}
