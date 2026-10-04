# Feature support and practical limits

This is a working Linux application. The scope below separates implemented
behavior from hardware-dependent observations and remaining extensions; it
does not claim that every possible network configuration has been tested.

| Requested area | Current implementation and limits |
| --- | --- |
| Dashboard | Live rate/RTT/DNS cards, large traffic/latency plots, actionable findings, network context and stale-snapshot state; explicit public IP and ICMP/loss/DNS results when enabled. Internet quality, public IP and speed are unmeasured until the appropriate test runs. |
| Interfaces | Full sysfs counters and addresses with structured `ip` plus native fallbacks. Speed/duplex can be unavailable for virtual links/drivers. Runtime link/MTU controls, NetworkManager DHCP reconnect/disconnect and static IP are implemented. |
| Wi-Fi | Scan/cache, signals/channels/security, saved profiles, open/WPA-personal/hidden connections, forget/disconnect, auto-connect and radio controls. Enterprise/WEP provisioning is not implemented. Scan overlap is a heuristic; actual airtime requires driver survey support. |
| Tailscale | Dedicated local JSON peer view, daemon state, tailnet/addresses, direct/DERP paths, RX/TX, advertised/active exit nodes and health warnings. Explicit discovery ping and structured NAT/DERP netcheck. DNS/exit-node changes are previewed and retain undo. Needs installed CLI/accessible daemon; CLI JSON/preferences can vary with version. Operator/root access must be configured; no automatic login. |
| Pi-hole | v6 session API, HTTPS verification, blocking/timer, aggregate statistics/history graph, explicit connection, visible-page polling, confirmed 60-second pause/resume with captured state. Credentials are memory-only and omitted from reports, debug output and argv. Redirects and embedded URL credentials are rejected; no per-client/domain queries. v5 and server installation are unsupported; API reachability is separate from DNS service reachability. |
| DNS | Sources, per-link DNS, presets, custom/automatic configuration, cache flush, supported record types, resolver comparison and resolved status/statistics/DoT observation plus native verified DoT A queries (TCP/TLS/DNS error separation). No portable cache-record enumeration or automatic unmanaged-file editing. |
| Speed testing | Full external Ookla/Python tests or explicit quick/full iperf3 tests against your server. Backend/server retained with tests. Data usage is stated. No automatically accepted provider terms or pretend upload measurement. Three matched previous results are used for a 40%-drop observation. |
| Latency | Multiple targets plus gateway, min/max/mean/median/jitter/loss, selected-target dithered line graph with missing-reply gaps, visible-window p95/loss and 30/90/300-probe ranges, bounded histories and spike/interruption observations. ICMP loss is response loss; gateway and destination reachability are not conflated. |
| Path analysis | Traceroute and MTR structured reports; stored traceroutes can be compared for the same target through the palette. A changed responder can be load balancing, not necessarily a changed physical route. Automated periodic MTR is not launched in the background. |
| Connections | Procfs TCP/UDP IPv4/IPv6 state, endpoints, UID, PID/process and executable/inode details when visible. Process names can be unavailable under normal user/container permissions. Sorting is by table columns; per-socket rate attribution is not invented. |
| Ports | Listening sockets, common-service labels, wildcard binds, filtering and detail view. Public exposure requires a real external reachability assessment and is not inferred from a wildcard address. |
| Bandwidth | Counter-derived interface rates/totals and primary-interface history; without a default interface, the overview shows an explicitly labeled sum of non-loopback links. Summed virtual/physical links can count traffic more than once. NetHogs provides a privileged five-interval process sample rather than a permanently enabled capture daemon. |
| Routes | IPv4/IPv6 table views, defaults/metrics/protocols and policy-rule inspection. Add/delete require exact main-table gateway routes. Modifying a route currently means explicitly removing and adding it; arbitrary policy-table/multipath changes are read-only. |
| Neighbors | ARP/IPv6 neighbor state, MAC/interface information, first/last session observations, local OUI vendor data and labels. Cached state does not prove that a device is currently online. Active duplicate-IP probing is not implemented. |
| LAN discovery | Explicit ICMP probes with bounded concurrency on directly connected private IPv4 /24–/30. No intrusive default scan. Silent/filtered devices may be missed; an empty response does not prove offline. Automatic vendor web lookups and broad subnet/port scanning are excluded. |
| Diagnostics | Readable interface/link/IP/routing/gateway/DNS/internet-IP/HTTPS checks with interpretation. Not an automatic repair engine; DHCP method is not inferred merely from an address. |
| Connectivity | ICMP, TCP connection timing, UDP response/indeterminate behavior, DNS and HTTP/HTTPS. Protocol-specific UDP validation beyond DNS is not implemented. |
| HTTP | HEAD, redirects, status, server headers, resulting IP and curl DNS/connect/TLS/TTFB/total timing; explicit IPv4/IPv6 comparison. Redirect timings are cumulative; separate differences are indicative across redirects. Body inspection is intentionally omitted. |
| TLS | Enforced trust/hostname verification, subject/issuer/SAN/expiry/days, available protocol/cipher and server-chain count. Invalid trust is an error; the app does not disable certificate validation to obtain a green status. |
| Public IP | Explicit IPv4/IPv6 retrieval and change observation; explicit ipapi.co ASN/org/region/country. Reverse DNS is available through PTR lookup. Location is approximate and service-provided. |
| VPN | Interface-name heuristics, addresses/default-route view, policy rules, non-secret WireGuard peer handshake/transfer/endpoint information, Tailscale status. Split tunnels and unusual interface names need manual interpretation; no automatic anonymity or DNS-leak guarantee. |
| Proxy | Redacted environment and GNOME proxy observations. Application-specific proxy settings may differ. Parent-shell proxy mutation is not implemented. |
| Firewall | Read-only nftables/iptables/UFW/firewalld observations, searchable result rows, access errors. No firewall changes. Complex rules are retained as structured expression summaries; effective reachability across all chains/namespaces/upstream firewalls is not statically guaranteed. |
| Namespaces | Named namespaces, `lsns` metadata when installed, named-namespace interface inspection. No namespace creation, deletion or automatic privilege escalation during inspection. |
| Containers | Docker/Podman network/IPAM/driver/attached IP/published-port overview when engine APIs are accessible. Host port mappings can be compared to listeners. No container/network mutation or inference of inaccessible container state. |
| Developer dashboard | Common dev TCP ports, PID/process and local HTTP HEAD responses, same-PID port observations, wildcard-binding hints. HTTPS/non-HTTP endpoints can fail plain HTTP. Multiple ports are not automatically called duplicate applications. |
| Events | Link/IP/routes/DNS/Wi-Fi/VPN/neighbor changes, probe interruptions/spikes, public-IP changes, tasks, speed degradation and confirmed changes. Polling can miss changes shorter than the refresh interval. |
| Historical metrics | Persisted bounded traffic/latency/loss/DNS samples, speed tests, diagnostic results and events. Traffic has 1/5/15-minute ranges; latency has 30/90/300-probe ranges; speed uses ordered tests. Space freezes graphs while collection continues. Arbitrary historical dates and historical path graphs remain extensions. |
| Profiles | Capture/apply named DNS and runtime MTU profiles, with previews and captured revert values. Optional proxy/routes exist in the data model but are not applied; unsupported metadata fails explicitly. Static IP can be configured directly. |
| Command palette | Ctrl+K fuzzy search across all pages, tools and supported controls; validated typed forms and contextual selected-interface defaults. |
| Global search | Interfaces, connections/processes/endpoints, routes, DNS, Wi-Fi, profiles, neighbors, events and current diagnostic rows. Search operates on locally known data and does not submit queries externally. |
| TUI | Full screen, rounded borders, responsive/compact layouts, cards/tables/graphs, overlays, loading indicators, help, selection/filter/sort/detail views, Vim keys, wheel scrolling and sidebar mouse clicks. Wide table fields can be inspected in a detail modal. |
| Smooth charts | Antialiased cubic vector paths rendered as PNG through Kitty graphics in known compatible terminals; SVG plot exports with headless previews. Curves preserve measured extrema and gaps. Unknown terminals and tmux/screen use curved Braille text. Cached images refresh only on changed data/layout/theme and are removed for modals, page changes and exit. Real emulator compositor/DPI behavior remains a desktop validation step. |
| Themes | Six built-in themes and custom TOML colors. Text labels accompany status colors. Unicode rendering quality depends on the terminal font. |
| Privileges | Normal-user monitoring, explicit mutation previews, optional pkexec for actions needing elevation, NetworkManager authorization through PolicyKit, bounded operations and best-effort confirmed revert. No automatic root restart. |
| Architecture | UI/state/services/models are separate; platform backend trait; Tokio workers; task IDs reject late cancelled-task results; command arguments are never passed to a shell; time/output bounds and terminal cleanup. |
| Integrations | Sysfs/procfs/getifaddrs/ioctl, structured iproute2 and backend-specific utilities. Missing commands/permissions return clear failures. Native packet capture and a full NetworkManager D-Bus backend are future improvements. |
| Reports | Redacted JSON/text and explicitly detailed CLI JSON/text, local metrics, interfaces/routes/listeners, capability/errors/events and recorded diagnostic/firewall/VPN results if those tools ran. Reports do not silently run privileged/external checks to fill gaps. |
| Privacy | Local-only default, individual task previews/global opt-in, no telemetry, no auto-geolocation, masked stdin Wi-Fi secret, no WireGuard private-key reads, proxy/header redaction, owner-only persistence. |

## Backend references

- Ratatui: https://docs.rs/ratatui/0.29.0/ratatui/
- Crossterm async events: https://docs.rs/crossterm/0.28.1/crossterm/event/struct.EventStream.html
- NetworkManager command and password-file semantics:
  https://networkmanager.pages.freedesktop.org/NetworkManager/NetworkManager/nmcli.html
- NetworkManager properties:
  https://networkmanager.pages.freedesktop.org/NetworkManager/NetworkManager/nm-settings-nmcli.html
- NetHogs trace mode: https://github.com/raboof/nethogs
- IP metadata: https://ipapi.co/api/

Linux is the only supported runtime in this release. Cross-platform support
requires real platform backends, not merely recompiling the UI.

- Tailscale CLI: https://tailscale.com/docs/reference/tailscale-cli
- Tailscale structured status / CLI implementations: https://github.com/tailscale/tailscale/tree/main/cmd/tailscale/cli
- Pi-hole v6 authentication: https://docs.pi-hole.net/api/auth/
- Pi-hole v6 OpenAPI: https://github.com/pi-hole/FTL/tree/master/src/api/docs/content/specs
