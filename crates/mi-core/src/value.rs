//! Animatable values (`e_value`) and their static properties.
//!
//! Every timeline and keyframe holds one [`Value`] per [`ValueId`]. The names
//! are what project files use as keys (`value_name_list`).

use crate::{Color, ObjRef};
use serde::{Deserialize, Serialize};

/// How a value is stored and written to files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueKind {
    /// Plain number. Some on/off values are numbers rather than booleans in
    /// the original (`BG_TWILIGHT`, `CAM_LENS_DIRT`, ...) and one colour is
    /// stored as a GameMaker colour integer (`BG_FOG_OBJECT_COLOR`); files
    /// depend on that, so it is kept.
    Number,
    /// `tl_value_is_bool`
    Bool,
    /// `tl_value_is_color`
    Color,
    /// `tl_value_is_string`, plus `ITEM_NAME` which is also text.
    String,
    /// `tl_value_is_texture`: reference to a resource, or "none".
    Texture,
    /// Reference to another object (timeline or resource).
    Object,
}

macro_rules! value_ids {
    ($($variant:ident $name:literal $kind:ident,)+) => {
        /// Identifier of an animatable value, in the order of `e_value`.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[repr(u16)]
        pub enum ValueId { $($variant),+ }

        impl ValueId {
            /// All values in enum order.
            pub const ALL: &'static [ValueId] = &[$(ValueId::$variant),+];

            /// Key used in project files.
            pub const fn name(self) -> &'static str {
                match self { $(ValueId::$variant => $name),+ }
            }

            pub fn from_name(name: &str) -> Option<Self> {
                match name { $($name => Some(ValueId::$variant),)+ _ => None }
            }

            pub const fn kind(self) -> ValueKind {
                match self { $(ValueId::$variant => ValueKind::$kind),+ }
            }
        }
    };
}

