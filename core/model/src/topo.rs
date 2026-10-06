// SPDX-License-Identifier: MIT
//! Topological naming (see `docs/architecture.md`): persistent names
//! of the faces, edges and vertices of bodies.
//!
//! Faces are named by how they were made, edges and vertices by the faces
//! that meet there:
//!
//! | Name | Form | Example |
//! |---|---|---|
//! | segment | `c<curve>[<start>,<end>]`, an end being `c<n>`, `c<n>+c<m>…` (several curves meet there, increasing) or `-` (free); `c<curve>` for a closed curve; `#k` for repeats | `c3[c2,c4]`, `c7[c2+c5,c4]` |
//! | region | `r{<outer boundary segments, sorted>}` | `r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}` |
//! | face | `<feature>:<role>[(<key>)][#k…]` | `F3:side(c1[c4,c2])`, `F3:end(r{c5})#1` |
//! | edge | `E{<face>\|<face>}[#k]`, faces sorted | `E{F3:end(r{c5})\|F3:side(c5)}` |
//! | vertex | `V{<face>\|<face>\|…}[#k]`, faces sorted | `V{F3:end(…)\|F3:side(…)\|F3:side(…)}` |
//!
//! - A segment is the piece of a sketch curve in a profile, named by the
//!   curves its two ends meet, in the curve's own direction, so the name
//!   does not depend on segment numbering or loop direction. `#k` numbers
//!   pieces of one curve between the same two curves, along the curve.
//! - Roles: extrusions name faces `side(<segment>)`, `start(<region>)` and
//!   `end(<region>)`; fillets and chamfers `fillet(<edge>)`, `chamfer(<edge>)`
//!   and `corner(<vertex>)`. New features add roles; the grammar takes any
//!   lowercase role with an optional key (a segment, region, edge, vertex,
//!   face or number).
//! - A face may carry several names (faces merged by a boolean). When an
//!   operation splits a face, every piece keeps the name with a suffix `#k`
//!   in a deterministic geometric order; a reference without the suffix
//!   means all pieces.
//! - Faces in edge and vertex names are sorted by their canonical text, byte
//!   by byte, which is also how the geometry library sorts them.

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use crate::ids::{EntityUid, FeatureUid, parse_number, string_serde};

/// A curve a segment lies on: a sketch curve `c<n>`, or a contour of a
/// text's glyph `t<n>.g<k>.c<j>` (the text, the character's index in it,
/// the contour's index in the glyph).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CurveId {
    Curve(EntityUid),
    Glyph {
        text: EntityUid,
        glyph: u32,
        contour: u32,
    },
}

impl CurveId {
    /// The sketch curve, for a curve.
    pub fn entity(self) -> Option<EntityUid> {
        match self {
            Self::Curve(uid) => Some(uid),
            Self::Glyph { .. } => None,
        }
    }

    /// The text, for a glyph contour.
    pub fn text(self) -> Option<EntityUid> {
        match self {
            Self::Glyph { text, .. } => Some(text),
            Self::Curve(_) => None,
        }
    }
}

impl From<EntityUid> for CurveId {
    fn from(uid: EntityUid) -> Self {
        Self::Curve(uid)
    }
}

impl fmt::Display for CurveId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Curve(uid) => write!(f, "c{}", uid.0),
            Self::Glyph {
                text,
                glyph,
                contour,
            } => write!(f, "t{}.g{glyph}.c{contour}", text.0),
        }
    }
}

/// The curves at one end of a segment, sorted: the other curves that meet
/// it there (`c2`, or `c2+c5` when several meet at the point). Empty for a
/// free end (`-`).
pub type SegmentEnd = Vec<CurveId>;

/// A piece of a sketch curve in a profile.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SegmentKey {
    pub curve: CurveId,
    /// The curves at the piece's start and end, in the curve's direction;
    /// None for a closed curve that is a loop by itself. (Boxed, so that
    /// names holding segment keys stay small.)
    pub ends: Option<Box<(SegmentEnd, SegmentEnd)>>,
    /// Numbers pieces of one curve between the same curves.
    pub occurrence: Option<u32>,
}

impl SegmentKey {
    pub fn closed(curve: impl Into<CurveId>) -> Self {
        Self {
            curve: curve.into(),
            ends: None,
            occurrence: None,
        }
    }

