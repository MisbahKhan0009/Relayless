---
name: Relayless
description: Direct, private, resumable large-file transfer between your own devices.
colors:
  bg: "#121212"
  elev: "#181818"
  raised: "#282828"
  surface: "rgba(255, 255, 255, 0.045)"
  surface-2: "rgba(255, 255, 255, 0.065)"
  surface-3: "rgba(255, 255, 255, 0.095)"
  hover: "rgba(255, 255, 255, 0.075)"
  border: "rgba(255, 255, 255, 0.08)"
  border-strong: "rgba(255, 255, 255, 0.15)"
  text: "#ffffff"
  text-2: "#b3b3b3"
  text-3: "#7c7c7c"
  accent: "#1ed760"
  accent-strong: "#3be477"
  accent-ink: "#000000"
  accent-soft: "rgba(30, 215, 96, 0.14)"
  series-received: "#16AE4D"
  series-sent: "#9277F5"
  magnitude-bar: "rgba(255, 255, 255, 0.58)"
  warn: "#ffc864"
  warn-soft: "rgba(255, 200, 100, 0.13)"
  err: "#f15e6c"
  err-soft: "rgba(241, 94, 108, 0.14)"
  info: "#7ab8ff"
  info-soft: "rgba(122, 184, 255, 0.13)"
  tooltip-bg: "#1e1e1e"
typography:
  page-title:
    fontFamily: "Figtree Variable, Segoe UI Variable Display, Segoe UI, system-ui, sans-serif"
    fontSize: "22px"
    fontWeight: 700
    letterSpacing: "-0.015em"
  month-title:
    fontFamily: "Figtree Variable, Segoe UI Variable Display, Segoe UI, system-ui, sans-serif"
    fontSize: "20px"
    fontWeight: 700
    letterSpacing: "-0.015em"
  figure:
    fontFamily: "Figtree Variable, Segoe UI Variable Display, Segoe UI, system-ui, sans-serif"
    fontSize: "26px"
    fontWeight: 750
    fontFeature: "tnum"
  modal-title:
    fontFamily: "Figtree Variable, Segoe UI Variable Display, Segoe UI, system-ui, sans-serif"
    fontSize: "17px"
    fontWeight: 700
    letterSpacing: "-0.015em"
  card-title:
    fontFamily: "Figtree Variable, Segoe UI Variable Display, Segoe UI, system-ui, sans-serif"
    fontSize: "15px"
    fontWeight: 700
    letterSpacing: "-0.015em"
  summary:
    fontFamily: "Figtree Variable, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.55
  body:
    fontFamily: "Figtree Variable, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.45
  control:
    fontFamily: "Figtree Variable, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 700
    letterSpacing: "0.005em"
  small:
    fontFamily: "Figtree Variable, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "12.5px"
    fontWeight: 400
  section-label:
    fontFamily: "Figtree Variable, Segoe UI Variable Display, Segoe UI, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 600
    letterSpacing: "0.06em"
  mono:
    fontFamily: "Cascadia Mono, Cascadia Code, Consolas, ui-monospace, monospace"
    fontSize: "12.5px"
    fontWeight: 400
