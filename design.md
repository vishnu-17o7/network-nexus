<!-- Hallmark · pre-emit critique: P4 H4 E4 S5 R4 V4 · native terminal workbench -->
# NEXUS interface system

Audience: people investigating a Linux host or Raspberry Pi over a local terminal or SSH. The interface must help them see the current state, select evidence, and choose a bounded next action.

## Direction and boundaries

The overview layout is retained. Secondary pages become a network workbench: section navigation, a short state summary, a purpose-built list, and visible details for the selection. Diagnostics use a finding/evidence layout; monitoring gives charts priority; settings groups actual controls alongside saved profiles. Services expose connection and polling state. No decorative dashboard cards, gradients, glass, or invented activity.

This is an in-place Rust/Ratatui redesign, informed by Nutlope/Hallmark's redesign discipline and the ordered-dither chart treatment in Boring Software's Dither Kit. Web-specific CSS, browser typography and marketing-page rules do not apply. The terminal owns the monospace font; keyboard, SSH and small screens are first-class.

## Locked tokens

`ui::Theme` is the single source for background, panel, foreground, muted, border, accent, good, warn, bad and violet. Existing dark, OLED, Catppuccin, Tokyo Night, Gruvbox, light and custom themes remain available. Muted Tokyo Night text and Gruvbox/light status colors meet 4.5:1 against their panel; selected table rows use foreground text for readability. All body gutters use `panel`, including chart pixels. Selection uses `border`; accent identifies the active page and primary action. Warnings and errors include words, never color alone.

Spacing is measured in terminal cells: one-cell list padding, two-cell panel gaps, one blank line between detail groups. Surfaces use a single top rule; full boxes are reserved for modal boundaries. Titles are short and bold, labels muted, values normal, actions accent. No entrance animation is added to the redesign.

## Page families

| Family | Pages | Main task |
| --- | --- | --- |
| Overview | Overview | Read local state and recent measurements |
| Investigation | Diagnostics | Choose a test, inspect findings and next steps |
| Inspection | Interfaces, Wi-Fi, DNS, connections, ports, routes, neighbors | Scan important identity/state columns, read all selected fields |
| Monitoring | Bandwidth, latency, events, test history | Read trends, compare measurements, inspect the selected record |
| Services | Tailscale, Pi-hole | Connect or refresh, inspect health and peers/queries |
| Settings | Preferences/profiles, capabilities | See current controls and available optional backends |

## Interaction

The five main tabs remain. Secondary navigation exposes sibling pages; comma/period moves within a section and mouse clicks select visible tabs. Existing Tab, number shortcuts, command palette, filtering, sorting, copy and Enter detail inspection remain. A visible `a` action is specific to each page. All network changes and external requests continue through the existing preview/consent paths. A diagnostic result can return to the test catalog without discarding saved history.

At wide sizes, inspection uses a list plus detail pane. At narrower sizes, the selected detail stacks below; very short terminals prioritize the list and retain Enter for the full record. Column priority is explicit per page; endpoints and identities receive the most space. Empty states explain what has not been observed and provide an actual action. Filtered-empty states point to Esc.

## Dithered charts

Clear shape-preserving traces sit above an ordered Bayer dot fill. The fill is anchored to the plotting grid, contains no random noise or animation, and stops at sample gaps. Both Kitty pixel graphics and portable Braille renderers use the same bounded fill sampler. Unmeasured values stay unmeasured; no interpolation crosses missing replies or stale samples. SVG previews contain the same dither treatment.

## Review evidence

Baseline: the secondary pages used equal-width generic columns, concealed important endpoint/state fields, and devoted large areas to empty tables. Profiles exposed saved records but hid actual app settings. Diagnostics mixed configuration actions with tests. Services lacked a useful first-run view. The approved overview and continuous body background are retained.

The preview generator uses the real widgets and explicit documentation fixtures with reserved example addresses. It covers all 17 pages at wide, compact and tall sizes, plus empty states, filtering, text charts and light mode. These are rendered application buffers, not live network measurements or pictures of physical terminal hardware.

Final critique and validation are recorded after render review in `docs/VALIDATION.md`.