    /// The piece between one curve at its start and one at its end.
    pub fn between(
        curve: impl Into<CurveId>,
        start: impl Into<CurveId>,
        end: impl Into<CurveId>,
    ) -> Self {
        Self::with_ends(curve, vec![start.into()], vec![end.into()])
    }

    /// The piece between the curves at its ends (sorted and deduplicated
    /// here); an empty end is free.
    pub fn with_ends(
        curve: impl Into<CurveId>,
        mut start: SegmentEnd,
        mut end: SegmentEnd,
    ) -> Self {
        for end in [&mut start, &mut end] {
            end.sort_unstable();
            end.dedup();
        }
        Self {
            curve: curve.into(),
            ends: Some(Box::new((start, end))),
            occurrence: None,
        }
    }

    /// The curves this segment names: its own and those at its ends.
    pub fn curves(&self) -> impl Iterator<Item = CurveId> + '_ {
        std::iter::once(self.curve).chain(
            self.ends
                .iter()
                .flat_map(|ends| ends.0.iter().chain(&ends.1))
                .copied(),
        )
    }
}

/// A profile region: the segments of its outer boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RegionKey {
    segments: BTreeSet<SegmentKey>,
}

impl RegionKey {
    /// Fails for an empty boundary.
    pub fn new(segments: impl IntoIterator<Item = SegmentKey>) -> Option<Self> {
        let segments: BTreeSet<_> = segments.into_iter().collect();
        (!segments.is_empty()).then_some(Self { segments })
    }

    pub fn segments(&self) -> impl Iterator<Item = &SegmentKey> {
        self.segments.iter()
    }
}

/// What a face role refers to: `side(<segment>)`, `end(<region>)`,
/// `fillet(<edge>)`, ...
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RoleKey {
    Segment(SegmentKey),
    Region(RegionKey),
    Edge(Box<EdgeName>),
    Vertex(Box<VertexName>),
    Face(Box<FaceName>),
    Index(u32),
}

/// The name of a face.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FaceName {
    /// The feature that made the face.
    pub feature: FeatureUid,
    /// Lowercase letters, digits, `_` and `.`, starting with a letter.
    pub role: String,
    pub key: Option<RoleKey>,
    /// Piece numbers after splits, outermost first (`#1#0`).
    pub split: Vec<u32>,
}

impl FaceName {
    pub fn new(feature: FeatureUid, role: &str, key: Option<RoleKey>) -> Self {
        debug_assert!(valid_role(role), "invalid role {role}");
        Self {
            feature,
            role: role.to_owned(),
            key,
            split: Vec::new(),
        }
    }

    /// The side face an extrusion sweeps from a profile segment.
    pub fn side(feature: FeatureUid, segment: SegmentKey) -> Self {
        Self::new(feature, "side", Some(RoleKey::Segment(segment)))
    }

    /// The cap of an extrusion at its start, on the sketch plane for a
    /// one-sided extent.
    pub fn start(feature: FeatureUid, region: RegionKey) -> Self {
        Self::new(feature, "start", Some(RoleKey::Region(region)))
    }

    /// The cap of an extrusion at its far end.
    pub fn end(feature: FeatureUid, region: RegionKey) -> Self {
        Self::new(feature, "end", Some(RoleKey::Region(region)))
    }

    pub fn fillet(feature: FeatureUid, edge: EdgeName) -> Self {
        Self::new(feature, "fillet", Some(RoleKey::Edge(Box::new(edge))))
    }

    pub fn chamfer(feature: FeatureUid, edge: EdgeName) -> Self {
        Self::new(feature, "chamfer", Some(RoleKey::Edge(Box::new(edge))))
    }

    pub fn corner(feature: FeatureUid, vertex: VertexName) -> Self {
        Self::new(feature, "corner", Some(RoleKey::Vertex(Box::new(vertex))))
    }

    /// The piece `k` of this face after a split.
    pub fn piece(mut self, k: u32) -> Self {
        self.split.push(k);
        self
    }

