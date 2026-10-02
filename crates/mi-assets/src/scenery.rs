//! Scenery: grids of blocks read from schematics, structures and `.blocks`
//! files (`res_load_scenery`, `builder_read_schematic`,
//! `builder_read_schematic_nbt`, `builder_read_blocks_file`), and their
//! meshes.
//!
//! Grids use the builder's axes: X is Minecraft's X, Y is Minecraft's Z
//! and Z is Minecraft's Y (up).

use crate::legacy::LegacyBlocks;
use crate::nbt::{self, Compound, NbtError, Tag};
use crate::pack::parse_state_vars;
use crate::{build_grid, AssetPack, Blocks, Grid, GridBlock, GridSource};
use mi_mesh::MeshData;

/// Largest number of blocks a scenery may hold.
const MAX_BLOCKS: usize = 1 << 28;

#[derive(Debug, thiserror::Error)]
pub enum SceneryError {
    #[error("could not read the file: {0}")]
    Nbt(#[from] NbtError),
    #[error("not a schematic: {0}")]
    Invalid(&'static str),
    #[error("unsupported format: {0}")]
    Unsupported(String),
}

/// A block of the scenery's palette.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneryBlock {
    pub block: String,
    /// Every declared state of the block.
    pub state: Vec<(String, String)>,
    pub waterlogged: bool,
}

/// Choices made when reading structures.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneryOptions {
    /// Which of several palettes of a structure to use.
    pub palette: usize,
    /// Share of the blocks of a structure that is kept, 0..1.
    pub integrity: f64,
    pub integrity_invert: bool,
}

impl Default for SceneryOptions {
    fn default() -> Self {
        Self { palette: 0, integrity: 1.0, integrity_invert: false }
    }
}

/// A grid of blocks.
#[derive(Debug, Clone, PartialEq)]
pub struct Scenery {
    pub size: [usize; 3],
    /// Palette index + 1 per position; 0 is air.
    cells: Vec<u32>,
    pub palette: Vec<SceneryBlock>,
    /// Name of the map the schematic was taken from, if it says.
    pub file_map: String,
    /// Read from numeric block ids, so connections between blocks are not
    /// stored and are worked out when building.
    pub legacy: bool,
}

impl Scenery {
    fn new(size: [i64; 3]) -> Result<Self, SceneryError> {
        if size.iter().any(|&s| s <= 0) {
            return Err(SceneryError::Invalid("size cannot be 0"));
        }
        let size = size.map(|s| s as usize);
        let total = size[0].checked_mul(size[1]).and_then(|v| v.checked_mul(size[2]));
        let total = total.filter(|&t| t <= MAX_BLOCKS).ok_or(SceneryError::Invalid("too large"))?;
        Ok(Self { size, cells: vec![0; total], palette: Vec::new(), file_map: String::new(), legacy: false })
    }

    fn index(&self, p: [usize; 3]) -> usize {
        (p[2] * self.size[1] + p[1]) * self.size[0] + p[0]
    }

    /// The palette index of the block at a position.
    pub fn cell(&self, p: [usize; 3]) -> Option<usize> {
        if (0..3).any(|i| p[i] >= self.size[i]) {
            return None;
        }
        self.cells[self.index(p)].checked_sub(1).map(|i| i as usize)
    }

    pub fn get(&self, p: [usize; 3]) -> Option<&SceneryBlock> {
        self.palette.get(self.cell(p)?)
    }

