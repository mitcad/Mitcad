// SPDX-License-Identifier: MIT
//! Writing specs as a binary USD layer (`.usdc`, the "crate" format,
//! version 0.8.0, which OpenUSD writes by default and every reader since
//! 2019 reads).
//!
//! The file is a bootstrap header (`PXR-USDC`, the version, the table of
//! contents' offset), the values that do not fit in a field's 8-byte value
//! reference, six structural sections and the table of contents:
//!
//! - `TOKENS`: every name and token, NUL-terminated, LZ4-compressed;
//! - `STRINGS`: string values as token indices;
//! - `FIELDS`: a token index (the name) and a value reference per field;
//! - `FIELDSETS`: each spec's field indices, ended by `!0`;
//! - `PATHS`: the path tree in depth-first order (path index, element
//!   token, negative for properties, and a jump to the next sibling);
//! - `SPECS`: per spec its path index, field set index and spec type.
//!
//! Integer lists are written with USD's integer coding, the rest of the
//! structure LZ4-compressed ([`crate::compress`]). A value reference is 64
//! bits: an array flag (bit 63), an inlined flag (62), a compressed flag
//! (61), the type (bits 48-55) and 48 bits of payload, the value itself
//! when it fits in 32 bits (tokens, strings and asset paths by their index)
//! or else the offset of its data in the file. Arrays start 8-byte aligned
//! with their 64-bit length; int arrays of 16 or more are compressed.
//!
//! Time samples (mitcad#1597) are written as OpenUSD writes them: the
//! times (a `std::vector<double>`: its 64-bit count and the doubles) and
//! each value's data first, then at the value's offset a 64-bit jump (8:
//! to what follows it) to the times' reference, another jump of 8, the
//! number of values and a reference per value.

use std::collections::HashMap;

use crate::compress::{compress_ints, fast_compress};
use crate::sdf::{ListOp, Path, Spec, Value};

/// The version written.
pub const VERSION: [u8; 3] = [0, 8, 0];
/// Size of the bootstrap header.
pub const BOOTSTRAP_SIZE: usize = 88;
/// Int arrays shorter than this are not compressed.
pub const MIN_COMPRESSED_ARRAY: usize = 16;

/// Value types (`Sdf_CrateFile::TypeEnum`).
pub mod types {
    pub const BOOL: u8 = 1;
    pub const INT: u8 = 3;
    pub const FLOAT: u8 = 8;
    pub const DOUBLE: u8 = 9;
    pub const STRING: u8 = 10;
    pub const TOKEN: u8 = 11;
    pub const ASSET_PATH: u8 = 12;
    pub const MATRIX4D: u8 = 15;
    pub const VEC2F: u8 = 20;
    pub const VEC3F: u8 = 24;
    pub const TOKEN_LIST_OP: u8 = 32;
    pub const PATH_LIST_OP: u8 = 34;
    pub const TOKEN_VECTOR: u8 = 41;
    pub const SPECIFIER: u8 = 42;
    pub const VARIABILITY: u8 = 44;
    pub const TIME_SAMPLES: u8 = 46;
    pub const DOUBLE_VECTOR: u8 = 48;
}

pub const ARRAY_BIT: u64 = 1 << 63;
pub const INLINED_BIT: u64 = 1 << 62;
pub const COMPRESSED_BIT: u64 = 1 << 61;
pub const PAYLOAD_MASK: u64 = (1 << 48) - 1;

/// `SdfListOp` header bits.
pub mod list_op {
    pub const IS_EXPLICIT: u8 = 1 << 0;
    pub const HAS_EXPLICIT_ITEMS: u8 = 1 << 1;
    pub const HAS_PREPENDED_ITEMS: u8 = 1 << 5;
}

fn rep(kind: u8, payload: u64) -> u64 {
    (u64::from(kind) << 48) | (payload & PAYLOAD_MASK)
}

fn inlined(kind: u8, payload: u32) -> u64 {
    rep(kind, u64::from(payload)) | INLINED_BIT
}

