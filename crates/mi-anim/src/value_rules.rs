//! Per-value rules: how a value blends between two keyframes
//! (`tl_value_interpolate`) and which range it is limited to
//! (`tl_value_clamp`).

use mi_core::{Value, ValueId, ValueKind};

/// Largest magnitude any value may take (`no_limit`).
pub const NO_LIMIT: f64 = 100_000_000.0;

/// How a value moves from one keyframe to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    /// Linear blend of the number.
    Lerp,
    /// Linear blend, rounded down to a whole number.
    LerpFloor,
    /// Per-channel colour blend with the progress limited to 0..1.
    Color,
    /// Keeps the value of the earlier keyframe.
    Hold,
}

/// The blend rule of a value.
pub fn blend_rule(id: ValueId) -> Blend {
    use ValueId::*;
    match id {
        CamBladeAmount | CamWidth | CamHeight | ItemSlot => Blend::LerpFloor,

        // Numbers that are switches, choices or belong to audio clips.
        Seed | CamTonemapper | CamShakeMode | CamLensDirt | CamLensDirtBloom | CamLensDirtGlow
        | BgSkyMoonPhase | BgTwilight | BgGroundSlot | SoundVolume | SoundPitch | SoundStart
        | SoundEnd | CustomItemSlot | EaseInX | EaseInY | EaseOutX | EaseOutY => Blend::Hold,

        _ => match id.kind() {
            ValueKind::Number => Blend::Lerp,
            ValueKind::Color => Blend::Color,
            ValueKind::Bool | ValueKind::String | ValueKind::Texture | ValueKind::Object => Blend::Hold,
        },
    }
}

/// Value at eased progress `progress` between `from` and `to`.
/// Progress may leave 0..1 for transitions that overshoot.
pub fn interpolate(id: ValueId, progress: f64, from: &Value, to: &Value) -> Value {
    match (blend_rule(id), from, to) {
        (Blend::Lerp, Value::Number(a), Value::Number(b)) => Value::Number(a + progress * (b - a)),
        (Blend::LerpFloor, Value::Number(a), Value::Number(b)) => Value::Number((a + progress * (b - a)).floor()),
        (Blend::Color, Value::Color(a), Value::Color(b)) => Value::Color(a.merge(*b, progress.clamp(0.0, 1.0))),
        _ => from.clone(),
    }
}

/// Limits that depend on project settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClampContext {
    /// `project_render_distance`
    pub render_distance: f64,
    /// The "unlimited values" program setting.
    pub unlimited_values: bool,
}

impl Default for ClampContext {
    fn default() -> Self {
        Self { render_distance: 30000.0, unlimited_values: false }
    }
}

