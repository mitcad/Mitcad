// SPDX-License-Identifier: MIT
//! Reading a binary USD layer and a USDZ package back, checking what the
//! formats require of them (tests use it on what [`crate::write`] wrote):
//! the package's files stored uncompressed and 64-byte aligned, its first
//! file a layer; the layer's header, sections, tables and the values of
//! the types the writer uses (and the inlined and compressed forms OpenUSD
//! writes of them).

use crate::Error;
use crate::compress::{decompress_ints, fast_decompress};
use crate::sdf::{ListOp, Path, Spec, SpecType, Specifier, Value, Variability};
use crate::usdc::{
    ARRAY_BIT, BOOTSTRAP_SIZE, COMPRESSED_BIT, INLINED_BIT, MIN_COMPRESSED_ARRAY, PAYLOAD_MASK,
    list_op, types,
};

/// A binary layer read back.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// Major, minor, patch.
    pub version: [u8; 3],
    pub specs: Vec<Spec>,
}

impl Layer {
    pub fn spec(&self, path: &str) -> Option<&Spec> {
        let path = Path::parse(path)?;
        self.specs.iter().find(|s| s.path == path)
    }

    /// A field of the spec at `path`.
    pub fn field(&self, path: &str, field: &str) -> Option<&Value> {
        self.spec(path)?.field(field)
    }
}

/// A file of a package.
#[derive(Debug, Clone, PartialEq)]
pub struct PackageFile {
    pub name: String,
    /// Where its data starts in the package.
    pub offset: usize,
    pub data: Vec<u8>,
}

/// A USDZ package read back: its files in order and its first file's layer.
#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    pub files: Vec<PackageFile>,
    pub layer: Layer,
}

fn fail(message: impl Into<String>) -> Error {
    Error(message.into())
}

/// Reads a USDZ package: every file stored (not compressed), its data at a
/// multiple of 64 bytes from the start, the first a `.usdc` layer, the
/// others of the types a package may hold.
pub fn read_package(data: &[u8]) -> Result<Package, Error> {
    let archive = mitcad_zip::Archive::new(data.to_vec()).map_err(|e| fail(format!("zip: {e}")))?;
    let mut files = Vec::new();
    for entry in archive.entries() {
        if entry.method != mitcad_zip::Method::Stored {
            return Err(fail(format!(
                "{} is compressed ({})",
                entry.name, entry.method
            )));
        }
        if entry.flags & 1 != 0 {
            return Err(fail(format!("{} is encrypted", entry.name)));
        }
        let offset = archive
            .data_offset(entry)
            .map_err(|e| fail(format!("{}: {e}", entry.name)))?;
        if offset % 64 != 0 {
            return Err(fail(format!(
                "{}'s data is at {offset}, not a multiple of 64",
                entry.name
            )));
        }
        let extension = entry
            .name
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(
            extension.as_str(),
            "usda"
                | "usdc"
                | "usd"
                | "usdz"
                | "png"
                | "jpg"
                | "jpeg"
                | "exr"
                | "m4a"
                | "mp3"
                | "wav"
        ) {
            return Err(fail(format!(
                "{} is no file type a package holds",
                entry.name
            )));
        }
        // Stored, so no larger than the package itself: the zip reader's
        // large entry limit (mitcad#923).
        let data = archive
            .read_with_limit(entry, mitcad_zip::LARGE_ENTRY_LIMIT)
            .map_err(|e| fail(format!("{}: {e}", entry.name)))?;
        files.push(PackageFile {
            name: entry.name.clone(),
            offset,
            data,
        });
    }
    let first = files.first().ok_or_else(|| fail("the package is empty"))?;
    if !first.name.ends_with(".usdc") {
        return Err(fail(format!(
            "the first file {} is no .usdc layer",
            first.name
        )));
    }
    let layer = read_layer(&first.data)?;
    Ok(Package { files, layer })
}

/// The bytes of a little-endian reader over a layer.
struct Bytes<'a> {
    data: &'a [u8],
}

impl<'a> Bytes<'a> {
    fn get(&self, at: usize, len: usize) -> Result<&'a [u8], Error> {
        at.checked_add(len)
            .and_then(|end| self.data.get(at..end))
            .ok_or_else(|| fail(format!("{len} bytes at {at} are outside the layer")))
    }

    fn u64(&self, at: usize) -> Result<u64, Error> {
        let b = self.get(at, 8)?;
        Ok(u64::from_le_bytes(b.try_into().expect("8 bytes")))
    }

    fn u32(&self, at: usize) -> Result<u32, Error> {
        let b = self.get(at, 4)?;
        Ok(u32::from_le_bytes(b.try_into().expect("4 bytes")))
    }

    fn usize(&self, at: usize) -> Result<usize, Error> {
        usize::try_from(self.u64(at)?).map_err(|_| fail("a size beyond memory"))
    }
}

