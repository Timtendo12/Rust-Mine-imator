//! The block builder: turns a grid of blocks into meshes
//! (`builder_set_model`, `builder_generate`, the `block_set_*` scripts).
//!
//! Some blocks change with their neighbours: fences connect, stairs form
//! corners, doors take the hinge of their upper half. Like the original,
//! this happens in two steps. First every block gets its state and models;
//! blocks whose rules look at the faces of their neighbours ("require
//! models": fences, panes, walls, redstone, vines) are then resolved while
//! the mesh is generated, when those faces are known.
//!
//! Most rules only apply where the file does not store the result:
//! schematics with numeric ids (`legacy`) and repeated block templates.
//! Modern schematics and structures keep their saved states.

use crate::blocks::{block_mesh, face_culled, pick_weighted, position_hash, Depth, Dir, PlacedBlock, RandomOffset};
use crate::liquid::{liquid_mesh, LiquidSurroundings};
use crate::{AssetPack, BlockDef, Blocks, RenderModel};
use mi_mesh::MeshData;
use std::collections::HashMap;
use std::sync::Arc;

const BLOCK: f64 = 16.0;

/// Where the grid comes from, which decides which rules apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GridSource {
    /// A scenery resource rather than a block template.
    pub scenery: bool,
    /// A file with numeric block ids, which stores no connections.
    pub legacy: bool,
}

/// A block and state of the grid's palette.
#[derive(Debug, Clone)]
pub struct GridBlock<'a> {
    pub def: &'a BlockDef,
    pub state: Vec<(String, String)>,
    /// Holds water besides the block itself.
    pub waterlogged: bool,
}

/// A grid of blocks to build.
pub struct Grid<'a> {
    pub size: [usize; 3],
    pub palette: &'a [GridBlock<'a>],
    /// The palette index at a position, `None` for air.
    pub cell: &'a dyn Fn([usize; 3]) -> Option<usize>,
    pub source: GridSource,
    pub randomize: bool,
    /// Leave out blocks that the project shows as timelines.
    pub skip_timelines: bool,
    /// Whether liquids wave (`liquid_animation`), which changes which of
    /// their sides are drawn.
    pub liquid_animation: bool,
}

type State = Vec<(String, String)>;

fn value<'s>(state: &'s [(String, String)], name: &str) -> &'s str {
    state.iter().find(|(n, _)| n == name).map_or("", |(_, v)| v.as_str())
}

/// `block_set_state_id_value`: one state changed, if the block has it.
fn with_value(def: &BlockDef, state: &[(String, String)], name: &str, new: &str) -> State {
    let mut state = state.to_vec();
    let valid = def.states.iter().any(|(n, values)| n == name && values.iter().any(|v| v.value == new));
    if valid {
        if let Some(slot) = state.iter_mut().find(|(n, _)| n == name) {
            slot.1 = new.to_owned();
        }
    }
    state
}

/// `block_get_state_id` from name and value pairs.
fn from_pairs(def: &BlockDef, pairs: &[(&str, &str)]) -> State {
    let vars: Vec<(String, String)> = pairs.iter().map(|(n, v)| ((*n).to_owned(), (*v).to_owned())).collect();
    def.state_from_vars(&vars)
}

/// What a block turns into at a position.
#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    /// The models of a state.
    State(State),
    /// The first model of each state, raised by the offset (doors and
    /// double plants draw both halves from the lower one).
    Halves(Vec<(State, f64)>),
    /// Nothing is drawn (upper halves, bed heads, the second chest).
    Nothing,
}

/// The side of a neighbour that faces the block being built.
#[derive(Debug, Clone, Copy, Default)]
struct NeighbourFace {
    full: bool,
    min_depth: Option<Depth>,
}

impl NeighbourFace {
    /// A full, opaque face, which fences and walls connect to.
    fn solid(self) -> bool {
        self.full && self.min_depth == Some(Depth::Opaque)
    }
}

struct Context<'g, 'a> {
    grid: &'g Grid<'a>,
    p: [i64; 3],
    def: &'a BlockDef,
    state: &'g [(String, String)],
    /// Faces of the neighbours, known while generating.
    faces: [NeighbourFace; 6],
}

impl<'a> Context<'_, 'a> {
    fn at(&self, offset: [i64; 3]) -> Option<&GridBlock<'a>> {
        let p = [self.p[0] + offset[0], self.p[1] + offset[1], self.p[2] + offset[2]];
        block_at(self.grid, p)
    }

    fn side(&self, dir: Dir) -> Option<&GridBlock<'a>> {
        self.at(dir.step())
    }

    fn face(&self, dir: Dir) -> NeighbourFace {
        self.faces[dir as usize]
    }

    fn scenery_keeps_state(&self) -> bool {
        self.grid.source.scenery && !self.grid.source.legacy
    }

    fn single_template(&self) -> bool {
        !self.grid.source.scenery && self.grid.size[0] == 1 && self.grid.size[1] == 1
    }

    fn same_block(&self, other: &GridBlock) -> bool {
        other.def.name == self.def.name
    }
}

fn block_at<'g, 'a>(grid: &'g Grid<'a>, p: [i64; 3]) -> Option<&'g GridBlock<'a>> {
    if (0..3).any(|i| p[i] < 0 || p[i] >= grid.size[i] as i64) {
        return None;
    }
    grid.palette.get((grid.cell)([p[0] as usize, p[1] as usize, p[2] as usize])?)
}

const SIDES: [(Dir, &str); 4] = [(Dir::East, "east"), (Dir::West, "west"), (Dir::South, "south"), (Dir::North, "north")];