value_ids! {
    PosX "POS_X" Number,
    PosY "POS_Y" Number,
    PosZ "POS_Z" Number,
    RotX "ROT_X" Number,
    RotY "ROT_Y" Number,
    RotZ "ROT_Z" Number,
    ScaX "SCA_X" Number,
    ScaY "SCA_Y" Number,
    ScaZ "SCA_Z" Number,
    BendAngleLegacy "BEND_ANGLE" Number,
    BendAngleX "BEND_ANGLE_X" Number,
    BendAngleY "BEND_ANGLE_Y" Number,
    BendAngleZ "BEND_ANGLE_Z" Number,
    Alpha "ALPHA" Number,
    RgbAdd "RGB_ADD" Color,
    RgbSub "RGB_SUB" Color,
    RgbMul "RGB_MUL" Color,
    HsbAdd "HSB_ADD" Color,
    HsbSub "HSB_SUB" Color,
    HsbMul "HSB_MUL" Color,
    MixColor "MIX_COLOR" Color,
    GlowColor "GLOW_COLOR" Color,
    MixPercent "MIX_PERCENT" Number,
    Emissive "EMISSIVE" Number,
    Metallic "METALLIC" Number,
    Roughness "ROUGHNESS" Number,
    Subsurface "SUBSURFACE" Number,
    SubsurfaceRadiusRed "SUBSURFACE_RADIUS_RED" Number,
    SubsurfaceRadiusGreen "SUBSURFACE_RADIUS_GREEN" Number,
    SubsurfaceRadiusBlue "SUBSURFACE_RADIUS_BLUE" Number,
    SubsurfaceColor "SUBSURFACE_COLOR" Color,
    WindInfluence "WIND_INFLUENCE" Number,
    Spawn "SPAWN" Bool,
    Freeze "FREEZE" Bool,
    Clear "CLEAR" Bool,
    CustomSeed "CUSTOM_SEED" Bool,
    Seed "SEED" Number,
    Attractor "ATTRACTOR" Object,
    Force "FORCE" Number,
    ForceDirectional "FORCE_DIRECTIONAL" Number,
    ForceVortex "FORCE_VORTEX" Number,
    LightColor "LIGHT_COLOR" Color,
    LightStrength "LIGHT_STRENGTH" Number,
    LightSpecularStrength "LIGHT_SPECULAR_STRENGTH" Number,
    LightSize "LIGHT_SIZE" Number,
    LightRange "LIGHT_RANGE" Number,
    LightFadeSize "LIGHT_FADE_SIZE" Number,
    LightSpotRadius "LIGHT_SPOT_RADIUS" Number,
    LightSpotSharpness "LIGHT_SPOT_SHARPNESS" Number,
    CamFov "CAM_FOV" Number,
    CamBladeAmount "CAM_BLADE_AMOUNT" Number,
    CamBladeAngle "CAM_BLADE_ANGLE" Number,
    CamLightManagement "CAM_LIGHT_MANAGEMENT" Bool,
    CamTonemapper "CAM_TONEMAPPER" Number,
    CamExposure "CAM_EXPOSURE" Number,
    CamGamma "CAM_GAMMA" Number,
    CamRotate "CAM_ROTATE" Bool,
    CamRotateDistance "CAM_ROTATE_DISTANCE" Number,
    CamRotateAngleXy "CAM_ROTATE_ANGLE_XY" Number,
    CamRotateAngleZ "CAM_ROTATE_ANGLE_Z" Number,
    CamShake "CAM_SHAKE" Bool,
    CamShakeMode "CAM_SHAKE_MODE" Number,
    CamShakeStrengthX "CAM_SHAKE_STRENGTH_X" Number,
    CamShakeStrengthY "CAM_SHAKE_STRENGTH_Y" Number,
    CamShakeStrengthZ "CAM_SHAKE_STRENGTH_Z" Number,
    CamShakeSpeedX "CAM_SHAKE_SPEED_X" Number,
    CamShakeSpeedY "CAM_SHAKE_SPEED_Y" Number,
    CamShakeSpeedZ "CAM_SHAKE_SPEED_Z" Number,
    CamDof "CAM_DOF" Bool,
    CamDofDepth "CAM_DOF_DEPTH" Number,
    CamDofRange "CAM_DOF_RANGE" Number,
    CamDofFadeSize "CAM_DOF_FADE_SIZE" Number,
    CamDofBlurSize "CAM_DOF_BLUR_SIZE" Number,
    CamDofBlurRatio "CAM_DOF_BLUR_RATIO" Number,
    CamDofBias "CAM_DOF_BIAS" Number,
    CamDofThreshold "CAM_DOF_THRESHOLD" Number,
    CamDofGain "CAM_DOF_GAIN" Number,
    CamDofFringe "CAM_DOF_FRINGE" Bool,
    CamDofFringeAngleRed "CAM_DOF_FRINGE_ANGLE_RED" Number,
    CamDofFringeAngleGreen "CAM_DOF_FRINGE_ANGLE_GREEN" Number,
    CamDofFringeAngleBlue "CAM_DOF_FRINGE_ANGLE_BLUE" Number,
    CamDofFringeRed "CAM_DOF_FRINGE_RED" Number,
    CamDofFringeGreen "CAM_DOF_FRINGE_GREEN" Number,
    CamDofFringeBlue "CAM_DOF_FRINGE_BLUE" Number,
    CamBloom "CAM_BLOOM" Bool,
    CamBloomThreshold "CAM_BLOOM_THRESHOLD" Number,
    CamBloomIntensity "CAM_BLOOM_INTENSITY" Number,
    CamBloomRadius "CAM_BLOOM_RADIUS" Number,
    CamBloomRatio "CAM_BLOOM_RATIO" Number,
    CamBloomBlend "CAM_BLOOM_BLEND" Color,
    CamLensDirt "CAM_LENS_DIRT" Number,
    CamLensDirtBloom "CAM_LENS_DIRT_BLOOM" Number,
    CamLensDirtGlow "CAM_LENS_DIRT_GLOW" Number,
    CamLensDirtRadius "CAM_LENS_DIRT_RADIUS" Number,
    CamLensDirtIntensity "CAM_LENS_DIRT_INTENSITY" Number,
    CamLensDirtPower "CAM_LENS_DIRT_POWER" Number,
    CamColorCorrection "CAM_COLOR_CORRECTION" Bool,
    CamContrast "CAM_CONTRAST" Number,
    CamBrightness "CAM_BRIGHTNESS" Number,
    CamSaturation "CAM_SATURATION" Number,
    CamVibrance "CAM_VIBRANCE" Number,
    CamColorBurn "CAM_COLOR_BURN" Color,
    CamGrain "CAM_GRAIN" Bool,
    CamGrainStrength "CAM_GRAIN_STRENGTH" Number,
    CamGrainSaturation "CAM_GRAIN_SATURATION" Number,
    CamGrainSize "CAM_GRAIN_SIZE" Number,
    CamVignette "CAM_VIGNETTE" Bool,
    CamVignetteRadius "CAM_VIGNETTE_RADIUS" Number,
    CamVignetteSoftness "CAM_VIGNETTE_SOFTNESS" Number,
    CamVignetteStrength "CAM_VIGNETTE_STRENGTH" Number,
    CamVignetteColor "CAM_VIGNETTE_COLOR" Color,
    CamCa "CAM_CA" Bool,
    CamCaBlurAmount "CAM_CA_BLUR_AMOUNT" Number,
    CamCaDistortChannels "CAM_CA_DISTORT_CHANNELS" Bool,
    CamCaRedOffset "CAM_CA_RED_OFFSET" Number,
    CamCaGreenOffset "CAM_CA_GREEN_OFFSET" Number,
    CamCaBlueOffset "CAM_CA_BLUE_OFFSET" Number,
    CamDistort "CAM_DISTORT" Bool,
    CamDistortRepeat "CAM_DISTORT_REPEAT" Bool,
    CamDistortZoomAmount "CAM_DISTORT_ZOOM_AMOUNT" Number,
    CamDistortAmount "CAM_DISTORT_AMOUNT" Number,
    CamSizeUseProject "CAM_SIZE_USE_PROJECT" Bool,
    CamSizeKeepAspectRatio "CAM_SIZE_KEEP_ASPECT_RATIO" Bool,
    CamWidth "CAM_WIDTH" Number,
    CamHeight "CAM_HEIGHT" Number,
    BgImageShow "BG_IMAGE_SHOW" Bool,
    BgImageRotation "BG_IMAGE_ROTATION" Number,
    BgSkyMoonPhase "BG_SKY_MOON_PHASE" Number,
    BgSkyTime "BG_SKY_TIME" Number,
    BgSkyRotation "BG_SKY_ROTATION" Number,
    BgSunlightStrength "BG_SUNLIGHT_STRENGTH" Number,
    BgSunlightAngle "BG_SUNLIGHT_ANGLE" Number,
    BgSkySunAngle "BG_SKY_SUN_ANGLE" Number,
    BgSkySunScale "BG_SKY_SUN_SCALE" Number,
    BgSkyMoonAngle "BG_SKY_MOON_ANGLE" Number,
    BgSkyMoonScale "BG_SKY_MOON_SCALE" Number,
    BgTwilight "BG_TWILIGHT" Number,
    BgSkyCloudsShow "BG_SKY_CLOUDS_SHOW" Bool,
    BgSkyCloudsSpeed "BG_SKY_CLOUDS_SPEED" Number,
    BgSkyCloudsHeight "BG_SKY_CLOUDS_HEIGHT" Number,
    BgSkyCloudsOffset "BG_SKY_CLOUDS_OFFSET" Number,
    BgGroundShow "BG_GROUND_SHOW" Bool,
    BgGroundSlot "BG_GROUND_SLOT" Number,
    BgBiome "BG_BIOME" String,
    BgSkyColor "BG_SKY_COLOR" Color,
    BgSkyCloudsColor "BG_SKY_CLOUDS_COLOR" Color,
    BgSunlightColor "BG_SUNLIGHT_COLOR" Color,
    BgAmbientColor "BG_AMBIENT_COLOR" Color,
    BgNightColor "BG_NIGHT_COLOR" Color,
    BgGrassColor "BG_GRASS_COLOR" Color,
    BgFoliageColor "BG_FOLIAGE_COLOR" Color,
    BgWaterColor "BG_WATER_COLOR" Color,
    BgLeavesOakColor "BG_LEAVES_OAK_COLOR" Color,
    BgLeavesSpruceColor "BG_LEAVES_SPRUCE_COLOR" Color,
    BgLeavesBirchColor "BG_LEAVES_BIRCH_COLOR" Color,
    BgLeavesJungleColor "BG_LEAVES_JUNGLE_COLOR" Color,
    BgLeavesAcaciaColor "BG_LEAVES_ACACIA_COLOR" Color,
    BgLeavesDarkOakColor "BG_LEAVES_DARK_OAK_COLOR" Color,
    BgLeavesMangroveColor "BG_LEAVES_MANGROVE_COLOR" Color,
    BgFogShow "BG_FOG_SHOW" Bool,
    BgFogSky "BG_FOG_SKY" Number,
    BgFogCustomColor "BG_FOG_CUSTOM_COLOR" Number,
    BgFogColor "BG_FOG_COLOR" Color,
    BgFogCustomObjectColor "BG_FOG_CUSTOM_OBJECT_COLOR" Number,
    BgFogObjectColor "BG_FOG_OBJECT_COLOR" Number,
    BgFogDistance "BG_FOG_DISTANCE" Number,
    BgFogSize "BG_FOG_SIZE" Number,
    BgFogHeight "BG_FOG_HEIGHT" Number,
    BgWind "BG_WIND" Bool,
    BgWindSpeed "BG_WIND_SPEED" Number,
    BgWindStrength "BG_WIND_STRENGTH" Number,
    BgWindDirection "BG_WIND_DIRECTION" Number,
    BgWindDirectionalSpeed "BG_WIND_DIRECTIONAL_SPEED" Number,
    BgWindDirectionalStrength "BG_WIND_DIRECTIONAL_STRENGTH" Number,
    BgTextureAniSpeed "BG_TEXTURE_ANI_SPEED" Number,
    TextureObj "TEXTURE_OBJ" Texture,
    TextureMaterialObj "TEXTURE_MATERIAL_OBJ" Texture,
    TextureNormalObj "TEXTURE_NORMAL_OBJ" Texture,
    SoundObj "SOUND_OBJ" Object,
    SoundVolume "SOUND_VOLUME" Number,
    SoundPitch "SOUND_PITCH" Number,
    SoundStart "SOUND_START" Number,
    SoundEnd "SOUND_END" Number,
    Text "TEXT" String,
    TextFont "TEXT_FONT" Object,
    TextHalign "TEXT_HALIGN" String,
    TextValign "TEXT_VALIGN" String,
    TextAa "TEXT_AA" Bool,
    TextOutline "TEXT_OUTLINE" Bool,
    TextOutlineColor "TEXT_OUTLINE_COLOR" Color,
    CustomItemSlot "CUSTOM_ITEM_SLOT" Number,
    ItemSlot "ITEM_SLOT" Number,
    ItemName "ITEM_NAME" String,
    PathObj "PATH_OBJ" Object,
    PathOffset "PATH_OFFSET" Number,
    PathPointAngle "PATH_POINT_ANGLE" Number,
    PathPointScale "PATH_POINT_SCALE" Number,
    IkTarget "IK_TARGET" Object,
    IkBlend "IK_BLEND" Number,
    IkTargetAngle "IK_TARGET_ANGLE" Object,
    IkAngleOffset "IK_ANGLE_OFFSET" Number,
    Visible "VISIBLE" Bool,
    Transition "TRANSITION" String,
    EaseInX "EASE_IN_X" Number,
    EaseInY "EASE_IN_Y" Number,
    EaseOutX "EASE_OUT_X" Number,
    EaseOutY "EASE_OUT_Y" Number,
}

