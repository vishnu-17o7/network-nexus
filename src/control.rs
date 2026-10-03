use crate::{
    command::{available, cmd, run, run_with_input},
    config::SavedProfile,
    model::*,
};
use anyhow::{bail, Context, Result};
use std::{net::IpAddr, time::Duration};

#[derive(Clone)]
pub struct Step {
    pub program: String,
    pub args: Vec<String>,
    pub privileged: bool,
    pub input: Option<Vec<u8>>,
    pub api: Option<crate::integrations::PiholeMutation>,
}
impl Step {
    pub fn pihole(change: crate::integrations::PiholeMutation) -> Self {
        Self {
            program: "Pi-hole API".into(),
            args: vec![],
            privileged: false,
            input: None,
            api: Some(change),
        }
    }
    fn new(program: &str, args: Vec<String>, privileged: bool) -> Self {
        Self {
            program: program.into(),
            args,
            privileged,
            input: None,
            api: None,
        }
    }
}
#[derive(Clone)]
pub struct Plan {
    pub title: String,
    pub summary: Vec<String>,
    pub steps: Vec<Step>,
    pub undo: Vec<Step>,
    pub disruptive: bool,
}
impl Plan {
    pub fn revert(&self) -> Option<Self> {
        if self.undo.is_empty() {
            return None;
        }
        let mut steps = self.undo.clone();
        for step in &mut steps {
            if let Some(api) = &mut step.api {
                api.prepared = chrono::Utc::now();
            }
        }
        Some(Self{title:format!("Revert {}",self.title),summary:vec!["Restore the values captured before this change. A later external change may be overwritten; Pi-hole rechecks its current blocking state.".into()],steps,undo:vec![],disruptive:self.disruptive})
    }
}

#[derive(Clone)]
pub enum Change {
    Pihole {
        url: String,
        password: crate::integrations::Secret,
        enabled: bool,
    },
    TailscaleDns {
        enabled: bool,
    },
    TailscaleExit {
        target: String,
    },
    Link {
        interface: String,
        up: bool,
    },
    Mtu {
        interface: String,
        value: u32,
    },
    Dns {
        interface: String,
        servers: Vec<String>,
        automatic: bool,
    },
    Dhcp {
        interface: String,
        release: bool,
    },
    Static {
        interface: String,
        address: String,
        gateway: String,
    },
    Route {
        destination: String,
        gateway: String,
        interface: String,
        metric: u32,
        remove: bool,
    },
    Radio {
        enabled: bool,
    },
    WifiConnect {
        interface: String,
        ssid: String,
        password: String,
        hidden: bool,
    },
    WifiDisconnect {
        interface: String,
    },
    WifiForget {
        uuid: String,
    },
    Autoconnect {
        uuid: String,
        enabled: bool,
    },
    FlushDns,
    Profile(SavedProfile),
    Restart,
}