    /// Reads a file by its extension (`.schematic`, `.nbt` or `.blocks`).
    pub fn read(
        bytes: &[u8],
        extension: &str,
        blocks: &Blocks,
        legacy: &LegacyBlocks,
        options: SceneryOptions,
    ) -> Result<Self, SceneryError> {
        match extension.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "schematic" | "schem" => {
                let (_, root) = nbt::read(bytes)?;
                // Version 3 schematics keep everything below "Schematic".
                let map = root.compound("Schematic").unwrap_or(&root);
                Self::read_schematic(map, blocks, legacy)
            }
            "nbt" => {
                let (_, root) = nbt::read(bytes)?;
                Self::read_structure(&root, blocks, options)
            }
            "blocks" => Self::read_blocks_file(bytes, legacy),
            other => Err(SceneryError::Unsupported(other.to_owned())),
        }
    }

    fn read_schematic(map: &Compound, blocks: &Blocks, legacy: &LegacyBlocks) -> Result<Self, SceneryError> {
        let (Some(width), Some(length), Some(height)) = (map.int("Width"), map.int("Length"), map.int("Height")) else {
            return Err(SceneryError::Invalid("size not fully defined"));
        };
        // Sizes are unsigned shorts stored as signed ones.
        let unsigned = |v: i64| if v < 0 { v + 65536 } else { v };
        let mut scenery = Self::new([unsigned(width), unsigned(length), unsigned(height)])?;
        let total = scenery.cells.len();

        let version3 = map.compound("Blocks");
        let palette_map = version3.and_then(|b| b.compound("Palette")).or_else(|| map.compound("Palette"));
        if let Some(palette_map) = palette_map {
            // Sponge schematics, versions 1 to 3.
            let version = map.int("Version").ok_or(SceneryError::Invalid("version not available"))?;
            if version > 3 {
                return Err(SceneryError::Unsupported(format!("schematic version {version}")));
            }
            let mut lookup: Vec<Option<u32>> = Vec::new();
            for (key, index) in palette_map.iter() {
                let Some(index) = index.as_int().filter(|&i| (0..1 << 24).contains(&i)) else { continue };
                let index = index as usize;
                if lookup.len() <= index {
                    lookup.resize(index + 1, None);
                }
                lookup[index] = scenery.palette_entry(blocks, key);
            }

            let data = version3
                .and_then(|b| b.get("Data"))
                .or_else(|| map.get("BlockData"))
                .ok_or(SceneryError::Invalid("BlockData array not found"))?;
            let indices: Vec<usize> = match data {
                Tag::ByteArray(bytes) => decode_varints(bytes, total),
                Tag::IntArray(ints) => ints.iter().map(|&v| v.max(0) as usize).collect(),
                _ => return Err(SceneryError::Invalid("BlockData array not found")),
            };
            for (cell, index) in scenery.cells.iter_mut().zip(indices) {
                // Index 0 is air in the original; real files put air there.
                if index > 0 {
                    if let Some(Some(entry)) = lookup.get(index) {
                        *cell = entry + 1;
                    }
                }
            }
            if let Some(from) = map.compound("Metadata").and_then(|m| m.string("FromMap")) {
                scenery.file_map = from.to_owned();
            }
        } else {
            // MCEdit schematics with numeric ids.
            scenery.legacy = true;
            let ids = map.bytes("Blocks").ok_or(SceneryError::Invalid("Blocks array not found"))?;
            let data = map.bytes("Data").ok_or(SceneryError::Invalid("Data array not found"))?;
            let mut entries: std::collections::HashMap<(u8, u8), Option<u32>> = Default::default();
            for i in 0..total.min(ids.len()).min(data.len()) {
                let (id, value) = (ids[i], data[i] & 15);
                if id == 0 || !legacy.is_known(id) {
                    continue;
                }
                let entry = *entries.entry((id, value)).or_insert_with(|| {
                    let found = legacy.get(id, value)?;
                    Some(scenery.add(SceneryBlock { block: found.block.clone(), state: found.state.clone(), waterlogged: false }))
                });
                if let Some(entry) = entry {
                    scenery.cells[i] = entry + 1;
                }
            }
            if let Some(from) = map.string("FromMap") {
                scenery.file_map = from.to_owned();
            }
        }
        Ok(scenery)
    }

    fn read_structure(root: &Compound, blocks: &Blocks, options: SceneryOptions) -> Result<Self, SceneryError> {
        let version = root.int("DataVersion").unwrap_or(1);
        if version < 2000 {
            return Err(SceneryError::Unsupported("structure version too low".into()));
        }
        let size = root.get("size").and_then(Tag::as_ints).filter(|s| s.len() == 3);
        let size = size.ok_or(SceneryError::Invalid("size not defined"))?;
        let mut scenery = Self::new([size[0], size[2], size[1]])?;

        let palette = match root.list("palettes").filter(|p| !p.is_empty()) {
            Some(palettes) => match &palettes[options.palette % palettes.len()] {
                Tag::List(list) => list.as_slice(),
                _ => &[],
            },
            None => root.list("palette").ok_or(SceneryError::Invalid("palette not found"))?,
        };
        let mut lookup = Vec::with_capacity(palette.len());
        for entry in palette {
            let Some(entry) = entry.as_compound() else {
                lookup.push(None);
                continue;
            };
            let name = entry.string("Name").unwrap_or("");
            let mut vars: Vec<(String, String)> = Vec::new();
            if let Some(properties) = entry.compound("Properties") {
                let mut properties: Vec<(&str, &Tag)> = properties.iter().collect();
                properties.sort_by_key(|(k, _)| *k);
                for (key, value) in properties {
                    if let Some(value) = value.as_str() {
                        vars.push((key.to_owned(), value.to_owned()));
                    }
                }
            }
            lookup.push(scenery.block_entry(blocks, name, &vars));
        }

        let list = root.list("blocks").ok_or(SceneryError::Invalid("block list not found"))?;
        for entry in list.iter().filter_map(Tag::as_compound) {
            let Some(pos) = entry.get("pos").and_then(Tag::as_ints).filter(|p| p.len() == 3) else { continue };
            let p = [pos[0], pos[2], pos[1]];
            if (0..3).any(|i| p[i] < 0 || p[i] as usize >= scenery.size[i]) {
                continue;
            }
            let p = p.map(|v| v as usize);
            let index = scenery.index(p);
            if !keep_for_integrity(index, options) {
                continue;
            }
            let mut block = entry.int("state").and_then(|s| lookup.get(s as usize).copied().flatten());
            // Jigsaw blocks are replaced by the block they turn into.
            if let Some(final_state) = entry.compound("nbt").and_then(|n| n.string("final_state")) {
                let (name, vars) = split_block_key(final_state);
                block = scenery.block_entry(blocks, name, &vars);
            }
            if let Some(block) = block {
                scenery.cells[index] = block + 1;
            }
        }
        Ok(scenery)
    }

    fn read_blocks_file(bytes: &[u8], legacy: &LegacyBlocks) -> Result<Self, SceneryError> {
        let short = |at: usize| -> Result<i64, SceneryError> {
            let b = bytes.get(at..at + 2).ok_or(SceneryError::Invalid("the file ends early"))?;
            Ok(u16::from_be_bytes([b[0], b[1]]) as i64)
        };
        let name_len = short(0)? as usize;
        let name = bytes.get(2..2 + name_len).ok_or(SceneryError::Invalid("the file ends early"))?;
        let at = 2 + name_len;
        // The Y size comes first.
        let (size_y, size_x, size_z) = (short(at)?, short(at + 2)?, short(at + 4)?);
        let mut scenery = Self::new([size_x, size_y, size_z])?;
        scenery.file_map = String::from_utf8_lossy(name).into_owned();
        // The original leaves this flag as the previous file set it.
        scenery.legacy = true;

        let data = &bytes[at + 6..];
        let mut entries: std::collections::HashMap<(u8, u8), Option<u32>> = Default::default();
        for i in 0..scenery.cells.len().min(data.len() / 2) {
            let (id, value) = (data[i * 2], data[i * 2 + 1] & 15);
            if id == 0 {
                continue;
            }
            let entry = *entries.entry((id, value)).or_insert_with(|| {
                let found = legacy.get(id, value)?;
                Some(scenery.add(SceneryBlock { block: found.block.clone(), state: found.state.clone(), waterlogged: false }))
            });
            if let Some(entry) = entry {
                scenery.cells[i] = entry + 1;
            }
        }
        Ok(scenery)
    }

    fn add(&mut self, block: SceneryBlock) -> u32 {
        match self.palette.iter().position(|b| *b == block) {
            Some(i) => i as u32,
            None => {
                self.palette.push(block);
                (self.palette.len() - 1) as u32
            }
        }
    }

    /// A palette entry from a key such as `minecraft:oak_stairs[facing=east]`.
    fn palette_entry(&mut self, blocks: &Blocks, key: &str) -> Option<u32> {
        let (name, vars) = split_block_key(key);
        self.block_entry(blocks, name, &vars)
    }

    fn block_entry(&mut self, blocks: &Blocks, mc_id: &str, properties: &[(String, String)]) -> Option<u32> {
        let (def, id_vars) = blocks.by_id(mc_id)?;
        let mut vars = id_vars.to_vec();
        for (name, value) in properties {
            match vars.iter_mut().find(|(n, _)| n == name) {
                Some(slot) => slot.1 = value.clone(),
                None => vars.push((name.clone(), value.clone())),
            }
        }
        let waterlogged_var = vars.iter().find(|(n, _)| n == "waterlogged").map(|(_, v)| v.as_str());
        let waterlogged = waterlogged_var != Some("false") && (def.waterlogged || waterlogged_var == Some("true"));
        let state = def.state_from_vars(&vars);
        Some(self.add(SceneryBlock { block: def.name.clone(), state, waterlogged }))
    }

    /// Number of blocks that become timelines when the scenery is added.
    pub fn timeline_count(&self, blocks: &Blocks) -> usize {
        let timeline: Vec<bool> =
            self.palette.iter().map(|b| blocks.def(&b.block).is_some_and(|d| d.timeline)).collect();
        self.cells.iter().filter(|&&c| c > 0 && timeline[(c - 1) as usize]).count()
    }

    /// Meshes of the scenery grouped by texture. With `timelines`, blocks
    /// that are placed as timelines (chests, doors, ...) are left out, as
    /// the project draws them itself.
    pub fn meshes(&self, pack: &AssetPack, timelines: bool, randomize: bool) -> Vec<(String, MeshData)> {
        let blocks = pack.blocks();
        // Blocks missing from the pack become air.
        let mut index = Vec::with_capacity(self.palette.len());
        let mut palette = Vec::new();
        for entry in &self.palette {
            match blocks.def(&entry.block) {
                Some(def) => {
                    index.push(Some(palette.len()));
                    palette.push(GridBlock { def, state: entry.state.clone() });
                }
                None => index.push(None),
            }
        }
        let cell = |p: [usize; 3]| self.cell(p).and_then(|i| index[i]);
        let grid = Grid {
            size: self.size,
            palette: &palette,
            cell: &cell,
            source: GridSource { scenery: true, legacy: self.legacy },
            randomize,
            skip_timelines: timelines,
        };
        build_grid(pack, &grid)
    }
}

