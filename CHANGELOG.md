# Changelog

## 0.4.2

- Remove dark background seams between metric cards, charts and findings by painting one continuous main content surface.
- Match service summaries and empty states to the same surface across all six themes and custom colors.
- Keep text charts and pixel graphics on the same background; preserve header/footer separation and selected-row highlights.
- Add background regression checks across all pages, compact/tall layouts, empty/populated charts and both renderers.
- Generate real widget/chart preview artifacts in CI and refresh documentation screenshots, including the unmeasured text-renderer view.
- Publish refreshed Ubuntu 22.04-compatible amd64 and Raspberry Pi ARM64 packages and archives.

## 0.4.1

- Build release packages in an Ubuntu 22.04 container so they can run with glibc 2.35; keep the GitHub runner independent of that compatibility baseline.
- Fail packaging if a release binary requires a newer GLIBC symbol version than the declared baseline.
- Build, test and verify actual Debian package installation in Ubuntu 22.04 and 24.04 CI containers.
- Use each build container's own Python and a tomli fallback on Python 3.10, avoiding an incompatible host-toolcache Python.
- Add native ARM64 Debian packages and archives for 64-bit Raspberry Pi OS and other compatible ARM64 Linux systems.
- Verify the exact ARM64 release packages and terminal behavior in Debian 12 and 13 before publishing both architectures together.
- Add Raspberry Pi install steps, architecture checks and explicit 32-bit/hardware testing limits; document Windows/macOS native backend work.

## 0.4.0

- Render antialiased, shape-preserving vector curves through Kitty terminal graphics, with a portable curved Braille fallback. Curves retain every measured point and leave gaps for failed replies and pauses.
- Redesign the dashboard around live metric cards, larger charts, compact findings and network context; refine dark/light colors and compact/tall layouts.
- Add Space to freeze graphs while collection continues, and [ / ] for 1/5/15-minute traffic windows and 30/90/300-probe latency windows.
- Add traffic peak, visible-window p95/packet loss, rounded axes and stale-snapshot indication.
- Cache unchanged chart images, resize to terminal cell dimensions, remove overlays before popups and restore the terminal on exit.
- Add PNG/SVG chart assets to headless renders and regression coverage for curve bounds, data gaps, freeze, graphics transport, modal cleanup and resize.

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
