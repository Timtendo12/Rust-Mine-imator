# File formats

Reference for the formats the original reads and writes, as implemented in `GmProject/scripts`
(`project_save*`, `project_load*`, `json_save_*`). Implemented here in `crates/mi-format`.

## JSON conventions

All project-family files are JSON written by a small streaming writer with a fixed layout:

- Tab indentation, CRLF line endings, no trailing newline.
- `"name": value` with one space after the colon.
- Short arrays are written inline as `[ a, b, c ]`; arrays of objects one element per line.
- Numbers: integers without a fraction, otherwise up to five decimals with trailing zeroes removed.
- Booleans are `true` / `false` where the original uses `json_save_var_bool`, and `1` / `0` where
  it passes a flag through the generic writer (for example `render_shadows_transparent`,
  `destroy_at_bounding_box`, `angle_speed_israndom`, `hide_color_tag`). Readers accept both.
- Colours are `"#RRGGBB"`.
- 3D points are `[X, Z, Y]`: Y and Z are swapped on disk because the program is Z-up and files
  follow Minecraft's Y-up order.
- Strings escape `\n`, `\t`, `"`, `\` and everything above U+007F as `\uXXXX`.
- References between objects are save ids: 16 characters from `0-9A-Z`. Reserved ids: `"root"`
  (top of the timeline tree), `"default"` (the built-in Minecraft resource pack), `"null"`.

Every file starts with `format` (integer) and `created_in` (program version string).

## Format numbers

| Number | Version | Notes |
|---|---|---|
| 1–14 | 0.1 – 1.0.6 | binary (`.mproj`, `.mani`) |
| 20–23 | Community Build 1.0.0 – 1.0.3 | binary |
| 24 | 1.1.0 pre 1 | first JSON format |
| 25, 26 | 1.1.0 pre 3, 1.1.0 | |
| 27 | 1.1.3 | per-axis bend angles |
| 28, 29 | 1.2.0 pre 1 / pre 3 | block names instead of ids; more background keyframes |
| 30 | 1.2.2 | `part_root` |
| 31 | 1.2.3 pre 2 | particle `temp_type`, launch angle |
| 32 | 1.2.5 | |
| 33 | 2.0.0 pre 1 | |
| 34 | 2.0.0 pre 5 – 2.0.2 | current |

## `.miproject`

```
format, created_in
project      name, author, description, video size, tempo, grid, view cameras,
             timeline { repeat, intervals, marker, zoom, region, hide_color_tag },
             work_camera { focus, angle_xy, angle_z, roll, zoom }
render       about 50 render settings (same content as .mirender)
background   sky, sun and moon, clouds, ground, biome colours, fog, wind
templates[]  library items; settings depend on "type"
timelines[]  scene objects with default_values, keyframes and hierarchy
resources[]  files used by the project, relative to the project folder
markers[]    optional
```

Timelines store only the values that differ from the project defaults in `default_values`, and each
keyframe (`keyframes: { "<frame>": { VALUE_NAME: value } }`) stores only what differs from the
timeline's defaults. The value names are listed in `crates/mi-core/src/value.rs`.

Which timeline settings are written depends on the timeline type (`tl_update_value_types`):
hierarchy settings (`lock_bend`, `inherit`, `scale_resize`) for everything except audio and
background; `rot_point*` for everything except particle spawners, cameras, lights, audio, background
and path points; appearance settings for everything except
cameras, audio, background and path points; `path` for paths.

## Other project-family files

| Extension | Content |
|---|---|
| `.miobject` | header, `templates`, `timelines`, `resources` of a selection |
| `.miframes` | header, `is_model`, `tempo`, `length`, `keyframes[{position, part_name?, values}]`, then objects. Positions are relative to the first keyframe and rescaled to the project tempo on import. |
| `.miparticles` | header, `particles` (one spawner), then objects |
| `.mirender` | header, `render` |
| `.backupN` | a `.miproject` written by autosave |

## Formats not implemented yet

| Extension | Content |
|---|---|
| `.mproj`, `.mani`, `.object`, `.keyframes`, `.particles` | binary predecessors of the files above |
| `.mimodel` | Modelbench model (JSON, Y-up) |
| `.midata` | settings, recent files, language list, legacy lookup tables, Minecraft version manifest |
| `.milanguage` | translations: nested JSON, keys ending in `/` are prefixes |
| `.schematic`, `.nbt`, `.blocks` | scenery sources |
| `.meshcache` | cached scenery mesh |
| region files (`.mca`, `.mcr`) | Minecraft worlds |
