# Mine-imator (Rust rewrite)

A rewrite of [Mine-imator](https://www.mineimator.com), the Minecraft 3D movie maker, in Rust with a
Tauri + React user interface. The original is written in GameMaker Language transpiled to C++ on a
Qt/DirectX/OpenGL runtime; this project reimplements it system by system while keeping existing
projects and assets working.

**This is work in progress.** The editor is not usable yet. What exists today:

- `mi-core`: animatable value table, object type tables, colours, save ids.
- `mi-format`: reading and writing of `.miproject`, `.miobject`, `.miframes`, `.miparticles` and
  `.mirender` (JSON formats 24–34), verified against the files bundled with the original.
- `mi-anim`: easing, keyframe interpolation, the transform hierarchy, paths and inverse kinematics.
- `mi-project`: an open project with its timeline tree, open/save and scene evaluation.
- A Tauri application that opens a project and shows its timelines, keyframes and the animated
  values at any frame, with playback. There is no 3D viewport and no editing yet.

[docs/PORTING_STATUS.md](docs/PORTING_STATUS.md) tracks every system of the original and whether it
has been ported.

## Quick start

Requirements: Rust 1.85+, Node 20+, pnpm, and the
[Tauri prerequisites](https://tauri.app/start/prerequisites/) for your platform.

```sh
pnpm install
pnpm tauri dev        # run the application
cargo test --workspace

# No project at hand? Generate a small animated one and open it:
cargo run -p mi-format --example sample_project -- sample.miproject
pnpm tauri dev -- -- "$PWD/sample.miproject"
```

See [docs/BUILD.md](docs/BUILD.md) for details.

## Documentation

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): how the original works and how this rewrite is structured
- [docs/PORTING_STATUS.md](docs/PORTING_STATUS.md): feature tracking and the phased plan
- [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md): intentional and known differences from the original
- [docs/formats.md](docs/formats.md): file formats
- [docs/BUILD.md](docs/BUILD.md): building, running and testing

## Layout

```
crates/       Rust libraries, independent of the user interface
src-tauri/    the application: state, commands, windows
ui/           React + TypeScript frontend
assets/       data files shipped with the program (Minecraft assets, fonts, languages, presets)
docs/         documentation
```