/// Splits `minecraft:oak_stairs[facing=east,half=bottom]` into the id and
/// its properties.
fn split_block_key(key: &str) -> (&str, Vec<(String, String)>) {
    match key.find('[') {
        Some(open) => {
            let inner = key[open + 1..].trim_end_matches(']');
            (&key[..open], parse_state_vars(inner))
        }
        None => (key, Vec::new()),
    }
}

/// Palette indices of Sponge schematics, stored as variable length
/// integers. The original reads one byte per block, which only works for
/// palettes of up to 128 entries.
fn decode_varints(bytes: &[u8], count: usize) -> Vec<usize> {
    let mut out = Vec::with_capacity(count);
    let mut value = 0usize;
    let mut shift = 0;
    for &byte in bytes {
        if shift < 28 {
            value |= ((byte & 0x7f) as usize) << shift;
        }
        if byte & 0x80 != 0 {
            shift += 7;
            continue;
        }
        out.push(value);
        if out.len() == count {
            break;
        }
        value = 0;
        shift = 0;
    }
    out
}

/// Whether a block of a structure is kept at an integrity below 1. The
/// choice is random but the same for a position every time.
fn keep_for_integrity(index: usize, options: SceneryOptions) -> bool {
    if options.integrity >= 1.0 && !options.integrity_invert {
        return true;
    }
    let mut h = (index as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    let random = (h >> 11) as f64 / (1u64 << 53) as f64;
    if options.integrity_invert {
        random >= options.integrity
    } else {
        random <= options.integrity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbt::Writer;

    fn pack() -> AssetPack {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/Data/Minecraft");
        AssetPack::open(&folder, "1.20.2").unwrap()
    }

    fn legacy(pack: &AssetPack) -> LegacyBlocks {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/Data/legacy.midata");
        LegacyBlocks::load(&std::fs::read(path).unwrap(), pack.blocks())
    }

    fn gzip(data: &[u8]) -> Vec<u8> {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut gz, data).unwrap();
        gz.finish().unwrap()
    }

    #[test]
    fn legacy_ids_and_data_values() {
        let pack = pack();
        let legacy = legacy(&pack);
        let granite = legacy.get(1, 1).unwrap();
        assert_eq!(granite.block, "stone");
        assert!(granite.state.contains(&("variant".into(), "granite".into())));
        assert_eq!(legacy.get(2, 0).unwrap().block, "grass_block");
        // Data values beyond the listed ones fall back to the id's block.
        assert_eq!(legacy.get(4, 9).unwrap().block, "cobblestone");
        // Oak stairs: the facing comes from the low bits, the half from 0x4.
        let east = legacy.get(53, 0).unwrap();
        let upside_down = legacy.get(53, 4).unwrap();
        assert_eq!(east.block, "stairs");
        let half = |b: &crate::LegacyBlock| b.state.iter().find(|(n, _)| n == "half").unwrap().1.clone();
        assert_ne!(half(east), half(upside_down));
        assert!(!legacy.is_known(0) || legacy.get(0, 0).is_none());
    }

    #[test]
    fn mcedit_schematic() {
        let pack = pack();
        let legacy = legacy(&pack);
        // 2 wide (X), 1 long (Z), 2 high (Y): stone at the bottom left,
        // glass above it, air on the right.
        let mut w = Writer::default();
        w.begin("Schematic").short("Width", 2).short("Length", 1).short("Height", 2).string("Materials", "Alpha");
        w.bytes("Blocks", &[1, 0, 20, 0]).bytes("Data", &[0, 0, 0, 0]).list("TileEntities", 10, 0).end();
        let scenery = Scenery::read(&gzip(&w.0), ".schematic", pack.blocks(), &legacy, Default::default()).unwrap();
        assert_eq!(scenery.size, [2, 1, 2]);
        assert_eq!(scenery.get([0, 0, 0]).unwrap().block, "stone");
        assert_eq!(scenery.get([0, 0, 1]).unwrap().block, "glass");
        assert!(scenery.get([1, 0, 0]).is_none());

        let meshes = scenery.meshes(&pack, true, false);
        let triangles = |name: &str| meshes.iter().find(|(n, _)| n == name).map_or(0, |(_, m)| m.triangle_count());
        // The glass hides nothing of the stone, the stone hides the
        // bottom of the glass.
        assert_eq!(triangles("block/stone"), 12);
        assert_eq!(triangles("block/glass"), 10);
    }

    #[test]
    fn sponge_schematic() {
        let pack = pack();
        let mut w = Writer::default();
        w.begin("Schematic").int("Version", 2).short("Width", 3).short("Length", 1).short("Height", 1);
        w.begin("Palette")
            .int("minecraft:air", 0)
            .int("minecraft:oak_stairs[facing=north,half=top,shape=straight,waterlogged=true]", 1)
            .int("minecraft:dirt_path", 2)
            .end();
        w.bytes("BlockData", &[1, 0, 2]).end();
        let scenery =
            Scenery::read(&gzip(&w.0), "schematic", pack.blocks(), &LegacyBlocks::empty(), Default::default()).unwrap();
        let stairs = scenery.get([0, 0, 0]).unwrap();
        assert_eq!(stairs.block, "stairs");
        assert!(stairs.waterlogged);
        for pair in [("variant", "oak"), ("facing", "north"), ("half", "top")] {
            assert!(stairs.state.contains(&(pair.0.into(), pair.1.into())), "{:?}", stairs.state);
        }
        assert!(scenery.get([1, 0, 0]).is_none());
        assert!(scenery.get([2, 0, 0]).is_some());
    }

    #[test]
    fn structure_file() {
        let pack = pack();
        let mut w = Writer::default();
        w.begin("").int("DataVersion", 3465);
        w.list("size", 3, 3).raw_int(2).raw_int(3).raw_int(1);
        w.list("palette", 10, 2);
        w.string("Name", "minecraft:stone").end();
        w.string("Name", "minecraft:jigsaw").end();
        w.list("blocks", 10, 2);
        // pos is [x, y, z]
        w.list("pos", 3, 3).raw_int(1).raw_int(2).raw_int(0).int("state", 0).end();
        w.list("pos", 3, 3).raw_int(0).raw_int(0).raw_int(0).int("state", 1);
        w.begin("nbt").string("final_state", "minecraft:oak_planks").end().end();
        w.end();
        let scenery = Scenery::read(&gzip(&w.0), "nbt", pack.blocks(), &LegacyBlocks::empty(), Default::default()).unwrap();
        assert_eq!(scenery.size, [2, 1, 3]);
        assert_eq!(scenery.get([1, 0, 2]).unwrap().block, "stone");
        assert_eq!(scenery.get([0, 0, 0]).unwrap().block, "planks");

        let half = SceneryOptions { integrity: 0.0, ..Default::default() };
        let empty = Scenery::read(&gzip(&w.0), "nbt", pack.blocks(), &LegacyBlocks::empty(), half).unwrap();
        assert!(empty.get([1, 0, 2]).is_none());
    }

    #[test]
    fn blocks_file_and_errors() {
        let pack = pack();
        let legacy = legacy(&pack);
        let mut data = Vec::new();
        data.extend(3u16.to_be_bytes());
        data.extend(b"Map");
        // Y size, X size, Z size
        data.extend(1u16.to_be_bytes());
        data.extend(2u16.to_be_bytes());
        data.extend(1u16.to_be_bytes());
        data.extend([0, 0, 5, 1]);
        let scenery = Scenery::read(&data, "blocks", pack.blocks(), &legacy, Default::default()).unwrap();
        assert_eq!(scenery.size, [2, 1, 1]);
        assert_eq!(scenery.file_map, "Map");
        assert!(scenery.get([0, 0, 0]).is_none());
        assert_eq!(scenery.get([1, 0, 0]).unwrap().block, "planks");

        assert!(Scenery::read(&data[..5], "blocks", pack.blocks(), &legacy, Default::default()).is_err());
        assert!(Scenery::read(b"junk", "schematic", pack.blocks(), &legacy, Default::default()).is_err());
        let mut w = Writer::default();
        w.begin("Schematic").short("Width", 0).short("Length", 1).short("Height", 1).end();
        assert!(Scenery::read(&w.0, "schematic", pack.blocks(), &legacy, Default::default()).is_err());
    }

    #[test]
    fn varints() {
        assert_eq!(decode_varints(&[0, 1, 0x7f, 0x80, 0x01, 0xff, 0x7f], 10), vec![0, 1, 127, 128, 16383]);
        assert_eq!(decode_varints(&[1, 2, 3], 2), vec![1, 2]);
    }

    #[test]
    fn block_keys() {
        let (name, vars) = split_block_key("minecraft:oak_stairs[facing=east,half=bottom]");
        assert_eq!(name, "minecraft:oak_stairs");
        assert_eq!(vars, vec![("facing".into(), "east".into()), ("half".into(), "bottom".into())]);
        assert_eq!(split_block_key("minecraft:stone"), ("minecraft:stone", Vec::new()));
    }

    #[test]
    fn integrity_keeps_about_the_share_asked_for() {
        let options = SceneryOptions { integrity: 0.25, ..Default::default() };
        let kept = (0..10_000).filter(|&i| keep_for_integrity(i, options)).count();
        assert!((2_200..2_800).contains(&kept), "{kept}");
        let inverted = SceneryOptions { integrity_invert: true, ..options };
        let kept_inverted = (0..10_000).filter(|&i| keep_for_integrity(i, inverted)).count();
        assert_eq!(kept + kept_inverted, 10_000);
    }
}
