---
version: 1
slug: "src-pages-history-tsx"
primary_target: "src/pages/History.tsx"
related_targets: ["src/pages/Overview.tsx"]
---

# History screen (Relayless app)

Scope: new History page in the desktop/Android app, plus the live flow graph (Overview, Transfers), privacy receipts, safety words and privacy settings that feed it. Mode: Operate. Extends the incumbent glass dark world; no DESIGN.md change.

Audience and job: small teams and labs who move large files between paired devices. First question the page answers: how much moved, with whom, and when. Second: prove each transfer stayed private.

Constraints: history is local only; retention setting (default 1 year, off clears it); names can be dropped. No invented figures in the shipped UI; mock data only in `npm run dev:ui`.

## Direction contract

THESIS: A month you can read at a glance. The user pinned a month calendar where each day carries a dot sized by how much moved, with details on hover. Refuses the default stat-tile row and the year-long contribution grid.

OWN-WORLD: Incumbent world: near-black #121212 glass panels, Figtree, pill controls, green #1ED760 as the received series, a validated violet as sent, neutral bars for category magnitude, text in text tokens only.

STORY: The visitor sees this month's dots, reads one plain sentence of what moved and how privately, taps a busy day, and sees who it was with, what kinds of files, and each transfer's privacy receipt.

FIRST VIEWPORT: Topbar title History. Left two thirds: month calendar panel, month name with previous/next and a Both / Received / Sent segmented control on the same header row (it sits on the calendar it filters, which leads per FORM), seven-column grid, area-scaled dots (received/sent wedges under Both, hollow ring for days too small to draw at scale), tooltip on hover/focus/tap. Right third: the month sentence, then With whom (stacked in/out bar per device pair, legend), then What (category bars). Receipts list below, full width. Phone: calendar first, the rest stacked.

FORM: Pinned by the user (month calendar with size-scaled dots and hover details), fused with dealt structure 5 (calendar leads and filters everything below). seed:f34e6963e9074877

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
