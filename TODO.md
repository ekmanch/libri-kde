# TODO

Build order for Libri desktop (Rust + iced, KDE Plasma on CachyOS). Screens refer to `design/canvas.html` (Splash, Setup, Library, Plans, Settings, with the design notes under each). Desktop decisions are in docs/DESIGN.md. The data model, tag rules and sync decisions are shared with the phone app and live in ekmanch/library-app → docs/DESIGN.md; this app follows them rather than redefining them.

Phase 1 builds the whole UI on fixture data, with nothing saved and no network. Functionality is wired up from phase 2.

## Phase 0: Project setup

### 0a. Toolchain and tools
- [x] System Rust from the repos (`rust` 1.99, includes cargo, rustfmt and clippy). No rustup
- [x] `rust-src` and `rust-analyzer` installed with Shelly
- [ ] rust-analyzer hooked up in the editor, format on save with rustfmt
- [x] System pieces iced needs on Plasma Wayland: `vulkan-icd-loader`, `libxkbcommon`, `wayland`, `fontconfig` (already present). `vulkaninfo` sees the RTX 4070 Super (NVIDIA proprietary) and the 7800X3D iGPU (RADV)
- [x] Confirm iced renders on the NVIDIA GPU, not the iGPU (iced installs no logger, so checked with `nvidia-smi`, which lists the running `libri`; `WGPU_ADAPTER_NAME` if that ever changes)
- [x] Confirm the software fallback (tiny-skia) also starts: `ICED_BACKEND=tiny-skia cargo run`
- [ ] Optional: `cargo install bacon` for rebuild-on-save, then `fish_add_path ~/.cargo/bin` (the folder only exists after the first `cargo install`)
- [x] Optional: `.githooks/pre-commit` running `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`, enabled with `git config core.hooksPath .githooks`

### 0b. Project skeleton
- [x] Binary crate `libri` (edition 2024) at the root of the `ekmanch/libri-kde` repo (`cargo init --name libri`), next to `assets/`, `design/` and the docs; `cargo run` builds and prints "Hello, world!"
- [x] `iced` 0.14.0 added with `svg`, `image` and `tokio` (defaults bring wgpu, tiny-skia, wayland, x11); `advanced` only if a custom widget needs it
- [x] Module layout: `app` (state, `Message`, `update`, `view`), `theme` (tokens), `ui` (shared widgets), `screens` (splash, setup, library, plans, settings), `model`, `repo`, `fixtures`
- [x] Main window: 1600×900 default, a sensible minimum size (1100×680 for now, decide in 1g), app id `libri` so KDE groups the window and finds its icon later
- [x] `CLAUDE.md`: stack (system Rust 1.99, iced 0.14, no rustup), module layout, the "never commit unless asked, suggest a commit message" rule, American spelling in UI text, Shelly for system packages, how to run and test
- [x] `design/canvas.html` in the repo as the visual reference; `README.md` with build and run commands
- [x] `.gitignore` ignores `target/`; `Cargo.lock` stays committed

### 0c. Fonts and theme
- [x] Typography: Figtree only, headings in ExtraBold.
- [x] Bundle the fonts in `assets/fonts/` and load them at startup with `include_bytes!` + `.font(...)`: Figtree Regular, Italic, Medium, SemiBold, Bold, ExtraBold (same six files as the phone app) and JetBrains Mono Regular and Medium (years, counts, dates); add both OFL license files
- [x] Check whether iced 0.14 supports letter spacing; the phone's headings use -0.03em. If not, note it in DESIGN.md and accept default tracking (it does not; noted in `docs/DESIGN.md`)
- [x] `theme` module from the mockup tokens: the four themes Graphite, Charcoal, Catppuccin Mocha and Light; the six type colors (Libri dark, Catppuccin with Mocha, light inks); accent palette (Vivid and Catppuccin) with Peach as the default; selection gray; small gray caps for section headers (12.5 px, 700, uppercase)
- [x] A throwaway "token sheet" view (`LIBRI_SCREEN=tokens cargo run`) that shows every color, text style and button, to eyeball against the mockup; remove at the end of phase 1

