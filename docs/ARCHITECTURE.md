# Architecture

Two parts: how the original program works (the behaviour being reproduced), and how this rewrite
is structured.

## 1. The original

Mine-imator 2.0 is about 2,000 GML scripts (`GmProject/scripts`) that a custom transpiler (`CppGen`)
converts to C++. The generated code runs on a hand-written runtime (`CppProject`) built on Qt 5 with
Direct3D 11 on Windows and OpenGL elsewhere, plus FFmpeg, OpenAL and libzip.

```
GmProject/scripts/*.gml ──CppGen──► CppProject/Generated/*.cpp
                                        │ calls
        ┌───────────────────────────────┴──────────────────────────────┐
        │ CppProject runtime (Qt main thread, 60 Hz timer)             │
        │  Type/    dynamic values, interned strings, lists/maps by id │
        │  Gml/     381 GML built-in functions                         │
        │  Render/  D3D11 or OpenGL, batching, surfaces, GLSL → HLSL   │
        │  World/   region/NBT/chunk meshing, scenery builder (OpenMP) │
        │  Library/ FFmpeg video+audio, unzip, file ops, simplex noise │
        └──────────────────────────────────────────────────────────────┘
frame: app_event_step (app_update_*) → app_event_draw → window_draw, once per OS window
```

### Systems

| System | Scripts | Responsibility |
|---|---|---|
| Actions | 821 | One script per user action; each contains its own do, undo and redo branch |
| Interface | 333 | Immediate-mode UI: panels, tabs, widgets, menus, popups, timeline, viewports |
| Project | 235 | Templates, timelines, keyframes, resources, particles, save and load |
| Render | 124 (+49 shaders) | Low quality forward pass, high quality progressive renderer, post effects |
| Minecraft | 120 | Asset pack loading, block states and models, character models, NBT |
| Utility | 265 | Maths, files, textures, vertex buffers, strings |
| Native only | 143 functions | Vector/matrix maths, shader uniforms, surfaces, windows, threads, scenery mesher, world importer |

### Data model

A singleton `app` holds all state. The library holds **templates** (`obj_template`: a character,
block, item, shape, text, particle spawner, ...). The scene is a tree of **timelines**
(`obj_timeline`), most of them instances of a template; cameras, lights, folders, audio, paths and
the background exist only as timelines. Each timeline has **keyframes** (`obj_keyframe`), each
holding a full array of about 200 animatable **values** (`e_value`). **Resources** (`obj_resource`)
are external files: skins, textures, resource packs, schematics, sounds, fonts, models. Objects
refer to each other by 16-character save ids.

### Animation

`tl_update_values` finds the keyframe before and after the current frame and interpolates each
value with the transition stored on the earlier keyframe (linear, instant, bezier, or one of 30
easing curves). Booleans, references and text do not interpolate. `tl_update_matrix` then walks the
tree in order and builds each timeline's matrix from position, rotation and scale, the parent's
matrix (subject to 14 `inherit_*` flags), body part bending, inverse kinematics (FABRIK over three
joints), path following and camera orbit. Material values accumulate down the tree.

Time is counted in integer frames at `project_tempo` frames per second.

### Rendering

Coordinates are Z-up and left-handed with depth 0..1; matrices are row-vector with rotation order
`Rz · Rx · Ry`.

- **Low quality** (`render_low`): one forward pass, unlit or lit per vertex by up to 64 lights.
- **High quality** (`render_high`): progressive. Each frame adds one sample with a sub-pixel jitter
  (default 24 samples). Per sample: diffuse, depth + normal and material passes; shadow maps (three
  sun cascades, spot, point cube atlas) with the light position jittered for soft shadows;
  subsurface blur; screen-space indirect light; SSAO; enchantment glint; lighting composite in HDR;
  screen-space reflections; tonemapping; fog; depth of field and glow. After the last sample: bloom,
  lens dirt, chromatic aberration, distortion, colour correction, grain, vignette, overlay.
- Transparency is either unsorted blending in depth-list order followed by an alpha repair pass, or
  hashed (stochastic discard that converges over samples).
- Materials use roughness, metallic and emissive, from uniforms or from SEUS / LabPBR texture maps.
- Picking renders object ids as colours; the selection outline is a mask plus an edge shader;
  gizmos are drawn in 2D from projected points.

### Assets