    /// True when a face carrying this name is meant by `reference`: the
    /// same name, or a piece of it.
    pub fn matches(&self, reference: &FaceName) -> bool {
        self.feature == reference.feature
            && self.role == reference.role
            && self.key == reference.key
            && self.split.starts_with(&reference.split)
    }

    /// The features this name refers to, its own and those in its key.
    pub fn features(&self) -> BTreeSet<FeatureUid> {
        let mut features = BTreeSet::new();
        self.collect_features(&mut features);
        features
    }

    fn collect_features(&self, features: &mut BTreeSet<FeatureUid>) {
        features.insert(self.feature);
        match &self.key {
            Some(RoleKey::Edge(edge)) => edge.collect_features(features),
            Some(RoleKey::Vertex(vertex)) => vertex.collect_features(features),
            Some(RoleKey::Face(face)) => face.collect_features(features),
            _ => {}
        }
    }
}

/// The name of an edge: the two faces that meet there.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EdgeName {
    faces: [FaceName; 2],
    /// Numbers several edges between the same faces.
    pub index: Option<u32>,
}

impl EdgeName {
    /// The faces in either order; the name sorts them. A seam edge lies
    /// between a face and itself.
    pub fn new(a: FaceName, b: FaceName) -> Self {
        let faces = if a.to_string() <= b.to_string() {
            [a, b]
        } else {
            [b, a]
        };
        Self { faces, index: None }
    }

    pub fn with_index(mut self, index: u32) -> Self {
        self.index = Some(index);
        self
    }

    /// Sorted by canonical text.
    pub fn faces(&self) -> &[FaceName; 2] {
        &self.faces
    }

    pub fn features(&self) -> BTreeSet<FeatureUid> {
        let mut features = BTreeSet::new();
        self.collect_features(&mut features);
        features
    }

    fn collect_features(&self, features: &mut BTreeSet<FeatureUid>) {
        for face in &self.faces {
            face.collect_features(features);
        }
    }
}

/// The name of a vertex: the faces that meet there.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VertexName {
    faces: Vec<FaceName>,
    pub index: Option<u32>,
}

impl VertexName {
    /// None without faces. The name sorts the faces and drops repeats.
    pub fn new(faces: impl IntoIterator<Item = FaceName>) -> Option<Self> {
        let mut faces: Vec<(String, FaceName)> =
            faces.into_iter().map(|f| (f.to_string(), f)).collect();
        faces.sort_by(|a, b| a.0.cmp(&b.0));
        faces.dedup_by(|a, b| a.0 == b.0);
        (!faces.is_empty()).then(|| Self {
            faces: faces.into_iter().map(|(_, f)| f).collect(),
            index: None,
        })
    }

    pub fn with_index(mut self, index: u32) -> Self {
        self.index = Some(index);
        self
    }

    pub fn faces(&self) -> &[FaceName] {
        &self.faces
    }

    fn collect_features(&self, features: &mut BTreeSet<FeatureUid>) {
        for face in &self.faces {
            face.collect_features(features);
        }
    }
}

/// Any topological name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TopoName {
    Face(FaceName),
    Edge(EdgeName),
    Vertex(VertexName),
}

// Canonical text forms.

fn write_index(f: &mut fmt::Formatter<'_>, index: Option<u32>) -> fmt::Result {
    match index {
        Some(k) => write!(f, "#{k}"),
        None => Ok(()),
    }
}

fn write_end(f: &mut fmt::Formatter<'_>, end: &[CurveId]) -> fmt::Result {
    if end.is_empty() {
        return f.write_str("-");
    }
    for (i, curve) in end.iter().enumerate() {
        if i > 0 {
            f.write_str("+")?;
        }
        write!(f, "{curve}")?;
    }
    Ok(())
}

impl fmt::Display for SegmentKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.curve)?;
        if let Some(ends) = &self.ends {
            let (start, end) = &**ends;
            f.write_str("[")?;
            write_end(f, start)?;
            f.write_str(",")?;
            write_end(f, end)?;
            f.write_str("]")?;
        }
        write_index(f, self.occurrence)
    }
}

impl fmt::Display for RegionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("r{")?;
        for (i, segment) in self.segments.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            segment.fmt(f)?;
        }
        f.write_str("}")
    }
}

