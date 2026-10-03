use super::NetworkBackend;
use crate::{
    command::{available, clean, cmd},
    model::*,
};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use std::{
    collections::HashMap,
    net::{Ipv4Addr, Ipv6Addr},
    path::Path,
};

pub struct LinuxBackend;

#[async_trait]
impl NetworkBackend for LinuxBackend {
    fn platform(&self) -> &'static str {
        "Linux / procfs + sysfs + iproute2"
    }
    async fn snapshot(&self) -> Result<Snapshot> {
        let mut s = Snapshot {
            timestamp: Utc::now(),
            ..Default::default()
        };
        let (native, addresses, routes, neighbors) = tokio::join!(
            tokio::task::spawn_blocking(read_interfaces),
            cmd("ip", &["-j", "address", "show"]),
            read_routes(),
            read_neighbors()
        );
        s.interfaces = native??;
        if let Ok(out) = addresses {
            if let Ok(v) = serde_json::from_str::<Vec<Value>>(&out) {
                for item in v {
                    if let Some(i) = s
                        .interfaces
                        .iter_mut()
                        .find(|i| Some(i.name.as_str()) == item["ifname"].as_str())
                    {
                        if let Some(a) = item["addr_info"].as_array() {
                            i.addresses = a
                                .iter()
                                .filter_map(|a| {
                                    Some(format!(
                                        "{}/{}",
                                        a["local"].as_str()?,
                                        a["prefixlen"].as_u64()?
                                    ))
                                })
                                .collect();
                        }
                    }
                }
            }
        } else {
            if let Err(e) = &addresses {
                s.warnings.push(format!(
                    "Address backend unavailable: {e}; using native fallbacks"
                ));
            }
            native_addresses(&mut s.interfaces);
        }
        s.routes = routes;
        s.neighbors = neighbors;
        s.primary = s
            .routes
            .iter()
            .filter(|r| r.destination == "default" && r.family == "IPv4")
            .min_by_key(|r| r.metric)
            .or_else(|| s.routes.iter().find(|r| r.destination == "default"))
            .map(|r| r.interface.clone());
        let conn = tokio::task::spawn_blocking(read_connections).await?;
        s.connections = conn;
        let resolv = std::fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
        s.dns = resolv
            .lines()
            .filter_map(|l| {
                l.strip_prefix("nameserver")
                    .and_then(|l| l.split_whitespace().next())
                    .map(str::to_string)
            })
            .collect();
        s.dns_source = if std::fs::read_link("/etc/resolv.conf")
            .ok()
            .is_some_and(|p| p.to_string_lossy().contains("systemd"))
        {
            "systemd-resolved".into()
        } else if resolv.contains("NetworkManager") {
            "NetworkManager".into()
        } else {
            "resolv.conf / source unknown".into()
        };
        if available("nmcli") {
            if let Ok(out) = cmd(
                "nmcli",
                &[
                    "-t",
                    "-f",
                    "GENERAL.DEVICE,GENERAL.CON-UUID,IP4.DNS,IP6.DNS",
                    "device",
                    "show",
                ],
            )
            .await
            {
                let mut device = String::new();
                for line in out.lines() {
                    let fields = crate::command::split_nm(line);
                    if fields.len() < 2 {
                        continue;
                    }
                    if fields[0] == "GENERAL.DEVICE" {
                        device = fields[1].clone();
                    }
                    if let Some(i) = s.interfaces.iter_mut().find(|i| i.name == device) {
                        if fields[0] == "GENERAL.CON-UUID" && fields[1] != "--" {
                            i.connection = Some(fields[1].clone());
                        }
                        if fields[0].starts_with("IP4.DNS") || fields[0].starts_with("IP6.DNS") {
                            i.dns.push(fields[1].clone());
                        }
                    }
                }
                let dns = s
                    .primary_interface()
                    .map(|i| i.dns.clone())
                    .unwrap_or_default();
                if !dns.is_empty() {
                    s.dns = dns;
                    s.dns_source = "NetworkManager (active link)".into();
                }
            }
        }
        if s.interfaces.iter().any(|i| i.kind == "Wi-Fi") {
            if let Ok(wifi) = crate::tools::scan_wifi(false).await {
                s.wifi = wifi;
            }
        }
        if available("resolvectl") && s.dns_source.starts_with("systemd") {
            if let Ok(out) = cmd("resolvectl", &["dns"]).await {
                for line in out.lines() {
                    if let Some((label, list)) = line.split_once(':') {
                        let device = label.split('(').nth(1).and_then(|n| n.strip_suffix(')'));
                        if let Some(i) = s
                            .interfaces
                            .iter_mut()
                            .find(|i| Some(i.name.as_str()) == device)
                        {
                            i.dns = list.split_whitespace().map(str::to_string).collect();
                        }
                    }
                }
                let dns = s
                    .primary_interface()
                    .map(|i| i.dns.clone())
                    .unwrap_or_default();
                if !dns.is_empty() {
                    s.dns = dns;
                }
            }
        }
        s.host_uptime = std::fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|v| v.split_whitespace().next()?.parse::<f64>().ok())
            .unwrap_or_default() as u64;
        for key in [
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "no_proxy",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
        ] {
            if let Ok(value) = std::env::var(key) {
                s.proxies.insert(key.into(), redact_proxy(&value));
            }
        }
        s.capabilities = capabilities();
        if s.primary.is_none() {
            s.warnings.push("No default route detected".into());
        }
        if s.dns.is_empty() {
            s.warnings.push("No DNS resolver detected".into());
        }
        if s.routes
            .iter()
            .filter(|r| r.destination == "default" && r.family == "IPv4")
            .count()
            > 1
        {
            s.warnings
                .push("Multiple IPv4 default routes; compare metrics and policy routing".into());
        }
        for i in &s.interfaces {
            if i.rx_errors + i.tx_errors > 0 {
                s.warnings.push(format!(
                    "{}: {} cumulative packet errors",
                    i.name,
                    i.rx_errors + i.tx_errors
                ));
            }
        }
        if !available("ip") {
            s.warnings
                .push("iproute2 unavailable; using procfs/getifaddrs fallback".into());
        }
        Ok(s)
    }
}

