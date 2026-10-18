# libri-kde

Libri for the desktop: a KDE Plasma front end, written in Rust with [iced](https://iced.rs), for a personal library of books, movies, shows, anime, games and music. It shares its data format with the phone app, [ekmanch/library-app](https://github.com/ekmanch/library-app).

## Build and run

Needs Rust 1.99 or newer with cargo (the system package is fine), and on Plasma Wayland `vulkan-icd-loader`, `libxkbcommon`, `wayland` and `fontconfig`.

```sh
cargo run                          # build and start
cargo run --release                # optimized build
ICED_BACKEND=tiny-skia cargo run   # software renderer, without the GPU
cargo test                         # unit and headless screen tests
```

`design/canvas.html` is the desktop mockup; open it in a browser to click through the screens.
