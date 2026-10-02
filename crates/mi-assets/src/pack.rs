//! The Minecraft asset pack that ships with the program:
//! `Data/Minecraft/<version>.zip` and its manifest `<version>.midata`
//! (`minecraft_assets_load`, `model_load`, `temp_update_model`).

use crate::model_file::{ModelError, ModelFile, ModelPart};
use mi_format::json::{self, Json, JsonObject};
use mi_format::StateValue;
use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("could not read the asset archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("could not read the asset archive: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not parse the asset manifest: {0}")]
    Manifest(#[from] json::JsonError),
    #[error("the asset manifest has no \"{0}\" list")]
    MissingList(&'static str),
}

/// What one value of a model state changes (`obj_model_state`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StateOption {
    pub value: String,
    /// Model file used instead of the default.
    pub file: Option<String>,
    /// Texture per part name; `""` is the whole model.
    pub texture: HashMap<String, String>,
    /// Texture per shape description.
    pub shape_texture: HashMap<String, String>,
    pub hide: Vec<String>,
    pub shape_hide: Vec<String>,
    /// Palette colour name per shape description.
    pub shape_color: HashMap<String, String>,
    pub pattern_type: Option<String>,
}

/// A character or special block of the asset pack (`obj_model`).
#[derive(Debug, Clone, PartialEq)]
pub struct ModelDef {
    pub name: String,
    /// `character` or `special_block`: the folder its files are in.
    pub folder: &'static str,
    pub file: Option<String>,
    pub texture: HashMap<String, String>,
    pub version: f64,
    pub pattern_type: String,
    /// States in manifest order, each with its possible values.
    pub states: Vec<(String, Vec<StateOption>)>,
    pub default_state: Vec<(String, String)>,
}

/// A model with its states applied: what to draw and with which textures.
#[derive(Debug, Clone)]
pub struct ResolvedModel {
    pub file: Arc<ModelFile>,
    /// Texture per part name (`""` for the model), after states.
    pub texture: HashMap<String, String>,
    pub shape_texture: HashMap<String, String>,
    pub hide: Vec<String>,
    pub shape_hide: Vec<String>,
    pub pattern_type: String,
}

impl ResolvedModel {
    /// Texture of a part (`model_part_get_texture_name`): one set by the
    /// model's state for the part, the part's own, or that of the nearest
    /// ancestor with one, else the model's.
    pub fn part_texture(&self, part_name: &str) -> String {
        fn search<'a>(parts: &'a [ModelPart], name: &str, chain: &mut Vec<&'a ModelPart>) -> bool {
            for part in parts {
                chain.push(part);
                if part.name == name || search(&part.parts, name, chain) {
                    return true;
                }
                chain.pop();
            }
            false
        }
        let mut chain = Vec::new();
        search(&self.file.parts, part_name, &mut chain);
        for part in chain.iter().rev() {
            if let Some(texture) = self.texture.get(&part.name) {
                return texture.clone();
            }
            if let Some(texture) = &part.texture_name {
                return texture.clone();
            }
        }
        self.texture.get("").cloned().unwrap_or_else(|| self.file.texture_name.clone())
    }

    /// Texture of a shape: a state's shape texture, the shape's own, or its
    /// part's.
    pub fn shape_texture(&self, part_name: &str, description: &str, own: Option<&str>) -> String {
        if let Some(texture) = self.shape_texture.get(description) {
            return texture.clone();
        }
        match own {
            Some(texture) => texture.to_owned(),
            None => self.part_texture(part_name),
        }
    }
}

fn string_map(json: Option<&Json>) -> HashMap<String, String> {
    match json {
        Some(Json::String(s)) => HashMap::from([(String::new(), s.clone())]),
        Some(Json::Object(map)) => {
            map.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.to_owned(), v.to_owned()))).collect()
        }
        _ => HashMap::new(),
    }
}