/// Brings a value into its allowed range. Values that are not numbers are
/// returned unchanged.
pub fn clamp(id: ValueId, value: Value, ctx: ClampContext) -> Value {
    use ValueId::*;
    let Value::Number(v) = value else {
        return value;
    };
    let is_scale = matches!(id, ScaX | ScaY | ScaZ);

    if ctx.unlimited_values {
        return Value::Number(if is_scale && v == 0.0 { 0.0001 } else { v.clamp(-NO_LIMIT, NO_LIMIT) });
    }

    Value::Number(match id {
        PosX | PosY | PosZ => v.clamp(-ctx.render_distance, ctx.render_distance),
        ScaX | ScaY | ScaZ => v.max(0.0001),
        Alpha | MixPercent | Metallic | Roughness | SubsurfaceRadiusRed | SubsurfaceRadiusGreen
        | SubsurfaceRadiusBlue | WindInfluence | CamBloomThreshold | CamVignetteRadius
        | CamVignetteSoftness | CamVignetteStrength => v.clamp(0.0, 1.0),
        BendAngleX | BendAngleY | BendAngleZ => v.clamp(-180.0, 180.0),
        CamFov => v.clamp(1.0, 170.0),
        CamBladeAmount => v.clamp(0.0, 32.0),
        CamRotateDistance => v.max(1.0),
        CamRotateAngleZ => v.clamp(-89.9, 89.9),
        CamExposure | CamGamma | BgSunlightStrength | LightStrength | Emissive | Subsurface
        | CamShakeStrengthX | CamShakeStrengthY | CamShakeStrengthZ | CamShakeSpeedX | CamShakeSpeedY
        | CamShakeSpeedZ | CamCaRedOffset | CamCaGreenOffset | CamCaBlueOffset => v.clamp(0.0, NO_LIMIT),
        CamWidth | CamHeight => v.max(1.0),
        BgSkyMoonPhase => v.clamp(0.0, 7.0),
        BgFogDistance | BgFogSize => v.clamp(10.0, ctx.render_distance.max(10.0)),
        BgFogHeight => v.clamp(10.0, 2000.0),
        BgWindSpeed => v.clamp(0.0, 1.0),
        BgWindStrength => v.clamp(0.0, 8.0),
        BgTextureAniSpeed => v.max(0.0),
        SoundVolume => v.clamp(0.0, 1.0),
        SoundPitch => v.clamp(0.5, 2.0),
        SoundStart => v.max(0.0),
        _ => v.clamp(-NO_LIMIT, NO_LIMIT),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mi_core::{Color, ObjRef};

    #[test]
    fn every_value_blends_according_to_its_kind() {
        for &id in ValueId::ALL {
            match (blend_rule(id), id.kind()) {
                (Blend::Lerp | Blend::LerpFloor, ValueKind::Number) => {}
                (Blend::Color, ValueKind::Color) => {}
                (Blend::Hold, _) => {}
                other => panic!("{id:?}: {other:?}"),
            }
        }
        // Counts from the switch statement in tl_value_interpolate.
        let count = |b| ValueId::ALL.iter().filter(|&&id| blend_rule(id) == b).count();
        assert_eq!(count(Blend::LerpFloor), 4);
        assert_eq!(count(Blend::Color), 31);
    }

    #[test]
    fn numbers_interpolate() {
        let v = interpolate(ValueId::PosX, 0.25, &Value::Number(10.0), &Value::Number(20.0));
        assert_eq!(v, Value::Number(12.5));
        // Overshooting transitions extrapolate.
        let v = interpolate(ValueId::PosX, 1.5, &Value::Number(10.0), &Value::Number(20.0));
        assert_eq!(v, Value::Number(25.0));
        let v = interpolate(ValueId::CamWidth, 0.5, &Value::Number(100.0), &Value::Number(201.0));
        assert_eq!(v, Value::Number(150.0));
    }

    #[test]
    fn colors_interpolate_within_range() {
        let a = Value::Color(Color::BLACK);
        let b = Value::Color(Color::rgb(200, 100, 0));
        assert_eq!(interpolate(ValueId::RgbAdd, 0.5, &a, &b), Value::Color(Color::rgb(100, 50, 0)));
        assert_eq!(interpolate(ValueId::RgbAdd, 1.7, &a, &b), b);
        assert_eq!(interpolate(ValueId::BgFogObjectColor, -1.0, &a, &b), a);
    }

    #[test]
    fn switches_and_references_hold() {
        let a = Value::Bool(true);
        assert_eq!(interpolate(ValueId::Visible, 0.99, &a, &Value::Bool(false)), a);
        let tex = Value::Ref(ObjRef::id("A"));
        assert_eq!(interpolate(ValueId::TextureObj, 0.5, &tex, &Value::Ref(ObjRef::Null)), tex);
        let text = Value::Str("one".into());
        assert_eq!(interpolate(ValueId::Text, 0.5, &text, &Value::Str("two".into())), text);
        let seed = Value::Number(5.0);
        assert_eq!(interpolate(ValueId::Seed, 0.5, &seed, &Value::Number(9.0)), seed);
        assert_eq!(interpolate(ValueId::SoundVolume, 0.5, &Value::Number(1.0), &Value::Number(0.0)), Value::Number(1.0));
    }

    #[test]
    fn clamping() {
        let ctx = ClampContext::default();
        let n = |id, v| clamp(id, Value::Number(v), ctx).as_f64();
        assert_eq!(n(ValueId::PosX, 1e9), 30000.0);
        assert_eq!(n(ValueId::ScaX, 0.0), 0.0001);
        assert_eq!(n(ValueId::Alpha, 1.5), 1.0);
        assert_eq!(n(ValueId::BendAngleX, -270.0), -180.0);
        assert_eq!(n(ValueId::CamFov, 0.0), 1.0);
        assert_eq!(n(ValueId::SoundPitch, 0.1), 0.5);
        assert_eq!(n(ValueId::RotX, 720.0), 720.0);
        assert_eq!(n(ValueId::BgFogDistance, 5.0), 10.0);

        let unlimited = ClampContext { unlimited_values: true, ..ctx };
        assert_eq!(clamp(ValueId::Alpha, Value::Number(3.0), unlimited), Value::Number(3.0));
        assert_eq!(clamp(ValueId::ScaY, Value::Number(0.0), unlimited), Value::Number(0.0001));
        assert_eq!(clamp(ValueId::ScaY, Value::Number(-2.0), unlimited), Value::Number(-2.0));

        let text = Value::Str("x".into());
        assert_eq!(clamp(ValueId::Text, text.clone(), ctx), text);
    }
}
