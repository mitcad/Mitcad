// SPDX-License-Identifier: MIT
//! The scene description data a layer holds: specs (the pseudo-root, prims,
//! attributes and relationships) at paths, each with fields of typed
//! values. [`crate::usdc`] writes it as a binary layer, [`crate::read`]
//! reads one back.

use std::fmt;

/// A path in a layer: `/`, a prim (`/Design/Arm_1`) or a prim's property
/// (`/Design/Arm_1/Body1.points`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Path {
    /// The prim names from the root down; empty for the pseudo-root.
    pub prims: Vec<String>,
    pub property: Option<String>,
}

impl Path {
    /// The pseudo-root, `/`.
    pub fn root() -> Self {
        Self::default()
    }

    /// A child prim of this prim path.
    pub fn child(&self, name: &str) -> Self {
        debug_assert!(self.property.is_none(), "a property has no children");
        let mut prims = self.prims.clone();
        prims.push(name.to_owned());
        Self {
            prims,
            property: None,
        }
    }

    /// A property of this prim path.
    pub fn property(&self, name: &str) -> Self {
        debug_assert!(self.property.is_none(), "a property has no properties");
        Self {
            prims: self.prims.clone(),
            property: Some(name.to_owned()),
        }
    }

    /// The parent: a property's prim, a prim's parent prim; None for the
    /// pseudo-root.
    pub fn parent(&self) -> Option<Self> {
        if self.property.is_some() {
            return Some(Self {
                prims: self.prims.clone(),
                property: None,
            });
        }
        let mut prims = self.prims.clone();
        prims.pop()?;
        Some(Self {
            prims,
            property: None,
        })
    }

    /// The last element: a property's name, a prim's name; None for the
    /// pseudo-root.
    pub fn name(&self) -> Option<&str> {
        self.property
            .as_deref()
            .or(self.prims.last().map(String::as_str))
    }

    /// Parses `/A/B` or `/A/B.prop` (no variants, no relative paths).
    pub fn parse(text: &str) -> Option<Self> {
        let rest = text.strip_prefix('/')?;
        let (prims, property) = match rest.split_once('.') {
            Some((prims, property)) => (prims, Some(property.to_owned())),
            None => (rest, None),
        };
        let prims: Vec<String> = if prims.is_empty() {
            Vec::new()
        } else {
            prims.split('/').map(str::to_owned).collect()
        };
        if prims.iter().any(String::is_empty) || (property.is_some() && prims.is_empty()) {
            return None;
        }
        Some(Self { prims, property })
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.prims.is_empty() {
            f.write_str("/")?;
        }
        for prim in &self.prims {
            write!(f, "/{prim}")?;
        }
        if let Some(property) = &self.property {
            write!(f, ".{property}")?;
        }
        Ok(())
    }
}

/// What a spec describes (`SdfSpecType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecType {
    Attribute = 1,
    Prim = 6,
    PseudoRoot = 7,
    Relationship = 8,
}

impl SpecType {
    pub fn from_code(code: u32) -> Option<Self> {
        Some(match code {
            1 => Self::Attribute,
            6 => Self::Prim,
            7 => Self::PseudoRoot,
            8 => Self::Relationship,
            _ => return None,
        })
    }
}

/// How a prim is specified (`SdfSpecifier`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Specifier {
    Def = 0,
    Over = 1,
    Class = 2,
}

/// Whether an attribute's value may vary in time (`SdfVariability`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variability {
    Varying = 0,
    Uniform = 1,
}

/// A list edit (`SdfListOp`): an explicit list, or items prepended to what
/// weaker layers have.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ListOp<T> {
    pub explicit: bool,
    pub explicit_items: Vec<T>,
    pub prepended_items: Vec<T>,
}

impl<T> ListOp<T> {
    pub fn explicit(items: Vec<T>) -> Self {
        Self {
            explicit: true,
            explicit_items: items,
            prepended_items: Vec::new(),
        }
    }

    pub fn prepended(items: Vec<T>) -> Self {
        Self {
            explicit: false,
            explicit_items: Vec::new(),
            prepended_items: items,
        }
    }
}

/// A field's value: the types this writer uses.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i32),
    Float(f32),
    Double(f64),
    Token(String),
    String(String),
    Asset(String),
    Specifier(Specifier),
    Variability(Variability),
    /// The names of a prim's children or properties.
    TokenVector(Vec<String>),
    TokenListOp(ListOp<String>),
    PathListOp(ListOp<Path>),
    Vec3f([f32; 3]),
    /// Row-major; points are row vectors (`p * M`), the translation is the
    /// last row.
    Matrix4d([[f64; 4]; 4]),
    IntArray(Vec<i32>),
    FloatArray(Vec<f32>),
    Vec2fArray(Vec<[f32; 2]>),
    Vec3fArray(Vec<[f32; 3]>),
    TokenArray(Vec<String>),
    /// A list of doubles (`std::vector<double>`): the times of time
    /// samples (mitcad#1597).
    DoubleVector(Vec<f64>),
    /// An attribute's values at time codes, the times increasing
    /// (`timeSamples`, mitcad#1597).
    TimeSamples(Vec<(f64, Value)>),
}

/// A spec: its path, what it is and its fields in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    pub path: Path,
    pub spec_type: SpecType,
    pub fields: Vec<(String, Value)>,
}

impl Spec {
    pub fn field(&self, name: &str) -> Option<&Value> {
        self.fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, value)| value)
    }
}