/// Interned tokens and strings, and the path table.
#[derive(Default)]
struct Tables {
    tokens: Vec<String>,
    token_index: HashMap<String, u32>,
    strings: Vec<u32>,
    string_index: HashMap<u32, u32>,
    path_index: HashMap<Path, u32>,
}

impl Tables {
    fn token(&mut self, text: &str) -> u32 {
        if let Some(&index) = self.token_index.get(text) {
            return index;
        }
        let index = self.tokens.len() as u32;
        self.tokens.push(text.to_owned());
        self.token_index.insert(text.to_owned(), index);
        index
    }

    fn string(&mut self, text: &str) -> u32 {
        let token = self.token(text);
        let next = self.strings.len() as u32;
        *self.string_index.entry(token).or_insert_with(|| {
            self.strings.push(token);
            next
        })
    }

    fn path(&self, path: &Path) -> u32 {
        self.path_index[path]
    }
}

/// The path tree: each path's children in the order they were first seen.
struct PathTree {
    children: HashMap<Path, Vec<Path>>,
}

impl PathTree {
    fn new<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Self {
        let mut tree = Self {
            children: HashMap::new(),
        };
        let mut known: std::collections::HashSet<Path> = [Path::root()].into();
        for path in paths {
            tree.add(path, &mut known);
        }
        tree
    }

    fn add(&mut self, path: &Path, known: &mut std::collections::HashSet<Path>) {
        if known.contains(path) {
            return;
        }
        let parent = path.parent().expect("only the root has no parent");
        self.add(&parent, known);
        known.insert(path.clone());
        self.children.entry(parent).or_default().push(path.clone());
    }

    /// Depth-first order from the root, each path's children after it.
    fn depth_first(&self) -> Vec<Path> {
        let mut out = Vec::new();
        let mut stack = vec![Path::root()];
        while let Some(path) = stack.pop() {
            if let Some(children) = self.children.get(&path) {
                stack.extend(children.iter().rev().cloned());
            }
            out.push(path);
        }
        out
    }
}

