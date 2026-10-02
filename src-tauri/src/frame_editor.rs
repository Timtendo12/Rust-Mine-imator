//! The frame editor: the values of a timeline at the current frame, grouped
//! the way the original's frame editor tab groups them
//! (`tab_frame_editor_*`), for the frontend to show and change.

use mi_core::{TlType, Value, ValueId, ValueKind, ValueType};
use serde::Serialize;

/// How the frontend edits a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlKind {
    Number,
    Bool,
    Color,
    /// One of `options`.
    Choice,
    Text,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValueEntry {
    /// Name in project files, such as `POS_X`.
    pub name: &'static str,
    pub label: String,
    pub kind: ControlKind,
    /// Number, boolean, `#RRGGBB` or text.
    pub value: serde_json::Value,
    /// Change per pixel when dragging a number.
    pub step: f64,
    pub options: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValueGroup {
    pub title: &'static str,
    pub values: Vec<ValueEntry>,
}

/// The group a value is shown in and the value type a timeline needs for
/// it; `None` for values the frame editor does not show (yet): references
/// to other objects, which need pickers, and the bezier handles.
fn group_of(id: ValueId) -> Option<(&'static str, ValueType)> {
    use ValueType as V;
    let name = id.name();
    if matches!(id.kind(), ValueKind::Object | ValueKind::Texture) {
        return None;
    }
    let camera = |title| Some((title, V::Camera));
    Some(match name {
        "POS_X" | "POS_Y" | "POS_Z" => ("Position", V::TransformPos),
        "ROT_X" | "ROT_Y" | "ROT_Z" => ("Rotation", V::TransformRot),
        "SCA_X" | "SCA_Y" | "SCA_Z" => ("Scale", V::TransformSca),
        "BEND_ANGLE_X" | "BEND_ANGLE_Y" | "BEND_ANGLE_Z" => ("Bend", V::TransformBend),
        "BEND_ANGLE" => return None,
        "PATH_POINT_ANGLE" | "PATH_POINT_SCALE" => ("Path point", V::TransformPathPoint),
        "PATH_OFFSET" => ("Path", V::Transform),
        "IK_BLEND" | "IK_ANGLE_OFFSET" => return None,
        "ALPHA" | "RGB_ADD" | "RGB_SUB" | "RGB_MUL" | "HSB_ADD" | "HSB_SUB" | "HSB_MUL" | "MIX_COLOR" | "MIX_PERCENT"
        | "GLOW_COLOR" => ("Color", V::MaterialColor),
        "EMISSIVE" | "METALLIC" | "ROUGHNESS" | "WIND_INFLUENCE" => ("Surface", V::MaterialSurface),
        _ if name.starts_with("SUBSURFACE") => ("Subsurface", V::MaterialSubsurface),
        "SPAWN" | "FREEZE" | "CLEAR" | "CUSTOM_SEED" | "SEED" | "FORCE" | "FORCE_DIRECTIONAL" | "FORCE_VORTEX" => {
            ("Particles", V::Particles)
        }
        _ if name.starts_with("LIGHT_SPOT") => ("Spotlight", V::Spotlight),
        _ if name.starts_with("LIGHT_") => ("Light", V::Light),
        _ if name.starts_with("CAM_ROTATE") => return camera("Camera rotation point"),
        _ if name.starts_with("CAM_SHAKE") => return camera("Camera shake"),
        _ if name.starts_with("CAM_DOF") => return camera("Depth of field"),
        _ if name.starts_with("CAM_BLOOM") => return camera("Bloom"),
        _ if name.starts_with("CAM_LENS_DIRT") => return camera("Lens dirt"),
        "CAM_COLOR_CORRECTION" | "CAM_CONTRAST" | "CAM_BRIGHTNESS" | "CAM_SATURATION" | "CAM_VIBRANCE" | "CAM_COLOR_BURN" => {
            return camera("Color correction")
        }
        _ if name.starts_with("CAM_GRAIN") => return camera("Film grain"),
        _ if name.starts_with("CAM_VIGNETTE") => return camera("Vignette"),
        _ if name.starts_with("CAM_CA") => return camera("Chromatic aberration"),
        _ if name.starts_with("CAM_DISTORT") => return camera("Distortion"),
        "CAM_SIZE_USE_PROJECT" | "CAM_SIZE_KEEP_ASPECT_RATIO" | "CAM_WIDTH" | "CAM_HEIGHT" => return camera("Camera size"),
        _ if name.starts_with("CAM_") => return camera("Camera"),
        _ if name.starts_with("BG_") => ("Background", V::Background),
        _ if name.starts_with("SOUND_") => ("Sound", V::Sound),
        _ if name.starts_with("TEXT") => ("Text", V::Text),
        "CUSTOM_ITEM_SLOT" | "ITEM_SLOT" | "ITEM_NAME" => ("Item", V::Item),
        "VISIBLE" | "TRANSITION" => ("Keyframe", V::Keyframe),
        _ => return None,
    })
}

/// Words that are dropped from labels because the group title says them.
fn label(id: ValueId, title: &str) -> String {
    let name = id.name();
    let short = match title {
        "Position" | "Rotation" | "Scale" => &name[4..],
        "Bend" => &name["BEND_ANGLE_".len()..],
        "Light" => name.trim_start_matches("LIGHT_"),
        "Spotlight" => name.trim_start_matches("LIGHT_SPOT_"),
        "Background" => name.trim_start_matches("BG_"),
        "Sound" => name.trim_start_matches("SOUND_"),
        "Subsurface" if name != "SUBSURFACE" => name.trim_start_matches("SUBSURFACE_"),
        _ if name.starts_with("CAM_") => {
            let rest = &name[4..];
            let prefix = match title {
                "Camera rotation point" => "ROTATE",
                "Camera shake" => "SHAKE",
                "Depth of field" => "DOF",
                "Bloom" => "BLOOM",
                "Lens dirt" => "LENS_DIRT",
                "Film grain" => "GRAIN",
                "Vignette" => "VIGNETTE",
                "Chromatic aberration" => "CA",
                "Distortion" => "DISTORT",
                _ => "",
            };
            // The value that switches the effect on keeps the full word.
            match rest.strip_prefix(prefix).map(|r| r.trim_start_matches('_')) {
                Some("") => "ENABLED",
                Some(r) if !prefix.is_empty() => r,
                _ => rest,
            }
        }
        _ => name,
    };
    let mut text = short.replace('_', " ").to_lowercase();
    if let Some(first) = text.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    match text.as_str() {
        "Fov" => "Field of view".into(),
        "Rgb add" => "Add".into(),
        "Rgb sub" => "Subtract".into(),
        "Rgb mul" => "Multiply".into(),
        "Hsb add" => "HSB add".into(),
        "Hsb sub" => "HSB subtract".into(),
        "Hsb mul" => "HSB multiply".into(),
        _ => text,
    }
}

fn step(id: ValueId) -> f64 {
    use ValueId::*;
    match id {
        PosX | PosY | PosZ | RotX | RotY | RotZ | BendAngleX | BendAngleY | BendAngleZ | LightRange | LightFadeSize
        | CamFov | CamRotateAngleXy | CamRotateAngleZ | CamRotateDistance | PathPointAngle => 0.5,
        Alpha | MixPercent | Metallic | Roughness | Emissive | Subsurface | WindInfluence => 0.005,
        _ => 0.01,
    }
}

fn options(id: ValueId) -> Vec<&'static str> {
    match id {
        ValueId::Transition => mi_anim::Transition::ALL.iter().map(|t| t.name()).collect(),
        ValueId::TextHalign => vec!["left", "center", "right"],
        ValueId::TextValign => vec!["top", "center", "bottom"],
        _ => Vec::new(),
    }
}

