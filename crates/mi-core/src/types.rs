//! Object type tables. The string names are what project files store
//! (`temp_type_name_list`, `tl_type_name_list`, `res_type_name_list`).

use serde::{Deserialize, Serialize};

/// Declares a fieldless enum with a stable order and a file name per variant.
macro_rules! named_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        $vis enum $name { $($variant),+ }

        impl $name {
            /// All variants in the original enum order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// Name used in project files.
            pub const fn name(self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }

            pub fn from_name(name: &str) -> Option<Self> {
                match name { $($text => Some($name::$variant),)+ _ => None }
            }

            /// Position in the original enum.
            pub const fn index(self) -> usize {
                self as usize
            }
        }
    };
}

named_enum! {
    /// Library item kinds (`e_temp_type`).
    pub enum TempType {
        Character => "char",
        SpecialBlock => "spblock",
        Scenery => "scenery",
        Item => "item",
        Block => "block",
        Bodypart => "bodypart",
        ParticleSpawner => "particles",
        Text => "text",
        Cube => "cube",
        Cone => "cone",
        Cylinder => "cylinder",
        Sphere => "sphere",
        Surface => "surface",
        Model => "model",
    }
}

impl TempType {
    /// `type_is_shape`
    pub const fn is_shape(self) -> bool {
        matches!(
            self,
            TempType::Cube | TempType::Cone | TempType::Cylinder | TempType::Sphere | TempType::Surface
        )
    }

    /// Whether the template is built from a Minecraft model definition.
    pub const fn uses_minecraft_model(self) -> bool {
        matches!(self, TempType::Character | TempType::SpecialBlock | TempType::Bodypart)
    }

    /// The timeline type an instance of this template gets.
    pub const fn tl_type(self) -> TlType {
        match self {
            TempType::Character => TlType::Character,
            TempType::SpecialBlock => TlType::SpecialBlock,
            TempType::Scenery => TlType::Scenery,
            TempType::Item => TlType::Item,
            TempType::Block => TlType::Block,
            TempType::Bodypart => TlType::Bodypart,
            TempType::ParticleSpawner => TlType::ParticleSpawner,
            TempType::Text => TlType::Text,
            TempType::Cube => TlType::Cube,
            TempType::Cone => TlType::Cone,
            TempType::Cylinder => TlType::Cylinder,
            TempType::Sphere => TlType::Sphere,
            TempType::Surface => TlType::Surface,
            TempType::Model => TlType::Model,
        }
    }
}

named_enum! {
    /// Timeline kinds (`e_tl_type`). The first 14 match [`TempType`].
    ///
    /// `Shape` and `LightSource` are pseudo types used by the workbench; they
    /// are listed in the file order of `tl_type_name_list`, which is the
    /// reverse of the original enum for these two.
    pub enum TlType {
        Character => "char",
        SpecialBlock => "spblock",
        Scenery => "scenery",
        Item => "item",
        Block => "block",
        Bodypart => "bodypart",
        ParticleSpawner => "particles",
        Text => "text",
        Cube => "cube",
        Cone => "cone",
        Cylinder => "cylinder",
        Sphere => "sphere",
        Surface => "surface",
        Model => "model",
        Camera => "camera",
        SpotLight => "spotlight",
        PointLight => "pointlight",
        Folder => "folder",
        Background => "background",
        Audio => "audio",
        Path => "path",
        PathPoint => "pathpoint",
        Shape => "shape",
        LightSource => "lightsource",
    }
}

impl TlType {
    /// The template type with the same meaning, for timelines that are
    /// instances of a template.
    pub const fn temp_type(self) -> Option<TempType> {
        Some(match self {
            TlType::Character => TempType::Character,
            TlType::SpecialBlock => TempType::SpecialBlock,
            TlType::Scenery => TempType::Scenery,
            TlType::Item => TempType::Item,
            TlType::Block => TempType::Block,
            TlType::Bodypart => TempType::Bodypart,
            TlType::ParticleSpawner => TempType::ParticleSpawner,
            TlType::Text => TempType::Text,
            TlType::Cube => TempType::Cube,
            TlType::Cone => TempType::Cone,
            TlType::Cylinder => TempType::Cylinder,
            TlType::Sphere => TempType::Sphere,
            TlType::Surface => TempType::Surface,
            TlType::Model => TempType::Model,
            _ => return None,
        })
    }

