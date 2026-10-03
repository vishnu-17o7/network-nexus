# NEXUS 0.4.2 release

This patch removes the dark background bars between metric cards, graphs and
findings. The entire main content area now has one continuous surface, including
layout spacing, empty states and service summaries. Text charts and pixel graphics
use the same background. All six themes and custom colors are covered; navigation,
footer and selected-row styling retain their visual hierarchy.

Background regression checks cover all 17 pages and compact, tall and wide
terminals, plus empty/populated charts in both renderers. CI exports the actual
widget buffers and PNG/SVG chart layers used for the refreshed screenshots.

The release includes Linux amd64 and ARM64 `.deb` packages and binary archives,
source and shared checksums. Native builds retain the Ubuntu 22.04 baseline and
reject imported GLIBC requirements newer than 2.35. The release gates run Rust
formatting, tests, Clippy, terminal navigation, blocked-I/O responsiveness and
graphics transport checks, then verify the exact packages on Ubuntu 24.04 amd64
and Debian 12/13 ARM64 before publication.

For Ubuntu/Debian amd64, download the `.deb` and `SHA256SUMS` from this release.
Verify that the downloaded package reports `OK`, then install:

```bash
sha256sum --check --ignore-missing SHA256SUMS
sudo apt install ./nexus-net_0.4.2_amd64.deb
nexus --version
nexus
```

For **64-bit Raspberry Pi OS Bookworm or newer**, confirm
`dpkg --print-architecture` reports `arm64`, then install
`nexus-net_0.4.2_arm64.deb` with the same steps. An `armhf` userspace cannot use
this package, even with a 64-bit kernel. Physical Pi hardware and wireless drivers
remain outside the automated test coverage.

The package installs `/usr/bin/nexus`, a manual page and a terminal desktop
launcher. It contains no maintainer scripts or service and does not alter network
configuration. Installing the newer `.deb` upgrades the existing package.

Smooth curves use Kitty-compatible terminal graphics. Other terminals and
tmux/screen use portable text graphs. Space freezes graph history while collection
continues; [ / ] changes the range. The background fix applies to both renderers,
including SSH sessions.

External checks remain explicit. Pi-hole uses the v6 API; Tailscale uses its
installed CLI and daemon. Credentials remain session-only and configuration
changes retain preview and confirmation.

Full [installation and upgrade instructions](https://github.com/vishnu-17o7/network-nexus#install-nexus-042)
are in the README, including checksums, standalone archives and source builds.
Common Linux distributions can use the archive when CPU/libc requirements match.
Native Windows, macOS, 32-bit ARM and static-musl assets are not included; see
[platform support](https://github.com/vishnu-17o7/network-nexus/blob/main/docs/PORTABILITY.md).

Source and build history: https://github.com/vishnu-17o7/network-nexus.