rounded:
  day-phone: "8px"
  sm: "10px"
  float: "12px"
  lg: "18px"
  modal: "22px"
  pill: "999px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "20px"
  page: "28px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.accent-ink}"
    typography: "{typography.control}"
    rounded: "{rounded.pill}"
    padding: "0 14px"
    height: "34px"
  button-primary-hover:
    backgroundColor: "{colors.accent-strong}"
    textColor: "{colors.accent-ink}"
  button-secondary:
    backgroundColor: "transparent"
    textColor: "{colors.text}"
    typography: "{typography.control}"
    rounded: "{rounded.pill}"
    padding: "0 14px"
    height: "34px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.text-2}"
    typography: "{typography.control}"
    rounded: "{rounded.pill}"
    padding: "0 14px"
    height: "34px"
  button-ghost-hover:
    backgroundColor: "{colors.hover}"
    textColor: "{colors.text}"
  button-danger:
    backgroundColor: "transparent"
    textColor: "{colors.err}"
    rounded: "{rounded.pill}"
    padding: "0 14px"
    height: "34px"
  pill-ok:
    backgroundColor: "{colors.accent-soft}"
    textColor: "{colors.accent}"
    rounded: "{rounded.pill}"
    padding: "0 10px"
    height: "24px"
  pill-warn:
    backgroundColor: "{colors.warn-soft}"
    textColor: "{colors.warn}"
    rounded: "{rounded.pill}"
    padding: "0 10px"
    height: "24px"
  segmented-option:
    backgroundColor: "transparent"
    textColor: "{colors.text-2}"
    rounded: "{rounded.pill}"
    padding: "0 12px"
    height: "28px"
  segmented-option-on:
    backgroundColor: "{colors.text}"
    textColor: "{colors.accent-ink}"
  input:
    backgroundColor: "rgba(255, 255, 255, 0.07)"
    textColor: "{colors.text}"
    rounded: "{rounded.sm}"
    padding: "0 11px"
    height: "36px"
  panel:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.lg}"
  tooltip:
    backgroundColor: "{colors.tooltip-bg}"
    textColor: "{colors.text}"
    rounded: "{rounded.float}"
    padding: "11px 13px 12px"
    width: "252px"
  safety-word:
    backgroundColor: "{colors.surface-2}"
    textColor: "{colors.text}"
    rounded: "{rounded.pill}"
    padding: "3px 10px"
  calendar-day:
    backgroundColor: "rgba(255, 255, 255, 0.022)"
    textColor: "{colors.text-3}"
    rounded: "{rounded.sm}"
  calendar-day-selected:
    backgroundColor: "rgba(255, 255, 255, 0.1)"
    textColor: "{colors.text}"
---

# Design System: Relayless

## Overview

**Creative North Star: "The Quiet Ledger"**

Relayless is a near-black, glass-paneled instrument that records what moved between your own devices and proves it stayed private. Translucent panels sit over a static near-black ground lit by two soft, fixed glows (a green one top left, a faint blue one bottom right), so the blur has something to refract without anything moving behind it. One vivid green carries action, focus and "this is fine"; everything else is white at three strengths. The world is calm at rest and specific when asked: figures are tabular, receipts are plain key and value lists, and charts speak in two validated series colours that never leak into text.

Density is moderate desktop-app density (14px body, 34px controls, 28px page rhythm) that relaxes on the phone into a bottom tab bar, 52px topbar and stacked single column. Every press target is a capsule; every container is a softly rounded glass panel. The project website (`website/`) is a sibling surface of the same world: the same #121212 ground with green and blue glows, the same green, Figtree, and capsule buttons with the same press and hover scale, set at marketing scale (17px body, display headlines up to 84px).

**Key Characteristics:**
- Near-black ground (bg) with fixed radial glows; translucent glass panels with a 1px top highlight.
- One action green (accent); data series use their own deeper green and a violet.
- Figtree Variable throughout, bold (700) titles in sentence case, tabular numerals for every figure.
- Capsule (999px) buttons, pills, segmented controls and safety words; 18px panels; 12px floating tips.
- Depth by translucency and blur; drop shadows only on things that float.
- One choreographed motion moment (calendar dots); everything else is brief state feedback.

## Colors

A monochrome near-black and white system with one action green and a two-colour data pair, plus muted status tones.

### Primary
- **Signal Green** (accent): primary buttons, links, focus outline, the active nav icon, the `on` switch, today's numeral and underline, success pills, the short invitation code. Text on it is always black (accent-ink).
- **Bright Signal Green** (accent-strong): primary button hover only.
- **Green Wash** (accent-soft): the background of green pills, the success dot halo and green icon wells.

### Secondary
- **Received Green** (series-received): the received series in the live speed graph, history calendar dots, device-pair bars and legend swatches. Deeper than Signal Green so data never reads as a control.
- **Sent Violet** (series-sent): the sent series in the same places. The pair was validated for this dark surface as a two-series set; use them together.

### Tertiary
- **Amber** (warn) with **Amber Wash** (warn-soft): relayed routes, reconnect pins, warnings.
- **Coral** (err) with **Coral Wash** (err-soft): failures, danger buttons, alert counts.
- **Sky** (info) with **Sky Wash** (info-soft): informational pills and bars.

