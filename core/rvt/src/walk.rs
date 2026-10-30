// SPDX-License-Identifier: MIT
//! Records walked by the schema: an object is its class's fields in
//! declaration order, the base class's first, each read as its type's
//! encoding says ([`crate::schema::Encoding`]). Objects behind pointers are
//! not in place: each pointer leaves its class tag, and the objects follow
//! the record's own fields in the order their pointers came, their own
//! pointers' objects after them. A pointer's `u32` is 0 (null), all ones (an
//! object of its own) or an object number: the first pointer with a number
//! brings the object, later ones with the same number share it.
//!
//! The walk of a record must end exactly at the record's end; anything
//! else is an error, so a misread is found, not passed on.

use std::collections::{HashMap, VecDeque};
use std::fmt;

use serde_json::{Value as Json, json};

use crate::schema::{Encoding, PointerKind, Scalar, Schema};

/// By-value objects nest a few levels deep in real records.
const MAX_DEPTH: usize = 64;

/// Elements read per record at most (a damaged count cannot make the walk
/// run long).
const MAX_STEPS: usize = 50_000_000;

/// Values that read no bytes of their own (objects other than ElementIds,
/// fixed arrays) a record may hold: [`ZERO_BASE`] and [`ZERO_PER_BYTE`]
/// for every byte of the record (mitcad#924). Every other value reads at
/// least one byte, so a walk takes at most `17 × bytes + 1024` steps
/// however its schema nests: a class without stored fields in arrays of
/// arrays (`Array(65536, Array(65536, Object(empty)))`, which the schema
/// allows) cannot make a record of a few bytes take billions. Real
/// records stay far below the bound: a value without bytes of its own
/// holds values that have them (a matrix of doubles, an object of
/// fields), except an object of a class with no stored fields, which real
/// schemas use by value a few times per record at most.
const ZERO_BASE: usize = 1024;
const ZERO_PER_BYTE: usize = 16;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(u64),
    Float(f64),
    Text(String),
    Guid([u8; 16]),
    /// An object of the ElementId class (sign-extended: -1 is no element).
    Id(i64),
    /// A class tag (kind 0x0a).
    Tag(u16),
    Object(Box<Object>),
    List(Vec<Value>),
    /// A pointer: the index of its object in [`Record::pointees`], or None.
    Pointer(Option<usize>),
    /// A reference pointer's `u32` (kind 3).
    Reference(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    pub class: u16,
    /// (class of the field, field index in that class, value).
    pub fields: Vec<(u16, u16, Value)>,
}

/// A record walked: its object and the objects behind its pointers.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub root: Object,
    pub pointees: Vec<Object>,
    /// The pointees by their object numbers (what reference pointers name).
    pub numbered: HashMap<u32, usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WalkError {
    pub offset: usize,
    /// Where in the record: class and field names.
    pub path: String,
    pub message: String,
}

impl fmt::Display for WalkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "at byte {} ({}): {}",
            self.offset, self.path, self.message
        )
    }
}

impl std::error::Error for WalkError {}

struct Walker<'a> {
    schema: &'a Schema,
    data: &'a [u8],
    at: usize,
    queue: VecDeque<(usize, u16)>,
    pointees: Vec<Option<Object>>,
    numbered: HashMap<u32, usize>,
    path: Vec<String>,
    steps: usize,
    /// Values read that take no bytes of their own, and how many the
    /// record may hold ([`ZERO_BASE`]).
    zero: usize,
    zero_limit: usize,
    /// The fewest bytes an object of a class reads, by class tag.
    min_sizes: HashMap<u16, usize>,
    /// By class tag: how many values an object of the class is when it
    /// reads no bytes at all (None when it reads some, or is not known).
    empty_values: HashMap<u16, Option<usize>>,
}