impl fmt::Display for RoleKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Segment(segment) => segment.fmt(f),
            Self::Region(region) => region.fmt(f),
            Self::Edge(edge) => edge.fmt(f),
            Self::Vertex(vertex) => vertex.fmt(f),
            Self::Face(face) => face.fmt(f),
            Self::Index(i) => write!(f, "{i}"),
        }
    }
}

impl fmt::Display for FaceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.feature, self.role)?;
        if let Some(key) = &self.key {
            write!(f, "({key})")?;
        }
        for k in &self.split {
            write!(f, "#{k}")?;
        }
        Ok(())
    }
}

impl fmt::Display for EdgeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "E{{{}|{}}}", self.faces[0], self.faces[1])?;
        write_index(f, self.index)
    }
}

impl fmt::Display for VertexName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("V{")?;
        for (i, face) in self.faces.iter().enumerate() {
            if i > 0 {
                f.write_str("|")?;
            }
            face.fmt(f)?;
        }
        f.write_str("}")?;
        write_index(f, self.index)
    }
}

impl fmt::Display for TopoName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Face(face) => face.fmt(f),
            Self::Edge(edge) => edge.fmt(f),
            Self::Vertex(vertex) => vertex.fmt(f),
        }
    }
}

// Names compare by canonical text, the order the geometry library uses.
macro_rules! text_order {
    ($type:ty) => {
        impl PartialOrd for $type {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for $type {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                self.to_string().cmp(&other.to_string())
            }
        }
    };
}

text_order!(FaceName);
text_order!(EdgeName);
text_order!(VertexName);
text_order!(TopoName);

// Parsing.

/// A malformed name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopoError {
    text: String,
    position: usize,
    expected: &'static str,
}

impl fmt::Display for TopoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid name '{}': expected {} at position {}",
            self.text, self.expected, self.position
        )
    }
}

impl std::error::Error for TopoError {}