### Neutral
- **Near-Black Ground** (bg): the app and website background beneath the glows.
- **Elevated Ground** (elev): the opaque fallback for glass layers under reduced transparency.
- **Raised Ground** (raised): opaque raised fills where glass is not used.
- **Glass 1 / 2 / 3** (surface, surface-2, surface-3): panel fill, inset blocks and control wells, then selected or muted fills.
- **Hover Veil** (hover): row and ghost-button hover.
- **Hairline / Strong Hairline** (border, border-strong): panel edges and dividers; tooltip, safety-word and secondary-control edges.
- **White / Silver / Pewter** (text, text-2, text-3): primary text and values; body and descriptions; labels, axis ticks, weekday names and quiet metadata.
- **Magnitude White** (magnitude-bar): category bars that show size only.
- **Tooltip Ground** (tooltip-bg): the opaque body of chart and calendar tooltips.

### Named Rules
**The Series Is Not Text Rule.** Received Green and Sent Violet fill marks, lines, areas, wedges, bars and legend swatches only. Any words or numbers next to them are set in text, text-2 or text-3.

**The One Action Green Rule.** Signal Green means "act here" or "this is fine". Data uses Received Green, never the action hex, so a dot can never be mistaken for a button.

**The Neutral Magnitude Rule.** When only size matters (file categories), bars are Magnitude White. Hue is reserved for direction (received or sent) and status.

## Typography

**Display Font:** Figtree Variable (with Segoe UI Variable Display, Segoe UI, system-ui)
**Body Font:** Figtree Variable (with Segoe UI Variable Text, Segoe UI, system-ui)
**Label/Mono Font:** Cascadia Mono (with Cascadia Code, Consolas, ui-monospace) for fingerprints, codes and paths

**Character:** One friendly geometric sans at many weights; hierarchy comes from weight (400 body, 550 to 650 emphasis, 700 to 800 titles) more than from size jumps. The website loads the same family from Google Fonts at 400 to 900.

### Hierarchy
- **Page title** (700, 22px, -0.015em; 16px on phone): the topbar title, one per screen.
- **Figure** (750, 26px, tabular; 18px on phone): headline stat values.
- **Month title** (700, 20px, -0.015em): the calendar's month name between its previous and next buttons.
- **Modal title** (700, 17px): dialog headings.
- **Card title** (700, 15px): panel headings such as the live graph and the history side blocks.
- **Summary** (400, 16px, 1.55; 15px on phone, text-2 with bold values in text): the one plain sentence that sums a period.
- **Body** (400, 14px, 1.45): default UI text.
- **Control** (700, 14px, 0.005em; 13px small, 15px large): button labels.
- **Small** (400, 12.5px): hints, metadata, legend captions.
- **Section label** (600, 13px, 0.06em, uppercase, text-3): the single heading over a list section (Transfers, Devices, Receipts); the same treatment at 12px for table heads and stat keys.
- **Mono** (400, 12.5px): key fingerprints, codes, paths. The short invitation code is mono 26px, 0.12em, accent.

### Named Rules
**The Tabular Figures Rule.** Every byte count, speed, percentage, count and calendar numeral uses tabular numerals so columns and live values do not jitter.

**The Sentence Case Rule.** Titles and every label a user reads as words are sentence case. Uppercase belongs only to the small section label, table heads and stat keys, and a section label is always the only heading for its block, never stacked above another title.

## Layout

Desktop is a two-column shell: a 236px glass sidebar and a main column with a 60px glass topbar, then scrolling content padded 24px 28px 48px, centred to a 1120px page with 28px between sections. Rows inside panels pad 12px 16px with hairline dividers. Grids use auto-fit columns (300px hero tiles, 170px stats, 340px two-column blocks) at 14px gaps.

History splits the top into calendar (1.55fr) and side column (1fr) at a 20px gap. Below 1080px the calendar spans full width and the side column becomes two columns, with the summary sentence across both. At 760px and below (the phone breakpoint, also used by the `usePhone` hook) the sidebar becomes a fixed bottom tab bar of 54px items with short labels, the topbar is 52px, content padding is 14px, body text rises to 15px, page rhythm tightens to 20px, and everything stacks with the calendar first. Modals become bottom sheets with a handle.

The website uses a 1180px wrap with its own breakpoints at 980px and 640px.

## Elevation & Depth

Depth is translucency first. Resting surfaces (sidebar, topbar, panels, stat tiles, hero tiles, tables) are low-alpha white over the near-black ground, blurred with `blur(22px) saturate(150%)` and edged with a 1px inner top highlight; they cast no drop shadow. Only floating layers cast shadows, and they are softer and blurrier the more they float. Under `prefers-reduced-transparency` the sidebar, topbar, modal, selection bar and toast become opaque Elevated Ground with no blur.