/// A cursor over a section.
struct Cursor<'a> {
    bytes: &'a Bytes<'a>,
    at: usize,
}

impl Cursor<'_> {
    fn u64(&mut self) -> Result<u64, Error> {
        let v = self.bytes.u64(self.at)?;
        self.at += 8;
        Ok(v)
    }

    fn count(&mut self) -> Result<usize, Error> {
        let v = self.u64()?;
        // No list of the layer has more entries than the layer has bytes.
        if v > self.bytes.data.len() as u64 {
            return Err(fail(format!(
                "a count of {v} in a layer of {} bytes",
                self.bytes.data.len()
            )));
        }
        Ok(v as usize)
    }

    fn take(&mut self, len: usize) -> Result<&[u8], Error> {
        let b = self.bytes.get(self.at, len)?;
        self.at += len;
        Ok(b)
    }

    /// A compressed integer list of `count` entries.
    fn ints(&mut self, count: usize) -> Result<Vec<i32>, Error> {
        let size = self.count()?;
        let data = self.take(size)?;
        let values = decompress_ints(data, count).map_err(fail)?;
        if values.len() != count {
            return Err(fail("an integer list of the wrong length"));
        }
        Ok(values)
    }
}

/// Reads a binary layer: its specs with their fields in order.
pub fn read_layer(data: &[u8]) -> Result<Layer, Error> {
    let bytes = Bytes { data };
    if bytes.get(0, 8)? != b"PXR-USDC" {
        return Err(fail("no PXR-USDC header"));
    }
    if data.len() < BOOTSTRAP_SIZE {
        return Err(fail("the header is cut short"));
    }
    let version = [data[8], data[9], data[10]];
    if version[0] != 0 || version[1] < 4 {
        return Err(fail(format!("version {version:?} is not read")));
    }
    let toc = bytes.usize(16)?;
    let count = bytes.usize(toc)?;
    let mut sections = std::collections::HashMap::new();
    for i in 0..count {
        let at = toc + 8 + i * 32;
        let name = bytes.get(at, 16)?;
        let name =
            String::from_utf8_lossy(name.split(|&b| b == 0).next().unwrap_or(&[])).into_owned();
        let start = bytes.usize(at + 16)?;
        let size = bytes.usize(at + 24)?;
        bytes.get(start, size)?;
        if start < BOOTSTRAP_SIZE || start + size > toc {
            return Err(fail(format!(
                "section {name} overlaps the header or the table"
            )));
        }
        sections.insert(name, start);
    }
    let section = |name: &str| -> Result<Cursor<'_>, Error> {
        let at = *sections
            .get(name)
            .ok_or_else(|| fail(format!("no {name} section")))?;
        Ok(Cursor { bytes: &bytes, at })
    };

    // TOKENS
    let mut c = section("TOKENS")?;
    let token_count = c.count()?;
    let size = c.count()?;
    let compressed_size = c.count()?;
    let text = fast_decompress(c.take(compressed_size)?, size).map_err(fail)?;
    if text.len() != size || text.last() != Some(&0) && size > 0 {
        return Err(fail("the tokens are cut short"));
    }
    let tokens: Vec<String> = if size == 0 {
        Vec::new()
    } else {
        text[..size - 1]
            .split(|&b| b == 0)
            .map(|t| String::from_utf8(t.to_vec()).map_err(|_| fail("a token is no UTF-8")))
            .collect::<Result<_, _>>()?
    };
    if tokens.len() != token_count {
        return Err(fail(format!(
            "{} tokens where the section says {token_count}",
            tokens.len()
        )));
    }
    let token = |index: u32| -> Result<String, Error> {
        tokens
            .get(index as usize)
            .cloned()
            .ok_or_else(|| fail(format!("token {index} of {}", tokens.len())))
    };
    // STRINGS
    let mut c = section("STRINGS")?;
    let string_count = c.count()?;
    let mut strings = Vec::with_capacity(string_count);
    for _ in 0..string_count {
        let index = u32::from_le_bytes(c.take(4)?.try_into().expect("4 bytes"));
        strings.push(token(index)?);
    }
    // FIELDS
    let mut c = section("FIELDS")?;
    let field_count = c.count()?;
    let names = c.ints(field_count)?;
    let reps_size = c.count()?;
    let reps = fast_decompress(c.take(reps_size)?, field_count * 8).map_err(fail)?;
    if reps.len() != field_count * 8 {
        return Err(fail("the field values are cut short"));
    }
    // FIELDSETS
    let mut c = section("FIELDSETS")?;
    let set_count = c.count()?;
    let sets = c.ints(set_count)?;
    // PATHS
    let mut c = section("PATHS")?;
    let path_count = c.count()?;
    let encoded = c.count()?;
    let indexes = c.ints(encoded)?;
    let elements = c.ints(encoded)?;
    let jumps = c.ints(encoded)?;
    let paths = decode_paths(path_count, &indexes, &elements, &jumps, &tokens)?;
    // SPECS
    let mut c = section("SPECS")?;
    let spec_count = c.count()?;
    let spec_paths = c.ints(spec_count)?;
    let spec_sets = c.ints(spec_count)?;
    let spec_types = c.ints(spec_count)?;

    let values = Values {
        bytes: &bytes,
        tokens: &tokens,
        strings: &strings,
        paths: &paths,
    };
    let mut specs = Vec::with_capacity(spec_count);
    for i in 0..spec_count {
        let path = paths
            .get(spec_paths[i] as usize)
            .and_then(Option::as_ref)
            .ok_or_else(|| fail(format!("spec {i} has no path")))?
            .clone();
        let spec_type = SpecType::from_code(spec_types[i] as u32)
            .ok_or_else(|| fail(format!("{path}: spec type {}", spec_types[i])))?;
        let mut fields = Vec::new();
        let mut at = usize::try_from(spec_sets[i]).map_err(|_| fail("a negative field set"))?;
        loop {
            let index = *sets
                .get(at)
                .ok_or_else(|| fail(format!("{path}: its field set is not ended")))?;
            if index == -1 {
                break;
            }
            let index = index as usize;
            let name = token(
                *names
                    .get(index)
                    .ok_or_else(|| fail("a field out of range"))? as u32,
            )?;
            let rep = u64::from_le_bytes(reps[index * 8..index * 8 + 8].try_into().expect("8"));
            let value = values
                .value(rep)
                .map_err(|e| fail(format!("{path}: {name}: {}", e.0)))?;
            fields.push((name, value));
            at += 1;
        }
        specs.push(Spec {
            path,
            spec_type,
            fields,
        });
    }
    Ok(Layer { version, specs })
}