struct Parser<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, pos: 0 }
    }

    fn error<T>(&self, expected: &'static str) -> Result<T, TopoError> {
        Err(TopoError {
            text: self.text.to_owned(),
            position: self.pos,
            expected,
        })
    }

    fn rest(&self) -> &'a str {
        &self.text[self.pos..]
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }

    fn eat(&mut self, token: &str) -> bool {
        if self.rest().starts_with(token) {
            self.pos += token.len();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, token: &str, expected: &'static str) -> Result<(), TopoError> {
        if self.eat(token) {
            Ok(())
        } else {
            self.error(expected)
        }
    }

    fn end(&self) -> Result<(), TopoError> {
        if self.pos == self.text.len() {
            Ok(())
        } else {
            self.error("the end of the name")
        }
    }

    /// A number in canonical form after `prefix`.
    fn number<T: FromStr>(&mut self, prefix: &str, expected: &'static str) -> Result<T, TopoError> {
        let digits = self.rest()[prefix.len().min(self.rest().len())..]
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
        let token = &self.rest()[..(prefix.len() + digits).min(self.rest().len())];
        match parse_number(token, prefix) {
            Some(value) => {
                self.pos += token.len();
                Ok(value)
            }
            None => self.error(expected),
        }
    }

    fn index(&mut self) -> Result<Option<u32>, TopoError> {
        if self.peek() == Some(b'#') {
            self.number("#", "a piece number after '#'").map(Some)
        } else {
            Ok(None)
        }
    }

    /// `c<n>`, or `t<n>.g<k>.c<j>` for a glyph contour.
    fn curve(&mut self) -> Result<CurveId, TopoError> {
        if self.peek() == Some(b't') {
            let text = EntityUid(self.number("t", "a text 't<number>'")?);
            let glyph = self.number(".g", "'.g<number>' after the text")?;
            let contour = self.number(".c", "'.c<number>' after the glyph")?;
            return Ok(CurveId::Glyph {
                text,
                glyph,
                contour,
            });
        }
        self.number(
            "c",
            "a curve 'c<number>' or 't<number>.g<number>.c<number>'",
        )
        .map(|n| CurveId::Curve(EntityUid(n)))
    }

    /// `-`, or curves joined by `+` in increasing order.
    fn segment_end(&mut self) -> Result<SegmentEnd, TopoError> {
        if self.eat("-") {
            return Ok(Vec::new());
        }
        let mut curves = vec![self.curve()?];
        while self.eat("+") {
            let curve = self.curve()?;
            if curves.last().is_some_and(|last| *last >= curve) {
                return self.error("curves in increasing order");
            }
            curves.push(curve);
        }
        Ok(curves)
    }

    fn segment(&mut self) -> Result<SegmentKey, TopoError> {
        let curve = self.curve()?;
        let ends = if self.eat("[") {
            let start = self.segment_end()?;
            self.expect(",", "','")?;
            let end = self.segment_end()?;
            self.expect("]", "']'")?;
            Some(Box::new((start, end)))
        } else {
            None
        };
        Ok(SegmentKey {
            curve,
            ends,
            occurrence: self.index()?,
        })
    }

    fn region(&mut self) -> Result<RegionKey, TopoError> {
        self.expect("r{", "'r{'")?;
        let mut segments = vec![self.segment()?];
        while self.eat(",") {
            segments.push(self.segment()?);
        }
        self.expect("}", "',' or '}'")?;
        Ok(RegionKey::new(segments).expect("at least one segment"))
    }

    fn face(&mut self) -> Result<FaceName, TopoError> {
        let feature = FeatureUid(self.number("F", "a feature 'F<number>'")?);
        self.expect(":", "':' after the feature")?;
        let start = self.pos;
        let length = self
            .rest()
            .bytes()
            .take_while(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_' || *b == b'.'
            })
            .count();
        let role = &self.text[start..start + length];
        if !valid_role(role) {
            return self.error("a role such as 'side'");
        }
        self.pos += length;
        let key = if self.eat("(") {
            let key = self.role_key()?;
            self.expect(")", "')'")?;
            Some(key)
        } else {
            None
        };
        let mut split = Vec::new();
        while let Some(k) = self.index()? {
            split.push(k);
        }
        Ok(FaceName {
            feature,
            role: role.to_owned(),
            key,
            split,
        })
    }

    fn role_key(&mut self) -> Result<RoleKey, TopoError> {
        let rest = self.rest();
        let second_is_digit = rest.as_bytes().get(1).is_some_and(u8::is_ascii_digit);
        if rest.starts_with("E{") {
            Ok(RoleKey::Edge(Box::new(self.edge()?)))
        } else if rest.starts_with("V{") {
            Ok(RoleKey::Vertex(Box::new(self.vertex()?)))
        } else if rest.starts_with("r{") {
            Ok(RoleKey::Region(self.region()?))
        } else if (rest.starts_with('c') || rest.starts_with('t')) && second_is_digit {
            Ok(RoleKey::Segment(self.segment()?))
        } else if rest.starts_with('F') && second_is_digit {
            Ok(RoleKey::Face(Box::new(self.face()?)))
        } else if rest.starts_with(|c: char| c.is_ascii_digit()) {
            Ok(RoleKey::Index(self.number("", "a number")?))
        } else {
            self.error("a segment, region, edge, vertex, face or number")
        }
    }

    fn edge(&mut self) -> Result<EdgeName, TopoError> {
        self.expect("E{", "'E{'")?;
        let a = self.face()?;
        self.expect("|", "'|'")?;
        let b = self.face()?;
        self.expect("}", "'}'")?;
        let mut edge = EdgeName::new(a, b);
        edge.index = self.index()?;
        Ok(edge)
    }

    fn vertex(&mut self) -> Result<VertexName, TopoError> {
        self.expect("V{", "'V{'")?;
        let mut faces = vec![self.face()?];
        while self.eat("|") {
            faces.push(self.face()?);
        }
        self.expect("}", "'|' or '}'")?;
        let mut vertex = VertexName::new(faces).expect("at least one face");
        vertex.index = self.index()?;
        Ok(vertex)
    }
}

fn valid_role(role: &str) -> bool {
    role.starts_with(|c: char| c.is_ascii_lowercase())
        && role
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.')
}

macro_rules! parse_with {
    ($type:ty, $method:ident) => {
        impl FromStr for $type {
            type Err = TopoError;

            fn from_str(text: &str) -> Result<Self, TopoError> {
                let mut parser = Parser::new(text);
                let value = parser.$method()?;
                parser.end()?;
                Ok(value)
            }
        }
    };
}