fn is_east_west(facing: &str) -> bool {
    facing == "east" || facing == "west"
}

fn is_south_north(facing: &str) -> bool {
    facing == "south" || facing == "north"
}

/// Whether a fence gate next to a fence or wall on side `dir` lines up
/// with it.
fn gate_connects(other: &GridBlock, dir: Dir) -> bool {
    let facing = value(&other.state, "facing");
    match dir {
        Dir::East | Dir::West => !is_east_west(facing),
        _ => !is_south_north(facing),
    }
}

/// Rules that only look at neighbouring blocks (`builder_set_model`).
fn set_rules(c: &Context) -> Outcome {
    let def = c.def;
    let state = c.state;
    match def.kind.as_str() {
        "stairs" => {
            if c.scenery_keeps_state() {
                return Outcome::State(state.to_vec());
            }
            let half = value(state, "half");
            let facing = value(state, "facing");
            let mut shape = "straight";
            let other_facing = |dir: Dir| -> Option<&str> {
                let other = c.side(dir).filter(|b| b.def.kind == "stairs")?;
                (value(&other.state, "half") == half).then(|| value(&other.state, "facing"))
            };
            if is_east_west(facing) {
                if let Some(other) = other_facing(Dir::East) {
                    shape = match (facing, other) {
                        ("east", "south") => "outer_right",
                        ("east", "north") => "outer_left",
                        ("west", "south") => "inner_left",
                        ("west", "north") => "inner_right",
                        _ => shape,
                    };
                }
                if let Some(other) = other_facing(Dir::West) {
                    shape = match (facing, other) {
                        ("east", "south") => "inner_right",
                        ("east", "north") => "inner_left",
                        ("west", "south") => "outer_left",
                        ("west", "north") => "outer_right",
                        _ => shape,
                    };
                }
            }
            if is_south_north(facing) {
                if let Some(other) = other_facing(Dir::South) {
                    shape = match (facing, other) {
                        ("south", "east") => "outer_left",
                        ("south", "west") => "outer_right",
                        ("north", "east") => "inner_right",
                        ("north", "west") => "inner_left",
                        _ => shape,
                    };
                }
                if let Some(other) = other_facing(Dir::North) {
                    shape = match (facing, other) {
                        ("south", "east") => "inner_left",
                        ("south", "west") => "inner_right",
                        ("north", "east") => "outer_right",
                        ("north", "west") => "outer_left",
                        _ => shape,
                    };
                }
            }
            if shape == "straight" {
                Outcome::State(state.to_vec())
            } else {
                Outcome::State(with_value(def, state, "shape", shape))
            }
        }
        "fence_gate" => {
            if c.scenery_keeps_state() {
                return Outcome::State(state.to_vec());
            }
            let facing = value(state, "facing");
            let sides = if is_east_west(facing) {
                [Dir::South, Dir::North]
            } else if is_south_north(facing) {
                [Dir::East, Dir::West]
            } else {
                return Outcome::State(with_value(def, state, "in_wall", "false"));
            };
            let in_wall = sides.iter().any(|&d| c.side(d).is_some_and(|b| b.def.kind == "wall"));
            Outcome::State(with_value(def, state, "in_wall", if in_wall { "true" } else { "false" }))
        }
        "snowy" => {
            if value(state, "snowy") == "true" {
                return Outcome::State(state.to_vec());
            }
            let snowy = c.side(Dir::Up).is_some_and(|b| b.def.kind == "snow");
            Outcome::State(with_value(def, state, "snowy", if snowy { "true" } else { "false" }))
        }
        "bed" => {
            if value(state, "part") == "head" {
                Outcome::Nothing
            } else {
                Outcome::State(state.to_vec())
            }
        }
        "double_plant" => {
            if value(state, "half") == "upper" {
                return Outcome::Nothing;
            }
            Outcome::Halves(vec![(state.to_vec(), 0.0), (with_value(def, state, "half", "upper"), BLOCK)])
        }
        "door" => {
            if value(state, "half") == "upper" {
                return Outcome::Nothing;
            }
            // The hinge is stored in the upper half.
            let mut state = state.to_vec();
            if let Some(above) = c.side(Dir::Up).filter(|b| c.same_block(b)) {
                state = with_value(def, &state, "hinge", value(&above.state, "hinge"));
            }
            let hinge_right = value(&state, "hinge") == "right";
            let facing = value(&state, "facing").to_owned();
            let open = value(&state, "open") == "true";
            let pick = |right: &'static str, left: &'static str| if hinge_right { right } else { left };
            let (location, closed_dir) = match facing.as_str() {
                "east" => (pick("south_west", "north_west"), pick("north", "south")),
                "west" => (pick("north_east", "south_east"), pick("south", "north")),
                "south" => (pick("north_west", "north_east"), pick("east", "west")),
                "north" => (pick("south_east", "south_west"), pick("west", "east")),
                _ => ("", ""),
            };
            let direction = if open { facing.as_str() } else { closed_dir };
            let state = with_value(def, &with_value(def, &state, "location", location), "direction", direction);
            let upper = with_value(def, &state, "half", "upper");
            Outcome::Halves(vec![(state, 0.0), (upper, BLOCK)])
        }
        "chest" => {
            // Chests are drawn from their model as timelines; here only
            // the second half of a double chest is dropped.
            match value(state, "type") {
                "left" => Outcome::Nothing,
                _ => Outcome::State(state.to_vec()),
            }
        }
        "fire" => {
            if !c.grid.source.legacy {
                return Outcome::State(state.to_vec());
            }
            let touches = |d: Dir| if c.side(d).is_some_and(|b| !c.same_block(b)) { "true" } else { "false" };
            Outcome::State(from_pairs(
                def,
                &[
                    ("variant", value(state, "variant")),
                    ("east", touches(Dir::East)),
                    ("west", touches(Dir::West)),
                    ("south", touches(Dir::South)),
                    ("north", touches(Dir::North)),
                    ("up", touches(Dir::Up)),
                ],
            ))
        }
        "vine" => {
            if !c.grid.source.legacy {
                return Outcome::State(state.to_vec());
            }
            let up = c.side(Dir::Up).is_some_and(|b| !c.same_block(b));
            Outcome::State(with_value(def, state, "up", if up { "true" } else { "false" }))
        }
        "tripwire" => {
            if c.scenery_keeps_state() || c.single_template() {
                return Outcome::State(state.to_vec());
            }
            let connects = |d: Dir| {
                if c.side(d).is_some_and(|b| b.def.name == "tripwire" || b.def.name == "tripwire_hook") {
                    "true"
                } else {
                    "false"
                }
            };
            Outcome::State(from_pairs(
                def,
                &[
                    ("east", connects(Dir::East)),
                    ("west", connects(Dir::West)),
                    ("south", connects(Dir::South)),
                    ("north", connects(Dir::North)),
                ],
            ))
        }
        "chorus_plant" => {
            if c.scenery_keeps_state() || (!c.grid.source.scenery && c.grid.size.iter().product::<usize>() == 1) {
                return Outcome::State(state.to_vec());
            }
            let connects = |d: Dir| {
                let yes = c.side(d).is_some_and(|b| {
                    c.same_block(b) || b.def.kind == "chorus_plant_connect" || (d == Dir::Down && b.def.name == "end_stone")
                });
                if yes { "true" } else { "false" }
            };
            Outcome::State(from_pairs(
                def,
                &[
                    ("east", connects(Dir::East)),
                    ("west", connects(Dir::West)),
                    ("south", connects(Dir::South)),
                    ("north", connects(Dir::North)),
                    ("up", connects(Dir::Up)),
                    ("down", connects(Dir::Down)),
                ],
            ))
        }
        "redstone_repeater" => {
            if c.scenery_keeps_state() || value(state, "locked") == "true" {
                return Outcome::State(state.to_vec());
            }
            let facing = value(state, "facing");
            let powered_facing = |d: Dir, wanted: &str| {
                c.side(d).is_some_and(|b| b.def.name == "powered_repeater" && value(&b.state, "facing") == wanted)
            };
            let locked = (is_south_north(facing) && (powered_facing(Dir::East, "east") || powered_facing(Dir::West, "west")))
                || (is_east_west(facing) && (powered_facing(Dir::South, "south") || powered_facing(Dir::North, "north")));
            Outcome::State(with_value(def, state, "locked", if locked { "true" } else { "false" }))
        }
        "dripstone" => {
            let previous = value(state, "thickness");
            if c.scenery_keeps_state() || previous == "base" {
                return Outcome::State(state.to_vec());
            }
            let dir = value(state, "vertical_direction");
            let top = c.grid.size[2] as i64 - 1;
            let size = if dir == "up" { top - c.p[2] } else { c.p[2] };
            let offset = size
                + match previous {
                    "frustum" => 1,
                    "middle" => 2,
                    _ => 0,
                };
            let thickness = if offset == 0 {
                previous
            } else if offset == 1 {
                "frustum"
            } else if size >= top {
                "base"
            } else {
                "middle"
            };
            Outcome::State(from_pairs(def, &[("vertical_direction", dir), ("thickness", thickness)]))
        }
        "big_dripleaf" => {
            if c.scenery_keeps_state() || value(state, "type") == "big_dripleaf_stem" {
                return Outcome::State(state.to_vec());
            }
            if c.p[2] != c.grid.size[2] as i64 - 1 {
                let facing = value(state, "facing");
                return Outcome::State(from_pairs(def, &[("type", "big_dripleaf_stem"), ("facing", facing), ("tilt", "none")]));
            }
            Outcome::State(state.to_vec())
        }
        _ => Outcome::State(state.to_vec()),
    }
}

