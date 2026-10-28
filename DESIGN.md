---
name: Relayless
description: Direct, private, resumable large-file transfer between your own devices.
colors:
  bg: "#050506"
  elev: "#0B0B0D"
  raised: "#141417"
  surface: "#0B0B0D"
  surface-2: "rgba(255, 255, 255, 0.04)"
  surface-3: "rgba(255, 255, 255, 0.075)"
  hover: "rgba(255, 255, 255, 0.05)"
  border: "rgba(255, 255, 255, 0.10)"
  border-strong: "rgba(255, 255, 255, 0.24)"
  text: "#F2F2F2"
  text-2: "#A3A3A3"
  text-3: "#6A6A6A"
  accent: "#FF3B47"
  accent-strong: "#FF5A64"
  accent-ink: "#0A0A0A"
  accent-soft: "rgba(255, 59, 71, 0.10)"
  series-received: "#F2F2F2"
  series-sent: "#E5323F"
  magnitude-bar: "rgba(255, 255, 255, 0.55)"
  warn: "#FFB020"
  warn-soft: "rgba(255, 176, 32, 0.10)"
typography:
  page-title:
    fontFamily: "Oxanium Variable, Segoe UI, system-ui, sans-serif"
    fontSize: "17px"
    fontWeight: 600
    letterSpacing: "0.2em"
    textTransform: "uppercase"
  label:
    fontFamily: "Oxanium Variable, Segoe UI, system-ui, sans-serif"
    fontSize: "10px to 11px"
    fontWeight: 600
    letterSpacing: "0.16em to 0.18em"
    textTransform: "uppercase"
  control:
    fontFamily: "Oxanium Variable, Segoe UI, system-ui, sans-serif"
    fontSize: "11.5px (10.5px small, 12.5px large)"
    fontWeight: 700
    letterSpacing: "0.16em"
    textTransform: "uppercase"
  display-name:
    fontFamily: "Oxanium Variable, Segoe UI, system-ui, sans-serif"
    fontSize: "21px (16px on phone)"
    fontWeight: 600
  body:
    fontFamily: "Figtree Variable, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.45
  figure:
    fontFamily: "JetBrains Mono Variable, Cascadia Mono, Consolas, ui-monospace, monospace"
    fontSize: "0.94em inline; 15px in fact grids; 30px in the ring"
    fontWeight: 400
shape:
  radius: "0 everywhere except data dots"
  corner-marks: "2px L marks, 12px long on panels, 16px on dialogs; 1.5px by 6px on outlined buttons"
  hairline: "1px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "20px"
  page: "28px"
---

# Design System: Relayless

## Overview

**Creative North Star: "Signal HUD"**

Relayless looks like a quiet instrument panel: a black ground with a faint dot grid, white at three strengths, one signal red, 1px lines and bright corner marks. It borrows the vocabulary of sci-fi HUD screens (segmented rings, tick scales, equalizer bars, node-and-wire diagrams, corner brackets, hazard stripes) and keeps the one rule those screens usually break: **every mark shows real data.** Relayless exists to prove privacy, not claim it, so a gauge that moves for decoration would undercut the product. The Network globe's turning is the one decorative motion; its markers are the real public relay regions, and it never shows where any device is.

The screen stays calm at rest. Red appears only where something is happening right now or where the user can act. Hazard stripes appear only where the user is needed.

The project website (`website/`) still uses the previous "Quiet Ledger" look (green on glass). It should be moved to this system separately.

**Key characteristics**
- Black ground (`bg`) with a 24px dot grid; panels are opaque `surface` with a 1px hairline. No blur, no glass, no glows except one faint red wash behind the active transfer.
- Uppercase Oxanium for labels, controls and titles; Figtree for sentences and file names; JetBrains Mono for every figure, ID, path, code and safety word.
- Square geometry with corner marks: every panel keeps a 1px hairline border, and each of its four corners carries a brighter 2px L that sits on the hairline.
- One choreographed motion (calendar dots); packets moving along the link line while data flows; selection and hover marks that slide between items (300ms); everything else is state feedback under 200ms.

## Colors

### Signal
- **Signal Red** (accent): primary buttons (black label), the active nav item, the focus outline, links, today's numeral, the ring cursor, the arriving segment of a ring or progress bar, and "Stored on servers: 0 bytes".
- **Bright Signal Red** (accent-strong): primary button hover only.
- **Red Wash** (accent-soft): the active nav item fill and red tag backgrounds.

### Data series
- **Received White** (series-received): received bars, history dots and pair bars.
- **Sent Red** (series-sent): sent bars, dots and bars. Deeper than Signal Red so data does not read as a control. The pair differs in lightness as well as hue, so it survives colour-blind viewing and greyscale.

