# NEXUS 0.4.1 release

This patch fixes the v0.4.0 installation failure on Ubuntu 22.04. Release binaries
are built natively for amd64 and ARM64 in Ubuntu 22.04 containers and packaging rejects requirements newer
than GLIBC 2.35. CI also installs the generated Debian package and runs the installed
executable. The host system does not need a libc replacement.

Smooth charts use Kitty-compatible terminal graphics; other terminals fall back to portable text. Space freezes graph history, [ / ] changes the traffic/probe range, and collection continues. See README for renderer selection and support boundaries.

The release includes Linux amd64 and ARM64 `.deb` packages and binary archives,
source and shared checksums. ARM64 packages target 64-bit Raspberry Pi OS Bookworm
or newer and other compatible ARM64 Linux systems.
The prebuilt executable targets glibc 2.35 or newer. Ubuntu 22.04 and 24.04 are CI targets;
older systems and unlisted architectures should build from source.

```bash
sudo apt install ./nexus-net_0.4.1_amd64.deb
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
blocked-I/O responsiveness checks in Ubuntu 22.04 and 24.04 userspaces. The release
job uses the Ubuntu 22.04 baseline and a maximum-GLIBC gate. The release workflow builds assets and creates
a GitHub release for a pushed `v*` tag or a main-branch commit that changes Cargo.toml. Main-branch publishing skips an existing version rather than replacing its assets. A manual run only creates downloadable workflow artifacts.

The included source archive contains `nexus-net.bundle`, the original v0.2.0/v0.3.0 Git history
bundle. Current development history is tracked in GitHub. To recover that history:

```bash
git clone nexus-net.bundle nexus-net
```

Source tracking: https://github.com/vishnu-17o7/network-nexus. GitHub build and release status must be checked separately from the locally validated binaries.

Common Linux distributions can use the standalone archive when they meet the CPU/libc requirements. Native Windows, macOS, 32-bit ARM and static-musl releases are not included. See [platform support](https://github.com/vishnu-17o7/network-nexus/blob/main/docs/PORTABILITY.md).

For Raspberry Pi, check `dpkg --print-architecture` reports `arm64`, then use `nexus-net_0.4.1_arm64.deb`. An `armhf` userspace cannot use this package, even with a 64-bit kernel. The ARM64 release artifacts pass installation and PTY checks on native ARM64 Debian 12/13 CI userspaces before publication; real Pi hardware and Wi-Fi drivers remain unverified. Full commands are in the [README](https://github.com/vishnu-17o7/network-nexus#raspberry-pi--64-bit-raspberry-pi-os).