### Shadow Vocabulary
- **Glass edge** (`box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.07)`): every resting glass panel and the active nav item.
- **Tooltip lift** (`box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5)`; calendar tip `0 14px 36px rgba(0, 0, 0, 0.55)`): chart and calendar tooltips on Tooltip Ground.
- **Floating layer** (`box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.06), 0 18px 48px rgba(0, 0, 0, 0.45)`): modals and toasts, which also use stronger blur (28px and 24px, saturate 160%).

### Named Rules
**The Shadows Float Rule.** A drop shadow means the element hovers above the page (tooltip, modal, toast). Panels at rest are glass with an edge highlight, never shadowed cards.

## Shapes

Two shape families. Anything you press is a capsule (999px): buttons, icon buttons, pills, the segmented control and its options, the search field, the selection bar, safety words, the switch. Anything that holds content is a softly rounded glass rectangle: panels and side blocks at 18px, modals at 22px, tooltips, icon wells and the safety-word block at 12px, inputs, nav items, receipt icons and calendar day cells at 10px (8px on phone). Data marks are circles (calendar dots, live-graph points, status dots) and legend swatches are 9px squares with 3px corners, or 14 by 3px strokes for line series.

**The Capsule Control Rule.** If it responds to a press, it is fully rounded; if it holds content, it has a gentle corner. Do not mix the two on one element.

## Components

### Buttons
Confident capsules that tip slightly toward you.
- **Shape:** capsule (pill), 34px tall with 14px side padding; small 28px, large 42px. On phone, medium grows to 42px and small to 36px for touch.
- **Primary:** Signal Green fill, black label, bold control type.
- **Hover / Focus:** primary brightens to accent-strong and scales to 1.03; every button presses to 0.97; transitions run 120ms. Focus is a 2px Signal Green outline at 2px offset.
- **Secondary:** transparent with a 30% white hairline that turns full white on hover.
- **Ghost:** transparent, text-2, hover veil and white text on hover.
- **Danger:** transparent with a coral hairline and coral label; coral wash on hover.
- **Icon button:** 30px circle (40px on phone), text-2, glass fill on hover.

### Chips
- **Status pills:** 24px capsules, 12px semibold label with a 12 to 13px icon, tinted wash background and matching tone text (ok, warn, err, info, muted).
- **Route chip (receipts):** an outlined capsule with a green Direct or amber Relayed icon; the text stays text-2. On phone only the icon shows.

### Cards / Containers
- **Corner Style:** 18px.
- **Background:** Glass 1 with backdrop blur; nested blocks (job details, notices, sub-lists) use a 3.5% white inset.
- **Shadow Strategy:** glass edge only (see Elevation & Depth).
- **Border:** 1px hairline.
- **Internal Padding:** 14px to 18px; rows 12px 16px.

### Inputs / Fields
- **Style:** 36px tall, 10px corners, 7% white fill, transparent border. Search is a capsule.
- **Focus:** border turns 55% white and the fill deepens to 9%; search shows a 2px 55% white ring.
- **Switch:** 36 by 20px capsule, 22% white off, Signal Green on with a black knob.
- **Segmented control:** capsule well at 6% white; the selected option is a solid white capsule with black text.

### Navigation
Sidebar items are 40px, 10px corners, semibold text-2; hover gets the veil and white text; active gets a 9% white fill, glass edge, white text and a green icon. Counts are tabular text-3, or a black-on-green capsule for active work and a dark-on-coral capsule for alerts. On phone the sidebar is a bottom bar of equal-width 54px items with icon over a 10.5px label.

### Safety Words
Five capsules in a wrapping row: 13px semibold white text, 3px 10px padding, Glass 2 fill, strong hairline. A compact variant (12px, 550) sits inline in receipts and lists; the full set sits in a 12px-cornered inset block when comparing devices.

### Live Speed Graph
A glass panel with a 15px bold title and a right-aligned legend (14 by 3px series strokes, values in bold white, a dimmed series at 45% opacity). Received and sent are 2px round-joined lines over 10% area fills in their series colours; hairline grid at 7% white, tick labels 11px text-3 tabular. Hover shows a 40% white crosshair, 4px series points with a dark stroke, and a tooltip on Tooltip Ground. Events (pause, reconnect, relay, direct) are 18px circular pins on the top edge, amber for reconnect or relay, green for direct.

