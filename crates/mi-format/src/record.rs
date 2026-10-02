//! `record!`: declares a settings struct together with its defaults and its
//! JSON field list, so that the three cannot drift apart.
//!
//! Each field names its JSON key, its kind and its default. Fields are
//! written in declaration order, which is the order the original writes
//! them. Loading keeps the current value when a key is missing or has the
//! wrong type, like the `value_get_*` scripts.
//!
//! Kinds:
//!
//! | kind       | Rust type        | written as                         |
//! |------------|------------------|------------------------------------|
//! | `num`      | `f64`            | number                             |
//! | `bool`     | `bool`           | `true` / `false`                   |
//! | `flag`     | `bool`           | `1` / `0`                          |
//! | `color`    | `Color`          | `"#RRGGBB"`                        |
//! | `string`   | `String`         | string                             |
//! | `point3`   | `[f64; 3]`       | `[ X, Z, Y ]`                      |
//! | `flags3`   | `[bool; 3]`      | `[ X, Z, Y ]` of `1` / `0`         |
//! | `obj`      | `ObjRef`         | save id or `"null"`                |
//! | `opt_obj`  | `ObjRef`         | save id, omitted when null         |
//! | `nullable` | `Option<f64>`    | number or `"null"`                 |

use crate::json::{Atom, JsonObject, JsonWriter};
use mi_core::{Color, ObjRef};