impl Walker<'_> {
    fn fail(&self, message: impl Into<String>) -> WalkError {
        WalkError {
            offset: self.at,
            path: self.path.join("."),
            message: message.into(),
        }
    }

    fn take(&mut self, n: usize) -> Result<&[u8], WalkError> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.data.len());
        match end {
            Some(end) => {
                let bytes = &self.data[self.at..end];
                self.at = end;
                Ok(bytes)
            }
            None => Err(self.fail(format!("{n} bytes past the record's end"))),
        }
    }

    fn u16(&mut self) -> Result<u16, WalkError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, WalkError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64, WalkError> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes(b.try_into().expect("8 bytes")))
    }

    fn step(&mut self) -> Result<(), WalkError> {
        self.steps += 1;
        if self.steps > MAX_STEPS {
            return Err(self.fail("the record has too many values"));
        }
        Ok(())
    }

    /// Counts `n` values that read no bytes of their own against the
    /// record's allowance ([`ZERO_BASE`]).
    fn charge(&mut self, n: usize) -> Result<(), WalkError> {
        self.zero = self.zero.saturating_add(n);
        if self.zero > self.zero_limit {
            return Err(self.fail(format!(
                "more values than its {} bytes can hold",
                self.data.len()
            )));
        }
        Ok(())
    }

    /// Refuses `count` elements of `element` that cannot fit what is left
    /// of the record: elements of a known size in fewer bytes than they
    /// take, elements that read no bytes beyond the record's allowance.
    fn check_count(&mut self, count: usize, element: &Encoding) -> Result<(), WalkError> {
        let left = self.data.len() - self.at;
        let min = self.min_size(element, 0);
        if min > 0 {
            if count.saturating_mul(min) > left {
                return Err(self.fail(format!(
                    "{count} values of at least {min} bytes in {left} bytes"
                )));
            }
        } else if let Some(each) = self.empty_count(element, 0)
            && count.saturating_mul(each) > self.zero_limit.saturating_sub(self.zero)
        {
            return Err(self.fail(format!(
                "{count} values that read no bytes: more than its {} bytes can hold",
                self.data.len()
            )));
        }
        Ok(())
    }

    /// The fewest bytes a value of the encoding reads (a lower bound: 0
    /// where it is not known).
    fn min_size(&mut self, encoding: &Encoding, depth: usize) -> usize {
        if depth > MAX_DEPTH {
            return 0;
        }
        match encoding {
            Encoding::Scalar(s) => match s {
                Scalar::Bool | Scalar::U8 => 1,
                Scalar::U16 | Scalar::ClassTag => 2,
                Scalar::U32 | Scalar::F32 | Scalar::Text => 4,
                Scalar::F64 | Scalar::U64 => 8,
                Scalar::Guid => 16,
            },
            Encoding::Object(tag) if *tag == self.schema.element_id => self.schema.id_width,
            Encoding::Object(tag) => {
                if let Some(&size) = self.min_sizes.get(tag) {
                    return size;
                }
                // (A class holding itself by value gives no bound.)
                self.min_sizes.insert(*tag, 0);
                let schema = self.schema;
                let mut size = 0usize;
                for class_tag in schema.lineage(*tag) {
                    let Some(class) = schema.class(class_tag) else {
                        continue;
                    };
                    for field in class.fields.iter().filter(|f| f.ty.stored()) {
                        if let Ok(e) = field.ty.encoding() {
                            size = size.saturating_add(self.min_size(&e, depth + 1));
                        }
                    }
                }
                self.min_sizes.insert(*tag, size);
                size
            }
            Encoding::Pointer(_) | Encoding::Container(_) => 4,
            Encoding::Array(count, element) => {
                (*count as usize).saturating_mul(self.min_size(element, depth + 1))
            }
        }
    }

    /// How many values a value of the encoding is when it never reads a
    /// byte (objects of classes without stored fields, arrays of them),
    /// None when it reads some (or that is not known).
    fn empty_count(&mut self, encoding: &Encoding, depth: usize) -> Option<usize> {
        if depth > MAX_DEPTH {
            return None;
        }
        match encoding {
            Encoding::Object(tag) if *tag != self.schema.element_id => {
                if let Some(&count) = self.empty_values.get(tag) {
                    return count;
                }
                self.empty_values.insert(*tag, None);
                let schema = self.schema;
                let mut count = Some(1usize);
                'fields: for class_tag in schema.lineage(*tag) {
                    let Some(class) = schema.class(class_tag) else {
                        continue;
                    };
                    for field in class.fields.iter().filter(|f| f.ty.stored()) {
                        let n = field
                            .ty
                            .encoding()
                            .ok()
                            .and_then(|e| self.empty_count(&e, depth + 1));
                        count = count.zip(n).map(|(a, b)| a.saturating_add(b));
                        if count.is_none() {
                            break 'fields;
                        }
                    }
                }
                self.empty_values.insert(*tag, count);
                count
            }
            Encoding::Array(0, _) => Some(1),
            Encoding::Array(count, element) => self
                .empty_count(element, depth + 1)
                .map(|n| n.saturating_mul(*count as usize).saturating_add(1)),
            _ => None,
        }
    }

    fn object(&mut self, tag: u16, depth: usize) -> Result<Object, WalkError> {
        if depth > MAX_DEPTH {
            return Err(self.fail("objects nest too deep"));
        }
        let schema = self.schema;
        if schema.class(tag).is_none() {
            return Err(self.fail(format!("no class with tag {tag}")));
        }
        let mut fields = Vec::new();
        for class_tag in schema.lineage(tag) {
            let class = schema.class(class_tag).expect("lineage of known classes");
            for (index, field) in class.fields.iter().enumerate() {
                if !field.ty.stored() {
                    continue;
                }
                let encoding = field
                    .ty
                    .encoding()
                    .map_err(|e| self.fail(format!("{}.{}: {e}", class.name, field.name)))?;
                self.path.push(format!("{}.{}", class.name, field.name));
                let value = self.value(&encoding, depth)?;
                self.path.pop();
                fields.push((class_tag, index as u16, value));
            }
        }
        Ok(Object { class: tag, fields })
    }

    fn value(&mut self, encoding: &Encoding, depth: usize) -> Result<Value, WalkError> {
        self.step()?;
        let no_bytes = match encoding {
            Encoding::Object(tag) => *tag != self.schema.element_id,
            Encoding::Array(..) => true,
            _ => false,
        };
        if no_bytes {
            self.charge(1)?;
        }
        Ok(match encoding {
            Encoding::Scalar(s) => self.scalar(*s)?,
            Encoding::Object(tag) if *tag == self.schema.element_id => {
                if self.schema.id_width == 8 {
                    Value::Id(self.u64()? as i64)
                } else {
                    Value::Id(i64::from(self.u32()? as i32))
                }
            }
            Encoding::Object(tag) => Value::Object(Box::new(self.object(*tag, depth + 1)?)),
            Encoding::Pointer(PointerKind::Reference) => Value::Reference(self.u32()?),
            Encoding::Pointer(_) => self.pointer()?,
            Encoding::Array(count, element) => {
                self.check_count(*count as usize, element)?;
                let mut items = Vec::with_capacity((*count as usize).min(1024));
                for _ in 0..*count {
                    items.push(self.value(element, depth)?);
                }
                Value::List(items)
            }
            Encoding::Container(element) => {
                let count = self.u32()? as usize;
                let left = self.data.len() - self.at;
                if count > left + 65_536 {
                    return Err(self.fail(format!("a container of {count} in {left} bytes")));
                }
                self.check_count(count, element)?;
                let mut items = Vec::with_capacity(count.min(1024));
                for _ in 0..count {
                    items.push(self.value(element, depth)?);
                }
                Value::List(items)
            }
        })
    }

    fn scalar(&mut self, scalar: Scalar) -> Result<Value, WalkError> {
        Ok(match scalar {
            Scalar::Bool => Value::Bool(self.take(1)?[0] != 0),
            Scalar::U8 => Value::Int(u64::from(self.take(1)?[0])),
            Scalar::U16 => Value::Int(u64::from(self.u16()?)),
            Scalar::U32 => Value::Int(u64::from(self.u32()?)),
            Scalar::F32 => Value::Float(f64::from(f32::from_bits(self.u32()?))),
            Scalar::F64 => Value::Float(f64::from_bits(self.u64()?)),
            Scalar::U64 => Value::Int(self.u64()?),
            Scalar::Guid => Value::Guid(self.take(16)?.try_into().expect("16 bytes")),
            Scalar::ClassTag => Value::Tag(self.u16()?),
            Scalar::Text => {
                let units = self.u32()? as usize;
                let bytes = self.take(
                    units
                        .checked_mul(2)
                        .ok_or_else(|| self.fail("text length"))?,
                )?;
                Value::Text(crate::info::utf16(bytes))
            }
        })
    }

    fn pointer(&mut self) -> Result<Value, WalkError> {
        let number = self.u32()?;
        if number == 0 {
            return Ok(Value::Pointer(None));
        }
        if number != u32::MAX && number >= 0x0100_0000 {
            return Err(self.fail(format!("pointer word {number:#x}")));
        }
        let tag = self.u16()?;
        if self.schema.class(tag).is_none() {
            return Err(self.fail(format!("a pointer to class tag {tag}")));
        }
        if number != u32::MAX
            && let Some(&slot) = self.numbered.get(&number)
        {
            return Ok(Value::Pointer(Some(slot)));
        }
        let slot = self.pointees.len();
        self.pointees.push(None);
        self.queue.push_back((slot, tag));
        if number != u32::MAX {
            self.numbered.insert(number, slot);
        }
        Ok(Value::Pointer(Some(slot)))
    }
}

