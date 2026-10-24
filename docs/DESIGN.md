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
- Window title is "<screen> — Libri" (for example "Movies — Libri", "Halloween 2026 — Libri"); the title bar is KDE's own, painted in the theme's chrome with the title in the secondary text color (section 10, Title bar).
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

- The same thirteen categories in the same order: Genres, Game genres, Music genres, Mood, Season, Languages, Authors, Directors, Actors, Studios, Streaming, Animation style, Other (settled with the phone on 2026-10-24; the phone's DESIGN.md already listed Animation style, so nothing moved).
- The sidebar's TAGS section lists tags only, grouped like the picker when "All tags" opens. Headers are gray caps, tags use the text color. A tag in several categories is listed under each; a click filters by it as that category.
- One tag picker (`ui::tag_picker`, state in `tags`) serves the Filter popover's "All tags…", the entry form's Tags and the plan dialog's linked hashtags: groups fold (those holding a picked tag start open; the plan dialog opens Season), a group of more than 24 tags is one row that drills into the full list inside the dialog (search, letter headers, a letter rail, a check per row), and a tag in several categories is a chip under each group with its own tick. The entry form's picker adds a right-click menu on every chip (the desktop's long press) and "+ Add to Directors" rows while searching.
- A plan links a hashtag by name, whatever its categories; the Filter popover's quick chips and the sidebar's tags pick by name too, which matches any role on an entry.

## 6. Keyboard and mouse

- Esc closes the topmost popover or dialog.
- Enter confirms a dialog's main action.
- Ctrl+K (or Ctrl+F) focuses search; Ctrl+N opens Add entry.
- Tab moves focus; focus is always visible.
- The mouse's Back and Forward buttons (and Alt+Left, Alt+Right) move through the places visited, as in a browser.
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
- **Default library view** (2026-10-18, phase 1a): the app opens on All, which is the overview titled "Library" (see below), not on one type.
- **Sidebar tags** (2026-10-18, phase 1b): pinned tags by default, as in the mockup; with nothing pinned the section says "Nothing pinned yet. Pin tags from All tags below." The most-used mode lists the six most used, ties A to Z. Pins are app state until phase 2 saves display preferences.
- **Popovers** (2026-10-18, phase 1a): iced 0.14 has no anchored popover, so `ui::popover` is a small custom widget (iced's `advanced` feature). It opens below its anchor, or above when there is no room, aligned to the anchor's left or right edge; a click outside it closes it. Dialogs use iced's own stack with a scrim over the whole window.
- **Keyboard focus** (2026-10-18, phase 1a): in iced 0.14 only text fields can take keyboard focus. Tab and Shift+Tab move between text fields; buttons, chips and rows got a focusable wrapper with a visible focus ring in the 1g keyboard pass (below).
- **Dashed borders** (2026-10-18, phase 1a): iced 0.14 draws only solid borders, so the mockup's dashed "+ Tag" and "All tags…" chips have a solid line border.
- **Library filters follow the phone** (2026-10-18, phase 1b): every selected tag must be on an entry; the decade chips come from the library, earliest to latest with the gaps filled and "Older" for anything before 1960, and several can be picked. The mockup had "any selected tag" and a fixed decade list; its revision of 2026-10-19 follows the phone too. The rating range, the seasonal-only switch and the status chips are the mockup's.
- **Search** (2026-10-18, phase 1b): the toolbar field narrows the list as you type, over titles and tags (a leading "#" is ignored), and finds seasonal-only entries too, as the phone's search does. Ctrl+K and Ctrl+F focus it.
- **Empty states** (2026-10-18, phase 1b), not drawn in the mockup, with the phone's wording: an empty type ("No movies yet", with Add), no search results ("No results for "x"", with Clear search), filters that match nothing ("Nothing matches these filters" with the spelled-out reason and Clear filters, so the chips row drops its Clear all), and a status nothing has ("Nothing marked done yet").
- **Status words** (2026-10-18, phase 1b): in progress reads Watching for shows and anime, Reading for books, Playing for games; done reads Saved for music. From the mockup.
- **Grid** (2026-10-18, phase 1b): cells at most 190 px wide with 20 px gaps, so the 1600 px window fits seven covers across as the mockup does; the covers are colored placeholders picked from the entry id (the mockup's `coverOf`) until phase 4 fetches real ones.
- **All is an overview** (2026-10-19, phase 1b, canvas revision of 2026-10-19): with no type, filter, status or search, All shows "now and next" instead of a list, titled "Library". A search counts as narrowing it too (the mockup does not say). In progress and Recently added (the past month) are newest added first, Top rated is 9 and 10, newest added first; seasonal-only entries are left out of all three. Coming soon is every wanted entry not out yet by the phone's release rule, at any precision ("2027-10" counts, where the mockup only listed full dates), soonest first. Each "See all" opens the full list: In progress as the status, Top rated as the rating range 9 to 10, and the other two as the "Not released yet" and "Added in the past month" chips.
- **The active plan** (2026-10-19, phase 1b): the first started plan with something not done. "Next up" is its next two items not done, dated ones by date first, then the rest in plan order; "n days left" counts to the plan's last date and is left out without dates. Today comes from the system clock in UTC.
- **Several types** (2026-10-19, phase 1b): two or more picked types count as a filter (on the Filter badge, as chips, cleared by Reset and Clear all); one picked type is where the library is and stays. A status chip shows when any picked type can have that status. The title joins the types: "Movies + Games".
- **Grouped results** (2026-10-19, phase 1b): All with something narrowing it, and several types, are grouped by type in type order, each headed by the type in its color with the count. The thin type rail replaces the scrollbar: one segment per group sized by its count, a click jumps to the group, and an outline marks the view. The current group's header pins to the top once its own has scrolled away. Both rest on `ui::Groups`, a column that reports where each group starts. In the text view the column header sits above the scrolling rows.
- **Type checkboxes** (2026-10-19, phase 1b): off by default; when on, each sidebar type has a checkbox in its color that adds or removes it, and the name shows only that type. The switch is in Settings (1f).
- **New tags** (2026-10-19, phase 1c): the tag picker's search can create a tag. A near miss shows inline as "Did you mean #horror?", as in the mockup, instead of the phone's separate D9 dialog. Creating always asks to confirm and pick the category (the phone's D10), starting on Game genres for a game, Music genres for music and Genres otherwise; the mockup's "goes under Other until you file it" is not used, since the phone files every tag when it is made. A new tag reaches the library only when the entry is saved. Tag names follow the phone's rule (lowercase ASCII, digits, hyphens, a dot between digits) and its similar-tag check.
- **The details pane** (2026-10-19, phase 1c): status and rating change at once. The note and the tags are edited in the form, so the note box and "+ Tag" open Edit ("+ Tag" with the tag picker open). "+ Add to plan" under In plans opens the phone's Add to plan (A7): one row per plan, a click adds or removes at once, and an entry already done is added as a repeat. E edits the selected entry.
- **Add and Edit** (2026-10-19, phase 1c): one dialog. Add has the lookup on the left (the arrow keys move, Enter picks; phase 1 searches a fixed catalog in `lookup.rs`); Edit has the form alone until "Look up again" (phase 4). A picked match fills every field: tags the library has go on, new ones are offered under "Suggested" (marked "new"; no lookup provider is named, as on the phone). Only the title is needed, and a release must read 2026, 2026-12 or 2026-12-25. Ctrl+Enter saves. A new entry's id follows the phone's rule; changing the type clears the lookup id; several platforms are saved as "PC, PlayStation 5".
- **Delete** (2026-10-19, phase 1c): the phone's confirmation (A9), naming the plans the entry leaves. Tags that no entry or plan uses any more are dropped after an edit or a delete, as on the phone.
- **Music** (2026-10-19, phase 1d): the mockup has no music boards, so music follows the phone (its section 5) in the desktop's look. With Music picked, the chips row starts with an Artists | Albums | Songs switch (Artists first), then the status chips. Artists are A to Z with a leading "The" ignored and Various Artists last; each shows what is saved ("2 albums, 3 songs") with a disc of their initials until photos come in phase 4, in all three views. A click opens the artist's page (a back arrow before the title): their albums, then their songs as "Album · Year". The Songs tab mixes song entries and rated album tracks as "Artist · Year"; a track opens its album. The counts on an artist's page are the artist's. Seasonal-only music stays hidden, as in every main list.
- **Album tracks** (2026-10-19, phase 1d): an album's details list its tracks, each with a small rating ("9/10" or "Rate") that opens the 1 to 10 picker; rated tracks the list lacks follow it. Only rated tracks are stored. Phase 1 has track lists for five of the fixture's albums; without one the pane says "No track list was found for this album." and shows the rated tracks.
- **Replace dialogs** (2026-10-19, phase 1d): the phone's D12 and D13, on adding or saving an album whose songs are saved, or a song whose album is saved; "Replace songs with album" moves the songs' ratings into the album's tracks. Keep both is outlined and Cancel plain text, since the neutral fill is the dialog's own color in the dark themes.
- **Popovers under dialogs** (2026-10-19, phase 1d): popovers always draw on top in iced, so opening any dialog closes them all.
- **Plan dates** (2026-10-19, phase 1e): `library.json` has no date range for a plan, so the mockup's Date range field is left out of the plan dialog. A plan's dates ("Oct 1–31") span its items' Whens, and the plan page and its card show them; without dated items the card says what the plan holds instead. Each item's When is set in the details pane with a month calendar: a click picks a day, a later second click makes it a period, Clear removes it.
- **Plans pages** (2026-10-19, phase 1e): Active and Upcoming plans are cards, Finished ones rows, then a card per seasonal collection (opened through the newest plan with that season's tag). Inside a seasonal plan the season's color is the accent and the ink. The View popover's Layout (checklist or timeline) is stored on the plan, so the phone shows it too; Sections (cards on Light, plain on the dark themes by default) and Entries (with or without covers) are kept per plan on this device. The timeline groups and orders items by the phone's rules.
- **The plan's side pane** (2026-10-19, phase 1e): "Add to this plan" lists entries with the plan's tags not in it yet, or wanted entries for a plan without tags; the filter searches the whole library. An entry added while done is a repeat ("Rewatch"). Selecting an entry shows its details with a "This plan" section (When on a timeline plan, and the plan's own note for it), and Delete there opens the phone's D4 (remove from this plan or delete everywhere).
- **New year** (2026-10-19, phase 1e): "New year…" in the year menu opens New plan filled in for the year after, copying the plan, so only the items not done carry over, without their dates. A new plan's id is the phone's slug of name and year, made unique.
- **Sidebar on Plans** (2026-10-19, phase 1e): as in the mockup, the Plans header has a + for a new plan, the open plan's row is highlighted, each bar is in its plan's accent, and the Tags section is left out (tags filter the library). Finished plans are listed only under All plans, on both screens.

- **Settings** (2026-10-20, phase 1f): as the mockup, with the sections measured by `ui::Groups` so the "On this page" rail scrolls to them and marks the one scrolled to (the last at the end of the page). The preferences live in `settings::Settings` (theme, background style, accent, timeline default, seasonal touches, default platform); the sidebar's tag mode and type checkboxes stay in the library state that draws them. Everything is app state until phase 2 saves it. Sync now does nothing until phase 3. Disconnect repository asks first (Cancel / Disconnect), then forgets the repository, so Library source shows "Not connected" with Set up; phase 3 clears the wallet too.
- **System theme** (2026-10-20, phase 1f): `ThemeChoice::System` resolves to Graphite or Light by a `plasma_dark` flag on the app, faked by `LIBRI_PLASMA=light`. Reading the portal needs a D-Bus crate or a `busctl` call, decided in phase 2 (TODO).
- **Flat background** (2026-10-20, phase 1f): `Tokens::styled(theme, accent, flat)`; Graphite flat uses #151515 for the field too, so the window is one shade. The content pane's rounded corner stays and simply vanishes in flat.
- **Seasonal touches off** (2026-10-20, phase 1f): `plans::shown_season` gates every seasonal look (banner, accent, ink, marker, card art, title icon) on the setting; the plan's own Seasonal theme switch is kept as it is.
- **Default platform** (2026-10-20, phase 1f): the phone's rule. A game filled in from a lookup starts with the default platform picked when the game came out on it, and with every fetched platform otherwise; "None" turns this off.
- **Setup** (2026-10-20, phase 1f): fixture branches and files from the mockup; a file that is not a Libri library is listed dimmed and cannot be picked; the new file's name must end in .json. The mockup's "Checking access…" and "Creating…" spinners wait for the real calls in phase 3, so Connect and Create library go straight on. Open Libri records the repo, branch, file and a masked token hint as the app's source. "Replace token" (from the warning, the sync row and Settings) opens Connect with the repository kept and the token empty; Connect or Back returns to the screen it came from, and Connect marks the sync as fine for now. The window is titled "Set up Libri" on the four numbered steps only. "How do I create a token?" opens GitHub's guide in the browser through `xdg-open`.
- **Splash** (2026-10-20, phase 1f): a first run (no source) says "Getting things ready…" with the version; otherwise "Loading your library…" with repo, branch and file. The bar's 72 px segment sweeps across and back, eased, on a 16 ms tick that runs only while the splash shows.
- **Token warning** (2026-10-20, phase 1f): `SyncState::ExpiresIn(days)` (from `LIBRI_SYNC=expires-in-6` until phase 3) shows the dialog once at start when days ≤ 7, titled "Your GitHub token expires in 6 days" with the date as today plus the days. It sits over every other dialog; Esc is Later, Enter is Replace token; Open GitHub opens the token settings page. Expired: the sync row's red dot, "Token expired" and the Replace token link, and Library source's card turns red with a Replace token button.
- **Light theme pass** (2026-10-20, phase 1f): every screen captured in Light; the only fix was the expired sync row, which wrapped. The captures now include the splash's first run, every setup step, the token warning and the expired row, Settings at full height and Settings flat.

- **Keyboard pass** (2026-10-20, phase 1g): `ui::Focusable` wraps every button, chip, row, card, swatch and switch (through the `ui` helpers and `FocusExt::focusable`). Tab and Shift+Tab reach them through iced's focus operations in tree order, Enter and Space press the focused one (a synthetic click at its center, so no message is doubled), and a 2 px ring in the accent ink shows where focus is once it moved by keyboard; a click moves focus silently, so Tab goes on from there without a ring and Enter does not press the clicked control again. Segments of a joined group draw the ring inside their bounds. `ui::Barrier` sits under a modal dialog: focus operations and keys do not reach the screen behind the scrim, so Tab and Enter stay in the dialog. Ctrl+N opens Add entry on Library and Plans. The arrow keys move the lookup's pick in the entry form, otherwise the selection through the library's list in the order shown (one type in its sort, All and several types by type first, Music by its tab; the artist lists have no selection) and through the plan's checklist (by type, then the plan's order) or timeline; the ends stop, and the list does not scroll to follow yet.
- **Long lists** (2026-10-20, phase 1g): the phone's rule. A list of one type in the text or list view that is at least three screens tall (its rows against the viewport `ui::Groups` reports) and splits into two sections or more gets section headers in gray caps with counts and a jump rail of short labels down the right edge, the section being scrolled through marked; a click jumps there. Sections follow the sort (`browse::sections`): decades for the year orders with Older before 1960 and undated entries last as "Not released yet", months of this year and then years for newest added, first letters for the title order with "#" for digits, the ratings down to "Unrated" (the rail says "None"). The artist list sections by first letter with a leading "The" ignored and Various Artists on its own at the end. Grouped results keep the type rail instead, and the grid has neither. `LIBRI_LONG_LISTS=1` forces the headers and rail on every list, since the fixture is short.
- **Window sizes** (2026-10-20, phase 1g): the minimum window is 1280 × 720, the logical size of a 1080p screen at KDE's 150 % scaling; captures at 1280 × 720, 1366 × 768, 1536 × 864 (1080p at 125 %), 1706 × 960 (1440p at 150 %), 1920 × 1080, 2048 × 1152 (1440p at 125 %) and 2560 × 1440 are in `screenshots/sizes/` (`cargo test --test screenshots -- --ignored`). Neither pane collapses or overlays; the details pane keeps its 352 px, the sidebar its 232 px, and the content gives way instead. The library toolbar compacts by the content's width: under 1000 px the search shrinks and the Sort button shows the order alone, under 820 px Filter and Add become icons and Sort a word. The plan page's header puts the scope tabs, View and Edit plan on a row of their own under 1000 px. On a 2560 px window the overview keeps the mockup's seven covers per shelf and the side column grows. The app learns its size from iced's resize events.
- **Contrast** (2026-10-20, phase 1g): a test checks every theme, two-tone and flat, against WCAG 2: body and secondary text reach 4.5 on the content and the chrome, every accent ink and type color reaches 3 (they are used in SemiBold or larger), and the text on every accent fill reaches 4.5. The lowest are Movies' gold on Light (3.5 on the chrome, 3.8 on the content, kept on purpose, section 8) and Amber's and Beige's light inks on the chrome (4.2 and 4.3).
- **Mockup walkthrough** (2026-10-20, phase 1g): every board was compared with its capture. The app differs from the mockup only where a decision above says so (letter spacing, dashed borders, the plan's Date range, the type order), plus: the plan page's "+ Plan-only entry (not in library)" is left out, since `library.json` plan items reference library entries and the phone has no plan-only items; the mockup's "Mark done" is only an accessibility label; the toolbar and the plan header compact on small windows, which the mockup (drawn at 1600 px only) does not show; the throwaway token sheet is gone.

- **Back and Forward** (2026-10-21, after phase 1g): `history::History` keeps the places visited, as a browser does; a place is the screen, what the library shows (types, status, filter, sort, search, music tab, artist page) and the plan page, never the selection, a popover or a scroll position. A move to a new place (a screen, a sidebar type or tag, "See all", a music tab, an artist page, a plan page or collection) records the place left and drops the places ahead; a pick that changes nothing records nothing. Back and Forward come from the mouse's side buttons (iced's `mouse::Button::Back` and `Forward`, which winit maps from BTN_SIDE and BTN_EXTRA on Wayland and buttons 8 and 9 on X11) and from Alt+Left and Alt+Right, KDE's shortcuts in Dolphin and the browsers. Returning to a place restores it with the selection and the popovers closed; the pins and the views stay. Nothing happens over a dialog or the token warning, so no form is lost; on the setup Back is the wizard's own Back and Forward does nothing; the splash and the setup are passed through, never visited. The history holds 100 places and is not saved.
- **Title bar** (2026-10-22, after phase 1g): KWin draws the title bar, so Libri cannot paint it; the mockup draws it in the chrome. On Plasma Wayland KWin takes a color scheme per window through the `org_kde_kwin_server_decoration_palette` protocol (what Konsole does per profile), so `decoration` ships one KDE color scheme per theme and style in `assets/color-schemes/` (chrome as the bar, secondary text as the title, halfway to the chrome when inactive) and names it once the window is open and on every theme change. Each file also sets the window colors (chrome and text): Klassy keeps the colors it derived until the window's application palette changes, and the title bar entries are not part of a Qt palette, so with the title bar entries alone the first scheme stuck and a theme switch at runtime was ignored. Only a decoration that follows color schemes shows it: Klassy (set up with the macOS-style buttons of the mockup), Breeze or Darkly; an Aurorae SVG theme, which is what was in use, paints its own fixed colors. Drawing the bar in iced (`decorations: false`) was rejected: it loses KWin's shadow, corners, double-click, window menu and tiling, and the buttons would match no other window. A KWin window rule was rejected because it pins one scheme and cannot follow the theme.

- **Tags in more than one category** (2026-10-24, phase 1h): the phone's rules (its DESIGN.md section 7) as built here. The right-click menu opens under the chip, anchored like the other popovers, not at the pointer; Categories… and Move to another group… open the same dialog as the phone's (the categories of the kind as chips, at least one picked, with the usage line; the one-category groups for a move). The changes are applied with `SetTagCategories` right before the entry is saved, as on the phone, so Cancel changes nothing. Roles show after the name in the form's Tags box (one chip per role), the inspector (merged), and the active filter chips (one per role); the toolbar search narrows the list over titles and tag names, so the phone's one-row-per-role search results have no counterpart here. The long group's letter rail scrolls the list from the rows' fixed heights, so nothing is measured. `LIBRI_LONG_LISTS=1` makes any group of four or more tags a long group, since the fixture has no group of 25. Languages and Streaming are in the picker, the new-tag dialog and the Filter popover; the lookup's suggestions for them come with the real providers in phase 4. The fixture is the phone's copy again, byte for byte, which brought `pages`, `series`, the anime `kind` and a season's `manual` flag into the model and codec ahead of phase 1i (no UI for them yet; the inspector shows a show's first season's date when the show has none of its own).

- **Seasons and series** (2026-10-24, phase 1i): the phone's rules (its DESIGN.md sections 4 and 8) as built here. All seasons, a season's episodes and a series open as pages in the content pane, in place of the list and with the inspector kept (`screens/library/page.rs`); a page is a place the history remembers, and it closes with the back button or any change of what the library shows. A season's status pill opens a popover menu (the phone's sheet) with the catch-up button under the radio rows, in the inspector, on the pages and in the form's season editor (`ui::season_pill`). A plan tick marks a show's seasons that are out Done and remembers what they were in app memory (`plans::State::before_done`), as on the phone; the untick puts them back while the show is still Done. The series' parts and a season's episodes come from `lookup::series_parts` and `lookup::episodes`, fixed for the fixture until phase 4 fetches them; the parts are matched to the library's entries by lookup id, then number, then title. A series as one row needs a long list: for the grid, which has no sections, a row of cards counts as four entries against the same three-screens rule. The dropped-season dialog has no daily check yet and opens with `LIBRI_DROPPED=<entry id>:<season>`. No source is named anywhere in the UI (the phone's "Episode details from TMDB." and "TMDB has dropped it" are left out, owner's decision of 2026-10-24): the sources are listed once, in Settings › About (phase 1k). A show's id takes Season 1's year. The phone's hidden series parts and "Couldn't update this series" belong with the real lookups in phase 4.

- **The lookup pane** (2026-10-24, phase 1j): the phone's rules (its DESIGN.md section 4, Add / edit entry, and 10c) as built here, on the fixture catalog in `lookup.rs`. "Look up again" is a link in the Title label of Edit; it opens the left pane on the entry's title in pick mode, and a pick replaces the creator, the date, the description, the kind and the platforms, keeps the title, note, status, tags and formats, sets `source`, offers the match's tags and shows the "Details fetched online" card. Pick a cover opens from the cover itself once there is a match; it lists the match's own cover and the other matches for the title (books with their editions), and a pick is stored as `cover_from`, cleared by "Use the match's cover" or a type change. A game lookup carries the "Platform · Any" chip with the picker (Any, Your platforms from Settings and the library's games, All platforms A to Z, a search over the other names); a picked platform narrows the matches, reads "On Super Nintendo" in the head and "Super Nintendo and 3 more" in the rows, and is picked in the form's Platforms with the match. The pane's states (looking up, offline, not set up, partial, partial with nothing found, no matches) come from `LookupState`, faked with `LIBRI_LOOKUP` until phase 4; the texts are the phone's, with no source named. Suggestions carry a role: the cast as new actors ("new · Actor"), a person the library has under another people category with the role alone, the language as a Languages tag (never English, never Japanese on an anime), and a movie's platform as a Streaming tag; a show's network goes on only when the library has its tag, never as a new one. "Year from the ebook edition. Check if it's a reprint." shows under Released while adding a book dated by its ebook edition, until the date is changed; never in Edit.

- **Sync states, Disconnect and About** (2026-10-24, phase 1k): the phone's rules (its DESIGN.md sections 10 and 10b) as built here, on fake states until phase 3. The sidebar row always tells the state (Synced, Saving 2 changes…, Offline, Could not save, Library not found, Sync conflict, Token expired, or Not connected with the library kept on this computer), with the action beside it; the states that need the owner (Could not save with Retry, Library not found with Fix, Sync conflict with Choose) also get a banner over the content on Library and Plans, inside the content pane above the toolbar; Offline and Saving fix themselves and stay in the row. "Keep which copy?" opens over everything from the row, the banner and Library source; Keep mine is the neutral fill, Keep GitHub's the destructive style, Decide later closes and keeps the conflict. Disconnect is two steps (confirm, then Keep it on this computer or Delete it from this computer); keeping puts Libri in local mode, where Library source reads "Not connected · Your library is kept on this computer" with Connect to GitHub, and connecting an existing file then asks "Which library?" in the setup (this computer's uploaded over the file, or GitHub's replacing it); a new file is made from the kept library without asking. About shows the icon, the version with the build (the commit, set by `build.rs`), an Open-source licenses dialog, and the data sources, the only place a provider is named: TMDB with its logo and required sentence, IGDB, Apple Books, Open Library, Google Books, Deezer with its logo in the light or dark variant, AniList and Wikidata, each opening its website. The Apple Books badge near book covers stays an open question with the phone.
- **Raster images in captures** (2026-10-24, phase 1k): iced 0.14 decodes PNGs on a worker thread and asks the shell for a redraw when one is ready; the headless shell of `iced_test` ignores that, so a single snapshot never shows a raster image (SVGs load synchronously and do show). The screenshot test renders Settings twice with a short wait between, so the About logos appear. Any screen that gains raster images (covers in phase 2) needs the same.
- **Presentation mode** (2026-10-24, after phase 1k): Libri presents with wgpu's mailbox mode, not iced's FIFO default (`app::run` sets `ICED_PRESENT_MODE=mailbox` unless the environment already names a mode or a backend). Cause: on KWin 6.8 with NVIDIA's 615 driver, the desktop froze for minutes whenever Libri's virtual desktop was switched back to, for as long as the window had been hidden (22:11:38 to 22:16:45 after 22:06:24 to 22:11:34 hidden, traced on 2026-10-24). With FIFO, NVIDIA's Vulkan window-system code attaches a `wp_commit_timing_v1` target to every frame and, after an idle gap, extrapolates one as far in the future as the gap was long; KWin holds the frame until that time and its render loop stops with it (KDE bug 527500, which KDE closed as an NVIDIA driver bug; KWin merge request 10217 of the same day turns commit timing off by default for Plasma 6.8.x). With mailbox the driver sends no timestamps and no fifo barriers, verified with `WAYLAND_DEBUG=1`. Mailbox shows the newest frame at the next refresh and never tears, and Libri redraws only on input, so nothing spins. Explicit sync was not the cause: the freeze happened with `__NV_DISABLE_EXPLICIT_SYNC=1`. The desktop-wide workaround, which also covers mpv and any other Vulkan client, is `KWIN_WAYLAND_SUPPORT_COMMIT_TIMING_V1=0` in KWin's environment until the KWin update lands. During a freeze, ending the client does not help, since KWin keeps the scheduled time on its own (tried at 22:25:12, KWin stayed idle), but a virtual-terminal switch does: Ctrl+Alt+F3 and then Ctrl+Alt+F1 back to Plasma made KWin reacquire the display and paint again at once (22:28:24).

## 11. Open

Nothing is open at the end of phase 1.