/// The frame editor of a timeline of type `kind` with the given values.
pub fn value_groups(kind: TlType, has_bend: bool, values: &mi_format::ValueSet) -> Vec<ValueGroup> {
    let types = kind.value_types(has_bend);
    let mut groups: Vec<ValueGroup> = Vec::new();
    for &id in ValueId::ALL {
        let Some((title, needs)) = group_of(id) else { continue };
        if !types.has(needs) {
            continue;
        }
        let options = options(id);
        let (control, value) = match &values[id] {
            Value::Number(n) => (ControlKind::Number, serde_json::json!(n)),
            Value::Bool(b) => (ControlKind::Bool, serde_json::json!(b)),
            Value::Color(c) => (ControlKind::Color, serde_json::json!(c.to_hex())),
            Value::Str(s) if !options.is_empty() => (ControlKind::Choice, serde_json::json!(s)),
            Value::Str(s) => (ControlKind::Text, serde_json::json!(s)),
            Value::Ref(_) => continue,
        };
        let entry = ValueEntry { name: id.name(), label: label(id, title), kind: control, value, step: step(id), options };
        match groups.iter_mut().find(|g| g.title == title) {
            Some(group) => group.values.push(entry),
            None => groups.push(ValueGroup { title, values: vec![entry] }),
        }
    }
    // The original's order: keyframe settings come last.
    groups.sort_by_key(|g| g.title == "Keyframe");
    groups
}