### Status
Status is carried by shape and pattern first, colour second:
- **Attention** is a solid red tag with black text, a black-on-red icon tile, and a 4px hazard strip on the left edge of the row or panel. It is the only solid red block that isn't a button, so it never reads like progress.
- **Active** (downloading, verifying) is a red outlined tag with red text.
- **Done / OK** is a white outlined tag.
- **Waiting** (connecting, reconnecting, waiting for sender) is a grey outlined tag.
- **Amber** (warn) is reserved for relayed routes and dropped connections: dashed amber route lines, amber mark labels, the "Partly relayed" chip.

### Neutral
- **Black Ground** (bg), **Panel** (surface, elev), **Raised** (raised).
- **Hairline / Strong Hairline** (border, border-strong): panel edges and dividers; outlined tags, secondary buttons and safety words.
- **White / Silver / Pewter** (text, text-2, text-3): values and names; sentences; labels, ticks and quiet metadata.
- **Magnitude White** (magnitude-bar): category bars that only show size.

### Named rules
**The Real Data Rule.** No gauge, bar, ring or number may move for decoration. If a HUD element can't be bound to something the engine knows, leave it out. (The globe's rotation is the documented exception; its markers are real relay regions, never devices.)

**The Red Means Now Rule.** Red marks what is happening now (arriving data, the live cursor, the active item) or where to act (primary buttons). Finished, verified data is white.

**The Hazard Is Rare Rule.** Hazard stripes mark only transfers that need the user. Never decorate with them.

**The Series Is Not Text Rule.** Series colours fill marks, bars and swatches only. Words and numbers beside them are text, text-2 or text-3.

## Typography

**Labels, controls and titles:** Oxanium Variable, uppercase, tracked 0.14em to 0.2em.
**Sentences and file names:** Figtree Variable, sentence case. Anything a user reads as prose stays here so it remains easy to read.
**Figures:** JetBrains Mono Variable. Every byte count, speed, percentage, count, time, path, fingerprint, code and safety word; the `.num` class switches inline figures to mono.

### Hierarchy
- **Page title** (Oxanium 600, 17px, 0.2em, uppercase; 15px on phone).
- **Section label** (Oxanium 600, 10.5px to 11px, 0.18em, uppercase, text-3): the only heading of its block.
- **Display name** (Oxanium 600, 21px; 16px on phone): the active transfer's file name.
- **Fact value** (mono, 15px; 12px on phone) under a section-label key.
- **Ring figure** (mono, 30px) with a smaller `%`.
- **Body** (Figtree 400, 14px; 15px on phone).
- **Control** (Oxanium 700, 11.5px, 0.16em, uppercase).
- **Tag** (Oxanium 700, 10px, 0.14em, uppercase).

## Layout

Desktop is a 224px sidebar and a main column with a 56 to 60px topbar, then content centred to 1120px with 28px between sections. Sidebar items are a 17px line icon (pewter, 1.75 stroke) and an uppercase label; the active item has a red icon, a red wash and a 2px red left edge.

The Overview opens with a two-column HUD row: the **active transfer** panel (about two thirds, with red corner marks) and the **Network** panel. With nothing in progress, the left column shows the Share and Connect tiles instead. Below 1080px the row stacks.

At 760px and below (the `usePhone` breakpoint) the sidebar becomes a bottom tab bar with icons and short labels; the active tab has a 2px red bar on its top edge. The active transfer shows in compact form on Phone Home, with the ring above its facts. Dialogs become bottom sheets with corner marks on the top corners only.

## Elevation and depth

There is no translucency. Depth comes from hairlines and, for floating layers only, a soft shadow (`0 18px 48px rgba(0,0,0,0.6)`): dialogs, toasts, tooltips and the selection bar. Toasts carry a 2px left edge in their tone (white, red or grey).

## Shapes

- **Radius is 0** on every control and container. Only data marks stay round (calendar dots and the history wedges).
- **Corner marks**: panels, stat tiles, hero tiles, side blocks, phone action tiles and dialogs are square with a 1px hairline, plus a 2px L at all four corners (12px arms in 78% white; 16px in full white on dialogs). Hovered tiles brighten their marks to full white; the phone Send tile's marks are red. Every outlined button (secondary, ghost, danger) gets small 1.5px by 6px marks; only the filled primary button stays plain.
- **Red corner marks** (16px arms) replace the white ones on the one panel that has focus on a screen: the active transfer. The selected calendar day gets small 8px red brackets.

## Components

