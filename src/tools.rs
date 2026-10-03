use crate::diagnosis::{Finding, Severity};
use crate::{
    backend::linux,
    command::{clean, cmd, run, run_with_input, split_nm},
    model::*,
};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    net::{IpAddr, Ipv4Addr},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Tool {
    Tailscale,
    TailscalePing {
        host: String,
    },
    TailscaleNetcheck,
    Pihole {
        url: String,
        #[serde(skip)]
        password: crate::integrations::Secret,
    },
    Diagnostics,
    Ping {
        host: String,
    },
    Dns {
        name: String,
        record: String,
        server: String,
    },
    CompareDns {
        name: String,
    },
    Trace {
        host: String,
        repeated: bool,
    },
    Http {
        url: String,
    },
    Tls {
        host: String,
    },
    Tcp {
        host: String,
        port: u16,
    },
    Udp {
        host: String,
        port: u16,
    },
    PublicIp,
    Speed {
        full: bool,
    },
    Iperf {
        host: String,
        full: bool,
    },
    Wifi {
        rescan: bool,
    },
    WifiSaved,
    WifiSurvey {
        interface: String,
    },
    Lan {
        cidr: String,
    },
    Firewall,
    Vpn,
    Containers,
    Namespaces {
        name: String,
    },
    Link {
        interface: String,
    },
    DnsStatus,
    Dot {
        server: String,
        identity: String,
        query: String,
        port: u16,
    },
    Proxy,
    IpInfo {
        ip: String,
    },
    ProcessTraffic {
        interface: String,
    },
    RouteRules,
    Developer,
    HttpFamilies {
        url: String,
    },
}
impl Tool {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Tailscale => "Tailscale",
            Self::TailscalePing { .. } => "Tailscale path test",
            Self::TailscaleNetcheck => "Tailscale netcheck",
            Self::Pihole { .. } => "Pi-hole",
            Self::Diagnostics => "Network diagnostics",
            Self::Ping { .. } => "ICMP test",
            Self::Dns { .. } => "DNS lookup",
            Self::CompareDns { .. } => "Resolver comparison",
            Self::Trace { repeated: true, .. } => "Repeated path analysis (MTR)",
            Self::Trace { .. } => "Traceroute",
            Self::Http { .. } => "HTTP / HTTPS inspector",
            Self::Tls { .. } => "TLS certificate inspector",
            Self::Tcp { .. } => "TCP connection test",
            Self::Udp { .. } => "UDP response check",
            Self::PublicIp => "Public IP",
            Self::Speed { .. } => "Internet speed test",
            Self::Iperf { .. } => "iperf3 bandwidth test",
            Self::Wifi { .. } => "Nearby Wi-Fi",
            Self::WifiSaved => "Saved Wi-Fi profiles",
            Self::WifiSurvey { .. } => "Wi-Fi channel survey",
            Self::Lan { .. } => "LAN discovery",
            Self::Firewall => "Firewall overview",
            Self::Vpn => "VPN overview",
            Self::Containers => "Container networking",
            Self::Namespaces { .. } => "Network namespaces",
            Self::Link { .. } => "Ethernet link details",
            Self::DnsStatus => "DNS status and cache statistics",
            Self::Dot { .. } => "DNS-over-TLS inspector",
            Self::Proxy => "Proxy configuration",
            Self::IpInfo { .. } => "IP ASN / region lookup",
            Self::ProcessTraffic { .. } => "Per-process bandwidth sample",
            Self::RouteRules => "Policy routing rules",
            Self::Developer => "Developer endpoint dashboard",
            Self::HttpFamilies { .. } => "IPv4 / IPv6 HTTP comparison",
        }
    }
    pub fn requires_external_consent(&self) -> bool {
        matches!(
            self,
            Self::Pihole { .. }
                | Self::TailscalePing { .. }
                | Self::TailscaleNetcheck
                | Self::Diagnostics
                | Self::Ping { .. }
                | Self::Dns { .. }
                | Self::Dot { .. }
                | Self::CompareDns { .. }
                | Self::Trace { .. }
                | Self::Http { .. }
                | Self::Tls { .. }
                | Self::Tcp { .. }
                | Self::Udp { .. }
                | Self::PublicIp
                | Self::Speed { .. }
                | Self::Iperf { .. }
                | Self::Lan { .. }
                | Self::IpInfo { .. }
                | Self::HttpFamilies { .. }
        )
    }
    pub fn consent_note(&self) -> String {
        match self {
            Self::TailscalePing{host}=>format!("Sends three Tailscale discovery pings to {host}. May use a DERP relay; does not modify your tailnet."),
            Self::TailscaleNetcheck=>"Contacts Tailscale STUN/DERP servers to inspect NAT, UDP reachability and relay latency. No settings are changed.".into(),
            Self::Pihole{url,..}=>format!("Reads status and aggregate activity from {url} using Pi-hole v6. Credentials stay in memory and are sent only to this server. Redirects are refused. {} Successful connection enables a 10-second refresh while the Pi-hole page is visible; p pauses polling.",if url.starts_with("http:"){ "HTTP sends credentials without TLS; use HTTPS or a trusted encrypted network." }else{"HTTPS verifies the server certificate."}),
            Self::Speed{..}=>"Contacts the selected speed-test provider and can consume substantial data. Ookla may ask you to accept its license/privacy policy separately. No acceptance flags are added automatically.".into(),
            Self::Lan{cidr}=>format!("Sends one ICMP probe per address on {cidr}. Only a directly connected private IPv4 subnet of /24 or smaller is accepted."),
            Self::PublicIp=>"Contacts api.ipify.org for IPv4/IPv6. The service sees your source IP. Geolocation is not sent or queried automatically.".into(),
            Self::CompareDns{..}=>"Sends this DNS name to Cloudflare, Google, Quad9 and AdGuard, with three queries per resolver.".into(),
            Self::Dot{server,identity,query,port}=>format!("Sends one A query for {query} to {server}:{port} over TLS, validating the certificate against {identity}. This tests the selected resolver; it does not change system DNS."),
            Self::IpInfo{ip}=>format!("Sends {ip} to ipapi.co to retrieve ASN, ISP organization and approximate region/country. This is external metadata, not locally verified location."),
            Self::ProcessTraffic{interface}=>format!("Locally samples process traffic on {interface} for five intervals using NetHogs. Packet-capture privileges are needed; pkexec may request authorization. No payload capture file is stored."),
            _=>"Sends network requests to the entered target or configured diagnostic endpoints. Results are stored only on this machine.".into()
        }
    }
}

fn result(title: &str, columns: &[&str]) -> ToolResult {
    ToolResult {
        title: title.into(),
        at: Utc::now(),
        columns: columns.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}
fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

pub fn validate_host(host: &str) -> Result<()> {
    if host.is_empty()
        || host.len() > 253
        || host.starts_with('-')
        || host.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        bail!("Enter a hostname or IP address without spaces or options");
    }
    if host.parse::<IpAddr>().is_ok() {
        return Ok(());
    }
    if !host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
    {
        bail!("Invalid hostname");
    }
    Ok(())
}
fn validate_url(value: &str) -> Result<url::Url> {
    let u = url::Url::parse(value).context("Enter an http:// or https:// URL")?;
    if !matches!(u.scheme(), "http" | "https") || u.host_str().is_none() {
        bail!("Only HTTP/HTTPS URLs are supported");
    }
    if !u.username().is_empty() || u.password().is_some() {
        bail!("Credentials in URLs are not stored or accepted");
    }
    Ok(u)
}

