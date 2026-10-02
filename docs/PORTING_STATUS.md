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
| 4 | Renderer (low quality) and viewport | partial (see below) |
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
| `recent.midata` | done | Read, written and seeded from an installation of the original. Writing `thumbnail.png` is not done. |
| `.milanguage` | done | English loads; switching languages (`languages.midata`, legacy format) not yet |
| `.mimodel` | done | All bundled models parse; block and plane shape meshes incl. bending (blocky and realistic). 3D planes are generated flat. Not yet used by the viewport. |
| Minecraft version manifest (`<version>.midata`) | not started | |
| `legacy.midata` | not started | |
| NBT, `.schematic`, `.nbt` structures, `.blocks` | not started | |
| `.meshcache` | not started | Planned to be regenerated, not read |
| Autosave backups (`.backupN`) | not started | The files themselves load as projects |
| Zipped projects | not started | |

## Project model and editing

| Feature | Status |
|---|---|
| Open project: object lookup by save id, timeline tree (with repair of broken trees), open, save | done |
| Scene evaluation at a frame (values, transforms, active camera) | done, without model data: body parts have no bend or part offsets until assets are loaded |
| Reference counting of resources | not started |
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
| Playback | partial: driven by the frontend clock; looping and regions not yet |

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
| wgpu device and surface under the transparent webview | done (one window; pop-out windows not yet) |
| Low quality mode (flat, shaded): per-vertex sun and point lights, ambient, fog, tonemapping, colour transforms, alpha blending | done |
| Wind, material maps, glint, alpha hashing, alpha-fix pass in the low quality shader | not started |
| Shapes (cube, cone, cylinder, sphere, surface) with texture mapping options | done (untextured until assets load) |
| Sun direction, day/night and twilight colours | done |
| Sky background image, sun and moon discs, stars, clouds | not started |
| Ground | partial: drawn in the grass colour; texture needs the Minecraft assets |
| Models, blocks, scenery, items, text, paths, particles | not started |
| Animated background (background timelines overriding the sky settings) | not started |
| Picking, selection outline, gizmos, grid, overlays | not started |
| Work camera: orbit, pan, zoom | done (zoom is immediate, not eased) |
| Work camera: fly mode (right drag + keys) | not started |
| Timeline cameras (active camera view, FOV, orbit) | done |
| Camera shake | not started |
| Offscreen rendering with pixel readback | done (used by tests; export will build on it) |
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
| Startup screen | partial | Recent projects with thumbnails, sort, remove, browse. No "New project", pinning or list view. |
| Menu bar | partial | File, Edit, Render, View, Help as in the original; only open, close and navigation work, the rest is shown disabled |
| Properties panel | partial | Project settings, render, library, environment and resources sections, read-only |
| Display names of unnamed timelines | done | From type, model part, block or template, through the language file |
| Shortcut bar | partial | Static hints for the viewport |
| Timeline (read-only) | partial | Tree, keyframe tracks, scrubbing, playback at project tempo, values of the selected timeline at the current frame. No editing, markers, regions or audio. |
| Docking panels, pop-out windows | not started | |
| Viewport | partial | One view with flat/shaded mode, work or active camera, mouse orbit/pan/zoom. No second view, tools, overlays or selection. |
| Timeline editing (keyframes, reparenting, markers, regions, audio clips) | not started | |
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
