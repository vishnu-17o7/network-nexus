# NEXUS 0.3.0 release

The release includes a Linux amd64 `.deb`, a binary archive, source and checksums.
The prebuilt executable requires glibc 2.39 or newer. Ubuntu 24.04+ is compatible;
older systems and other architectures should build from source.

```bash
sudo apt install ./nexus-net_0.3.0_amd64.deb
nexus
```

The package installs `/usr/bin/nexus`, a manual page and a terminal desktop
launcher. It contains no maintainer scripts or service and does not alter network
configuration. Optional networking utilities are recommended or suggested.

Pi-hole requires the v6 API; Tailscale requires its installed CLI and daemon. Connect explicitly via Ctrl+K. Credentials are session-only, and configuration controls keep the preview/confirmation flow. See README for setup and limitations.

All external checks remain explicit. Press `d` to run the diagnostic sequence,
then choose a finding to inspect its evidence and next step. Use Ctrl+K for
HTTP, DNS, DNS-over-TLS, TLS and configuration tools.

The GitHub workflows run formatting, tests, Clippy, release build, PTY and
blocked-I/O responsiveness checks. The release workflow builds assets and creates
a GitHub release for a pushed `v*` tag or a main-branch commit that changes Cargo.toml. Main-branch publishing skips an existing version rather than replacing its assets. A manual run only creates downloadable workflow artifacts.

The included source archive contains `nexus-net.bundle`, a local Git history
bundle. To recover that history:

```bash
git clone nexus-net.bundle nexus-net
```

Source tracking: https://github.com/vishnu-17o7/network-nexus. GitHub build and release status must be checked separately from the locally validated binaries.
