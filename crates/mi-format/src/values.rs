//! Sets of animatable values and how they are written to project files
//! (`project_save_values` / `project_load_values`).

use crate::json::{Json, JsonObject, JsonWriter};
use crate::project::Background;
use mi_core::version::project as fmt;
use mi_core::{ObjRef, Value, ValueId, ValueKind, VALUE_COUNT};
use std::ops::{Index, IndexMut};

/// One [`Value`] per [`ValueId`]: the state of a keyframe, or the defaults of
/// a timeline or project.
#[derive(Debug, Clone, PartialEq)]
pub struct ValueSet(Box<[Value]>);

impl ValueSet {
    /// The defaults of a project (`app.value_default`), which timelines start
    /// from. `ground_slot` is the block texture index of the default ground
    /// and `seed` the particle seed, which the original picks at random
    /// (1..=32000) every time a project is reset.
    pub fn project_defaults(background: &Background, ground_slot: f64, seed: f64) -> Self {
        let values = ValueId::ALL
            .iter()
            .map(|&id| match id.static_default() {
                Some(value) => value,
                None if id == ValueId::Seed => Value::Number(seed),
                None if id == ValueId::BgGroundSlot => Value::Number(ground_slot),
                None => background
                    .value(id)
                    .expect("every BG_* value except the ground slot mirrors a background setting"),
            })
            .collect();
        Self(values)
    }

    pub fn iter(&self) -> impl Iterator<Item = (ValueId, &Value)> {
        ValueId::ALL.iter().copied().zip(self.0.iter())
    }

    pub fn number(&self, id: ValueId) -> f64 {
        self[id].as_f64()
    }

    pub fn flag(&self, id: ValueId) -> bool {
        self[id].as_bool()
    }

    /// Stores `value` if it has the representation `id` expects.
    /// Returns whether it was stored.
    pub fn set(&mut self, id: ValueId, value: Value) -> bool {
        let ok = value.matches_kind(id.kind());
        if ok {
            self[id] = value;
        }
        ok
    }

    /// Writes the values that differ from `defaults` as an object called
    /// `name`.
    pub(crate) fn save_diff(&self, w: &mut JsonWriter, name: &str, defaults: &ValueSet) {
        w.object_start(Some(name));
        for (id, value) in self.iter() {
            if *value == defaults[id] {
                continue;
            }
            let key = id.name();
            match value {
                Value::Bool(b) => w.var_bool(key, *b),
                Value::Color(c) if id.color_stored_as_integer() => w.var(key, c.to_gm() as f64),
                Value::Color(c) => w.var_color(key, *c),
                Value::Str(s) => w.var(key, s),
                Value::Number(n) => w.var(key, *n),
                Value::Ref(ObjRef::Id(save_id)) => w.var(key, save_id.as_str()),
                Value::Ref(ObjRef::None) => w.var(key, "none"),
                // `save_id_get(null)` hands back the null constant itself,
                // which is the number -4.
                Value::Ref(ObjRef::Null) => w.var(key, -4.0),
            }
        }
        w.object_done();
    }

    /// Reads the values present in `map` over the current ones. Keys of
    /// values that were renamed in later versions are translated according to
    /// `format`; unknown keys are ignored.
    pub(crate) fn load_from(&mut self, map: &JsonObject, format: i32) {
        for (key, json) in map.iter() {
            let Some(id) = ValueId::from_name(update_value_name(key, format)) else {
                continue;
            };
            if let Some(value) = read_value(id, json) {
                self[id] = value;
            }
        }
    }
}

impl Index<ValueId> for ValueSet {
    type Output = Value;
    fn index(&self, id: ValueId) -> &Value {
        &self.0[id.index()]
    }
}

impl IndexMut<ValueId> for ValueSet {
    fn index_mut(&mut self, id: ValueId) -> &mut Value {
        &mut self.0[id.index()]
    }
}

const _: () = assert!(VALUE_COUNT > 0);

