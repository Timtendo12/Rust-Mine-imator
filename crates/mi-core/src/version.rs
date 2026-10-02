//! File format version numbers (`e_project`, `e_settings`, ... in `enums.gml`).

/// Version of the original program whose behaviour this rewrite tracks.
pub const MINEIMATOR_VERSION: &str = "2.0.2";
/// Minecraft version of the bundled asset pack.
pub const MINECRAFT_VERSION: &str = "1.20.2";

/// `.miproject` / `.miobject` / `.miframes` / `.miparticles` / `.mirender`
/// format numbers.
pub mod project {
    pub const FORMAT_01: i32 = 1;
    pub const FORMAT_02: i32 = 2;
    pub const FORMAT_05: i32 = 3;
    pub const FORMAT_06: i32 = 4;
    pub const FORMAT_07_DEMO: i32 = 5;
    pub const FORMAT_100_DEMO_2: i32 = 6;
    pub const FORMAT_100_DEMO_3: i32 = 7;
    pub const FORMAT_100_DEMO_4: i32 = 8;
    pub const FORMAT_100_DEBUG: i32 = 9;
    pub const FORMAT_100: i32 = 10;
    pub const FORMAT_105: i32 = 11;
    pub const FORMAT_105_2: i32 = 12;
    pub const FORMAT_106: i32 = 13;
    pub const FORMAT_106_2: i32 = 14;
    pub const FORMAT_CB_100: i32 = 20;
    pub const FORMAT_CB_102_PRE: i32 = 21;
    pub const FORMAT_CB_102: i32 = 22;
    pub const FORMAT_CB_103: i32 = 23;
    /// First JSON based format.
    pub const FORMAT_110_PRE_1: i32 = 24;
    pub const FORMAT_110_PRE_3: i32 = 25;
    pub const FORMAT_110: i32 = 26;
    pub const FORMAT_113: i32 = 27;
    pub const FORMAT_120_PRE_1: i32 = 28;
    pub const FORMAT_120_PRE_3: i32 = 29;
    pub const FORMAT_122: i32 = 30;
    pub const FORMAT_123_PRE_2: i32 = 31;
    pub const FORMAT_125: i32 = 32;
    pub const FORMAT_200_PRE_1: i32 = 33;
    pub const FORMAT_200_PRE_5: i32 = 34;

    /// Format written by this program.
    pub const CURRENT: i32 = FORMAT_200_PRE_5;
}

/// `settings.midata` format numbers.
pub mod settings {
    pub const FORMAT_100_DEMO_4: i32 = 0;
    pub const FORMAT_100_DEMO_5: i32 = 1;
    pub const FORMAT_100: i32 = 2;
    pub const FORMAT_103: i32 = 3;
    pub const FORMAT_106: i32 = 4;
    pub const FORMAT_106_2: i32 = 5;
    pub const FORMAT_106_3: i32 = 6;
    pub const FORMAT_CB_100: i32 = 20;
    pub const FORMAT_CB_102: i32 = 22;
    pub const FORMAT_CE_110: i32 = 23;
    pub const FORMAT_110_PRE_1: i32 = 24;
    pub const FORMAT_110: i32 = 25;
    pub const FORMAT_113: i32 = 26;
    pub const FORMAT_114: i32 = 27;
    pub const FORMAT_120: i32 = 28;
    pub const FORMAT_200: i32 = 29;

    pub const CURRENT: i32 = FORMAT_200;
}

/// Minecraft version manifest (`<version>.midata`) format numbers.
pub mod minecraft_assets {
    pub const FORMAT_110_PRE_1: i32 = 1;
    pub const FORMAT_110_PRE_2: i32 = 2;
    pub const FORMAT_113: i32 = 3;
    pub const FORMAT_120: i32 = 4;
    pub const FORMAT_123: i32 = 5;
    pub const FORMAT_128: i32 = 6;
    pub const FORMAT_129: i32 = 7;
    pub const FORMAT_200: i32 = 8;
    pub const FORMAT_201: i32 = 9;

    pub const CURRENT: i32 = FORMAT_201;
}

/// Resource pack layout generations (`e_minecraft_pack`).
pub mod minecraft_pack {
    pub const FORMAT_161: i32 = 1;
    pub const FORMAT_19: i32 = 2;
    pub const FORMAT_111: i32 = 3;
    pub const FORMAT_113: i32 = 4;
    pub const FORMAT_115: i32 = 5;

    pub const LATEST: i32 = FORMAT_115;
}