parse_with!(SegmentKey, segment);
parse_with!(RegionKey, region);
parse_with!(FaceName, face);
parse_with!(EdgeName, edge);
parse_with!(VertexName, vertex);

impl FromStr for TopoName {
    type Err = TopoError;

    fn from_str(text: &str) -> Result<Self, TopoError> {
        if text.starts_with("E{") {
            text.parse().map(Self::Edge)
        } else if text.starts_with("V{") {
            text.parse().map(Self::Vertex)
        } else {
            text.parse().map(Self::Face)
        }
    }
}

string_serde!(SegmentKey, "a segment key like \"c3[c2,c4]\"");
string_serde!(RegionKey, "a region key like \"r{c5}\"");
string_serde!(FaceName, "a face name like \"F3:side(c1[c4,c2])\"");
string_serde!(
    EdgeName,
    "an edge name like \"E{F3:end(r{c5})|F3:side(c5)}\""
);
string_serde!(VertexName, "a vertex name like \"V{F2:a|F2:b|F2:c}\"");
string_serde!(
    TopoName,
    "a face, edge or vertex name like \"E{F3:end(r{c5})|F3:side(c5)}\""
);

#[cfg(test)]
mod tests {
    use super::*;

    fn c(n: u32) -> EntityUid {
        EntityUid(n)
    }

    /// Line i of a rectangle with lines c1..c4.
    fn line(i: u32) -> SegmentKey {
        let k = |j: u32| c(1 + (j + 4) % 4);
        SegmentKey::between(k(i), k(i + 3), k(i + 1))
    }

    fn rectangle() -> RegionKey {
        RegionKey::new((0..4).map(line)).unwrap()
    }

    fn round_trip<T>(text: &str)
    where
        T: FromStr<Err = TopoError> + fmt::Display + fmt::Debug + PartialEq,
    {
        let value: T = text.parse().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(value.to_string(), text);
        assert_eq!(value.to_string().parse::<T>().unwrap(), value);
    }

    #[test]
    fn segments_and_regions() {
        assert_eq!(line(0).to_string(), "c1[c4,c2]");
        assert_eq!(SegmentKey::closed(c(5)).to_string(), "c5");
        assert_eq!(
            rectangle().to_string(),
            "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"
        );
        round_trip::<SegmentKey>("c2[c1,c1]#1");
        round_trip::<SegmentKey>("c7[c2+c5,-]#0");
        let several =
            SegmentKey::with_ends(c(7), vec![c(5).into(), c(2).into(), c(5).into()], vec![]);
        assert_eq!(several.to_string(), "c7[c2+c5,-]");
        assert_eq!(
            several.curves().collect::<Vec<_>>(),
            [c(7).into(), c(2).into(), c(5).into()]
        );
        assert!("c7[c5+c2,c1]".parse::<SegmentKey>().is_err());
        assert!("c7[c5+,c1]".parse::<SegmentKey>().is_err());
        // Regions sort their segments by curve number, not by text.
        let region: RegionKey = "r{c10,c9[c1,c2]}".parse().unwrap();
        assert_eq!(region.to_string(), "r{c9[c1,c2],c10}");
        assert!(RegionKey::new([]).is_none());
        // Text glyph contours, after the curves.
        round_trip::<SegmentKey>("t7.g0.c1");
        round_trip::<SegmentKey>("t7.g12.c0[c3,t7.g11.c0+t7.g12.c1]#2");
        let glyphs: RegionKey = "r{t7.g2.c0,c3[c1,c2],t7.g10.c0}".parse().unwrap();
        assert_eq!(glyphs.to_string(), "r{c3[c1,c2],t7.g2.c0,t7.g10.c0}");
        assert!("t7.c0".parse::<SegmentKey>().is_err());
        assert!("t7.g01.c0".parse::<SegmentKey>().is_err());
        let face: FaceName = "F4:side(t7.g2.c0)#1".parse().unwrap();
        assert_eq!(face.to_string(), "F4:side(t7.g2.c0)#1");
    }