/// `project_load_values_update_name`
fn update_value_name(name: &str, format: i32) -> &str {
    if format >= fmt::FORMAT_200_PRE_5 {
        return name;
    }
    match name {
        // Cloud height used to be called Z; the old "height" meant
        // thickness, which is no longer a value.
        "BG_SKY_CLOUDS_Z" => "BG_SKY_CLOUDS_HEIGHT",
        "BG_SKY_CLOUDS_HEIGHT" => "",
        "BRIGHTNESS" => "EMISSIVE",
        "CAM_SHAKE_HORIZONTAL_SPEED" => "CAM_SHAKE_SPEED_X",
        "CAM_SHAKE_VERTICAL_SPEED" => "CAM_SHAKE_SPEED_Y",
        "CAM_SHAKE_HORIZONTAL_STRENGTH" => "CAM_SHAKE_STRENGTH_X",
        "CAM_SHAKE_VERTICAL_STRENGTH" => "CAM_SHAKE_STRENGTH_Y",
        other => other,
    }
}

/// Converts a JSON value into the representation of `id`. Returns `None`
/// when the JSON value cannot be used, in which case the previous value is
/// kept.
fn read_value(id: ValueId, json: &Json) -> Option<Value> {
    match id.kind() {
        ValueKind::Bool => json.as_flag().map(Value::Bool),
        ValueKind::Color => match json {
            Json::String(s) => Some(Value::Color(mi_core::Color::from_hex(s))),
            Json::Number(n) if id.color_stored_as_integer() => {
                Some(Value::Color(mi_core::Color::from_gm(n.clamp(0.0, 16_777_215.0) as u32)))
            }
            _ => None,
        },
        ValueKind::String => json.as_str().map(|s| Value::Str(s.to_owned())),
        ValueKind::Number => json.as_real().map(Value::Number),
        ValueKind::Texture | ValueKind::Object => Some(Value::Ref(match json {
            Json::String(s) if s == "null" => ObjRef::Null,
            Json::String(s) if s == "none" && id.kind() == ValueKind::Texture => ObjRef::None,
            Json::String(s) => ObjRef::id(s.as_str()),
            // The original stores 0 for "no texture" and a negative number
            // for null.
            Json::Number(n) if *n == 0.0 && id.kind() == ValueKind::Texture => ObjRef::None,
            Json::Number(_) | Json::Null => ObjRef::Null,
            _ => return None,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::parse;
    use mi_core::Color;

    fn defaults() -> ValueSet {
        ValueSet::project_defaults(&Background::default(), 1.0, 1234.0)
    }

    #[test]
    fn project_defaults_follow_the_background() {
        let d = defaults();
        assert_eq!(d[ValueId::Seed], Value::Number(1234.0));
        assert_eq!(d[ValueId::BgGroundSlot], Value::Number(1.0));
        assert_eq!(d[ValueId::BgSkyTime], Value::Number(-45.0));
        assert_eq!(d[ValueId::BgBiome], Value::Str("plains".into()));
        assert_eq!(d[ValueId::BgSkyColor], Value::Color(Color::rgb(120, 167, 255)));
        assert_eq!(d[ValueId::BgTwilight], Value::Number(1.0));
        assert_eq!(d[ValueId::BgFogShow], Value::Bool(true));
        for (id, value) in d.iter() {
            assert!(value.matches_kind(id.kind()), "{id:?}: {value:?}");
        }
    }

    #[test]
    fn only_differences_are_written() {
        let d = defaults();
        let mut v = d.clone();
        v[ValueId::PosX] = Value::Number(12.5);
        v[ValueId::Visible] = Value::Bool(false);
        v[ValueId::RgbAdd] = Value::Color(Color::rgb(255, 0, 16));
        v[ValueId::Text] = Value::Str("Hi\n\"you\"".into());
        v[ValueId::TextureObj] = Value::Ref(ObjRef::None);
        v[ValueId::SoundObj] = Value::Ref(ObjRef::id("ABCDEFGHIJKLMNOP"));

        let mut w = JsonWriter::new();
        w.object_start(None);
        v.save_diff(&mut w, "values", &d);
        w.object_done();
        let text = w.finish();
        assert_eq!(
            text,
            "{\r\n\t\"values\": {\r\n\t\t\"POS_X\": 12.5,\r\n\t\t\"RGB_ADD\": \"#FF0010\",\r\n\
             \t\t\"TEXTURE_OBJ\": \"none\",\r\n\t\t\"SOUND_OBJ\": \"ABCDEFGHIJKLMNOP\",\r\n\
             \t\t\"TEXT\": \"Hi\\n\\\"you\\\"\",\r\n\t\t\"VISIBLE\": false\r\n\t}\r\n}"
        );

        let parsed = parse(text.as_bytes()).unwrap();
        let mut loaded = d.clone();
        loaded.load_from(parsed.as_object().unwrap().object("values").unwrap(), fmt::CURRENT);
        assert_eq!(loaded, v);
    }

    #[test]
    fn null_reference_round_trips_as_minus_four() {
        let mut d = defaults();
        d[ValueId::TextureObj] = Value::Ref(ObjRef::id("TEX"));
        let mut v = d.clone();
        v[ValueId::TextureObj] = Value::Ref(ObjRef::Null);

        let mut w = JsonWriter::new();
        w.object_start(None);
        v.save_diff(&mut w, "v", &d);
        w.object_done();
        let text = w.finish();
        assert!(text.contains("\"TEXTURE_OBJ\": -4"), "{text}");

        let parsed = parse(text.as_bytes()).unwrap();
        let mut loaded = d.clone();
        loaded.load_from(parsed.as_object().unwrap().object("v").unwrap(), fmt::CURRENT);
        assert_eq!(loaded, v);
    }

    #[test]
    fn old_value_names_are_translated() {
        let doc = parse(
            br#"{"BRIGHTNESS": 0.5, "CAM_SHAKE_VERTICAL_SPEED": 3, "BG_SKY_CLOUDS_HEIGHT": 99, "NOT_A_VALUE": 1}"#,
        )
        .unwrap();
        let map = doc.as_object().unwrap();

        let mut old = defaults();
        old.load_from(map, fmt::FORMAT_125);
        assert_eq!(old[ValueId::Emissive], Value::Number(0.5));
        assert_eq!(old[ValueId::CamShakeSpeedY], Value::Number(3.0));
        assert_eq!(old[ValueId::BgSkyCloudsHeight], defaults()[ValueId::BgSkyCloudsHeight]);

        let z = parse(br#"{"BG_SKY_CLOUDS_Z": 512}"#).unwrap();
        old.load_from(z.as_object().unwrap(), fmt::FORMAT_125);
        assert_eq!(old[ValueId::BgSkyCloudsHeight], Value::Number(512.0));

        let mut new = defaults();
        new.load_from(map, fmt::CURRENT);
        assert_eq!(new[ValueId::Emissive], Value::Number(0.0));
        assert_eq!(new[ValueId::BgSkyCloudsHeight], Value::Number(99.0));
    }

    #[test]
    fn fog_object_color_is_an_integer_on_disk() {
        let d = defaults();
        let mut v = d.clone();
        v[ValueId::BgFogObjectColor] = Value::Color(Color::rgb(1, 2, 3));
        let mut w = JsonWriter::new();
        w.object_start(None);
        v.save_diff(&mut w, "v", &d);
        w.object_done();
        let text = w.finish();
        assert!(text.contains("\"BG_FOG_OBJECT_COLOR\": 197121"), "{text}");

        let parsed = parse(text.as_bytes()).unwrap();
        let mut loaded = d.clone();
        loaded.load_from(parsed.as_object().unwrap().object("v").unwrap(), fmt::CURRENT);
        assert_eq!(loaded, v);

        let hex = parse(br##"{"BG_FOG_OBJECT_COLOR": "#010203"}"##).unwrap();
        let mut from_hex = d.clone();
        from_hex.load_from(hex.as_object().unwrap(), fmt::CURRENT);
        assert_eq!(from_hex, v);
    }

    #[test]
    fn wrong_types_keep_the_previous_value() {
        let doc = parse(br#"{"POS_X": "oops", "VISIBLE": "yes", "RGB_ADD": 5, "TEXT": 1, "SPAWN": 0}"#).unwrap();
        let mut v = defaults();
        v.load_from(doc.as_object().unwrap(), fmt::CURRENT);
        let d = defaults();
        assert_eq!(v[ValueId::PosX], d[ValueId::PosX]);
        assert_eq!(v[ValueId::Visible], d[ValueId::Visible]);
        assert_eq!(v[ValueId::RgbAdd], d[ValueId::RgbAdd]);
        assert_eq!(v[ValueId::Text], d[ValueId::Text]);
        assert_eq!(v[ValueId::Spawn], Value::Bool(false));
    }
}
