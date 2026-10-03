use crate::{
    config::{Config, History, SavedProfile},
    control::{Change, Plan},
    model::*,
    tools::Tool,
};
use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{cell::RefCell, collections::BTreeMap, time::Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Dashboard,
    Interfaces,
    Wifi,
    Dns,
    Latency,
    Connections,
    Ports,
    Bandwidth,
    Routes,
    Neighbors,
    Tools,
    Events,
    History,
    Profiles,
    System,
    Tailscale,
    Pihole,
}
impl Page {
    pub const ALL: [Page; 17] = [
        Self::Dashboard,
        Self::Interfaces,
        Self::Wifi,
        Self::Dns,
        Self::Latency,
        Self::Connections,
        Self::Ports,
        Self::Bandwidth,
        Self::Routes,
        Self::Neighbors,
        Self::Tools,
        Self::Events,
        Self::History,
        Self::Profiles,
        Self::System,
        Self::Tailscale,
        Self::Pihole,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Dashboard => "Overview",
            Self::Interfaces => "Interfaces",
            Self::Wifi => "Wi-Fi",
            Self::Dns => "DNS",
            Self::Latency => "Latency",
            Self::Connections => "Connections",
            Self::Ports => "Listening ports",
            Self::Bandwidth => "Bandwidth",
            Self::Routes => "Routes",
            Self::Neighbors => "Neighbors / LAN",
            Self::Tools => "Diagnostics / tools",
            Self::Events => "Event timeline",
            Self::History => "History",
            Self::Profiles => "Profiles",
            Self::System => "System / capabilities",
            Self::Tailscale => "Tailscale",
            Self::Pihole => "Pi-hole",
        }
    }
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }
}

#[derive(Clone)]
pub struct Field {
    pub label: String,
    pub value: String,
    pub secret: bool,
}
#[derive(Clone)]
pub struct Form {
    pub id: String,
    pub title: String,
    pub fields: Vec<Field>,
    pub active: usize,
    pub note: String,
}
#[derive(Clone)]
pub enum Modal {
    Palette {
        query: String,
        selected: usize,
        global: bool,
    },
    Form(Form),
    ConfirmTool(Tool),
    ConfirmPlan(Plan),
    ExternalConsent,
    Help,
    Detail {
        title: String,
        lines: Vec<String>,
        scroll: u16,
    },
}
pub enum Effect {
    None,
    Run(Tool),
    Prepare(Change),
    Apply(Plan),
    Refresh,
    Save,
    Export { full: bool, text: bool },
    Copy(String),
    Quit,
}

#[derive(Clone)]
pub struct Action {
    pub id: &'static str,
    pub name: String,
    pub hint: &'static str,
}

pub fn actions() -> Vec<Action> {
    let mut a: Vec<Action> = Page::ALL
        .iter()
        .map(|p| Action {
            id: match p {
                Page::Dashboard => "page:0",
                Page::Interfaces => "page:1",
                Page::Wifi => "page:2",
                Page::Dns => "page:3",
                Page::Latency => "page:4",
                Page::Connections => "page:5",
                Page::Ports => "page:6",
                Page::Bandwidth => "page:7",
                Page::Routes => "page:8",
                Page::Neighbors => "page:9",
                Page::Tools => "page:10",
                Page::Events => "page:11",
                Page::History => "page:12",
                Page::Profiles => "page:13",
                Page::System => "page:14",
                Page::Tailscale => "page:15",
                Page::Pihole => "page:16",
            },
            name: format!("Open {}", p.title()),
            hint: "Navigate",
        })
        .collect();
    for (id, name, hint) in [
        (
            "tailscale-status",
            "Refresh Tailscale peers and health",
            "Local tailscaled JSON",
        ),
        (
            "tailscale-ping",
            "Test Tailscale peer path",
            "Three explicit discovery pings",
        ),
        (
            "tailscale-netcheck",
            "Tailscale NAT / DERP netcheck",
            "Explicit STUN and relay tests",
        ),
        (
            "tailscale-exit",
            "Set Tailscale exit node",
            "Preview routing change; blank disables",
        ),
        (
            "tailscale-dns",
            "Set Tailscale accept-DNS",
            "Preview; retain previous preference",
        ),
        (
            "pihole-connect",
            "Connect to Pi-hole v6",
            "URL and masked session-only application password",
        ),
        (
            "pihole-refresh",
            "Refresh Pi-hole",
            "Configured server only",
        ),
        (
            "pihole-pause",
            "Pause Pi-hole for 60 seconds",
            "Read current state; preview and confirm",
        ),
        (
            "pihole-resume",
            "Resume Pi-hole blocking",
            "Read current state; preview and confirm",
        ),
        (
            "pihole-disconnect",
            "Disconnect Pi-hole monitoring",
            "Clear credentials from session memory",
        ),
        (
            "diagnostics",
            "Run network diagnostics",
            "External requests · no repairs",
        ),
        ("ping", "Ping host", "5 ICMP probes"),
        (
            "monitor-target",
            "Add latency target",
            "Used when monitoring and external access are enabled",
        ),
        (
            "monitor",
            "Toggle live latency monitoring",
            "Gateway locally; external targets only with consent",
        ),
        (
            "dns-lookup",
            "DNS lookup",
            "A / AAAA / MX / TXT / CNAME / NS / PTR / SOA",
        ),
        (
            "dns-compare",
            "Compare DNS resolvers",
            "Cloudflare / Google / Quad9 / AdGuard",
        ),
        (
            "dns-status",
            "Resolved DNS status",
            "DoT and cache statistics",
        ),
        (
            "dot",
            "Test DNS over TLS",
            "Verified resolver query · certificate / port / DNS errors",
        ),
        (
            "dns-set",
            "Change per-interface DNS",
            "Preview persistent/runtime change",
        ),
        (
            "dns-auto",
            "Restore automatic DNS",
            "NetworkManager profiles",
        ),
        ("dns-cloudflare", "Use Cloudflare DNS", "1.1.1.1 / 1.0.0.1"),
        ("dns-google", "Use Google DNS", "8.8.8.8 / 8.8.4.4"),
        ("dns-quad9", "Use Quad9 DNS", "9.9.9.9 / 149.112.112.112"),
        (
            "dns-adguard",
            "Use AdGuard DNS",
            "94.140.14.14 / 94.140.15.15",
        ),
        (
            "dns-flush",
            "Flush DNS cache",
            "Confirmation · systemd-resolved",
        ),
        (
            "http",
            "Inspect HTTP / HTTPS URL",
            "Headers / redirects / timings",
        ),
        (
            "tls",
            "Inspect TLS certificate",
            "Validation / expiry / SAN",
        ),
        ("tcp", "Test TCP host and port", "Connection establishment"),
        (
            "udp",
            "Test UDP response",
            "No response does not prove closed",
        ),
        ("trace", "Traceroute to host", "20 hops / bounded probes"),
        ("mtr", "Repeated path analysis", "MTR JSON / 10 cycles"),
        (
            "public-ip",
            "Retrieve public IPv4 / IPv6",
            "Explicit external service",
        ),
        (
            "speed",
            "Full internet speed test",
            "Ookla or Python backend · data usage",
        ),
        (
            "iperf-quick",
            "Quick iperf3 bandwidth test",
            "5 seconds per direction · your server",
        ),
        (
            "iperf-full",
            "Full iperf3 bandwidth test",
            "15 seconds per direction · your server",
        ),
        (
            "http-families",
            "Compare IPv4 / IPv6 HTTP",
            "Explicit requests using each family",
        ),
        (
            "ip-info",
            "Look up IP ASN / ISP / region",
            "External ipapi.co service · approximate metadata",
        ),
        (
            "process-traffic",
            "Sample per-process bandwidth",
            "NetHogs · privileges · five intervals",
        ),
        (
            "route-rules",
            "Inspect policy routing rules",
            "Read-only iproute2 JSON",
        ),
        (
            "developer",
            "Inspect development endpoints",
            "Localhost HEAD checks on common dev ports",
        ),
        ("wifi-scan", "Scan Wi-Fi", "Local radio scan"),
        (
            "wifi-saved",
            "Saved Wi-Fi profiles",
            "Never reads credentials",
        ),
        (
            "wifi-connect",
            "Connect Wi-Fi / hidden SSID",
            "WPA personal/open · stdin secret",
        ),
        ("wifi-disconnect", "Disconnect Wi-Fi", "Preview change"),
        (
            "wifi-forget",
            "Forget saved Wi-Fi",
            "Irreversible profile deletion",
        ),
        ("wifi-auto", "Set profile auto-connect", "Connection UUID"),
        ("radio-on", "Enable Wi-Fi radio", "Preview change"),
        (
            "radio-off",
            "Disable Wi-Fi radio",
            "Disrupts wireless connections",
        ),
        (
            "wifi-survey",
            "Wi-Fi airtime survey",
            "Driver support / permissions required",
        ),
        ("link-up", "Bring interface up", "Privileged runtime change"),
        ("link-down", "Bring interface down", "Disruptive · preview"),
        ("mtu", "Set interface MTU", "Runtime · revert available"),
        (
            "dhcp-renew",
            "Refresh DHCP connection",
            "NetworkManager disconnect / reactivate",
        ),
        (
            "dhcp-release",
            "Disconnect DHCP connection",
            "Server-side RELEASE is manager-controlled",
        ),
        (
            "static",
            "Configure static IPv4 / IPv6",
            "NetworkManager · preview",
        ),
        (
            "ethtool",
            "Inspect Ethernet link",
            "Link speed / duplex / capabilities",
        ),
        ("route-add", "Add route", "Main table runtime route"),
        ("route-delete", "Remove exact route", "Preview and revert"),
        (
            "lan",
            "Discover LAN devices",
            "Private directly connected /24–/30 only",
        ),
        ("known-device", "Label known LAN device", "Local metadata"),
        (
            "vpn",
            "Inspect VPN / WireGuard / Tailscale",
            "Never reads private keys",
        ),
        (
            "firewall",
            "Inspect firewall rules",
            "Read-only · permission-dependent",
        ),
        (
            "containers",
            "Inspect Docker / Podman networks",
            "Engine access required",
        ),
        ("namespaces", "List network namespaces", "Read-only"),
        (
            "namespace-inspect",
            "Inspect named namespace",
            "May require privileges",
        ),
        (
            "proxy",
            "Inspect proxy configuration",
            "Credentials redacted",
        ),
        (
            "profile-save",
            "Save current network profile",
            "Capture active DNS and MTU",
        ),
        (
            "profile-apply",
            "Apply a saved profile",
            "Preview and revert",
        ),
        (
            "profile-delete",
            "Delete local profile",
            "Local metadata only",
        ),
        (
            "export",
            "Export redacted JSON report",
            "Local export directory",
        ),
        (
            "export-text",
            "Export redacted text report",
            "Local export directory",
        ),
        (
            "export-full",
            "Export detailed private JSON report",
            "Contains local IPs, MACs and endpoints",
        ),
        (
            "theme",
            "Change theme",
            "Dark / OLED / Catppuccin / Tokyo / Gruvbox / light",
        ),
        (
            "external",
            "Toggle external service access",
            "Consent needed before enabling",
        ),
        (
            "revert",
            "Revert last network change",
            "Preview captured previous values",
        ),
        (
            "restart",
            "Restart NetworkManager",
            "Explicitly disruptive · confirmation",
        ),
        ("refresh", "Refresh local snapshot", "Local only"),
        (
            "tool-history",
            "Saved diagnostic results",
            "Locally retained results with timestamps",
        ),
        (
            "tool-open",
            "Open saved diagnostic result",
            "Select a retained result index",
        ),
        (
            "trace-compare",
            "Compare last two traceroutes",
            "Same target · detects responder changes",
        ),
        (
            "chart-pause",
            "Pause / resume graphs",
            "Space · collection continues",
        ),
        (
            "chart-range",
            "Change graph range",
            "] · 1 / 5 / 15 min; 30 / 90 / 300 probes",
        ),
        (
            "chart-renderer",
            "Toggle smooth / text graphs",
            "Kitty graphics or portable text",
        ),
        ("help", "Keyboard help", "?"),
    ] {
        a.push(Action {
            id,
            name: name.into(),
            hint,
        });
    }
    a
}

