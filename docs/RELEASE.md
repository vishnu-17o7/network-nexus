# NEXUS 0.5.1 — interface polish

A focused polish pass informed by the installed Hallmark skill and a fresh review of all 17 pages.

- Inspection details use aligned fields with identity and state first. Lists have descriptive titles, a visible selection marker, right-aligned measurements and explicit truncation.
- Compact views retain more useful information. Empty lists no longer show an unused selection pane.
- Preferences group appearance, collection and access. Diagnostic findings are easier to scan on small terminals.
- Dialogs keep keyboard focus clear. Long inputs retain their visible cursor; passwords remain masked. Port-validation errors appear inside the form, focus the invalid field and clear when edited.
- Search explains empty results and supports Ctrl+U. Help is grouped by task; Home/End and PageUp/PageDown work without scrolling into a blank view.
- The overview composition, dithered graphs, measured spikes, missing-data gaps and continuous content background are retained.

## Install or upgrade

Download the matching `.deb` and `SHA256SUMS` into an empty directory. Verify the file reports `OK`, then install:

```bash
sha256sum --check --ignore-missing SHA256SUMS
sudo apt install ./nexus-net_0.5.1_amd64.deb
nexus --version
nexus --doctor
nexus
```

Use `nexus-net_0.5.1_arm64.deb` for ARM64 Linux, including 64-bit Raspberry Pi OS Bookworm or newer. Confirm `dpkg --print-architecture` reports `arm64`. Both architectures also have standalone archives.

The Ubuntu 22.04 build baseline and GLIBC 2.35 ceiling are unchanged. Publication requires native build/install and terminal checks on Ubuntu 22.04 amd64/ARM64, followed by exact-package verification on Ubuntu 24.04 amd64 and Debian 12/13 ARM64.

65 Rust tests cover the application and interaction regressions. Screenshots use actual widgets with labeled example data. Physical Pi hardware, individual terminal compositors and screen readers remain outside automated coverage. Windows/macOS and 32-bit ARM binaries are not included.

[Installation instructions](https://github.com/vishnu-17o7/network-nexus#install-nexus-051) · [UI audit and evidence](https://github.com/vishnu-17o7/network-nexus/blob/main/docs/UI-AUDIT-0.5.1.md) · [Validation](https://github.com/vishnu-17o7/network-nexus/blob/main/docs/VALIDATION.md)