pub async fn execute(
    tool: Tool,
    snapshot: Snapshot,
    config: crate::config::Config,
) -> Result<ToolResult> {
    let kind = tool.clone();
    let title = tool.title();
    match execute_inner(tool, snapshot, config).await {
        Ok(mut r) => {
            if matches!(kind, Tool::Http { .. }) {
                if let Some(code) = r.metrics.get("http_code").and_then(|v| v.parse().ok()) {
                    r.findings.push(crate::diagnosis::http_status(code));
                }
            }
            Ok(r)
        }
        Err(e)
            if matches!(
                kind,
                Tool::Http { .. } | Tool::Tls { .. } | Tool::Dns { .. }
            ) =>
        {
            let mut finding = crate::diagnosis::transport_failure(&e.to_string());
            if matches!(kind, Tool::Dns { .. }) && finding.severity != Severity::Unknown {
                finding.title = "DNS query failed".into();
                finding.next_step = "Check the name, resolver response, DNS configuration and DNS-over-TLS if enabled.".into();
            }
            let mut r = result(title, &["Status", "Finding", "Evidence", "Next step"]);
            r.rows.push(finding.row());
            r.findings.push(finding);
            Ok(r)
        }
        Err(e) => Err(e),
    }
}

async fn execute_inner(
    tool: Tool,
    snapshot: Snapshot,
    config: crate::config::Config,
) -> Result<ToolResult> {
    match tool {
        Tool::Tailscale => crate::integrations::tailscale_status().await,
        Tool::TailscalePing { host } => crate::integrations::tailscale_ping(&host).await,
        Tool::TailscaleNetcheck => crate::integrations::tailscale_netcheck().await,
        Tool::Pihole { url, password } => crate::integrations::pihole_status(&url, &password).await,
        Tool::Dot {
            server,
            identity,
            query,
            port,
        } => crate::dot::inspect(&server, &identity, &query, port).await,
        Tool::Ping { host } => {
            validate_host(&host)?;
            let out = run(
                "ping",
                &args(&["-n", "-c", "5", "-W", "2", "--", &host]),
                Duration::from_secs(15),
            )
            .await?;
            let mut r = result("ICMP test", &["Reply / summary"]);
            r.rows = clean(&out).lines().map(|l| vec![l.into()]).collect();
            r.notes.push("ICMP may be filtered even when HTTPS works; packet loss here is ICMP response loss.".into());
            Ok(r)
        }
        Tool::Dns {
            name,
            record,
            server,
        } => dns_lookup(&name, &record, &server).await,
        Tool::CompareDns { name } => {
            validate_host(&name)?;
            let mut r = result(
                "Resolver comparison",
                &["Provider", "Resolver", "Successful", "Median", "Answers"],
            );
            for (provider, server) in [
                ("Cloudflare", "1.1.1.1"),
                ("Google", "8.8.8.8"),
                ("Quad9", "9.9.9.9"),
                ("AdGuard", "94.140.14.14"),
            ] {
                let mut times = Vec::new();
                let mut answers = String::new();
                for _ in 0..3 {
                    if let Ok(d) = dns_lookup(&name, "A", server).await {
                        if let Some(ms) = d
                            .metrics
                            .get("Query time (ms)")
                            .and_then(|n| n.parse::<f64>().ok())
                        {
                            times.push(ms);
                        }
                        answers = d
                            .rows
                            .iter()
                            .filter_map(|row| row.last())
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ");
                    }
                }
                times.sort_by(f64::total_cmp);
                let median = times
                    .get(times.len() / 2)
                    .map(|n| format!("{n:.1} ms"))
                    .unwrap_or_else(|| "Failed".into());
                r.rows.push(vec![
                    provider.into(),
                    server.into(),
                    format!("{}/3", times.len()),
                    median,
                    answers,
                ]);
            }
            r.notes.push("Query durations are resolver/network observations, affected by cache, anycast and local policy; three samples are not a reliability guarantee.".into());
            Ok(r)
        }
        Tool::Http { url } => http(&url).await,
        Tool::Tls { host } => tls(&host).await,
        Tool::Tcp { host, port } => {
            validate_host(&host)?;
            if port == 0 {
                bail!("Port must be 1–65535");
            }
            let start = Instant::now();
            let socket = tokio::time::timeout(
                Duration::from_secs(5),
                tokio::net::TcpStream::connect((host.as_str(), port)),
            )
            .await??;
            let mut r = result("TCP connection test", &["Target", "Peer", "Connected in"]);
            r.rows.push(vec![
                format!("{host}:{port}"),
                socket.peer_addr()?.to_string(),
                format!("{:.2} ms", start.elapsed().as_secs_f64() * 1000.0),
            ]);
            r.notes
                .push("Duration includes DNS resolution when the target is a hostname.".into());
            Ok(r)
        }
        Tool::Udp { host, port } => {
            validate_host(&host)?;
            if port == 0 {
                bail!("Port must be 1–65535");
            }
            let addr = tokio::time::timeout(
                Duration::from_secs(5),
                tokio::net::lookup_host((host.as_str(), port)),
            )
            .await??
            .next()
            .context("No address")?;
            let s = tokio::net::UdpSocket::bind(if addr.is_ipv4() {
                "0.0.0.0:0"
            } else {
                "[::]:0"
            })
            .await?;
            s.connect(addr).await?;
            let start = Instant::now();
            s.send(b"NEXUS connectivity check").await?;
            let mut buf = [0u8; 4096];
            let reply = tokio::time::timeout(Duration::from_secs(3), s.recv(&mut buf)).await;
            let mut r = result("UDP response check", &["Target", "Result"]);
            r.rows.push(vec![
                addr.to_string(),
                match reply {
                    Ok(Ok(n)) => format!(
                        "{n} bytes returned in {:.1} ms",
                        start.elapsed().as_secs_f64() * 1000.0
                    ),
                    Ok(Err(e)) => e.to_string(),
                    Err(_) => {
                        "No reply; open/filtered/unsupported payload are indistinguishable".into()
                    }
                },
            ]);
            Ok(r)
        }
        Tool::Trace { host, repeated } => trace(&host, repeated).await,
        Tool::PublicIp => {
            let mut r = result(
                "Public IP (external service)",
                &["Family", "Address", "Source"],
            );
            for family in ["-4", "-6"] {
                match run(
                    "curl",
                    &args(&[
                        family,
                        "--fail",
                        "--silent",
                        "--show-error",
                        "--connect-timeout",
                        "5",
                        "--max-time",
                        "8",
                        "--max-filesize",
                        "4096",
                        "--url",
                        &config.public_ip_url,
                    ]),
                    Duration::from_secs(10),
                )
                .await
                {
                    Ok(ip) => {
                        let ip = ip.trim();
                        ip.parse::<IpAddr>()
                            .context("IP service returned an invalid address")?;
                        r.rows
                            .push(vec![family.into(), ip.into(), config.public_ip_url.clone()]);
                    }
                    Err(e) => r
                        .rows
                        .push(vec![family.into(), "Unavailable".into(), e.to_string()]),
                }
            }
            r.notes.push("ISP/ASN/region requires a separately selected external service; not inferred from a local interface. IPv6 failure can indicate absent IPv6 or service restrictions.".into());
            Ok(r)
        }
        Tool::Speed { full } => speed(full).await,
        Tool::Iperf { host, full } => iperf(&host, full).await,
        Tool::Wifi { rescan } => wifi(rescan).await,
        Tool::WifiSaved => saved_wifi().await,
        Tool::WifiSurvey { interface } => {
            crate::control::validate_interface(&interface, &snapshot)?;
            let out = cmd("iw", &["dev", &interface, "survey", "dump"]).await?;
            let mut r = result("Wi-Fi channel utilization", &["Survey"]);
            r.rows = out.lines().map(|l| vec![clean(l)]).collect();
            r.notes.push("Survey support depends on driver and privileges. Scan counts describe nearby APs, not actual airtime utilization.".into());
            Ok(r)
        }
        Tool::Lan { cidr } => lan(&cidr, &snapshot).await,
        Tool::Diagnostics => diagnostics(&snapshot, &config).await,
        Tool::Firewall => firewall().await,
        Tool::Vpn => vpn(&snapshot).await,
        Tool::Containers => containers().await,
        Tool::Namespaces { name } => {
            let mut r = result("Network namespaces", &["Namespace", "Details"]);
            if name.is_empty() {
                for name in linux::namespaces() {
                    r.rows.push(vec![
                        name,
                        "Named namespace; inspect through palette".into(),
                    ]);
                }
                if let Ok(out) = cmd(
                    "lsns",
                    &[
                        "--type",
                        "net",
                        "--json",
                        "--output",
                        "NS,TYPE,NPROCS,PID,USER,COMMAND",
                    ],
                )
                .await
                {
                    if let Ok(v) = serde_json::from_str::<Value>(&out) {
                        for n in v["namespaces"].as_array().into_iter().flatten() {
                            r.rows.push(vec![
                                n["ns"].to_string(),
                                format!(
                                    "PID {} · processes {} · user {}",
                                    n["pid"], n["nprocs"], n["user"]
                                ),
                            ]);
                        }
                    }
                }
            } else {
                if !linux::namespaces().contains(&name) {
                    bail!("Choose an existing named namespace");
                }
                let out = cmd("ip", &["-n", &name, "-j", "address", "show"]).await?;
                let v: Vec<Value> = serde_json::from_str(&out)?;
                for i in v {
                    r.rows.push(vec![
                        i["ifname"].as_str().unwrap_or_default().into(),
                        i["addr_info"].to_string(),
                    ]);
                }
            }
            r.notes.push(
                "Namespace inspection may require root. No namespace is created or deleted.".into(),
            );
            Ok(r)
        }
        Tool::Link { interface } => {
            crate::control::validate_interface(&interface, &snapshot)?;
            let out = cmd("ethtool", &[&interface]).await?;
            Ok(lines("Ethernet link capabilities", &out))
        }
        Tool::DnsStatus => {
            let mut r = result("Resolver status", &["Property", "Value"]);
            for action in ["status", "statistics"] {
                match cmd("resolvectl", &[action]).await {
                    Ok(out) => r
                        .rows
                        .extend(out.lines().map(|l| vec![action.into(), clean(l)])),
                    Err(e) => r.notes.push(e.to_string()),
                }
            }
            r.notes.push("systemd-resolved exposes statistics, not a portable enumeration of cached DNS records. DoT status is reported by resolvectl when supported.".into());
            Ok(r)
        }
        Tool::Proxy => {
            let mut r = result("Proxy configuration", &["Source", "Value"]);
            for (k, v) in snapshot.proxies {
                r.rows.push(vec![k, v]);
            }
            for key in ["mode", "autoconfig-url", "ignore-hosts"] {
                if let Ok(out) = cmd("gsettings", &["get", "org.gnome.system.proxy", key]).await {
                    r.rows.push(vec![
                        format!("GNOME {key}"),
                        linux::redact_proxy(out.trim()),
                    ]);
                }
            }
            r.notes.push("Environment and GNOME settings may differ from application-specific proxies. Credentials are redacted.".into());
            Ok(r)
        }
        Tool::IpInfo { ip } => {
            let address: IpAddr = ip.parse().context("Enter an IP address")?;
            let u = format!("https://ipapi.co/{address}/json/");
            let out = run(
                "curl",
                &args(&[
                    "--fail",
                    "--silent",
                    "--show-error",
                    "--connect-timeout",
                    "5",
                    "--max-time",
                    "10",
                    "--max-filesize",
                    "65536",
                    "--url",
                    &u,
                ]),
                Duration::from_secs(12),
            )
            .await?;
            let v: Value = serde_json::from_str(&out)?;
            if v["error"].as_bool() == Some(true) {
                bail!("IP metadata service: {}", v["reason"]);
            }
            let mut r = result(
                "IP metadata (external service)",
                &["Property", "Externally retrieved value"],
            );
            for key in ["ip", "asn", "org", "region", "country_name", "city"] {
                if let Some(value) = v[key].as_str() {
                    r.rows.push(vec![key.into(), clean(value)]);
                }
            }
            r.notes.push("Source: ipapi.co. Geographic location is approximate and can reflect an ISP, VPN exit or hosting provider. No precise user location is inferred.".into());
            Ok(r)
        }
        Tool::ProcessTraffic { interface } => process_traffic(&interface, &snapshot).await,
        Tool::RouteRules => {
            let mut r = result(
                "Policy routing rules",
                &["Family", "Priority", "From", "To", "Table / action"],
            );
            for family in ["-4", "-6"] {
                let out = cmd("ip", &["-j", family, "rule", "show"]).await?;
                let rules: Vec<Value> = serde_json::from_str(&out)?;
                for rule in rules {
                    r.rows.push(vec![
                        family.into(),
                        rule["priority"].to_string(),
                        rule["src"].as_str().unwrap_or("all").into(),
                        rule["dst"].as_str().unwrap_or("all").into(),
                        rule["table"]
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| rule.to_string()),
                    ]);
                }
            }
            r.notes.push("Read-only policy routing. Interpret VPN/default routes together with rules and all tables.".into());
            Ok(r)
        }
        Tool::Developer => developer(&snapshot).await,
        Tool::HttpFamilies { url } => {
            let mut r = result(
                "IPv4 / IPv6 HTTP comparison",
                &["Family", "Remote IP", "Status", "Total", "Observation"],
            );
            for family in ["-4", "-6"] {
                match http_family(&url, Some(family)).await {
                    Ok(h) => r.rows.push(vec![
                        family.into(),
                        h.metrics.get("remote_ip").cloned().unwrap_or_default(),
                        h.metrics.get("http_code").cloned().unwrap_or_default(),
                        h.metrics.get("Total").cloned().unwrap_or_default(),
                        "Connected".into(),
                    ]),
                    Err(e) => r.rows.push(vec![
                        family.into(),
                        "—".into(),
                        "—".into(),
                        "—".into(),
                        e.to_string(),
                    ]),
                }
            }
            r.notes.push("Both tests use HEAD and verify TLS normally. A hostname may publish only one address family.".into());
            Ok(r)
        }
    }
}

