//! Normalized service adapters. Pi-hole secrets never enter argv, history or reports.
use crate::{
    command::{clean, cmd, run},
    diagnosis::{Finding, Severity},
    model::ToolResult,
};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use zeroize::Zeroizing;

#[derive(Clone, Default)]
pub struct Secret(Arc<Zeroizing<String>>);
impl Secret {
    pub fn new(value: String) -> Self {
        Self(Arc::new(Zeroizing::new(value)))
    }
    fn value(&self) -> &str {
        self.0.as_str()
    }
}
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Tailnet {
    pub state: String,
    pub name: String,
    pub dns_suffix: String,
    pub self_name: String,
    pub ips: Vec<String>,
    pub exit_node: String,
    pub health: Vec<String>,
    pub peers: Vec<Peer>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Peer {
    pub name: String,
    pub ips: Vec<String>,
    pub os: String,
    pub online: Option<bool>,
    pub active: bool,
    pub path: String,
    pub exit_node: bool,
    pub exit_available: bool,
    pub rx: u64,
    pub tx: u64,
    pub last_seen: String,
}
fn string(v: &Value, key: &str) -> String {
    v[key].as_str().map(clean).unwrap_or_default()
}
fn strings(v: &Value, key: &str) -> Vec<String> {
    v[key]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).map(clean).collect())
        .unwrap_or_default()
}
pub fn parse_tailnet(v: &Value) -> Result<Tailnet> {
    if !v.is_object() || v["BackendState"].as_str().is_none() {
        bail!("Tailscale returned an unsupported status schema; update NEXUS or the CLI");
    }
    let mut peers = Vec::new();
    if let Some(map) = v["Peer"].as_object() {
        for p in map.values() {
            let current = string(p, "CurAddr");
            let relay = string(p, "Relay");
            peers.push(Peer {
                name: string(p, "DNSName").trim_end_matches('.').into(),
                ips: strings(p, "TailscaleIPs"),
                os: string(p, "OS"),
                online: p["Online"].as_bool(),
                active: p["Active"].as_bool().unwrap_or(false),
                path: if !current.is_empty() {
                    format!("direct {current}")
                } else if !relay.is_empty() {
                    format!("DERP {relay}")
                } else {
                    "idle / unknown".into()
                },
                exit_node: p["ExitNode"].as_bool().unwrap_or(false),
                exit_available: p["ExitNodeOption"].as_bool().unwrap_or(false),
                rx: p["RxBytes"].as_u64().unwrap_or(0),
                tx: p["TxBytes"].as_u64().unwrap_or(0),
                last_seen: string(p, "LastSeen"),
            });
        }
    }
    peers.sort_by(|a, b| b.online.cmp(&a.online).then(a.name.cmp(&b.name)));
    let exit = peers
        .iter()
        .find(|p| p.exit_node)
        .map(|p| p.name.clone())
        .or_else(|| {
            v["ExitNodeStatus"]["TailscaleIPs"].as_array().map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
        })
        .unwrap_or_else(|| "none".into());
    Ok(Tailnet {
        state: string(v, "BackendState"),
        name: string(&v["CurrentTailnet"], "Name"),
        dns_suffix: string(&v["CurrentTailnet"], "MagicDNSSuffix"),
        self_name: string(&v["Self"], "DNSName").trim_end_matches('.').into(),
        ips: strings(v, "TailscaleIPs"),
        exit_node: exit,
        health: strings(v, "Health"),
        peers,
    })
}
pub fn tailnet_result(tailnet: Tailnet) -> ToolResult {
    let mut r = ToolResult {
        title: "Tailscale".into(),
        at: Utc::now(),
        columns: [
            "Peer",
            "State",
            "IP addresses",
            "Path",
            "OS",
            "RX",
            "TX",
            "Exit node",
            "Last seen",
        ]
        .map(str::to_string)
        .to_vec(),
        ..Default::default()
    };
    r.metrics = BTreeMap::from([
        ("State".into(), tailnet.state.clone()),
        ("Tailnet".into(), tailnet.name.clone()),
        ("This device".into(), tailnet.self_name.clone()),
        ("Addresses".into(), tailnet.ips.join(" · ")),
        ("Exit node".into(), tailnet.exit_node.clone()),
        ("MagicDNS suffix".into(), tailnet.dns_suffix.clone()),
    ]);
    for p in &tailnet.peers {
        r.rows.push(vec![
            p.name.clone(),
            match p.online {
                Some(true) => "ONLINE",
                Some(false) => "OFFLINE",
                None => "UNKNOWN",
            }
            .into(),
            p.ips.join(" · "),
            p.path.clone(),
            p.os.clone(),
            crate::model::bytes(p.rx),
            crate::model::bytes(p.tx),
            if p.exit_node {
                "ACTIVE"
            } else if p.exit_available {
                "available"
            } else {
                "—"
            }
            .into(),
            p.last_seen.clone(),
        ]);
    }
    for health in &tailnet.health {
        r.findings.push(Finding::new(
            Severity::Warning,
            "Tailscale health warning",
            health,
            "Run an explicit tailnet ping or netcheck; review tailscaled health.",
        ));
    }
    if tailnet.state != "Running" {
        r.findings.push(Finding::new(Severity::Warning,"Tailscale is not running",format!("Backend state: {}",tailnet.state),"Use the Tailscale client to sign in or reconnect; NEXUS never starts a login automatically."));
    }
    r.notes.push("Local tailscaled status · direct/DERP reflects the last known path, not a fresh reachability test. Exit-node status does not prove every application routes through it.".into());
    r.tailscale = Some(tailnet);
    r
}
pub async fn tailscale_status() -> Result<ToolResult> {
    let v: Value = serde_json::from_str(&cmd("tailscale", &["status", "--json"]).await?)?;
    Ok(tailnet_result(parse_tailnet(&v)?))
}
pub async fn tailscale_ping(host: &str) -> Result<ToolResult> {
    crate::tools::validate_host(host)?;
    let output = run(
        "tailscale",
        &[
            "ping",
            "--c=3",
            "--timeout=3s",
            "--until-direct=false",
            host,
        ]
        .map(str::to_string),
        Duration::from_secs(15),
    )
    .await?;
    let mut r = ToolResult {
        title: "Tailscale path test".into(),
        at: Utc::now(),
        columns: vec!["Path / reply".into()],
        rows: clean(&output).lines().map(|s| vec![s.into()]).collect(),
        ..Default::default()
    };
    r.notes.push("Three explicit Tailscale discovery pings. The CLI has no JSON ping output; these labeled reply lines are retained as evidence. DERP is a relayed path, not a connectivity failure.".into());
    Ok(r)
}
pub async fn tailscale_netcheck() -> Result<ToolResult> {
    let out = run(
        "tailscale",
        &["netcheck", "--format=json"].map(str::to_string),
        Duration::from_secs(20),
    )
    .await?;
    let v: Value = serde_json::from_str(&out)?;
    let mut r = ToolResult {
        title: "Tailscale netcheck".into(),
        at: Utc::now(),
        columns: vec!["Check".into(), "Observed value".into()],
        ..Default::default()
    };
    for key in [
        "UDP",
        "IPv4",
        "IPv6",
        "MappingVariesByDestIP",
        "HairPinning",
        "UPnP",
        "PMP",
        "PCP",
        "PreferredDERP",
        "GlobalV4",
        "GlobalV6",
    ] {
        if let Some(value) = v.get(key) {
            r.rows.push(vec![key.into(), clean(&value.to_string())]);
        }
    }
    if let Some(map) = v["RegionLatency"].as_object() {
        for (region, ns) in map {
            r.rows.push(vec![
                format!("DERP region {region}"),
                ns.as_f64()
                    .map(|n| format!("{:.1} ms", n / 1e6))
                    .unwrap_or_else(|| "unknown".into()),
            ]);
        }
    }
    r.notes.push("Explicit STUN/DERP tests. JSON fields vary between CLI versions; unavailable observations are omitted.".into());
    Ok(r)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PiholeStatus {
    pub endpoint: String,
    pub blocking: String,
    pub timer: Option<f64>,
    pub queries: u64,
    pub blocked: u64,
    pub percent: f64,
    pub clients: u64,
    pub domains: u64,
    pub frequency: Option<f64>,
    pub history: Vec<PiholePoint>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PiholePoint {
    pub timestamp: f64,
    pub total: f64,
    pub blocked: f64,
}
pub fn endpoint(value: &str) -> Result<url::Url> {
    let mut u = url::Url::parse(value).context("Enter a Pi-hole HTTP(S) base URL")?;
    if !matches!(u.scheme(), "http" | "https")
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
    {
        bail!("Pi-hole URL must use HTTP(S), without credentials, query or fragment");
    }
    let path = u.path().trim_end_matches('/');
    let path = if path.ends_with("/api") {
        format!("{path}/")
    } else {
        format!("{path}/api/")
    };
    u.set_path(&path);
    Ok(u)
}
struct Session {
    client: reqwest::Client,
    base: url::Url,
    sid: Secret,
    created: bool,
}
impl Drop for Session {
    fn drop(&mut self) {
        if self.created {
            let client = self.client.clone();
            let url = self.base.join("auth").expect("constant path");
            let sid = self.sid.clone();
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    let _ = client
                        .delete(url)
                        .header("X-FTL-SID", sid.value())
                        .send()
                        .await;
                });
            }
        }
    }
}
impl Session {
    async fn connect(url: &str, password: &Secret) -> Result<Self> {
        let base = endpoint(url)?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .connect_timeout(Duration::from_secs(4))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()?;
        let mut s = Self {
            client,
            base,
            sid: Secret::default(),
            created: false,
        };
        let auth = s
            .request(
                reqwest::Method::POST,
                "auth",
                Some(json!({"password":password.value()})),
            )
            .await?;
        if auth["session"]["valid"].as_bool() != Some(true) {
            bail!("Pi-hole authentication failed. Use a v6 application password; two-factor accounts require it.");
        }
        if let Some(sid) = auth["session"]["sid"].as_str().filter(|s| !s.is_empty()) {
            s.sid = Secret::new(sid.into());
            s.created = true;
        }
        Ok(s)
    }
    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        payload: Option<Value>,
    ) -> Result<Value> {
        let mut request = self.client.request(method, self.base.join(path)?);
        if !self.sid.value().is_empty() {
            request = request.header("X-FTL-SID", self.sid.value());
        }
        if let Some(payload) = payload {
            request = request.json(&payload);
        }
        let response = request.send().await.map_err(|e| {
            if e.is_timeout() {
                anyhow::anyhow!("Pi-hole request timed out")
            } else if e.is_connect() {
                anyhow::anyhow!(
                    "Pi-hole connection failed; check its address, service and TLS trust"
                )
            } else {
                anyhow::anyhow!("Pi-hole HTTP transport failed")
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            bail!("Pi-hole API returned HTTP {} for {path}; check v6 compatibility, credentials and API permissions",status.as_u16());
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Pi-hole response interrupted")?;
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                bail!("Pi-hole response exceeded 2 MiB");
            }
            bytes.extend(chunk);
        }
        serde_json::from_slice(&bytes).context("Pi-hole returned invalid JSON (v6 API required)")
    }
}
pub fn parse_pihole(
    endpoint: &str,
    summary: &Value,
    blocking: &Value,
    history: &Value,
) -> Result<PiholeStatus> {
    if summary["queries"]["total"].as_u64().is_none()
        || summary["queries"]["blocked"].as_u64().is_none()
        || blocking["blocking"].as_str().is_none()
    {
        bail!("Unsupported Pi-hole response; NEXUS requires the v6 API");
    }
    let q = &summary["queries"];
    let mut points = Vec::new();
    if let Some(values) = history["history"].as_array() {
        for value in values {
            if let (Some(timestamp), Some(total), Some(blocked)) = (
                value["timestamp"].as_f64(),
                value["total"].as_f64(),
                value["blocked"].as_f64(),
            ) {
                if timestamp.is_finite() && total >= 0.0 && blocked >= 0.0 {
                    points.push(PiholePoint {
                        timestamp,
                        total,
                        blocked,
                    });
                }
            }
        }
    }
    points.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
    if points.len() > 300 {
        points.drain(..points.len() - 300);
    }
    Ok(PiholeStatus {
        endpoint: endpoint.into(),
        blocking: string(blocking, "blocking"),
        timer: blocking["timer"].as_f64(),
        queries: q["total"].as_u64().unwrap_or(0),
        blocked: q["blocked"].as_u64().unwrap_or(0),
        percent: q["percent_blocked"].as_f64().unwrap_or(0.0),
        clients: summary["clients"]["active"].as_u64().unwrap_or(0),
        domains: summary["gravity"]["domains_being_blocked"]
            .as_u64()
            .unwrap_or(0),
        frequency: q["frequency"].as_f64(),
        history: points,
    })
}
pub fn pihole_result(p: PiholeStatus) -> ToolResult {
    let mut r = ToolResult {
        title: "Pi-hole".into(),
        at: Utc::now(),
        columns: vec!["Metric".into(), "Observed value".into()],
        ..Default::default()
    };
    r.rows = vec![
        vec!["API endpoint".into(), p.endpoint.clone()],
        vec!["DNS blocking".into(), p.blocking.to_uppercase()],
        vec![
            "Timer remaining".into(),
            p.timer
                .map(|v| format!("{v:.0}s"))
                .unwrap_or_else(|| "none".into()),
        ],
        vec!["Queries (API window)".into(), p.queries.to_string()],
        vec![
            "Blocked queries".into(),
            format!("{} ({:.1}%)", p.blocked, p.percent),
        ],
        vec!["Active clients".into(), p.clients.to_string()],
        vec!["Gravity domains".into(), p.domains.to_string()],
        vec![
            "Query frequency".into(),
            p.frequency
                .map(|v| format!("{v:.2} queries/s"))
                .unwrap_or_else(|| "unavailable".into()),
        ],
    ];
    if p.blocking != "enabled" {
        r.findings.push(Finding::new(if p.blocking=="disabled"{Severity::Warning}else{Severity::Error},"Pi-hole blocking is not enabled",format!("API reports {} · timer {:?}",p.blocking,p.timer),"Check the service or use the explicit resume-blocking action. This does not establish DNS reachability."));
    }
    r.notes.push("Pi-hole v6 API observations. Query counts follow the server's statistics window. No per-client names or queried domains are retrieved. r refreshes; Ctrl+K opens controls.".into());
    r.pihole = Some(p);
    r
}
pub async fn pihole_status(url: &str, password: &Secret) -> Result<ToolResult> {
    let session = Session::connect(url, password).await?;
    let (summary, blocking) = tokio::try_join!(
        session.request(reqwest::Method::GET, "stats/summary", None),
        session.request(reqwest::Method::GET, "dns/blocking", None)
    )?;
    let history = session.request(reqwest::Method::GET, "history", None).await;
    let p = parse_pihole(
        url,
        &summary,
        &blocking,
        history.as_ref().unwrap_or(&Value::Null),
    )?;
    let mut r = pihole_result(p);
    if history.is_err() {
        r.notes
            .push("Activity history unavailable; summary and blocking state remain valid.".into());
    }
    Ok(r)
}
#[derive(Clone)]
pub struct PiholeMutation {
    pub url: String,
    pub password: Secret,
    pub enabled: bool,
    pub expected: bool,
    pub timer: Option<u32>,
    pub prepared: DateTime<Utc>,
}
pub async fn prepare_pihole(
    url: String,
    password: Secret,
    enabled: bool,
) -> Result<crate::control::Plan> {
    let r = pihole_status(&url, &password).await?;
    let state = r.pihole.context("Missing Pi-hole state")?;
    let current = match state.blocking.as_str() {
        "enabled" => true,
        "disabled" => false,
        _ => bail!("Pi-hole state is unknown; refusing to change blocking"),
    };
    if state.timer.is_some() {
        bail!("Pi-hole already has an active blocking timer; wait for it to finish before changing state");
    }
    let mutation = PiholeMutation {
        url: url.clone(),
        password: password.clone(),
        enabled,
        expected: current,
        timer: if enabled { None } else { Some(60) },
        prepared: Utc::now(),
    };
    let undo = PiholeMutation {
        url: url.clone(),
        password,
        enabled: current,
        expected: enabled,
        timer: None,
        prepared: Utc::now(),
    };
    Ok(crate::control::Plan{title:if enabled{"Resume Pi-hole blocking"}else{"Pause Pi-hole for 60 seconds"}.into(),summary:vec![format!("{url}: blocking {} → {}",state.blocking,if enabled{"enabled"}else{"disabled for 60s"}),"Changes only DNS filtering on the selected server. A pause automatically resumes after 60s. Captured state can be restored with u.".into()],steps:vec![crate::control::Step::pihole(mutation)],undo:vec![crate::control::Step::pihole(undo)],disruptive:false})
}
pub async fn apply_pihole(change: &PiholeMutation) -> Result<String> {
    if (Utc::now() - change.prepared).num_seconds() > 300 {
        bail!("Pi-hole preview expired; prepare a fresh change");
    }
    let session = Session::connect(&change.url, &change.password).await?;
    let before = session
        .request(reqwest::Method::GET, "dns/blocking", None)
        .await?;
    let expected = if change.expected {
        "enabled"
    } else {
        "disabled"
    };
    if before["blocking"].as_str() != Some(expected) {
        bail!("Pi-hole state changed since preview; refresh before applying");
    }
    let after = session
        .request(
            reqwest::Method::POST,
            "dns/blocking",
            Some(json!({"blocking":change.enabled,"timer":change.timer})),
        )
        .await?;
    if after["blocking"].as_str()
        != Some(if change.enabled {
            "enabled"
        } else {
            "disabled"
        })
    {
        bail!("Pi-hole did not confirm the requested state; refresh to inspect");
    }
    Ok("Applied; Pi-hole confirmed the requested blocking state".into())
}