/// Walks a record: `data` starts with the record's `u16` class tag and
/// ends where the record ends.
pub fn walk(schema: &Schema, data: &[u8]) -> Result<Record, WalkError> {
    let (record, end) = walk_prefix(schema, data)?;
    if end != data.len() {
        return Err(WalkError {
            offset: end,
            path: schema.class_name(record.root.class).to_string(),
            message: format!(
                "the walk ends {} bytes before the record's end",
                data.len() - end
            ),
        });
    }
    Ok(record)
}

/// Walks the record at the start of `data` (from its class tag): the
/// record and where it ends.
pub fn walk_prefix(schema: &Schema, data: &[u8]) -> Result<(Record, usize), WalkError> {
    match walk_partial(schema, data) {
        (Some(record), end, None) => Ok((record, end)),
        (_, _, Some(e)) => Err(e),
        (None, _, None) => unreachable!("a walk without a record has an error"),
    }
}

/// A record walked as far as it goes: the record (None when its own
/// fields did not read), where the walk ended, and the error that stopped
/// it. Objects behind pointers that were not reached are empty (their
/// class, no fields): a record whose own fields read keeps them when an
/// object queued after them does not (data of a schema the file's classes
/// do not describe, as extensible storage entities).
pub fn walk_partial(schema: &Schema, data: &[u8]) -> (Option<Record>, usize, Option<WalkError>) {
    let mut walker = Walker {
        schema,
        data,
        at: 0,
        queue: VecDeque::new(),
        pointees: Vec::new(),
        numbered: HashMap::new(),
        path: Vec::new(),
        steps: 0,
        zero: 0,
        zero_limit: ZERO_BASE.saturating_add(data.len().saturating_mul(ZERO_PER_BYTE)),
        min_sizes: HashMap::new(),
        empty_values: HashMap::new(),
    };
    let tag = match walker.u16() {
        Ok(tag) => tag,
        Err(e) => return (None, 0, Some(e)),
    };
    walker.path.push(schema.class_name(tag).to_string());
    let root = match walker.object(tag, 0) {
        Ok(root) => root,
        Err(e) => return (None, walker.at, Some(e)),
    };
    let mut error = None;
    let mut failed = None;
    while let Some((slot, tag)) = walker.queue.pop_front() {
        walker.path = vec![format!("pointee {slot} {}", schema.class_name(tag))];
        match walker.object(tag, 0) {
            Ok(object) => walker.pointees[slot] = Some(object),
            Err(e) => {
                error = Some(e);
                failed = Some((slot, tag));
                break;
            }
        }
    }
    let end = walker.at;
    let numbered = std::mem::take(&mut walker.numbered);
    let classes: HashMap<usize, u16> = walker.queue.iter().copied().chain(failed).collect();
    let record = Record {
        numbered,
        root,
        pointees: walker
            .pointees
            .into_iter()
            .enumerate()
            .map(|(slot, o)| {
                o.unwrap_or_else(|| Object {
                    class: classes.get(&slot).copied().unwrap_or(0),
                    fields: Vec::new(),
                })
            })
            .collect(),
    };
    (Some(record), end, error)
}