/// The binary layer of the specs. Every path a spec or a value names is in
/// the path table (with its ancestors). Panics on a property path whose
/// name is token 0, which the path encoding cannot tell from a prim's (the
/// first token is made a prim's name).
pub fn write(specs: &[Spec]) -> Vec<u8> {
    let mut tables = Tables::default();
    // Token 0 is the first prim's name: a property's element token is
    // stored negated, so it must not be 0.
    if let Some(first) = specs.iter().find_map(|s| s.path.prims.first()) {
        tables.token(first);
    }
    // The path table: the specs' paths and those their values name.
    let mut named: Vec<&Path> = specs.iter().map(|s| &s.path).collect();
    for spec in specs {
        for (_, value) in &spec.fields {
            if let Value::PathListOp(op) = value {
                named.extend(op.explicit_items.iter().chain(&op.prepended_items));
            }
        }
    }
    let tree = PathTree::new(named);
    let paths = tree.depth_first();
    for (i, path) in paths.iter().enumerate() {
        tables.path_index.insert(path.clone(), i as u32);
        if let Some(name) = path.name() {
            tables.token(name);
        }
    }

    let mut out = vec![0u8; BOOTSTRAP_SIZE];
    // Fields (name token, value rep), deduplicated; field sets likewise.
    let mut fields: Vec<(u32, u64)> = Vec::new();
    let mut field_index: HashMap<(u32, u64), u32> = HashMap::new();
    let mut field_sets: Vec<u32> = Vec::new();
    let mut field_set_index: HashMap<Vec<u32>, u32> = HashMap::new();
    let mut spec_rows: Vec<(u32, u32, u32)> = Vec::with_capacity(specs.len());
    for spec in specs {
        let mut set = Vec::with_capacity(spec.fields.len());
        for (name, value) in &spec.fields {
            let name = tables.token(name);
            let value = pack(value, &mut tables, &mut out);
            let next = fields.len() as u32;
            let index = *field_index.entry((name, value)).or_insert_with(|| {
                fields.push((name, value));
                next
            });
            set.push(index);
        }
        let next = field_sets.len() as u32;
        let set_index = match field_set_index.get(&set) {
            Some(&index) => index,
            None => {
                field_sets.extend(&set);
                field_sets.push(u32::MAX);
                field_set_index.insert(set, next);
                next
            }
        };
        spec_rows.push((tables.path(&spec.path), set_index, spec.spec_type as u32));
    }

    let mut sections: Vec<(&str, u64, u64)> = Vec::new();
    let mut section = |out: &mut Vec<u8>, name: &'static str, body: Vec<u8>| {
        sections.push((name, out.len() as u64, body.len() as u64));
        out.extend(body);
    };
    // TOKENS
    let mut body = Vec::new();
    let mut text = Vec::new();
    for token in &tables.tokens {
        text.extend_from_slice(token.as_bytes());
        text.push(0);
    }
    body.extend((tables.tokens.len() as u64).to_le_bytes());
    body.extend((text.len() as u64).to_le_bytes());
    let compressed = fast_compress(&text);
    body.extend((compressed.len() as u64).to_le_bytes());
    body.extend(compressed);
    section(&mut out, "TOKENS", body);
    // STRINGS
    let mut body = Vec::new();
    body.extend((tables.strings.len() as u64).to_le_bytes());
    for index in &tables.strings {
        body.extend(index.to_le_bytes());
    }
    section(&mut out, "STRINGS", body);
    // FIELDS
    let mut body = Vec::new();
    body.extend((fields.len() as u64).to_le_bytes());
    let names: Vec<i32> = fields.iter().map(|f| f.0 as i32).collect();
    compressed_ints(&mut body, &names);
    let reps: Vec<u8> = fields.iter().flat_map(|f| f.1.to_le_bytes()).collect();
    let compressed = fast_compress(&reps);
    body.extend((compressed.len() as u64).to_le_bytes());
    body.extend(compressed);
    section(&mut out, "FIELDS", body);
    // FIELDSETS
    let mut body = Vec::new();
    body.extend((field_sets.len() as u64).to_le_bytes());
    let sets: Vec<i32> = field_sets.iter().map(|&i| i as i32).collect();
    compressed_ints(&mut body, &sets);
    section(&mut out, "FIELDSETS", body);
    // PATHS
    let mut body = Vec::new();
    body.extend((paths.len() as u64).to_le_bytes());
    body.extend((paths.len() as u64).to_le_bytes());
    let (indexes, elements, jumps) = path_columns(&tree, &tables);
    compressed_ints(&mut body, &indexes);
    compressed_ints(&mut body, &elements);
    compressed_ints(&mut body, &jumps);
    section(&mut out, "PATHS", body);
    // SPECS
    let mut body = Vec::new();
    body.extend((spec_rows.len() as u64).to_le_bytes());
    for column in 0..3 {
        let values: Vec<i32> = spec_rows
            .iter()
            .map(|row| match column {
                0 => row.0,
                1 => row.1,
                _ => row.2,
            } as i32)
            .collect();
        compressed_ints(&mut body, &values);
    }
    section(&mut out, "SPECS", body);

    // The table of contents and the bootstrap header.
    let toc = out.len() as u64;
    out.extend((sections.len() as u64).to_le_bytes());
    for (name, start, size) in &sections {
        let mut padded = [0u8; 16];
        padded[..name.len()].copy_from_slice(name.as_bytes());
        out.extend(padded);
        out.extend(start.to_le_bytes());
        out.extend(size.to_le_bytes());
    }
    out[..8].copy_from_slice(b"PXR-USDC");
    out[8..11].copy_from_slice(&VERSION);
    out[16..24].copy_from_slice(&toc.to_le_bytes());
    out
}

/// A list of integers as USD writes them: the compressed size, then the
/// compressed data.
fn compressed_ints(out: &mut Vec<u8>, values: &[i32]) {
    let compressed = compress_ints(values);
    out.extend((compressed.len() as u64).to_le_bytes());
    out.extend(compressed);
}