pub async fn ping_once(host: &str) -> Option<f64> {
    if validate_host(host).is_err() {
        return None;
    }
    let out = run(
        "ping",
        &args(&["-n", "-c", "1", "-W", "1", "--", host]),
        Duration::from_secs(3),
    )
    .await
    .ok()?;
    parse_ping(&out)
}
pub fn parse_ping(out: &str) -> Option<f64> {
    out.lines().find_map(|line| {
        let tail = line
            .split("time=")
            .nth(1)
            .or_else(|| line.split("time<").nth(1))?;
        tail.split_whitespace().next()?.parse().ok()
    })
}

pub async fn dns_lookup(name: &str, record: &str, server: &str) -> Result<ToolResult> {
    validate_host(name)?;
    let record = record.to_ascii_uppercase();
    if !["A", "AAAA", "MX", "TXT", "CNAME", "NS", "PTR", "SOA"].contains(&record.as_str()) {
        bail!("Supported record types: A AAAA MX TXT CNAME NS PTR SOA");
    }
    if !server.is_empty() {
        server
            .parse::<IpAddr>()
            .context("DNS resolver must be an IP address")?;
    }
    let mut a = args(&[
        "+time=2",
        "+tries=1",
        "+noall",
        "+answer",
        "+comments",
        "+stats",
    ]);
    if !server.is_empty() {
        a.push(format!("@{server}"));
    }
    if record == "PTR" {
        name.parse::<IpAddr>()
            .context("PTR lookup requires an IP address")?;
        a.extend(args(&["-x", name]));
    } else {
        a.extend(args(&[name, &record]));
    }
    let out = run("dig", &a, Duration::from_secs(4)).await?;
    let mut r = result("DNS lookup", &["Name", "TTL", "Type", "Answer"]);
    for line in out.lines() {
        if line.contains("status:") {
            r.notes.push(clean(line.trim_start_matches(';').trim()));
        }
        if let Some(v) = line.strip_prefix(";; Query time:") {
            r.metrics.insert(
                "Query time (ms)".into(),
                v.split_whitespace().next().unwrap_or("?").into(),
            );
        }
        if line.starts_with(';') || line.trim().is_empty() {
            continue;
        }
        let f: Vec<_> = line.split_whitespace().collect();
        if f.len() >= 5 {
            r.rows.push(vec![
                clean(f[0]),
                f[1].into(),
                f[3].into(),
                clean(&f[4..].join(" ")),
            ]);
        }
    }
    if !r.notes.iter().any(|s| s.contains("status: NOERROR")) {
        bail!("DNS response did not succeed: {}", r.notes.join(" "));
    }
    if r.rows.is_empty() {
        r.notes
            .push("Successful response with no records of this type.".into());
    }
    r.metrics.insert(
        "Resolver".into(),
        if server.is_empty() {
            "System resolver".into()
        } else {
            server.into()
        },
    );
    Ok(r)
}

