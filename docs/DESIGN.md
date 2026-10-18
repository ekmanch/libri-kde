# Libri desktop: settled design

Everything here was decided while designing the desktop mockup (October 2026). Screens refer to the desktop mockup `design/canvas.html` (Splash, Setup, Library, Plans, Settings); open it in a browser to click through them, with the design notes beside each screen.

This file covers only what is specific to the desktop app. Everything about the data, tags and sync is shared with the phone app and lives in `ekmanch/library-app` → `docs/DESIGN.md` (called "the phone's DESIGN.md" below). When the two disagree on shared behavior, the phone's file wins and this one gets fixed.

Items in section 11 ("Open") are not decided yet; decide them in the phase the TODO names and move them into section 10.

## 1. What the app is

- A desktop version of Libri for Linux, built in Rust with iced, for KDE Plasma (Wayland) on CachyOS. Single user, no installer beyond a local package.
- The same library as the phone: `library.json` in `ekmanch/library`, read and written through the GitHub API. Both apps write the same bytes for the same library (the canonical encoding in the phone's DESIGN.md section 8).
- The same six types, statuses, ratings, tags and plans as the phone. The word in the UI is "Type".
- American spelling, sentence case.

## 2. Shared with the phone (not repeated here)

| Topic | Where |
|---|---|
| Words and rules: statuses per type, rating 1 to 10, Note vs About, row content, type order | Phone's DESIGN.md section 2 |
| Music: artists, albums, songs, track ratings, "Various Artists" last, replace dialogs | Section 5 |
| Tags: categories and their order, A-to-Z sorting, similar-tag check, creating a tag only from the entry form, creator and actor suggestions | Section 7 |
| `library.json` v2 and the canonical encoding | Section 8 |
| Sync: cached first, conditional reads, bundled commits with readable messages, three-way comparison, conflicts, offline queue | Sections 9 and 10b |
| Lookups and covers | Sections 10 and 10c |

## 3. Window and layout

- One main window, 1600 × 900 by default. The mockup is drawn at that size.
- Window title is "<screen> — Libri" (for example "Movies — Libri", "Halloween 2026 — Libri"); the title bar is KDE's own.
- Three panes:
  - **Sidebar** on the window's background (chrome). Sections: LIBRARY (All plus the six types, with counts and type-color dots), PLANS (each plan's name with its year in gray and a progress bar, then "All plans" after the plans), TAGS (the most-used tags with counts, then "All tags"). The sync row and the Settings gear sit at the bottom.
  - **Content** on a slightly lighter surface with a rounded top-left corner, so it reads as a sheet laid on the chrome.
  - **Inspector** on the right: shows only while an entry is selected, and holds the entry details (see 4.3).
- Section headers everywhere (sidebar, menus, popovers, the inspector) are small gray caps: 12.5 px, weight 700, uppercase, slight letter spacing, in the secondary text color. Selectable items under them (entries, tags, chips) use the normal text color. This was compared against colored headers and chosen on purpose.
- The repository and branch show quietly in the content footer ("ekmanch/library · main") in mono.

## 4. Screens

### 4.1 Splash

- Shown while Libri starts: logo, "Libri" wordmark, an indeterminate bar in the accent color, a status line. No buttons.
- First run says "Getting things ready…" and shows the version in the footer. A returning user sees "Loading your library…" and the repo, branch and file in the footer.

### 4.2 First-time setup

- The whole window is the setup, not a card floating in an empty window: the steps rail sits where the sidebar normally is, the form in the content pane (max 680 px wide), Back and the main action pinned at the bottom.
- Steps: Welcome → Connect GitHub (repository and fine-grained token; the token is stored in KDE Wallet) → Branch (default branch first and preselected) → Library file (create new or pick an existing Libri JSON) → Create / Pick → All set.
- Files that are not in Libri's format are listed but cannot be picked. Creating never overwrites an existing file. Everything can be changed later in Settings → Library source.
- "Replace token" (from the token warning, 4.6) opens the Connect GitHub step with the repository kept.

### 4.3 Library