fn a(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}
pub fn validate_interface<'a>(name: &str, s: &'a Snapshot) -> Result<&'a Interface> {
    if name.starts_with('-') || name.chars().any(|c| c.is_control()) {
        bail!("Invalid interface name");
    }
    s.interfaces
        .iter()
        .find(|i| i.name == name)
        .context("Interface no longer exists; refresh and select an existing interface")
}
fn uuid(value: &str) -> Result<()> {
    if value.len() != 36 || !value.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        bail!("Use a full NetworkManager connection UUID");
    }
    Ok(())
}
fn cidr(value: &str) -> Result<IpAddr> {
    let (ip, prefix) = value
        .split_once('/')
        .context("Address must use CIDR notation")?;
    let ip: IpAddr = ip.parse()?;
    let n: u8 = prefix.parse()?;
    if n > if ip.is_ipv4() { 32 } else { 128 } {
        bail!("Invalid prefix length");
    }
    Ok(ip)
}
fn valid_servers(servers: &[String]) -> Result<()> {
    if servers.is_empty() {
        bail!("Enter at least one DNS IP address");
    }
    for server in servers {
        server
            .parse::<IpAddr>()
            .context("DNS values must be IP addresses")?;
    }
    Ok(())
}
fn nm_uuid(i: &Interface) -> Result<String> {
    if !available("nmcli") {
        bail!("Persistent settings require NetworkManager/nmcli. This app does not overwrite unmanaged network files.");
    }
    let id = i
        .connection
        .clone()
        .context("Interface has no active NetworkManager connection")?;
    uuid(&id)?;
    Ok(id)
}
async fn nm_value(id: &str, key: &str) -> Result<String> {
    Ok(cmd(
        "nmcli",
        &[
            "--escape",
            "no",
            "-g",
            key,
            "connection",
            "show",
            "uuid",
            id,
        ],
    )
    .await?
    .trim()
    .to_string())
}
async fn nm_modify(i: &Interface, values: &[(&str, String)], title: &str) -> Result<Plan> {
    let id = nm_uuid(i)?;
    let mut forward = a(&["connection", "modify", "uuid", &id]);
    let mut undo = forward.clone();
    let mut summary = Vec::new();
    for (key, value) in values {
        let old = nm_value(&id, key).await?;
        summary.push(format!(
            "{key}: {} → {}",
            if old.is_empty() {
                "(automatic/empty)"
            } else {
                &old
            },
            if value.is_empty() {
                "(automatic/empty)"
            } else {
                value
            }
        ));
        forward.extend(a(&[key, value]));
        undo.extend(a(&[key, &old]));
    }
    let reapply = Step::new("nmcli", a(&["device", "reapply", &i.name]), false);
    Ok(Plan {
        title: title.into(),
        summary,
        steps: vec![Step::new("nmcli", forward, false), reapply.clone()],
        undo: vec![Step::new("nmcli", undo, false), reapply],
        disruptive: true,
    })
}