### Sliding marks
Every group of choices (sidebar, phone tab bar, Transfers tabs, segmented controls, the More menu) draws one selection mark and one hover mark and slides them between items over 300ms (`cubic-bezier(0.2, 0.8, 0.2, 1)`), instead of each item lighting up on its own. Hover is a faint white block that follows a mouse pointer (touch doesn't show it). Selection is the group's own mark: red wash with a red left edge in the sidebar and More menu, a red underline under tabs, a white block in segmented controls, a short red bar on top of the phone tab. The marks jump without sliding on first appearance and on layout changes, and stop sliding under reduced motion. Add the `slides` class and `useSlide` (`src/slide.ts`) to any new group.

### Buttons
Square blocks with uppercase Oxanium labels. Primary: red fill, black label, brighter red on hover. Secondary: transparent with a strong hairline. Ghost: outlined like secondary but one step quieter (silver label, 55% white marks), brightening to white on hover. Danger: red hairline and label. Press nudges the button down 1px.

### Tags (status pills)
20px tall, square, outlined, uppercase. See Status above for tone.

### Progress bars
A row of 6px cells with 2px gaps. Filled cells are white; the leading cell of an active download is red. Sending rows use Sent Red cells.

### Chunk ring
The active transfer's progress: 96 segments around a circle, each standing for 1/96 of the file.
- White: that share of the file is verified on disk.
- Red: that share has arrived and is being verified (at least one segment while data is flowing).
- Dim: still to fetch.
Segments count amount, not position, because chunks arrive in parallel. An inner tick scale, an outer hairline, four cardinal ticks and a red cursor at 12 o'clock frame it. Paused dims the white segments to 50%; needs attention dims them to 30%. The centre shows the verified percentage, "VERIFIED" (or "CHECKING" during the final verification) and verified of total bytes.

### Link line
This device and the peer as two square nodes joined by a wire: solid white for a direct path, dashed amber with a diamond relay node for a relay, dotted grey for no path. While data flows, short red packets travel toward the receiver. Under reduced motion the packets hold still.

### Throughput graph
One bar per second over the last two minutes. Received bars rise above a baseline in white; sent bars hang below it in red; both use one scale, and each side gets height in proportion to its own peak. Events (paused, dropped, relay, direct) are dashed vertical lines with uppercase labels; dropped and relay are amber. Hover shows a crosshair and a tooltip with both speeds and any events at that second.

### Network panel
Online state, home relay, devices linked, "Stored on servers: 0 bytes" in red, and a slowly turning dotted Earth after cobe's "CDN" showcase ([cobe](https://github.com/shuding/cobe), WebGL, ~13 KB, MIT). Pewter-to-white land dots on a black sphere with a faint signal-red halo, inside an SVG tick ring. Each of iroh's four public relay regions (use1, usw1, euc1, aps1, from `iroh::defaults::prod`, placed at the metro its code names) carries a marker dot with a mono region tag just above it, on a dark fill with no outline; the home relay's dot and tag are red. Tags are positioned from cobe's marker anchors in script (no CSS anchor positioning needed), fade near the rim and behind the globe, and give way when they would overlap a tag nearer the centre. There are no arcs: relays don't send data to each other and Relayless never knows where a peer is, so any line between places would be invented. No device is ever placed on the globe. Offline, the halo and home marker turn grey and the land dims. It spins only while on screen, online and without reduced motion, at about 30 fps (one turn in about 40 s); without WebGL it falls back to an SVG wireframe globe.

### History calendar
Seven-column grid of square day cells with a faint hairline. Dot area is proportional to bytes moved that day; days too small to draw at scale are hollow rings, and the legend names the threshold. Under Both, a two-way day splits into a white received wedge from 12 o'clock and a red sent wedge, divided by a dark seam. Today's numeral is red with a short red underline. The selected day gets a white 1px ring and red corner brackets. Dots scale in on month or filter change (the one choreographed motion; off under reduced motion).

### Safety words
Five square mono chips, uppercase, with a strong hairline. Matching words on both devices mean nobody is in the middle.

### Receipts
Rows with a square icon tile, the file name and metadata, the size in mono, and a route chip. The open receipt is a key/value list with uppercase Oxanium keys.

## Do and don't

**Do**
- Bind every HUD element to real engine data, or leave it out.
- Keep red for "now" and "act here"; verified data is white.
- Use mono for every figure so live values don't jitter.
- Keep prose and file names in Figtree, sentence case.
- Respect reduced motion: packets stop, calendar dots don't animate.

**Don't**
- Add decorative gauges, fake readouts, scan lines or glitch effects.
- Use hazard stripes for anything but "needs attention".
- Show peer locations on the globe or anywhere else.
- Round corners on controls or panels.
- Put a series colour on text.