pub struct App {
    pub config: Config,
    pub history: History,
    pub snapshot: Snapshot,
    pub page: Page,
    pub selected: usize,
    pub filter: String,
    pub modal: Option<Modal>,
    pub probes: BTreeMap<String, Probe>,
    pub result: Option<ToolResult>,
    pub assessment: Option<ToolResult>,
    pub tailscale_result: Option<ToolResult>,
    pub pihole_result: Option<ToolResult>,
    pub pihole_connection: Option<(String, crate::integrations::Secret)>,
    pub pihole_polling: bool,
    pub wifi: Vec<WifiNetwork>,
    pub busy: Option<String>,
    pub mutating: bool,
    pub last_plan: Option<Plan>,
    pub notification: String,
    pub notice_at: Instant,
    pub started: Instant,
    pub last_snapshot: Instant,
    pub tick: usize,
    pub connection_filter: u8,
    pub sort: usize,
    pub paused: bool,
    pub chart_window: usize,
    pub chart_snapshot: Option<crate::charts::Snapshot>,
    pub graphics: RefCell<crate::graphics::Graphics>,
    pub last_public_ip: Option<String>,
    pub internet: Option<bool>,
    pub dns_ms: Option<f64>,
    pub scroll: u16,
}
impl App {
    pub fn new(config: Config, mut history: History) -> Self {
        let excess = history
            .samples
            .len()
            .saturating_sub(config.retention_samples);
        if excess > 0 {
            history.samples.drain(..excess);
        }
        history.tests.truncate(100);
        history.tool_results.truncate(100);
        history.events.truncate(500);
        Self {
            config,
            history,
            snapshot: Snapshot::default(),
            page: Page::Dashboard,
            selected: 0,
            filter: String::new(),
            modal: None,
            probes: BTreeMap::new(),
            result: None,
            assessment: None,
            tailscale_result: None,
            pihole_result: None,
            pihole_connection: None,
            pihole_polling: false,
            wifi: Vec::new(),
            busy: None,
            mutating: false,
            last_plan: None,
            notification: "Local-only mode · Ctrl+K opens every tool and control".into(),
            notice_at: Instant::now(),
            started: Instant::now(),
            last_snapshot: Instant::now(),
            tick: 0,
            connection_filter: 0,
            sort: 0,
            paused: false,
            chart_window: 1,
            chart_snapshot: None,
            graphics: RefCell::new(crate::graphics::Graphics::new(crate::graphics::Mode::Text)),
            last_public_ip: None,
            internet: None,
            dns_ms: None,
            scroll: 0,
        }
    }
    pub fn notice(&mut self, value: impl Into<String>) {
        self.notification = crate::command::clean(&value.into());
        self.notice_at = Instant::now();
    }
    pub fn event(&mut self, kind: &str, detail: String) {
        self.history.events.insert(
            0,
            NetworkEvent {
                at: Utc::now(),
                kind: kind.into(),
                detail,
            },
        );
        self.history.events.truncate(500);
    }
    pub fn update_snapshot(&mut self, mut next: Snapshot) {
        let elapsed = self.last_snapshot.elapsed().as_secs_f64().max(0.1);
        for i in &mut next.interfaces {
            if let Some(old) = self.snapshot.interfaces.iter().find(|o| o.name == i.name) {
                i.rx_rate = i.rx_bytes.saturating_sub(old.rx_bytes) as f64 / elapsed;
                i.tx_rate = i.tx_bytes.saturating_sub(old.tx_bytes) as f64 / elapsed;
            }
        }
        for event in snapshot_events(&self.snapshot, &next) {
            self.history.events.insert(0, event);
        }
        let (rx, tx, _, _) = next.traffic_totals();
        let probe = if self.config.external_enabled && self.config.monitoring_enabled {
            self.internet_probe()
        } else {
            None
        };
        self.history.samples.push(Sample {
            at: Utc::now(),
            rx,
            tx,
            latency: probe.and_then(|p| p.last),
            loss: probe.map(Probe::loss),
            dns_ms: self.dns_ms,
        });
        let excess = self
            .history
            .samples
            .len()
            .saturating_sub(self.config.retention_samples);
        if excess > 0 {
            self.history.samples.drain(..excess);
        }
        for n in &next.neighbors {
            let now = Utc::now();
            self.history
                .devices
                .entry(n.ip.clone())
                .and_modify(|d| {
                    d.mac = n.mac.clone();
                    d.state = n.state.clone();
                    if n.state.contains("REACHABLE") {
                        d.last_seen = now;
                    }
                })
                .or_insert_with(|| crate::config::DeviceObservation {
                    first_seen: now,
                    last_seen: now,
                    mac: n.mac.clone(),
                    state: n.state.clone(),
                    vendor: crate::backend::linux::local_vendor(&n.mac),
                });
        }
        if !next.wifi.is_empty() {
            self.wifi = next.wifi.clone();
        }
        self.history.events.truncate(500);
        self.snapshot = next;
        self.last_snapshot = Instant::now();
    }
    pub fn update_probes(&mut self, values: Vec<(String, Option<f64>)>, dns: Option<f64>) {
        self.dns_ms = dns;
        for (target, value) in values {
            let p = self.probes.entry(target.clone()).or_insert_with(|| Probe {
                target: target.clone(),
                ..Default::default()
            });
            let prior = p.last;
            p.record(value);
            if let (Some(old), Some(new)) = (prior, value) {
                if new > old * 3.0 && new - old > 30.0 {
                    self.event("Latency spike", format!("{target}: {old:.1} → {new:.1} ms"));
                }
            }
        }
        let current = self
            .config
            .targets
            .iter()
            .filter_map(|t| self.probes.get(t))
            .filter(|p| p.sent > 0)
            .any(|p| p.last.is_some());
        if self.config.external_enabled && self.config.monitoring_enabled {
            if self.internet.is_some_and(|old| old != current) {
                self.event(
                    if current {
                        "Probe response restored"
                    } else {
                        "Internet probes failed"
                    },
                    "ICMP observation; test HTTPS before concluding outage".into(),
                );
            }
            self.internet = Some(current);
        }
    }
    pub fn internet_probe(&self) -> Option<&Probe> {
        self.config.targets.iter().find_map(|t| self.probes.get(t))
    }
    pub fn findings(&self) -> Vec<crate::diagnosis::Finding> {
        let mut findings = crate::diagnosis::local_findings(&self.snapshot);
        if let Some(result) = &self.assessment {
            for finding in &result.findings {
                if !findings.iter().any(|f| f.title == finding.title) {
                    findings.push(finding.clone());
                }
            }
            let age = (Utc::now() - result.at).num_seconds();
            if age > 300 {
                findings.push(crate::diagnosis::Finding::new(crate::diagnosis::Severity::Unknown, "Diagnostic results need refreshing", format!("Last findings are {} minutes old.", age / 60), "Press d to run fresh diagnostics; earlier results are historical observations."));
            }
        } else {
            findings.push(crate::diagnosis::Finding::new(
                crate::diagnosis::Severity::Info,
                "Internet and endpoints have not been tested",
                "Local monitoring is active; external checks run only when requested.",
                "Press d for diagnostics, or Ctrl+K to inspect a URL, DNS or TLS.",
            ));
        }
        for service in [&self.tailscale_result, &self.pihole_result]
            .into_iter()
            .flatten()
        {
            if (Utc::now() - service.at).num_seconds() <= 300 {
                findings.extend(service.findings.clone());
            }
        }
        findings.sort_by_key(|f| f.severity.rank());
        if !self.filter.is_empty() {
            let query = self.filter.to_lowercase();
            findings.retain(|f| f.row().join(" ").to_lowercase().contains(&query));
        }
        findings
    }
    pub fn finish_result(&mut self, result: ToolResult) {
        if !result.findings.is_empty() && result.tailscale.is_none() && result.pihole.is_none() {
            self.assessment = Some(result.clone());
        }
        if result.title.starts_with("Public IP") {
            let ip = result
                .rows
                .iter()
                .find(|r| r.first().is_some_and(|v| v == "-4"))
                .and_then(|r| r.get(1))
                .cloned()
                .filter(|s| s.parse::<std::net::IpAddr>().is_ok());
            if self.last_public_ip.is_some() && self.last_public_ip != ip && ip.is_some() {
                self.event(
                    "Public IP changed",
                    format!(
                        "{} → {}",
                        self.last_public_ip.as_deref().unwrap_or("?"),
                        ip.as_deref().unwrap_or("?")
                    ),
                );
            }
            if ip.is_some() {
                self.last_public_ip = ip;
            }
        }
        if let Some(download) = result
            .metrics
            .get("Download Mbps")
            .and_then(|v| v.parse::<f64>().ok())
        {
            let prior: Vec<_> = self
                .history
                .tests
                .iter()
                .filter(|r| {
                    r.metrics.get("Backend") == result.metrics.get("Backend")
                        && r.metrics.get("Server") == result.metrics.get("Server")
                })
                .take(3)
                .filter_map(|r| r.metrics.get("Download Mbps")?.parse::<f64>().ok())
                .collect();
            if prior.len() >= 3 {
                let average = prior.iter().sum::<f64>() / prior.len() as f64;
                if average > 0.0 && download < average * 0.6 {
                    self.event("Speed degradation observed",format!("Download {download:.1} Mbps vs previous matched-test mean {average:.1} Mbps"));
                }
            }
            self.history.tests.insert(0, result.clone());
            self.history.tests.truncate(100);
        }
        if result.title == "LAN discovery" {
            for row in &result.rows {
                if row.len() >= 4 {
                    let now = Utc::now();
                    let d = self
                        .history
                        .devices
                        .entry(row[0].clone())
                        .or_insert_with(|| crate::config::DeviceObservation {
                            first_seen: now,
                            last_seen: now,
                            mac: row[1].clone(),
                            state: row[2].clone(),
                            vendor: crate::backend::linux::local_vendor(&row[1]),
                        });
                    d.mac = row[1].clone();
                    d.state = if row[3] != "Unmeasured" {
                        "ICMP responded"
                    } else {
                        "Neighbor cache only"
                    }
                    .into();
                    if row[3] != "Unmeasured" {
                        d.last_seen = now;
                    }
                }
            }
        }
        self.event("Task completed", result.title.clone());
        let mut stored = result.clone();
        if serde_json::to_vec(&stored).is_ok_and(|v| v.len() > 65536) {
            stored.rows.truncate(50);
            for row in &mut stored.rows {
                for cell in row {
                    *cell = cell.chars().take(200).collect();
                }
            }
            stored.notes.truncate(20);
            for note in &mut stored.notes {
                *note = note.chars().take(500).collect();
            }
            for value in stored.metrics.values_mut() {
                *value = value.chars().take(500).collect();
            }
            stored.notes.push(
                "Retained history truncated to bound storage; latest live result remains complete."
                    .into(),
            );
        }
        self.history.tool_results.insert(0, stored);
        self.history.tool_results.truncate(100);
        self.notice(format!("{} completed", result.title));
        let keep_selection = (result.tailscale.is_some() && self.page == Page::Tailscale)
            || (result.pihole.is_some() && self.page == Page::Pihole);
        if result.tailscale.is_some() {
            self.tailscale_result = Some(result);
            self.page = Page::Tailscale;
        } else if result.pihole.is_some() {
            self.pihole_result = Some(result);
            self.pihole_polling = true;
            self.page = Page::Pihole;
        } else {
            self.result = Some(result);
            self.page = Page::Tools;
        }
        if !keep_selection {
            self.selected = 0;
            self.scroll = 0;
            self.filter.clear();
        }
    }
    pub fn navigate(&mut self, page: Page) {
        self.page = page;
        self.selected = 0;
        self.scroll = 0;
        self.filter.clear();
        self.sort = 0;
    }
    pub fn request_tool(&mut self, tool: Tool) -> Effect {
        if self.busy.is_some() {
            self.notice("A task is running; x cancels diagnostic tasks");
            return Effect::None;
        }
        let known_pihole = matches!(&tool,Tool::Pihole{url,..} if self.pihole_connection.as_ref().is_some_and(|(known,_)|known==url) && self.pihole_result.is_some() && self.pihole_polling);
        if (tool.requires_external_consent() && !self.config.external_enabled && !known_pihole)
            || matches!(tool, Tool::ProcessTraffic { .. })
        {
            self.modal = Some(Modal::ConfirmTool(tool));
            Effect::None
        } else {
            Effect::Run(tool)
        }
    }
    fn iface(&self) -> String {
        if self.page == Page::Interfaces {
            self.filtered_interface_names()
                .get(self.selected)
                .cloned()
                .unwrap_or_default()
        } else {
            self.snapshot.primary.clone().unwrap_or_default()
        }
    }
    fn filtered_interface_names(&self) -> Vec<String> {
        self.snapshot
            .interfaces
            .iter()
            .filter(|i| self.matches(&format!("{} {} {}", i.name, i.kind, i.addresses.join(" "))))
            .map(|i| i.name.clone())
            .collect()
    }
    fn form(&mut self, id: &str, title: &str, fields: Vec<(&str, String, bool)>, note: &str) {
        self.modal = Some(Modal::Form(Form {
            id: id.into(),
            title: title.into(),
            fields: fields
                .into_iter()
                .map(|(l, v, s)| Field {
                    label: l.into(),
                    value: v,
                    secret: s,
                })
                .collect(),
            active: 0,
            note: note.into(),
        }));
    }
    pub fn dispatch(&mut self, id: &str) -> Effect {
        if let Some(n) = id
            .strip_prefix("page:")
            .and_then(|n| n.parse::<usize>().ok())
        {
            if let Some(p) = Page::ALL.get(n) {
                self.navigate(*p);
                if *p == Page::Tailscale {
                    return self.request_tool(Tool::Tailscale);
                }
            }
            return Effect::None;
        }
        let iface = self.iface();
        let host = "example.com".to_string();
        match id {
            "tailscale-status" => return self.request_tool(Tool::Tailscale),
            "tailscale-ping" => self.form(id,"Tailscale path test",vec![("Peer hostname / Tailscale IP",self.tailscale_result.as_ref().and_then(|r|r.tailscale.as_ref()).and_then(|t|t.peers.get(self.selected)).and_then(|p|p.ips.first()).cloned().unwrap_or_default(),false)],"Three explicit discovery pings. Direct and DERP paths are reported."),
            "tailscale-netcheck" => return self.request_tool(Tool::TailscaleNetcheck),
            "tailscale-dns" => self.form(id,"Tailscale DNS preference",vec![("Accept tailnet DNS? yes/no","yes".into(),false)],"Reads current preferences before preview. Changes only accept-dns; operator/root access required."),
            "tailscale-exit" => self.form(id,"Tailscale exit node",vec![("Advertised exit-node IP (blank disables)",String::new(),false)],"Only an exit-node address from the current tailnet is accepted. Preview and undo capture the current node."),
            "pihole-connect" => self.form(id,"Connect to Pi-hole v6",vec![("Base URL",self.config.pihole_url.clone().unwrap_or_else(||"https://pi.hole".into()),false),("Application password (blank if no auth)",String::new(),true)],"Password stays in memory only. HTTPS verifies certificates; HTTP sends credentials unencrypted. No queried domains or client identities are fetched."),
            "pihole-refresh" => { if let Some((url,password)) = self.pihole_connection.clone() { return self.request_tool(Tool::Pihole{url,password}); } return self.dispatch("pihole-connect"); },
            "pihole-pause" | "pihole-resume" => { if let Some((url,password)) = self.pihole_connection.clone() { return Effect::Prepare(Change::Pihole{url,password,enabled:id=="pihole-resume"}); } return self.dispatch("pihole-connect"); },
            "pihole-disconnect" => {self.pihole_polling=false;self.pihole_connection=None;if self.last_plan.as_ref().is_some_and(|p|p.steps.iter().any(|s|s.api.is_some())) {self.last_plan=None;}self.notice("Pi-hole monitoring disconnected; session credentials cleared");},
            "diagnostics"=>return self.request_tool(Tool::Diagnostics),"ping"=>self.form(id,"Ping host",vec![("Host",host,false)],"Sends five ICMP probes."),
            "dns-lookup"=>self.form(id,"DNS lookup",vec![("Name / IP",host,false),("Record type","A".into(),false),("Resolver IP (blank = system)",String::new(),false)],"A AAAA MX TXT CNAME NS PTR SOA"),
            "dot"=>self.form(id,"DNS-over-TLS inspector",vec![("Resolver address",self.config.dot_server.clone().unwrap_or_else(||"1.1.1.1".into()),false),("TLS certificate hostname",self.config.dot_tls_name.clone().unwrap_or_else(||"cloudflare-dns.com".into()),false),("Query name",self.config.dns_test_name.clone(),false),("Port","853".into(),false)],"Validates the certificate and sends one encrypted A query. Does not alter system DNS."),
            "dns-compare"=>self.form(id,"Compare resolvers",vec![("DNS name",host,false)],"Three queries each to Cloudflare, Google, Quad9 and AdGuard."),
            "trace"|"mtr"|"tls"=>self.form(id,if id=="tls"{"TLS certificate"}else{"Path analysis"},vec![("Host",host,false)],"No target is contacted until submitted."),
            "http"=>self.form(id,"HTTP inspector",vec![("URL","https://example.com".into(),false)],"HEAD request; redirects and timing; normal TLS verification."),
            "http-families"=>self.form(id,"Compare HTTP address families",vec![("URL","https://example.com".into(),false)],"Explicit HEAD requests with IPv4 and IPv6, normal certificate verification."),
            "ip-info"=>self.form(id,"External IP metadata",vec![("Public IP",self.last_public_ip.clone().unwrap_or_else(||"1.1.1.1".into()),false)],"Sends the entered IP to ipapi.co; ASN/ISP and approximate region are external metadata."),
            "process-traffic"=>self.form(id,"Per-process traffic",vec![("Interface",iface,false)],"NetHogs trace sample needs packet-capture privileges; explicit preview follows."),
            "route-rules"=>return self.request_tool(Tool::RouteRules),"developer"=>return self.request_tool(Tool::Developer),
            "tcp"|"udp"=>self.form(id,"Connectivity test",vec![("Host","127.0.0.1".into(),false),("Port","8080".into(),false)],"TCP connects; UDP sends a small test payload."),
            "public-ip"=>return self.request_tool(Tool::PublicIp),"speed"=>return self.request_tool(Tool::Speed{full:true}),
            "iperf-quick"|"iperf-full"=>self.form(id,"iperf3 bandwidth",vec![("Server you control","192.168.1.2".into(),false)],"Requires iperf3 -s on the target. Data is sent in both directions."),
            "dns-status"=>return self.request_tool(Tool::DnsStatus),"dns-flush"=>return Effect::Prepare(Change::FlushDns),
            "dns-set"=>self.form(id,"Change DNS",vec![("Interface",iface,false),("DNS IPs (comma separated)","1.1.1.1,1.0.0.1".into(),false)],"Persistent through NetworkManager; otherwise supported resolved runtime only. Preview follows."),
            "dns-auto"=>return Effect::Prepare(Change::Dns{interface:iface,servers:vec![],automatic:true}),
            "dns-cloudflare"|"dns-google"|"dns-quad9"|"dns-adguard"=>{let ips=match id{"dns-google"=>vec!["8.8.8.8","8.8.4.4"],"dns-quad9"=>vec!["9.9.9.9","149.112.112.112"],"dns-adguard"=>vec!["94.140.14.14","94.140.15.15"],_=>vec!["1.1.1.1","1.0.0.1"]};return Effect::Prepare(Change::Dns{interface:iface,servers:ips.into_iter().map(str::to_string).collect(),automatic:false});},
            "wifi-scan"=>return self.request_tool(Tool::Wifi{rescan:true}),"wifi-saved"=>return self.request_tool(Tool::WifiSaved),
            "wifi-connect"=>{
                let wifi=self.snapshot.interfaces.iter().find(|i|i.kind=="Wi-Fi").map(|i|i.name.clone()).unwrap_or_default();
                let ssid=self.wifi.get(self.selected).map(|n|n.ssid.clone()).unwrap_or_default();
                self.form(id,"Connect Wi-Fi",vec![("Wi-Fi interface",wifi,false),("SSID",ssid,false),("Password (blank = open)",String::new(),true),("Hidden? yes/no","no".into(),false)],"WPA personal/open only. Password is not saved, logged or placed in process arguments. Preview follows.");
            },
            "wifi-disconnect"=>{let wifi=self.snapshot.interfaces.iter().find(|i|i.kind=="Wi-Fi").map(|i|i.name.clone()).unwrap_or_default();return Effect::Prepare(Change::WifiDisconnect{interface:wifi});},
            "wifi-forget"=>self.form(id,"Forget Wi-Fi profile",vec![("Connection UUID",String::new(),false)],"Use Saved Wi-Fi profiles to find a UUID. Deletion cannot be reverted."),
            "wifi-auto"=>self.form(id,"Auto-connect setting",vec![("Connection UUID",String::new(),false),("Enable? yes/no","yes".into(),false)],"Affects this NetworkManager profile only."),
            "radio-on"|"radio-off"=>return Effect::Prepare(Change::Radio{enabled:id=="radio-on"}),
            "wifi-survey"=>self.form(id,"Wi-Fi survey",vec![("Interface",self.snapshot.interfaces.iter().find(|i|i.kind=="Wi-Fi").map(|i|i.name.clone()).unwrap_or_default(),false)],"Read-only driver survey; support varies."),
            "link-up"|"link-down"=>return Effect::Prepare(Change::Link{interface:iface,up:id=="link-up"}),
            "mtu"=>self.form(id,"Set runtime MTU",vec![("Interface",iface,false),("MTU","1500".into(),false)],"Previous value is retained for revert."),
            "dhcp-renew"|"dhcp-release"=>return Effect::Prepare(Change::Dhcp{interface:iface,release:id=="dhcp-release"}),
            "static"=>self.form(id,"Static IP configuration",vec![("Interface",iface,false),("IP / prefix","192.168.1.20/24".into(),false),("Gateway","192.168.1.1".into(),false)],"IPv4 or IPv6. Changes the active NetworkManager profile; preview follows."),
            "ethtool"=>return self.request_tool(Tool::Link{interface:iface}),
            "route-add"|"route-delete"=>self.form(id,"Route configuration",vec![("Destination CIDR / default","10.20.0.0/16".into(),false),("Gateway",self.snapshot.gateway().unwrap_or("192.168.1.1").into(),false),("Interface",iface,false),("Metric","100".into(),false)],"Main table runtime route. Exact matches only; no automatic route replacement."),
            "lan"=>self.form(id,"LAN discovery",vec![("Direct local private CIDR","192.168.1.0/24".into(),false)],"One ICMP probe per address; /24–/30; concurrency limited to 16."),
            "known-device"=>self.form(id,"Label device",vec![("IP address",String::new(),false),("Label",String::new(),false)],"Local metadata; no network request."),
            "vpn"=>return self.request_tool(Tool::Vpn),"firewall"=>return self.request_tool(Tool::Firewall),"containers"=>return self.request_tool(Tool::Containers),"namespaces"=>return self.request_tool(Tool::Namespaces{name:String::new()}),"proxy"=>return self.request_tool(Tool::Proxy),
            "namespace-inspect"=>self.form(id,"Inspect namespace",vec![("Existing namespace name",String::new(),false)],"Use List network namespaces first."),
            "profile-save"=>self.form(id,"Save network profile",vec![("Name","Home".into(),false),("Interface",iface,false)],"Capture current per-link DNS and MTU. No Wi-Fi secrets are read."),
            "profile-apply"|"profile-delete"=>self.form(id,"Saved profile",vec![("Exact profile name",self.history.profiles.first().map(|p|p.name.clone()).unwrap_or_default(),false)],"DNS + MTU profiles in this release."),
            "monitor-target"=>self.form(id,"Add latency target",vec![("Host","1.1.1.1".into(),false)],"External targets are probed only when external access is enabled."),
            "monitor"=>{if !self.snapshot.has("ping")&&!self.config.monitoring_enabled{self.notice("Install iputils-ping before enabling ICMP monitoring");return Effect::None;}self.config.monitoring_enabled = !self.config.monitoring_enabled;self.notice(if self.config.monitoring_enabled{"Monitoring enabled; gateway only until external access is enabled"}else{"Monitoring paused"});return Effect::Save;},
            "chart-pause"=>{self.chart_snapshot=if self.chart_snapshot.is_some(){None}else{Some(crate::charts::Snapshot::capture(self))};self.notice(if self.chart_snapshot.is_some(){"Graphs frozen · network collection continues · Space resumes"}else{"Graphs live"});},
            "chart-range"=>{self.chart_window=(self.chart_window+1)%3;self.notice(format!("Graph range: {} · [ / ] changes range",crate::charts::range_label(self)));},
            "chart-renderer"=>{let enabled=self.graphics.borrow().enabled();self.config.chart_renderer=if enabled{"text"}else{"kitty"}.into();self.graphics.borrow_mut().mode=crate::graphics::Mode::detect(&self.config.chart_renderer);self.notice(if self.graphics.borrow().enabled(){"Smooth graphs · requires Kitty-compatible terminal graphics"}else{"Portable text graphs · tmux/screen use this renderer"});return Effect::Save;},
            "theme"=>{let themes=["dark","oled","catppuccin","tokyo-night","gruvbox","light"];let n=themes.iter().position(|t|*t==self.config.theme).unwrap_or(0);self.config.theme=themes[(n+1)%themes.len()].into();self.notice(format!("Theme: {}",self.config.theme));return Effect::Save;},
            "external"=>{if self.config.external_enabled{self.config.external_enabled=false;self.internet=None;self.notice("External access disabled; cancel a running task with x");return Effect::Save;}self.modal=Some(Modal::ExternalConsent);},
            "revert"=>{if let Some(p)=self.last_plan.as_ref().and_then(Plan::revert){self.modal=Some(Modal::ConfirmPlan(p));}else{self.notice("No reversible network change this session");}},
            "restart"=>return Effect::Prepare(Change::Restart),"refresh"=>return Effect::Refresh,"export"=>return Effect::Export{full:false,text:false},"export-text"=>return Effect::Export{full:false,text:true},"export-full"=>{self.modal=Some(Modal::Detail{title:"Detailed export contains private metadata".into(),lines:vec!["Use the CLI --report --include-sensitive option to explicitly export unredacted local IPs, MACs, process names and endpoints. The standard TUI report is redacted.".into()],scroll:0});},
            "tool-history"=>{self.result=Some(ToolResult{title:"Saved diagnostic results".into(),at:Utc::now(),columns:vec!["Index".into(),"Time (UTC)".into(),"Tool".into()],rows:self.history.tool_results.iter().enumerate().map(|(i,r)|vec![i.to_string(),r.at.format("%m-%d %H:%M").to_string(),r.title.clone()]).collect(),notes:vec!["Ctrl+K → Open saved diagnostic result to inspect an index.".into()],..Default::default()});self.navigate(Page::Tools);},
            "tool-open"=>self.form(id,"Open saved tool result",vec![("Result index","0".into(),false)],"Use Saved diagnostic results to see indexes."),
            "trace-compare"=>self.compare_traces(),
            "help"=>self.modal=Some(Modal::Help),_=>self.notice("Unknown action")
        }
        Effect::None
    }
    fn submit(&mut self, f: Form) -> Effect {
        let values: Vec<String> = f
            .fields
            .iter()
            .map(|v| v.value.trim().to_string())
            .collect();
        let v = |i: usize| values.get(i).cloned().unwrap_or_default();
        let parsed = (|| -> anyhow::Result<Effect> {
            let yes = |value: String| -> anyhow::Result<bool> {
                match value.to_lowercase().as_str() {
                    "yes" | "true" | "on" => Ok(true),
                    "no" | "false" | "off" => Ok(false),
                    _ => anyhow::bail!("Enter yes or no"),
                }
            };
            let effect = match f.id.as_str() {
                "tailscale-ping" => self.request_tool(Tool::TailscalePing { host: v(0) }),
                "tailscale-dns" => Effect::Prepare(Change::TailscaleDns {
                    enabled: yes(v(0))?,
                }),
                "tailscale-exit" => Effect::Prepare(Change::TailscaleExit { target: v(0) }),
                "pihole-connect" => {
                    crate::integrations::endpoint(&v(0))?;
                    self.request_tool(Tool::Pihole {
                        url: v(0),
                        password: crate::integrations::Secret::new(v(1)),
                    })
                }
                "ping" => self.request_tool(Tool::Ping { host: v(0) }),
                "dns-lookup" => self.request_tool(Tool::Dns {
                    name: v(0),
                    record: v(1),
                    server: v(2),
                }),
                "dns-compare" => self.request_tool(Tool::CompareDns { name: v(0) }),
                "dot" => self.request_tool(Tool::Dot {
                    server: v(0),
                    identity: v(1),
                    query: v(2),
                    port: v(3).parse()?,
                }),
                "trace" | "mtr" => self.request_tool(Tool::Trace {
                    host: v(0),
                    repeated: f.id == "mtr",
                }),
                "http" => self.request_tool(Tool::Http { url: v(0) }),
                "tls" => self.request_tool(Tool::Tls { host: v(0) }),
                "tcp" => self.request_tool(Tool::Tcp {
                    host: v(0),
                    port: v(1).parse()?,
                }),
                "udp" => self.request_tool(Tool::Udp {
                    host: v(0),
                    port: v(1).parse()?,
                }),
                "iperf-quick" | "iperf-full" => self.request_tool(Tool::Iperf {
                    host: v(0),
                    full: f.id == "iperf-full",
                }),
                "wifi-survey" => self.request_tool(Tool::WifiSurvey { interface: v(0) }),
                "lan" => self.request_tool(Tool::Lan { cidr: v(0) }),
                "namespace-inspect" => self.request_tool(Tool::Namespaces { name: v(0) }),
                "http-families" => self.request_tool(Tool::HttpFamilies { url: v(0) }),
                "ip-info" => self.request_tool(Tool::IpInfo { ip: v(0) }),
                "process-traffic" => self.request_tool(Tool::ProcessTraffic { interface: v(0) }),
                "dns-set" => Effect::Prepare(Change::Dns {
                    interface: v(0),
                    servers: v(1)
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect(),
                    automatic: false,
                }),
                "mtu" => Effect::Prepare(Change::Mtu {
                    interface: v(0),
                    value: v(1).parse()?,
                }),
                "static" => Effect::Prepare(Change::Static {
                    interface: v(0),
                    address: v(1),
                    gateway: v(2),
                }),
                "route-add" | "route-delete" => Effect::Prepare(Change::Route {
                    destination: v(0),
                    gateway: v(1),
                    interface: v(2),
                    metric: v(3).parse()?,
                    remove: f.id == "route-delete",
                }),
                "wifi-connect" => Effect::Prepare(Change::WifiConnect {
                    interface: v(0),
                    ssid: v(1),
                    password: f.fields[2].value.clone(),
                    hidden: yes(v(3))?,
                }),
                "wifi-forget" => Effect::Prepare(Change::WifiForget { uuid: v(0) }),
                "wifi-auto" => Effect::Prepare(Change::Autoconnect {
                    uuid: v(0),
                    enabled: yes(v(1))?,
                }),
                "monitor-target" => {
                    crate::tools::validate_host(&v(0))?;
                    if !self.config.targets.contains(&v(0)) {
                        self.config.targets.push(v(0));
                    }
                    self.notice("Target added; enable external access and monitoring to probe it");
                    Effect::Save
                }
                "known-device" => {
                    v(0).parse::<std::net::IpAddr>()?;
                    self.history.known_devices.insert(v(0), v(1));
                    self.notice("Device label saved");
                    Effect::Save
                }
                "profile-save" => {
                    if v(0).is_empty() {
                        anyhow::bail!("Enter a profile name");
                    }
                    let i = crate::control::validate_interface(&v(1), &self.snapshot)?;
                    if i.connection.is_none() {
                        anyhow::bail!("Profile capture needs an active NetworkManager connection");
                    }
                    if i.dns.is_empty() {
                        anyhow::bail!(
                            "Per-interface DNS is unavailable; profile capture would be ambiguous"
                        );
                    }
                    let profile = SavedProfile {
                        name: v(0),
                        interface: v(1),
                        dns: i.dns.clone(),
                        mtu: Some(i.mtu),
                        ..Default::default()
                    };
                    self.history.profiles.retain(|p| p.name != profile.name);
                    self.history.profiles.push(profile);
                    self.notice("Profile saved (DNS + MTU)");
                    Effect::Save
                }
                "profile-apply" => {
                    let profile = self
                        .history
                        .profiles
                        .iter()
                        .find(|p| p.name == v(0))
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("No profile with that exact name"))?;
                    Effect::Prepare(Change::Profile(profile))
                }
                "profile-delete" => {
                    self.history.profiles.retain(|p| p.name != v(0));
                    self.notice("Local profile removed");
                    Effect::Save
                }
                "tool-open" => {
                    let index: usize = v(0).parse()?;
                    let result = self
                        .history
                        .tool_results
                        .get(index)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("No saved result at this index"))?;
                    self.result = Some(result);
                    self.navigate(Page::Tools);
                    Effect::None
                }
                _ => Effect::None,
            };
            Ok(effect)
        })();
        match parsed {
            Ok(effect) => effect,
            Err(e) => {
                self.notice(e.to_string());
                self.modal = Some(Modal::Form(f));
                Effect::None
            }
        }
    }
    pub fn key(&mut self, key: KeyEvent) -> Effect {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if self.mutating {
                self.notice(
                    "Configuration in progress; wait for completion or timeout before exit",
                );
                return Effect::None;
            }
            return Effect::Quit;
        }
        if let Some(modal) = self.modal.take() {
            match modal {
                Modal::Palette {
                    mut query,
                    mut selected,
                    global,
                } => {
                    match key.code {
                        KeyCode::Esc => return Effect::None,
                        KeyCode::Backspace => {
                            query.pop();
                            selected = 0;
                        }
                        KeyCode::Char(c) => {
                            query.push(c);
                            selected = 0;
                        }
                        KeyCode::Down => selected = selected.saturating_add(1),
                        KeyCode::Up => selected = selected.saturating_sub(1),
                        KeyCode::Enter => {
                            if global {
                                if let Some((page, _, label)) =
                                    self.search_results(&query).get(selected).cloned()
                                {
                                    self.navigate(page);
                                    self.filter = if matches!(page, Page::Wifi | Page::Profiles) {
                                        label
                                    } else {
                                        label
                                            .split_whitespace()
                                            .next()
                                            .unwrap_or_default()
                                            .to_string()
                                    };
                                }
                            } else if let Some(a) = self.palette_results(&query).get(selected) {
                                return self.dispatch(a.id);
                            }
                            return Effect::None;
                        }
                        _ => {}
                    }
                    let len = if global {
                        self.search_results(&query).len()
                    } else {
                        self.palette_results(&query).len()
                    };
                    selected = selected.min(len.saturating_sub(1));
                    self.modal = Some(Modal::Palette {
                        query,
                        selected,
                        global,
                    });
                }
                Modal::Form(mut f) => {
                    match key.code {
                        KeyCode::Esc => return Effect::None,
                        KeyCode::Tab | KeyCode::Down => f.active = (f.active + 1) % f.fields.len(),
                        KeyCode::BackTab | KeyCode::Up => {
                            f.active = (f.active + f.fields.len() - 1) % f.fields.len()
                        }
                        KeyCode::Backspace => {
                            f.fields[f.active].value.pop();
                        }
                        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                            if f.fields[f.active].value.len() < 2048 {
                                f.fields[f.active].value.push(c);
                            }
                        }
                        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            f.fields[f.active].value.clear()
                        }
                        KeyCode::Enter => return self.submit(f),
                        _ => {}
                    }
                    self.modal = Some(Modal::Form(f));
                }
                Modal::ConfirmTool(tool) => match key.code {
                    KeyCode::Enter => return Effect::Run(tool),
                    KeyCode::Esc | KeyCode::Char('n') => {}
                    _ => self.modal = Some(Modal::ConfirmTool(tool)),
                },
                Modal::ConfirmPlan(plan) => match key.code {
                    KeyCode::Enter => {
                        if self.busy.is_none() {
                            return Effect::Apply(plan);
                        }
                        self.notice("Wait for the active task before applying a change");
                    }
                    KeyCode::Esc | KeyCode::Char('n') => {}
                    _ => self.modal = Some(Modal::ConfirmPlan(plan)),
                },
                Modal::ExternalConsent => match key.code {
                    KeyCode::Enter => {
                        self.config.external_enabled = true;
                        self.notice("External access enabled; configured monitoring targets can now be probed");
                        return Effect::Save;
                    }
                    KeyCode::Esc | KeyCode::Char('n') => {}
                    _ => self.modal = Some(Modal::ExternalConsent),
                },
                Modal::Help => {
                    match key.code {
                        KeyCode::Down | KeyCode::Char('j') => {
                            self.scroll = self.scroll.saturating_add(1)
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            self.scroll = self.scroll.saturating_sub(1)
                        }
                        _ => {}
                    }
                    if !matches!(key.code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::Enter) {
                        self.modal = Some(Modal::Help);
                    }
                }
                Modal::Detail {
                    title,
                    lines,
                    mut scroll,
                } => {
                    match key.code {
                        KeyCode::Esc | KeyCode::Enter => return Effect::None,
                        KeyCode::Down | KeyCode::Char('j') => scroll = scroll.saturating_add(1),
                        KeyCode::Up | KeyCode::Char('k') => scroll = scroll.saturating_sub(1),
                        _ => {}
                    }
                    self.modal = Some(Modal::Detail {
                        title,
                        lines,
                        scroll,
                    });
                }
            }
            return Effect::None;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('k') {
            self.modal = Some(Modal::Palette {
                query: String::new(),
                selected: 0,
                global: false,
            });
            return Effect::None;
        }
        match key.code {
            KeyCode::Char('q') => {
                if self.mutating {
                    self.notice("Configuration in progress; wait for completion before exit");
                } else {
                    return Effect::Quit;
                }
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                self.navigate(Page::ALL[(self.page.index() + 1) % Page::ALL.len()])
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => self
                .navigate(Page::ALL[(self.page.index() + Page::ALL.len() - 1) % Page::ALL.len()]),
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = self
                    .selected
                    .saturating_add(1)
                    .min(self.rows().1.len().saturating_sub(1));
                self.scroll = self.scroll.saturating_add(1);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                self.scroll = self.scroll.saturating_sub(1);
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.selected = 0;
                self.scroll = 0;
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.selected = self.rows().1.len().saturating_sub(1);
                self.scroll = u16::MAX;
            }
            KeyCode::PageDown => {
                self.selected = (self.selected + 10).min(self.rows().1.len().saturating_sub(1));
                self.scroll = self.scroll.saturating_add(10);
            }
            KeyCode::PageUp => {
                self.selected = self.selected.saturating_sub(10);
                self.scroll = self.scroll.saturating_sub(10);
            }
            KeyCode::Char(c @ '1'..='9') => self.navigate(Page::ALL[c as usize - '1' as usize]),
            KeyCode::Char('/') => {
                self.modal = Some(Modal::Form(Form {
                    id: "filter".into(),
                    title: format!("Filter {}", self.page.title()),
                    fields: vec![Field {
                        label: "Search text".into(),
                        value: self.filter.clone(),
                        secret: false,
                    }],
                    active: 0,
                    note: "Matches visible table values. Escape cancels; enter applies.".into(),
                }))
            }
            KeyCode::Char('S') => {
                self.modal = Some(Modal::Palette {
                    query: String::new(),
                    selected: 0,
                    global: true,
                })
            }
            KeyCode::Esc => {
                self.filter.clear();
                self.selected = 0;
            }
            KeyCode::Char('?') => {
                self.scroll = 0;
                self.modal = Some(Modal::Help);
            }
            KeyCode::Char('r') => {
                return match self.page {
                    Page::Tailscale => self.request_tool(Tool::Tailscale),
                    Page::Pihole => self.dispatch("pihole-refresh"),
                    _ => Effect::Refresh,
                }
            }
            KeyCode::Char(' ') => return self.dispatch("chart-pause"),
            KeyCode::Char(']') => return self.dispatch("chart-range"),
            KeyCode::Char('[') => {
                self.chart_window = (self.chart_window + 1) % 3;
                return self.dispatch("chart-range");
            }
            KeyCode::Char('d') => return self.dispatch("diagnostics"),
            KeyCode::Char('m') => return self.dispatch("monitor"),
            KeyCode::Char('e') => return self.dispatch("external"),
            KeyCode::Char('t') => return self.dispatch("theme"),
            KeyCode::Char('u') => return self.dispatch("revert"),
            KeyCode::Char('E') => {
                return Effect::Export {
                    full: false,
                    text: false,
                }
            }
            KeyCode::Char('f') => {
                self.connection_filter = (self.connection_filter + 1) % 4;
                self.selected = 0;
            }
            KeyCode::Char('s') => {
                self.sort = (self.sort + 1) % 3;
                self.selected = 0;
            }
            KeyCode::Char('p') => {
                self.paused = !self.paused;
                self.notice(if self.paused {
                    "Snapshot display paused; no history samples recorded"
                } else {
                    "Snapshot display resumed"
                });
            }
            KeyCode::Char('c') => {
                let (_, rows) = self.rows();
                if let Some(row) = rows.get(self.selected) {
                    return Effect::Copy(row.join("\t"));
                }
            }
            KeyCode::Enter => {
                if self.page == Page::Tools && self.result.is_none() {
                    if let Some(name) = self.rows().1.get(self.selected).and_then(|r| r.first()) {
                        if let Some(action) = actions().into_iter().find(|a| &a.name == name) {
                            return self.dispatch(action.id);
                        }
                    }
                }
                self.inspect();
            }
            _ => {}
        }
        Effect::None
    }
    pub fn handle_key(&mut self, key: KeyEvent) -> Effect {
        if key.code == KeyCode::Enter {
            if let Some(Modal::Form(f)) = &self.modal {
                if f.id == "filter" {
                    self.filter = f.fields[0].value.clone();
                    self.selected = 0;
                    self.modal = None;
                    return Effect::None;
                }
            }
        }
        self.key(key)
    }
    fn inspect(&mut self) {
        if self.page == Page::Dashboard {
            if let Some(finding) = self.findings().get(self.selected) {
                self.modal = Some(Modal::Detail {
                    title: finding.title.clone(),
                    lines: vec![
                        format!("Status: {}", finding.severity.label()),
                        String::new(),
                        finding.evidence.clone(),
                        String::new(),
                        format!("Next step: {}", finding.next_step),
                    ],
                    scroll: 0,
                });
            }
            return;
        }
        let (columns, rows) = self.rows();
        let Some(row) = rows.get(self.selected) else {
            return;
        };
        let mut lines: Vec<String> = columns
            .iter()
            .zip(row)
            .map(|(c, v)| format!("{c}: {v}"))
            .collect();
        if self.page == Page::Interfaces {
            if let Some(i) = self
                .snapshot
                .interfaces
                .iter()
                .find(|i| Some(&i.name) == row.first())
            {
                lines = vec![
                    format!("{} · {} · {}", i.name, i.kind, i.state),
                    format!("Addresses: {}", i.addresses.join(", ")),
                    format!("MAC: {} · MTU: {}", i.mac, i.mtu),
                    format!(
                        "RX: {} / {} packets / {} errors",
                        bytes(i.rx_bytes),
                        i.rx_packets,
                        i.rx_errors
                    ),
                    format!(
                        "TX: {} / {} packets / {} errors",
                        bytes(i.tx_bytes),
                        i.tx_packets,
                        i.tx_errors
                    ),
                    format!(
                        "Dropped: {} · duplex: {}",
                        i.dropped,
                        i.duplex.as_deref().unwrap_or("Unavailable")
                    ),
                    format!("Per-link DNS: {}", i.dns.join(", ")),
                    format!(
                        "NetworkManager UUID: {}",
                        i.connection.as_deref().unwrap_or("Unmanaged")
                    ),
                    "Ctrl+K: bring up/down, MTU, DHCP, DNS and static IP".into(),
                ];
            }
        }
        if self.page == Page::Connections || self.page == Page::Ports {
            if let Some(c) = self
                .snapshot
                .connections
                .iter()
                .find(|c| row.contains(&c.local) && row.contains(&c.remote))
            {
                lines.push(format!("Executable: {}", c.executable));
                lines.push(format!("Socket inode: {} · UID: {}", c.inode, c.uid));
            }
            lines.push("Bandwidth per socket/process is unavailable without additional instrumentation; interface bandwidth is measured separately.".into());
        }
        self.modal = Some(Modal::Detail {
            title: "Selection details".into(),
            lines,
            scroll: 0,
        });
    }
    pub fn matches(&self, value: &str) -> bool {
        self.filter.is_empty() || value.to_lowercase().contains(&self.filter.to_lowercase())
    }
    fn compare_traces(&mut self) {
        let traces: Vec<_> = self
            .history
            .tool_results
            .iter()
            .filter(|r| r.title == "Traceroute")
            .collect();
        let Some(latest) = traces.first() else {
            self.notice("Run at least two traceroutes to the same host");
            return;
        };
        let Some(old) = traces
            .iter()
            .skip(1)
            .find(|r| r.metrics.get("Target") == latest.metrics.get("Target"))
        else {
            self.notice("No earlier traceroute for the same host");
            return;
        };
        let mut result=ToolResult{title:"Traceroute comparison".into(),at:Utc::now(),columns:vec!["Hop".into(),"Earlier IP".into(),"Latest IP".into(),"Observation".into()],notes:vec!["Different responders can reflect load balancing, ICMP policy or a route change; compare destination performance too.".into()],..Default::default()};
        for index in 0..old.rows.len().max(latest.rows.len()) {
            let before = old
                .rows
                .get(index)
                .and_then(|r| r.get(1))
                .cloned()
                .unwrap_or_else(|| "—".into());
            let after = latest
                .rows
                .get(index)
                .and_then(|r| r.get(1))
                .cloned()
                .unwrap_or_else(|| "—".into());
            result.rows.push(vec![
                (index + 1).to_string(),
                before.clone(),
                after.clone(),
                if before == after {
                    "Same responder"
                } else {
                    "Responder changed"
                }
                .into(),
            ]);
        }
        self.result = Some(result);
        self.navigate(Page::Tools);
    }
    pub fn rows(&self) -> (Vec<String>, Vec<Vec<String>>) {
        let (cols, mut rows): (Vec<&str>, Vec<Vec<String>>) = match self.page {
            Page::Dashboard => (
                vec!["Status", "Finding", "Evidence", "Next step"],
                self.findings()
                    .iter()
                    .map(crate::diagnosis::Finding::row)
                    .collect(),
            ),
            Page::Interfaces => (
                vec![
                    "Interface",
                    "Type",
                    "State",
                    "Addresses",
                    "MTU",
                    "RX / s",
                    "TX / s",
                ],
                self.snapshot
                    .interfaces
                    .iter()
                    .map(|i| {
                        vec![
                            i.name.clone(),
                            i.kind.clone(),
                            i.state.clone(),
                            i.addresses.join(", "),
                            i.mtu.to_string(),
                            rate(i.rx_rate),
                            rate(i.tx_rate),
                        ]
                    })
                    .collect(),
            ),
            Page::Wifi => (
                vec!["SSID", "Signal", "Channel", "Band", "Security", "Connected"],
                self.wifi
                    .iter()
                    .map(|n| {
                        vec![
                            n.ssid.clone(),
                            format!("{}%", n.signal),
                            n.channel.to_string(),
                            n.frequency.clone(),
                            n.security.clone(),
                            if n.connected { "YES" } else { "—" }.into(),
                        ]
                    })
                    .collect(),
            ),
            Page::Dns => (vec!["Scope", "Resolver / setting", "Source"], {
                let mut rows: Vec<_> = self
                    .snapshot
                    .dns
                    .iter()
                    .map(|d| vec!["Active".into(), d.clone(), self.snapshot.dns_source.clone()])
                    .collect();
                for i in &self.snapshot.interfaces {
                    for d in &i.dns {
                        rows.push(vec![i.name.clone(), d.clone(), "Per-link".into()]);
                    }
                }
                for (name, ips) in [
                    ("Cloudflare", "1.1.1.1, 1.0.0.1"),
                    ("Google", "8.8.8.8, 8.8.4.4"),
                    ("Quad9", "9.9.9.9, 149.112.112.112"),
                    ("AdGuard", "94.140.14.14, 94.140.15.15"),
                ] {
                    rows.push(vec![
                        name.into(),
                        ips.into(),
                        "Preset · Ctrl+K to apply".into(),
                    ]);
                }
                rows
            }),
            Page::Latency => (
                vec![
                    "Target",
                    "Last",
                    "Min",
                    "Avg",
                    "Median",
                    "Max",
                    "Jitter",
                    "Loss / sent",
                ],
                self.probes
                    .values()
                    .map(|p| {
                        let stats = p.stats();
                        vec![
                            p.target.clone(),
                            ms(p.last),
                            ms(stats.map(|s| s.0)),
                            ms(stats.map(|s| s.2)),
                            ms(stats.map(|s| s.3)),
                            ms(stats.map(|s| s.1)),
                            ms(stats.map(|s| s.4)),
                            format!("{:.1}% / {}", p.loss(), p.sent),
                        ]
                    })
                    .collect(),
            ),
            Page::Connections | Page::Ports => (
                vec![
                    "Proto",
                    "State",
                    "Local endpoint",
                    "Remote endpoint",
                    "PID",
                    "Process",
                    "UID",
                ],
                self.snapshot
                    .connections
                    .iter()
                    .filter(|c| self.page != Page::Ports || c.listening())
                    .filter(|c| match self.connection_filter {
                        1 => c.state == "ESTABLISHED",
                        2 => c.listening(),
                        3 => {
                            !c.remote.starts_with("127.")
                                && !c.remote.starts_with("[::1]")
                                && !c.remote.starts_with("0.0.0.0")
                                && !c.remote.starts_with("[::]:0")
                        }
                        _ => true,
                    })
                    .map(|c| {
                        vec![
                            c.protocol.clone(),
                            if c.wildcard() && c.listening() {
                                format!("{} / ALL", c.state)
                            } else {
                                c.state.clone()
                            },
                            c.local.clone(),
                            c.remote.clone(),
                            c.pid.map(|n| n.to_string()).unwrap_or_else(|| "—".into()),
                            format!(
                                "{} {}",
                                c.process,
                                if c.listening() { service(c.port()) } else { "" }
                            ),
                            c.uid.to_string(),
                        ]
                    })
                    .collect(),
            ),
            Page::Bandwidth => (
                vec![
                    "Interface",
                    "Download",
                    "Upload",
                    "Total RX",
                    "Total TX",
                    "Errors / dropped",
                ],
                self.snapshot
                    .interfaces
                    .iter()
                    .map(|i| {
                        vec![
                            i.name.clone(),
                            rate(i.rx_rate),
                            rate(i.tx_rate),
                            bytes(i.rx_bytes),
                            bytes(i.tx_bytes),
                            format!("{} / {}", i.rx_errors + i.tx_errors, i.dropped),
                        ]
                    })
                    .collect(),
            ),
            Page::Routes => (
                vec![
                    "Family",
                    "Destination",
                    "Gateway",
                    "Interface",
                    "Metric",
                    "Protocol",
                    "Table",
                ],
                self.snapshot
                    .routes
                    .iter()
                    .map(|r| {
                        vec![
                            r.family.clone(),
                            r.destination.clone(),
                            r.gateway.clone(),
                            r.interface.clone(),
                            r.metric.to_string(),
                            r.protocol.clone(),
                            r.table.clone(),
                        ]
                    })
                    .collect(),
            ),
            Page::Neighbors => (
                vec![
                    "IP",
                    "MAC",
                    "State",
                    "Vendor / label",
                    "First seen",
                    "Last seen",
                ],
                self.history
                    .devices
                    .iter()
                    .map(|(ip, d)| {
                        vec![
                            ip.clone(),
                            d.mac.clone(),
                            d.state.clone(),
                            self.history
                                .known_devices
                                .get(ip)
                                .cloned()
                                .unwrap_or_else(|| d.vendor.clone()),
                            d.first_seen.format("%m-%d %H:%M").to_string(),
                            d.last_seen.format("%m-%d %H:%M").to_string(),
                        ]
                    })
                    .collect(),
            ),
            Page::Tools => (
                self.result
                    .as_ref()
                    .map(|r| r.columns.iter().map(String::as_str).collect())
                    .unwrap_or_else(|| vec!["Action", "Description"]),
                self.result
                    .as_ref()
                    .map(|r| r.rows.clone())
                    .unwrap_or_else(|| {
                        actions()
                            .into_iter()
                            .filter(|a| !a.id.starts_with("page:"))
                            .map(|a| vec![a.name, a.hint.into()])
                            .collect()
                    }),
            ),
            Page::Events => (
                vec!["Time (UTC)", "Event", "Details"],
                self.history
                    .events
                    .iter()
                    .map(|e| {
                        vec![
                            e.at.format("%H:%M:%S").to_string(),
                            e.kind.clone(),
                            e.detail.clone(),
                        ]
                    })
                    .collect(),
            ),
            Page::History => (
                vec![
                    "Time (UTC)",
                    "Backend",
                    "Download Mbps",
                    "Upload Mbps",
                    "Test",
                ],
                self.history
                    .tests
                    .iter()
                    .map(|r| {
                        vec![
                            r.at.format("%m-%d %H:%M").to_string(),
                            r.metrics.get("Backend").cloned().unwrap_or_default(),
                            r.metrics.get("Download Mbps").cloned().unwrap_or_default(),
                            r.metrics.get("Upload Mbps").cloned().unwrap_or_default(),
                            r.title.clone(),
                        ]
                    })
                    .collect(),
            ),
            Page::Profiles => (
                vec!["Name", "Interface", "DNS", "MTU"],
                self.history
                    .profiles
                    .iter()
                    .map(|p| {
                        vec![
                            p.name.clone(),
                            p.interface.clone(),
                            p.dns.join(", "),
                            p.mtu.map(|n| n.to_string()).unwrap_or_default(),
                        ]
                    })
                    .collect(),
            ),
            Page::Tailscale | Page::Pihole => {
                let result = if self.page == Page::Tailscale {
                    &self.tailscale_result
                } else {
                    &self.pihole_result
                };
                if let Some(result) = result {
                    (
                        result.columns.iter().map(String::as_str).collect(),
                        result.rows.clone(),
                    )
                } else {
                    (
                        vec!["Service", "Status"],
                        vec![vec![
                            self.page.title().into(),
                            if self.page == Page::Pihole {
                                "Ctrl+K → Connect to Pi-hole v6"
                            } else {
                                "r refresh · requires the tailscale CLI"
                            }
                            .into(),
                        ]],
                    )
                }
            }
            Page::System => (
                vec!["Capability", "Status", "Purpose"],
                self.snapshot
                    .capabilities
                    .iter()
                    .map(|c| {
                        vec![
                            c.command.clone(),
                            if c.available { "AVAILABLE" } else { "MISSING" }.into(),
                            c.purpose.clone(),
                        ]
                    })
                    .collect(),
            ),
        };
        rows.retain(|r| self.matches(&r.join(" ")));
        if self.sort == 1 {
            rows.sort_by_key(|r| r.first().cloned().unwrap_or_default());
        } else if self.sort == 2 {
            rows.sort_by_key(|r| r.get(1).cloned().unwrap_or_default());
        }
        (cols.into_iter().map(str::to_string).collect(), rows)
    }
    pub fn palette_results(&self, query: &str) -> Vec<Action> {
        let mut a: Vec<_> = actions()
            .into_iter()
            .filter_map(|a| {
                fuzzy_score(query, &format!("{} {}", a.name, a.hint)).map(|score| (score, a))
            })
            .collect();
        a.sort_by_key(|(score, _)| *score);
        a.into_iter().map(|(_, a)| a).collect()
    }
    pub fn search_results(&self, query: &str) -> Vec<(Page, String, String)> {
        let mut items = Vec::new();
        for i in &self.snapshot.interfaces {
            items.push((Page::Interfaces, "Interface".into(), i.name.clone()));
        }
        for c in &self.snapshot.connections {
            items.push((
                Page::Connections,
                "Connection".into(),
                format!("{} {} {}", c.local, c.remote, c.process),
            ));
        }
        for r in &self.snapshot.routes {
            items.push((
                Page::Routes,
                "Route".into(),
                format!("{} {} {}", r.destination, r.gateway, r.interface),
            ));
        }
        for n in &self.snapshot.neighbors {
            items.push((
                Page::Neighbors,
                "Neighbor".into(),
                format!("{} {}", n.ip, n.mac),
            ));
        }
        for d in &self.snapshot.dns {
            items.push((Page::Dns, "DNS".into(), d.clone()));
        }
        for network in &self.wifi {
            items.push((Page::Wifi, "Wi-Fi".into(), network.ssid.clone()));
        }
        for profile in &self.history.profiles {
            items.push((Page::Profiles, "Profile".into(), profile.name.clone()));
        }
        if let Some(result) = &self.result {
            for row in &result.rows {
                items.push((Page::Tools, "Tool result".into(), row.join(" ")));
            }
        }
        for e in &self.history.events {
            items.push((
                Page::Events,
                "Event".into(),
                format!("{} {}", e.kind, e.detail),
            ));
        }
        items
            .into_iter()
            .filter(|(_, _, label)| fuzzy_score(query, label).is_some())
            .take(100)
            .collect()
    }
}