fn read_interfaces() -> Result<Vec<Interface>> {
    let mut result = Vec::new();
    for entry in std::fs::read_dir("/sys/class/net")? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let p = entry.path();
        let read = |key: &str| {
            std::fs::read_to_string(p.join(key))
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        let n = |key: &str| read(key).parse::<u64>().unwrap_or(0);
        let kind = if name == "lo" {
            "Loopback"
        } else if p.join("wireless").exists() {
            "Wi-Fi"
        } else if name.starts_with("wg") || name.starts_with("tailscale") {
            "VPN"
        } else if name.starts_with("tun")
            || name.starts_with("tap")
            || name.starts_with("sit")
            || name.starts_with("gre")
        {
            "Tunnel"
        } else if name.starts_with("docker")
            || name.starts_with("veth")
            || name.starts_with("br-")
            || name.starts_with("cni")
        {
            "Container"
        } else if p.join("bridge").exists() {
            "Bridge"
        } else if !p.join("device").exists() {
            "Virtual"
        } else {
            "Ethernet"
        };
        let speed = read("speed")
            .parse::<i64>()
            .ok()
            .filter(|n| *n > 0)
            .map(|n| n as u64);
        result.push(Interface {
            name,
            kind: kind.into(),
            state: read("operstate"),
            mac: read("address"),
            mtu: n("mtu") as u32,
            rx_bytes: n("statistics/rx_bytes"),
            tx_bytes: n("statistics/tx_bytes"),
            rx_packets: n("statistics/rx_packets"),
            tx_packets: n("statistics/tx_packets"),
            rx_errors: n("statistics/rx_errors"),
            tx_errors: n("statistics/tx_errors"),
            dropped: n("statistics/rx_dropped") + n("statistics/tx_dropped"),
            speed_mbps: speed,
            duplex: Some(read("duplex")).filter(|s| !s.is_empty()),
            ..Default::default()
        });
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}

fn native_addresses(interfaces: &mut [Interface]) {
    #[cfg(target_os = "linux")]
    unsafe {
        let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut head) != 0 {
            ioctl_addresses(interfaces);
            return;
        }
        let mut cur = head;
        while !cur.is_null() {
            let item = &*cur;
            if !item.ifa_addr.is_null() && !item.ifa_name.is_null() {
                let name = std::ffi::CStr::from_ptr(item.ifa_name).to_string_lossy();
                if let Some(i) = interfaces.iter_mut().find(|i| i.name == name) {
                    match (*item.ifa_addr).sa_family as i32 {
                        libc::AF_INET => {
                            let addr = &*(item.ifa_addr as *const libc::sockaddr_in);
                            let ip = Ipv4Addr::from(addr.sin_addr.s_addr.to_ne_bytes());
                            let prefix = if item.ifa_netmask.is_null() {
                                0
                            } else {
                                (*(item.ifa_netmask as *const libc::sockaddr_in))
                                    .sin_addr
                                    .s_addr
                                    .count_ones()
                            };
                            i.addresses.push(format!("{ip}/{prefix}"));
                        }
                        libc::AF_INET6 => {
                            let addr = &*(item.ifa_addr as *const libc::sockaddr_in6);
                            let ip = Ipv6Addr::from(addr.sin6_addr.s6_addr);
                            let prefix = if item.ifa_netmask.is_null() {
                                0
                            } else {
                                (*(item.ifa_netmask as *const libc::sockaddr_in6))
                                    .sin6_addr
                                    .s6_addr
                                    .iter()
                                    .map(|n| n.count_ones())
                                    .sum()
                            };
                            i.addresses.push(format!("{ip}/{prefix}"));
                        }
                        _ => {}
                    }
                }
            }
            cur = item.ifa_next;
        }
        libc::freeifaddrs(head);
    }
}