fn string_list(json: Option<&Json>) -> Vec<String> {
    json.and_then(Json::as_array)
        .map(|items| items.iter().filter_map(|i| i.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

/// `string_get_state_vars`: `"a=1,b=2"`.
pub fn parse_state_vars(text: &str) -> Vec<(String, String)> {
    text.split(',')
        .filter(|s| !s.is_empty())
        .filter_map(|pair| pair.split_once('=').map(|(k, v)| (k.to_owned(), v.to_owned())))
        .collect()
}

impl ModelDef {
    fn load(map: &JsonObject, folder: &'static str) -> Option<Self> {
        let states = map
            .object("states")
            .map(|states| {
                states
                    .iter()
                    .map(|(name, values)| {
                        let options = values
                            .as_array()
                            .unwrap_or_default()
                            .iter()
                            .filter_map(Json::as_object)
                            .map(|v| StateOption {
                                value: match v.get("value") {
                                    Some(Json::String(s)) => s.clone(),
                                    Some(other) => other.as_real().map(json::format_number).unwrap_or_default(),
                                    None => String::new(),
                                },
                                file: v.string("file").map(str::to_owned),
                                texture: string_map(v.get("texture")),
                                shape_texture: string_map(v.get("shape_texture")),
                                hide: string_list(v.get("hide")),
                                shape_hide: string_list(v.get("shape_hide")),
                                shape_color: string_map(v.get("shape_color")),
                                pattern_type: v.string("pattern_type").map(str::to_owned),
                            })
                            .collect();
                        (name.to_owned(), options)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            name: map.string("name")?.to_owned(),
            folder,
            file: map.string("file").map(str::to_owned),
            texture: string_map(map.get("texture")),
            version: map.real("version").unwrap_or(0.0),
            pattern_type: map.string("pattern_type").unwrap_or("").to_owned(),
            states,
            default_state: map.string("default_state").map(parse_state_vars).unwrap_or_default(),
        })
    }
}

/// The loaded asset pack. Files stay compressed in memory and are read on
/// demand; parsed model files are cached.
pub struct AssetPack {
    archive: Mutex<zip::ZipArchive<std::io::Cursor<Vec<u8>>>>,
    models: HashMap<String, ModelDef>,
    /// Model names in manifest order: characters first, then special blocks.
    model_order: Vec<String>,
    model_files: Mutex<HashMap<String, Arc<ModelFile>>>,
    manifest: JsonObject,
}

impl std::fmt::Debug for AssetPack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssetPack").field("models", &self.models.len()).finish_non_exhaustive()
    }
}

const ASSETS: &str = "assets/minecraft/";

impl AssetPack {
    /// Opens the pack from the archive and manifest contents.
    pub fn load(zip_bytes: Vec<u8>, manifest: &[u8]) -> Result<Self, PackError> {
        let archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))?;
        let manifest = match json::parse(manifest)? {
            Json::Object(map) => map,
            _ => return Err(PackError::MissingList("characters")),
        };
        let mut models = HashMap::new();
        let mut model_order = Vec::new();
        for (key, folder) in [("characters", "character"), ("special_blocks", "special_block")] {
            let list = manifest.array(key).ok_or(PackError::MissingList(if key == "characters" {
                "characters"
            } else {
                "special_blocks"
            }))?;
            for def in list.iter().filter_map(Json::as_object).filter_map(|m| ModelDef::load(m, folder)) {
                model_order.push(def.name.clone());
                models.insert(def.name.clone(), def);
            }
        }
        Ok(Self { archive: Mutex::new(archive), models, model_order, model_files: Mutex::default(), manifest })
    }

    /// Opens the pack from the `Data/Minecraft` folder for `version`.
    pub fn open(folder: &std::path::Path, version: &str) -> Result<Self, PackError> {
        let zip = std::fs::read(folder.join(format!("{version}.zip")))?;
        let manifest = std::fs::read(folder.join(format!("{version}.midata")))?;
        Self::load(zip, &manifest)
    }

    /// The parsed manifest, for lists not modelled here yet.
    pub fn manifest(&self) -> &JsonObject {
        &self.manifest
    }

    /// A file of the archive, by its path below `assets/minecraft/`.
    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        let mut archive = self.archive.lock().unwrap_or_else(|e| e.into_inner());
        let mut entry = archive.by_name(&format!("{ASSETS}{path}")).ok()?;
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes).ok()?;
        Some(bytes)
    }

    pub fn model(&self, name: &str) -> Option<&ModelDef> {
        self.models.get(name)
    }

    pub fn model_names(&self) -> &[String] {
        &self.model_order
    }

    /// A model file of the pack, parsed once.
    pub fn model_file(&self, folder: &str, file: &str) -> Result<Arc<ModelFile>, ModelError> {
        let path = format!("models/{folder}/{file}");
        if let Some(model) = self.model_files.lock().unwrap_or_else(|e| e.into_inner()).get(&path) {
            return Ok(model.clone());
        }
        let bytes = self.read(&path).ok_or(ModelError::Missing("model file"))?;
        let model = Arc::new(ModelFile::load(&bytes)?);
        self.model_files.lock().unwrap_or_else(|e| e.into_inner()).insert(path, model.clone());
        Ok(model)
    }

    /// Applies a state to a model (`temp_update_model`). Values not given
    /// in `state` take the model's default.
    pub fn resolve(&self, name: &str, state: &[(String, StateValue)]) -> Option<ResolvedModel> {
        let def = self.models.get(name)?;
        let value_of = |state_name: &str| -> Option<String> {
            state
                .iter()
                .find(|(n, _)| n == state_name)
                .map(|(_, v)| v.to_text())
                .or_else(|| def.default_state.iter().find(|(n, _)| n == state_name).map(|(_, v)| v.clone()))
        };

        let mut file = def.file.clone();
        let mut texture = def.texture.clone();
        let mut shape_texture = HashMap::new();
        let mut hide = Vec::new();
        let mut shape_hide = Vec::new();
        let mut pattern_type = def.pattern_type.clone();
        for (state_name, options) in &def.states {
            let Some(value) = value_of(state_name) else { continue };
            let Some(option) = options.iter().find(|o| o.value == value) else { continue };
            if option.file.is_some() {
                file = option.file.clone();
            }
            if let Some(pattern) = &option.pattern_type {
                pattern_type = pattern.clone();
            }
            texture.extend(option.texture.clone());
            shape_texture.extend(option.shape_texture.clone());
            hide.extend(option.hide.iter().cloned());
            shape_hide.extend(option.shape_hide.iter().cloned());
        }

        let file = self.model_file(def.folder, file.as_deref()?).ok()?;
        Some(ResolvedModel { file, texture, shape_texture, hide, shape_hide, pattern_type })
    }

    /// A texture of the pack as RGBA, padded to a square like
    /// `texture_create_square` (model UVs assume square textures).
    pub fn texture(&self, name: &str) -> Option<Rgba> {
        let bytes = self.read(&format!("textures/{name}.png"))?;
        decode_square(&bytes)
    }
}