async fn http(value: &str) -> Result<ToolResult> {
    http_family(value, None).await
}
async fn http_family(value: &str, family: Option<&str>) -> Result<ToolResult> {
    let url = validate_url(value)?;
    let mut arguments = args(&[
        "--silent",
        "--show-error",
        "--location",
        "--max-redirs",
        "8",
        "--connect-timeout",
        "5",
        "--max-time",
        "20",
        "--proto",
        "=http,https",
        "--proto-redir",
        "=http,https",
        "--max-filesize",
        "1048576",
        "--head",
        "--dump-header",
        "-",
        "--output",
        "/dev/null",
        "--write-out",
        "\nNEXUS_TIMING:%{json}",
        "--url",
        url.as_str(),
    ]);
    if let Some(family) = family {
        arguments.push(family.into());
    }
    let out = run("curl", &arguments, Duration::from_secs(22)).await?;
    let (headers, timing) = out
        .rsplit_once("NEXUS_TIMING:")
        .context("curl lacks JSON write-out support (need curl 7.70+)")?;
    let v: Value = serde_json::from_str(timing.trim())?;
    let mut r = result("HTTP / HTTPS inspector", &["Redirect / header", "Value"]);
    for line in headers.lines().filter(|l| !l.trim().is_empty()) {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let secret = ["cookie", "auth", "token", "secret", "api-key"]
                .iter()
                .any(|s| key.to_ascii_lowercase().contains(s));
            r.rows.push(vec![
                clean(key),
                if secret {
                    "[redacted]".into()
                } else {
                    clean(value.trim())
                },
            ]);
        } else {
            r.rows.push(vec![clean(line), String::new()]);
        }
    }
    let n = |key: &str| v[key].as_f64().unwrap_or(0.0) * 1000.0;
    let dns = n("time_namelookup");
    let connect = n("time_connect");
    let tls = n("time_appconnect");
    for (k, val) in [
        ("DNS", dns),
        ("TCP connect", (connect - dns).max(0.0)),
        (
            "TLS",
            if tls > 0.0 {
                (tls - connect).max(0.0)
            } else {
                0.0
            },
        ),
        ("TTFB (cumulative)", n("time_starttransfer")),
        ("Total", n("time_total")),
    ] {
        r.metrics.insert(k.into(), format!("{val:.2} ms"));
    }
    for key in [
        "remote_ip",
        "local_ip",
        "http_code",
        "http_version",
        "num_redirects",
        "ssl_verify_result",
    ] {
        r.metrics
            .insert(key.into(), v[key].to_string().trim_matches('"').to_string());
    }
    r.notes.push("Uses HEAD with normal certificate validation. Some servers reject HEAD. Timings include redirects; stage differences are indicative for multi-hop requests. Cookies are redacted; curl never receives Wi-Fi credentials.".into());
    Ok(r)
}

