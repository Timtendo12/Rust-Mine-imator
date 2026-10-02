# Compatibility with the original

The aim is that projects and assets made with Mine-imator 2.0.2 and earlier open here with the same
result, and that projects saved here open in Mine-imator 2.0.2. This page lists where that does not
hold, on purpose or for now.

## Intentional differences

| Area | Difference | Why | Migration |
|---|---|---|---|
| Licence key / trial | Removed. All features are available; the watermark is an ordinary export option. | Decided for the rewrite. | None. `key.midata` is ignored. |
| Video encoding | Done by an `ffmpeg` executable run as a child process instead of FFmpeg linked into the program. | Avoids building FFmpeg and x264 from source on every platform. | None; same containers and codecs. (Not implemented yet.) |
| User interface | HTML instead of the custom immediate-mode UI. Layout and workflows are kept; pixel-exact appearance is not a goal. | Target architecture. | None. |
| JSON reading | Raw control characters inside strings and a UTF-8 byte order mark are accepted. | Files in the wild contain them. | None. |

## File format notes

- Saving always writes format 34 (2.0.0 pre-release 5 and later), like the original.
- Text outside ASCII is written as one `\uXXXX` escape per UTF-16 unit, as the C++ runtime of the
  original does. Characters outside the Basic Multilingual Plane therefore become surrogate pairs.
- Numbers are written with at most five decimals and without trailing zeroes, including the
  original's `-0` for small negative numbers.
- A null object reference inside keyframe values is written as the number `-4`, as in the original.

## Bugs of the original fixed here

Fixes change behaviour in memory, not the layout on disk, so files stay readable by the original.

| Bug in the original | Fix |
|---|---|
| Cloud height keyframes (`BG_SKY_CLOUDS_Z`) of projects older than 2.0.0 are silently dropped on load, because the rename table maps the wrong key. | The value is carried over to `BG_SKY_CLOUDS_HEIGHT`. |
| Leaf colours of projects older than 2.0.0 are only derived from the foliage colour if the project has at least one keyframe (the upgrade is a side effect of loading a keyframe). | Always derived. |
| `part_root` is written for every timeline that is part of scenery but only read back for special blocks. | Read back for all of them. |
| `BG_FOG_OBJECT_COLOR` is missing from the list of colour values, so it is written as a raw colour integer and is not clamped like other colours. | Treated as a colour everywhere; still written as the integer so the original can read it, and `#RRGGBB` is accepted too. |
| A timeline whose `glint_tex` is `"null"` crashes on load. | Falls back to the built-in texture. |
| The inverse kinematics solver tests convergence against the wrong joint and so always runs all 30 rounds. | Stops when the end of the limb is on the target. Results are the same. |
| Models that copy the pose of their parent model stop following paths. | Path following is kept in the pose pass. |
| The tip of an inverted cone keeps an upward normal, so the inside of the cone is lit wrongly near the tip. | The normal is flipped with the rest. |
| Walls test whether a wall stands on top of them with the face data of their eastern neighbour, so a wall only grows tall when that neighbour happens to be solid. | The face above is used. |
| `.blocks` files do not set whether the scenery comes from numeric ids, so whether their stairs and fences connect depends on the file loaded before. | They always count as numeric-id files, which store no connections. |
| Sponge schematics store palette indices as variable length integers, but the original reads one byte per block, so schematics with more than 128 palette entries load wrong blocks. Versions 2 and 3 are refused. | Indices are decoded properly; versions 1 to 3 are read. |

## Quirks of the original kept on purpose

Changing these would alter how existing projects look.

| Quirk | Why it is kept |
|---|---|
| Path lengths are measured about 5% short (a loop meant to sample 0..1 stops at 0.95). | `PATH_OFFSET` keyframes are expressed in these units; correcting the length would move every object that follows a path. |
| `matrix_build` rotates before it scales. | The timeline transform compensates for it ("resize" scaling); results are the same as in the original. |
| Blocks with several models, random plant offsets and the integrity of structures use a random number generator seeded by position. | Kept in spirit: the choice is still stable per position, but it comes from a different generator, so the picked variants differ from the original's. |
| Block templates are drawn turned 90° about Z ("for legacy support"), with the repeat counts of X and Y swapped to match. | Block timelines of existing projects would otherwise point the other way. |
| With UV lock, a rotated block face takes the texture of the face whose direction it ends up in. | Matches what the original shows for rotated, UV-locked blocks. |
| The world is left-handed (Z up; +X is to the left when looking along +Y), and a positive Z rotation turns +X towards -Y. | All saved positions and rotations depend on it. It is what makes Minecraft worlds, whose Y and Z are swapped on import, appear unmirrored. |

## Not yet supported

These are gaps, not decisions. They are tracked in [PORTING_STATUS.md](PORTING_STATUS.md).

- Projects from before 1.1.0 (`.mproj`, `.mani`, binary formats 3–23) cannot be opened. The
  application says so instead of failing silently.
- Upgrades of old JSON projects that need Minecraft assets or the legacy lookup tables are not
  applied yet: numeric block ids (before 1.2.0), renamed models and model states, item texture
  renames, the ground slot of keyframes (before 1.2.5). The data needed for them is kept when
  loading (`loaded_format`, `legacy_block`), so nothing is lost; it is just not translated yet.
- `.meshcache` files written by the original are expected to be regenerated rather than read.