#[cfg(target_os = "linux")]
fn ioctl_addresses(interfaces: &mut [Interface]) {
    // getifaddrs uses netlink on glibc. ioctl/procfs still work in sandboxes
    // that deny AF_NETLINK but permit ordinary IPv4 sockets.
    unsafe {
        let fd = libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0);
        if fd >= 0 {
            for i in interfaces.iter_mut() {
                let mut request: libc::ifreq = std::mem::zeroed();
                for (dst, src) in request
                    .ifr_name
                    .iter_mut()
                    .zip(i.name.as_bytes().iter().take(libc::IFNAMSIZ - 1))
                {
                    *dst = *src as libc::c_char;
                }
                if libc::ioctl(fd, libc::SIOCGIFADDR, &mut request) == 0 {
                    let address = &request.ifr_ifru.ifru_addr as *const libc::sockaddr
                        as *const libc::sockaddr_in;
                    let ip = Ipv4Addr::from((*address).sin_addr.s_addr.to_ne_bytes());
                    let prefix = if libc::ioctl(fd, libc::SIOCGIFNETMASK, &mut request) == 0 {
                        let mask = &request.ifr_ifru.ifru_netmask as *const libc::sockaddr
                            as *const libc::sockaddr_in;
                        (*mask).sin_addr.s_addr.count_ones()
                    } else {
                        32
                    };
                    i.addresses.push(format!("{ip}/{prefix}"));
                }
            }
            libc::close(fd);
        }
    }
    if let Ok(data) = std::fs::read_to_string("/proc/net/if_inet6") {
        for line in data.lines() {
            let f: Vec<_> = line.split_whitespace().collect();
            if f.len() != 6 || f[0].len() != 32 {
                continue;
            }
            let Some(i) = interfaces.iter_mut().find(|i| i.name == f[5]) else {
                continue;
            };
            let mut octets = [0u8; 16];
            let mut valid = true;
            for (k, o) in octets.iter_mut().enumerate() {
                match u8::from_str_radix(&f[0][k * 2..k * 2 + 2], 16) {
                    Ok(v) => *o = v,
                    Err(_) => {
                        valid = false;
                        break;
                    }
                }
            }
            if valid {
                if let Ok(prefix) = u8::from_str_radix(f[2], 16) {
                    i.addresses
                        .push(format!("{}/{prefix}", Ipv6Addr::from(octets)));
                }
            }
        }
    }
}

pub async fn read_routes() -> Vec<Route> {
    let mut result = Vec::new();
    for (arg, family) in [("-4", "IPv4"), ("-6", "IPv6")] {
        if let Ok(out) = cmd("ip", &["-j", arg, "route", "show", "table", "all"]).await {
            if let Ok(items) = serde_json::from_str::<Vec<Value>>(&out) {
                for v in items {
                    result.push(Route {
                        family: family.into(),
                        destination: v["dst"].as_str().unwrap_or("default").into(),
                        gateway: v["gateway"].as_str().unwrap_or("—").into(),
                        interface: v["dev"].as_str().unwrap_or("—").into(),
                        metric: v["metric"].as_u64().unwrap_or(0),
                        protocol: v["protocol"].as_str().unwrap_or("—").into(),
                        table: v["table"].as_str().map(str::to_string).unwrap_or_else(|| {
                            v["table"]
                                .as_u64()
                                .map(|n| n.to_string())
                                .unwrap_or_else(|| "main".into())
                        }),
                    });
                }
            }
        }
    }
    if result.is_empty() {
        if let Ok(data) = std::fs::read_to_string("/proc/net/route") {
            for line in data.lines().skip(1) {
                let f: Vec<_> = line.split_whitespace().collect();
                if f.len() < 8 {
                    continue;
                }
                let ip = |s: &str| {
                    u32::from_str_radix(s, 16)
                        .ok()
                        .map(|n| Ipv4Addr::from(n.to_le_bytes()))
                        .unwrap_or(Ipv4Addr::UNSPECIFIED)
                };
                let dest = ip(f[1]);
                let gateway = ip(f[2]);
                result.push(Route {
                    family: "IPv4".into(),
                    destination: if dest.is_unspecified() {
                        "default".into()
                    } else {
                        format!(
                            "{dest}/{}",
                            u32::from_str_radix(f[7], 16).unwrap_or(0).count_ones()
                        )
                    },
                    gateway: if gateway.is_unspecified() {
                        "—".into()
                    } else {
                        gateway.to_string()
                    },
                    interface: f[0].into(),
                    metric: f[6].parse().unwrap_or(0),
                    protocol: "procfs".into(),
                    table: "main".into(),
                });
            }
        }
    }
    result.sort_by(|a, b| {
        (&a.family, &a.destination, a.metric).cmp(&(&b.family, &b.destination, b.metric))
    });
    result
}

