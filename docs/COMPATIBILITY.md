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
- Keyframes of older projects that used `BG_SKY_CLOUDS_HEIGHT` or `BG_SKY_CLOUDS_Z` lose that value
  on load. The original drops it too.

## Not yet supported

These are gaps, not decisions. They are tracked in [PORTING_STATUS.md](PORTING_STATUS.md).

- Projects from before 1.1.0 (`.mproj`, `.mani`, binary formats 3–23) cannot be opened. The
  application says so instead of failing silently.
- Upgrades of old JSON projects that need Minecraft assets or the legacy lookup tables are not
  applied yet: numeric block ids (before 1.2.0), renamed models and model states, item texture
  renames, the ground slot of keyframes (before 1.2.5). The data needed for them is kept when
  loading (`loaded_format`, `legacy_block`), so nothing is lost; it is just not translated yet.
- `.meshcache` files written by the original are expected to be regenerated rather than read.