/// The path tree's columns in depth-first order: path index, element token
/// (negated for properties) and jump (-2 a leaf without a next sibling, -1
/// only a child next, 0 only a sibling next, else the distance to the next
/// sibling while the first child follows).
fn path_columns(tree: &PathTree, tables: &Tables) -> (Vec<i32>, Vec<i32>, Vec<i32>) {
    let mut indexes = Vec::new();
    let mut elements = Vec::new();
    let mut jumps = Vec::new();
    // (path, has a next sibling)
    fn visit(
        path: &Path,
        sibling: bool,
        tree: &PathTree,
        tables: &Tables,
        columns: &mut (&mut Vec<i32>, &mut Vec<i32>, &mut Vec<i32>),
    ) {
        let this = columns.0.len();
        columns.0.push(tables.path(path) as i32);
        let element = match path.name() {
            None => 0,
            Some(name) => {
                let token = tables.token_index[name] as i32;
                if path.property.is_some() {
                    -token
                } else {
                    token
                }
            }
        };
        columns.1.push(element);
        columns.2.push(0);
        let children = tree.children.get(path).map(Vec::as_slice).unwrap_or(&[]);
        for (i, child) in children.iter().enumerate() {
            visit(child, i + 1 < children.len(), tree, tables, columns);
        }
        columns.2[this] = match (!children.is_empty(), sibling) {
            (true, true) => (columns.0.len() - this) as i32,
            (false, true) => 0,
            (true, false) => -1,
            (false, false) => -2,
        };
    }
    visit(
        &Path::root(),
        false,
        tree,
        tables,
        &mut (&mut indexes, &mut elements, &mut jumps),
    );
    (indexes, elements, jumps)
}

