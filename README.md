# libri-kde

Libri for the desktop: a KDE Plasma front end, written in Rust with [iced](https://iced.rs), for a personal library of books, movies, shows, anime, games and music. It shares its data format with the phone app, [ekmanch/library-app](https://github.com/ekmanch/library-app).

## Build and run

Needs Rust 1.99 or newer with cargo (the system package is fine), and on Plasma Wayland `vulkan-icd-loader`, `libxkbcommon`, `wayland` and `fontconfig`.

```sh
cargo run                          # build and start
cargo run --release                # optimized build
ICED_BACKEND=tiny-skia cargo run   # software renderer, without the GPU
WGPU_BACKEND=gl cargo run          # OpenGL instead of Vulkan
ICED_PRESENT_MODE=mailbox cargo run # Vulkan without FIFO waits
cargo test                         # unit and headless screen tests
```

For development, `LIBRI_SCREEN` starts on a screen (`splash`, `setup`, `library`, `plans`, `settings`), `LIBRI_SETUP` on a setup step, `LIBRI_SYNC` fakes the sync state (`expired`, `expires-in-6`), `LIBRI_PLASMA=light` a light Plasma for the System theme, `LIBRI_FIRST_RUN=1` a first run and `LIBRI_LONG_LISTS=1` the long-list headers and rail on every list. `cargo test --test screenshots -- --ignored` renders every screen to `screenshots/`.

`design/canvas.html` is the desktop mockup; open it in a browser to click through the screens.