pub async fn read_neighbors() -> Vec<Neighbor> {
    if let Ok(out) = cmd("ip", &["-j", "neighbor", "show"]).await {
        if let Ok(items) = serde_json::from_str::<Vec<Value>>(&out) {
            return items
                .into_iter()
                .map(|v| Neighbor {
                    ip: v["dst"].as_str().unwrap_or_default().into(),
                    mac: v["lladdr"].as_str().unwrap_or("—").into(),
                    interface: v["dev"].as_str().unwrap_or_default().into(),
                    state: v["state"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join(",")
                        })
                        .unwrap_or_else(|| v["state"].as_str().unwrap_or("unknown").into()),
                })
                .collect();
        }
    }
    std::fs::read_to_string("/proc/net/arp")
        .unwrap_or_default()
        .lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<_> = l.split_whitespace().collect();
            if f.len() < 6 {
                return None;
            }
            Some(Neighbor {
                ip: f[0].into(),
                mac: f[3].into(),
                interface: f[5].into(),
                state: if f[2] == "0x2" {
                    "REACHABLE"
                } else {
                    "INCOMPLETE"
                }
                .into(),
            })
        })
        .collect()
}

pub fn parse_proc_address(value: &str, ipv6: bool) -> Option<String> {
    let (hex, port) = value.split_once(':')?;
    let port = u16::from_str_radix(port, 16).ok()?;
    if ipv6 {
        if hex.len() != 32 {
            return None;
        }
        let mut bytes = [0u8; 16];
        for k in 0..4 {
            bytes[k * 4..k * 4 + 4].copy_from_slice(
                &u32::from_str_radix(&hex[k * 8..k * 8 + 8], 16)
                    .ok()?
                    .to_ne_bytes(),
            );
        }
        Some(format!("[{}]:{port}", Ipv6Addr::from(bytes)))
    } else {
        Some(format!(
            "{}:{port}",
            Ipv4Addr::from(u32::from_str_radix(hex, 16).ok()?.to_ne_bytes())
        ))
    }
}

pub fn parse_proc_sockets(data: &str, protocol: &str) -> Vec<Connection> {
    data.lines()
        .skip(1)
        .filter_map(|line| {
            let f: Vec<_> = line.split_whitespace().collect();
            if f.len() < 10 {
                return None;
            }
            let state = match f[3] {
                "01" => "ESTABLISHED",
                "02" => "SYN_SENT",
                "03" => "SYN_RECV",
                "04" => "FIN_WAIT1",
                "05" => "FIN_WAIT2",
                "06" => "TIME_WAIT",
                "07" => "UNCONN",
                "08" => "CLOSE_WAIT",
                "09" => "LAST_ACK",
                "0A" => "LISTEN",
                "0B" => "CLOSING",
                _ => "UNKNOWN",
            };
            Some(Connection {
                protocol: protocol.into(),
                state: state.into(),
                local: parse_proc_address(f[1], protocol.ends_with('6'))?,
                remote: parse_proc_address(f[2], protocol.ends_with('6'))?,
                uid: f[7].parse().ok()?,
                inode: f[9].parse().ok()?,
                ..Default::default()
            })
        })
        .collect()
}