pub async fn prepare(change: Change, s: &Snapshot) -> Result<Plan> {
    match change {
        Change::Pihole { url, password, enabled } => crate::integrations::prepare_pihole(url,password,enabled).await,
        Change::TailscaleDns { enabled } => prepare_tailscale_dns(enabled).await,
        Change::TailscaleExit { target } => prepare_tailscale_exit(&target).await,
        Change::Link{interface,up}=>{
            let i=validate_interface(&interface,s)?;let state=if up{"up"}else{"down"};
            Ok(Plan{title:format!("Interface {state}"),summary:vec![format!("{}: {} → {state}",i.name,i.state)],steps:vec![Step::new("ip",a(&["link","set","dev",&i.name,state]),true)],undo:vec![Step::new("ip",a(&["link","set","dev",&i.name,if i.state=="up"{"up"}else{"down"}]),true)],disruptive:true})
        },
        Change::Mtu{interface,value}=>{
            let i=validate_interface(&interface,s)?;if !(576..=65535).contains(&value){bail!("MTU must be 576–65535; driver may impose narrower limits");}
            Ok(Plan{title:"Set MTU (runtime)".into(),summary:vec![format!("{}: MTU {} → {value}",i.name,i.mtu)],steps:vec![Step::new("ip",a(&["link","set","dev",&i.name,"mtu",&value.to_string()]),true)],undo:vec![Step::new("ip",a(&["link","set","dev",&i.name,"mtu",&i.mtu.to_string()]),true)],disruptive:true})
        },
        Change::Dns{interface,servers,automatic}=>{
            let i=validate_interface(&interface,s)?;if !automatic{valid_servers(&servers)?;}
            if i.connection.is_some()&&available("nmcli") {
                let v4=servers.iter().filter(|s|s.parse::<IpAddr>().is_ok_and(|ip|ip.is_ipv4())).cloned().collect::<Vec<_>>().join(",");
                let v6=servers.iter().filter(|s|s.parse::<IpAddr>().is_ok_and(|ip|ip.is_ipv6())).cloned().collect::<Vec<_>>().join(",");
                nm_modify(i,&[("ipv4.dns",if automatic{String::new()}else{v4}),("ipv6.dns",if automatic{String::new()}else{v6}),("ipv4.ignore-auto-dns",if automatic{"no"}else{"yes"}.into()),("ipv6.ignore-auto-dns",if automatic{"no"}else{"yes"}.into())],"Change DNS").await
            }else if s.dns_source.starts_with("systemd")&&available("resolvectl") {
                if automatic {bail!("Automatic DNS for an unmanaged resolved link is not safely reconstructable. Restore it using the link's network manager.");}
                let mut arguments=a(&["dns",&i.name]);arguments.extend(servers);
                let mut restore=a(&["dns",&i.name]);restore.extend(i.dns.clone());
                if i.dns.is_empty(){bail!("Cannot capture existing per-link DNS; no change applied");}
                Ok(Plan{title:"Change per-link DNS (runtime)".into(),summary:vec![format!("{} DNS: {} → {}",i.name,i.dns.join(", "),arguments[2..].join(", "))],steps:vec![Step::new("resolvectl",arguments,true)],undo:vec![Step::new("resolvectl",restore,true)],disruptive:false})
            }else{bail!("No supported DNS configuration backend for this interface. Manual resolv.conf is read-only.");}
        },
        Change::Dhcp{interface,release}=>{
            let i=validate_interface(&interface,s)?;let id=nm_uuid(i)?;
            let method=nm_value(&id,"ipv4.method").await?;
            if method!="auto"{bail!("Selected profile uses ipv4.method={method}; a DHCP refresh would change its addressing policy");}
            let down=Step::new("nmcli",a(&["--wait","10","device","disconnect",&i.name]),false);
            let up=Step::new("nmcli",a(&["--wait","20","connection","up","uuid",&id,"ifname",&i.name]),false);
            Ok(Plan{title:if release{"Disconnect DHCP connection"}else{"Reconnect DHCP connection"}.into(),summary:vec![format!("{} active profile {id} will disconnect{}",i.name,if release{""}else{" and reactivate to reacquire a lease"}),"A DHCP RELEASE packet is controlled by NetworkManager/profile policy; this action does not promise a server-side release.".into()],steps:if release{vec![down]}else{vec![down,up.clone()]},undo:vec![up],disruptive:true})
        },
        Change::Static{interface,address,gateway}=>{
            let i=validate_interface(&interface,s)?;let ip=cidr(&address)?;let gw:IpAddr=gateway.parse().context("Enter a gateway IP")?;
            if ip.is_ipv4()!=gw.is_ipv4(){bail!("Address and gateway must use the same IP family");}
            let family=if ip.is_ipv4(){"ipv4"}else{"ipv6"};
            nm_modify(i,&[(&format!("{family}.method"),"manual".into()),(&format!("{family}.addresses"),address),(&format!("{family}.gateway"),gateway)],"Configure static IP").await
        },
        Change::Route{destination,gateway,interface,metric,remove}=>{
            validate_interface(&interface,s)?;
            let gw:IpAddr=gateway.parse().context("Gateway must be an IP address")?;
            if destination!="default" && cidr(&destination)?.is_ipv4()!=gw.is_ipv4(){bail!("Route destination/gateway family mismatch");}
            let family=if gw.is_ipv4(){"-4"}else{"-6"};
            let operation=if remove{"del"}else{"add"};let opposite=if remove{"add"}else{"del"};
            let mk=|op:&str|Step::new("ip",a(&[family,"route",op,&destination,"via",&gateway,"dev",&interface,"metric",&metric.to_string()]),true);
            if !remove && s.routes.iter().any(|r|r.destination==destination&&r.interface==interface&&r.metric==metric as u64){bail!("A matching route exists; remove it explicitly before adding a replacement");}
            if remove && !s.routes.iter().any(|r|r.destination==destination&&r.gateway==gateway&&r.interface==interface&&r.metric==metric as u64 && (r.table=="main"||r.table=="254")){bail!("Exact route was not found in the main table; no change applied");}
            Ok(Plan{title:format!("{operation} route (runtime)"),summary:vec![format!("{destination} via {gateway} dev {interface} metric {metric}"),"Main routing table only. Policy tables are shown but not edited.".into()],steps:vec![mk(operation)],undo:vec![mk(opposite)],disruptive:true})
        },
        Change::Radio{enabled}=>{
            let old=cmd("nmcli", &["radio","wifi"]).await?.trim().to_string();let value=if enabled{"on"}else{"off"};
            Ok(Plan{title:"Wi-Fi radio".into(),summary:vec![format!("Radio: {old} → {value}")],steps:vec![Step::new("nmcli",a(&["radio","wifi",value]),false)],undo:vec![Step::new("nmcli",a(&["radio","wifi",if old=="enabled"{"on"}else{"off"}]),false)],disruptive:true})
        },
        Change::WifiConnect{interface,ssid,password,hidden}=>{
            let i=validate_interface(&interface,s)?;if i.kind!="Wi-Fi"{bail!("Choose a Wi-Fi interface");}
            if ssid.is_empty()||ssid.len()>32||ssid.chars().any(char::is_control){bail!("SSID must be 1–32 bytes without control characters");}
            if !password.is_empty() && (!(8..=63).contains(&password.len())||password.chars().any(char::is_control)){bail!("WPA personal password must be 8–63 bytes without control characters. Enterprise/WEP profiles should be created with NetworkManager externally.");}
            let name=format!("nexus-{}",chrono::Utc::now().timestamp_millis());
            let mut add=a(&["connection","add","type","wifi","ifname",&interface,"con-name",&name,"ssid",&ssid,"wifi.hidden",if hidden{"yes"}else{"no"},"connection.autoconnect","no"]);
            if !password.is_empty(){add.extend(a(&["wifi-sec.key-mgmt","wpa-psk","wifi-sec.psk-flags","2"]));}
            let mut up=Step::new("nmcli",a(&["--wait","20","connection","up","id",&name,"ifname",&interface,"passwd-file","/dev/stdin"]),false);
            if !password.is_empty(){up.input=Some(format!("802-11-wireless-security.psk:{password}\n").into_bytes());}
            let mut undo=vec![Step::new("nmcli",a(&["connection","delete","id",&name]),false)];
            if let Some(id)=&i.connection{undo.push(Step::new("nmcli",a(&["connection","up","uuid",id,"ifname",&interface]),false));}
            Ok(Plan{title:"Connect Wi-Fi".into(),summary:vec![format!("{} → SSID {ssid} (hidden: {hidden})",i.name),"Creates a profile with auto-connect off. Password is supplied on stdin, excluded from argv/reports, and marked not saved. WPA personal/open networks only.".into()],steps:vec![Step::new("nmcli",add,false),up],undo,disruptive:true})
        },
        Change::WifiDisconnect{interface}=>{
            let i=validate_interface(&interface,s)?;let id=nm_uuid(i)?;
            Ok(Plan{title:"Disconnect Wi-Fi".into(),summary:vec![format!("{}: disconnect active profile {id}",i.name)],steps:vec![Step::new("nmcli",a(&["device","disconnect",&i.name]),false)],undo:vec![Step::new("nmcli",a(&["connection","up","uuid",&id]),false)],disruptive:true})
        },
        Change::WifiForget{uuid:id}=>{
            uuid(&id)?;let kind=nm_value(&id,"connection.type").await?;if kind!="802-11-wireless"{bail!("Only Wi-Fi profiles may be forgotten here");}
            let name=nm_value(&id,"connection.id").await?;
            Ok(Plan{title:"Forget Wi-Fi profile".into(),summary:vec![format!("Delete saved profile {name} ({id})"),"This deletes the saved profile and cannot be undone; credentials are never exported.".into()],steps:vec![Step::new("nmcli",a(&["connection","delete","uuid",&id]),false)],undo:vec![],disruptive:true})
        },
        Change::Autoconnect{uuid:id,enabled}=>{
            uuid(&id)?;let old=nm_value(&id,"connection.autoconnect").await?;let value=if enabled{"yes"}else{"no"};
            Ok(Plan{title:"Set auto-connect".into(),summary:vec![format!("Profile {id}: {old} → {value}")],steps:vec![Step::new("nmcli",a(&["connection","modify","uuid",&id,"connection.autoconnect",value]),false)],undo:vec![Step::new("nmcli",a(&["connection","modify","uuid",&id,"connection.autoconnect",&old]),false)],disruptive:false})
        },
        Change::FlushDns=>Ok(Plan{title:"Flush DNS cache".into(),summary:vec!["systemd-resolved cache will be cleared. Cache contents cannot be restored.".into()],steps:vec![Step::new("resolvectl",a(&["flush-caches"]),true)],undo:vec![],disruptive:false}),
        Change::Profile(profile)=>{
            if profile.proxy.is_some()||!profile.routes.is_empty(){bail!("This release applies captured DNS and MTU profiles only; proxy/routes are retained as metadata, not silently applied");}
            let mut plan=prepare_basic_profile(&profile,s).await?;plan.title=format!("Apply profile {}",profile.name);Ok(plan)
        },
        Change::Restart=>Ok(Plan{title:"Restart NetworkManager".into(),summary:vec!["Restart NetworkManager.service. All managed links may disconnect; no guaranteed rollback. Do not use during a remote session unless you have a recovery path.".into()],steps:vec![Step::new("systemctl",a(&["restart","NetworkManager.service"]),true)],undo:vec![],disruptive:true})
    }
}
async fn prepare_basic_profile(p: &SavedProfile, s: &Snapshot) -> Result<Plan> {
    let interface = validate_interface(&p.interface, s)?;
    valid_servers(&p.dns)?;
    let v4 = p
        .dns
        .iter()
        .filter(|s| s.parse::<IpAddr>().is_ok_and(|ip| ip.is_ipv4()))
        .cloned()
        .collect::<Vec<_>>()
        .join(",");
    let v6 = p
        .dns
        .iter()
        .filter(|s| s.parse::<IpAddr>().is_ok_and(|ip| ip.is_ipv6()))
        .cloned()
        .collect::<Vec<_>>()
        .join(",");
    let mut plan = nm_modify(
        interface,
        &[
            ("ipv4.dns", v4),
            ("ipv6.dns", v6),
            ("ipv4.ignore-auto-dns", "yes".into()),
            ("ipv6.ignore-auto-dns", "yes".into()),
        ],
        "Apply captured DNS",
    )
    .await?;
    if let Some(value) = p.mtu {
        let i = validate_interface(&p.interface, s)?;
        if !(576..=65535).contains(&value) {
            bail!("Invalid profile MTU");
        }
        plan.steps.push(Step::new(
            "ip",
            a(&["link", "set", "dev", &i.name, "mtu", &value.to_string()]),
            true,
        ));
        plan.undo.insert(
            0,
            Step::new(
                "ip",
                a(&["link", "set", "dev", &i.name, "mtu", &i.mtu.to_string()]),
                true,
            ),
        );
        plan.summary.push(format!("MTU: {} → {value}", i.mtu));
    }
    Ok(plan)
}

