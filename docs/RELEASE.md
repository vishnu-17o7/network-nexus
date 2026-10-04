# NEXUS 0.5.0 — network workbench

The overview keeps its familiar layout. The rest of the app now has task-specific
views: inspection lists with visible details, a focused diagnostic test catalog,
findings with evidence and next steps, chart-led monitoring, service connection
states, and preferences beside saved profiles.

Both pixel and portable terminal graphs use stable ordered-dither fills beneath
clear traces. Real spikes and missing-reply gaps remain visible. The Latency page
charts the selected target, including frozen history. Background seams between
panels, chart layers and gutters are removed across all six themes and custom colors.

Secondary tabs are visible and clickable. Use **comma / period** to move within a
section, **a** for the displayed page action, and **b** to return from a result to
the test catalog. **Ctrl+K** continues to expose every tool and control. Filtering
and sorting now keep interface, Wi-Fi, profile and Tailscale form defaults aligned
with the visible selection. Compact layouts retain profile rows and diagnostic
next steps. Muted and status text contrast is improved in built-in themes.

The design system and Hallmark review are documented in `design.md`. Screenshots
come from the actual widgets with labeled example data. The pending 0.4.2
background fix is included here; there was no intermediate 0.4.2 release.

## Install or upgrade

Download the appropriate `.deb` and `SHA256SUMS` from this release into an empty
directory. Verify the package reports `OK`, then install:

```bash
sha256sum --check --ignore-missing SHA256SUMS
sudo apt install ./nexus-net_0.5.0_amd64.deb
nexus --version
nexus --doctor
nexus
```

For **64-bit Raspberry Pi OS Bookworm or newer**, confirm
`dpkg --print-architecture` reports `arm64`, then install
`nexus-net_0.5.0_arm64.deb` using the same steps. An `armhf` userspace cannot use the
ARM64 binary. Standalone amd64 and ARM64 archives are also available.

Native builds use Ubuntu 22.04 and reject GLIBC requirements above 2.35. Release
publication requires Rust formatting/tests/Clippy, terminal navigation and
restoration, blocked-I/O responsiveness, graphics transport, package installation,
and verification of the exact artifacts on Ubuntu 24.04 amd64 and Debian 12/13 ARM64.
Physical Pi hardware and individual terminal compositors remain outside CI coverage.

Installation does not start a service or alter network configuration. External
tests and network changes retain their existing consent and confirmation flows.
Pi-hole credentials remain session-only. Windows, macOS, 32-bit ARM and static-musl
binaries are not included.

[Full installation instructions](https://github.com/vishnu-17o7/network-nexus#install-nexus-050)
· [Platform support](https://github.com/vishnu-17o7/network-nexus/blob/main/docs/PORTABILITY.md)
· [Screenshots](https://github.com/vishnu-17o7/network-nexus/blob/main/docs/workbench-gallery.png)
· [Validation](https://github.com/vishnu-17o7/network-nexus/blob/main/docs/VALIDATION.md)
