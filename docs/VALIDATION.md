# Validation evidence

## v0.4.1 compatibility gate

CI now builds and tests inside Ubuntu 22.04 and 24.04 containers on a supported
GitHub-hosted runner. The release uses the Ubuntu 22.04 userspace and runs
`scripts/package.py --max-glibc 2.35`; a newer imported GLIBC version fails the
release before publication. The CI jobs install the generated `.deb` and
run its installed `nexus --version` and `nexus --doctor`. Each container uses its
own Python; Python 3.10 uses the `python3-tomli` package for TOML parsing.

Native ARM64 build jobs run the Rust, lint and PTY suites. The exact ARM64 release
artifacts then undergo installation, navigation, blocked-I/O responsiveness and
graphics PTY checks in Debian 12 and 13 ARM64 containers. The amd64 release
artifact is also installed and exercised in Ubuntu 24.04. Publication waits for
all these gates. This covers a compatible userspace for 64-bit Raspberry Pi OS;
physical Pi hardware, wireless drivers, 32-bit OS images and board-specific
permissions have not been tested.

Check the [GitHub runs](https://github.com/vishnu-17o7/network-nexus/actions) for
results of this gate. These container tests validate ABI compatibility and
installation, not every distribution's desktop services, hardware or permissions.

## Application validation (v0.4.0)

Verified in a Linux x86_64 / Ubuntu 24.04 execution environment using Rust 1.88.0,
Ratatui 0.29, Crossterm 0.28 and the committed Cargo lockfile. The source
dependency graph requires Rust 1.88 or newer.

| Check | Result |
| --- | --- |
| `cargo fmt --check` | Pass |
| `cargo clippy --locked --all-targets -- -D warnings` | Pass |
| `cargo test --locked` | 54 tests pass |
| `cargo build --release --locked` | Optimized executable builds |
| Actual executable in a PTY | Pass: page navigation, detail overlay, palette, MTU form cancellation, theme, local refresh, help, 40×12 / 60×48 / 80×24 / 200×48 resize, masked Pi-hole form cancellation, exit |
| Background responsiveness | Help opens within 600 ms during a stalled local HTTP request; cancellation restores control |
| Terminal restoration | Alternate screen exited; canonical input and echo restored after quit |
| Temporary session | `--fresh` does not overwrite persistent history/settings |
| `--doctor` | Lists actual present/missing backend binaries |
| `--snapshot` | Reads actual sysfs/procfs interface and socket state |
| `--report PATH` | Valid JSON; interfaces/MACs/endpoints/process metadata redacted |
| Live local HTTP server | Sends HEAD, records actual status/timing, distinguishes 404 from an outage, redacts cookie and API-key headers |
| Live local TCP / UDP servers | Connect/response behavior verified using loopback sockets |
| UI render coverage | 17 pages × 6 themes × 9 terminal sizes plus overlays; no panics |
| Actual dark/light dashboard | Rendered from real local snapshots and visually reviewed |
| DNS-over-TLS | Local native TLS server: verified A response, SERVFAIL, hostname mismatch and untrusted CA |
| Debian release | Metadata, ownership/modes, extracted executable and package dependencies checked |
| Tokscale-inspired previews | Real widgets with labeled documentation fixtures; overview, DoT finding, HTTP 404 and animated progress |
| Pi-hole v6 | Local mock API: authentication/header SID, normalized summary/history, logout cleanup, redirect refusal, 401 secrecy, preview-only preparation, timed pause, revert and changed-state protection |
| Tailscale | Structured JSON fixtures cover direct/DERP, exit node and unknown peer state; actual daemon/tailnet unavailable in this environment, so no real peer or routing change is claimed |
| Line charts | Shape-preserving cubic interpolation retains measured endpoints and stays within adjacent values; missing/invalid values and long sampling gaps split segments; traffic uses binary rate units and 1/5/15-minute ranges; latency uses probe indices, visible-window p95/loss and 30/90/300-probe ranges |
| Graphics transport in PTY | PNG payloads, chunk boundaries, cell dimensions, unchanged-frame cache, freeze, popup removal/restoration, resize, range control, owned-image deletion, text fallback and terminal restoration pass |
| Smooth previews | Actual renderer PNG layers and SVG paths generated from labeled fixtures; dark/light, 80×24, 60×48 and 200×48 layouts visually reviewed |
| Safety behavior | Esc from change preview produces no apply action; invalid MTU/interface/route deletion rejected; untrusted host/options rejected |
| History and rate math | Newest samples retained; counter reset produces no overflow rate; packet loss stays separate from successful-reply latency/jitter |
| Private atomic save | New file mode 0600; existing parent-directory mode is preserved |

The PTY test is reproducible:

```bash
python3 tests/terminal_smoke.py target/release/nexus
python3 tests/responsiveness.py target/release/nexus
python3 tests/graphics_pty.py target/release/nexus
```

The preview is reproducible on a Linux machine with Pillow and DejaVu fonts:

```bash
nexus --fresh --chart-renderer kitty --render buffer.json
python3 docs/render_preview.py buffer.json dashboard.png
```

## Limits of these checks

The graphics PTY test validates application output and lifecycle, not a real Kitty
or Ghostty compositor. Smooth screenshots combine the actual widget buffer and
PNG renderer layer. Live terminal emulator rendering, remote SSH transport and
font/DPI behavior still need testing on the target desktop. Unknown terminals
and tmux/screen use the portable text renderer.

This environment denies netlink sockets. That exposed and verified the native
getifaddrs/ioctl/procfs fallback. It does not represent a normal Linux desktop's
complete network state; unavailable default routes/addresses remain visibly
unavailable rather than being fabricated. Loopback, sockets, counters and
capability checks were exercised with real data.

No machine interfaces, DNS settings, routes, firewall, Wi-Fi credentials or
NetworkManager profiles were changed during verification. Preview generation
and validation are tested; applying these settings on a real desktop is still
hardware/backend/authorization dependent. Wi-Fi connection, NetworkManager
reapply, PolicyKit dialogs, driver survey, WireGuard/Tailscale, real
Docker/Podman engines, provider speed tests and broad desktop compatibility were
not available for end-to-end verification here. External speed/geo services were
not contacted merely to populate the preview.

The v0.4.0 prebuilt executable imported glibc 2.39 symbols. Version 0.4.1
changes the build baseline and caps requirements at glibc 2.35. Native amd64 and ARM64 assets are provided. Other CPU/libc
combinations should compile locally until separately built and tested assets exist. The binary is not a universal Windows/macOS application.

See [FEATURES.md](FEATURES.md) for implemented behavior and support boundaries.
