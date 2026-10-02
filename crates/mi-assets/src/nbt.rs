//! Named Binary Tag reading (`nbt_read_tag_compound`): the format of
//! Minecraft schematics, structures and worlds. Gzip compressed input is
//! decompressed first.

use std::collections::HashMap;
use std::io::Read;

#[derive(Debug, thiserror::Error)]
pub enum NbtError {
    #[error("the data ends early")]
    Truncated,
    #[error("unknown tag type {0}")]
    UnknownTag(u8),
    #[error("the root is not a compound")]
    NotCompound,
    #[error("could not decompress: {0}")]
    Gzip(#[from] std::io::Error),
    #[error("nesting is too deep")]
    TooDeep,
}

/// A tag value.
#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<u8>),
    String(String),
    List(Vec<Tag>),
    Compound(Compound),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

/// A compound tag: named tags.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Compound(pub HashMap<String, Tag>);

impl Compound {
    pub fn get(&self, key: &str) -> Option<&Tag> {
        self.0.get(key)
    }

    pub fn compound(&self, key: &str) -> Option<&Compound> {
        self.get(key)?.as_compound()
    }

    pub fn list(&self, key: &str) -> Option<&[Tag]> {
        match self.get(key)? {
            Tag::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn string(&self, key: &str) -> Option<&str> {
        self.get(key)?.as_str()
    }

    /// Any number tag as an integer.
    pub fn int(&self, key: &str) -> Option<i64> {
        self.get(key)?.as_int()
    }

    pub fn bytes(&self, key: &str) -> Option<&[u8]> {
        match self.get(key)? {
            Tag::ByteArray(b) => Some(b),
            _ => None,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Tag)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }
}

impl Tag {
    pub fn as_int(&self) -> Option<i64> {
        Some(match self {
            Tag::Byte(v) => *v as i64,
            Tag::Short(v) => *v as i64,
            Tag::Int(v) => *v as i64,
            Tag::Long(v) => *v,
            Tag::Float(v) => *v as i64,
            Tag::Double(v) => *v as i64,
            _ => return None,
        })
    }

    pub fn as_compound(&self) -> Option<&Compound> {
        match self {
            Tag::Compound(c) => Some(c),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Tag::String(s) => Some(s),
            _ => None,
        }
    }

    /// The numbers of a list or number array.
    pub fn as_ints(&self) -> Option<Vec<i64>> {
        match self {
            Tag::List(l) => l.iter().map(Tag::as_int).collect(),
            Tag::IntArray(a) => Some(a.iter().map(|&v| v as i64).collect()),
            Tag::LongArray(a) => Some(a.clone()),
            Tag::ByteArray(a) => Some(a.iter().map(|&v| v as i8 as i64).collect()),
            _ => None,
        }
    }
}

const MAX_DEPTH: usize = 512;

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], NbtError> {
        let end = self.pos.checked_add(n).ok_or(NbtError::Truncated)?;
        let slice = self.data.get(self.pos..end).ok_or(NbtError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, NbtError> {
        Ok(self.take(1)?[0])
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], NbtError> {
        Ok(self.take(N)?.try_into().expect("slice has the requested length"))
    }

    fn len(&mut self) -> Result<usize, NbtError> {
        // Negative lengths mean empty.
        Ok(i32::from_be_bytes(self.array()?).max(0) as usize)
    }

    fn string(&mut self) -> Result<String, NbtError> {
        let n = u16::from_be_bytes(self.array()?) as usize;
        // Java's modified UTF-8; it only differs from UTF-8 for NUL and
        // characters outside the BMP, which are replaced.
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }

    fn payload(&mut self, kind: u8, depth: usize) -> Result<Tag, NbtError> {
        if depth > MAX_DEPTH {
            return Err(NbtError::TooDeep);
        }
        Ok(match kind {
            1 => Tag::Byte(self.u8()? as i8),
            2 => Tag::Short(i16::from_be_bytes(self.array()?)),
            3 => Tag::Int(i32::from_be_bytes(self.array()?)),
            4 => Tag::Long(i64::from_be_bytes(self.array()?)),
            5 => Tag::Float(f32::from_be_bytes(self.array()?)),
            6 => Tag::Double(f64::from_be_bytes(self.array()?)),
            7 => {
                let n = self.len()?;
                Tag::ByteArray(self.take(n)?.to_vec())
            }
            8 => Tag::String(self.string()?),
            9 => {
                let element = self.u8()?;
                let n = self.len()?;
                if element == 0 {
                    Tag::List(Vec::new())
                } else {
                    // Every element takes at least a byte, which bounds
                    // lengths that claim more than the data holds.
                    if n > self.data.len() - self.pos {
                        return Err(NbtError::Truncated);
                    }
                    let mut list = Vec::with_capacity(n);
                    for _ in 0..n {
                        list.push(self.payload(element, depth + 1)?);
                    }
                    Tag::List(list)
                }
            }
            10 => {
                let mut map = HashMap::new();
                loop {
                    let kind = self.u8()?;
                    if kind == 0 {
                        break;
                    }
                    let name = self.string()?;
                    let value = self.payload(kind, depth + 1)?;
                    map.insert(name, value);
                }
                Tag::Compound(Compound(map))
            }
            11 => {
                let n = self.len()?;
                let bytes = self.take(n.checked_mul(4).ok_or(NbtError::Truncated)?)?;
                Tag::IntArray(bytes.chunks_exact(4).map(|c| i32::from_be_bytes([c[0], c[1], c[2], c[3]])).collect())
            }
            12 => {
                let n = self.len()?;
                let bytes = self.take(n.checked_mul(8).ok_or(NbtError::Truncated)?)?;
                Tag::LongArray(
                    bytes.chunks_exact(8).map(|c| i64::from_be_bytes(c.try_into().expect("chunk of 8"))).collect(),
                )
            }
            other => return Err(NbtError::UnknownTag(other)),
        })
    }
}

/// Decompresses gzip data; other data is returned as it is.
pub fn gunzip(bytes: &[u8]) -> Result<std::borrow::Cow<'_, [u8]>, NbtError> {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::MultiGzDecoder::new(bytes).read_to_end(&mut out)?;
        Ok(out.into())
    } else {
        Ok(bytes.into())
    }
}

/// Reads the root compound and its name.
pub fn read(bytes: &[u8]) -> Result<(String, Compound), NbtError> {
    let data = gunzip(bytes)?;
    let mut reader = Reader { data: &data, pos: 0 };
    if reader.u8()? != 10 {
        return Err(NbtError::NotCompound);
    }
    let name = reader.string()?;
    match reader.payload(10, 0)? {
        Tag::Compound(c) => Ok((name, c)),
        _ => Err(NbtError::NotCompound),
    }
}

/// A small writer for building test data.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct Writer(pub Vec<u8>);

#[cfg(test)]
impl Writer {
    pub fn name(&mut self, kind: u8, name: &str) -> &mut Self {
        self.0.push(kind);
        self.0.extend((name.len() as u16).to_be_bytes());
        self.0.extend(name.as_bytes());
        self
    }
    pub fn short(&mut self, name: &str, v: i16) -> &mut Self {
        self.name(2, name);
        self.0.extend(v.to_be_bytes());
        self
    }
    pub fn int(&mut self, name: &str, v: i32) -> &mut Self {
        self.name(3, name);
        self.0.extend(v.to_be_bytes());
        self
    }
    pub fn string(&mut self, name: &str, v: &str) -> &mut Self {
        self.name(8, name);
        self.raw_string(v)
    }
    pub fn raw_string(&mut self, v: &str) -> &mut Self {
        self.0.extend((v.len() as u16).to_be_bytes());
        self.0.extend(v.as_bytes());
        self
    }
    pub fn bytes(&mut self, name: &str, v: &[u8]) -> &mut Self {
        self.name(7, name);
        self.0.extend((v.len() as i32).to_be_bytes());
        self.0.extend(v);
        self
    }
    pub fn begin(&mut self, name: &str) -> &mut Self {
        self.name(10, name)
    }
    pub fn end(&mut self) -> &mut Self {
        self.0.push(0);
        self
    }
    /// A list header; the elements follow as bare payloads.
    pub fn list(&mut self, name: &str, element: u8, n: i32) -> &mut Self {
        self.name(9, name);
        self.0.push(element);
        self.0.extend(n.to_be_bytes());
        self
    }
    pub fn raw_int(&mut self, v: i32) -> &mut Self {
        self.0.extend(v.to_be_bytes());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_tags_and_gzip() {
        let mut w = Writer::default();
        w.begin("Schematic").short("Width", 3).string("Materials", "Alpha").bytes("Blocks", &[1, 2, 3]);
        w.list("Pos", 3, 2).raw_int(4).raw_int(-5);
        w.begin("Inner").int("x", 7).end();
        w.list("Empty", 0, 0);
        w.end();
        let (name, root) = read(&w.0).unwrap();
        assert_eq!(name, "Schematic");
        assert_eq!(root.int("Width"), Some(3));
        assert_eq!(root.string("Materials"), Some("Alpha"));
        assert_eq!(root.bytes("Blocks"), Some(&[1u8, 2, 3][..]));
        assert_eq!(root.get("Pos").unwrap().as_ints(), Some(vec![4, -5]));
        assert_eq!(root.compound("Inner").unwrap().int("x"), Some(7));
        assert_eq!(root.list("Empty").map(|l| l.len()), Some(0));

        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut gz, &w.0).unwrap();
        let (_, again) = read(&gz.finish().unwrap()).unwrap();
        assert_eq!(again, root);
    }

    #[test]
    fn bad_data_is_an_error() {
        assert!(matches!(read(&[]), Err(NbtError::Truncated)));
        assert!(matches!(read(&[8, 0, 0]), Err(NbtError::NotCompound)));
        let mut w = Writer::default();
        w.begin("").bytes("B", &[1, 2, 3]);
        let mut data = w.0.clone();
        data.truncate(data.len() - 2);
        assert!(matches!(read(&data), Err(NbtError::Truncated)));
        // A list claiming to be huge is refused before allocating.
        let mut w = Writer::default();
        w.begin("").list("L", 3, i32::MAX);
        assert!(matches!(read(&w.0), Err(NbtError::Truncated)));
    }
}