macro_rules! record {
    (
        $(#[$meta:meta])*
        pub struct $name:ident {
            $( $(#[$fmeta:meta])* $key:literal => $field:ident : $kind:ident = $default:expr ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq)]
        pub struct $name {
            $( $(#[$fmeta])* pub $field: $crate::record::record_type!($kind), )*
        }

        impl Default for $name {
            fn default() -> Self {
                Self { $( $field: ($default).into(), )* }
            }
        }

        impl $name {
            /// Writes the fields in file order into the current object.
            #[allow(dead_code)]
            pub(crate) fn save_fields(&self, w: &mut $crate::json::JsonWriter) {
                $( $crate::record::Field::save(&self.$field, $crate::record::kind::$kind, w, $key); )*
            }

            /// Reads every field that is present in `map`.
            #[allow(dead_code)]
            pub(crate) fn load_fields(&mut self, map: &$crate::json::JsonObject) {
                $( $crate::record::Field::load(&mut self.$field, $crate::record::kind::$kind, map, $key); )*
            }
        }
    };
}

macro_rules! record_type {
    (num) => { f64 };
    (bool) => { bool };
    (flag) => { bool };
    (color) => { mi_core::Color };
    (string) => { String };
    (point3) => { [f64; 3] };
    (flags3) => { [bool; 3] };
    (obj) => { mi_core::ObjRef };
    (opt_obj) => { mi_core::ObjRef };
    (nullable) => { Option<f64> };
}

pub(crate) use record;
pub(crate) use record_type;

/// Marker types selecting how a field is written; one per `record!` kind.
#[allow(non_camel_case_types)]
pub(crate) mod kind {
    pub struct num;
    pub struct bool;
    pub struct flag;
    pub struct color;
    pub struct string;
    pub struct point3;
    pub struct flags3;
    pub struct obj;
    pub struct opt_obj;
    pub struct nullable;
}

pub(crate) trait Field<K> {
    fn save(&self, kind: K, w: &mut JsonWriter, key: &str);
    fn load(&mut self, kind: K, map: &JsonObject, key: &str);
}

impl Field<kind::num> for f64 {
    fn save(&self, _: kind::num, w: &mut JsonWriter, key: &str) {
        w.var(key, *self);
    }
    fn load(&mut self, _: kind::num, map: &JsonObject, key: &str) {
        if let Some(v) = map.real(key) {
            *self = v;
        }
    }
}

impl Field<kind::bool> for bool {
    fn save(&self, _: kind::bool, w: &mut JsonWriter, key: &str) {
        w.var_bool(key, *self);
    }
    fn load(&mut self, _: kind::bool, map: &JsonObject, key: &str) {
        if let Some(v) = map.flag(key) {
            *self = v;
        }
    }
}

impl Field<kind::flag> for bool {
    fn save(&self, _: kind::flag, w: &mut JsonWriter, key: &str) {
        w.var_flag(key, *self);
    }
    fn load(&mut self, _: kind::flag, map: &JsonObject, key: &str) {
        if let Some(v) = map.flag(key) {
            *self = v;
        }
    }
}

impl Field<kind::color> for Color {
    fn save(&self, _: kind::color, w: &mut JsonWriter, key: &str) {
        w.var_color(key, *self);
    }
    fn load(&mut self, _: kind::color, map: &JsonObject, key: &str) {
        if let Some(v) = map.color(key) {
            *self = v;
        }
    }
}

impl Field<kind::string> for String {
    fn save(&self, _: kind::string, w: &mut JsonWriter, key: &str) {
        w.var(key, self);
    }
    fn load(&mut self, _: kind::string, map: &JsonObject, key: &str) {
        if let Some(v) = map.string(key) {
            *self = v.to_owned();
        }
    }
}

impl Field<kind::point3> for [f64; 3] {
    fn save(&self, _: kind::point3, w: &mut JsonWriter, key: &str) {
        w.var_point3(key, *self);
    }
    fn load(&mut self, _: kind::point3, map: &JsonObject, key: &str) {
        if let Some(v) = map.point3(key) {
            *self = v;
        }
    }
}

impl Field<kind::flags3> for [bool; 3] {
    fn save(&self, _: kind::flags3, w: &mut JsonWriter, key: &str) {
        w.var_point3(key, self.map(|b| b as u8 as f64));
    }
    fn load(&mut self, _: kind::flags3, map: &JsonObject, key: &str) {
        if let Some(v) = map.point3(key) {
            *self = v.map(|n| n > 0.5);
        }
    }
}

/// `json_save_var_save_id`
pub(crate) fn save_obj(w: &mut JsonWriter, key: &str, value: &ObjRef) {
    match value {
        ObjRef::Id(id) => w.var(key, id.as_str()),
        ObjRef::Null | ObjRef::None => w.var(key, "null"),
    }
}

/// `value_get_save_id`: `"null"` clears the reference, any other string is an
/// id, anything else leaves the current value alone.
pub(crate) fn load_obj(map: &JsonObject, key: &str) -> Option<ObjRef> {
    match map.string(key)? {
        "null" => Some(ObjRef::Null),
        id => Some(ObjRef::id(id)),
    }
}

impl Field<kind::obj> for ObjRef {
    fn save(&self, _: kind::obj, w: &mut JsonWriter, key: &str) {
        save_obj(w, key, self);
    }
    fn load(&mut self, _: kind::obj, map: &JsonObject, key: &str) {
        if let Some(v) = load_obj(map, key) {
            *self = v;
        }
    }
}

impl Field<kind::opt_obj> for ObjRef {
    fn save(&self, _: kind::opt_obj, w: &mut JsonWriter, key: &str) {
        if !self.is_null() {
            save_obj(w, key, self);
        }
    }
    fn load(&mut self, _: kind::opt_obj, map: &JsonObject, key: &str) {
        if let Some(v) = load_obj(map, key) {
            *self = v;
        }
    }
}

impl Field<kind::nullable> for Option<f64> {
    fn save(&self, _: kind::nullable, w: &mut JsonWriter, key: &str) {
        w.var_nullable(key, *self);
    }
    fn load(&mut self, _: kind::nullable, map: &JsonObject, key: &str) {
        if let Some(v) = map.nullable_real(key) {
            *self = v;
        }
    }
}

/// `json_save_var_state_vars`: an object of `name: value` pairs.
pub(crate) fn save_state_vars(w: &mut JsonWriter, key: &str, vars: &[(String, StateValue)]) {
    w.object_start(Some(key));
    for (name, value) in vars {
        match value {
            StateValue::Str(s) => w.var(name, Atom::Str(s)),
            StateValue::Num(n) => w.var(name, *n),
        }
    }
    w.object_done();
}

/// `value_get_state_vars`
pub(crate) fn load_state_vars(map: &JsonObject, key: &str) -> Vec<(String, StateValue)> {
    let Some(vars) = map.object(key) else {
        return Vec::new();
    };
    vars.iter()
        .filter_map(|(name, value)| {
            let value = match value {
                crate::json::Json::String(s) => StateValue::Str(s.clone()),
                other => StateValue::Num(other.as_real()?),
            };
            Some((name.to_owned(), value))
        })
        .collect()
}

/// Value of a block or model state variable (`"facing": "north"`).
#[derive(Debug, Clone, PartialEq)]
pub enum StateValue {
    Str(String),
    Num(f64),
}

impl StateValue {
    /// Text form, as block state matching compares values.
    pub fn to_text(&self) -> String {
        match self {
            StateValue::Str(s) => s.clone(),
            StateValue::Num(n) => crate::json::format_number(*n),
        }
    }
}