/// An image in memory.
#[derive(Debug, Clone, PartialEq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Decodes a PNG or JPEG and pads it with transparency to a square.
pub fn decode_square(bytes: &[u8]) -> Option<Rgba> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let side = w.max(h);
    if w == h {
        return Some(Rgba { width: w, height: h, pixels: image.into_raw() });
    }
    let mut pixels = vec![0u8; (side * side * 4) as usize];
    for (y, row) in image.as_raw().chunks((w * 4) as usize).enumerate() {
        let start = y * (side * 4) as usize;
        pixels[start..start + row.len()].copy_from_slice(row);
    }
    Some(Rgba { width: side, height: side, pixels })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_vars_are_parsed() {
        assert_eq!(
            parse_state_vars("variant=normal,facing=north"),
            vec![("variant".into(), "normal".into()), ("facing".into(), "north".into())]
        );
        assert!(parse_state_vars("").is_empty());
    }

    #[test]
    fn padding_to_square() {
        let mut png = Vec::new();
        let img = image::RgbaImage::from_pixel(4, 2, image::Rgba([255, 0, 0, 255]));
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let out = decode_square(&png).unwrap();
        assert_eq!((out.width, out.height), (4, 4));
        assert_eq!(&out.pixels[0..4], &[255, 0, 0, 255]);
        assert_eq!(&out.pixels[(4 * 4 * 3) as usize..(4 * 4 * 3 + 4) as usize], &[0, 0, 0, 0]);
    }
}