/// Number of animatable values (`e_value.amount`).
pub const VALUE_COUNT: usize = ValueId::ALL.len();

impl ValueId {
    pub const fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    /// Camera values that a camera timeline can override per keyframe
    /// (`camera_values_list`: `CAM_FOV` through `CAM_HEIGHT`).
    pub fn is_camera(self) -> bool {
        (ValueId::CamFov..=ValueId::CamHeight).contains(&self)
    }

    /// Values mirroring the project background (`BG_*`).
    pub fn is_background(self) -> bool {
        (ValueId::BgImageShow..=ValueId::BgTextureAniSpeed).contains(&self)
    }

    /// The value a new project uses, as in `tl_value_default`.
    ///
    /// Returns `None` for values whose default is taken from the project's
    /// background settings (`BG_*`) or is random (`SEED`); the project
    /// supplies those.
    pub fn static_default(self) -> Option<Value> {
        use ValueId::*;
        if self.is_background() || self == Seed {
            return None;
        }

        Some(match self {
            ScaX | ScaY | ScaZ | Alpha | Roughness => Value::Number(1.0),
            SubsurfaceRadiusRed | SubsurfaceRadiusGreen | SubsurfaceRadiusBlue | WindInfluence => {
                Value::Number(1.0)
            }
            GlowColor | RgbMul | HsbMul | SubsurfaceColor | TextOutlineColor | LightColor
            | CamBloomBlend | CamColorBurn => Value::Color(Color::WHITE),
            Spawn | Visible | CamSizeUseProject | CamSizeKeepAspectRatio => Value::Bool(true),
            Force | LightStrength | LightSpecularStrength => Value::Number(1.0),
            LightSize => Value::Number(2.0),
            LightRange => Value::Number(250.0),
            LightFadeSize | LightSpotSharpness => Value::Number(0.5),
            LightSpotRadius => Value::Number(50.0),
            CamFov => Value::Number(45.0),
            CamExposure => Value::Number(1.0),
            CamGamma => Value::Number(2.2),
            CamRotateDistance => Value::Number(100.0),
            CamShakeMode | CamShakeStrengthX | CamShakeStrengthY | CamShakeStrengthZ
            | CamShakeSpeedX | CamShakeSpeedY | CamShakeSpeedZ => Value::Number(1.0),
            CamDofRange => Value::Number(200.0),
            CamDofFadeSize => Value::Number(100.0),
            CamDofBlurSize => Value::Number(0.01),
            CamDofFringeRed | CamDofFringeGreen | CamDofFringeBlue => Value::Number(1.0),
            CamDofFringeAngleRed => Value::Number(90.0),
            CamDofFringeAngleGreen => Value::Number(-135.0),
            CamDofFringeAngleBlue => Value::Number(-45.0),
            CamBloomThreshold => Value::Number(0.85),
            CamBloomIntensity => Value::Number(0.4),
            CamBloomRadius => Value::Number(1.0),
            CamLensDirtBloom | CamLensDirtGlow => Value::Number(1.0),
            CamLensDirtRadius => Value::Number(0.5),
            CamLensDirtIntensity => Value::Number(0.8),
            CamLensDirtPower => Value::Number(1.5),
            CamSaturation => Value::Number(1.0),
            CamGrainStrength | CamGrainSaturation => Value::Number(0.10),
            CamGrainSize | CamVignetteRadius | CamVignetteStrength => Value::Number(1.0),
            CamVignetteSoftness => Value::Number(0.5),
            CamVignetteColor => Value::Color(Color::BLACK),
            CamCaBlurAmount => Value::Number(0.05),
            CamCaRedOffset => Value::Number(0.12),
            CamCaGreenOffset => Value::Number(0.08),
            CamCaBlueOffset => Value::Number(0.04),
            CamDistortZoomAmount => Value::Number(1.0),
            CamDistortAmount => Value::Number(0.05),
            CamWidth => Value::Number(1280.0),
            CamHeight => Value::Number(720.0),
            IkBlend | SoundVolume | SoundPitch | PathPointScale => Value::Number(1.0),
            TextHalign | TextValign => Value::Str("center".to_owned()),
            Transition => Value::Str("linear".to_owned()),
            EaseInX | EaseOutY => Value::Number(1.0),
            other => match other.kind() {
                ValueKind::Number => Value::Number(0.0),
                ValueKind::Bool => Value::Bool(false),
                ValueKind::Color => Value::Color(Color::BLACK),
                ValueKind::String => Value::Str(String::new()),
                ValueKind::Texture | ValueKind::Object => Value::Ref(ObjRef::Null),
            },
        })
    }
}

