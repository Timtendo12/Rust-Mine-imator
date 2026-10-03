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
| 3 | Assets: Minecraft pack, block and character models, atlases, schematics | partial (see below) |
| 4 | Renderer (low quality) and viewport | partial (see below) |
| 5 | Editor shell: docking, timeline, editors, workbench, undo | partial (see below) |
| 6 | Renderer (high quality) and post effects | not started |
| 7 | Particles, audio, export | partial (see below) |
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
| NBT, `.schematic`, `.nbt` structures, `.blocks` | done | NBT (plain and gzip), MCEdit schematics through the legacy id table, Sponge schematics versions 1–3, structures with palettes, integrity and jigsaw final states, legacy `.blocks`. Tile entity data (sign text, skull skins, banners) is not read yet. |
| `.meshcache` | not started | Planned to be regenerated, not read |
| Autosave backups (`.backupN`) | done | Every 3 minutes, 5 kept, named and rotated as in the original; File > Open last backup. The interval and number are not settings yet |
| Zipped projects | not started | |

## Project model and editing

| Feature | Status |
|---|---|
| Open project: object lookup by save id, timeline tree (with repair of broken trees), open, save | done |
| Scene evaluation at a frame (values, transforms, active camera) | done, without model data: body parts have no bend or part offsets until assets are loaded |
| Reference counting of resources | not started |
| Save id remapping when importing | not started |
| Undo / redo (commands replacing 821 action scripts) | partial: snapshot-based history (100 steps) with merged drags; edits so far: values of the selected keyframes or at the marker (with the keyframe rules of `tl_value_set`), creating, moving, removing, cutting and pasting keyframes, renaming and hiding timelines |
| Selection, copy / paste, duplicate | partial: several timelines can be selected (Ctrl toggles, Shift selects a range in the list or adds in the viewport, Ctrl+A all); value edits, timeline settings, keyframe creation, duplicating, removing and dragging in the list act on all of them, the editors show the last one. Not yet: copy and paste of timelines |
| Timeline operations (add, remove, reparent, parts of models) | partial: creating folders, cameras (at the work camera), lights and shapes (with their own template); removing with children and clearing references (`tl_remove_clean`); duplicating subtrees; reparenting and reordering by dragging in the list. Characters, special blocks (with a timeline per model part in the model's hierarchy), blocks in their default state and items, from a workbench with search. Scenery from a schematic, structure or `.blocks` file (as a resource that is read from where it is and copied next to the project on saving, `new_res` / `res_save`) and text. Not yet: workbench settings (states, skins, previews), timelines for the chests, doors and the like of new scenery, particles, placing in the viewport, copy and paste |
| Keyframe operations (add, move, select, copy, transitions) | partial: creating at the marker (Ctrl+Q), moving, removing, copy / cut / paste (Ctrl+C/X/V, with the free / model / fixed target rules of `tl_keyframes_paste`), editing all selected keyframes at once, transitions from the frame editor. Not yet: bezier handles, box selection, the select before / after marker commands, saving keyframes to `.miframes` |
| Library operations (templates, resources) | partial: resources are added with scenery, shared by file, undone with their timeline and copied with the project when it is saved or saved somewhere else. Not yet: the library and resource tabs (replacing, removing, reloading), reference counting |

## Animation

| Feature | Status |
|---|---|
| Transitions: linear, instant, bezier, 30 easing curves | done |
| Value interpolation rules and clamping | done |
| Keyframe lookup and per-timeline value evaluation, including seamless repeat | done |
| Matrix and vector maths with the original conventions | done |
| Hierarchy, inherit flags, value inheritance | done |
| Timeline matrices (position, rotation, scale, rotation point) | done, including the default rotation point of templates (scenery, blocks, shapes, items, text); block-format models still use 0 |
| Body part bend transform (children locked to the bent half) | done | 
| Bending of the body part meshes themselves | done for blocks and planes, in the blocky and realistic styles (`model_shape_generate_block`, `model_shape_generate_plane`); 3D planes are bent as flat planes |
| Inverse kinematics (two-bone limbs, pole target, blend) | done |
| Paths (spline sampling, frames) and path following | done |
| Inherit pose | partial: implemented, not covered by a test yet |
| Camera orbit | done |
| Camera shake | done: the simplex noise of the original (`simplex_lib`), turning or moving the active camera |
| Playback | done: driven by the frontend clock at the project tempo; repeat and seamless repeat loop the region or the whole animation. Without a repeat mode playback stops at the last keyframe instead of running on |

## Assets

| Feature | Status |
|---|---|
| Minecraft asset pack loading | done (bundled 1.20.2; version switching and downloads not yet) |
| Block states, block models, render models, connected blocks | partial: manifest blocks and ids, state defaults, variant and multipart blockstates, model parents and textures, element and variant rotation, UV lock, weighted variants, face culling between neighbours by texture transparency (opaque, cut-out, translucent; leaves), random offsets of plants. Every state value of the bundled pack resolves. Connected blocks (`block_set_*`) for legacy schematics and repeated templates: stairs corners, fences, panes and bars, walls, fence gates, doors and double plants (both halves), beds, snowy blocks, fire, vines, tripwire, chorus plants, repeaters, redstone wire (with its colour and glow), kelp and other vines, dripstone, big dripleaf. Water and lava (levels, flow direction of the surface texture, waterlogged blocks, the wave gap rule). Not yet: opaque leaves setting, wind and liquid waves in the shader, animated textures beyond the first frame, wind and subsurface values, resource pack block textures. |
| Character and special block models (`.mimodel`), states | done: states choose files, textures and hidden parts/shapes; drawn in the viewport with bending. Armour, patterns (banners), model colour palettes and 3D planes not yet. |
| Texture atlases, animated textures, biome tints | partial: block textures are drawn one by one rather than from a sheet; animated ones (water, lava, fire, ...) follow their `.mcmeta` in the 64 frame loop of the original, at the project's texture animation speed; tints by the project's biome colours. Not yet: the opaque and no-alpha variants of the sheet, animated ground |
| Resource packs, material and normal maps | not started |
| Skins (including download and old 64×32 layout) | not started |
| Items, item sheets | partial: items from textures of the pack (`item/...`), flat or extruded; item sheets of resources not yet |
| Fonts and text meshes | partial: the Minecraft sprite font and its text meshes; TrueType fonts of resources not yet |
| Scenery from schematics and structures, block entities as timelines | partial: scenery files of a project are read and drawn, blocks that are timelines in the project (chests, doors, ...) are left out of the mesh. Not yet: creating those timelines when scenery is added, scenery from worlds, `.meshcache`, the "remove edges" setting, meshing on a background thread. |
| World import (all chunk formats, preview, selection) | not started |

## Rendering

| Feature | Status |
|---|---|
| wgpu device and surface under the transparent webview | done (one window; pop-out windows not yet) |
| Low quality mode (flat, shaded): per-vertex sun and point lights, ambient, fog, tonemapping, colour transforms, alpha blending | done |
| Wind, material maps, glint, alpha hashing, alpha-fix pass in the low quality shader | not started |
| Shapes (cube, cone, cylinder, sphere, surface) with texture mapping options | done (untextured until assets load) |
| Sun direction, day/night and twilight colours | done |
| Sky background image, sun and moon discs, stars, clouds | partial: the haze dome at the horizon, stars at night, the sun and the moon (phase, angle, scale) drawn additively, clouds in the normal, flat and faded modes drifting with the animation, the fog colour of `background_sky_update` and the twilight glow towards the rising or setting sun. Not yet: background images (flat, sphere, box), sun, moon and cloud textures of resources |
| Ground | done: pack texture tinted by biome colour (resource pack ground textures not yet) |
| Characters and special blocks | done (body parts, textured, bent) |
| Blocks | done for block templates (with repeat and randomised variants) and block timelines of scenery; one mesh per texture, tinted like the original |
| Scenery | done (with repeat) |
| Text | partial: text timelines in the Minecraft font (the original's sprite font, shipped as `Data/Fonts/minecraft.png`) with alignment and line breaks. Not yet: other fonts, 3D text, outlines, facing the camera |
| Items | partial: item timelines from pack textures, flat or extruded pixel by pixel (`render_generate_item`, `vbuffer_add_pixels`), and a keyframe's custom item. Not yet: item sheets of resources, facing the camera, spinning and bouncing |
| Paths, particles | not started |
| Animated background (background timelines overriding the sky settings) | not started |
| Picking, selection outline, gizmos, grid, overlays | partial: clicking selects (`view_click`: the outermost unlocked timeline first, the clicked part once something is selected or with Ctrl; selected and locked timelines let clicks through; lights and cameras by a box); the selection and everything below it gets the white border of `render_select`. The selected timeline has the move arrows of `view_control_move` and the rotation rings of `view_control_rotate`, switched with the Move/Rotate tool; a drag is one undo step. Not yet: move planes, scale and bend controls, snapping, grid, overlays |
| Work camera: orbit, pan, zoom | done (zoom is immediate, not eased) |
| Work camera: fly mode (right drag + keys) | not started |
| Timeline cameras (active camera view, FOV, orbit) | done |
| Camera shake | done |
| Offscreen rendering with pixel readback | done (used by tests, thumbnails and export) |
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
| Image export | partial: Render > Export image (F10) saves the current frame as PNG at the project's size through the active camera, with the low quality renderer. Not yet: high quality, the options to include hidden objects, remove the background or add the watermark |
| Image sequence and video export with audio | partial: Render > Export animation renders every frame at the export frame rate through the active camera into numbered PNG files (named as the original names them) or, through `ffmpeg`, an mp4, mov or wmv video with the original's qualities; progress and stopping. The region is exported when there is one. Not yet: audio, high quality, hidden objects, removing the background, watermark, remembering the settings |

## User interface

| Feature | Status | Notes |
|---|---|---|
| Startup screen | partial | Recent projects with thumbnails, sort, remove, browse, new project. No pinning or list view. |
| Menu bar | partial | File, Edit, Render, View, Help as in the original; items of features that are not ported are shown disabled |
| Properties panel | partial | Project settings (name, author, description, render size, tempo), render settings (samples, distance, main effects), environment (time, rotation, clouds, ground, twilight, fog, wind, scene colours, texture speed) are editable with undo; library and resources are read-only. The selected timeline has a frame editor with every number, switch, colour, choice and text value its type supports (position, rotation, scale, bend, colour, surface, light, camera effects, background, text, visibility and transition), grouped like the original's; values that point at other objects (textures, paths, IK targets, sounds) and the bezier handles are not editable yet. Its settings that are not animated can be switched too: what it inherits from its parent, appearance options, and lock and hide flags |
| Display names of unnamed timelines | done | From type, model part, block or template, through the language file |
| Shortcut bar | partial | Static hints for the viewport |
| Timeline | partial | Tree, keyframe tracks, scrubbing, playback at project tempo, keyframe selection (Shift/Ctrl to add), dragging and deleting keyframes, renaming (double click) and hiding timelines. Copy, cut and paste of keyframes (pasting at the frame under the mouse). Region (right drag on the ruler, edges dragged), repeat modes, markers (added at the playhead, dragged, edited by double click). No audio or box selection. |
| Docking panels, pop-out windows | not started | |
| Viewport | partial | One view with flat/shaded mode, work or active camera, mouse orbit/pan/zoom, click selection, move and rotate tools. No second view or overlays. |
| Timeline editing (keyframes, reparenting, markers, regions, audio clips) | partial | Keyframes, reparenting, markers and regions; audio clips not yet |
| New project | done | File > New project (Ctrl+N), saved with save as |
| Saving from the editor | partial | Save and save as (Ctrl+S, Ctrl+Shift+S), unsaved-changes mark in the title and a question before closing. Saving writes `thumbnail.png` (240 x 180, work camera) and moves the project to the top of the recent list, as `recent_add` does. Backups and autosave are not written yet |
| Properties tabs (project, render, library, background, resources) | partial | Project, render and background settings are edited in the properties panel; library and resources only list what the project has |
| Template editor, timeline editor, frame editor | partial | Frame editor: all number, switch, colour, choice and text values of the selected timelines, by group. Timeline editor: inherit, appearance and lock switches. Not yet: values that refer to other objects (textures, paths, IK targets, sounds), the bezier curve editor, the template editor |
| Ground editor | not started | |
| Settings (program, interface, controls) | not started | |
| Workbench and placing objects | partial | Workbench with search for basic objects, text, scenery files, characters, special blocks, blocks and items; new objects appear at the origin instead of being placed with the mouse |
| Toolbar menus, context menus, tooltips, toasts, shortcut bar | not started | |
| Popups (new project, save as, export, skin download, pattern and armour editors, ...) | partial | Export animation; saving and opening use the system dialogs |
| Themes and accent colours, interface scale | not started | |
| Translations | not started | |
| Key bindings | not started | |
| Recent projects, autosave, asset version updates | partial | Recent projects are kept and shown, changed projects are backed up; asset updates not yet |
| Crash / error reporting, log file | not started | |

## Dropped on purpose

See [COMPATIBILITY.md](COMPATIBILITY.md): licence key and trial restrictions.