- **Toolbar:** type title with count, Search (Ctrl+K hint), Filter, Sort, View (grid, list with covers, text), Add.
- **View mode** is remembered per type.
- **Sort:** Newest added, Title A to Z, Year newest first, Year oldest first, Rating highest first (same five as the phone). The button shows a short label ("Title A–Z", "Year ↓", "Rating ↓").
- **Filter popover:** Decade chips, Tags chips with "All tags…", Rating range 1 to 10 (two ends, picked by tapping, never a slider), "Include seasonal-only" switch, Reset, and a live result count.
- **Chips row** under the title: status chips (All, Want, In progress, Done, DNF as the type allows, with counts), then the active filters as removable chips (tags, decade, seasonal, rating), then an inline "Clear all" right after the last chip, in the accent ink. Not at the far right edge: it belongs to the chips.
- **Selection:** a gray fill with no border (the selection gray in section 8), never the accent. In the grid, unselected covers dim to 60% while one is selected.
- **Inspector (entry details):** cover, type label in the type color, title, creator, released, Status as a segmented control, Rating as a button that opens the 1-to-10 picker, Tags with "+ Tag", Note (my text), About (from the lookup), In plans (links), then Edit and Delete at the bottom. The same details are used from Plans.
- **Add entry dialog** (opens over the library from Add): online lookup on the left (several matches to pick from), the form on the right with every fetched field editable. The form scrolls between a fixed header and a fixed bottom bar (Cancel, Add); nothing scrolls under the bottom bar.
- **Tag picker** (from the Add dialog, the Filter popover and "All tags"): a modal with search across every group, collapsible groups in the phone's category order, a "n selected" count on folded groups, multi-select.
- **Delete buttons** have no fill, the gray line border and red text, never a red fill (section 10 has the reds).

### 4.4 Plans