/// The paths of the depth-first columns; `count` entries, None for those
/// no column fills.
fn decode_paths(
    count: usize,
    indexes: &[i32],
    elements: &[i32],
    jumps: &[i32],
    tokens: &[String],
) -> Result<Vec<Option<Path>>, Error> {
    let mut paths: Vec<Option<Path>> = vec![None; count];
    // (index into the columns, parent path); None parent for the root.
    let mut pending: Vec<(usize, Option<Path>)> = vec![(0, None)];
    let mut visited = 0;
    while let Some((start, parent)) = pending.pop() {
        let mut i = start;
        let mut parent = parent;
        loop {
            visited += 1;
            if visited > indexes.len() || i >= indexes.len() {
                return Err(fail("the path tree is corrupt"));
            }
            let slot = usize::try_from(indexes[i])
                .ok()
                .filter(|&s| s < count)
                .ok_or_else(|| fail("a path index out of range"))?;
            let path = match &parent {
                None => Path::root(),
                Some(parent) => {
                    let element = elements[i];
                    let name = tokens
                        .get(element.unsigned_abs() as usize)
                        .ok_or_else(|| fail("a path element out of range"))?;
                    if element < 0 {
                        parent.property(name)
                    } else {
                        parent.child(name)
                    }
                }
            };
            if paths[slot].replace(path.clone()).is_some() {
                return Err(fail("a path index used twice"));
            }
            let jump = jumps[i];
            let child = jump > 0 || jump == -1;
            let sibling = jump >= 0;
            if child && sibling {
                pending.push((i + jump as usize, parent.clone()));
            }
            if child {
                parent = Some(path);
            }
            if !(child || sibling) {
                break;
            }
            i += 1;
        }
    }
    Ok(paths)
}

/// Decodes value references.
struct Values<'a> {
    bytes: &'a Bytes<'a>,
    tokens: &'a [String],
    strings: &'a [String],
    paths: &'a [Option<Path>],
}