/// A value's 64-bit reference; data that does not fit is appended to
/// `out` first.
fn pack(value: &Value, tables: &mut Tables, out: &mut Vec<u8>) -> u64 {
    use types::*;
    let at = |out: &Vec<u8>| out.len() as u64;
    match value {
        Value::Bool(b) => inlined(BOOL, u32::from(*b)),
        Value::Int(i) => inlined(INT, *i as u32),
        Value::Float(f) => inlined(FLOAT, f.to_bits()),
        Value::Double(d) => {
            let f = *d as f32;
            if f64::from(f) == *d {
                inlined(DOUBLE, f.to_bits())
            } else {
                let offset = at(out);
                out.extend(d.to_le_bytes());
                rep(DOUBLE, offset)
            }
        }
        Value::Token(t) => inlined(TOKEN, tables.token(t)),
        Value::String(s) => inlined(STRING, tables.string(s)),
        Value::Asset(a) => inlined(ASSET_PATH, tables.token(a)),
        Value::Specifier(s) => inlined(SPECIFIER, *s as u32),
        Value::Variability(v) => inlined(VARIABILITY, *v as u32),
        Value::TokenVector(names) => {
            let indexes: Vec<u32> = names.iter().map(|n| tables.token(n)).collect();
            let offset = at(out);
            out.extend((indexes.len() as u64).to_le_bytes());
            for index in indexes {
                out.extend(index.to_le_bytes());
            }
            rep(TOKEN_VECTOR, offset)
        }
        Value::TokenListOp(op) => {
            let explicit: Vec<u32> = op.explicit_items.iter().map(|t| tables.token(t)).collect();
            let prepended: Vec<u32> = op.prepended_items.iter().map(|t| tables.token(t)).collect();
            let offset = at(out);
            list_op_out(out, op, &explicit, &prepended);
            rep(TOKEN_LIST_OP, offset)
        }
        Value::PathListOp(op) => {
            let explicit: Vec<u32> = op.explicit_items.iter().map(|p| tables.path(p)).collect();
            let prepended: Vec<u32> = op.prepended_items.iter().map(|p| tables.path(p)).collect();
            let offset = at(out);
            list_op_out(out, op, &explicit, &prepended);
            rep(PATH_LIST_OP, offset)
        }
        Value::Vec3f(v) => {
            let offset = at(out);
            for c in v {
                out.extend(c.to_le_bytes());
            }
            rep(VEC3F, offset)
        }
        Value::Matrix4d(m) => {
            let offset = at(out);
            for c in m.iter().flatten() {
                out.extend(c.to_le_bytes());
            }
            rep(MATRIX4D, offset)
        }
        Value::IntArray(values) => {
            if values.is_empty() {
                return rep(INT, 0) | ARRAY_BIT;
            }
            let offset = at(out);
            out.extend((values.len() as u64).to_le_bytes());
            if values.len() < MIN_COMPRESSED_ARRAY {
                for v in values {
                    out.extend(v.to_le_bytes());
                }
                rep(INT, offset) | ARRAY_BIT
            } else {
                compressed_ints(out, values);
                rep(INT, offset) | ARRAY_BIT | COMPRESSED_BIT
            }
        }
        Value::FloatArray(values) => array_out(
            out,
            FLOAT,
            values.len(),
            values.iter().flat_map(|f| f.to_le_bytes()),
        ),
        Value::Vec2fArray(values) => array_out(
            out,
            VEC2F,
            values.len(),
            values.iter().flatten().flat_map(|f| f.to_le_bytes()),
        ),
        Value::Vec3fArray(values) => array_out(
            out,
            VEC3F,
            values.len(),
            values.iter().flatten().flat_map(|f| f.to_le_bytes()),
        ),
        Value::TokenArray(names) => {
            let indexes: Vec<u32> = names.iter().map(|n| tables.token(n)).collect();
            array_out(
                out,
                TOKEN,
                indexes.len(),
                indexes.iter().flat_map(|i| i.to_le_bytes()),
            )
        }
        Value::DoubleVector(values) => {
            let offset = at(out);
            out.extend((values.len() as u64).to_le_bytes());
            for v in values {
                out.extend(v.to_le_bytes());
            }
            rep(DOUBLE_VECTOR, offset)
        }
        Value::TimeSamples(samples) => {
            // The times' and the values' data, then the jumps and the
            // references (see the module documentation).
            let times = Value::DoubleVector(samples.iter().map(|(t, _)| *t).collect());
            let times = pack(&times, tables, out);
            let values: Vec<u64> = samples
                .iter()
                .map(|(_, value)| pack(value, tables, out))
                .collect();
            let offset = at(out);
            out.extend(8i64.to_le_bytes());
            out.extend(times.to_le_bytes());
            out.extend(8i64.to_le_bytes());
            out.extend((values.len() as u64).to_le_bytes());
            for value in values {
                out.extend(value.to_le_bytes());
            }
            rep(TIME_SAMPLES, offset)
        }
    }
}

/// An uncompressed array: 8-byte aligned, its length as 64 bits, then the
/// elements.
fn array_out(out: &mut Vec<u8>, kind: u8, len: usize, bytes: impl Iterator<Item = u8>) -> u64 {
    if len == 0 {
        return rep(kind, 0) | ARRAY_BIT;
    }
    out.resize(out.len().next_multiple_of(8), 0);
    let offset = out.len() as u64;
    out.extend((len as u64).to_le_bytes());
    out.extend(bytes);
    rep(kind, offset) | ARRAY_BIT
}

/// A list op: the header byte, then each list present (64-bit count and
/// 32-bit indices).
fn list_op_out<T>(out: &mut Vec<u8>, op: &ListOp<T>, explicit: &[u32], prepended: &[u32]) {
    let mut bits = 0;
    if op.explicit {
        bits |= list_op::IS_EXPLICIT;
    }
    if !explicit.is_empty() {
        bits |= list_op::HAS_EXPLICIT_ITEMS;
    }
    if !prepended.is_empty() {
        bits |= list_op::HAS_PREPENDED_ITEMS;
    }
    out.push(bits);
    for list in [explicit, prepended] {
        if !list.is_empty() {
            out.extend((list.len() as u64).to_le_bytes());
            for index in list {
                out.extend(index.to_le_bytes());
            }
        }
    }
}
