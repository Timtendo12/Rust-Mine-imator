//! Save ids: the strings that objects in a project use to refer to each other.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Identifier of a template, timeline, resource, particle type or marker.
///
/// Projects written by 1.1.0+ use 16 characters from `[0-9A-Z]`
/// (`save_id_create`); two ids are reserved: [`SaveId::ROOT`] for the
/// timeline tree root and [`SaveId::DEFAULT`] for the built-in Minecraft
/// resource pack.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SaveId(String);

impl SaveId {
    pub const ROOT: &'static str = "root";
    pub const DEFAULT: &'static str = "default";
    pub const LENGTH: usize = 16;

    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn root() -> Self {
        Self(Self::ROOT.to_owned())
    }

    /// The built-in Minecraft assets resource (`mc_res` in the original).
    pub fn default_resource() -> Self {
        Self(Self::DEFAULT.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0 == Self::ROOT
    }

    pub fn is_default_resource(&self) -> bool {
        self.0 == Self::DEFAULT
    }

    /// Generates ids the way `save_id_create` does: 16 characters, each one
    /// first chosen to be a digit or a letter with equal chance. `next` must
    /// return uniformly distributed values.
    pub fn generate(mut next: impl FnMut() -> u32) -> Self {
        let mut id = String::with_capacity(Self::LENGTH);
        for _ in 0..Self::LENGTH {
            let c = if next() % 2 == 0 {
                b'0' + (next() % 10) as u8
            } else {
                b'A' + (next() % 26) as u8
            };
            id.push(c as char);
        }
        Self(id)
    }
}

/// Source of new save ids.
///
/// The original reseeds GameMaker's generator with a counter for every id so
/// that undo can recreate objects with identical ids; here the generator
/// state itself can be saved and restored for the same purpose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdGenerator {
    state: u64,
}

impl IdGenerator {
    pub fn new(seed: u64) -> Self {
        // A zero state would make xorshift produce zeroes forever.
        Self { state: seed | 1 }
    }

    /// Seeds the generator from the clock.
    pub fn from_time() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9E37_79B9_7F4A_7C15, |d| d.as_nanos() as u64);
        Self::new(nanos ^ 0x9E37_79B9_7F4A_7C15)
    }

    /// Current state, to be restored with [`IdGenerator::new`].
    pub fn state(&self) -> u64 {
        self.state
    }

    fn next_u32(&mut self) -> u32 {
        // xorshift64*
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        (self.state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }

    pub fn next_id(&mut self) -> SaveId {
        SaveId::generate(|| self.next_u32())
    }
}

impl fmt::Display for SaveId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for SaveId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// A reference held by an object-valued field or animatable value.
///
/// Files write `"null"` for [`ObjRef::Null`] and, for texture values only,
/// `"none"` for [`ObjRef::None`] (the original stores the number `0` there to
/// mean "explicitly no texture" as opposed to "inherit").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ObjRef {
    #[default]
    Null,
    None,
    Id(SaveId),
}

impl ObjRef {
    pub fn id(id: impl Into<String>) -> Self {
        ObjRef::Id(SaveId::new(id))
    }

    pub fn default_resource() -> Self {
        ObjRef::Id(SaveId::default_resource())
    }

    pub fn as_id(&self) -> Option<&SaveId> {
        match self {
            ObjRef::Id(id) => Some(id),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, ObjRef::Null)
    }
}

impl From<Option<SaveId>> for ObjRef {
    fn from(id: Option<SaveId>) -> Self {
        id.map_or(ObjRef::Null, ObjRef::Id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_ids_use_the_expected_alphabet() {
        let mut ids = IdGenerator::new(12345);
        let a = ids.next_id();
        let b = ids.next_id();
        assert_ne!(a, b);
        for id in [&a, &b] {
            assert_eq!(id.as_str().len(), SaveId::LENGTH);
            assert!(id.as_str().chars().all(|c| c.is_ascii_digit() || c.is_ascii_uppercase()));
        }
        // Both digits and letters occur.
        let many: String = (0..20).map(|_| ids.next_id().to_string()).collect();
        assert!(many.chars().any(|c| c.is_ascii_digit()) && many.chars().any(|c| c.is_ascii_uppercase()));
    }

    #[test]
    fn generator_state_can_be_restored() {
        let mut ids = IdGenerator::new(99);
        ids.next_id();
        let mut copy = IdGenerator::new(ids.state());
        assert_eq!(ids.next_id(), copy.next_id());
    }
}