### 0d. Data on fixtures
- [x] `model`: Entry, Track, Season, Plan, PlanItem, tag categories (Genres, Game genres, Music genres, Mood, Season, Authors, Directors, Actors, Studios, Animation style, Other) with serde, matching DESIGN.md section 8
- [x] Canonical JSON encoding identical to the phone's codec (key order, category order, formatting), so both apps write the same bytes; a round-trip test on the fixture proves it
- [x] `LibraryRepository` trait with the starter set of operations
- [x] Copy `assets/fixtures/library.json` from the phone app; `FixtureLibraryRepository` loads it and keeps changes in memory only
- [x] Icons: export the mockup's icons as SVG into `assets/icons/` and draw them with iced's `svg` widget, tinted by the theme
- [x] Test setup: unit tests on `update` (message in, state out); iced's headless test crate for screen tests if 0.14 has it (`iced_test` 0.14, dev-dependency). `cargo test` passes and the app opens to an empty window
- [x] Screen captures: `cargo test --test screenshots -- --ignored` renders every screen headless in Graphite and Light at 1600×900 to `screenshots/` (git-ignored), to compare with the mockup

## Phase 1: UI on fixture data

### 1a. Shell and shared pieces
- [x] Window shell: sidebar on the left, content area with toolbar on the right (title and count for now; the toolbar's controls are 1b); the content pane's rounded corner as in the mockup
- [x] Sidebar: Library with All and the six types and counts in their type colors; Plans with each plan's name, gray year, done count and progress bar, then "All plans"; TAGS with the six most used tags and "All tags"; the sync row at the bottom (dot and status text, fake states: `LIBRI_SYNC=expired`); the window title follows the screen ("Movies — Libri")
- [x] Shared widgets: chip (filter, status, tag, removable, add), buttons (primary, outline, neutral, destructive = neutral fill with red text), search field with magnifier, popover (a small custom widget) and radio menu, modal dialog with scrim, collapsible tag group (header, count, "n selected"), section header in small gray caps
- [x] Rating picker: tap → 1 to 10 grid, never a slider
- [x] Theme and accent switching, live (state only, from the token sheet; the Settings UI is 1f)
- [x] Keyboard basics from the start: Esc closes the top popover or dialog, Tab moves focus, Enter confirms dialogs (Tab reaches text fields only; see 1g)

### 1b. Library
- [x] Type sections with their colors and counts; selecting a type in the sidebar shows that type only
- [x] Three views per type (text, list with covers, grid with covers, placeholder images), the choice remembered per type; each view's entry row or card is a shared widget in `ui`
- [x] Toolbar: search (Ctrl+K or Ctrl+F focuses it), Filter, Sort, View toggle, Add (opens the Add dialog from 1c)
- [x] Sort menu: Newest added, Title A to Z, Year newest first, Year oldest first, Rating highest first; the button shows the short label
- [x] Filter popover: Decade chips, Tags chips with "All tags…", Rating range 1 to 10, "Include seasonal-only" switch, Reset, live result count
- [x] Tag picker modal (from the Filter's "All tags…"): search across every group, collapsible groups, multi-select. The sidebar's "All tags" opens the All tags popover instead, as in the mockup
- [x] Sidebar tags: pinned or most used (DESIGN.md 4.5); pinning from the All tags popover, up to 8. Pins are state only until phase 2; the choice of mode is in Settings (1f)
- [x] Chips row: status chips, then the active filters (tags, decade, seasonal, rating) as removable chips, then an inline "Clear all"
- [x] Selection highlight on the picked entry (selection gray, not the accent)
- [x] Empty states: empty type, no search results, no filter results
- [x] All as the start screen, a "now and next" overview: In progress, the active plan, Coming soon, Recently added (the past month) and Top rated. Text and list views: two columns with "See all N" links; grid: cover shelves with clickable headers plus a side column with the plan card and Recently added. The plan card always uses the regular accent. Any type, filter, status or search turns All into the full grouped list. "See all" for Coming soon and Recently added filters by "Not released yet" and "Added in the past month", shown as removable chips
- [x] Several types at once: a Type section in Filter, Ctrl+click in the sidebar adds or removes a type, picked types as removable chips (two or more), results grouped by type in the current sort; the view is remembered for All, for several types and per type
- [x] Type checkboxes in the sidebar (optional, colored, a box adds or removes the type, the name shows only it); the setting itself is in Settings (1f)
- [x] DNF badge on grid covers
- [x] Grouped results: a thin type-colored scrollbar (one segment per group, sized by its count; a click jumps to the group; the thumb is an outline) and pinned type headers. `ui::Groups` measures where each group starts; the text view's column header stays above the rows

### 1c. Entry details and editing
- [x] Entry details as in the mockup: cover spot, status, rating, note, about, tags, plans, Edit, Delete; the same details view is used from Plans (wired with the plan screen, 1e)
- [x] Add entry dialog: type, lookup matches (static fixture results), the form, tag picker with collapsible categories and search
- [x] Similar-tag warning and new-tag confirmation with a category pick
- [x] Add to plan
- [x] Delete confirmation ("plan only or everywhere" when the entry is in a plan; the plan-only choice comes with the plan screen in 1e)

### 1d. Music
- [x] Artists / Albums / Songs, "Various Artists" pinned last, a leading "The" ignored when sorting
- [x] One artist: Albums and Songs sections; album details with track ratings (static track lists)
- [x] Album / Song switch in the entry form, with the optional album field for songs (built with the form in 1c)
- [x] Replace dialogs as on the phone: saving an album when songs from it are saved (D12: keep both, or replace the songs and move their ratings to the album's tracks), and saving a song when its album is saved (D13)

### 1e. Plans
- [x] Plans overview ("All plans")
- [x] Plan screen: title with the year dropdown (other years of the same plan, "New year…" in the accent ink), checklist grouped by type
- [x] Timeline view (per-plan override of the global default; the default itself is a Settings item in 1f)
- [x] Seasonal themes: the Halloween and Christmas banner strips (SVG from the mockup), the title icon (the phone's pumpkin), the seasonal accent
- [x] New plan and Edit plan dialog: Name, Year, Started (edit only), Linked hashtags as a drill-in tag view (search, collapsible categories, Season first and the only one open, Done / Clear), Seasonal theme, "Start with: Empty / Copy a plan", Timeline, Delete. No Date range: `library.json` has none, so a plan's dates come from its items and the calendar sets each item's When in the details pane
- [x] The plan page's side pane: "Add to this plan" (tagged entries not in the plan, or wanted ones for a plan without tags; the filter searches the whole library), and the entry details with the plan's When and note
- [x] Sidebar on Plans: plan rows open their plan, the + opens New plan, finished plans only under All plans
- [x] Remove from plan or delete everywhere; Delete plan
- [x] Ctrl+click on a checklist or timeline row ticks it or unticks it, without selecting it (2026-10-25)
- [x] The seasonal progress marker sits flush with the bar's ends at 0 and at 100 percent; it was indented by a percent at both (2026-10-25)
- [x] A plan with every entry ticked stays Active until its last date has passed, so more entries can be added while there is time left; the phone finishes it at once (2026-10-25)

### 1f. Settings and states
- [x] Settings: type checkboxes (off or on), theme (System, Graphite, Charcoal, Catppuccin Mocha, Light; System follows Plasma's color scheme through the desktop portal), background style (two-tone or flat, with the flat tokens in `theme`), sidebar tags (pinned or most used), accent color (Vivid and Catppuccin groups, Peach default), timeline default, seasonal theme default, tags, About row
- [x] Library connection section (UI only)
- [x] Splash: logo, wordmark, indeterminate bar; first-run and returning variants
- [x] Setup wizard, full window (UI only): Welcome, Connect GitHub (repo and token, "stored in KDE Wallet" copy), Branch, Library file (new or existing), Create / Pick existing, All set
- [x] Token-expiry dialog: "Your GitHub token expires in 6 days" with Later / Open GitHub / Replace token; 1-day and expired variants; the sync row's "Replace token" link when expired (driven by fake states)
- [x] Light theme pass over every screen

### 1g. Phase 1 wrap-up
- [x] Walk through every mockup screen next to the app and list the differences; fix them or record the decision in DESIGN.md
- [x] Window sizes: the minimum size, 1366×768, 1920×1080, 2560×1440; KDE fractional scaling at 125 % and 150 %
- [x] Long lists as on the phone (L1 to L6): letter headers and an alphabet rail for artists and title order, decade headers for the year orders, once a list is three screens tall with two sections or more
- [x] Keyboard pass: Ctrl+N new entry (Ctrl+K and Ctrl+F already focus the search), arrow keys in lists, focus visible everywhere. iced 0.14 can focus only text fields, so buttons, chips and rows need a focusable wrapper widget with a visible focus ring
- [x] Contrast check of the inks and type colors in dark and light
- [x] Tests for filtering, sorting and the similar-tag check
- [x] `cargo clippy -- -D warnings` clean; remove the token sheet
- [x] Mouse Back and Forward buttons (and Alt+Left, Alt+Right) move through the places visited: screens, library views, artist pages, plan pages
- [x] KWin's title bar in Libri's colors: a KDE color scheme per theme and style under `assets/color-schemes/`, named to KWin through Plasma's decoration palette protocol (`src/decoration.rs`). Needs a decoration that follows color schemes (Klassy, Breeze, Darkly); an Aurorae theme paints its own
- [x] Desktop freezes with Libri open: solved 2026-10-24, see DESIGN.md "Presentation mode". The trace of the 22:11 freeze showed KWin's render loop idle for exactly as long as Libri's window had been hidden, with explicit sync off; that is KDE bug 527500, NVIDIA's Vulkan driver extrapolating a `wp_commit_timing_v1` target after an idle gap, which KWin 6.8 honors. Libri now presents in mailbox mode, which sends no timestamps (`ICED_PRESENT_MODE=fifo` reproduces the freeze). The monitor and trace stay on for every run until a few days pass without a freeze. Earlier notes, kept for the record: reported 2026-10-20 to 2026-10-24: the whole desktop stops for tens of seconds to minutes; intermittent, and nearly always on coming back to Libri, by switching to its virtual desktop or restoring it). Not reproduced on 2026-10-23 in scripted runs (hidden on another desktop 15 s and 60 s, minimized 30 s, the splash animating while hidden 20 s, on Vulkan, OpenGL and tiny-skia). Facts so far: no NVIDIA Xid, GPU reset, OOM or kernel error in six days of logs, and the kernel keeps logging through a freeze, so it is the compositor's output that stops, not the machine; during the 2026-10-20 freeze KWin logged `Failed to allocate a graphics buffer for a swapchain` for 20 minutes (VRAM or the driver's allocator exhausted then), during the 2026-10-24 one (20:03:54, right at the desktop switch back to Libri, a minute after `cargo run --release`) nothing at all; KWin's event loop answered D-Bus afterwards and every KWin and Libri thread was idle; Libri is the only Vulkan-on-Wayland client on the machine (Qt, Chromium and mpv use EGL), and its frames reach KWin as dma-buf buffers with explicit-sync acquire and release points (`wp_linux_drm_syncobj_v1`, verified with `WAYLAND_DEBUG=1` on 2026-10-24), which is the one path only Libri uses; the title-bar palette is not the cause (it landed on 2026-10-23, after the first freezes); KWin keeps sending frame callbacks to hidden windows at about 40 Hz. Next: run `scripts/freeze-monitor.sh` before every session (it logs KWin's D-Bus liveness, CPU, VRAM per client and every thread's state every two seconds; a gap or `NO_ANSWER` marks the freeze), and A/B over a few days, one switch at a time: `__NV_DISABLE_EXPLICIT_SYNC=1` (takes Libri's frames off syncobj fences; verified to work), then `ICED_PRESENT_MODE=mailbox`, then `WGPU_BACKEND=gl`. If one never freezes, make it the default in `app::run` (set the variable before `iced::application`, since iced 0.14 exposes neither through `iced::Settings`) and record the decision in DESIGN.md. 2026-10-24 21:25 to 21:29, a third freeze with a plain `cargo run --release` (build c79e596, started 21:20:24, no switch set) and again nothing from the monitor, since it had to be started by hand and was not; from 21:24:33 to 21:27:26 the journal is silent (no kernel, KWin, driver or OOM line, no core dump), then KWin and plasmashell log bursts at 21:27:26 to 21:27:29, 21:28:47 to 21:28:52 and 21:29:26 (`This plugin does not support setting window masks` from KWin and the panel colorizer flipping its preset every 150 ms), which is the queued input being worked through as the desktop comes back; the 20:03:54 freeze started with the same `window masks` line, so that line marks the moment (a desktop switch) but not the cause; the screen is a 4K output at 200 %, 120 Hz, adaptive sync on automatic. Since then Libri starts the monitor itself (`app::run`, `LIBRI_NO_MONITOR=1` skips it) and the log goes to `~/.cache/libri/`, which survives a reboot, and Libri writes its own trace into the same file (`src/trace.rs`: window focus, size, scale, open and close events, the redraw rate once a second, any gap of over a second between redraws, which is a present call blocking the event loop, and iced's, winit's and wgpu's warnings through the `log` crate), so the next freeze shows whether Libri was redrawing while hidden, got an event at the switch, or sat blocked in present; `~/.cache/libri/freeze-watch.log` is the 2026-10-23 script's log and can go. 2026-10-24 21:42 to 21:55:37, a fourth freeze, 13 minutes, on clicking the title bar's Close of the same 21:20:24 run (build c79e596, so no monitor and no trace; the two `~/.cache/libri/freeze-20261024-21*.log` files are 4 s smoke tests of the new logging, killed together with their Libri, and the 21:44:44 one shows KWin opening and focusing a new window during the freeze). The machine never stopped: uptime ran on, the kernel logged throughout, and KWin's event loop handled input (`window masks` bursts at 21:46:53, 21:47:55 and at the return, 21:55:37), answered D-Bus and had every thread sleeping with no kernel wait, VRAM at 3.8 of 12 GB; no kernel, driver or OOM line. Only presentation stopped, so KWin was waiting on a page flip or fence that never completed until something timed out. The screen came back within a minute of a read-only D-Bus call to KWin and `kscreen-doctor -o` from the phone-driven session; probably coincidence, but a lead. The stack: KWin 6.8.1 (6.8.0 landed 2026-10-16, two days before Libri's first commit; 6.8.1 on 10-22), NVIDIA 615.78.08 userspace since 10-09 with the open kernel module rebuilt for kernel 7.3 on 10-21 and 10-22, HDR and wide color gamut on, adaptive sync automatic, tearing allowed; nothing was upgraded between 10-16 and the first freeze on 10-20, so no package change explains the start, only Libri's arrival. All four freezes sit on a change to the window set that includes Libri's surface (reveal, restore, close), which fits a fence on Libri's last buffer. Since the close is now a known trigger, the monitor keeps sampling KWin for 90 s after Libri exits, in its own process group so a Ctrl+C does not end it with Libri (`scripts/freeze-monitor.sh`, `app::freeze_log`). Next: the A/B as above, `__NV_DISABLE_EXPLICIT_SYNC=1` first; during the next freeze try `qdbus6 org.kde.KWin /Compositor org.kde.kwin.Compositing.reinitialize` from SSH or the phone-driven session, then `kscreen-doctor output.HDMI-A-1.vrrpolicy.never`, and record whether either brings the screen back; if explicit sync off still freezes, A/B the system side next, `kscreen-doctor output.HDMI-A-1.hdr.disable` and adaptive sync never, since the stall is on the scanout side

The parts below catch up with the phone's canvas revisions of 2026-10-18 to 2026-10-24 (`ekmanch/library-app` → `TODO.md`, "Ongoing: Polish and improvements"). Each part starts with a canvas revision of `design/canvas.html` that adapts the phone's boards to the desktop (sidebar, content pane, inspector, dialogs, right-click instead of long-press); the build items follow once the boards exist. Phone-only mechanics stay out: pull-to-close sheets, page transitions, the Released wheel picker, the grid's fast-scroll thumb, portrait lock, scroll capture.

- [ ] `/tmp` holds thousands of empty `libri-cache-XXXXXX` folders (4151 on 2026-10-24), one per test or capture run by the look of the timestamps; nothing in `src` or `tests` names them, so find which crate makes them (iced_test or wgpu, probably through `std::env::temp_dir`) and make the runs clean up

### 1h. Tags
- [x] Reconcile the tag categories with the phone first, since they are part of the file format: the phone added Languages (after Season) and Streaming (after Studios); the desktop has Animation style, which the phone's TODO never mentions. Settled: the phone's DESIGN.md section 7 lists all thirteen, Animation style included, so `model::tag`, the codec and the fixture (the phone's copy again, byte for byte) follow it
- [x] Canvas revision: tags in more than one category (role chips such as "Clint Eastwood · Director, Actor" in the form's Tags box, the inspector, active filter chips and search results; per-group ticks in the tag picker; the categories dialog, opened with right-click; "Add to Directors" for a person found under another people category; the New tag dialog taking several categories of one kind)
- [x] Canvas revision: long tag groups (more than 24 tags) in the tag picker, the Filter popover and the plan dialog, as a desktop pattern in place of the phone's full-screen list (search, letter headers, index rail, a check per row); groups folded by default except those holding a tag on this entry and "New on this entry"
- [x] Canvas revision: moving a one-category tag to another one-category group (Other to Languages or Streaming) from the right-click menu
- [x] Build: `tag_categories` on entries in the model and codec (written only for tags in several categories), the per-category filter, and the screens from the boards above (one shared picker, `ui::tag_picker`, for the Filter popover, the form and the plan dialog)
- [x] Build: Languages and Streaming (once settled), Streaming only for movies and shows in the picker, the new-tag dialog and the Filter popover
- [x] Tests for the file form, filtering per category, searching and the picker; captures of the new states

### 1i. Shows, anime and series
- [x] Canvas revision: seasons for shows (row subtitle "2022–2026 · 3 seasons" and the "New season" badge, shortening to "New" when tight; the inspector's Seasons section with a status pill per season and the derived Status row; one-season shows; announced seasons as "Coming 2027"; the catch-up dialog; "Announced season dropped")
- [x] Canvas revision: all seasons and a season's episodes (number, title, air date, runtime; "4 of 6 episodes out", "Airs Oct 22", "Title TBA"). Decided 2026-10-24: the content pane with the inspector kept, not a drill-in inside it
- [x] Canvas revision: anime kind (series or movie): the "Series or movie" control in the form; an anime series uses the show screens in the anime color
- [x] Canvas revision: series (the inspector's Series section with up to five parts, else three around this one and "All N volumes"; "+ Add" for missing parts; Series and Number fields in the form; a series as one row in long lists with a stack icon and "2 of 4 done"; the series page; the cover sheet that picks which part's cover stands for the series)
- [x] Design note (no board needed): page count for books, "160 pages" under the author in the inspector and a Pages field in the form (DESIGN.md section 10, "Seasons and series")
- [x] Build: `pages`, `series` (with `cover`) and the anime `kind` in the model and codec, matching the phone's key order (`seasons` is already read). Done in phase 1h, when the fixture became the phone's copy again; a season's `manual` flag too
- [x] Build: seasons in rows, the inspector, the form's season editor and plan ticks through the seasons; derived show status. The dropped-season dialog is built too, opened by `LIBRI_DROPPED=<id>:<n>` until the daily check of phase 4
- [x] Build: all seasons and episodes from the boards, on fixture episode lists until phase 4 (`lookup::episodes`)
- [x] Build: series in the inspector, series rows in long lists (`isLongList` rule, status All, Done or DNF, two parts or more), the series page and its cover sheet, on fixture part lists until phase 4 (`lookup::series_parts`: The Lord of the Rings and Berserk)
- [ ] Fixture: the phone's Haunted Hotel, Carrie, South Park, Frieren, The Lord of the Rings and Berserk entries are in `assets/fixtures/library.json` once the phone's copy has them (keep it byte-identical). Frieren, The Lord of the Rings and Berserk are in since phase 1h; Haunted Hotel, Carrie and South Park are not in the phone's copy yet, so the captures build those states with `SetSeasons` on Stranger Things
- [x] Tests for derived status, series grouping and the new fields; captures of every new state in Graphite and Light

### 1j. Add and Edit dialog
- [x] Canvas revision: the cover in the form with "From the match · tap to change" or "From another match · Use the match's cover", and the Pick a cover picker (the match's cover, then the other matches with year and edition)
- [x] Canvas revision: game lookup on a platform (a "Platform · Any" chip under the type chips with a cross to clear, the platform picker with search, Any, Your platforms and All platforms)
- [x] Canvas revision: Look up again for an existing entry (the lookup pane in Edit; a pick replaces creator, year, description, pages and album, keeps title, note, status, tags and formats)
- [x] Canvas revision: the lookup's states in the left pane: loading, offline, "not set up", partial failure ("a match may be missing"), no matches
- [x] Design note (no board needed): "Year from the ebook edition. Check if it's a reprint." under Released while adding a book dated only by Apple Books; suggested actor tags from the cast; Languages and Streaming suggestions (DESIGN.md section 10, "The lookup pane")
- [x] Build: `cover_from` in the model and codec, the cover picker and the platform filter on fixture results; Look up again and the lookup states driven by fake states (`LIBRI_LOOKUP`; the real providers stay phase 4)
- [x] Tests and captures of the new states

### 1k. Sync and settings
- [x] Canvas revision: sync states beyond "Synced" and "Token expired": offline, saving, "Could not save" with Retry, "Library not found on GitHub" with what is gone and Fix (to Settings → Library source). Decided 2026-10-24: the sidebar row always tells; the states that need the owner (Could not save, Library not found, Sync conflict) also get a banner over the content on Library and Plans
- [x] Canvas revision: the conflict (where it is announced, and the "Keep mine / Keep GitHub's / Decide later" dialog, also reachable from Library source)
- [x] Canvas revision: Disconnect in two steps (confirm, then keep the library on this computer or delete it) and Library source in local mode ("Connect to GitHub"); reconnecting a kept library to another file ("Which library?")
- [x] Canvas revision: About Libri (icon, name, version and build, open-source licenses, Data sources with TMDB's logo and required sentence, Deezer's logo in its light and dark variant, AniList, Apple Books, Open Library, IGDB, Wikidata; each row opens the site). The logos are in `assets/logos/`
- [ ] Decide with the phone: the Apple Books badge near book covers from the iTunes Search API (open on the phone too)
- [x] Build: every state above on fake sync states (`LIBRI_SYNC=saving`, `offline`, `error`, `not-found`, `conflict`; `LIBRI_LOCAL=1` for the kept library), so phase 3 only wires them up. The licenses dialog lists the crates and fonts with their licenses; phase 5 generates the full texts at build time
- [x] Captures of every new state in Graphite and Light
- [x] Glass theme from the canvas revision of 2026-10-26 (`design/canvas.html`, Library and Settings boards): see-through surfaces on a transparent window, KWin's blur asked for through the `org_kde_kwin_blur` protocol (`decoration`), the Settings card and help, `glass.colors` for the title bar, the contrast check over a mid gray, and Glass in the captures. `LIBRI_THEME=glass` and `LIBRI_FLAT=1` start in it (DESIGN.md section 10, Glass)
- [x] Fix: opening a dialog (the licenses, Disconnect, the tag picker, any `ui::modal`) scrolled the screen under it back to the top, since the stack only existed with a dialog open and iced rebuilt the tree. The stack and the barrier are always there now, the barrier inactive without a dialog (2026-10-24)

## Phase 2: Local data

- [x] Persist the library to a local file (`~/.local/share/libri/`), changes survive a restart (`repo::LocalLibraryRepository`, `storage`; seeded from the fixture until phase 3, DESIGN.md section 10, "Local files")
- [x] Operations layer: every change is a small named operation with a readable summary, as on the phone (`Operation::summary`, in the phone's words; written to the trace until phase 3 uses them as commit messages)
- [x] Display preferences on the device (`~/.config/libri/`), never synced (`preferences`; a run with `LIBRI_THEME`, `LIBRI_FLAT` or `LIBRI_PLASMA` reads them but never writes them)
- [x] System theme for real: read Plasma's color scheme from the desktop portal (`org.freedesktop.appearance` `color-scheme`) and follow it while Libri runs. Decided 2026-10-27: no crate of our own; iced 0.14 already reads the portal (through `mundy` and `zbus`, both in the build) and offers `iced::system::theme()` and `theme_changes()`. `LIBRI_PLASMA=light` still fakes a light Plasma, and then the portal is ignored
- [ ] Try a live switch of Plasma's color scheme with Libri on System and confirm the window and the title bar follow (only the answer at the start was seen on 2026-10-27)

## Phase 3: GitHub sync

- [x] Setup for real: token in KDE Wallet (`wallet`, over kwalletd's own D-Bus interface), repo, branch, file; the real branch and file lists, the mockup's "Checking access…" and "Creating library.json…" states, Sync now and Disconnect repository doing their work (`screens::setup` with `github::Client`; DESIGN.md section 10, "GitHub sync" and "Setup for real")
- [x] `connection.json` next to the library (repo, branch, file, and whether the library was kept on this computer, as on the phone), so a restart keeps the connection and local mode (`connection`); a start without one is the first run, and a run with a screen switch keeps the mockup's source
- [x] `pending.json`: the operations in JSON (the phone's `op` names) with their summaries, written before the local file applies the change, and the commit messages from the summaries (one gives its summary, several "<first> and N other changes" with every summary in the body) (`repo::json`, `sync::pending`, `sync::commit_message`)
- [x] Read, bundled saves with readable commit messages, three-way sync and conflicts, offline queue, the same rules as the phone (DESIGN.md section 10b) (`sync::Engine` with `sync.json` as the base; the app runs its requests as Tasks)
- [x] Real token expiry from GitHub's response header drives the warning dialog (`connection.json` keeps the date and the day last warned)
- [x] Fix: the Add and Edit dialog was see-through on Glass (it painted the content and chrome colors), so the library showed through the form. Every dialog surface is opaque on every theme now (`dialog` and `dialog_chrome` tokens, Graphite's shades on Glass), checked in every theme, two-tone and flat; a dialog over a dialog keeps its own scrim (DESIGN.md section 10, "Dialog surfaces")
- [x] The details pane's "+ Tag" opens the tag picker alone, and Done writes the tags as one change (`Operation::SetTags`); X or Esc drops them, a new tag still asks for its category. The note is typed in the pane: Enter saves it as one change (`Operation::SetNote`), Esc puts the saved note back, moving on saves it (DESIGN.md section 10, "The details pane")
- [x] The plan note in the details pane wrote one `SetPlanItem` per keystroke, each a queued change with its own summary line in the commit. It has the entry note's rule now: a draft, saved on Enter or when the pane moves on, dropped with Esc
- [x] Live test against `ekmanch/libri-scratch` (seeded with a copy of the real `library.json` on 2026-10-29). Done on 2026-10-29: the first run picking the file (the token checked, the branches and files listed, the library downloaded behind the splash) and two edits landing as commits with the token's expiry recorded. Done on 2026-10-30: a hand edit on GitHub pulled at the next start, and a hand edit while Libri ran picked up by the five-minute check (it found the conflict with a note typed here, but the hand edit had broken the file, see the next item). Still to try: a conflict with Keep mine and Keep GitHub's on a readable file, Disconnect keeping the library and connecting the same file again, Replace token
- [x] A file on GitHub that no longer decodes (a hand edit that put a number for a string) stopped as "Could not save · GitHub didn't answer", and Retry brought the conflict back forever. Found 2026-10-30; now its own state, "Broken on GitHub", with the reason, Check again and Keep mine (DESIGN.md "A file on GitHub that is no library"; `LIBRI_SYNC=unreadable`)
- [x] Settings › Library source shows the repository, branch and file but no longer edits them: a change goes through the setup (Pick a file from the not-found card, Disconnect and connect again). Decided 2026-10-31: the values stay read-only, and "Change repository or file…" opens the setup's Connect step with the connection and the token kept; Back returns to Settings (DESIGN.md "Reconnecting from Settings")
- [x] "Pick a file" from Library not found drops the queue with the old engine, so edits queued for the missing file stay only in the local file and are lost when the new file's copy replaces it. Match the phone's "Different file: which library?" there. Done 2026-10-31: another file asks "Which library?" (this computer's uploaded over it, with the unsaved count, or GitHub's), the same file keeps the queue, a gone branch or repo opens at that step (`App::connect`, `settings::reconnect`)
- [x] A spinning ring for the setup's busy line; iced 0.14 has no animation for a border, so it stands still (a canvas, or the splash's tick). Done 2026-10-31: `ui::spinner`, 24 SVG frames turned by the splash's tick while the wizard is busy
- [x] The daily token warning is checked only at start; Libri left open past midnight waits for the next start. Done 2026-10-31: checked with every sync round (the five-minute check, Sync now, the window coming back), never over the setup

## Phase 4: Lookups and images

- [x] "Look up again" in the Edit dialog (the lookup pane for an existing entry), as on the phone (A8). Done 2026-10-31: a pick fetches the match's details, replaces the lookup's fields where the match has them and forgets the entry's cover record on Save, so the next look resolves through the source
- [x] The phone's lookup providers and keys handling (`secrets.json`, git-ignored); covers cached under `~/.cache/libri/`. Done 2026-10-31 (DESIGN.md "Lookups and images"): TMDB, IGDB, AniList, Open Library with Apple Books (and Google Books by id), Deezer and Wikidata in `src/lookup/`, the covers, photos, episodes, series and track lists in `src/artwork.rs` and under `~/.cache/libri/`; `cargo test --test lookup_live -- --ignored --nocapture --test-threads=1` asks the real providers once
- [ ] The phone's daily checks (its DESIGN.md 10c: new seasons with the "season no longer listed" dialog, release dates, followed series and creators, Coming soon: everything upcoming) have no desktop counterpart yet; the dropped-season dialog still opens only with `LIBRI_DROPPED`
- [ ] A first track rating could store the Deezer track id as the phone does (`deezer:track:<id>`); `SetTrackRating` carries the title alone, so ratings are matched to the list by title (case, punctuation and edition notes aside), which the phone also falls back to
- [ ] The fake lookup paints no images, so no capture shows a real cover; look at them with `cargo run` and a connected library
- [x] Fetched covers drew nothing (the title went, the tint stayed): the files were `<sha1>.img` and iced's decoder picks the format by the extension. Done 2026-10-31: named by the format the bytes start with, old files renamed on the way
- [x] The sidebar's All reads Home. Done 2026-10-31
- [x] The tag picker's long group: the dialog grew to the window and the letter rail ran past the list. Done 2026-10-31

## Phase 5: Packaging

- [ ] `.desktop` file and app icon from `assets/logos/libri`; a local PKGBUILD so Shelly can install it
- [ ] Install the title bar color schemes (`assets/color-schemes/`) with the app and point `decoration::scheme_path` at the installed copy; until then they are read from the checkout, so an installed binary would keep KWin's default bar

## Later

- [ ] Drop `vendor/iced_wgpu` and the `[patch.crates-io]` lines in `Cargo.toml` with the upgrade to the next iced: iced master already skips 2-bit-alpha surface formats (DESIGN.md section 10, Glass), and Cargo warns that the patch is unused once iced_wgpu moves past 0.14.0