impl Object {
    /// The value of the field `name` of this object's class or a base.
    pub fn get<'a>(&'a self, schema: &Schema, name: &str) -> Option<&'a Value> {
        self.fields.iter().find_map(|(class, index, value)| {
            let field = schema.class(*class)?.fields.get(usize::from(*index))?;
            (field.name == name).then_some(value)
        })
    }

    /// The value of `name` in this object's own class (not a base).
    pub fn get_in<'a>(&'a self, schema: &Schema, class: &str, name: &str) -> Option<&'a Value> {
        let tag = schema.tag(class)?;
        self.fields.iter().find_map(|(c, index, value)| {
            if *c != tag {
                return None;
            }
            let field = schema.class(*c)?.fields.get(usize::from(*index))?;
            (field.name == name).then_some(value)
        })
    }
}

impl Value {
    pub fn as_id(&self) -> Option<i64> {
        match self {
            Value::Id(id) => Some(*id),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(v) => Some(*v),
            Value::Int(v) => Some(*v as f64),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Int(v) => Some(*v),
            Value::Bool(b) => Some(u64::from(*b)),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Int(v) => Some(*v != 0),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&Object> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }

    /// Numbers of a (nested) list, flattened.
    pub fn floats(&self) -> Vec<f64> {
        match self {
            Value::List(items) => items.iter().flat_map(Value::floats).collect(),
            other => other.as_f64().into_iter().collect(),
        }
    }
}

impl Record {
    /// The object a pointer value points at.
    pub fn deref<'a>(&'a self, value: &'a Value) -> Option<&'a Object> {
        match value {
            Value::Pointer(Some(slot)) => self.pointees.get(*slot),
            Value::Object(object) => Some(object),
            // A reference to a numbered object of the record (the B-rep's
            // edges and faces naming each other).
            Value::Reference(number) => self.pointees.get(*self.numbered.get(number)?),
            _ => None,
        }
    }

