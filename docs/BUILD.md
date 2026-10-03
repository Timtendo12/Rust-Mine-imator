# Building and running

## Requirements

- Rust 1.85 or newer (`rustup`), with the default toolchain for your platform
- Node.js 20 or newer and [pnpm](https://pnpm.io)
- The [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/):
  - Windows: Microsoft C++ Build Tools and WebView2 (preinstalled on Windows 11)
  - macOS: Xcode command line tools
  - Linux: `webkit2gtk-4.1`, `libayatana-appindicator3`, `librsvg2` and `libasound2` (ALSA, for sound playback) development packages

No other native libraries need to be built. (The original needs Qt, FFmpeg, OpenAL and libzip
compiled from source; this rewrite uses Rust crates instead.)

Exporting videos runs an `ffmpeg` program at run time (see COMPATIBILITY.md); it is not needed
to build, and the encoding test is skipped where it is not installed.

## Commands

Run from the repository root.

| Command | What it does |
|---|---|
| `pnpm install` | Installs the frontend dependencies and the Tauri CLI |
| `pnpm tauri dev` | Runs the application with hot reload of the frontend and rebuild of the backend |
| `pnpm tauri build` | Builds a release bundle into `target/release/bundle` |
| `cargo test --workspace` | Runs all Rust tests |
| `pnpm build` | Type-checks and builds the frontend only |
| `cargo build -p mine-imator` | Builds the application binary only |

## Tests

Tests in `crates/mi-format/tests` read the data files in `assets/`, which were written by the
original program, and check that they are reproduced exactly. If you change how a file is written,
these tests tell you whether the original would still produce the same bytes.

## Repository layout

The `assets/` folder is a copy of `GmProject/datafiles` from the original repository (without the
Windows-only DLLs). `assets/Data/Minecraft/<version>.zip` and `.midata` are the Minecraft asset pack
and its manifest.

## Checking a render without the window

`src-tauri/src/render_check.rs` renders a project to a PNG with the viewport's own scene builder and renderer:

```
MI_RENDER_PROJECT=<file.miproject> MI_RENDER_OUT=<file.png> cargo test -p mine-imator --lib render_check
```

Optional variables choose the frame, the camera, timelines to outline and a pixel to pick; they are listed at the top of that file. Without `MI_RENDER_PROJECT` the test does nothing.