pub async fn apply(plan: &Plan) -> Result<ToolResult> {
    for step in &plan.steps {
        if step.api.is_some() {
            continue;
        }
        if !available(&step.program) {
            bail!(
                "{} is unavailable; no steps have been applied",
                step.program
            );
        }
        if step.privileged && !is_root() && !available("pkexec") {
            bail!("pkexec is unavailable; no steps have been applied. Install a PolicyKit agent/helper to authorize privileged actions.");
        }
    }
    let mut r = ToolResult {
        title: plan.title.clone(),
        at: chrono::Utc::now(),
        columns: vec!["Step".into(), "Result".into()],
        ..Default::default()
    };
    for (index, step) in plan.steps.iter().enumerate() {
        let (program, arguments) = if step.privileged && !is_root() {
            if !available("pkexec") {
                bail!("This action needs elevated privileges. pkexec is not installed. No automatic sudo or shell fallback is used.");
            }
            let mut arguments = vec![step.program.clone()];
            arguments.extend(step.args.clone());
            ("pkexec", arguments)
        } else {
            (step.program.as_str(), step.args.clone())
        };
        let output = if let Some(change) = &step.api {
            crate::integrations::apply_pihole(change).await
        } else if step.input.is_some() {
            run_with_input(
                program,
                &arguments,
                step.input.clone(),
                Duration::from_secs(30),
            )
            .await
        } else {
            run(program, &arguments, Duration::from_secs(30)).await
        };
        match output {
            Ok(_) => r.rows.push(vec![
                format!("{} / {} ({})", index + 1, plan.steps.len(), step.program),
                "Applied".into(),
            ]),
            Err(e) => {
                r.notes.push(format!("Step {} failed: {e}. Earlier steps may have applied; review the result and use 'u' to preview a revert.",index+1));
                r.metrics
                    .insert("Outcome".into(), "Partial / failed".into());
                return Ok(r);
            }
        }
    }
    r.metrics.insert("Outcome".into(), "Applied".into());
    r.notes.push(
        if plan.undo.is_empty() {
            "No revert is available for this action."
        } else {
            "Previous values remain in memory; press u to review a revert."
        }
        .into(),
    );
    Ok(r)
}
pub fn is_root() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