    pub const fn is_shape(self) -> bool {
        matches!(
            self,
            TlType::Cube | TlType::Cone | TlType::Cylinder | TlType::Sphere | TlType::Surface
        )
    }

    /// `type_is_timeline`: kinds that exist only as timelines, without a
    /// library template behind them.
    pub const fn is_timeline_only(self) -> bool {
        matches!(
            self,
            TlType::Camera
                | TlType::PointLight
                | TlType::SpotLight
                | TlType::Background
                | TlType::Folder
                | TlType::Audio
                | TlType::LightSource
                | TlType::Path
                | TlType::PathPoint
        )
    }

    /// Whether the timeline has wind settings in its appearance block.
    pub const fn has_wind_settings(self) -> bool {
        matches!(self, TlType::Scenery | TlType::Block | TlType::ParticleSpawner | TlType::Text)
            || self.is_shape()
    }

    /// Which groups of values a timeline of this type exposes
    /// (`tl_update_value_types`). `has_bend` tells whether the timeline is a
    /// body part whose model part can bend.
    pub fn value_types(self, has_bend: bool) -> ValueTypes {
        use ValueType as V;
        let mut t = ValueTypes::default();

        if self == TlType::Audio {
            t.set(V::Sound);
            t.set(V::Audio);
            return t;
        }

        t.set(V::Keyframe);

        if self == TlType::Background {
            t.set(V::Background);
            return t;
        }

        if self == TlType::Path {
            t.set(V::Path);
        }

        if self == TlType::PathPoint {
            t.set(V::Transform);
            t.set(V::TransformPos);
            t.set(V::TransformPathPoint);
            t.set(V::Hierarchy);
            return t;
        }

        let is_light = matches!(self, TlType::PointLight | TlType::SpotLight);
        let is_camera = self == TlType::Camera;

        t.set(V::Hierarchy);
        if !is_camera {
            t.set(V::Appearance);
        }

        t.set(V::Transform);
        t.set(V::TransformPos);
        if self != TlType::PointLight {
            t.set(V::TransformRot);
        }
        if self != TlType::ParticleSpawner && !is_camera && !is_light {
            t.set(V::TransformSca);
        }
        if self == TlType::Bodypart && has_bend {
            t.set(V::TransformBend);
        }

        if self == TlType::ParticleSpawner {
            t.set(V::Particles);
        }
        if is_light {
            t.set(V::Light);
        }
        if self == TlType::SpotLight {
            t.set(V::Spotlight);
        }
        if is_camera {
            t.set(V::Camera);
        }

        if !is_camera && !is_light && self != TlType::Folder {
            t.set(V::MaterialTexture);
        }
        if !is_light {
            t.set(V::MaterialColor);
        }
        if !is_camera && !is_light {
            t.set(V::MaterialSurface);
            t.set(V::MaterialSubsurface);
        }

        if self == TlType::Text {
            t.set(V::Text);
        }
        if self == TlType::Item {
            t.set(V::Item);
        }

        if self != TlType::ParticleSpawner && !is_camera && !is_light {
            t.set(V::RotPoint);
        }

        if t.has(V::MaterialTexture)
            || t.has(V::MaterialColor)
            || t.has(V::MaterialSurface)
            || t.has(V::MaterialSubsurface)
        {
            t.set(V::Material);
        }

        t
    }
}

/// Groups of animatable values (`e_value_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueType {
    Transform,
    TransformPos,
    TransformRot,
    TransformSca,
    TransformBend,
    TransformPathPoint,
    Material,
    MaterialTexture,
    MaterialColor,
    MaterialSurface,
    MaterialSubsurface,
    Particles,
    Light,
    Spotlight,
    Camera,
    Background,
    Sound,
    Text,
    Keyframe,
    RotPoint,
    Hierarchy,
    Appearance,
    Audio,
    Item,
    Path,
}

/// Set of [`ValueType`] flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ValueTypes(u32);

impl ValueTypes {
    pub fn set(&mut self, t: ValueType) {
        self.0 |= 1 << t as u32;
    }

    pub fn has(self, t: ValueType) -> bool {
        self.0 & (1 << t as u32) != 0
    }
}