/// A value from the frontend in the representation `id` stores.
pub fn parse_value(id: ValueId, value: &serde_json::Value) -> Option<Value> {
    Some(match id.kind() {
        ValueKind::Number => Value::Number(match value {
            serde_json::Value::Bool(b) => *b as u8 as f64,
            other => other.as_f64()?,
        }),
        ValueKind::Bool => Value::Bool(match value {
            serde_json::Value::Bool(b) => *b,
            other => other.as_f64()? != 0.0,
        }),
        ValueKind::Color => Value::Color(mi_core::Color::from_hex(value.as_str()?)),
        ValueKind::String => Value::Str(value.as_str()?.to_owned()),
        ValueKind::Object | ValueKind::Texture => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn groups(kind: TlType, has_bend: bool) -> Vec<ValueGroup> {
        let file = mi_format::project::ProjectFile::new(0.0, 1.0);
        value_groups(kind, has_bend, &file.defaults)
    }

    fn titles(groups: &[ValueGroup]) -> Vec<&str> {
        groups.iter().map(|g| g.title).collect()
    }

    #[test]
    fn groups_follow_the_timeline_type() {
        let cube = groups(TlType::Cube, false);
        assert_eq!(titles(&cube)[..3], ["Position", "Rotation", "Scale"]);
        assert_eq!(*titles(&cube).last().unwrap(), "Keyframe");
        assert!(!titles(&cube).contains(&"Bend") && !titles(&cube).contains(&"Camera"));
        let position: Vec<&str> = cube[0].values.iter().map(|v| v.label.as_str()).collect();
        assert_eq!(position, ["X", "Y", "Z"]);

        assert!(titles(&groups(TlType::Bodypart, true)).contains(&"Bend"));
        assert!(!titles(&groups(TlType::Bodypart, false)).contains(&"Bend"));

        let light = groups(TlType::PointLight, false);
        assert!(titles(&light).contains(&"Light") && !titles(&light).contains(&"Rotation"));
        assert!(titles(&groups(TlType::SpotLight, false)).contains(&"Spotlight"));

        let camera = groups(TlType::Camera, false);
        assert!(titles(&camera).contains(&"Depth of field") && !titles(&camera).contains(&"Scale"));
        let general = camera.iter().find(|g| g.title == "Camera").unwrap();
        assert_eq!(general.values[0].label, "Field of view");
        let dof = camera.iter().find(|g| g.title == "Depth of field").unwrap();
        assert_eq!((dof.values[0].label.as_str(), dof.values[0].kind), ("Enabled", ControlKind::Bool));
        assert_eq!(dof.values[1].label, "Depth");
    }

    #[test]
    fn kinds_and_parsing() {
        let cube = groups(TlType::Cube, false);
        let keyframe = cube.last().unwrap();
        let transition = keyframe.values.iter().find(|v| v.name == "TRANSITION").unwrap();
        assert_eq!(transition.kind, ControlKind::Choice);
        assert!(transition.options.contains(&"easeinoutquad"));
        let color = cube.iter().find(|g| g.title == "Color").unwrap();
        let mul = color.values.iter().find(|v| v.name == "RGB_MUL").unwrap();
        assert_eq!((mul.kind, mul.label.as_str(), mul.value.as_str()), (ControlKind::Color, "Multiply", Some("#FFFFFF")));

        assert_eq!(parse_value(ValueId::PosX, &serde_json::json!(2.5)), Some(Value::Number(2.5)));
        assert_eq!(parse_value(ValueId::Visible, &serde_json::json!(false)), Some(Value::Bool(false)));
        assert_eq!(
            parse_value(ValueId::RgbMul, &serde_json::json!("#FF0000")),
            Some(Value::Color(mi_core::Color::rgb(255, 0, 0)))
        );
        assert_eq!(parse_value(ValueId::Transition, &serde_json::json!("instant")), Some(Value::Str("instant".into())));
        assert_eq!(parse_value(ValueId::PosX, &serde_json::json!("x")), None);
        assert_eq!(parse_value(ValueId::PathObj, &serde_json::json!("x")), None);
    }
}