/// Rules that also look at the faces of neighbours, run while generating.
fn generate_rules(c: &Context) -> State {
    let def = c.def;
    let state = c.state;
    match def.kind.as_str() {
        "fence" => {
            if c.scenery_keeps_state() || c.single_template() {
                return state.to_vec();
            }
            let connects = |d: Dir| {
                let yes = c.side(d).is_some_and(|b| {
                    b.def.kind == def.kind || c.face(d).solid() || (b.def.kind == "fence_gate" && gate_connects(b, d))
                });
                if yes { "true" } else { "false" }
            };
            let mut pairs = vec![("variant", value(state, "variant"))];
            for (d, name) in SIDES {
                pairs.push((name, connects(d)));
            }
            from_pairs(def, &pairs)
        }
        "bars" | "colored_bars" => {
            if c.scenery_keeps_state() || c.single_template() {
                return state.to_vec();
            }
            let connects = |d: Dir| {
                let face = c.face(d);
                let yes = c.side(d).is_some_and(|b| b.def.kind == "bars" || b.def.kind == "colored_bars")
                    || (face.full && face.min_depth != Some(Depth::Cutout));
                if yes { "true" } else { "false" }
            };
            let pairs: Vec<(&str, &str)> = SIDES.iter().map(|&(d, name)| (name, connects(d))).collect();
            let new = from_pairs(def, &pairs);
            if def.kind == "colored_bars" {
                with_value(def, &new, "color", value(state, "color"))
            } else {
                new
            }
        }
        "wall" => {
            if c.scenery_keeps_state() || c.single_template() {
                return state.to_vec();
            }
            let joins = |other: &GridBlock, face: NeighbourFace| other.def.kind == def.kind || face.solid();
            // The original tests the face to the east here instead of the
            // one above.
            let tall = c.side(Dir::Up).is_some_and(|b| joins(b, c.face(Dir::Up)));
            let mut count = [0usize; 4];
            for level in 0..if tall { 2 } else { 1 } {
                for (i, (d, _)) in SIDES.iter().enumerate() {
                    if level == 1 && count[i] == 0 {
                        continue;
                    }
                    let s = d.step();
                    let Some(other) = c.at([s[0], s[1], level]) else { continue };
                    if joins(other, c.face(*d)) || (other.def.kind == "fence_gate" && gate_connects(other, *d)) {
                        count[i] += 1;
                    }
                }
            }
            let names = ["none", "low", "tall"];
            let [east, west, south, north] = count;
            let straight = (east > 0 && west > 0 && south == 0 && north == 0) || (east == 0 && west == 0 && south > 0 && north > 0);
            from_pairs(
                def,
                &[
                    ("variant", value(state, "variant")),
                    ("east", names[east]),
                    ("west", names[west]),
                    ("south", names[south]),
                    ("north", names[north]),
                    ("up", if straight { "false" } else { "true" }),
                ],
            )
        }
        "redstone_wire" => {
            if c.scenery_keeps_state() || (!c.grid.source.scenery && c.grid.size.iter().product::<usize>() == 1) {
                return state.to_vec();
            }
            let up_open = !c.face(Dir::Up).solid();
            let mut sides = ["none"; 4];
            for (i, &(d, _)) in SIDES.iter().enumerate() {
                let s = d.step();
                if let Some(other) = c.side(d) {
                    let lined_up = |facing: &str| match d {
                        Dir::East | Dir::West => is_east_west(facing),
                        _ => is_south_north(facing),
                    };
                    let repeater = other.def.kind == "redstone_repeater" || other.def.kind == "redstone_comparator";
                    if c.same_block(other)
                        || other.def.kind == "redstone_connect"
                        || (repeater && lined_up(value(&other.state, "facing")))
                    {
                        sides[i] = "side";
                    }
                }
                // Wire running up the side of the neighbour, or down to
                // the block below it.
                if sides[i] == "none" && up_open && c.at([s[0], s[1], 1]).is_some_and(|b| c.same_block(b)) {
                    sides[i] = "up";
                }
                if sides[i] == "none" && !c.face(d).solid() && c.at([s[0], s[1], -1]).is_some_and(|b| c.same_block(b)) {
                    sides[i] = "side";
                }
            }
            if sides.iter().all(|s| *s == "none") {
                sides = ["side"; 4];
            }
            from_pairs(
                def,
                &[
                    ("east", sides[0]),
                    ("west", sides[1]),
                    ("south", sides[2]),
                    ("north", sides[3]),
                    ("power", value(state, "power")),
                ],
            )
        }
        "kelp" | "twisting_vines" => {
            let plant = if def.kind == "kelp" { "kelp_plant" } else { "twisting_vines_plant" };
            if c.scenery_keeps_state() || value(state, "variant") == plant {
                return state.to_vec();
            }
            if c.p[2] != c.grid.size[2] as i64 - 1 {
                return from_pairs(def, &[("variant", plant)]);
            }
            state.to_vec()
        }
        "weeping_vines" => {
            if c.scenery_keeps_state() || value(state, "variant") == "weeping_vines_plant" {
                return state.to_vec();
            }
            let variant = if c.p[2] == 0 { "weeping_vines" } else { "weeping_vines_plant" };
            from_pairs(def, &[("variant", variant)])
        }
        "cave_vines" => {
            if c.scenery_keeps_state() || value(state, "type") == "cave_vines_plant" {
                return state.to_vec();
            }
            let kind = if c.p[2] == 0 { "cave_vines" } else { "cave_vines_plant" };
            from_pairs(def, &[("type", kind), ("berries", value(state, "berries"))])
        }
        _ => state.to_vec(),
    }
}

