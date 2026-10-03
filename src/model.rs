use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Interface {
    pub name: String,
    pub kind: String,
    pub state: String,
    pub mac: String,
    pub mtu: u32,
    pub addresses: Vec<String>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
    pub dropped: u64,
    pub speed_mbps: Option<u64>,
    pub duplex: Option<String>,
    pub rx_rate: f64,
    pub tx_rate: f64,
    pub dns: Vec<String>,
    pub connection: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Route {
    pub family: String,
    pub destination: String,
    pub gateway: String,
    pub interface: String,
    pub metric: u64,
    pub protocol: String,
    pub table: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Connection {
    pub protocol: String,
    pub state: String,
    pub local: String,
    pub remote: String,
    pub uid: u32,
    pub inode: u64,
    pub pid: Option<u32>,
    pub process: String,
    pub executable: String,
}
impl Connection {
    pub fn listening(&self) -> bool {
        self.state == "LISTEN" || (self.protocol.starts_with("udp") && self.remote.ends_with(":0"))
    }
    pub fn port(&self) -> u16 {
        self.local
            .rsplit(':')
            .next()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }
    pub fn wildcard(&self) -> bool {
        self.local.starts_with("0.0.0.0:") || self.local.starts_with("[::]:")
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Neighbor {
    pub ip: String,
    pub mac: String,
    pub interface: String,
    pub state: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WifiNetwork {
    pub connected: bool,
    pub ssid: String,
    pub bssid: String,
    pub signal: u8,
    pub frequency: String,
    pub channel: u16,
    pub security: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Capability {
    pub command: String,
    pub available: bool,
    pub purpose: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub timestamp: DateTime<Utc>,
    pub interfaces: Vec<Interface>,
    pub routes: Vec<Route>,
    pub connections: Vec<Connection>,
    pub neighbors: Vec<Neighbor>,
    pub dns: Vec<String>,
    pub dns_source: String,
    pub primary: Option<String>,
    pub capabilities: Vec<Capability>,
    pub proxies: BTreeMap<String, String>,
    pub warnings: Vec<String>,
    pub host_uptime: u64,
    pub wifi: Vec<WifiNetwork>,
}
impl Snapshot {
    pub fn primary_interface(&self) -> Option<&Interface> {
        self.primary
            .as_ref()
            .and_then(|n| self.interfaces.iter().find(|i| &i.name == n))
    }
    pub fn gateway(&self) -> Option<&str> {
        self.routes
            .iter()
            .find(|r| r.destination == "default" && Some(&r.interface) == self.primary.as_ref())
            .map(|r| r.gateway.as_str())
    }
    pub fn has(&self, command: &str) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.command == command && c.available)
    }
    pub fn traffic_totals(&self) -> (f64, f64, u64, u64) {
        if let Some(i) = self.primary_interface() {
            return (i.rx_rate, i.tx_rate, i.rx_bytes, i.tx_bytes);
        }
        self.interfaces
            .iter()
            .filter(|i| i.kind != "Loopback")
            .fold((0.0, 0.0, 0, 0), |(rx, tx, rb, tb), i| {
                (
                    rx + i.rx_rate,
                    tx + i.tx_rate,
                    rb + i.rx_bytes,
                    tb + i.tx_bytes,
                )
            })
    }
    pub fn vpn_interfaces(&self) -> Vec<&Interface> {
        self.interfaces
            .iter()
            .filter(|i| i.kind == "VPN" || i.kind == "Tunnel")
            .collect()
    }
    pub fn local_health(&self) -> Option<u8> {
        if self.timestamp.timestamp() == 0 {
            return None;
        }
        let checks = [
            self.primary.is_some(),
            self.primary_interface()
                .is_some_and(|i| i.state == "up" || i.state == "unknown"),
            self.primary_interface()
                .is_some_and(|i| !i.addresses.is_empty()),
            !self.dns.is_empty(),
        ];
        Some((checks.iter().filter(|v| **v).count() * 25) as u8)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkEvent {
    pub at: DateTime<Utc>,
    pub kind: String,
    pub detail: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Probe {
    pub target: String,
    pub sent: u64,
    pub received: u64,
    pub history: VecDeque<Option<f64>>,
    pub last: Option<f64>,
}
impl Probe {
    pub fn record(&mut self, value: Option<f64>) {
        self.sent += 1;
        if value.is_some() {
            self.received += 1;
        }
        self.last = value;
        self.history.push_back(value);
        while self.history.len() > 300 {
            self.history.pop_front();
        }
    }
    pub fn loss(&self) -> f64 {
        if self.sent == 0 {
            0.0
        } else {
            100.0 * (1.0 - self.received as f64 / self.sent as f64)
        }
    }
    pub fn stats(&self) -> Option<(f64, f64, f64, f64, f64)> {
        let values: Vec<f64> = self.history.iter().flatten().copied().collect();
        if values.is_empty() {
            return None;
        }
        let mut sorted = values.clone();
        sorted.sort_by(f64::total_cmp);
        let n = sorted.len();
        let median = if n.is_multiple_of(2) {
            (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
        } else {
            sorted[n / 2]
        };
        let diffs: Vec<f64> = self
            .history
            .iter()
            .zip(self.history.iter().skip(1))
            .filter_map(|(a, b)| Some((b.as_ref()? - a.as_ref()?).abs()))
            .collect();
        let jitter = if diffs.is_empty() {
            0.0
        } else {
            diffs.iter().sum::<f64>() / diffs.len() as f64
        };
        Some((
            sorted[0],
            sorted[n - 1],
            values.iter().sum::<f64>() / n as f64,
            median,
            jitter,
        ))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sample {
    pub at: DateTime<Utc>,
    pub rx: f64,
    pub tx: f64,
    pub latency: Option<f64>,
    pub loss: Option<f64>,
    pub dns_ms: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ToolResult {
    pub title: String,
    pub at: DateTime<Utc>,
    pub rows: Vec<Vec<String>>,
    pub columns: Vec<String>,
    pub notes: Vec<String>,
    pub metrics: BTreeMap<String, String>,
    #[serde(default)]
    pub findings: Vec<crate::diagnosis::Finding>,
    #[serde(default)]
    pub tailscale: Option<crate::integrations::Tailnet>,
    #[serde(default)]
    pub pihole: Option<crate::integrations::PiholeStatus>,
}

pub fn rate(v: f64) -> String {
    let units = ["B/s", "KiB/s", "MiB/s", "GiB/s"];
    let mut value = v.max(0.0);
    let mut unit = 0;
    while value >= 1024.0 && unit < 3 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", units[unit])
}
pub fn bytes(v: u64) -> String {
    rate(v as f64).replace("/s", "")
}
pub fn ms(v: Option<f64>) -> String {
    v.map(|v| format!("{v:.1} ms"))
        .unwrap_or_else(|| "Unmeasured".into())
}
pub fn service(port: u16) -> &'static str {
    match port {
        22 => "SSH",
        53 => "DNS",
        80 => "HTTP",
        443 => "HTTPS",
        631 => "IPP",
        3306 => "MySQL",
        5432 => "Postgres",
        6379 => "Redis",
        27017 => "MongoDB",
        3000 | 4200 | 5173 | 8000 | 8080 | 8888 => "Dev / HTTP?",
        _ => "—",
    }
}

pub fn snapshot_events(old: &Snapshot, new: &Snapshot) -> Vec<NetworkEvent> {
    let mut events = Vec::new();
    let mut push = |kind: &str, detail: String| {
        events.push(NetworkEvent {
            at: Utc::now(),
            kind: kind.into(),
            detail,
        })
    };
    if old.timestamp.timestamp() == 0 {
        return events;
    }
    for i in &new.interfaces {
        match old.interfaces.iter().find(|o| o.name == i.name) {
            None => push("Interface added", i.name.clone()),
            Some(o) => {
                if o.state != i.state {
                    push(
                        "Link changed",
                        format!("{}: {} → {}", i.name, o.state, i.state),
                    );
                }
                if o.addresses != i.addresses {
                    push(
                        "IP changed",
                        format!("{}: {}", i.name, i.addresses.join(", ")),
                    );
                }
            }
        }
    }
    for i in &old.interfaces {
        if !new.interfaces.iter().any(|n| n.name == i.name) {
            push("Interface removed", i.name.clone());
        }
    }
    if old.routes != new.routes {
        push("Routing changed", "Routing table updated".into());
    }
    if old.dns != new.dns {
        push("DNS changed", new.dns.join(", "));
    }
    let connected = |s: &Snapshot| {
        s.wifi
            .iter()
            .find(|w| w.connected)
            .map(|w| (w.ssid.clone(), w.bssid.clone()))
    };
    if connected(old) != connected(new) {
        push(
            "Wi-Fi changed",
            connected(new)
                .map(|(ssid, bssid)| format!("{ssid} ({bssid})"))
                .unwrap_or_else(|| "No connected AP in cache".into()),
        );
    }
    let active_vpn = |s: &Snapshot| {
        s.vpn_interfaces()
            .iter()
            .filter(|i| i.state == "up" || i.state == "unknown")
            .map(|i| i.name.clone())
            .collect::<Vec<_>>()
    };
    if active_vpn(old) != active_vpn(new) {
        push("VPN interfaces changed", active_vpn(new).join(", "));
    }
    for n in &new.neighbors {
        if !old
            .neighbors
            .iter()
            .any(|o| o.ip == n.ip && o.interface == n.interface)
        {
            push(
                "Neighbor discovered",
                format!("{} on {}", n.ip, n.interface),
            );
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn latency_stats_keep_loss_separate() {
        let mut p = Probe::default();
        for v in [Some(10.0), None, Some(20.0), Some(40.0)] {
            p.record(v);
        }
        assert_eq!(p.loss(), 25.0);
        let (min, max, avg, median, jitter) = p.stats().unwrap();
        assert_eq!((min, max, median, jitter), (10.0, 40.0, 20.0, 20.0));
        assert!((avg - 70.0 / 3.0).abs() < 0.001);
    }
    #[test]
    fn udp_listening_and_ipv6_binding() {
        let c = Connection {
            protocol: "udp6".into(),
            local: "[::]:53".into(),
            remote: "[::]:0".into(),
            ..Default::default()
        };
        assert!(c.listening());
        assert!(c.wildcard());
        assert_eq!(c.port(), 53);
    }
}