    /// The record as JSON: objects as `{"class": ..., field: value}`,
    /// pointers as their objects' indices, the objects behind them in
    /// `pointees`, and the numbered objects' indices by number (what
    /// `reference`s name) when there are any.
    pub fn to_json(&self, schema: &Schema) -> Json {
        let mut out = json!({
            "object": object_json(schema, &self.root),
            "pointees": self.pointees.iter().map(|o| object_json(schema, o)).collect::<Vec<_>>(),
        });
        if !self.numbered.is_empty() {
            let numbered: std::collections::BTreeMap<String, usize> = self
                .numbered
                .iter()
                .map(|(n, s)| (n.to_string(), *s))
                .collect();
            out["numbered"] = json!(numbered);
        }
        out
    }
}

/// An object as JSON (see [`Record::to_json`]).
pub fn object_json(schema: &Schema, object: &Object) -> Json {
    let mut map = serde_json::Map::new();
    map.insert("class".into(), json!(schema.class_name(object.class)));
    for (class, index, value) in &object.fields {
        let name = schema
            .class(*class)
            .and_then(|c| c.fields.get(usize::from(*index)))
            .map_or("?", |f| f.name.as_str());
        let key = if map.contains_key(name) {
            format!("{}.{name}", schema.class_name(*class))
        } else {
            name.to_string()
        };
        map.insert(key, value_json(schema, value));
    }
    Json::Object(map)
}

fn value_json(schema: &Schema, value: &Value) -> Json {
    match value {
        Value::Bool(b) => json!(b),
        Value::Int(v) => json!(v),
        Value::Float(v) if v.is_finite() => json!(v),
        Value::Float(v) => json!(v.to_string()),
        Value::Text(t) => json!(t),
        Value::Guid(g) => json!(mitcad_cfb::guid_text(g)),
        Value::Id(id) => json!({ "id": id }),
        Value::Tag(t) => json!({ "class": schema.class_name(*t) }),
        Value::Object(o) => object_json(schema, o),
        Value::List(items) => Json::Array(items.iter().map(|v| value_json(schema, v)).collect()),
        Value::Pointer(None) => Json::Null,
        Value::Pointer(Some(slot)) => json!({ "pointee": slot }),
        Value::Reference(r) => json!({ "reference": r }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::types::*;
    use crate::schema::{SchemaWriter, Type};

    /// A schema of an element-like class with a pointer and a container,
    /// and a record of it.
    fn sample() -> (Schema, Vec<u8>) {
        let mut w = SchemaWriter::new();
        let identifier = w.class("Identifier", None, &[("m_id64", word(U64))]);
        let id = w.class("ElementId", None, &[("m_id", object(identifier))]);
        let outline = w.class("Outline", None, &[("m_minmax", matrix(2, 3))]);
        let item = w.class(
            "Item",
            None,
            &[("m_text", word(TEXT)), ("m_next", word(OWNED))],
        );
        let thing = w.class(
            "Thing",
            None,
            &[
                ("m_id", object(id)),
                ("m_box", word(OWNED)),
                ("m_items", container_of(id)),
                ("m_more", word(0x510e)),
                ("m_doc", word(DOC)),
                ("m_class", word(CLASS_TAG)),
                ("m_flag", word(BOOL)),
            ],
        );
        let schema = Schema::parse(&w.finish()).unwrap();
        let mut r = Vec::new();
        r.extend(thing.to_le_bytes());
        r.extend(7u64.to_le_bytes()); // m_id
        r.extend(u32::MAX.to_le_bytes()); // m_box -> Outline
        r.extend(outline.to_le_bytes());
        r.extend(2u32.to_le_bytes()); // m_items
        r.extend(5u64.to_le_bytes());
        r.extend(u64::MAX.to_le_bytes());
        r.extend(3u32.to_le_bytes()); // m_more: two items sharing object 4, and a null
        r.extend(4u32.to_le_bytes());
        r.extend(item.to_le_bytes());
        r.extend(4u32.to_le_bytes());
        r.extend(item.to_le_bytes());
        r.extend(0u32.to_le_bytes());
        r.extend(1u32.to_le_bytes()); // m_doc
        r.extend(outline.to_le_bytes()); // m_class
        r.push(1); // m_flag
        for v in [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0] {
            r.extend(v.to_le_bytes()); // the Outline
        }
        r.extend(2u32.to_le_bytes()); // the Item: "hi", and its own next
        r.extend("hi".encode_utf16().flat_map(u16::to_le_bytes));
        r.extend(u32::MAX.to_le_bytes());
        r.extend(item.to_le_bytes());
        r.extend(0u32.to_le_bytes()); // the next Item: "", no next
        r.extend(0u32.to_le_bytes());
        (schema, r)
    }

    #[test]
    fn walks_pointers_after_the_fields() {
        let (schema, data) = sample();
        let record = walk(&schema, &data).unwrap();
        let root = &record.root;
        assert_eq!(root.get(&schema, "m_id"), Some(&Value::Id(7)));
        let items = root.get(&schema, "m_items").unwrap().as_list().unwrap();
        assert_eq!(items, &[Value::Id(5), Value::Id(-1)]);
        let outline = record.deref(root.get(&schema, "m_box").unwrap()).unwrap();
        assert_eq!(
            outline.get(&schema, "m_minmax").unwrap().floats(),
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]
        );
        let more = root.get(&schema, "m_more").unwrap().as_list().unwrap();
        assert_eq!(more[0], more[1], "numbered pointers share their object");
        assert_eq!(more[2], Value::Pointer(None));
        let item = record.deref(&more[0]).unwrap();
        assert_eq!(item.get(&schema, "m_text").unwrap().as_text(), Some("hi"));
        let next = record.deref(item.get(&schema, "m_next").unwrap()).unwrap();
        assert_eq!(next.get(&schema, "m_text").unwrap().as_text(), Some(""));
        assert_eq!(root.get(&schema, "m_doc"), Some(&Value::Reference(1)));
        assert_eq!(
            root.get(&schema, "m_class"),
            Some(&Value::Tag(schema.tag("Outline").unwrap()))
        );
        let json = record.to_json(&schema);
        assert_eq!(json["object"]["class"], "Thing");
        assert_eq!(json["pointees"][0]["class"], "Outline");
    }

    /// The test binary's allocator: the system's, counting each thread's
    /// bytes in use and their peak (mitcad#924: what a walk allocates).
    pub(crate) mod counting {
        use std::alloc::{GlobalAlloc, Layout, System};
        use std::cell::Cell;

        pub struct Counting;

        thread_local! {
            static LIVE: Cell<isize> = const { Cell::new(0) };
            static PEAK: Cell<isize> = const { Cell::new(0) };
        }

        fn add(bytes: isize) {
            let _ = LIVE.try_with(|live| {
                let now = live.get() + bytes;
                live.set(now);
                let _ = PEAK.try_with(|peak| peak.set(peak.get().max(now)));
            });
        }

        // SAFETY: every call is passed on to the system allocator as it
        // came; the counters are thread-locals without destructors.
        unsafe impl GlobalAlloc for Counting {
            unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
                add(layout.size() as isize);
                // SAFETY: the caller's layout, as GlobalAlloc requires.
                unsafe { System.alloc(layout) }
            }
            unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
                add(-(layout.size() as isize));
                // SAFETY: allocated by `alloc` above with this layout.
                unsafe { System.dealloc(ptr, layout) }
            }
        }

        #[global_allocator]
        static ALLOCATOR: Counting = Counting;

        /// What `f` returns, and the most bytes it had allocated at once
        /// on this thread.
        pub fn peak<T>(f: impl FnOnce() -> T) -> (T, usize) {
            let start = LIVE.with(Cell::get);
            PEAK.with(|p| p.set(start));
            let out = f();
            let top = PEAK.with(Cell::get);
            (out, (top - start).max(0) as usize)
        }
    }

