# TODO

Build order for Libri desktop (Rust + iced, KDE Plasma on CachyOS). Screens refer to design/libri-desktop-mockup.html (Splash, Setup, Library, Plans, Settings, with the design notes under each). Desktop decisions are in docs/DESIGN.md. The data model, tag rules and sync decisions are shared with the phone app and live in ekmanch/library-app → docs/DESIGN.md; this app follows them rather than redefining them.

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
- [ ] Plans overview ("All plans")
- [ ] Plan screen: title with the year dropdown (other years of the same plan, "New year…" in the accent ink), checklist grouped by type
- [ ] Timeline view (per-plan override of the global default)
- [ ] Seasonal themes: the Halloween and Christmas banner strips (SVG from the mockup), the title icon (the phone's pumpkin), the seasonal accent
- [ ] New plan and Edit plan dialog: Name, Year, Date range with the calendar, Started (edit only), Linked hashtags as a drill-in tag view (search, collapsible categories, Season first and the only one open, Done / Clear), Seasonal theme, "Start with: Empty / Copy a plan", Timeline, Delete
- [ ] Remove from plan or delete everywhere; Delete plan

### 1f. Settings and states
- [ ] Settings: type checkboxes (off or on), theme (System, Graphite, Charcoal, Catppuccin Mocha, Light; System follows Plasma's color scheme through the desktop portal), background style (two-tone or flat, with the flat tokens in `theme`), sidebar tags (pinned or most used), accent color (Vivid and Catppuccin groups, Peach default), timeline default, seasonal theme default, tags, About row
- [ ] Library connection section (UI only)
- [ ] Splash: logo, wordmark, indeterminate bar; first-run and returning variants
- [ ] Setup wizard, full window (UI only): Welcome, Connect GitHub (repo and token, "stored in KDE Wallet" copy), Branch, Library file (new or existing), Create / Pick existing, All set
- [ ] Token-expiry dialog: "Your GitHub token expires in 6 days" with Later / Open GitHub / Replace token; 1-day and expired variants; the sync row's "Replace token" link when expired (driven by fake states)
- [ ] Light theme pass over every screen

### 1g. Phase 1 wrap-up
- [ ] Walk through every mockup screen next to the app and list the differences; fix them or record the decision in DESIGN.md
- [ ] Window sizes: the minimum size, 1366×768, 1920×1080, 2560×1440; KDE fractional scaling at 125 % and 150 %
- [ ] Long lists as on the phone (L1 to L6): letter headers and an alphabet rail for artists and title order, decade headers for the year orders, once a list is three screens tall with two sections or more
- [ ] Keyboard pass: Ctrl+N new entry (Ctrl+K and Ctrl+F already focus the search), arrow keys in lists, focus visible everywhere. iced 0.14 can focus only text fields, so buttons, chips and rows need a focusable wrapper widget with a visible focus ring
- [ ] Contrast check of the inks and type colors in dark and light
- [ ] Tests for filtering, sorting and the similar-tag check
- [ ] `cargo clippy -- -D warnings` clean; remove the token sheet

## Phase 2: Local data

- [ ] Persist the library to a local file (`~/.local/share/libri/`), changes survive a restart
- [ ] Operations layer: every change is a small named operation with a readable summary, as on the phone
- [ ] Display preferences on the device (`~/.config/libri/`), never synced

## Phase 3: GitHub sync

- [ ] Setup for real: token in KDE Wallet (Secret Service), repo, branch, file
- [ ] Read, bundled saves with readable commit messages, three-way sync and conflicts, offline queue, the same rules as the phone (DESIGN.md section 10b)
- [ ] Real token expiry from GitHub's response header drives the warning dialog

## Phase 4: Lookups and images

- [ ] "Look up again" in the Edit dialog (the lookup pane for an existing entry), as on the phone (A8)
- [ ] The phone's lookup providers and keys handling (`secrets.json`, git-ignored); covers cached under `~/.cache/libri/`

## Phase 5: Packaging

- [ ] `.desktop` file and app icon from `assets/logos/libri`; a local PKGBUILD so Shelly can install it

## Later