fn read_connections() -> Vec<Connection> {
    let mut result = Vec::new();
    for protocol in ["tcp", "tcp6", "udp", "udp6"] {
        if let Ok(data) = std::fs::read_to_string(format!("/proc/net/{protocol}")) {
            result.extend(parse_proc_sockets(&data, protocol));
        }
    }
    let mut owners = HashMap::new();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let p = entry.path();
            if let Ok(fds) = std::fs::read_dir(p.join("fd")) {
                for fd in fds.flatten() {
                    if let Ok(link) = std::fs::read_link(fd.path()) {
                        let link = link.to_string_lossy();
                        if let Some(inode) = link
                            .strip_prefix("socket:[")
                            .and_then(|s| s.strip_suffix(']'))
                            .and_then(|s| s.parse::<u64>().ok())
                        {
                            owners.insert(inode, pid);
                        }
                    }
                }
            }
        }
    }
    for c in &mut result {
        if let Some(pid) = owners.get(&c.inode) {
            c.pid = Some(*pid);
            c.process = clean(
                std::fs::read_to_string(format!("/proc/{pid}/comm"))
                    .unwrap_or_default()
                    .trim(),
            );
            c.executable = clean(
                &std::fs::read_link(format!("/proc/{pid}/exe"))
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            );
        } else {
            c.process = "Unavailable / permissions".into();
        }
    }
    result.sort_by(|a, b| (&a.local, &a.protocol).cmp(&(&b.local, &b.protocol)));
    result
}

pub fn redact_proxy(value: &str) -> String {
    if let Ok(mut u) = url::Url::parse(value) {
        if !u.username().is_empty() || u.password().is_some() {
            let _ = u.set_username("REDACTED");
            let _ = u.set_password(Some("REDACTED"));
        }
        u.set_query(None);
        u.set_fragment(None);
        u.to_string()
    } else if value.contains('@') {
        "[redacted credential-bearing proxy]".into()
    } else {
        clean(value)
    }
}

pub fn capabilities() -> Vec<Capability> {
    [
        ("ip", "Routes, neighbors and interface controls"),
        ("nmcli", "Wi-Fi and persistent network configuration"),
        ("resolvectl", "Resolved DNS settings and cache"),
        ("dig", "DNS lookup and resolver comparison"),
        ("ping", "ICMP latency and packet loss"),
        ("traceroute", "Path analysis"),
        ("mtr", "Repeated path monitoring"),
        ("curl", "HTTP, public IP and TLS timing"),
        ("openssl", "Certificate inspection"),
        ("iperf3", "Controlled LAN bandwidth test"),
        ("speedtest", "Ookla speed testing"),
        ("speedtest-cli", "Python speedtest backend"),
        ("iw", "Wireless survey and RF details"),
        ("ethtool", "Ethernet capabilities"),
        ("nethogs", "Per-process bandwidth with capture privileges"),
        ("nft", "nftables rules"),
        ("iptables-save", "iptables rules"),
        ("ufw", "UFW status"),
        ("firewall-cmd", "firewalld status"),
        ("wg", "WireGuard peers"),
        ("tailscale", "Tailscale state"),
        ("docker", "Docker networks"),
        ("podman", "Podman networks"),
        ("pkexec", "On-demand privileged controls"),
        ("wl-copy", "Wayland clipboard"),
        ("xclip", "X11 clipboard"),
    ]
    .into_iter()
    .map(|(c, p)| Capability {
        command: c.into(),
        available: available(c),
        purpose: p.into(),
    })
    .collect()
}

pub fn namespaces() -> Vec<String> {
    std::fs::read_dir(Path::new("/var/run/netns"))
        .map(|e| {
            e.flatten()
                .map(|e| clean(&e.file_name().to_string_lossy()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn local_vendor(mac: &str) -> String {
    let prefix = mac
        .replace(':', "-")
        .chars()
        .take(8)
        .collect::<String>()
        .to_uppercase();
    if prefix.len() != 8 {
        return "Unavailable".into();
    }
    for path in ["/usr/share/ieee-data/oui.txt", "/usr/share/misc/oui.txt"] {
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Some(line) = data
                .lines()
                .find(|l| l.starts_with(&prefix) && l.contains("(hex)"))
            {
                return line
                    .split("(hex)")
                    .nth(1)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
            }
        }
    }
    "No local OUI database".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proc_ipv4_ipv6_endianness() {
        assert_eq!(
            parse_proc_address("0100007F:1F90", false).unwrap(),
            "127.0.0.1:8080"
        );
        assert_eq!(
            parse_proc_address("00000000000000000000000001000000:0035", true).unwrap(),
            "[::1]:53"
        );
    }
    #[test]
    fn socket_parser_handles_uid_and_inode() {
        let data="header\n 0: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000 1000 0 12345";
        let c = parse_proc_sockets(data, "tcp");
        assert_eq!(c[0].uid, 1000);
        assert_eq!(c[0].inode, 12345);
        assert!(c[0].listening());
    }
    #[test]
    fn proxy_credentials_never_displayed() {
        assert!(!redact_proxy("http://bob:secret@proxy.test:8080/x?token=abc").contains("secret"));
    }
}