    /// A schema with a class without stored fields and a header class of
    /// `fields`.
    fn hostile(fields: impl FnOnce(u16) -> Vec<(&'static str, Type)>) -> (Schema, u16) {
        let mut w = SchemaWriter::new();
        let identifier = w.class("Identifier", None, &[("m_id64", word(U64))]);
        w.class("ElementId", None, &[("m_id", object(identifier))]);
        let empty = w.class("Empty", None, &[]);
        let header = w.class("Header", None, &fields(empty));
        (Schema::parse(&w.finish()).unwrap(), header)
    }

    #[test]
    fn values_without_bytes_are_bounded_by_the_record_s_size() {
        // mitcad#924: Array(65536, Array(65536, Object(empty))) on a
        // header class, the record only its class tag.
        let (schema, header) = hostile(|empty| {
            let inner = Type {
                count: Some(65_536),
                class: Some(empty),
                ..word(0x100e)
            };
            vec![(
                "m_nested",
                Type {
                    count: Some(65_536),
                    inner: Some(Box::new(inner)),
                    class: Some(empty),
                    ..word(0x100d)
                },
            )]
        });
        assert_eq!(
            schema.class(header).unwrap().fields[0].ty.encoding(),
            Ok(Encoding::Array(
                65_536,
                Box::new(Encoding::Array(
                    65_536,
                    Box::new(Encoding::Object(schema.tag("Empty").unwrap()))
                ))
            ))
        );
        let data = header.to_le_bytes().to_vec();
        // The best of three, against a busy machine's pauses.
        let mut best = std::time::Duration::MAX;
        let mut most = 0;
        for _ in 0..3 {
            let started = std::time::Instant::now();
            let (result, bytes) = counting::peak(|| walk(&schema, &data));
            best = best.min(started.elapsed());
            most = most.max(bytes);
            let e = result.unwrap_err();
            assert!(e.message.contains("can hold"), "{e}");
        }
        assert!(best.as_millis() < 10, "{best:?}");
        assert!(most < 1 << 20, "{most} bytes");

        // A container of objects without bytes: refused by its count.
        let (schema, header) = hostile(|empty| vec![("m_list", container_of(empty))]);
        let mut data = header.to_le_bytes().to_vec();
        data.extend(60_000u32.to_le_bytes());
        let e = walk(&schema, &data).unwrap_err();
        assert!(e.message.contains("read no bytes"), "{e}");
        // A few of them read.
        let mut data = header.to_le_bytes().to_vec();
        data.extend(3u32.to_le_bytes());
        let record = walk(&schema, &data).unwrap();
        assert_eq!(
            record
                .root
                .get(&schema, "m_list")
                .unwrap()
                .as_list()
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn counts_that_cannot_fit_the_record_are_refused() {
        // A container of 1000 doubles in 8 bytes.
        let (schema, header) = hostile(|_| vec![("m_values", word(0x5007))]);
        let mut data = header.to_le_bytes().to_vec();
        data.extend(1000u32.to_le_bytes());
        data.extend(1.0f64.to_le_bytes());
        let e = walk(&schema, &data).unwrap_err();
        assert!(e.message.contains("at least 8 bytes"), "{e}");
        // One double in its 8 bytes reads.
        let mut data = header.to_le_bytes().to_vec();
        data.extend(1u32.to_le_bytes());
        data.extend(1.5f64.to_le_bytes());
        let record = walk(&schema, &data).unwrap();
        assert_eq!(
            record.root.get(&schema, "m_values").unwrap().floats(),
            vec![1.5]
        );
    }

    #[test]
    fn a_walk_must_end_at_the_record_end() {
        let (schema, mut data) = sample();
        data.push(0);
        let e = walk(&schema, &data).unwrap_err();
        assert!(e.message.contains("before the record's end"), "{e}");
        let (schema, data) = sample();
        let e = walk(&schema, &data[..data.len() - 1]).unwrap_err();
        assert!(e.message.contains("past the record's end"), "{e}");
        // A pointer to a class the schema does not have.
        let (schema, mut data) = sample();
        data[14] = 0xee;
        data[15] = 0x7e;
        assert!(
            walk(&schema, &data)
                .unwrap_err()
                .message
                .contains("class tag")
        );
    }
}