`Data/Minecraft/<version>.zip` holds vanilla block states, block models and textures plus
Mine-imator's own character models (`.mimodel`) and animation loops (`.miframes`). The matching
`.midata` manifest lists characters, special blocks, blocks, textures, biomes and patterns. Blocks
are meshed from block state and model JSON with hand-written rules for connecting blocks. Scenery
comes from `.schematic` / `.nbt` / `.blocks` files or from a world save, meshed on several threads
and cached.

### User interface

Entirely custom and immediate-mode. Six docks plus pop-out windows; tabs for properties, ground
editor, template editor (with the particle editor), timeline, timeline editor, frame editor and
settings; a toolbar with menus; the workbench for creating objects; 16 popups; context menus,
tooltips, toasts, a shortcut bar; three themes with nine accent colours; translations; rebindable
keys.

### Export

For each output frame the timeline marker is advanced, the high quality renderer runs until all
samples are in, and the frame goes to FFmpeg (mp4, mov, wmv with x264 and mixed audio) or to a PNG.

## 2. The rewrite

### Principles

- **Rust owns the state.** The project, selection, playback position and undo history live in the
  backend. The frontend holds view state only (scroll positions, open panels, hover) and sends
  typed commands; the backend answers with state updates.
- **Libraries do not know about the UI.** Everything under `crates/` can be built and tested
  without Tauri or a GPU window.
- **The renderer does not know about timelines.** The animation layer produces an immutable scene
  snapshot (matrices, material values, lights, camera, background); the renderer draws it.
- **Behaviour is translated, not syntax.** For example the 821 self-undoing action scripts become
  command objects with `apply` and `revert`; the dynamic `ds_map` trees become typed structs.

### Crates

| Crate | Status | Responsibility |
|---|---|---|
| `mi-core` | exists | Save ids, colours, the animatable value table, object type tables, format version numbers |
| `mi-format` | exists (JSON project family) | File formats: read, write, upgrade old versions |
| `mi-project` | planned | Project model on top of `mi-format`: object tree, references, commands and undo |
| `mi-anim` | exists (values and maths) | Value evaluation, easing, hierarchy, matrices, bend, IK, paths |
| `mi-assets` | planned | Minecraft asset pack, block and character models, atlases, resource packs, NBT, schematics, scenery mesher |
| `mi-world` | planned | World saves: `level.dat`, regions, chunks of every supported version, preview mesh |
| `mi-particles` | planned | Deterministic particle simulation |
| `mi-render` | planned | wgpu renderer: low and high quality pipelines, post effects, picking, overlays |
| `mi-audio` | planned | Decoding, playback, waveforms, mixdown for export |
| `mi-export` | planned | Image, image sequence and video export |
| `src-tauri` | exists (shell) | Application state, commands and events, windows, dialogs |
| `ui` | exists (shell) | React + TypeScript frontend |

Dependencies point one way:

```
mi-core ← mi-format ← mi-project ← mi-anim ← mi-render / mi-particles / mi-audio ← mi-export ← src-tauri ← ui
                          ↑             ↑          ↑
                      mi-assets ────────┴──────────┘        mi-world → mi-assets
```

### Viewport

The wgpu renderer draws straight to the surface of the OS window; the webview on top is transparent
where the viewports are. The frontend reports the viewport rectangles and forwards pointer input in
them; camera control, picking and gizmo dragging happen in Rust. Popped-out views are additional
windows with their own surface.

### Format layer design (`mi-format`)

- `json.rs` contains a tolerant reader and a writer that reproduces the original's layout exactly,
  so files stay diff-friendly and readable by the original.
- Settings structs are declared with the `record!` macro (`record.rs`): one table gives the JSON
  key, the type, the default and the order, and generates the struct, `Default`, the writer and the
  reader. Save and load therefore cannot drift apart.
- Upgrades of older formats that need only the file are applied while loading. Upgrades that need
  Minecraft assets or the legacy tables are left to `mi-project`; the loader keeps what they need.
- Values are stored fully resolved per keyframe (as in the original), and written as differences
  from the timeline's defaults.

### Verification

- Unit tests next to the code for every table and conversion.
- `crates/mi-format/tests/bundled_files.rs` reproduces the files shipped with the original byte for
  byte (render presets, particle presets) and loads all bundled animation loops.
- Planned: reference tables for easing and matrices computed from the original formulas, block
  model resolution over the whole asset pack, image comparisons for the renderer.