named_enum! {
    /// Resource kinds (`e_res_type`).
    pub enum ResType {
        Pack => "pack",
        PackUnzipped => "packunzipped",
        Skin => "skin",
        DownloadedSkin => "downloadskin",
        ItemSheet => "itemsheet",
        LegacyBlockSheet => "legacyblocksheet",
        BlockSheet => "blocksheet",
        Scenery => "scenery",
        FromWorld => "fromworld",
        ParticleSheet => "particlesheet",
        Texture => "texture",
        Font => "font",
        Sound => "sound",
        Model => "model",
    }
}

/// Declares an enum that files store as its integer position.
macro_rules! indexed_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident { $($variant:ident),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        $vis enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub const fn index(self) -> usize {
                self as usize
            }

            /// Returns `None` for numbers outside the enum.
            pub fn from_index(index: f64) -> Option<Self> {
                if index < 0.0 || index.fract() != 0.0 {
                    return None;
                }
                Self::ALL.get(index as usize).copied()
            }
        }
    };
}

indexed_enum! {
    /// How transparency is resolved (`e_alpha_mode`).
    pub enum AlphaMode { Blend, Hashed, Default }
}

indexed_enum! {
    /// Enchantment glint style (`e_glint`).
    pub enum GlintMode { None, Item, Entity }
}

indexed_enum! {
    /// Layout of a resource pack's material maps (`e_material`).
    pub enum MaterialFormat { None, Seus, LabPbr }
}

indexed_enum! {
    /// `e_tonemapper`
    pub enum Tonemapper { None, Reinhard, Aces }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for &t in TempType::ALL {
            assert_eq!(TempType::from_name(t.name()), Some(t));
        }
        for &t in TlType::ALL {
            assert_eq!(TlType::from_name(t.name()), Some(t));
        }
        for &t in ResType::ALL {
            assert_eq!(ResType::from_name(t.name()), Some(t));
        }
        assert_eq!(TlType::from_name("nope"), None);
    }

    #[test]
    fn template_and_timeline_types_line_up() {
        for &t in TempType::ALL {
            assert_eq!(t.tl_type().index(), t.index());
            assert_eq!(t.tl_type().name(), t.name());
            assert_eq!(t.tl_type().temp_type(), Some(t));
        }
        assert_eq!(TlType::Camera.temp_type(), None);
    }

    #[test]
    fn value_types_follow_the_original_rules() {
        use ValueType as V;

        let audio = TlType::Audio.value_types(false);
        assert!(audio.has(V::Sound) && audio.has(V::Audio));
        assert!(!audio.has(V::Keyframe) && !audio.has(V::Hierarchy));

        let bg = TlType::Background.value_types(false);
        assert!(bg.has(V::Background) && bg.has(V::Keyframe));
        assert!(!bg.has(V::Hierarchy) && !bg.has(V::Appearance));

        let cam = TlType::Camera.value_types(false);
        assert!(cam.has(V::Camera) && cam.has(V::Hierarchy) && cam.has(V::TransformRot));
        assert!(!cam.has(V::Appearance) && !cam.has(V::RotPoint) && !cam.has(V::TransformSca));
        assert!(cam.has(V::MaterialColor) && cam.has(V::Material));

        let point = TlType::PointLight.value_types(false);
        assert!(point.has(V::Light) && !point.has(V::Spotlight) && !point.has(V::TransformRot));
        assert!(!point.has(V::Material) && point.has(V::Appearance));

        let path = TlType::Path.value_types(false);
        assert!(path.has(V::Path) && path.has(V::RotPoint) && path.has(V::Appearance));

        let point = TlType::PathPoint.value_types(false);
        assert!(point.has(V::TransformPathPoint) && point.has(V::Hierarchy));
        assert!(!point.has(V::Appearance) && !point.has(V::RotPoint));

        assert!(TlType::Bodypart.value_types(true).has(V::TransformBend));
        assert!(!TlType::Bodypart.value_types(false).has(V::TransformBend));
        assert!(!TlType::Folder.value_types(false).has(V::MaterialTexture));
    }

    #[test]
    fn indexed_enums_reject_out_of_range() {
        assert_eq!(AlphaMode::from_index(2.0), Some(AlphaMode::Default));
        assert_eq!(AlphaMode::from_index(3.0), None);
        assert_eq!(AlphaMode::from_index(-1.0), None);
        assert_eq!(GlintMode::from_index(0.5), None);
        assert_eq!(MaterialFormat::from_index(2.0), Some(MaterialFormat::LabPbr));
    }
}