/// Content of one animatable value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Number(f64),
    Bool(bool),
    Color(Color),
    Str(String),
    Ref(ObjRef),
}

impl Value {
    /// Numeric view: booleans count as 0/1, everything else as 0.
    pub fn as_f64(&self) -> f64 {
        match self {
            Value::Number(n) => *n,
            Value::Bool(b) => *b as u8 as f64,
            _ => 0.0,
        }
    }

    /// Truthiness as GML sees it (numbers are true above 0.5).
    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Number(n) => *n > 0.5,
            _ => false,
        }
    }

    pub fn as_color(&self) -> Option<Color> {
        match self {
            Value::Color(c) => Some(*c),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_ref(&self) -> Option<&ObjRef> {
        match self {
            Value::Ref(r) => Some(r),
            _ => None,
        }
    }

    /// Whether the variant is the one `kind` stores.
    pub fn matches_kind(&self, kind: ValueKind) -> bool {
        matches!(
            (self, kind),
            (Value::Number(_), ValueKind::Number)
                | (Value::Bool(_), ValueKind::Bool)
                | (Value::Color(_), ValueKind::Color)
                | (Value::Str(_), ValueKind::String)
                | (Value::Ref(_), ValueKind::Texture | ValueKind::Object)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_matches_the_original_enum() {
        assert_eq!(VALUE_COUNT, 207);
        assert_eq!(ValueId::PosX.index(), 0);
        assert_eq!(ValueId::BendAngleLegacy.name(), "BEND_ANGLE");
        assert_eq!(ValueId::EaseOutY.index(), VALUE_COUNT - 1);
        for (i, &id) in ValueId::ALL.iter().enumerate() {
            assert_eq!(id.index(), i);
            assert_eq!(ValueId::from_name(id.name()), Some(id));
            assert_eq!(ValueId::from_index(i), Some(id));
        }
    }

    #[test]
    fn kind_counts_match_the_original_predicates() {
        let count = |k| ValueId::ALL.iter().filter(|v| v.kind() == k).count();
        assert_eq!(count(ValueKind::Bool), 27); // tl_value_is_bool
        assert_eq!(count(ValueKind::Color), 30); // tl_value_is_color
        assert_eq!(count(ValueKind::String), 6); // tl_value_is_string + ITEM_NAME
        assert_eq!(count(ValueKind::Texture), 3);
        assert_eq!(count(ValueKind::Object), 6);
    }

    #[test]
    fn static_defaults_have_the_right_kind() {
        for &id in ValueId::ALL {
            match id.static_default() {
                Some(v) => assert!(v.matches_kind(id.kind()), "{id:?} default {v:?}"),
                None => assert!(id.is_background() || id == ValueId::Seed, "{id:?}"),
            }
        }
        assert_eq!(ValueId::CamFov.static_default(), Some(Value::Number(45.0)));
        assert_eq!(ValueId::Transition.static_default(), Some(Value::Str("linear".into())));
        assert_eq!(ValueId::Visible.static_default(), Some(Value::Bool(true)));
        assert_eq!(ValueId::CamVignetteColor.static_default(), Some(Value::Color(Color::BLACK)));
    }

    #[test]
    fn ranges() {
        assert!(ValueId::CamFov.is_camera() && ValueId::CamHeight.is_camera());
        assert!(!ValueId::BgImageShow.is_camera());
        assert!(ValueId::BgTextureAniSpeed.is_background());
        assert!(!ValueId::TextureObj.is_background());
    }
}