async fn tls(host: &str) -> Result<ToolResult> {
    validate_host(host)?;
    let connect = if host.contains(':') {
        format!("[{host}]:443")
    } else {
        format!("{host}:443")
    };
    let check = if host.parse::<IpAddr>().is_ok() {
        "-verify_ip"
    } else {
        "-verify_hostname"
    };
    let out = run_with_input(
        "openssl",
        &args(&[
            "s_client",
            "-connect",
            &connect,
            "-servername",
            host,
            "-showcerts",
            check,
            host,
            "-verify_return_error",
        ]),
        Some(Vec::new()),
        Duration::from_secs(12),
    )
    .await?;
    let begin = out
        .find("-----BEGIN CERTIFICATE-----")
        .context("Server returned no certificate")?;
    let end = out[begin..]
        .find("-----END CERTIFICATE-----")
        .context("Malformed certificate")?
        + begin
        + "-----END CERTIFICATE-----".len();
    let cert = &out[begin..end];
    let details = run_with_input(
        "openssl",
        &args(&[
            "x509",
            "-noout",
            "-subject",
            "-issuer",
            "-dates",
            "-ext",
            "subjectAltName",
        ]),
        Some(cert.as_bytes().to_vec()),
        Duration::from_secs(3),
    )
    .await?;
    let mut r = result("TLS certificate inspector", &["Property", "Value"]);
    for line in details.lines() {
        if let Some((k, v)) = line.split_once('=') {
            r.rows.push(vec![clean(k), clean(v)]);
        } else {
            r.rows.push(vec!["SAN".into(), clean(line.trim())]);
        }
    }
    for line in out.lines() {
        if line.contains("Cipher")
            || line.contains("Protocol")
            || line.contains("Verification")
            || line.contains("Verify return")
            || line.trim_start().starts_with("depth=")
        {
            r.rows.push(vec!["TLS".into(), clean(line.trim())]);
        }
    }
    let expiry = details.lines().find_map(|l| l.strip_prefix("notAfter="));
    if let Some(v) = expiry {
        if let Ok(d) = chrono::NaiveDateTime::parse_from_str(v.trim(), "%b %e %H:%M:%S %Y GMT") {
            let days = (d.and_utc() - Utc::now()).num_days();
            r.metrics.insert("Days remaining".into(), days.to_string());
            if days < 30 {
                r.notes.push(format!("Certificate expires in {days} days"));
            }
        }
    }
    r.metrics.insert(
        "Certificates sent by server".into(),
        out.matches("-----BEGIN CERTIFICATE-----")
            .count()
            .to_string(),
    );
    r.notes.push("Hostname and certificate chain verification are enforced. A validation failure is reported as an error; verification is never disabled automatically.".into());
    Ok(r)
}

async fn trace(host: &str, repeated: bool) -> Result<ToolResult> {
    validate_host(host)?;
    if repeated {
        let out = run(
            "mtr",
            &args(&[
                "--json",
                "--report",
                "--report-cycles",
                "10",
                "--no-dns",
                host,
            ]),
            Duration::from_secs(45),
        )
        .await?;
        let v: Value = serde_json::from_str(&out)?;
        let mut r = result(
            "MTR path analysis",
            &[
                "Hop", "Host", "Loss %", "Sent", "Last", "Avg", "Best", "Worst",
            ],
        );
        r.metrics.insert("Target".into(), host.into());
        for hop in v["report"]["hubs"].as_array().into_iter().flatten() {
            r.rows.push(
                [
                    "count", "host", "Loss%", "Snt", "Last", "Avg", "Best", "Wrst",
                ]
                .iter()
                .map(|k| hop[k].to_string().trim_matches('"').to_string())
                .collect(),
            );
        }
        r.notes.push("Intermediate routers may rate-limit ICMP. Loss at a hop is not end-to-end loss unless it continues at the destination.".into());
        Ok(r)
    } else {
        let out = run(
            "traceroute",
            &args(&["-n", "-m", "20", "-w", "1", "-q", "1", "--", host]),
            Duration::from_secs(25),
        )
        .await?;
        let mut r = result("Traceroute", &["Hop", "Address", "Probe"]);
        r.metrics.insert("Target".into(), host.into());
        for line in out.lines().skip(1) {
            let f: Vec<_> = line.split_whitespace().collect();
            if f.len() >= 2 {
                r.rows
                    .push(vec![f[0].into(), f[1].into(), f[2..].join(" ")]);
            }
        }
        r.notes.push("One probe per hop. Asterisk means no response, not proof that traffic cannot traverse the router.".into());
        Ok(r)
    }
}

async fn speed(full: bool) -> Result<ToolResult> {
    if !full {
        bail!("A quick bandwidth test needs a server you control. Use the 5-second iperf3 test. Internet speed backends are full tests; no misleading partial estimate is shown.");
    }
    let (backend, out) = if crate::command::available("speedtest") {
        (
            "Ookla speedtest",
            run(
                "speedtest",
                &args(&["--format=json", "--progress=no"]),
                Duration::from_secs(180),
            )
            .await?,
        )
    } else {
        (
            "Python speedtest-cli",
            run(
                "speedtest-cli",
                &args(&["--json", "--secure"]),
                Duration::from_secs(180),
            )
            .await?,
        )
    };
    let v:Value=serde_json::from_str(&out).context("Backend output is not compatible JSON. Install Ookla's official speedtest or Python speedtest-cli.")?;
    let mut r = result("Internet speed test", &["Metric", "Measured value"]);
    let ookla = backend.starts_with("Ookla");
    let download = if ookla {
        v["download"]["bandwidth"].as_f64().unwrap_or(0.0) * 8.0
    } else {
        v["download"].as_f64().unwrap_or(0.0)
    };
    let upload = if ookla {
        v["upload"]["bandwidth"].as_f64().unwrap_or(0.0) * 8.0
    } else {
        v["upload"].as_f64().unwrap_or(0.0)
    };
    r.metrics
        .insert("Download Mbps".into(), format!("{:.2}", download / 1e6));
    r.metrics
        .insert("Upload Mbps".into(), format!("{:.2}", upload / 1e6));
    r.metrics.insert("Backend".into(), backend.into());
    r.metrics.insert("Server".into(), v["server"].to_string());
    for key in ["ping", "packetLoss", "server", "timestamp"] {
        if !v[key].is_null() {
            r.rows.push(vec![key.into(), v[key].to_string()]);
        }
    }
    if ookla {
        for direction in ["download", "upload"] {
            r.rows.push(vec![
                format!("{direction} duration"),
                format!("{} ms", v[direction]["elapsed"]),
            ]);
        }
    }
    r.notes.push("This is an explicit external bandwidth test. Backend/server are recorded with each result; unsupported metrics are omitted.".into());
    Ok(r)
}
async fn iperf(host: &str, full: bool) -> Result<ToolResult> {
    validate_host(host)?;
    let seconds = if full { "15" } else { "5" };
    let mut r = result(
        "iperf3 bidirectional bandwidth",
        &["Direction", "Mbps", "Duration", "Retransmits"],
    );
    for reverse in [false, true] {
        let mut a = args(&["-c", host, "-J", "-t", seconds, "--connect-timeout", "5000"]);
        if reverse {
            a.push("-R".into());
        }
        let out = run(
            "iperf3",
            &a,
            Duration::from_secs(if full { 25 } else { 15 }),
        )
        .await?;
        let v: Value = serde_json::from_str(&out)?;
        if let Some(e) = v["error"].as_str() {
            bail!("iperf3: {e}");
        }
        let s = &v["end"]["sum_received"];
        let mbps = s["bits_per_second"]
            .as_f64()
            .context("iperf3 returned no bandwidth")?
            / 1e6;
        r.rows.push(vec![
            if reverse { "Download" } else { "Upload" }.into(),
            format!("{mbps:.2}"),
            format!("{} s", s["seconds"]),
            v["end"]["sum_sent"]["retransmits"].to_string(),
        ]);
        r.metrics.insert(
            if reverse {
                "Download Mbps"
            } else {
                "Upload Mbps"
            }
            .into(),
            format!("{mbps:.2}"),
        );
    }
    r.metrics
        .insert("Backend".into(), format!("iperf3 / {host}:5201"));
    r.notes.push("Requires an iperf3 server you own or have permission to test. Measures this path, not necessarily your internet subscription.".into());
    Ok(r)
}

