# Libri desktop

Desktop front end (KDE Plasma on CachyOS) for my personal media library: movies, shows, books, games, music and anime. It is the desktop sibling of the phone app `ekmanch/library-app` and reads and writes the same `library.json`.

## Read these first

- `TODO.md`: the build order. Work through it top to bottom unless I say otherwise.
- `design/canvas.html`: the desktop mockup (Splash, Setup, Library, Plans, Settings, with the design notes under each). The visual reference for every screen.
- `ekmanch/library-app` → `docs/DESIGN.md`: the data model (section 8), tag rules, sync rules (section 10b) and visual design (section 11). This app follows them rather than redefining them. Locally at `/mnt/media/github/own/library-app`.
- `docs/DESIGN.md`: decisions that only concern the desktop app.

## Stack

- System Rust 1.99 from the repos (cargo, rustfmt, clippy). No rustup.
- iced 0.14 with `svg`, `image` and `tokio`; serde and serde_json for the data; `iced_test` for headless screen tests.
- System packages (and anything else installed on the machine) go through Shelly.

## Module layout

- `app`: state, `Message`, `update`, `view`, window settings and font loading.
- `theme`: design tokens from the mockup (backgrounds, type colors, accents, fonts, text sizes). Never hard-code a color or font in a screen; add it here.
- `ui`: widgets shared by every screen (buttons, headers, icons). UI that repeats across screens is one shared widget here, never copied.
- `screens`: one module per screen (splash, setup, library, plans, settings), plus the throwaway token sheet.
- `model`: the `library.json` model and `model::codec`, the canonical JSON encoding.
- `repo`: the `LibraryRepository` trait and its implementations (the fixture one for now).
- `fixtures`: the bundled fixture library.

## Git rules

- **Never commit, amend, push, tag or create a branch unless I explicitly ask for it in that message.** Finishing a task is not a request to commit.
- When a task is done, end your reply with the suggested commits so I can run them myself, in the style `type(scope): Subject (phase N)` (types: feat, fix, doc, test, asset, git, chore; the phase is the TODO.md phase with its letter, e.g. `phase 0d`).

## Working rules

- Tick off items in `TODO.md` as they are completed. Add new items there when you find missing work, instead of doing unplanned work silently.
- Phase 1 is UI only, on fixture data: no networking, no GitHub access, no file writing before the phase that calls for it.
- UI text uses American spelling ("color", "favorite") and sentence case.
- All `library.json` text goes through `model::codec`, which writes the same bytes as the phone's `lib/data/library_codec.dart`. Keep `assets/fixtures/library.json` byte-identical to the phone's copy; the round-trip test checks it is canonical.
- Ask before adding a crate. Say what it is for and what the alternative is.

## Run and test

```fish
cargo run                          # opens the app (library screen)
LIBRI_SCREEN=tokens cargo run      # starts on another screen: splash, setup, library, plans, settings, tokens
LIBRI_SYNC=expired cargo run       # the sync row's expired-token state
ICED_BACKEND=tiny-skia cargo run   # software renderer
```

Before saying a task is done, run all three and tell me if any fail:

```fish
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

To look at a screen, capture it rather than taking a desktop screenshot. This renders every screen in Graphite and Light at 1600 × 900 (2× scale) to `screenshots/` (git-ignored):

```fish
cargo test --test screenshots -- --ignored
```

Compare each capture with its board in `design/canvas.html`. When a screen is built or changed, capture it and check it before saying it is done.

The pre-commit hook in `.githooks/` runs the formatter and clippy; enable it once per clone with `git config core.hooksPath .githooks`.

Screen tests use `iced_test::simulator` on a screen's `view` (see `screens/token_sheet.rs`); logic tests call `App::update` directly (message in, state out).