    #[test]
    fn faces_edges_and_vertices_round_trip() {
        let f3 = FeatureUid(3);
        let side = FaceName::side(f3, line(1));
        assert_eq!(side.to_string(), "F3:side(c2[c1,c3])");
        let top = FaceName::end(f3, rectangle());
        let edge = EdgeName::new(side.clone(), top.clone());
        assert_eq!(
            edge.to_string(),
            "E{F3:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})|F3:side(c2[c1,c3])}"
        );
        assert_eq!(edge, EdgeName::new(top.clone(), side.clone()));
        let fillet = FaceName::fillet(FeatureUid(5), edge.clone().with_index(1));
        round_trip::<FaceName>(&fillet.to_string());
        round_trip::<FaceName>("F12:hole2.wall");
        round_trip::<FaceName>("F12:inst3(F2:side(c5))");
        round_trip::<FaceName>("F12:import(7)#0#2");
        round_trip::<EdgeName>("E{F2:side(c5)|F2:side(c5)}");
        round_trip::<VertexName>("V{F2:a|F2:b|F3:c}#4");
        let corner = VertexName::new([top.clone(), side.clone(), top.clone()]).unwrap();
        assert_eq!(corner.faces().len(), 2);
        round_trip::<FaceName>(&FaceName::corner(FeatureUid(9), corner).to_string());

        let parsed: TopoName = edge.to_string().parse().unwrap();
        assert_eq!(parsed, TopoName::Edge(edge.clone()));
        assert!(matches!("F3:top".parse(), Ok(TopoName::Face(_))));
        assert!(matches!("V{F3:top}".parse(), Ok(TopoName::Vertex(_))));
    }

    #[test]
    fn edge_faces_sort_by_text_like_the_geometry_library() {
        let a = FaceName::new(FeatureUid(10), "end", None);
        let b = FaceName::new(FeatureUid(3), "side", None);
        // "F10:end" < "F3:side" byte by byte.
        assert_eq!(
            EdgeName::new(b.clone(), a.clone()).to_string(),
            "E{F10:end|F3:side}"
        );
        assert!(a < b);
        let unsorted: EdgeName = "E{F3:side|F10:end}".parse().unwrap();
        assert_eq!(unsorted.to_string(), "E{F10:end|F3:side}");
    }

    #[test]
    fn references_match_split_pieces() {
        let top = FaceName::end(FeatureUid(2), rectangle());
        let piece = top.clone().piece(1);
        assert_eq!(piece.to_string(), format!("{top}#1"));
        assert!(piece.matches(&top));
        assert!(piece.clone().piece(0).matches(&piece));
        assert!(!top.matches(&piece));
        assert!(!piece.matches(&top.clone().piece(0)));
        assert!(!FaceName::start(FeatureUid(2), rectangle()).matches(&top));
    }

    #[test]
    fn names_list_their_features() {
        let edge: EdgeName = "E{F2:end(r{c5})|F7:fillet(E{F2:side(c5)|F4:side(c9)})}"
            .parse()
            .unwrap();
        let features: Vec<_> = edge.features().into_iter().collect();
        assert_eq!(features, vec![FeatureUid(2), FeatureUid(4), FeatureUid(7)]);
    }

    #[test]
    fn invalid_names_are_rejected_with_a_position() {
        for text in [
            "",
            "F3",
            "F3:",
            "F3:Side",
            "F3:side(",
            "F3:side()",
            "F3:side(c)",
            "F3:side(c1[c2])",
            "F3:side(c1[c2,c3)",
            "F3:end(r{})",
            "F3:end(r{c1,})",
            "F3:side(c01)",
            "F3:side(c1)#",
            "F3:side(c1) ",
            "F3:side(x)",
            "E{F3:a}",
            "E{F3:a|F3:b",
            "E{F3:a|F3:b|F3:c}",
            "V{}",
            "F3:side(c1)#99999999999",
        ] {
            assert!(text.parse::<TopoName>().is_err(), "accepted {text:?}");
        }
        assert_eq!(
            "F3:side(c1[c2])"
                .parse::<FaceName>()
                .unwrap_err()
                .to_string(),
            "invalid name 'F3:side(c1[c2])': expected ',' at position 13"
        );
        let error = serde_json::from_str::<EdgeName>("\"E{F3:a}\"").unwrap_err();
        assert!(error.to_string().contains("expected '|'"), "{error}");
    }
}