/// Colour and emission set by a block for all its faces.
fn block_color(def: &BlockDef, state: &[(String, String)]) -> Option<([f32; 4], f64)> {
    if def.kind != "redstone_wire" {
        return None;
    }
    let power = value(state, "power").parse::<f64>().unwrap_or(0.0) / 15.0;
    let red = if power == 0.0 { 0.3 } else { 0.6 * power + 0.4 };
    // `make_color_rgb` takes whole numbers.
    let red = (red * 255.0).floor() / 255.0;
    Some(([red as f32, 0.0, 0.0, 1.0], power))
}

fn state_key(name: &str, state: &[(String, String)]) -> String {
    let mut key = name.to_owned();
    for (n, v) in state {
        key.push(',');
        key.push_str(n);
        key.push('=');
        key.push_str(v);
    }
    key
}

/// What is drawn at a position after the rules ran.
#[derive(Debug, Clone)]
struct Placement {
    /// Models with their height offset.
    parts: Vec<(Arc<PlacedBlock>, f64)>,
    color: Option<([f32; 4], f64)>,
}

impl Placement {
    /// The model whose faces neighbours see: only plain single models, as
    /// in the original (multipart blocks and blocks resolved later have
    /// none).
    fn face_model(&self, seed: u64, randomize: bool) -> Option<&RenderModel> {
        match self.parts.as_slice() {
            [(placed, offset)] if *offset == 0.0 && placed.models.len() == 1 => {
                pick_weighted(&placed.models[0], seed, randomize)
            }
            _ => None,
        }
    }
}