async fn prepare_tailscale_dns(enabled: bool) -> Result<Plan> {
    let prefs: serde_json::Value =
        serde_json::from_str(&cmd("tailscale", &["debug", "prefs"]).await?)?;
    let current = prefs["CorpDNS"].as_bool().context(
        "Tailscale CLI cannot report its current accept-DNS preference; no change prepared",
    )?;
    Ok(Plan { title: "Tailscale DNS preference".into(), summary: vec![format!("Accept tailnet DNS: {current} → {enabled}"), "Changes only accept-dns. Operator or root permission is required; NEXUS does not reset other preferences.".into()], steps: vec![Step::new("tailscale",vec!["set".into(),format!("--accept-dns={enabled}")],false)], undo: vec![Step::new("tailscale",vec!["set".into(),format!("--accept-dns={current}")],false)], disruptive: true })
}
async fn prepare_tailscale_exit(target: &str) -> Result<Plan> {
    let value: serde_json::Value =
        serde_json::from_str(&cmd("tailscale", &["status", "--json"]).await?)?;
    let status = crate::integrations::parse_tailnet(&value)?;
    let old = status
        .peers
        .iter()
        .find(|p| p.exit_node)
        .and_then(|p| p.ips.first())
        .cloned()
        .unwrap_or_default();
    if value["ExitNodeStatus"].is_object() && old.is_empty() {
        bail!("Cannot capture current exit-node address; no change prepared");
    }
    if !target.is_empty() {
        let address: IpAddr = target
            .parse()
            .context("Select an exit-node IP shown on the Tailscale page (blank disables)")?;
        if !status.peers.iter().any(|p| {
            p.exit_available
                && p.ips
                    .iter()
                    .any(|ip| ip.parse::<IpAddr>().ok() == Some(address))
        }) {
            bail!("Address is not an advertised exit node in this tailnet");
        }
    }
    Ok(Plan { title: "Tailscale exit node".into(), summary: vec![format!("Exit node: {} → {}",if old.is_empty(){"none"}else{&old},if target.is_empty(){"none"}else{target}), "Default traffic can move to this exit node. Changes only exit-node; preserves LAN-access and other preferences. Operator or root permission is required.".into()], steps: vec![Step::new("tailscale",vec!["set".into(),format!("--exit-node={target}")],false)], undo: vec![Step::new("tailscale",vec!["set".into(),format!("--exit-node={old}")],false)], disruptive: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Snapshot {
        Snapshot {
            interfaces: vec![Interface {
                name: "eth0".into(),
                mtu: 1500,
                state: "up".into(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }
    #[tokio::test]
    async fn mtu_plan_captures_undo_without_mutating() {
        let p = prepare(
            Change::Mtu {
                interface: "eth0".into(),
                value: 1400,
            },
            &snapshot(),
        )
        .await
        .unwrap();
        assert_eq!(p.steps[0].args.last().unwrap(), "1400");
        assert_eq!(p.undo[0].args.last().unwrap(), "1500");
    }
    #[tokio::test]
    async fn invalid_mtu_and_interface_rejected() {
        assert!(prepare(
            Change::Mtu {
                interface: "eth0".into(),
                value: 1
            },
            &snapshot()
        )
        .await
        .is_err());
        assert!(prepare(
            Change::Link {
                interface: "--help".into(),
                up: true
            },
            &snapshot()
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn no_arbitrary_route_delete() {
        assert!(prepare(
            Change::Route {
                destination: "default".into(),
                gateway: "10.0.0.1".into(),
                interface: "eth0".into(),
                metric: 0,
                remove: true
            },
            &snapshot()
        )
        .await
        .is_err());
    }
}
