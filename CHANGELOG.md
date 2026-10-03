# Changelog

## 0.3.0

- Replace block traffic bars and latency dots with Braille line plots, labeled axes, separate RX/TX, automatic rate units and explicit missing-reply gaps.
- Reflow charts, navigation, tables, diagnostics and scrollable help from 40×12 through wide/tall terminals.
- Add Tailscale peers/health, exit-node awareness, explicit path/NAT checks and confirmed DNS/exit-node controls.
- Add Pi-hole v6 native session API, aggregate statistics/activity, visible-page polling, masked memory-only credentials and confirmed 60-second pause/resume.
- Add authenticated local API fixtures, redirect/secret/session/race tests and expanded terminal-size checks.

## 0.2.0

- Make troubleshooting findings, evidence and next steps central to the overview.
- Distinguish HTTP 404/5xx, system DNS failure, TLS verification failure and failed internet probes.
- Add native DNS-over-TLS queries with strict CA/hostname validation and TCP/TLS/DNS error separation.
- Adopt Tokscale-inspired cyan tabs, focused panels, 30 FPS rendering and animated task progress.
- Add amd64 Debian packaging, binary archives, checksums and GitHub CI/release workflows.
- Verify local HTTP 404, authenticated DoT, SERVFAIL, untrusted CA, hostname mismatch and UI responsiveness during blocked I/O.
