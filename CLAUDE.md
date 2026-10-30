# Libri desktop

Desktop front end (KDE Plasma on CachyOS) for my personal media library: movies, shows, books, games, music and anime. It is the desktop sibling of the phone app `ekmanch/library-app` and reads and writes the same `library.json`.

## Read these first

- `TODO.md`: the build order. Work through it top to bottom unless I say otherwise.
- `design/canvas.html`: the desktop mockup (Splash, Setup, Library, Plans, Settings, with the design notes under each). The visual reference for every screen.
- `ekmanch/library-app` → `docs/DESIGN.md`: the data model (section 8), tag rules, sync rules (section 10b) and visual design (section 11). This app follows them rather than redefining them. Locally at `/mnt/media/github/own/library-app`.
- `docs/DESIGN.md`: decisions that only concern the desktop app.

## Stack

- System Rust 1.99 from the repos (cargo, rustfmt, clippy). No rustup.
- iced 0.14 with `svg`, `image` and `tokio`; serde and serde_json for the data; `iced_test` for headless screen tests. `vendor/iced_wgpu` is iced_wgpu 0.14.0 with one change (the surface format keeps 8 bits of alpha), patched in through `Cargo.toml`.
- System packages (and anything else installed on the machine) go through Shelly.

## Module layout

- `app`: state, `Message`, `update`, `view`, window settings and font loading.
- `theme`: design tokens from the mockup (backgrounds, type colors, accents, fonts, text sizes). Never hard-code a color or font in a screen; add it here.
- `ui`: widgets shared by every screen (buttons, headers, icons). UI that repeats across screens is one shared widget here, never copied.
- `screens`: one module per screen (splash, setup, library, plans, settings), the window shell and the token warning.
- `model`: the `library.json` model and `model::codec`, the canonical JSON encoding.
- `repo`: the `LibraryRepository` trait, every change as an `Operation` with its summary, and the implementations: the local file (`LocalLibraryRepository`) and the fixture in memory for the tests.
- `storage`: the folders under the XDG homes and the atomic file write.
- `preferences`: the display preferences file, what it holds and when it is written.
- `connection`: `connection.json`, the repo, branch, file, local mode and the token's expiry.
- `github`: the REST client (blocking `ureq`, run on tokio's blocking pool from a Task) and the git blob sha.
- `sync`: the three-way sync engine (pure: requests out, answers in), `pending.json` and `sync.json`.
- `wallet`: the token in KDE Wallet over kwalletd's D-Bus interface.
- `clock`: timestamps as GitHub and the sync files write them, local dates, "5 minutes ago".
- `fixtures`: the bundled fixture library.

## Git rules

- **Never commit, amend, push, tag or create a branch unless I explicitly ask for it in that message.** Finishing a task is not a request to commit.
- When a task is done, end your reply with the suggested commits so I can run them myself, in the style `type(scope): Subject (phase N)` (types: feat, fix, doc, test, asset, git, chore; the phase is the TODO.md phase with its letter, e.g. `phase 0d`).

## Working rules

- Tick off items in `TODO.md` as they are completed. Add new items there when you find missing work, instead of doing unplanned work silently.
- The files on this computer, all written atomically through `storage`: the library `~/.local/share/libri/library.json` with `connection.json`, `pending.json` and `sync.json` beside it, and the preferences `~/.config/libri/preferences.json`. The token is in KDE Wallet only. The tests never touch any of them: `App::new` keeps everything in memory, and file tests use `storage::TempDir`.
- Networking is GitHub's REST API alone, through `github::Client`, and only from the sync engine's requests and the setup. The sync logic lives in `sync::Engine` and is tested with scripted answers, never against GitHub; the live checks run by hand against `ekmanch/libri-scratch`, never `ekmanch/library`.
- UI text uses American spelling ("color", "favorite") and sentence case.
- All `library.json` text goes through `model::codec`, which writes the same bytes as the phone's `lib/data/library_codec.dart`. Keep `assets/fixtures/library.json` byte-identical to the phone's copy; the round-trip test checks it is canonical.
- Ask before adding a crate. Say what it is for and what the alternative is.

## Run and test

```fish
cargo run                          # the real app: connection.json, KDE Wallet and GitHub; the setup on a first run
LIBRI_SCREEN=plans cargo run       # a mock run on another screen: splash, setup, library, plans, settings. LIBRI_SCREEN, LIBRI_SYNC, LIBRI_SETUP, LIBRI_FIRST_RUN and LIBRI_LOCAL all make the run a mock: the mockup's source, the fixture library, no GitHub and no wallet
LIBRI_SYNC=expired cargo run       # the sync row's state: synced, saving, offline, error, not-found, conflict, expired; expires-in-6 shows the warning
LIBRI_LOCAL=1 cargo run            # disconnected with the library kept on this computer (local mode)
LIBRI_SHOT=settings cargo test --test screenshots -- --ignored capture_every_screen   # only the captures whose name contains it; LIBRI_SHOT_THEME=charcoal,mocha picks other themes than Graphite, Light and Glass, LIBRI_SHOT_FLAT=1 the flat style (named "-flat")
LIBRI_PLASMA=light cargo run       # a light Plasma, for the System theme (any value stops following the portal)
LIBRI_THEME=glass cargo run        # starts in a theme: system, graphite, charcoal, mocha, glass, light; LIBRI_FLAT=1 for the flat style. With LIBRI_THEME, LIBRI_FLAT or LIBRI_PLASMA set the preferences are read but never written
LIBRI_FIRST_RUN=1 cargo run        # no repository yet (the splash's first-run line, Settings not connected)
LIBRI_SETUP=branch cargo run       # with LIBRI_SCREEN=setup: welcome, connect, branch, file, create, existing, done
LIBRI_LONG_LISTS=1 cargo run       # section headers and the jump rail on every list, and tag groups of four or more as long groups, since the fixture is short
LIBRI_DROPPED=shows/stranger-things-2016:1 cargo run  # the "season no longer listed" dialog for a show's season
LIBRI_LOOKUP=offline cargo run     # the Add dialog's lookup state: loading, offline, not-set-up, partial, partial-none
ICED_BACKEND=tiny-skia cargo run   # software renderer
ICED_PRESENT_MODE=fifo cargo run   # iced's FIFO default instead of the app's mailbox; reproduces the 2026-10-24 desktop freeze on KWin 6.8 with NVIDIA (DESIGN.md "Presentation mode")
WGPU_BACKEND=gl cargo run          # OpenGL through EGL instead of Vulkan
LIBRI_NO_MONITOR=1 cargo run       # without the freeze log; by default every run writes ~/.cache/libri/freeze-<date>.log: Libri's window events, redraw rate and gaps, and iced/winit/wgpu warnings (src/trace.rs), and scripts/freeze-monitor.sh adds KWin, VRAM and thread states every 2 s until 90 s after Libri exits (the 21:42 freeze began at the close)
scripts/freeze-monitor.sh          # the monitor on its own, to watch the desktop without Libri (Ctrl+C stops it)
qdbus6 org.kde.KWin /Compositor org.kde.kwin.Compositing.reinitialize  # during a freeze, from SSH or a Claude Code session driven from the phone: restarts KWin's render path without ending the session (untried; TODO.md 1g)
```

Before saying a task is done, run all three and tell me if any fail:

```fish
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

To look at a screen, capture it rather than taking a desktop screenshot. This renders every screen in Graphite, Light and Glass at 1600 × 900 (2× scale) to `screenshots/` (git-ignored), and the window sizes of phase 1g to `screenshots/sizes/`. Glass comes out see-through on nothing; to see it over the blur, run `LIBRI_THEME=glass cargo run` and capture the window with `spectacle -b -n -a -o <file>`:

```fish
cargo test --test screenshots -- --ignored
```

Compare each capture with its board in `design/canvas.html`. When a screen is built or changed, capture it and check it before saying it is done.

The pre-commit hook in `.githooks/` runs the formatter and clippy; enable it once per clone with `git config core.hooksPath .githooks`.

Screen tests use `iced_test::simulator` on the app's `view` (see the tests in `screens/library/entries.rs`); logic tests call `App::update` directly (message in, state out).