### History Calendar
A seven-column month grid whose day cells carry one dot each.
- **Dot area:** dot area is proportional to bytes moved that day (diameter scales with the square root of the share of the month's busiest day). Maximum diameter is 40px on desktop and 30px on phone; minimum drawn mark is 7px.
- **Direction:** under Both, a day that moved data in both directions is split into a received wedge (from 12 o'clock clockwise) and a sent wedge, divided by a thin dark seam. Under Received or Sent, and for one-way days, the dot is solid in the dominant series colour.
- **Hollow ring:** a day below the drawable minimum is a hollow ring (1.75px stroke in the series colour), not a filled dot. The threshold is the size at which a true-to-scale disc would fall under the 7px minimum, snapped up to the next round size (1 MB, 10 MB, 100 MB, 250 MB, 500 MB, 1 GB, 2 GB, 5 GB and so on), and the legend names it ("under 1.0 GB"). Because the maximum dot differs, the threshold differs by device: the same month can read "under 1.0 GB" on desktop and "under 2.0 GB" on phone.
- **Legend:** below a hairline, "Dot area shows how much moved", then the hollow-ring threshold, two or three round reference sizes drawn at true scale, and under Both a split sample captioned "received and sent".
- **Cells:** 10px corners (8px on phone) on a 2.2% white wash; days with nothing moved are disabled; numerals are 12px tabular text-3 top left, text-2 on active days, 40% opacity in the future.
- **Selected day:** a 1.5px inset white ring, 10% white fill and a bold (750) white numeral.
- **Today:** a bold (750) Signal Green numeral with a 12 by 2px green underline beneath it.
- **Tooltip:** on hover, keyboard focus or tap, a 252px tooltip with the date, total and count, the received and sent split under Both, the three largest files and the peers.
- **Motion:** dots scale in from 0.2 with a fade on month or filter change, 320ms `cubic-bezier(0.16, 1, 0.3, 1)`, staggered 9ms per cell in reading order. Hovering an active day scales its dot to 1.08. Under reduced motion the entrance and its stagger do not play.

**The Area Not Radius Rule.** A day mark's area, not its diameter, is proportional to what moved, so twice the bytes is twice the ink.

**The Named Threshold Rule.** Anything too small to draw at scale is drawn as a hollow ring, and the legend states the round size below which rings appear. A tiny day never borrows a filled dot.

**The One Moment Rule.** The calendar dot entrance is the system's single choreographed motion. Everything else is state feedback of 120 to 200ms (hover, press, sheet and toast rise at 160ms) or progress.

### Receipts
Rows of a 34px icon well (10px corners), name and metadata, right-aligned size, the route chip and a chevron that rotates on open. An open receipt is a key and value list (112px key column of 12.5px semibold text-3; values 13.5px text-2), with green check icons beside verified facts and a 4px route meter (green direct share over amber). On phone the list collapses to one column.

## Do's and Don'ts

### Do:
- **Do** set every word and number in text, text-2 or text-3, including labels beside series swatches.
- **Do** pair Received Green and Sent Violet as a set wherever direction is shown (graph, dots, pair bars, legends).
- **Do** use Magnitude White bars when only size matters.
- **Do** make every pressable control a capsule (999px) and every container a gently cornered glass panel (18px panels, 12px floating tips).
- **Do** use tabular numerals for every figure.
- **Do** scale data marks by area and name the hollow-ring threshold as a round size in the legend.
- **Do** keep drop shadows for floating layers and give glass a reduced-transparency opaque fallback (elev).
- **Do** mark the selected calendar day with a 1.5px white ring and a bold numeral, and today with a green numeral and short green underline.

### Don't:
- **Don't** use Signal Green as a data colour or a series colour as text.
- **Don't** encode file categories or other size-only data with hue.
- **Don't** draw a below-threshold day as a filled dot, or scale dots by diameter.
- **Don't** add a second choreographed animation; new motion is state feedback under 200ms and stops under reduced motion.
- **Don't** put a small uppercase label above another heading; the section label is the block's only heading.
- **Don't** give resting panels drop shadows or square corners.