pub async fn scan_wifi(rescan: bool) -> Result<Vec<WifiNetwork>> {
    let out = run(
        "nmcli",
        &args(&[
            "-t",
            "-f",
            "IN-USE,SSID,BSSID,SIGNAL,FREQ,CHAN,SECURITY",
            "device",
            "wifi",
            "list",
            "--rescan",
            if rescan { "yes" } else { "no" },
        ]),
        Duration::from_secs(if rescan { 15 } else { 5 }),
    )
    .await?;
    let mut networks: Vec<WifiNetwork> = out
        .lines()
        .filter_map(|l| {
            let f = split_nm(l);
            if f.len() != 7 {
                return None;
            }
            Some(WifiNetwork {
                connected: f[0] == "*",
                ssid: clean(&f[1]),
                bssid: f[2].clone(),
                signal: f[3].parse().unwrap_or_default(),
                frequency: f[4].clone(),
                channel: f[5].parse().unwrap_or_default(),
                security: f[6].clone(),
            })
        })
        .collect();
    networks.sort_by_key(|n| std::cmp::Reverse(n.signal));
    Ok(networks)
}
async fn wifi(rescan: bool) -> Result<ToolResult> {
    let networks = scan_wifi(rescan).await?;
    let mut r = result(
        "Nearby Wi-Fi",
        &[
            "Active",
            "SSID",
            "Signal",
            "Frequency",
            "Channel",
            "Security",
            "BSSID",
        ],
    );
    let mut channels: BTreeMap<u16, u32> = BTreeMap::new();
    for n in networks {
        *channels.entry(n.channel).or_default() += 1;
        r.rows.push(vec![
            if n.connected { "Connected" } else { "—" }.into(),
            n.ssid,
            format!("{}%", n.signal),
            n.frequency,
            n.channel.to_string(),
            n.security,
            n.bssid,
        ]);
    }
    r.notes.push(format!(
        "Nearby APs by channel: {}",
        channels
            .iter()
            .map(|(c, n)| format!("{c}: {n}"))
            .collect::<Vec<_>>()
            .join(" · ")
    ));
    let candidates = [1u16, 6, 11];
    let best = candidates
        .into_iter()
        .min_by_key(|c| {
            channels
                .iter()
                .filter(|(other, _)| **other <= 14 && (**other as i32 - *c as i32).abs() < 5)
                .map(|(_, n)| *n)
                .sum::<u32>()
        })
        .unwrap_or(1);
    r.notes.push(format!("2.4 GHz scan-count heuristic: channel {best} has the fewest overlapping APs among 1/6/11. Check region and actual airtime before changing your router."));
    Ok(r)
}
async fn saved_wifi() -> Result<ToolResult> {
    let out = cmd(
        "nmcli",
        &[
            "-t",
            "-f",
            "NAME,UUID,TYPE,AUTOCONNECT",
            "connection",
            "show",
        ],
    )
    .await?;
    let mut r = result(
        "Saved Wi-Fi profiles",
        &["Name", "UUID", "Type", "Auto-connect"],
    );
    for l in out.lines() {
        let f = split_nm(l);
        if f.len() == 4 && (f[2] == "802-11-wireless" || f[2] == "wifi") {
            r.rows.push(f.into_iter().map(|s| clean(&s)).collect());
        }
    }
    r.notes.push(
        "Credentials are never read. Forget and auto-connect controls use the UUID from this list."
            .into(),
    );
    Ok(r)
}