pub fn fuzzy_score(query: &str, text: &str) -> Option<usize> {
    let q = query.to_lowercase();
    let t = text.to_lowercase();
    if q.is_empty() {
        return Some(0);
    }
    if let Some(pos) = t.find(&q) {
        return Some(pos);
    }
    let mut chars = q.chars();
    let mut target = chars.next()?;
    let mut score = 100;
    for (i, c) in t.chars().enumerate() {
        if c == target {
            score += i;
            if let Some(n) = chars.next() {
                target = n;
            } else {
                return Some(score);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn palette_is_fuzzy_and_searchable() {
        assert!(fuzzy_score("dnc", "DNS Control").is_some());
        let a = App::new(Config::default(), History::default());
        assert!(a.palette_results("mtu").iter().any(|a| a.id == "mtu"));
    }
    #[test]
    fn default_never_automatically_enables_external_probes() {
        let a = App::new(Config::default(), History::default());
        assert!(!a.config.external_enabled);
        assert!(!a.config.monitoring_enabled);
    }
    #[test]
    fn tool_consent_happens_before_external_execution() {
        let mut a = App::new(Config::default(), History::default());
        assert!(matches!(a.request_tool(Tool::PublicIp), Effect::None));
        assert!(matches!(a.modal, Some(Modal::ConfirmTool(_))));
    }
}