- **All plans** is the Plans home: Active, Upcoming, Finished, plus Seasonal collections. Plans have a name and an optional year, and the year shows next to the name everywhere.
- **Plan screen:** breadcrumb, title with a year dropdown (other years of the same plan, then "New year…" in the accent ink, which copies the plan), status badge, date range, "This plan / All #tag" tabs, View, Edit plan, a progress bar with "4 of 9 done" and "Linked to #halloween". The checklist is grouped by type in two columns. An "Add to this plan" panel on the right lists entries with the plan's tags that are not in it yet, plus "+ Plan-only entry (not in library)".
- **View popover** per plan: checklist or timeline, plain or card sections, text or covers.
- **Seasonal themes:** Halloween and Christmas plans get an illustrated banner strip above the header (nothing overlaps it), a seasonal title icon (the phone's pumpkin for Halloween), a seasonal progress marker and accent. The banners are SVG in the mockup and are reused as-is. Seasonal touches can be turned off in Settings.
- **New / Edit plan dialog:** Name, Year, Date range (with a calendar), Started (edit only), Linked hashtags, Seasonal theme, "Start with: Empty / Copy a plan" (new only), Timeline, and Delete plan (outlined, red text).
- **Linked hashtags** open a full tag view inside the dialog (drill-in, with Done and Clear), not a list that scrolls inside the dialog's own scroll. Season comes first and is the only group open; the other groups follow the phone's category order.

### 4.5 Settings

- A sidebar of its own sections ("On this page"), changes apply right away.
- **Appearance:**
  - **Theme**, one list of five (decided 2026-10-18, replacing a Dark/Light switch plus a separate "Dark background" choice). There are four themes; System picks one of them:

    | Theme | Behavior |
    |---|---|
    | System (default) | Graphite when Plasma's color scheme is dark, Light when it is light; switches live when Plasma switches |
    | Graphite | Always Graphite |
    | Charcoal | Always Charcoal |
    | Catppuccin Mocha | Always Mocha, and the type colors switch to the Catppuccin set (section 8) |
    | Light | Always light |

    Picking a named theme is an explicit override and ignores Plasma. Plasma's light or dark preference is read from the desktop portal (`org.freedesktop.appearance` `color-scheme`), including changes while Libri runs. With no portal answer, System means Graphite.
  - **Background style:** two-tone (the default) or flat, a separate setting that applies to every theme.
  - **Sidebar tags:** pinned or most used.
  - **Accent color:** a Vivid group with Peach as the default, and a Catppuccin group. On the phone the second group is called "Pastel", since most Android users won't know Catppuccin; on a KDE desktop the name says what it is.
- **Library source:** repository, branch, library file, access token (Replace), Sync now, Disconnect repository.
- **Plans:** timeline by default, seasonal touches.
- **Games:** default platform.
- **About:** name and version.

### 4.6 Token expiry

- GitHub sends a `GitHub-Authentication-Token-Expiration` header on API responses made with a token that has an expiry date. Libri saves it from normal sync calls; no extra request.
- From 7 days before expiry, a dialog shows once a day, the first time Libri opens that day: "Your GitHub token expires in 6 days", with Later, Open GitHub and Replace token.
  - Later closes it until the next day.
  - Open GitHub opens the token settings page in the browser.
  - Replace token opens the Connect GitHub step of setup with the repository kept.
- A token without an expiry never sends the header, so nothing shows.
- Once expired, GitHub answers 401 and the sidebar sync row says "Token expired" with a "Replace token" link.

## 5. Tags on the desktop

The rules are the phone's (section 7). Desktop-specific:

- The same eleven categories in the same order: Genres, Game genres, Music genres, Mood, Season, Authors, Directors, Actors, Studios, Animation style, Other.
- The sidebar's TAGS section lists tags only, grouped like the picker when "All tags" opens. Headers are gray caps, tags use the text color.
- Groups in the picker, the Filter popover and the plan dialog are collapsible. Only the plan dialog opens with one group (Season) expanded.

## 6. Keyboard and mouse

- Esc closes the topmost popover or dialog.
- Enter confirms a dialog's main action.
- Ctrl+K (or Ctrl+F) focuses search; Ctrl+N opens Add entry.
- Tab moves focus; focus is always visible.
- Rating is always click → picker, never a slider or a drag.

## 7. Fonts

- **Figtree** for all text: Regular 400 (body), Italic 400 (subtitles, notes), Medium 500, SemiBold 600 (labels, buttons), Bold 700 (section headers in small caps).
- **Headings** (screen titles, plan names, dialog titles, the wordmark, cover titles) are Figtree ExtraBold 800. The phone uses -0.03em letter spacing; iced 0.14 cannot, so the desktop uses default tracking (section 10).
- **JetBrains Mono** Regular and Medium for years, counts, dates, the window title and the repo footer.
- The font files are bundled in the app (the same six Figtree files as the phone, plus the two JetBrains Mono files), with their OFL licenses.

## 8. Visual design

Exact values live in the `theme` module once it exists; these are the mockup's.

**Surfaces (dark)**

| Token | Graphite | Charcoal | Catppuccin Mocha |
|---|---|---|---|
| chrome (sidebar, window) | #131416 | #151515 | #181825 |
| content | #1D1F22 | #1C1C1C | #1E1E2E |
| raised (popovers, chips) | #2A2D31 | #272727 | #2B2C3E |
| field | #1D1F22 | #202020 | #232334 |
| line | #3A3E44 | #2E2E2E | #313244 |
| text | #EEF0F2 | #EDEDED | #CDD6F4 |
| secondary text | #A3A8AE | #A3A3A3 | #A6ADC8 |

Flat style uses one shade for chrome and content (Graphite flat: #151515 everywhere, raised #252525, line #2A2A2A; Charcoal flat: #151515, raised #242424; Mocha flat: #181825, raised #2A2B3C).

**Surfaces (light):** content #FFFFFF, raised #E9EBEE, field #FFFFFF, line #D5D9DE, text #17181B, secondary text #5B616B; chrome #F4F5F7 when two-tone, #FFFFFF when flat.

**Selection gray:** the content color mixed 14% toward white in dark themes; #E9EBEE in light.

**Type colors** (order Movies, Shows, Books, Games, Anime, Music as listed in the mockup). There is no separate type-color setting: the Libri set is used with Graphite and Charcoal, the Catppuccin dark set with Catppuccin Mocha, and the Libri light inks with Light. The Catppuccin light inks are kept for a possible Catppuccin Latte theme later.

| Set | Movies | Shows | Books | Games | Anime | Music |
|---|---|---|---|---|---|---|
| Libri, dark | #F2B544 | #7FB7FF | #8FD694 | #FF8F7A | #FF8CC1 | #C4A7FF |
| Libri, light ink | #A87C00 | #0074DC | #01872C | #D63A1F | #C8157E | #7C3AED |
| Catppuccin, dark | #F9E2AF | #89B4FA | #A6E3A1 | #FAB387 | #F5C2E7 | #CBA6F7 |
| Catppuccin, light ink | #B7791F | #1E66F5 | #40A02B | #D9530A | #D6409F | #8839EF |

**Accent:** chosen in Settings. Fills use the accent itself in both themes; on the light theme, accent text, icons and outlines use a darker ink.

| Vivid | Fill | Light ink |
|---|---|---|
| Peach (default) | #F8B289 | #C2521B |
| Violet | #A78BFA | #7C5CE0 |
| Hot pink | #FF7AB6 | #D6337F |
| Amber | #F2B544 | #A36706 |
| Green | #5EE08A | #15803D |
| Blue | #5AA9FF | #1D6FE0 |

The Catppuccin group (Beige, Rosewater, Flamingo, Pink, Red, Mauve, Lavender, Blue, Sky, Green) has the same fill/ink pairs as `Settings` in the mockup.

With the Catppuccin Mocha theme, the default Peach becomes Catppuccin's own peach, #FAB387, for both fill and ink; other accents are unchanged.

**Seasonal accents:** Halloween orange and Christmas green, only inside a themed plan.

**Sizes from the mockup:** body text 13 to 14 px, secondary text 12 to 12.5 px, section headers 12.5 px caps, type title 26 px, plan title 30 px. Common radii: 8 px (buttons, fields, chips), 12 px (dialogs, window), 14 to 15 px (pills).

## 9. Tried and rejected

- **Bricolage Grotesque** for headings: dropped on 2026-10-18 to match the phone.
- **DejaVu Sans with DejaVu Sans Mono**, to match the Plasma desktop font: tried in a side-by-side spike. With its lowercase letters scaled to Figtree's height it is still about 10% wider in bold, has no Medium or SemiBold, and did not look enough better to justify that.
- **A setup card in the middle of an empty window:** replaced by the full-window setup (4.2).
- **Colored section headers:** small gray caps were preferred (section 3).
- **Theme Dark/Light plus a "Dark background" choice:** two settings for one decision, and the second did nothing in Light. Replaced by one Theme list with System as the default (4.5).
- **A separate type-color palette setting:** dropped; Catppuccin Mocha brings its own type colors (section 8).
- **A list of linked hashtags that scrolls inside the plan dialog:** replaced by the drill-in tag view (4.4), so there is never a scroll inside a scroll.

## 10. Decisions made while building

Add decisions here as they are made during phases 0 and 1, with the date.

- **Letter spacing** (2026-10-18, phase 0c): iced 0.14 has no letter-spacing setting on text (checked in `iced_core`, `iced_widget` and `iced_graphics`). Headings use default tracking instead of -0.03em, and the small-caps section headers default tracking instead of the slight spacing in section 3.
- **Minimum window size** (2026-10-18, phase 0b): 1100 × 680 for now, so the window cannot collapse while phase 1 is built. The real value is decided in 1g (section 11).
- **Icons** (2026-10-18, phase 0d): the mockup's icons are exported as 24 × 24 SVGs to `assets/icons/` and tinted with the theme color. The Halloween ghost and Christmas star timeline markers keep their own colors.
- **Type order** (2026-10-18, phase 1a): the phone's global order everywhere, Movies, Shows, Books, Games, Music, Anime. The mockup's Anime-before-Music is incidental.
- **Delete button style** (2026-10-18, phase 1a): every destructive button (Delete, Delete plan, Disconnect repository) has no fill, the gray line border and red text, as on the phone; the canvas revision of 2026-10-18 unified a gray-filled Delete plan to this. The red is brighter than before, which read as pink: #F0525F on the dark themes, #D7263D on Light.
- **Default library view** (2026-10-18, phase 1a): the app opens on All ("Everything" in the title), not on one type.
- **Sidebar tags** (2026-10-18, phase 1a): the six most used tags, most used first and ties A to Z, until pinning from the All tags popover is built in 1b.
- **Popovers** (2026-10-18, phase 1a): iced 0.14 has no anchored popover, so `ui::popover` is a small custom widget (iced's `advanced` feature). It opens below its anchor, or above when there is no room, aligned to the anchor's left or right edge; a click outside it closes it. Dialogs use iced's own stack with a scrim over the whole window.
- **Keyboard focus** (2026-10-18, phase 1a): in iced 0.14 only text fields can take keyboard focus. Tab and Shift+Tab move between text fields; buttons, chips and rows get a focusable wrapper with a visible focus ring in the 1g keyboard pass.
- **Dashed borders** (2026-10-18, phase 1a): iced 0.14 draws only solid borders, so the mockup's dashed "+ Tag" and "All tags…" chips have a solid line border.

## 11. Open

- **Minimum window size** (1100 × 680 for now, section 10) and how the three panes give way below 1600 px wide: does the inspector overlay the content, or the sidebar collapse? (phase 1g)