/// Placements, each kept once.
#[derive(Default)]
struct Placements {
    list: Vec<Placement>,
    by_key: HashMap<String, u32>,
}

impl Placements {
    fn add(&mut self, key: String, make: &mut dyn FnMut() -> Placement) -> u32 {
        if let Some(&id) = self.by_key.get(&key) {
            return id;
        }
        self.list.push(make());
        let id = (self.list.len() - 1) as u32;
        self.by_key.insert(key, id);
        id
    }
}

/// Builds the meshes of a grid, grouped by texture.
pub fn build_grid(pack: &AssetPack, grid: &Grid) -> Vec<(String, MeshData)> {
    let blocks = pack.blocks();
    let size = grid.size.map(|s| s.max(1));
    let index = |p: [i64; 3]| ((p[2] as usize * size[1]) + p[1] as usize) * size[0] + p[0] as usize;

    let mut placed_cache: HashMap<String, Arc<PlacedBlock>> = HashMap::new();
    let mut placed = |def: &BlockDef, state: &[(String, String)], first_only: bool| -> Arc<PlacedBlock> {
        let key = format!("{}{}", if first_only { "first:" } else { "" }, state_key(&def.name, state));
        placed_cache
            .entry(key)
            .or_insert_with(|| {
                let mut block = blocks.placed(pack, def, state);
                if first_only {
                    // The first model of the state, without alternatives.
                    block.models = block.models.first().and_then(|c| c.first()).map(|m| vec![vec![m.clone()]]).unwrap_or_default();
                }
                Arc::new(block)
            })
            .clone()
    };
    let mut store = Placements::default();

    // Step 1: states and models from the neighbouring blocks.
    const NONE: u32 = u32::MAX;
    const LATER: u32 = u32::MAX - 1;
    let total = size[0] * size[1] * size[2];
    let mut cells = vec![NONE; total];
    for z in 0..size[2] as i64 {
        for y in 0..size[1] as i64 {
            for x in 0..size[0] as i64 {
                let p = [x, y, z];
                let Some(block) = block_at(grid, p) else { continue };
                let def = block.def;
                if grid.skip_timelines && def.timeline && !def.model_double {
                    continue;
                }
                if def.require_models {
                    cells[index(p)] = LATER;
                    continue;
                }
                let context = Context { grid, p, def, state: &block.state, faces: Default::default() };
                let outcome = set_rules(&context);
                let color = block_color(def, &block.state);
                let key = format!("{outcome:?}{}", def.name);
                let id = store.add(key, &mut || {
                    let parts = match &outcome {
                        Outcome::State(state) => vec![(placed(def, state, false), 0.0)],
                        Outcome::Halves(halves) => {
                            halves.iter().map(|(state, offset)| (placed(def, state, true), *offset)).collect()
                        }
                        Outcome::Nothing => Vec::new(),
                    };
                    Placement { parts, color }
                });
                cells[index(p)] = id;
            }
        }
    }

    let neighbour_faces = |p: [i64; 3], cells: &[u32], placements: &[Placement]| -> [NeighbourFace; 6] {
        Dir::ALL.map(|dir| {
            let s = dir.step();
            let n = [p[0] + s[0], p[1] + s[1], p[2] + s[2]];
            if (0..3).any(|i| n[i] < 0 || n[i] >= size[i] as i64) {
                return NeighbourFace::default();
            }
            let id = cells[index(n)];
            if id >= LATER {
                return NeighbourFace::default();
            }
            match placements[id as usize].face_model(position_hash(n, 0), grid.randomize) {
                Some(model) => {
                    let o = dir.opposite() as usize;
                    NeighbourFace { full: model.face_full[o], min_depth: model.face_min_depth[o] }
                }
                None => NeighbourFace::default(),
            }
        })
    };

    // Step 2: blocks that needed the faces of their neighbours.
    let mut resolved: Vec<(usize, u32)> = Vec::new();
    for z in 0..size[2] as i64 {
        for y in 0..size[1] as i64 {
            for x in 0..size[0] as i64 {
                let p = [x, y, z];
                if cells[index(p)] != LATER {
                    continue;
                }
                let Some(block) = block_at(grid, p) else { continue };
                let def = block.def;
                let faces = neighbour_faces(p, &cells, &store.list);
                let context = Context { grid, p, def, state: &block.state, faces };
                let state = generate_rules(&context);
                let color = block_color(def, &state);
                let key = format!("later:{}", state_key(&def.name, &state));
                let id = store.add(key, &mut || Placement { parts: vec![(placed(def, &state, false), 0.0)], color });
                resolved.push((index(p), id));
            }
        }
    }

    // Liquids take their shape from the levels and faces around them.
    let solid_face = |q: [i64; 3], side: Dir| -> bool {
        if (0..3).any(|i| q[i] < 0 || q[i] >= size[i] as i64) {
            return false;
        }
        let id = cells[index(q)];
        if id >= LATER {
            return false;
        }
        store.list[id as usize]
            .face_model(position_hash(q, 0), grid.randomize)
            .is_some_and(|m| m.face_full[side as usize] && m.face_min_depth[side as usize] == Some(Depth::Opaque))
    };
    let waterlogged_at = |q: [i64; 3]| block_at(grid, q).is_some_and(|b| b.waterlogged);
    let liquid = |def: &BlockDef, p: [i64; 3], level: i64, waterlogged: bool, out: &mut HashMap<String, MeshData>| {
        let level_at = |q: [i64; 3]| {
            let b = block_at(grid, q).filter(|b| b.def.name == def.name)?;
            Some(value(&b.state, "level").parse::<i64>().unwrap_or(0))
        };
        let surroundings = LiquidSurroundings {
            level_at: &level_at,
            waterlogged_at: &waterlogged_at,
            solid_face: &solid_face,
            animation: grid.liquid_animation,
        };
        liquid_mesh(&def.name, p, level, waterlogged, def.emissive, &surroundings, out);
    };

    // Generate. Blocks resolved in step 2 keep hiding nothing, as in the
    // original, so their placements are looked up separately.
    let mut late: HashMap<usize, u32> = resolved.into_iter().collect();
    let mut out: HashMap<String, MeshData> = HashMap::new();
    for z in 0..size[2] as i64 {
        for y in 0..size[1] as i64 {
            for x in 0..size[0] as i64 {
                let p = [x, y, z];
                let i = index(p);
                let id = match cells[i] {
                    NONE => continue,
                    LATER => match late.remove(&i) {
                        Some(id) => id,
                        None => continue,
                    },
                    id => id,
                };
                let placement = &store.list[id as usize];
                if let Some(block) = block_at(grid, p) {
                    if block.def.kind == "liquid" {
                        let level = value(&block.state, "level").parse().unwrap_or(0);
                        liquid(block.def, p, level, false, &mut out);
                    } else if block.waterlogged {
                        if let Some(water) = blocks.def("water") {
                            liquid(water, p, 0, true, &mut out);
                        }
                    }
                }
                if placement.parts.is_empty() {
                    continue;
                }
                let seed = position_hash(p, 0);
                let neighbours: [Option<&RenderModel>; 6] = Dir::ALL.map(|dir| {
                    let s = dir.step();
                    let n = [p[0] + s[0], p[1] + s[1], p[2] + s[2]];
                    if (0..3).any(|i| n[i] < 0 || n[i] >= size[i] as i64) {
                        return None;
                    }
                    let id = cells[index(n)];
                    if id >= LATER {
                        return None;
                    }
                    store.list[id as usize].face_model(position_hash(n, 0), grid.randomize)
                });
                let leaves = block_at(grid, p).is_some_and(|b| b.def.kind == "leaves");

                for (part, height) in &placement.parts {
                    let here: Vec<&RenderModel> =
                        part.models.iter().filter_map(|choices| pick_weighted(choices, seed, grid.randomize)).collect();
                    let mut offset = [x as f64 * BLOCK, y as f64 * BLOCK, z as f64 * BLOCK + height];
                    let offset_xy = match part.offset {
                        RandomOffset::Xyz => total > 1,
                        RandomOffset::Xy => size[0] * size[1] > 1,
                        RandomOffset::None => false,
                    };
                    if offset_xy {
                        let h = position_hash(p, 1);
                        offset[0] += (h % 9) as f64 - 4.0;
                        offset[1] += ((h >> 8) % 9) as f64 - 4.0;
                        if part.offset == RandomOffset::Xyz {
                            offset[2] -= ((h >> 16) % 4) as f64;
                        }
                    }
                    // Only the lower half of a door or plant borders the
                    // neighbours it is tested against.
                    let culled = |model: &RenderModel, element: &crate::blocks::RenderElement, dir: Dir| {
                        *height == 0.0 && face_culled(model, element, dir, neighbours[dir as usize], leaves)
                    };
                    let (color, emissive) = match placement.color {
                        Some((color, emissive)) => (color, emissive),
                        None => ([1.0; 4], part.emissive),
                    };
                    block_mesh(&here, offset, emissive, color, &culled, &mut out);
                }
            }
        }
    }
    let mut meshes: Vec<(String, MeshData)> = out.into_iter().collect();
    meshes.sort_by(|a, b| a.0.cmp(&b.0));
    meshes
}