impl Values<'_> {
    fn token(&self, index: u32) -> Result<String, Error> {
        self.tokens
            .get(index as usize)
            .cloned()
            .ok_or_else(|| fail(format!("token {index} out of range")))
    }

    fn path(&self, index: u32) -> Result<Path, Error> {
        self.paths
            .get(index as usize)
            .and_then(Option::clone)
            .ok_or_else(|| fail(format!("path {index} out of range")))
    }

    fn value(&self, rep: u64) -> Result<Value, Error> {
        let kind = ((rep >> 48) & 0xff) as u8;
        let payload = rep & PAYLOAD_MASK;
        let small = payload as u32;
        let inline = rep & INLINED_BIT != 0;
        let at = payload as usize;
        let b = self.bytes;
        use types::*;
        if rep & ARRAY_BIT != 0 {
            return self.array(kind, payload, rep & COMPRESSED_BIT != 0);
        }
        Ok(match kind {
            BOOL => Value::Bool(small != 0),
            INT => Value::Int(small as i32),
            FLOAT => Value::Float(f32::from_bits(small)),
            DOUBLE if inline => Value::Double(f64::from(f32::from_bits(small))),
            DOUBLE => Value::Double(f64::from_le_bytes(b.get(at, 8)?.try_into().expect("8"))),
            TOKEN => Value::Token(self.token(small)?),
            STRING => Value::String(
                self.strings
                    .get(small as usize)
                    .cloned()
                    .ok_or_else(|| fail("a string out of range"))?,
            ),
            ASSET_PATH => Value::Asset(self.token(small)?),
            SPECIFIER => Value::Specifier(match small {
                0 => Specifier::Def,
                1 => Specifier::Over,
                2 => Specifier::Class,
                other => return Err(fail(format!("specifier {other}"))),
            }),
            VARIABILITY => Value::Variability(match small {
                0 => Variability::Varying,
                1 => Variability::Uniform,
                other => return Err(fail(format!("variability {other}"))),
            }),
            TOKEN_VECTOR => {
                let count = b.usize(at)?;
                let mut names = Vec::with_capacity(count.min(1 << 20));
                for i in 0..count {
                    names.push(self.token(b.u32(at + 8 + 4 * i)?)?);
                }
                Value::TokenVector(names)
            }
            TOKEN_LIST_OP => Value::TokenListOp(self.list_op(at, |index| self.token(index))?),
            PATH_LIST_OP => Value::PathListOp(self.list_op(at, |index| self.path(index))?),
            VEC3F if inline => {
                let [x, y, z, _] = small.to_le_bytes();
                Value::Vec3f([x, y, z].map(|c| f32::from(c as i8)))
            }
            VEC3F => {
                let d = b.get(at, 12)?;
                Value::Vec3f(std::array::from_fn(|i| {
                    f32::from_le_bytes(d[4 * i..4 * i + 4].try_into().expect("4"))
                }))
            }
            MATRIX4D if inline => {
                let diagonal = small.to_le_bytes();
                Value::Matrix4d(std::array::from_fn(|r| {
                    std::array::from_fn(|c| {
                        if r == c {
                            f64::from(diagonal[r] as i8)
                        } else {
                            0.0
                        }
                    })
                }))
            }
            MATRIX4D => {
                let d = b.get(at, 128)?;
                Value::Matrix4d(std::array::from_fn(|r| {
                    std::array::from_fn(|c| {
                        let i = (r * 4 + c) * 8;
                        f64::from_le_bytes(d[i..i + 8].try_into().expect("8"))
                    })
                }))
            }
            DOUBLE_VECTOR => {
                let count = b.usize(at)?;
                let d = b.get(
                    at + 8,
                    count
                        .checked_mul(8)
                        .ok_or_else(|| fail("a list too large"))?,
                )?;
                Value::DoubleVector(
                    d.as_chunks::<8>()
                        .0
                        .iter()
                        .map(|c| f64::from_le_bytes(*c))
                        .collect(),
                )
            }
            TIME_SAMPLES => self.time_samples(at)?,
            other => return Err(fail(format!("value type {other} is not read"))),
        })
    }

    /// Time samples at `at` (mitcad#1597, `usdc`'s module documentation): a
    /// jump to the times' reference, a jump to the count and the values'
    /// references; one value per time, the times increasing.
    fn time_samples(&self, at: usize) -> Result<Value, Error> {
        let b = self.bytes;
        // A jump is relative to where it is written.
        let jump = |from: usize| -> Result<usize, Error> {
            let offset =
                isize::try_from(b.u64(from)? as i64).map_err(|_| fail("a jump beyond memory"))?;
            from.checked_add_signed(offset)
                .filter(|&to| to > from)
                .ok_or_else(|| fail("a time samples jump that does not go forward"))
        };
        let kind = |rep: u64| ((rep >> 48) & 0xff) as u8;
        let times_at = jump(at)?;
        let times_rep = b.u64(times_at)?;
        if kind(times_rep) != types::DOUBLE_VECTOR || times_rep & ARRAY_BIT != 0 {
            return Err(fail(format!(
                "the times of time samples are of type {}, not a list of doubles",
                kind(times_rep)
            )));
        }
        let Value::DoubleVector(times) = self.value(times_rep)? else {
            unreachable!("a list of doubles")
        };
        if times.iter().any(|t| !t.is_finite()) || times.windows(2).any(|w| w[1] <= w[0]) {
            return Err(fail(format!(
                "the times of time samples {times:?} do not increase"
            )));
        }
        let values_at = jump(times_at + 8)?;
        let count = b.usize(values_at)?;
        if count != times.len() {
            return Err(fail(format!(
                "{count} time sample values for {} times",
                times.len()
            )));
        }
        let mut samples = Vec::with_capacity(count);
        for (i, time) in times.into_iter().enumerate() {
            let rep = b.u64(values_at + 8 + 8 * i)?;
            if kind(rep) == types::TIME_SAMPLES {
                return Err(fail("time samples inside time samples"));
            }
            samples.push((time, self.value(rep)?));
        }
        Ok(Value::TimeSamples(samples))
    }

    fn list_op<T>(
        &self,
        at: usize,
        item: impl Fn(u32) -> Result<T, Error>,
    ) -> Result<ListOp<T>, Error> {
        let b = self.bytes;
        let bits = b.get(at, 1)?[0];
        let mut pos = at + 1;
        let mut list = |present: bool| -> Result<Vec<T>, Error> {
            if !present {
                return Ok(Vec::new());
            }
            let count = b.usize(pos)?;
            b.get(pos + 8, count.saturating_mul(4))?;
            let items = (0..count)
                .map(|i| b.u32(pos + 8 + 4 * i).and_then(&item))
                .collect::<Result<Vec<_>, _>>()?;
            pos += 8 + 4 * count;
            Ok(items)
        };
        if bits
            & !(list_op::IS_EXPLICIT | list_op::HAS_EXPLICIT_ITEMS | list_op::HAS_PREPENDED_ITEMS)
            != 0
        {
            return Err(fail(format!("list op bits {bits:#x} are not read")));
        }
        let explicit_items = list(bits & list_op::HAS_EXPLICIT_ITEMS != 0)?;
        let prepended_items = list(bits & list_op::HAS_PREPENDED_ITEMS != 0)?;
        Ok(ListOp {
            explicit: bits & list_op::IS_EXPLICIT != 0,
            explicit_items,
            prepended_items,
        })
    }

    fn array(&self, kind: u8, payload: u64, compressed: bool) -> Result<Value, Error> {
        use types::*;
        let b = self.bytes;
        if payload == 0 {
            return Ok(match kind {
                INT => Value::IntArray(Vec::new()),
                FLOAT => Value::FloatArray(Vec::new()),
                VEC2F => Value::Vec2fArray(Vec::new()),
                VEC3F => Value::Vec3fArray(Vec::new()),
                TOKEN => Value::TokenArray(Vec::new()),
                other => return Err(fail(format!("array type {other} is not read"))),
            });
        }
        let at = payload as usize;
        let count = b.usize(at)?;
        let floats = |n: usize| -> Result<Vec<f32>, Error> {
            let d = b.get(
                at + 8,
                n.checked_mul(4).ok_or_else(|| fail("an array too large"))?,
            )?;
            Ok(d.as_chunks::<4>()
                .0
                .iter()
                .map(|c| f32::from_le_bytes(*c))
                .collect())
        };
        Ok(match kind {
            INT if compressed && count >= MIN_COMPRESSED_ARRAY => {
                let mut c = Cursor {
                    bytes: b,
                    at: at + 8,
                };
                Value::IntArray(c.ints(count)?)
            }
            INT => Value::IntArray(floats(count)?.iter().map(|f| f.to_bits() as i32).collect()),
            FLOAT if !compressed => Value::FloatArray(floats(count)?),
            VEC2F => Value::Vec2fArray(
                floats(
                    count
                        .checked_mul(2)
                        .ok_or_else(|| fail("an array too large"))?,
                )?
                .as_chunks::<2>()
                .0
                .to_vec(),
            ),
            VEC3F => Value::Vec3fArray(
                floats(
                    count
                        .checked_mul(3)
                        .ok_or_else(|| fail("an array too large"))?,
                )?
                .as_chunks::<3>()
                .0
                .to_vec(),
            ),
            TOKEN => Value::TokenArray(
                floats(count)?
                    .iter()
                    .map(|f| self.token(f.to_bits()))
                    .collect::<Result<_, _>>()?,
            ),
            other => return Err(fail(format!("array type {other} is not read"))),
        })
    }
}
