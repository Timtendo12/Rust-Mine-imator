# Porting status

Every system of the original and where it stands. Nothing here is "done" unless it is implemented
and tested; partial work says what is missing.

Legend: **done** · **partial** · **not started**

## Phases

| # | Phase | Status |
|---|---|---|
| 0 | Repository, workspace, Tauri + React shell, docs | done |
| 1 | Core tables and file formats | partial (see below) |
| 2 | Animation: value evaluation, easing, hierarchy, matrices, bend, IK, paths | partial (see below) |
| 3 | Assets: Minecraft pack, block and character models, atlases, schematics | not started |
| 4 | Renderer (low quality) and viewport | not started |
| 5 | Editor shell: docking, timeline, editors, workbench, undo | not started |
| 6 | Renderer (high quality) and post effects | not started |
| 7 | Particles, audio, export | not started |
| 8 | World import and remaining tools | not started |
| 9 | Hardening and performance | not started |

## Core and formats

| Feature | Status | Notes |
|---|---|---|
| Animatable value table (207 values, names, kinds, defaults) | done | `mi-core/src/value.rs` |
| Template / timeline / resource type tables | done | `mi-core/src/types.rs` |
| Value groups per timeline type (`tl_update_value_types`) | done | |
| JSON reader and layout-exact writer | done | `mi-format/src/json.rs` |
| `.miproject` load and save (format 34) | done | Round-trip tested on a synthetic project covering every object kind. Not yet tested on real user projects: none were available. |
| `.miproject` upgrades from formats 24–33 that need only the file | done | Value renames, background changes, particle changes, alpha mode, leaf colours, camera shake |
| `.miproject` upgrades that need assets or legacy tables | not started | Numeric block ids (< 1.2.0), renamed models and states, item texture renames, model version updates, ground slot keyframe fix (< 1.2.5), `part_root` (< 1.2.2), legacy bend axis (< 1.1.3) |
| `.miobject` | done | Load and save; merging into an open project (id remapping) belongs to `mi-project` |
| `.miframes` | done | 94 bundled loops load; applying them to timelines belongs to `mi-project` |
| `.miparticles` | done | 14 bundled presets reproduced byte for byte |
| `.mirender` | done | 3 bundled presets reproduced byte for byte |
| Legacy binary projects (`.mproj`, `.mani`, formats 3–23) | not started | `project_load_legacy*`, needs `legacy.midata` |
| Legacy `.object`, `.keyframes`, `.particles` | not started | |
| `settings.midata` (and legacy settings) | not started | |
| `recent.midata`, thumbnails | not started | |
| `.milanguage`, `languages.midata` | not started | |
| `.mimodel` | not started | |
| Minecraft version manifest (`<version>.midata`) | not started | |
| `legacy.midata` | not started | |
| NBT, `.schematic`, `.nbt` structures, `.blocks` | not started | |
| `.meshcache` | not started | Planned to be regenerated, not read |
| Autosave backups (`.backupN`) | not started | The files themselves load as projects |
| Zipped projects | not started | |

## Project model and editing

| Feature | Status |
|---|---|
| Project state, object tree, reference counting | not started |
| Save id remapping when importing | not started |
| Undo / redo (commands replacing 821 action scripts) | not started |
| Selection, copy / paste, duplicate | not started |
| Timeline operations (add, remove, reparent, parts of models) | not started |
| Keyframe operations (add, move, select, copy, transitions) | not started |
| Library operations (templates, resources) | not started |

## Animation

| Feature | Status |
|---|---|
| Transitions: linear, instant, bezier, 30 easing curves | done |
| Value interpolation rules and clamping | done |
| Keyframe lookup and per-timeline value evaluation, including seamless repeat | done |
| Matrix and vector maths with the original conventions | done |
| Hierarchy, inherit flags, value inheritance | done |
| Timeline matrices (position, rotation, scale, rotation point) | done |
| Body part bend transform (children locked to the bent half) | done | 
| Bending of the body part meshes themselves | not started |
| Inverse kinematics (two-bone limbs, pole target, blend) | done |
| Paths (spline sampling, frames) and path following | done |
| Inherit pose | partial: implemented, not covered by a test yet |
| Camera orbit | done |
| Camera shake | not started |
| Playback, looping, regions, seamless repeat | not started |

## Assets

| Feature | Status |
|---|---|
| Minecraft asset pack loading and version switching | not started |
| Block states, block models, render models, connected blocks | not started |
| Character and special block models (`.mimodel`), states, armour, patterns | not started |
| Texture atlases, animated textures, biome tints | not started |
| Resource packs, material and normal maps | not started |
| Skins (including download and old 64×32 layout) | not started |
| Items, item sheets | not started |
| Fonts and text meshes | not started |
| Scenery from schematics and structures, block entities as timelines | not started |
| World import (all chunk formats, preview, selection) | not started |

## Rendering

| Feature | Status |
|---|---|
| wgpu device, surface under the webview, multiple windows | not started |
| Low quality mode (flat, shaded) | not started |
| Sky, sun and moon, clouds, ground, fog | not started |
| Models, blocks, scenery, items, text, shapes, paths, particles | not started |
| Picking, selection outline, gizmos, grid, overlays | not started |
| Work camera (orbit, pan, fly) and timeline cameras | not started |
| High quality mode: shadows, SSAO, indirect light, reflections, subsurface, glint | not started |
| Post effects: DOF, glow, bloom, lens dirt, CA, distort, colour correction, grain, vignette | not started |
| Tonemapping, alpha modes, material formats | not started |
| Debug render passes | not started |

## Particles, audio, export

| Feature | Status |
|---|---|
| Particle simulation (deterministic, seeded) | not started |
| Particle editor | not started |
| Audio decoding, playback, waveforms | not started |
| Image export | not started |
| Image sequence and video export with audio | not started |

## User interface

| Feature | Status | Notes |
|---|---|---|
| Application window, start page | partial | Opens a project file and lists its content; no editing |
| Docking panels, pop-out windows | not started | |
| Viewports and view toolbar | not started | |
| Timeline (tree, dope sheet, markers, regions, audio clips) | not started | |
| Properties tabs (project, render, library, background, resources) | not started | |
| Template editor, timeline editor, frame editor | not started | |
| Ground editor | not started | |
| Settings (program, interface, controls) | not started | |
| Workbench and placing objects | not started | |
| Toolbar menus, context menus, tooltips, toasts, shortcut bar | not started | |
| Popups (new project, save as, export, skin download, pattern and armour editors, ...) | not started | |
| Themes and accent colours, interface scale | not started | |
| Translations | not started | |
| Key bindings | not started | |
| Recent projects, autosave, asset version updates | not started | |
| Crash / error reporting, log file | not started | |

## Dropped on purpose

See [COMPATIBILITY.md](COMPATIBILITY.md): licence key and trial restrictions.