fn lines(title: &str, out: &str) -> ToolResult {
    let mut r = result(title, &["Details"]);
    r.rows = out.lines().map(|l| vec![clean(l)]).collect();
    r
}
async fn firewall() -> Result<ToolResult> {
    let mut r = result("Firewall overview", &["Backend", "Rule / state"]);
    for (program, a) in [
        ("nft", vec!["-j", "list", "ruleset"]),
        ("iptables-save", vec![]),
        ("ufw", vec!["status", "verbose"]),
        ("firewall-cmd", vec!["--list-all"]),
    ] {
        if !crate::command::available(program) {
            continue;
        }
        match cmd(program, &a).await {
            Ok(out) => {
                if program == "nft" {
                    let v: Value = serde_json::from_str(&out)?;
                    for rule in v["nftables"].as_array().into_iter().flatten() {
                        r.rows.push(vec![program.into(), clean(&rule.to_string())]);
                    }
                } else {
                    r.rows
                        .extend(out.lines().map(|l| vec![program.into(), clean(l)]));
                }
            }
            Err(e) => r.notes.push(e.to_string()),
        }
    }
    r.notes.push("Read-only. Rules, namespaces and upstream firewalls affect reachability. Wildcard listeners alone are not proof of public exposure.".into());
    Ok(r)
}
async fn vpn(s: &Snapshot) -> Result<ToolResult> {
    let mut r = result("VPN awareness", &["Interface / backend", "Status"]);
    for i in s.vpn_interfaces() {
        r.rows.push(vec![
            i.name.clone(),
            format!(
                "{} · {} · default route: {}",
                i.state,
                i.addresses.join(", "),
                s.primary.as_ref() == Some(&i.name)
            ),
        ]);
    }
    // 'wg show all dump' includes private keys. Use only non-secret subcommands.
    if crate::command::available("wg") {
        for key in ["latest-handshakes", "transfer", "endpoints", "allowed-ips"] {
            match cmd("wg", &["show", "all", key]).await {
                Ok(out) => r.rows.extend(
                    out.lines()
                        .map(|l| vec![format!("WireGuard {key}"), clean(l)]),
                ),
                Err(e) => r.notes.push(e.to_string()),
            }
        }
    }
    if let Ok(out) = cmd("tailscale", &["status", "--json"]).await {
        if let Ok(v) = serde_json::from_str::<Value>(&out) {
            r.rows.push(vec![
                "Tailscale".into(),
                format!("{} · {}", v["BackendState"], v["TailscaleIPs"]),
            ]);
        }
    }
    r.notes.push("Interface-name VPN detection is a heuristic. Split tunnels and policy routing need route/rule inspection; no DNS-leak or anonymity claim is made.".into());
    Ok(r)
}
async fn containers() -> Result<ToolResult> {
    let backend = if crate::command::available("docker") {
        "docker"
    } else {
        "podman"
    };
    let out = cmd(backend, &["network", "ls", "--format", "{{.ID}}"]).await?;
    let mut r = result(
        "Container networking",
        &["Network", "Driver", "IPAM", "Attached containers"],
    );
    for id in out.lines().take(50) {
        if !id.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let out = cmd(backend, &["network", "inspect", id]).await?;
        let v: Vec<Value> = serde_json::from_str(&out)?;
        for n in v {
            r.rows.push(vec![
                n["Name"]
                    .as_str()
                    .or(n["name"].as_str())
                    .unwrap_or(id)
                    .into(),
                n["Driver"]
                    .as_str()
                    .or(n["driver"].as_str())
                    .unwrap_or("unknown")
                    .into(),
                if n["IPAM"].is_null() {
                    n["subnets"].to_string()
                } else {
                    n["IPAM"]["Config"].to_string()
                },
                n["Containers"]
                    .as_object()
                    .map(|o| {
                        o.iter()
                            .map(|(_, v)| format!("{} {}", v["Name"], v["IPv4Address"]))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default(),
            ]);
        }
    }
    if let Ok(out) = cmd(backend, &["ps", "--format", "{{.Names}}\t{{.Ports}}"]).await {
        for l in out.lines() {
            r.rows.push(vec![
                "Published ports".into(),
                backend.into(),
                clean(l),
                String::new(),
            ]);
        }
    }
    r.notes.push("Needs access to the container engine. The UI does not start or stop containers. Compare host listeners to published-port mappings for conflicts.".into());
    Ok(r)
}

pub fn parse_process_traffic(out: &str) -> Vec<Vec<String>> {
    let latest = out.rsplit("Refreshing:").next().unwrap_or(out);
    let mut rows = Vec::new();
    for line in latest.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() != 3 {
            continue;
        }
        let (Ok(tx), Ok(rx)) = (fields[1].parse::<f64>(), fields[2].parse::<f64>()) else {
            continue;
        };
        let parts: Vec<_> = fields[0].rsplitn(3, '/').collect();
        if parts.len() != 3 {
            continue;
        }
        rows.push(vec![
            clean(parts[2]),
            parts[1].into(),
            parts[0].into(),
            rate(rx * 1024.0),
            rate(tx * 1024.0),
        ]);
    }
    rows.sort_by(|a, b| a[0].cmp(&b[0]));
    rows
}
async fn process_traffic(interface: &str, s: &Snapshot) -> Result<ToolResult> {
    crate::control::validate_interface(interface, s)?;
    if !crate::command::available("nethogs") {
        bail!("Install nethogs for per-process traffic sampling. Interface totals remain available without packet-capture privileges.");
    }
    let mut arguments = args(&["-t", "-d", "1", "-c", "5", interface]);
    let program = if crate::control::is_root() {
        "nethogs"
    } else {
        if !crate::command::available("pkexec") {
            bail!("Per-process capture needs privileges and a pkexec/PolicyKit agent");
        }
        arguments.insert(0, "nethogs".into());
        "pkexec"
    };
    let out = run(program, &arguments, Duration::from_secs(20)).await?;
    let mut r = result(
        "Per-process bandwidth (NetHogs)",
        &["Process", "PID", "UID", "Download", "Upload"],
    );
    r.rows = parse_process_traffic(&out);
    r.notes.push("A five-interval sample, showing the final trace refresh. Process attribution depends on NetHogs, namespace visibility and access to procfs; unknown traffic can remain unattributed. No payload capture is saved.".into());
    Ok(r)
}
async fn developer(s: &Snapshot) -> Result<ToolResult> {
    let ports = [
        3000u16, 3001, 4200, 5173, 5000, 5001, 8000, 8001, 8080, 8081, 8888, 9000,
    ];
    let mut r = result(
        "Developer port dashboard",
        &["Port", "Process", "Bind", "HTTP HEAD", "Observation"],
    );
    for c in s
        .connections
        .iter()
        .filter(|c| c.listening() && c.protocol.starts_with("tcp") && ports.contains(&c.port()))
        .take(16)
    {
        let host = if c.local.starts_with('[') {
            "[::1]"
        } else {
            "127.0.0.1"
        };
        let u = format!("http://{host}:{}/", c.port());
        let status = run(
            "curl",
            &args(&[
                "--silent",
                "--show-error",
                "--head",
                "--connect-timeout",
                "1",
                "--max-time",
                "2",
                "--output",
                "/dev/null",
                "--write-out",
                "%{http_code}",
                "--url",
                &u,
            ]),
            Duration::from_secs(3),
        )
        .await;
        let peers = s
            .connections
            .iter()
            .filter(|other| {
                other.listening()
                    && other.pid == c.pid
                    && other.pid.is_some()
                    && other.port() != c.port()
                    && ports.contains(&other.port())
            })
            .count();
        r.rows.push(vec![
            c.port().to_string(),
            c.process.clone(),
            c.local.clone(),
            status.unwrap_or_else(|_| "No HTTP response".into()),
            if peers > 0 {
                format!("{peers} other dev port(s) for same PID")
            } else if c.wildcard() {
                "Bound to all addresses; review firewall".into()
            } else {
                "Loopback candidate".into()
            },
        ]);
    }
    r.notes.push("Tests only loopback endpoints on common development ports. A failed plain HTTP test may indicate HTTPS, another protocol or an address-specific bind. Shared PID ports are not automatically treated as duplicates.".into());
    Ok(r)
}

fn local_subnet(cidr: &str, s: &Snapshot) -> Result<(u32, u32)> {
    let (ip, prefix) = cidr
        .split_once('/')
        .context("Enter an IPv4 CIDR, e.g. 192.168.1.0/24")?;
    let ip = ip.parse::<Ipv4Addr>()?;
    let prefix = prefix.parse::<u32>()?;
    if !(24..=30).contains(&prefix) || !ip.is_private() {
        bail!("Discovery accepts directly connected private IPv4 networks /24 through /30");
    }
    let mask = u32::MAX << (32 - prefix);
    let network = u32::from(ip) & mask;
    if !s.interfaces.iter().flat_map(|i| &i.addresses).any(|a| {
        a.split_once('/')
            .and_then(|(ip, _)| ip.parse::<Ipv4Addr>().ok())
            .is_some_and(|ip| u32::from(ip) & mask == network)
    }) {
        bail!("CIDR is not on a local interface");
    }
    let direct = s.routes.iter().any(|r| {
        r.destination == format!("{}/{prefix}", Ipv4Addr::from(network))
            && (r.gateway == "—" || r.gateway.is_empty())
    });
    if !direct {
        bail!("CIDR must match a directly connected route");
    }
    Ok((network + 1, network + (1 << (32 - prefix)) - 2))
}
async fn lan(cidr: &str, s: &Snapshot) -> Result<ToolResult> {
    let (start, end) = local_subnet(cidr, s)?;
    use futures_util::{stream, StreamExt};
    let replies: Vec<_> = stream::iter(start..=end)
        .map(|ip| async move {
            let host = Ipv4Addr::from(ip).to_string();
            let latency = ping_once(&host).await;
            (host, latency)
        })
        .buffer_unordered(16)
        .collect()
        .await;
    let neighbors = linux::read_neighbors().await;
    let mut r = result(
        "LAN discovery",
        &["IP", "MAC", "State", "Latency", "Source"],
    );
    for (ip, latency) in replies {
        let neighbor = neighbors.iter().find(|n| n.ip == ip);
        if latency.is_some() || neighbor.is_some() {
            r.rows.push(vec![
                ip,
                neighbor
                    .map(|n| n.mac.clone())
                    .unwrap_or_else(|| "Unavailable".into()),
                neighbor
                    .map(|n| n.state.clone())
                    .unwrap_or_else(|| "ICMP response".into()),
                ms(latency),
                "ICMP + local neighbor table".into(),
            ]);
        }
    }
    r.notes.push("Silent devices can be present. No port scan, hostname query or vendor API is performed. First/last seen are recorded in the session timeline.".into());
    Ok(r)
}

async fn diagnostics(s: &Snapshot, c: &crate::config::Config) -> Result<ToolResult> {
    let mut r = result(
        "Network diagnostics",
        &["Status", "Finding", "Evidence", "Next step"],
    );
    r.findings = crate::diagnosis::local_findings(s);
    if r.findings.is_empty() {
        r.findings.push(Finding::new(
            Severity::Pass,
            "Local configuration looks ready",
            "Default route, interface address and DNS configuration are visible.",
            "Continue with connectivity and endpoint checks.",
        ));
    }
    let gateway = async {
        if let Some(gw) = s.gateway().filter(|g| !g.is_empty() && *g != "—") {
            if crate::command::available("ping") {
                return Some((gw.to_owned(), ping_once(gw).await));
            }
        }
        None
    };
    let direct = |ip: &'static str| async move {
        tokio::time::timeout(
            Duration::from_secs(4),
            tokio::net::TcpStream::connect((ip, 443)),
        )
        .await
        .is_ok_and(|v| v.is_ok())
    };
    let (gateway, dns, one, two, endpoint) = tokio::join!(
        gateway,
        dns_lookup(&c.dns_test_name, "A", ""),
        direct("1.1.1.1"),
        direct("8.8.8.8"),
        http(&c.internet_url)
    );
    if let Some((gw, reply)) = gateway {
        r.findings.push(Finding::new(
            if reply.is_some() {
                Severity::Pass
            } else {
                Severity::Warning
            },
            if reply.is_some() {
                "Gateway responded to ICMP"
            } else {
                "Gateway did not answer ICMP"
            },
            format!("{gw}: {}", ms(reply)),
            "Inspect the gateway and local link; ICMP filtering can also explain no response.",
        ));
    }
    let dns_ok = dns.is_ok();
    match dns {
        Ok(d) => {
            let time = d
                .metrics
                .get("Query time (ms)")
                .and_then(|v| v.parse::<f64>().ok());
            let slow = time.is_some_and(|v| v > 250.0);
            r.findings.push(Finding::new(
                if slow {
                    Severity::Warning
                } else {
                    Severity::Pass
                },
                if slow {
                    "DNS response is slow"
                } else {
                    "System DNS answered"
                },
                format!("{}: {}", c.dns_test_name, ms(time)),
                if slow {
                    "Compare resolvers and inspect VPN, DoT and upstream DNS latency."
                } else {
                    "The configured resolver answered this A lookup."
                },
            ));
        }
        Err(e) => {
            let level = if crate::command::available("dig") {
                Severity::Error
            } else {
                Severity::Unknown
            };
            r.findings.push(Finding::new(
                level,
                "System DNS query failed",
                e.to_string(),
                "Run DNS lookup and inspect DNS settings; test DoT when your resolver uses it.",
            ));
        }
    }
    let http_reachable = endpoint
        .as_ref()
        .ok()
        .and_then(|h| h.metrics.get("http_code"))
        .and_then(|v| v.parse::<u16>().ok())
        .is_some_and(|v| (100..600).contains(&v));
    let reachable = one || two || http_reachable;
    r.metrics.insert(
        "Internet reachability".into(),
        if reachable {
            "Reachable"
        } else {
            "No test response"
        }
        .into(),
    );
    r.findings.push(Finding::new(if reachable { Severity::Pass } else { Severity::Error }, if reachable { "Internet endpoint reached" } else { "Internet connectivity may be down" }, format!("TCP 1.1.1.1:443: {} · 8.8.8.8:443: {} · configured HTTP endpoint: {}", if one {"connected"} else {"no connection"}, if two {"connected"} else {"no connection"}, if http_reachable {"responded"} else {"no response"}), if reachable { "At least one tested endpoint responded; this does not guarantee every service works." } else { "Check local link, gateway, VPN, proxy and firewall. Filtering or endpoint failures can resemble an outage." }));
    if reachable && !dns_ok && crate::command::available("dig") {
        r.findings.push(Finding::new(
            Severity::Error,
            "Internet reachable, DNS unavailable",
            "A network endpoint responded while the system DNS test failed.",
            "Inspect DNS settings and resolver reachability before changing the interface.",
        ));
    }
    match endpoint {
        Ok(h) => {
            if let Some(code) = h.metrics.get("http_code").and_then(|v| v.parse().ok()) {
                let mut finding = crate::diagnosis::http_status(code);
                finding.evidence = format!("{} · {}", c.internet_url, finding.evidence);
                r.findings.push(finding);
            }
            for (key, value) in h.metrics {
                r.metrics.insert(format!("HTTP {key}"), value);
            }
        }
        Err(e) => r
            .findings
            .push(crate::diagnosis::transport_failure(&e.to_string())),
    }
    if let Some(server) = &c.dot_server {
        let identity = c.dot_tls_name.as_deref().unwrap_or(server);
        match crate::dot::inspect(server, identity, &c.dns_test_name, 853).await {
            Ok(dot) => r.findings.extend(dot.findings),
            Err(e) => r.findings.push(Finding::new(
                Severity::Unknown,
                "DoT test configuration invalid",
                e.to_string(),
                "Set dot_server and dot_tls_name in config.toml.",
            )),
        }
    } else {
        r.findings.push(Finding::new(
            Severity::Info,
            "DNS-over-TLS not tested",
            "No diagnostic DoT endpoint is configured.",
            "Ctrl+K → Test DNS over TLS, or set dot_server / dot_tls_name in config.toml.",
        ));
    }
    r.findings.sort_by_key(|f| f.severity.rank());
    r.rows = r.findings.iter().map(Finding::row).collect();
    r.notes.push("Findings describe this run, not continuous reachability. Direct TCP probes target 1.1.1.1 and 8.8.8.8 on port 443; HTTPS uses the configured internet_url. No repairs are applied automatically.".into());
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reject_options_and_urls_as_hosts() {
        for h in ["-I", "a b", "example.com;id", "$(id)"] {
            assert!(validate_host(h).is_err());
        }
        assert!(validate_host("::1").is_ok());
    }
    #[test]
    fn parse_ping_timing() {
        assert_eq!(parse_ping("64 bytes time=12.34 ms"), Some(12.34));
        assert_eq!(parse_ping("timeout"), None);
    }
    #[test]
    fn reject_nonlocal_discovery() {
        assert!(local_subnet("8.8.8.0/24", &Snapshot::default()).is_err());
        assert!(local_subnet("192.168.0.0/16", &Snapshot::default()).is_err());
    }
}
