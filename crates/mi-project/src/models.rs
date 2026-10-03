//! Binds timelines to Minecraft models: which model a character or special
//! block shows, and which part of it each body part timeline is
//! (`temp_update_model`, `temp_update_model_timeline_parts`).

use crate::Project;
use mi_anim::PartInfo;
use mi_assets::{AssetPack, ModelPart, ResolvedModel};
use mi_core::{ObjRef, ResType, TempType, TlType};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Where the textures of a model come from.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelTextures {
    /// The textures of the asset pack.
    Pack,
    /// One image for every texture name: a skin or texture file.
    Image {
        path: PathBuf,
        /// It is a player skin, which may be in the layout from before
        /// Minecraft 1.8 (see `mi_assets::player_skin`).
        player_skin: bool,
    },
}

/// The model part a timeline draws.
#[derive(Debug, Clone)]
pub struct PartBinding {
    pub model: Arc<ResolvedModel>,
    pub part_name: String,
    pub textures: ModelTextures,
    /// Key that identifies the model and state, for caching meshes.
    pub model_key: String,
}

impl PartBinding {
    pub fn part(&self) -> Option<&ModelPart> {
        self.model.file.find_part(&self.part_name)
    }

    /// Whether the model's state hides this part.
    pub fn hidden(&self) -> bool {
        self.model.hide.iter().any(|h| h == &self.part_name)
    }
}

/// Model bindings of all timelines of a project.
#[derive(Debug, Default)]
pub struct ModelBindings {
    parts: HashMap<usize, PartBinding>,
}

impl ModelBindings {
    /// Resolves the model of every body part timeline.
    pub fn bind(project: &Project, pack: &AssetPack) -> Self {
        let mut resolved: HashMap<String, Option<Arc<ResolvedModel>>> = HashMap::new();
        let mut parts = HashMap::new();

        for (index, timeline) in project.timelines().iter().enumerate() {
            if timeline.kind != TlType::Bodypart {
                continue;
            }
            let Some(temp_id) = timeline.temp.as_id() else { continue };

            // The model comes from the template, or for special blocks that
            // are part of scenery from that timeline itself.
            let (name, state, texture_ref, template_part) = if let Some(template) = project.template(temp_id) {
                if !matches!(template.kind, TempType::Character | TempType::SpecialBlock | TempType::Bodypart) {
                    continue;
                }
                (template.model_name.clone(), template.model_state.clone(), template.model_tex.clone(), template.model_part_name.clone())
            } else if let Some((name, state)) = project.timeline(temp_id).and_then(|t| t.part_model.clone()) {
                (name, state, ObjRef::default_resource(), String::new())
            } else {
                continue;
            };

            let state_key: Vec<String> = state.iter().map(|(k, v)| format!("{k}={}", v.to_text())).collect();
            let model_key = format!("{name}|{}", state_key.join(","));
            let Some(model) = resolved
                .entry(model_key.clone())
                .or_insert_with(|| pack.resolve(&name, &state).map(Arc::new))
                .clone()
            else {
                continue;
            };

            let part_name =
                if timeline.model_part_name.is_empty() { template_part } else { timeline.model_part_name.clone() };
            if model.file.find_part(&part_name).is_none() {
                continue;
            }

            let textures = match texture_ref.as_id().and_then(|id| project.resource(id)) {
                Some(res) if matches!(res.kind, ResType::Skin | ResType::DownloadedSkin | ResType::Texture) => {
                    match project.resource_path(res) {
                        Some(path) => ModelTextures::Image { path, player_skin: res.player_skin },
                        None => ModelTextures::Pack,
                    }
                }
                _ => ModelTextures::Pack,
            };
            let model_key = match &textures {
                ModelTextures::Pack => model_key,
                ModelTextures::Image { path, .. } => format!("{model_key}|{}", path.display()),
            };
            parts.insert(index, PartBinding { model, part_name, textures, model_key });
        }
        Self { parts }
    }

    pub fn part(&self, timeline: usize) -> Option<&PartBinding> {
        self.parts.get(&timeline)
    }

    /// Model part data for the transform update.
    pub fn part_info(&self, timeline: usize) -> Option<PartInfo> {
        self.parts.get(&timeline).and_then(PartBinding::part).map(ModelPart::info)
    }

    pub fn len(&self) -> usize {
        self.parts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}
