//! Numeric block ids of Minecraft before 1.13, used by MCEdit schematics,
//! `.blocks` files and old projects (`legacy_block_id` in `legacy.midata`,
//! `minecraft_assets_load_legacy_block_data`).

use crate::Blocks;
use mi_format::json::{self, Json, JsonObject};

/// A block an id and data value stand for.
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyBlock {
    pub block: String,
    /// Every declared state of the block.
    pub state: Vec<(String, String)>,
}

/// The table of id (0..256) and data value (0..16).
#[derive(Debug, Clone)]
pub struct LegacyBlocks {
    known: Vec<bool>,
    blocks: Vec<[Option<LegacyBlock>; 16]>,
}

/// While loading: the block and state variables of each data value.
#[derive(Default, Clone)]
struct Slot {
    block: Option<String>,
    vars: Option<Vec<(String, String)>>,
}

/// `state_vars_add`: later variables replace earlier ones of that name.
fn add_vars(dest: &mut Vec<(String, String)>, src: &[(String, String)]) {
    for (name, value) in src {
        match dest.iter_mut().find(|(n, _)| n == name) {
            Some(slot) => slot.1 = value.clone(),
            None => dest.push((name.clone(), value.clone())),
        }
    }
}

impl LegacyBlocks {
    /// An empty table, for when `legacy.midata` is missing.
    pub fn empty() -> Self {
        Self { known: vec![false; 256], blocks: vec![Default::default(); 256] }
    }

    /// Builds the table from the contents of `legacy.midata`.
    pub fn load(legacy: &[u8], blocks: &Blocks) -> Self {
        let mut table = Self::empty();
        let Ok(Json::Object(root)) = json::parse(legacy) else { return table };
        let Some(ids) = root.object("legacy_block_id") else { return table };

        for (key, entry) in ids.iter() {
            let Ok(id) = key.trim().parse::<usize>() else { continue };
            let Some(entry) = entry.as_object() else { continue };
            if id >= 256 {
                continue;
            }
            table.known[id] = true;

            let base = entry.string("id").and_then(|mc_id| blocks.by_id(mc_id));
            let mut slots: Vec<Slot> = vec![
                Slot {
                    block: base.map(|(def, _)| def.name.clone()),
                    vars: base.filter(|(_, vars)| !vars.is_empty()).map(|(_, vars)| vars.to_vec()),
                };
                16
            ];
            if let Some(data) = entry.object("data") {
                apply_data(&mut slots, data, 0, 1, blocks);
            }
            for (d, slot) in slots.into_iter().enumerate() {
                let Some(def) = slot.block.as_deref().and_then(|name| blocks.def(name)) else { continue };
                let state = def.state_from_vars(&slot.vars.unwrap_or_default());
                table.blocks[id][d] = Some(LegacyBlock { block: def.name.clone(), state });
            }
        }
        table
    }

    /// Whether the table knows the id; unknown ids are treated as air.
    pub fn is_known(&self, id: u8) -> bool {
        self.known[id as usize]
    }

    pub fn get(&self, id: u8, data: u8) -> Option<&LegacyBlock> {
        self.blocks[id as usize][(data & 15) as usize].as_ref()
    }
}

/// Applies the data values of an id: numbers, or bit masks that group
/// further numbers.
fn apply_data(slots: &mut [Slot], map: &JsonObject, mask: usize, base: usize, blocks: &Blocks) {
    for (key, value) in map.iter() {
        let nested = match key {
            "0x1" => Some((1, 1)),
            "0x2" => Some((2, 2)),
            "0x4" => Some((4, 4)),
            "0x8" => Some((8, 8)),
            "0x1+0x2" => Some((3, 1)),
            "0x1+0x2+0x4" => Some((7, 1)),
            "0x4+0x8" => Some((12, 4)),
            _ => None,
        };
        if let Some((mask, base)) = nested {
            if let Some(inner) = value.as_object() {
                apply_data(slots, inner, mask, base, blocks);
            }
            continue;
        }
        let Ok(number) = key.trim().parse::<usize>() else { continue };
        let vars = crate::pack::parse_state_vars(value.as_str().unwrap_or(""));
        let found = vars.iter().find(|(n, _)| n == "id").and_then(|(_, id)| blocks.by_id(id));

        let targets: Vec<usize> = if mask > 0 {
            (0..16).filter(|d| (d & mask) / base == number).collect()
        } else if number < 16 {
            vec![number]
        } else {
            Vec::new()
        };
        for d in targets {
            let slot = &mut slots[d];
            let slot_vars = slot.vars.get_or_insert_with(Vec::new);
            if let Some((def, id_vars)) = found {
                slot.block = Some(def.name.clone());
                add_vars(slot_vars, id_vars);
            }
            add_vars(slot_vars, &vars);
        }
    }
}