impl Blocks {
    /// Meshes of a block in a state repeated `size` times along each axis
    /// (`temp_update_block`).
    pub fn grid_meshes(
        &self,
        pack: &AssetPack,
        block: &BlockDef,
        state: &[(String, String)],
        size: [usize; 3],
        randomize: bool,
    ) -> Vec<(String, MeshData)> {
        let palette = [GridBlock { def: block, state: state.to_vec(), waterlogged: false }];
        let grid = Grid {
            size,
            palette: &palette,
            cell: &|_| Some(0),
            source: GridSource::default(),
            randomize,
            skip_timelines: false,
            liquid_animation: true,
        };
        build_grid(pack, &grid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack() -> AssetPack {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/Data/Minecraft");
        AssetPack::open(&folder, "1.20.2").unwrap()
    }

    fn block<'a>(pack: &'a AssetPack, name: &str, pairs: &[(&str, &str)]) -> GridBlock<'a> {
        let def = pack.blocks().def(name).unwrap();
        GridBlock { def, state: from_pairs(def, pairs), waterlogged: false }
    }

    /// A grid from a list of blocks at positions.
    struct Layout<'a> {
        size: [usize; 3],
        palette: Vec<GridBlock<'a>>,
        at: Vec<([usize; 3], usize)>,
    }

    impl<'a> Layout<'a> {
        fn new(size: [usize; 3]) -> Self {
            Self { size, palette: Vec::new(), at: Vec::new() }
        }

        fn put(mut self, p: [usize; 3], block: GridBlock<'a>) -> Self {
            self.palette.push(block);
            self.at.push((p, self.palette.len() - 1));
            self
        }

        fn with<R>(&self, source: GridSource, f: impl FnOnce(&Grid) -> R) -> R {
            let cell = |p: [usize; 3]| self.at.iter().find(|(q, _)| *q == p).map(|(_, i)| *i);
            let grid = Grid {
                size: self.size,
                palette: &self.palette,
                cell: &cell,
                source,
                randomize: false,
                skip_timelines: false,
                liquid_animation: true,
            };
            f(&grid)
        }
    }

    const LEGACY: GridSource = GridSource { scenery: true, legacy: true };
    const MODERN: GridSource = GridSource { scenery: true, legacy: false };

    fn set_at(layout: &Layout, source: GridSource, p: [usize; 3]) -> Outcome {
        layout.with(source, |grid| {
            let p = p.map(|v| v as i64);
            let b = block_at(grid, p).unwrap();
            let c = Context { grid, p, def: b.def, state: &b.state, faces: Default::default() };
            set_rules(&c)
        })
    }

    fn generate_at(layout: &Layout, source: GridSource, p: [usize; 3], faces: [NeighbourFace; 6]) -> State {
        layout.with(source, |grid| {
            let p = p.map(|v| v as i64);
            let b = block_at(grid, p).unwrap();
            let c = Context { grid, p, def: b.def, state: &b.state, faces };
            generate_rules(&c)
        })
    }

    fn state_of(outcome: Outcome) -> State {
        match outcome {
            Outcome::State(s) => s,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn stairs_form_corners_in_legacy_files_only() {
        let pack = pack();
        let layout = Layout::new([2, 1, 1])
            .put([0, 0, 0], block(&pack, "stairs", &[("facing", "east"), ("half", "bottom")]))
            .put([1, 0, 0], block(&pack, "stairs", &[("facing", "south"), ("half", "bottom")]));
        let legacy = state_of(set_at(&layout, LEGACY, [0, 0, 0]));
        assert_eq!(value(&legacy, "shape"), "outer_right");
        let modern = state_of(set_at(&layout, MODERN, [0, 0, 0]));
        assert_eq!(value(&modern, "shape"), "straight");
    }

    #[test]
    fn fences_connect_to_fences_and_solid_faces() {
        let pack = pack();
        let fence = || block(&pack, "fence", &[("variant", "spruce")]);
        let layout = Layout::new([3, 2, 1])
            .put([0, 0, 0], fence())
            .put([1, 0, 0], fence())
            .put([1, 1, 0], block(&pack, "stone", &[]));
        let middle = generate_at(&layout, LEGACY, [1, 0, 0], Default::default());
        assert_eq!(value(&middle, "variant"), "spruce");
        assert_eq!((value(&middle, "west"), value(&middle, "east")), ("true", "false"));
        // Stone to the south counts once its face is known to be full.
        assert_eq!(value(&middle, "south"), "false");
        let mut faces: [NeighbourFace; 6] = Default::default();
        faces[Dir::South as usize] = NeighbourFace { full: true, min_depth: Some(Depth::Opaque) };
        let middle = generate_at(&layout, LEGACY, [1, 0, 0], faces);
        assert_eq!(value(&middle, "south"), "true");
        // A single fence template stays as it is.
        let single = Layout::new([1, 1, 1]).put([0, 0, 0], fence());
        let state = generate_at(&single, GridSource::default(), [0, 0, 0], faces);
        assert_eq!(value(&state, "south"), "false");
    }

    #[test]
    fn walls_drop_the_post_when_straight_and_grow_under_walls() {
        let pack = pack();
        let wall = || block(&pack, "wall", &[]);
        let layout = Layout::new([3, 1, 2]).put([0, 0, 0], wall()).put([1, 0, 0], wall()).put([2, 0, 0], wall());
        let middle = generate_at(&layout, LEGACY, [1, 0, 0], Default::default());
        assert_eq!((value(&middle, "east"), value(&middle, "west"), value(&middle, "up")), ("low", "low", "false"));
        // With a wall above it and above its eastern neighbour, the east
        // side is tall.
        let layout = layout.put([1, 0, 1], wall()).put([2, 0, 1], wall());
        let middle = generate_at(&layout, LEGACY, [1, 0, 0], Default::default());
        assert_eq!((value(&middle, "east"), value(&middle, "west")), ("tall", "low"));
    }

    #[test]
    fn doors_draw_both_halves_from_the_lower_one() {
        let pack = pack();
        let layout = Layout::new([1, 1, 2])
            .put([0, 0, 0], block(&pack, "door", &[("half", "lower"), ("facing", "east"), ("hinge", "right")]))
            .put([0, 0, 1], block(&pack, "door", &[("half", "upper"), ("hinge", "left")]));
        let Outcome::Halves(halves) = set_at(&layout, LEGACY, [0, 0, 0]) else { panic!() };
        assert_eq!(halves.len(), 2);
        assert_eq!(value(&halves[0].0, "hinge"), "left");
        assert_eq!(value(&halves[0].0, "location"), "north_west");
        assert_eq!(value(&halves[0].0, "direction"), "south");
        assert_eq!((value(&halves[1].0, "half"), halves[1].1), ("upper", 16.0));
        assert_eq!(set_at(&layout, LEGACY, [0, 0, 1]), Outcome::Nothing);
    }

    #[test]
    fn grass_under_snow_is_snowy_everywhere() {
        let pack = pack();
        let layout = Layout::new([1, 1, 2])
            .put([0, 0, 0], block(&pack, "grass_block", &[]))
            .put([0, 0, 1], block(&pack, "snow", &[]));
        assert_eq!(value(&state_of(set_at(&layout, MODERN, [0, 0, 0])), "snowy"), "true");
    }

    #[test]
    fn redstone_is_coloured_by_its_power() {
        let pack = pack();
        let def = pack.blocks().def("redstone_wire").unwrap();
        let (off, off_light) = block_color(def, &from_pairs(def, &[("power", "0")])).unwrap();
        let (on, on_light) = block_color(def, &from_pairs(def, &[("power", "15")])).unwrap();
        assert!((off[0] - 76.0 / 255.0).abs() < 1e-6 && off_light == 0.0);
        assert_eq!((on[0], on_light), (1.0, 1.0));
        // A wire that connects to nothing shows the cross.
        let layout = Layout::new([2, 1, 1]).put([0, 0, 0], GridBlock { def, state: from_pairs(def, &[]), waterlogged: false });
        let wire = generate_at(&layout, LEGACY, [0, 0, 0], Default::default());
        assert_eq!(value(&wire, "north"), "side");
    }

    #[test]
    fn a_built_fence_row_is_more_than_posts() {
        let pack = pack();
        let fence = || block(&pack, "fence", &[]);
        let triangles = |layout: &Layout| -> usize {
            layout.with(LEGACY, |grid| build_grid(&pack, grid)).iter().map(|(_, m)| m.triangle_count()).sum()
        };
        let row = Layout::new([3, 1, 1]).put([0, 0, 0], fence()).put([1, 0, 0], fence()).put([2, 0, 0], fence());
        let apart = Layout::new([5, 1, 1]).put([0, 0, 0], fence()).put([2, 0, 0], fence()).put([4, 0, 0], fence());
        assert!(triangles(&row) > triangles(&apart), "{} {}", triangles(&row), triangles(&apart));
    }

    fn meshes_of(pack: &AssetPack, layout: &Layout) -> HashMap<String, MeshData> {
        layout.with(LEGACY, |grid| build_grid(pack, grid)).into_iter().collect()
    }

    fn top_z(mesh: &MeshData) -> f32 {
        mesh.vertices.iter().map(|v| v.position[2]).fold(f32::MIN, f32::max)
    }

    #[test]
    fn still_water_sits_below_the_top() {
        let pack = pack();
        let water = || block(&pack, "water", &[("level", "0")]);
        let one = meshes_of(&pack, &Layout::new([1, 1, 1]).put([0, 0, 0], water()));
        assert_eq!(top_z(&one["block/water_still"]), 14.0);
        // Four sides of two triangles plus a corner triangle each.
        assert_eq!(one["block/water_flow"].triangle_count(), 12);
        // Top (four triangles) and bottom (two).
        assert_eq!(one["block/water_still"].triangle_count(), 6);

        // Neighbouring water hides the side between them.
        let pool = meshes_of(&pack, &Layout::new([2, 1, 1]).put([0, 0, 0], water()).put([1, 0, 0], water()));
        assert_eq!(pool["block/water_flow"].triangle_count(), 18);

        // Water under water fills its block and has no top.
        let column = meshes_of(&pack, &Layout::new([1, 1, 2]).put([0, 0, 0], water()).put([0, 0, 1], water()));
        assert_eq!(top_z(&column["block/water_still"]), 30.0);
        let lower_sides = column["block/water_flow"].vertices.iter().filter(|v| v.position[2] <= 16.0).count();
        assert!(lower_sides > 0);
    }

    #[test]
    fn water_flows_down_towards_lower_levels() {
        let pack = pack();
        let layout = Layout::new([2, 1, 1])
            .put([0, 0, 0], block(&pack, "water", &[("level", "1")]))
            .put([1, 0, 0], block(&pack, "water", &[("level", "4")]));
        let meshes = meshes_of(&pack, &layout);
        // Both tops flow, so no still texture is used for them.
        assert!(!meshes.contains_key("block/water_still"), "{:?}", meshes.keys().collect::<Vec<_>>());
        // The surface slopes: lower towards the higher level number.
        let flow = &meshes["block/water_flow"];
        let z_at = |x: f32| {
            flow.vertices.iter().filter(|v| v.position[0] == x).map(|v| v.position[2]).fold(f32::MIN, f32::max)
        };
        assert!(z_at(0.0) > z_at(32.0), "{} {}", z_at(0.0), z_at(32.0));
    }

    #[test]
    fn waterlogged_blocks_hold_water_and_lava_glows() {
        let pack = pack();
        let mut stairs = block(&pack, "stairs", &[("half", "bottom")]);
        stairs.waterlogged = true;
        let meshes = meshes_of(&pack, &Layout::new([1, 1, 1]).put([0, 0, 0], stairs));
        let water = &meshes["block/water_flow"];
        // The sides are pulled in a little.
        let max_x = water.vertices.iter().map(|v| v.position[0]).fold(f32::MIN, f32::max);
        assert!((max_x - 15.95).abs() < 1e-4, "{max_x}");
        assert!(meshes.keys().any(|k| k.contains("planks")));

        let lava = meshes_of(&pack, &Layout::new([1, 1, 1]).put([0, 0, 0], block(&pack, "lava", &[])));
        assert!(lava["block/lava_still"].vertices.iter().all(|v| v.custom[2] == 1.0));
    }
}
