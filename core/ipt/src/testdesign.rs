// SPDX-License-Identifier: MIT
//! A writer of definitions segment records as the files store them (see
//! `dc`, `params`, `sketch`, `features`), for tests here and of the
//! import: no third-party file is needed to test the decoding.

use crate::dc::{COLLECTION, DOCUMENT, LABEL};
use crate::design::{FEATURE, SKETCH, STATE_TABLE};
use crate::features::{BOOLEAN, BOUNDARY_PATCH, DIRECTION, ENUM_EXTENT, ENUM_OPERATION};
use crate::groups::{CIRCULAR, GROUP, MEMBER, RECTANGULAR};
use crate::params::{INTEGER, NUMBER, PARAMETER, UNIT};
use crate::sketch::{CIRCLE, LINE, POINT, TRANSFORM};

/// Base units (see `params`).
pub const LENGTH: [u8; 16] = crate::dc::type_id("bc204162d2119b0b60006ab760fec3b0");
pub const ANGLE: [u8; 16] = crate::dc::type_id("f0cd305cd2113f0d60006ab760fec3b0");
pub const METRE: [u8; 16] = crate::dc::type_id("f579a7f8d2118f09c0005a9a2378d04f");
pub const INCH: [u8; 16] = crate::dc::type_id("f679a7f8d2118f09c0005a9a2378d04f");
pub const DEGREE: [u8; 16] = crate::dc::type_id("f6cd305cd2113f0d60006ab760fec3b0");
pub const UNITLESS: [u8; 16] = crate::dc::type_id("23009d5fd2118e09c0005a9a2378d04f");
/// The extrusion's label class.
pub const EXTRUSION: [u8; 16] = crate::dc::type_id("3111a90cd0118b83000819b00524dc09");
pub const COINCIDENT: [u8; 16] = crate::dc::type_id("944d8790d011f8d10008cabc0663dc09");
pub const HORIZONTAL: [u8; 16] = crate::dc::type_id("984d8790d011f8d10008cabc0663dc09");
pub const VERTICAL: [u8; 16] = crate::dc::type_id("994d8790d011f8d10008cabc0663dc09");
pub const DISTANCE: [u8; 16] = crate::dc::type_id("58850511d211e39560000cb38932edb0");
pub const DIAMETER: [u8; 16] = crate::dc::type_id("e096df74d11169e0800066b1e13554c7");

/// A reference with the flag bit.
fn r(record: usize) -> [u8; 4] {
    ((record as u32 + 1) | 0x8000_0000).to_le_bytes()
}

fn list(kind: u16, items: &[usize]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&kind.to_le_bytes());
    b.extend_from_slice(&0x3000u16.to_le_bytes());
    b.extend_from_slice(&(items.len() as u32).to_le_bytes());
    if !items.is_empty() {
        b.extend_from_slice(&vec![0u8; if kind == 2 { 8 } else { 4 }]);
        for &i in items {
            b.extend_from_slice(&r(i));
        }
    }
    b
}

fn text(s: &str) -> Vec<u8> {
    let units: Vec<u16> = s.encode_utf16().collect();
    let mut b = (units.len() as u32).to_le_bytes().to_vec();
    for u in units {
        b.extend_from_slice(&u.to_le_bytes());
    }
    b
}

/// The records of a definitions segment being written.
pub struct DesignWriter {
    pub major: u8,
    pub records: Vec<([u8; 16], Vec<u8>)>,
    node: u32,
    /// The browser's top-level records.
    pub top: Vec<usize>,
    /// The part's parameter table.
    pub table: usize,
}

impl DesignWriter {
    /// A document record (record 0) naming the parameter table (record 1).
    pub fn new(major: u8) -> DesignWriter {
        let mut w = DesignWriter {
            major,
            records: Vec::new(),
            node: 100,
            top: Vec::new(),
            table: 1,
        };
        let mut doc = w.header(None);
        doc.extend_from_slice(&r(1));
        w.push(DOCUMENT, doc);
        let table = w.header(None);
        w.push(COLLECTION, table);
        w
    }

    /// The common header (with a fresh node number).
    pub fn header(&mut self, context: Option<usize>) -> Vec<u8> {
        self.node += 1;
        let mut b = vec![0u8; 4];
        b.extend_from_slice(&7u16.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&0x0003_4200u32.to_le_bytes());
        if self.major >= 28 {
            b.extend_from_slice(&0u32.to_le_bytes());
        }
        b.extend_from_slice(&context.map_or([0; 4], r));
        b.extend_from_slice(&self.node.to_le_bytes());
        b
    }

    /// The prefix after the header of entities, transforms, parameters and
    /// properties (`Definitions::prefix_len`): an i32 −1 and a release
    /// stamp from major 25, the −1 alone in majors 23 and 24, none before.
    pub fn prefix(&self) -> Vec<u8> {
        match self.major {
            0..=22 => Vec::new(),
            23 | 24 => vec![0xFF; 4],
            _ => vec![0xFF, 0xFF, 0xFF, 0xFF, 0x92, 0x0A, 0, 0],
        }
    }

    /// The node number of the last header written.
    pub fn last_node(&self) -> u32 {
        self.node
    }

    pub fn push(&mut self, t: [u8; 16], bytes: Vec<u8>) -> usize {
        self.records.push((t, bytes));
        self.records.len() - 1
    }

    /// A unit of one base unit.
    pub fn unit(&mut self, base: [u8; 16], magnitude: f64) -> usize {
        let mut b = vec![0u8; 6];
        b.extend_from_slice(&magnitude.to_le_bytes());
        b.extend_from_slice(&1f64.to_le_bytes());
        let base = self.push(base, b);
        let mut b = vec![0u8; 6];
        b.extend_from_slice(&list(3, &[base]));
        b.extend_from_slice(&list(3, &[]));
        b.extend_from_slice(&[1, 0, 0, 0, 0]);
        self.push(UNIT, b)
    }

    /// A number (internal units) shown in `unit`.
    pub fn number(&mut self, value: f64, unit: usize) -> usize {
        let mut b = vec![0u8; 6];
        b.extend_from_slice(&r(unit));
        b.extend_from_slice(&value.to_le_bytes());
        b.extend_from_slice(&[0; 6]);
        self.push(NUMBER, b)
    }

    /// An expression node: `code` 05 a parameter, 06–0b the operators, 0c
    /// negation.
    pub fn node(&mut self, code: u8, operands: &[usize]) -> usize {
        let mut t = NUMBER;
        t[0] = code;
        let mut b = vec![0u8; 10];
        for &o in operands {
            b.extend_from_slice(&r(o));
        }
        self.push(t, b)
    }

    /// A function node (`params::Function`): the display unit, the
    /// operands as a list and the function's code.
    pub fn call(&mut self, code: u32, unit: usize, operands: &[usize]) -> usize {
        let mut t = NUMBER;
        t[0] = 0x03;
        let mut b = vec![0u8; 6];
        b.extend_from_slice(&r(unit));
        b.extend_from_slice(&list(2, operands));
        b.extend_from_slice(&code.to_le_bytes());
        self.push(t, b)
    }

    /// A parameter of the part's table.
    pub fn parameter(&mut self, name: &str, unit: usize, formula: usize, value: f64) -> usize {
        let table = self.table;
        let mut b = self.header(Some(table));
        b.extend_from_slice(&self.prefix());
        b.extend_from_slice(&text(name));
        b.extend_from_slice(&[0; 4]);
        b.extend_from_slice(&r(unit));
        b.extend_from_slice(&r(formula));
        b.extend_from_slice(&value.to_le_bytes());
        b.extend_from_slice(&value.to_le_bytes());
        b.extend_from_slice(&[0, 0, 0xFF, 0xFF]);
        self.push(PARAMETER, b)
    }

    /// An integer parameter of the part's table.
    pub fn integer(&mut self, name: &str, value: u32) -> usize {
        let table = self.table;
        let mut b = self.header(Some(table));
        b.extend_from_slice(&self.prefix());
        b.extend_from_slice(&text(name));
        b.extend_from_slice(&[0; 4]);
        b.extend_from_slice(&value.to_le_bytes());
        b.extend_from_slice(&value.to_le_bytes());
        self.push(INTEGER, b)
    }

    /// A label of `owner` with the records shown under it.
    pub fn label(
        &mut self,
        owner: usize,
        name: &str,
        children: &[usize],
        class: [u8; 16],
    ) -> usize {
        let mut b = vec![0u8; 4];
        b.extend_from_slice(&7u16.to_le_bytes());
        b.extend_from_slice(&[0; 8]);
        if self.major >= 28 {
            b.extend_from_slice(&[0; 4]);
        }
        b.extend_from_slice(&r(owner));
        b.extend_from_slice(&[0; 12]);
        b.extend_from_slice(&list(2, children));
        b.extend_from_slice(&text(name));
        b.extend_from_slice(&class);
        self.push(LABEL, b)
    }

    /// A transform record (written with every element as an f64).
    pub fn transform(&mut self, m: [[f64; 4]; 4]) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&self.prefix());
        b.extend_from_slice(&0u32.to_le_bytes());
        for v in m.iter().flatten() {
            b.extend_from_slice(&v.to_le_bytes());
        }
        self.push(TRANSFORM, b)
    }

    /// An empty planar sketch; its entities are added with `sketch_*` and
    /// listed by [`DesignWriter::finish_sketch`].
    pub fn sketch(&mut self) -> usize {
        self.push(SKETCH, Vec::new())
    }

    fn entity(&mut self, t: [u8; 16], sketch: usize, flags: u32, rest: &[u8]) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&self.prefix());
        b.extend_from_slice(&flags.to_le_bytes());
        b.extend_from_slice(&r(sketch));
        b.extend_from_slice(rest);
        self.push(t, b)
    }

    pub fn sketch_point(&mut self, sketch: usize, at: [f64; 2]) -> usize {
        let mut rest = Vec::new();
        rest.extend_from_slice(&at[0].to_le_bytes());
        rest.extend_from_slice(&at[1].to_le_bytes());
        rest.extend_from_slice(&list(2, &[]));
        self.entity(POINT, sketch, 0, &rest)
    }

    pub fn sketch_line(&mut self, sketch: usize, start: usize, end: usize, flags: u32) -> usize {
        let mut rest = list(2, &[start, end]);
        rest.extend_from_slice(&[0; 32]);
        self.entity(LINE, sketch, flags, &rest)
    }

    /// A full circle (`ends` empty) or an arc (counter-clockwise from the
    /// first end to the second).
    pub fn sketch_circle(
        &mut self,
        sketch: usize,
        center: usize,
        radius: f64,
        ends: &[usize],
    ) -> usize {
        let mut rest = list(2, ends);
        rest.extend_from_slice(&[0; 8]);
        rest.extend_from_slice(&r(center));
        rest.extend_from_slice(&radius.to_le_bytes());
        rest.push(u8::from(ends.is_empty()));
        self.entity(CIRCLE, sketch, 0, &rest)
    }

    /// A straight segment between two points (no geometry of its own).
    pub fn sketch_segment(&mut self, sketch: usize, start: usize, end: usize) -> usize {
        let mut rest = list(2, &[start, end]);
        rest.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0]);
        self.entity(crate::sketch::SEGMENT, sketch, 0x40, &rest)
    }

    /// A full ellipse (`ends` empty) or an elliptical arc, with its major
    /// axis's direction and its radii.
    pub fn sketch_ellipse(
        &mut self,
        sketch: usize,
        center: usize,
        major_axis: [f64; 2],
        radii: [f64; 2],
        ends: &[usize],
    ) -> usize {
        let mut rest = list(2, ends);
        rest.extend_from_slice(&list(2, &[]));
        rest.extend_from_slice(&r(center));
        for v in [major_axis[0], major_axis[1], radii[0], radii[1]] {
            rest.extend_from_slice(&v.to_le_bytes());
        }
        rest.push(u8::from(ends.is_empty()));
        self.entity(crate::sketch::ELLIPSE, sketch, 0, &rest)
    }

    /// A spline (`sketch::SPLINE`) of `degree` from `start` to `end`
    /// through its control points (cm), clamped, with `flags`.
    pub fn sketch_spline(
        &mut self,
        sketch: usize,
        ends: [usize; 2],
        degree: u32,
        poles: &[[f64; 2]],
        flags: u32,
    ) -> usize {
        let mut rest = list(2, &ends);
        rest.extend_from_slice(&list(2, &[]));
        let np = poles.len();
        let nk = np + degree as usize + 1;
        let knots: Vec<f64> = (0..nk)
            .map(|i| {
                let inner = np - degree as usize;
                (i.saturating_sub(degree as usize).min(inner)) as f64 / inner as f64
            })
            .collect();
        // The NURBS data, twice.
        for _ in 0..2 {
            rest.extend_from_slice(&degree.to_le_bytes());
            rest.extend_from_slice(&1e-9f64.to_le_bytes());
            for n in [nk, nk + 3, 8] {
                rest.extend_from_slice(&(n as u32).to_le_bytes());
            }
            for k in &knots {
                rest.extend_from_slice(&k.to_le_bytes());
            }
            rest.extend_from_slice(&[0; 8]);
            for n in [8, np, np + 4, 8] {
                rest.extend_from_slice(&(n as u32).to_le_bytes());
            }
            for p in poles {
                rest.extend_from_slice(&p[0].to_le_bytes());
                rest.extend_from_slice(&p[1].to_le_bytes());
            }
        }
        self.entity(crate::sketch::SPLINE, sketch, flags, &rest)
    }

    /// A projected point (flag 0x40000).
    pub fn projected_point(&mut self, sketch: usize, at: [f64; 2]) -> usize {
        let p = self.sketch_point(sketch, at);
        let at = self.header_len_for_flags();
        self.records[p].1[at..at + 4].copy_from_slice(&0x4_0000u32.to_le_bytes());
        p
    }

    /// Where an entity's flags start.
    fn header_len_for_flags(&self) -> usize {
        (if self.major >= 28 { 26 } else { 22 }) + self.prefix().len()
    }

    /// A constraint or dimension: its parameter and the u32 after it.
    pub fn sketch_constraint(
        &mut self,
        t: [u8; 16],
        parameter: Option<usize>,
        entities: &[Option<usize>],
    ) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&(-1i32).to_le_bytes());
        b.extend_from_slice(&[0; 4]);
        for _ in 0..2 {
            b.extend_from_slice(&6u16.to_le_bytes());
            b.extend_from_slice(&0x3000u16.to_le_bytes());
            b.extend_from_slice(&0u32.to_le_bytes());
        }
        b.extend_from_slice(&parameter.map_or([0; 4], r));
        for e in entities {
            b.extend_from_slice(&e.map_or([0; 4], r));
        }
        self.push(t, b)
    }

    /// A sketch group (`groups`): its entity and pattern record.
    pub fn sketch_group(&mut self, sketch: usize, entity: usize, pattern: Option<usize>) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&self.prefix());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&r(sketch));
        b.extend_from_slice(&r(entity));
        b.extend_from_slice(&pattern.map_or([0; 4], r));
        self.push(GROUP, b)
    }

    /// A group member: two entities (adjacent sides, or a copy and its
    /// source) of the group with its entity and pattern record.
    pub fn group_member(
        &mut self,
        a: usize,
        b: usize,
        entity: usize,
        group: usize,
        pattern: Option<usize>,
    ) -> usize {
        self.sketch_constraint(
            MEMBER,
            None,
            &[
                Some(a),
                Some(b),
                Some(entity),
                Some(group),
                Some(group),
                None,
                pattern,
            ],
        )
    }

    /// A pattern record (`groups`): the map of entities to instances, the
    /// sketch and the references after it.
    pub fn sketch_pattern(
        &mut self,
        t: [u8; 16],
        sketch: usize,
        map: &[(usize, usize)],
        after: &[usize],
    ) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&(-1i32).to_le_bytes());
        b.extend_from_slice(&6u16.to_le_bytes());
        b.extend_from_slice(&0x3000u16.to_le_bytes());
        b.extend_from_slice(&(map.len() as u32).to_le_bytes());
        if !map.is_empty() {
            b.extend_from_slice(&[0; 8]);
        }
        for &(e, i) in map {
            b.extend_from_slice(&r(e));
            b.extend_from_slice(&r(i));
        }
        b.extend_from_slice(&r(sketch));
        b.extend_from_slice(&list(2, &[]));
        for &a in after {
            b.extend_from_slice(&r(a));
        }
        b.extend_from_slice(&std::f64::consts::TAU.to_le_bytes());
        self.push(t, b)
    }

    /// Writes the sketch record: its entities, transform and normal.
    pub fn finish_sketch(
        &mut self,
        sketch: usize,
        entities: &[usize],
        transform: usize,
        normal: usize,
    ) {
        let mut b = self.header(None);
        b.extend_from_slice(&2i32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&list(8, entities));
        b.extend_from_slice(&r(transform));
        b.extend_from_slice(&r(normal));
        b.extend_from_slice(&[0; 8]);
        b.extend_from_slice(&list(2, &[]));
        self.records[sketch].1 = b;
    }

    /// An enumeration property.
    pub fn enumeration(&mut self, t: [u8; 16], kind: u16, value: u16) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&self.prefix());
        b.extend_from_slice(&kind.to_le_bytes());
        b.extend_from_slice(&value.to_le_bytes());
        self.push(t, b)
    }

    pub fn boolean(&mut self, value: bool) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&self.prefix());
        b.extend_from_slice(&[0; 8]);
        b.push(u8::from(value));
        self.push(BOOLEAN, b)
    }

    pub fn direction(&mut self, v: [f64; 3]) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&self.prefix());
        b.extend_from_slice(&[0; 12]);
        for x in v {
            b.extend_from_slice(&x.to_le_bytes());
        }
        self.push(DIRECTION, b)
    }

    /// A list property (a boundary patch of profile selections).
    pub fn patch(&mut self, selections: &[usize]) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&self.prefix());
        b.extend_from_slice(&list(2, selections));
        self.push(BOUNDARY_PATCH, b)
    }

    /// A profile selection of body `body` of an ASM file (`profile`): the
    /// ASM record of the definitions segment, the entity link and the
    /// selection, which it returns.
    pub fn selection(&mut self, asm: &[u8], body: u32) -> usize {
        let file = self.push(crate::BREP_RECORD_TYPE, crate::testdata::brep_record(asm));
        let mut link = vec![0u8; 11];
        link.extend_from_slice(&r(file));
        link.extend_from_slice(&body.to_le_bytes());
        link.extend_from_slice(&[0x2B, 0, 0, 0]);
        let link = self.push(crate::profile::ENTITY_LINK, link);
        let mut b = self.header(None);
        b.extend_from_slice(&self.prefix());
        b.extend_from_slice(&r(link));
        b.push(0);
        self.push(crate::profile::SELECTION, b)
    }

    /// A feature record with its property slots; returns it and its node.
    pub fn feature(&mut self, slots: &[Option<usize>]) -> (usize, u32) {
        let mut b = self.header(None);
        let node = self.node;
        b.extend_from_slice(&2i32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&0x3000u16.to_le_bytes());
        b.extend_from_slice(&(slots.len() as u32).to_le_bytes());
        b.extend_from_slice(&[0; 8]);
        for s in slots {
            b.extend_from_slice(&s.map_or([0; 4], r));
        }
        b.extend_from_slice(&[0; 4]);
        (self.push(FEATURE, b), node)
    }

    /// An extrusion of a sketch's profile (operation 1 new body, 2 cut, 3
    /// join; extent 1 distance, 5 through all) along `direction`, with its
    /// profile selections.
    #[allow(clippy::too_many_arguments)]
    pub fn extrusion(
        &mut self,
        operation: u16,
        extent: u16,
        distance: usize,
        taper: usize,
        direction: [f64; 3],
        reversed: bool,
        selections: &[usize],
    ) -> (usize, u32) {
        let op = self.enumeration(ENUM_OPERATION, 5, operation);
        let patch = self.patch(selections);
        let dir = self.direction(direction);
        let rev = self.boolean(reversed);
        let ext = self.enumeration(ENUM_EXTENT, 12, extent);
        let sym = self.boolean(false);
        self.feature(&[
            Some(op),
            Some(patch),
            Some(dir),
            Some(rev),
            Some(distance),
            Some(taper),
            Some(ext),
            Some(sym),
        ])
    }

    /// The history state table: per feature node, the states before and
    /// after it.
    pub fn states(&mut self, entries: &[(u32, i64, i64)]) -> usize {
        self.states_marked(entries, 0x108, 2)
    }

    /// [`DesignWriter::states`] with the two words of the entries' mark
    /// that differ between parts.
    pub fn states_marked(&mut self, entries: &[(u32, i64, i64)], word: u16, byte: u8) -> usize {
        let mut b = self.header(None);
        let [w0, w1] = word.to_le_bytes();
        let mark = [
            0x02, 0x00, 0x00, 0x30, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, w0, w1, 0x00,
            0x00, byte, 0x00, 0x00, 0x00, 0x01,
        ];
        for k in 0..2 {
            for &(node, before, after) in entries {
                b.extend_from_slice(&(node | 0x8000_0000).to_le_bytes());
                b.extend_from_slice(&mark);
                let s = if k == 0 { before } else { after };
                b.extend_from_slice(&(s as i32).to_le_bytes());
                b.extend_from_slice(&[3, 0, 0, 0, 0, 0, 0, 0, 0]);
            }
        }
        self.push(STATE_TABLE, b)
    }

    /// The records, with the document's label listing the top-level ones.
    pub fn finish(mut self) -> Vec<([u8; 16], Vec<u8>)> {
        let top = std::mem::take(&mut self.top);
        self.label(0, "Part", &top, [0; 16]);
        self.records
    }
}

/// The design of the test part (`testdata::test_part_with_design`): a
/// user parameter `Side` = 10 mm, a sketch on the XY plane of a square of
/// that side (four points, four lines, horizontal and vertical
/// constraints, two distance dimensions `d0` = `Side` and `d1` = 10 mm) and
/// an extrusion of it by `d2` = 10 mm, a new body: the 10 mm cube; the
/// history state table names state 1 for the extrusion.
pub fn cube_design(major: u8) -> Vec<([u8; 16], Vec<u8>)> {
    cube_design_with(major, 0, 0, 1)
}

/// The cube's design with another extent of the extrusion (5 through all,
/// 4 up to the next face, ...).
pub fn cube_design_extent(major: u8, extent: u16) -> Vec<([u8; 16], Vec<u8>)> {
    cube_design_with(major, 0, 0, extent)
}

/// The cube's design with the square's lines flagged 0x40, a diagonal
/// flagged so too, and the extrusion's profile selected: the square sheet
/// of [`square_sheet`] (as segment majors up to 24 keep it).
pub fn flagged_cube_design(major: u8) -> Vec<([u8; 16], Vec<u8>)> {
    cube_design_with(major, 0x40, 1, 1)
}

/// The flagged cube's design whose extrusion selects the square twice
/// (as it does the regions of coincident curves).
pub fn twice_selected_cube_design(major: u8) -> Vec<([u8; 16], Vec<u8>)> {
    cube_design_with(major, 0x40, 2, 1)
}

fn cube_design_with(
    major: u8,
    flags: u32,
    selected: usize,
    extent: u16,
) -> Vec<([u8; 16], Vec<u8>)> {
    let mut w = DesignWriter::new(major);
    let length = w.unit(LENGTH, 1.0);
    let mm = w.unit(METRE, 1e-3);
    let n = w.number(1.0, mm);
    let side = w.parameter("Side", length, n, 1.0);
    let rs = w.node(0x05, &[side]);
    let d0 = w.parameter("d0", length, rs, 1.0);
    let n = w.number(1.0, mm);
    let d1 = w.parameter("d1", length, n, 1.0);
    let n = w.number(1.0, mm);
    let d2 = w.parameter("d2", length, n, 1.0);
    let angle = w.unit(ANGLE, 1.0);
    let deg = w.unit(DEGREE, 1.0);
    let n = w.number(0.0, deg);
    let taper = w.parameter("d3", angle, n, 0.0);

    let sketch = w.sketch();
    let identity = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let transform = w.transform(identity);
    let normal = w.direction([0.0, 0.0, 1.0]);
    let corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let points: Vec<usize> = corners.iter().map(|&c| w.sketch_point(sketch, c)).collect();
    let lines: Vec<usize> = (0..4)
        .map(|i| w.sketch_line(sketch, points[i], points[(i + 1) % 4], flags))
        .collect();
    let mut entities = points.clone();
    entities.extend(&lines);
    if selected > 0 {
        entities.push(w.sketch_line(sketch, points[0], points[2], flags));
    }
    entities.push(w.sketch_constraint(HORIZONTAL, None, &[Some(lines[0])]));
    entities.push(w.sketch_constraint(VERTICAL, None, &[Some(lines[1])]));
    entities.push(w.sketch_constraint(HORIZONTAL, None, &[Some(lines[2])]));
    entities.push(w.sketch_constraint(VERTICAL, None, &[Some(lines[3])]));
    entities.push(w.sketch_constraint(
        DISTANCE,
        Some(d0),
        &[None, Some(points[0]), Some(points[1])],
    ));
    entities.push(w.sketch_constraint(
        DISTANCE,
        Some(d1),
        &[None, Some(points[1]), Some(points[2])],
    ));
    w.finish_sketch(sketch, &entities, transform, normal);
    w.label(sketch, "Sketch1", &[], [0; 16]);

    let selections: Vec<usize> = (0..selected)
        .map(|_| w.selection(&square_sheet(), 4))
        .collect();
    let (extrusion, node) = w.extrusion(1, extent, d2, taper, [0.0, 0.0, 1.0], false, &selections);
    w.label(extrusion, "Extrusion1", &[sketch], EXTRUSION);
    w.top.push(extrusion);
    w.states(&[(node, 2, 1)]);
    w.finish()
}

/// A sketch on the XY plane of a 2 cm square from the origin and its
/// offset 2.5 mm inwards (`groups::offsets`): four offset records, one per
/// pair of neighbouring sides, and the distance `d0` = 2.5 mm between the
/// first side and its offset. The sketch is the only item.
pub fn offset_design(major: u8) -> Vec<([u8; 16], Vec<u8>)> {
    let mut w = DesignWriter::new(major);
    let length = w.unit(LENGTH, 1.0);
    let mm = w.unit(METRE, 1e-3);
    let n = w.number(0.25, mm);
    let d0 = w.parameter("d0", length, n, 0.25);
    let sketch = w.sketch();
    let identity = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let transform = w.transform(identity);
    let normal = w.direction([0.0, 0.0, 1.0]);
    let square = |w: &mut DesignWriter, a: f64, b: f64| -> (Vec<usize>, Vec<usize>) {
        let corners = [[a, a], [b, a], [b, b], [a, b]];
        let points: Vec<usize> = corners.iter().map(|&c| w.sketch_point(sketch, c)).collect();
        let sides = (0..4)
            .map(|i| w.sketch_line(sketch, points[i], points[(i + 1) % 4], 0))
            .collect();
        (points, sides)
    };
    let (outer_points, outer) = square(&mut w, 0.0, 2.0);
    let (inner_points, inner) = square(&mut w, 0.25, 1.75);
    let mut entities: Vec<usize> = outer_points.iter().chain(&inner_points).copied().collect();
    entities.extend(outer.iter().chain(&inner));
    for k in 0..4 {
        let next = (k + 1) % 4;
        entities.push(w.sketch_constraint(
            crate::groups::OFFSET,
            None,
            &[
                Some(outer[k]),
                Some(inner[k]),
                Some(outer[next]),
                Some(inner[next]),
            ],
        ));
    }
    entities.push(w.sketch_constraint(DISTANCE, Some(d0), &[None, Some(outer[0]), Some(inner[0])]));
    w.finish_sketch(sketch, &entities, transform, normal);
    w.label(sketch, "Sketch1", &[], [0; 16]);
    w.top.push(sketch);
    w.finish()
}

/// The revolution's label class.
pub const REVOLUTION: [u8; 16] = crate::dc::type_id("5f015400d211251c600067b79b49ebb0");

/// The cube's sketch (a 10 mm square from the origin on the XY plane)
/// revolved about the y axis (a work axis: its point and direction before
/// its last byte), with the revolution's extent and the angle parameter
/// `d0` (rad).
pub fn revolve_design(major: u8, extent: u16, angle: f64) -> Vec<([u8; 16], Vec<u8>)> {
    let mut w = DesignWriter::new(major);
    let angle_unit = w.unit(ANGLE, 1.0);
    let deg = w.unit(DEGREE, 1.0);
    let n = w.number(angle, deg);
    let d0 = w.parameter("d0", angle_unit, n, angle);
    let sketch = w.sketch();
    let identity = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let transform = w.transform(identity);
    let normal = w.direction([0.0, 0.0, 1.0]);
    let corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let points: Vec<usize> = corners.iter().map(|&c| w.sketch_point(sketch, c)).collect();
    let mut entities = points.clone();
    for i in 0..4 {
        entities.push(w.sketch_line(sketch, points[i], points[(i + 1) % 4], 0));
    }
    w.finish_sketch(sketch, &entities, transform, normal);
    w.label(sketch, "Sketch1", &[], [0; 16]);
    let mut axis = w.header(None);
    axis.extend_from_slice(&w.prefix());
    for v in [0.0f64, 0.0, 0.0, 0.0, 1.0, 0.0] {
        axis.extend_from_slice(&v.to_le_bytes());
    }
    axis.push(0);
    let axis = w.push(crate::features::WORK_AXIS, axis);
    let op = w.enumeration(ENUM_OPERATION, 5, 1);
    let patch = w.patch(&[]);
    let ext = w.enumeration(ENUM_EXTENT, 12, extent);
    let (revolution, node) = w.feature(&[Some(op), Some(patch), Some(axis), Some(ext), Some(d0)]);
    w.label(revolution, "Revolution1", &[sketch], REVOLUTION);
    w.top.push(revolution);
    w.states(&[(node, 2, 1)]);
    w.finish()
}

/// The revolution's sketch as a coil about the y axis (a cut), of coil
/// type `kind` (0 pitch and turns, 1 turns and height, 2 pitch and
/// height): pitch `d0` = 2 mm, height `d1` = 10 mm, turns `d2` = 5 and
/// taper `d3` = 0.
pub fn coil_design(major: u8, kind: u16) -> Vec<([u8; 16], Vec<u8>)> {
    let mut w = DesignWriter::new(major);
    let length = w.unit(LENGTH, 1.0);
    let mm = w.unit(METRE, 1e-3);
    let angle_unit = w.unit(ANGLE, 1.0);
    let deg = w.unit(DEGREE, 1.0);
    let unitless = w.unit(UNITLESS, 1.0);
    let n = w.number(2.0, mm);
    let d0 = w.parameter("d0", length, n, 0.2);
    let n = w.number(10.0, mm);
    let d1 = w.parameter("d1", length, n, 1.0);
    let n = w.number(5.0, unitless);
    let d2 = w.parameter("d2", unitless, n, 5.0);
    let n = w.number(0.0, deg);
    let d3 = w.parameter("d3", angle_unit, n, 0.0);
    let sketch = w.sketch();
    let identity = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let transform = w.transform(identity);
    let normal = w.direction([0.0, 0.0, 1.0]);
    let corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let points: Vec<usize> = corners.iter().map(|&c| w.sketch_point(sketch, c)).collect();
    let mut entities = points.clone();
    for i in 0..4 {
        entities.push(w.sketch_line(sketch, points[i], points[(i + 1) % 4], 0));
    }
    w.finish_sketch(sketch, &entities, transform, normal);
    w.label(sketch, "Sketch1", &[], [0; 16]);
    let mut axis = w.header(None);
    axis.extend_from_slice(&w.prefix());
    for v in [0.0f64, 0.0, 0.0, 0.0, 1.0, 0.0] {
        axis.extend_from_slice(&v.to_le_bytes());
    }
    axis.push(0);
    let axis = w.push(crate::features::WORK_AXIS, axis);
    let op = w.enumeration(ENUM_OPERATION, 5, 2);
    let patch = w.patch(&[]);
    let coil_type = w.enumeration(crate::features::COIL_TYPE, 4, kind);
    let (coil, node) = w.feature(&[
        Some(op),
        Some(patch),
        Some(axis),
        None,
        None,
        Some(coil_type),
        Some(d0),
        Some(d1),
        Some(d2),
        Some(d3),
    ]);
    w.label(coil, "Coil1", &[sketch], [1; 16]);
    w.top.push(coil);
    w.states(&[(node, 2, 1)]);
    w.finish()
}

/// A thread (`M6x1`, class `6g`, modelled or not) from (0, 0, 0) to
/// (0, 0, 1) cm: of full length, or of length `d0` = 8 mm at offset `d1` =
/// 1 mm.
pub fn thread_design(major: u8, full: bool, modeled: bool) -> Vec<([u8; 16], Vec<u8>)> {
    let mut w = DesignWriter::new(major);
    let length = w.unit(LENGTH, 1.0);
    let mm = w.unit(METRE, 1e-3);
    let n = w.number(8.0, mm);
    let d0 = w.parameter("d0", length, n, 0.8);
    let n = w.number(1.0, mm);
    let d1 = w.parameter("d1", length, n, 0.1);
    let full = w.boolean(full);
    let modeled = w.boolean(modeled);
    let mut ends = w.header(None);
    ends.extend_from_slice(&w.prefix());
    for v in [0.0f64, 0.0, 0.0, 0.0, 0.0, 1.0] {
        ends.extend_from_slice(&v.to_le_bytes());
    }
    let ends = w.push(crate::features::THREAD_ENDS, ends);
    let (thread, node) = w.feature(&[
        None,
        Some(full),
        None,
        Some(d0),
        Some(d1),
        None,
        Some(modeled),
        None,
        None,
        None,
        Some(ends),
    ]);
    let mut size = w.header(Some(thread));
    size.extend_from_slice(&0u32.to_le_bytes());
    for t in ["6", "M6x1", "ISO Metric profile", ""] {
        size.extend_from_slice(&text(t));
    }
    size.extend_from_slice(&0u32.to_le_bytes());
    for t in ["", "", "", "", "6g", "5.974", "5.794", "1"] {
        size.extend_from_slice(&text(t));
    }
    w.push(crate::features::THREAD_SIZE, size);
    w.label(thread, "Thread1", &[], [1; 16]);
    w.top.push(thread);
    w.states(&[(node, 2, 1)]);
    w.finish()
}

/// Two points on a horizontal and on a vertical line (`sketch`).
pub const HORIZONTAL_POINTS: [u8; 16] = crate::dc::type_id("c0a3558fd1115ee0800066b1e13554c7");
pub const VERTICAL_POINTS: [u8; 16] = crate::dc::type_id("5052da64d1115ee0800066b1e13554c7");

/// A sketch on the XY plane with the groups of `groups` (cm): a triangle
/// as a polygon around the origin (its centre fixed); a circle of radius
/// 0.2 at (6, 0) and
/// its copies at 120 and 240 degrees about (5, 0) (count `d0` = 3, angle
/// `d1` = 360 deg); a circle of radius 0.2 at (0, 5) copied 2 cm along a
/// line to the right and 1 cm against a line upwards (counts `d2`, `d3` =
/// 2, spacings `d4` = 20 mm, `d5` = 10 mm), the lines' ends constrained
/// on a horizontal and a vertical line; an arc about (10, 0) of
/// radius 1 from (11, 0) to (10, 1) with a point on it listed after its
/// ends. The sketch is the only item of the timeline.
pub fn groups_design(major: u8) -> Vec<([u8; 16], Vec<u8>)> {
    let mut w = DesignWriter::new(major);
    let length = w.unit(LENGTH, 1.0);
    let mm = w.unit(METRE, 1e-3);
    let angle = w.unit(ANGLE, 1.0);
    let deg = w.unit(DEGREE, 1.0);
    let count = w.integer("d0", 3);
    let n = w.number(std::f64::consts::TAU, deg);
    let total = w.parameter("d1", angle, n, std::f64::consts::TAU);
    let count1 = w.integer("d2", 2);
    let count2 = w.integer("d3", 2);
    let n = w.number(2.0, mm);
    let spacing1 = w.parameter("d4", length, n, 2.0);
    let n = w.number(1.0, mm);
    let spacing2 = w.parameter("d5", length, n, 1.0);

    let sketch = w.sketch();
    let identity = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let transform = w.transform(identity);
    let normal = w.direction([0.0, 0.0, 1.0]);
    let mut entities = Vec::new();
    // The triangle.
    let center = w.sketch_point(sketch, [0.0, 0.0]);
    let corners: Vec<usize> = [90.0f64, 210.0, 330.0]
        .iter()
        .map(|a| {
            let a = a.to_radians();
            w.sketch_point(sketch, [a.cos(), a.sin()])
        })
        .collect();
    let sides: Vec<usize> = (0..3)
        .map(|i| w.sketch_line(sketch, corners[i], corners[(i + 1) % 3], 0))
        .collect();
    entities.push(center);
    entities.extend(&corners);
    entities.extend(&sides);
    entities.push(w.sketch_constraint(crate::sketch::FIX, None, &[Some(center)]));
    let polygon = w.sketch_group(sketch, center, None);
    entities.push(polygon);
    for i in 0..3 {
        let m = w.group_member(sides[i], sides[(i + 1) % 3], center, polygon, None);
        entities.push(m);
        let m = w.group_member(corners[i], corners[(i + 1) % 3], center, polygon, None);
        entities.push(m);
    }
    // The circular pattern: each copy made from the one before.
    let axis = w.sketch_point(sketch, [5.0, 0.0]);
    let centers: Vec<usize> = [0.0f64, 120.0, 240.0]
        .iter()
        .map(|a| {
            let a = a.to_radians();
            w.sketch_point(sketch, [5.0 + a.cos(), a.sin()])
        })
        .collect();
    let circles: Vec<usize> = centers
        .iter()
        .map(|&c| w.sketch_circle(sketch, c, 0.2, &[]))
        .collect();
    entities.push(axis);
    entities.extend(&centers);
    entities.extend(&circles);
    let group = w.records.len();
    let pattern = group + 1;
    w.sketch_group(sketch, axis, Some(pattern));
    let map: Vec<(usize, usize)> = circles.iter().map(|&c| (c, group)).collect();
    w.sketch_pattern(CIRCULAR, sketch, &map, &[axis, total, group, count]);
    entities.push(group);
    for i in 1..3 {
        let m = w.group_member(circles[i], circles[i - 1], axis, group, Some(pattern));
        entities.push(m);
        let m = w.group_member(centers[i], centers[i - 1], axis, group, Some(pattern));
        entities.push(m);
    }
    // The rectangular pattern, along its two direction lines.
    let ends: Vec<usize> = [[0.0, 6.0], [1.0, 6.0], [-1.0, 5.0], [-1.0, 6.0]]
        .iter()
        .map(|&p| w.sketch_point(sketch, p))
        .collect();
    let right = w.sketch_line(sketch, ends[0], ends[1], 0x40);
    let up = w.sketch_line(sketch, ends[2], ends[3], 0x40);
    let places: Vec<usize> = [[0.0, 5.0], [2.0, 5.0], [0.0, 4.0], [2.0, 4.0]]
        .iter()
        .map(|&p| w.sketch_point(sketch, p))
        .collect();
    let rounds: Vec<usize> = places
        .iter()
        .map(|&c| w.sketch_circle(sketch, c, 0.2, &[]))
        .collect();
    entities.extend(&ends);
    entities.extend([right, up]);
    entities.extend(&places);
    entities.extend(&rounds);
    let first = w.records.len();
    let (second, pattern) = (first + 1, first + 2);
    w.sketch_group(sketch, right, Some(pattern));
    w.sketch_group(sketch, up, Some(pattern));
    let map: Vec<(usize, usize)> = rounds.iter().map(|&c| (c, first)).collect();
    w.sketch_pattern(
        RECTANGULAR,
        sketch,
        &map,
        &[right, up, spacing1, spacing2, first, second, count1, count2],
    );
    entities.extend([first, second]);
    for (copy, source, g, line) in [(1, 0, first, right), (2, 0, second, up), (3, 1, second, up)] {
        let m = w.group_member(rounds[copy], rounds[source], line, g, Some(pattern));
        entities.push(m);
        let m = w.group_member(places[copy], places[source], line, g, Some(pattern));
        entities.push(m);
    }
    entities.push(w.sketch_constraint(HORIZONTAL_POINTS, None, &[Some(ends[0]), Some(ends[1])]));
    entities.push(w.sketch_constraint(VERTICAL_POINTS, None, &[Some(ends[2]), Some(ends[3])]));
    // The arc with a point on it.
    let arc: Vec<usize> = [
        [10.0, 0.0],
        [11.0, 0.0],
        [10.0, 1.0],
        [10.0 + 0.5f64.sqrt(), 0.5f64.sqrt()],
    ]
    .iter()
    .map(|&p| w.sketch_point(sketch, p))
    .collect();
    let a = w.sketch_circle(sketch, arc[0], 1.0, &arc[1..]);
    entities.extend(&arc);
    entities.push(a);
    w.finish_sketch(sketch, &entities, transform, normal);
    w.label(sketch, "Sketch1", &[], [0; 16]);
    w.top.push(sketch);
    w.finish()
}

/// An ASM file with a sheet body of id 4: one square face of side 1 cm
/// in the XY plane, from the origin (records: header, body, lump,
/// shell, face, loop, coedges 6–9, edges 10–13, vertices 14–17, points
/// 18–21, curves 22–25, the plane 26).
pub fn square_sheet() -> Vec<u8> {
    square_sheet_at(0.0)
}

/// The square sheet of [`square_sheet`] at the height `z` (cm).
pub fn square_sheet_at(z: f64) -> Vec<u8> {
    use mitcad_f3d::asm::writer::Writer;
    let corners = [[0.0, 0.0, z], [1.0, 0.0, z], [1.0, 1.0, z], [0.0, 1.0, z]];
    let mut w = Writer::new(2, 2);
    w.record("asmheader")
        .ptr(-1)
        .int(-1)
        .str("225.5.0.65535")
        .end();
    w.record("body")
        .ptr(-1)
        .int(4)
        .ptr(-1)
        .ptr(2)
        .ptr(-1)
        .ptr(-1)
        .end();
    w.record("lump").head().ptr(-1).ptr(3).ptr(1).end();
    w.record("shell")
        .head()
        .ptr(-1)
        .ptr(-1)
        .ptr(4)
        .ptr(-1)
        .ptr(2)
        .end();
    w.record("face")
        .head()
        .ptr(-1)
        .ptr(5)
        .ptr(3)
        .ptr(-1)
        .ptr(26)
        .bool(false)
        .bool(false)
        .end();
    w.record("loop").head().ptr(-1).ptr(6).ptr(4).end();
    for k in 0..4 {
        w.record("coedge")
            .head()
            .ptr(6 + (k + 1) % 4)
            .ptr(6 + (k + 3) % 4)
            .ptr(-1)
            .ptr(10 + k)
            .bool(false)
            .ptr(5)
            .int(0)
            .ptr(-1)
            .end();
    }
    for k in 0..4 {
        w.record("edge")
            .head()
            .ptr(14 + k)
            .dbl(0.0)
            .ptr(14 + (k + 1) % 4)
            .dbl(1.0)
            .ptr(6 + k)
            .ptr(22 + k)
            .bool(false)
            .str("unknown")
            .end();
    }
    for k in 0..4 {
        w.record("vertex")
            .head()
            .ptr(10 + k)
            .int(0)
            .ptr(18 + k)
            .end();
    }
    for p in corners {
        w.record("point").head().pos(p).end();
    }
    for k in 0..4 {
        let (a, b) = (corners[k], corners[(k + 1) % 4]);
        w.record("straight-curve")
            .head()
            .pos(a)
            .vec([b[0] - a[0], b[1] - a[1], b[2] - a[2]])
            .bool(false)
            .bool(false)
            .end();
    }
    w.record("plane-surface")
        .head()
        .pos([0.0, 0.0, z])
        .vec([0.0, 0.0, 1.0])
        .vec([1.0, 0.0, 0.0])
        .bool(false)
        .bool(false)
        .bool(false)
        .bool(false)
        .bool(false)
        .end();
    w.finish()
}

/// Parameters with functions (`params::Function`): `SW` = 2 mm, `d0` =
/// `SW / cos(30 deg)`, `d1` = the units function of `SW / 2`, 1 mm and
/// 1 deg (`SW / 2`); `d2`, unitless, whose expression 0 does not give its
/// stored value 0.785 and which nothing uses; `d3` (expression 1 mm, stored
/// 2 mm), the distance of a sketch's two points 2 mm apart, the only item.
pub fn function_design(major: u8) -> Vec<([u8; 16], Vec<u8>)> {
    let mut w = DesignWriter::new(major);
    let length = w.unit(LENGTH, 1.0);
    let mm = w.unit(METRE, 1e-3);
    let deg = w.unit(DEGREE, 1.0);
    let none = w.unit(UNITLESS, 1.0);
    let n = w.number(0.2, mm);
    let sw = w.parameter("SW", length, n, 0.2);
    let p = w.node(0x05, &[sw]);
    let thirty = w.number(std::f64::consts::PI / 6.0, deg);
    let cos = w.call(1, none, &[thirty]);
    let quotient = w.node(0x09, &[p, cos]);
    w.parameter("d0", length, quotient, 0.2 / 0.75f64.sqrt());
    let p = w.node(0x05, &[sw]);
    let two = w.number(2.0, none);
    let half = w.node(0x09, &[p, two]);
    let one_mm = w.number(0.1, mm);
    let one_deg = w.number(std::f64::consts::PI / 180.0, deg);
    let units = w.call(26, mm, &[half, one_mm, one_deg]);
    w.parameter("d1", length, units, 0.1);
    let zero = w.number(0.0, none);
    w.parameter("d2", none, zero, std::f64::consts::FRAC_PI_4);
    let n = w.number(0.1, mm);
    let d3 = w.parameter("d3", length, n, 0.2);
    let sketch = w.sketch();
    let identity = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let transform = w.transform(identity);
    let normal = w.direction([0.0, 0.0, 1.0]);
    let a = w.sketch_point(sketch, [0.0, 0.0]);
    let b = w.sketch_point(sketch, [0.2, 0.0]);
    let dim = w.sketch_constraint(DISTANCE, Some(d3), &[None, Some(a), Some(b)]);
    w.finish_sketch(sketch, &[a, b, dim], transform, normal);
    w.label(sketch, "Sketch1", &[], [0; 16]);
    w.top.push(sketch);
    w.finish()
}

pub const PERPENDICULAR: [u8; 16] = crate::dc::type_id("964d8790d011f8d10008cabc0663dc09");

/// A sketch on the XY plane (cm) of: an arc about the origin of radius 1
/// from (1, 0) to (0, 1) and a line from (1, 0) to (2, 0) perpendicular to
/// it, stored arc first; a line from (3, 3) to that point again, horizontal;
/// a circle of radius 0.5 about (5, 0) patterned three times about its own
/// centre (count `d0` = 3, angle `d1` = 360 deg): each copy and its centre
/// on the original. The sketch is the only item.
pub fn sketch_fixes_design(major: u8) -> Vec<([u8; 16], Vec<u8>)> {
    let mut w = DesignWriter::new(major);
    let angle = w.unit(ANGLE, 1.0);
    let deg = w.unit(DEGREE, 1.0);
    let count = w.integer("d0", 3);
    let n = w.number(std::f64::consts::TAU, deg);
    let total = w.parameter("d1", angle, n, std::f64::consts::TAU);
    let sketch = w.sketch();
    let identity = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let transform = w.transform(identity);
    let normal = w.direction([0.0, 0.0, 1.0]);
    let mut entities = Vec::new();
    let pts: Vec<usize> = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [2.0, 0.0], [3.0, 3.0]]
        .iter()
        .map(|&p| w.sketch_point(sketch, p))
        .collect();
    entities.extend(&pts);
    let arc = w.sketch_circle(sketch, pts[0], 1.0, &[pts[1], pts[2]]);
    let radial = w.sketch_line(sketch, pts[1], pts[3], 0);
    let nothing = w.sketch_line(sketch, pts[4], pts[4], 0);
    entities.extend([arc, radial, nothing]);
    entities.push(w.sketch_constraint(PERPENDICULAR, None, &[Some(arc), Some(radial)]));
    entities.push(w.sketch_constraint(HORIZONTAL, None, &[Some(nothing)]));
    // The circle and its copies, all about (5, 0).
    let centres: Vec<usize> = (0..3).map(|_| w.sketch_point(sketch, [5.0, 0.0])).collect();
    let circles: Vec<usize> = centres
        .iter()
        .map(|&c| w.sketch_circle(sketch, c, 0.5, &[]))
        .collect();
    entities.extend(&centres);
    entities.extend(&circles);
    let group = w.records.len();
    let pattern = group + 1;
    w.sketch_group(sketch, centres[0], Some(pattern));
    let map: Vec<(usize, usize)> = circles.iter().map(|&c| (c, group)).collect();
    w.sketch_pattern(CIRCULAR, sketch, &map, &[centres[0], total, group, count]);
    entities.push(group);
    for i in 1..3 {
        let m = w.group_member(circles[i], circles[i - 1], centres[0], group, Some(pattern));
        entities.push(m);
        let m = w.group_member(centres[i], centres[i - 1], centres[0], group, Some(pattern));
        entities.push(m);
    }
    // A centre line from (6, 0) to (8, 0) and the point (7, 1) at `d2` =
    // 20 mm from it, a diameter about it.
    let length = w.unit(LENGTH, 1.0);
    let mm = w.unit(METRE, 1e-3);
    let n = w.number(2.0, mm);
    let d2 = w.parameter("d2", length, n, 2.0);
    let ends: Vec<usize> = [[6.0, 0.0], [8.0, 0.0], [7.0, 1.0]]
        .iter()
        .map(|&p| w.sketch_point(sketch, p))
        .collect();
    let axis = w.sketch_line(sketch, ends[0], ends[1], 0x8_0000);
    entities.extend(&ends);
    entities.push(axis);
    entities.push(w.sketch_constraint(DISTANCE, Some(d2), &[None, Some(ends[2]), Some(axis)]));
    w.finish_sketch(sketch, &entities, transform, normal);
    w.label(sketch, "Sketch1", &[], [0; 16]);
    w.top.push(sketch);
    w.finish()
}

/// Work planes through work points and axes (cm): z = 1 through three
/// points; x = 1 through two axes along z and y; through the z axis and
/// (1, 1, 0); through (0, 0, 3) normal to the z axis.
pub fn planes_design(major: u8) -> Vec<([u8; 16], Vec<u8>)> {
    use crate::features::{PLANE_POINT_AXIS, PLANE_THREE_POINTS, PLANE_TWO_AXES};
    let mut w = DesignWriter::new(major);
    let plane = |w: &mut DesignWriter, name: &str, v: [[f64; 3]; 3]| {
        let p = w.work_plane(v[0], v[1], v[2]);
        w.label(p, name, &[], [0; 16]);
        w.top.push(p);
        p
    };
    let (x, y, z) = ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
    let three = plane(&mut w, "Work Plane1", [[0.0, 0.0, 1.0], x, y]);
    let points = [[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [0.0, 1.0, 1.0]].map(|p| w.work_point(p));
    w.plane_definition(PLANE_THREE_POINTS, three, &points);
    let two = plane(&mut w, "Work Plane2", [[1.0, 0.0, 0.0], y, z]);
    let a1 = w.work_axis([1.0, 0.0, 0.0], z);
    let b = w.work_axis([1.0, 2.0, 0.0], y);
    w.plane_definition(PLANE_TWO_AXES, two, &[a1, b]);
    let a = w.work_axis([0.0; 3], z);
    let diagonal = [0.5f64.sqrt(), 0.5f64.sqrt(), 0.0];
    let along = plane(&mut w, "Work Plane3", [[0.0; 3], diagonal, z]);
    let p = w.work_point([1.0, 1.0, 0.0]);
    w.plane_definition(PLANE_POINT_AXIS, along, &[p, a]);
    let normal = plane(&mut w, "Work Plane4", [[0.0, 0.0, 3.0], x, y]);
    let p = w.work_point([0.0, 0.0, 3.0]);
    w.plane_definition(PLANE_POINT_AXIS, normal, &[p, a]);
    w.finish()
}

/// The loft's label class.
pub const LOFT: [u8; 16] = crate::dc::type_id("e0ef542fd111340900084eba32a3dc09");

/// Two sketches of a 1 cm square from the origin, on the XY plane and 1
/// cm above it, and a loft (a new body) between them: its sections the
/// squares' sheets (`profile::LOFT_SECTION`).
pub fn loft_design(major: u8) -> Vec<([u8; 16], Vec<u8>)> {
    let mut w = DesignWriter::new(major);
    let mut sketches = Vec::new();
    let mut sections = Vec::new();
    for (k, z) in [0.0, 1.0].into_iter().enumerate() {
        let sketch = w.sketch();
        let m = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, z],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let transform = w.transform(m);
        let normal = w.direction([0.0, 0.0, 1.0]);
        let corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let points: Vec<usize> = corners.iter().map(|&c| w.sketch_point(sketch, c)).collect();
        let mut entities = points.clone();
        for i in 0..4 {
            entities.push(w.sketch_line(sketch, points[i], points[(i + 1) % 4], 0));
        }
        w.finish_sketch(sketch, &entities, transform, normal);
        w.label(sketch, &format!("Sketch{}", k + 1), &[], [0; 16]);
        sketches.push(sketch);
        // The section: as a profile selection, of another type.
        let selection = w.selection(&square_sheet_at(z), 4);
        w.records[selection].0 = crate::profile::LOFT_SECTION;
        sections.push(selection);
    }
    let list = w.list_of(crate::features::LOFT_SECTIONS, &sections);
    let op = w.enumeration(ENUM_OPERATION, 5, 1);
    let (loft, node) = w.feature(&[Some(list), Some(op)]);
    w.label(loft, "Loft1", &sketches, LOFT);
    w.top.push(loft);
    w.states(&[(node, 2, 1)]);
    w.finish()
}

/// The label classes of combines, shells and splits.
pub const COMBINE: [u8; 16] = crate::dc::type_id("336723c07e40eacb2eff669e9add87f8");
pub const SHELL: [u8; 16] = crate::dc::type_id("12963cb8d1118a2860008bb801f31bb0");
pub const SPLIT: [u8; 16] = crate::dc::type_id("baa87183d311347bc000e39545df724f");

impl DesignWriter {
    /// A solid body (`features::BODY`) and its label.
    pub fn body(&mut self, name: &str, number: u32) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&self.prefix());
        b.extend_from_slice(&number.to_le_bytes());
        let body = self.push(crate::features::BODY, b);
        self.label(body, name, &[], [0; 16]);
        body
    }

    /// A list of records of type `t` (bodies, faces).
    pub fn list_of(&mut self, t: [u8; 16], items: &[usize]) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&self.prefix());
        b.extend_from_slice(&list(2, items));
        self.push(t, b)
    }

    /// A work point at `at` (cm): after the prefix 12 bytes, its position
    /// and two empty lists.
    pub fn work_point(&mut self, at: [f64; 3]) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&self.prefix());
        b.extend_from_slice(&[0; 12]);
        for v in at {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&list(2, &[]));
        b.extend_from_slice(&list(2, &[]));
        self.push(crate::features::WORK_POINT, b)
    }

    /// A work axis through `at` along `direction` (cm): its point and
    /// direction before its last byte.
    pub fn work_axis(&mut self, at: [f64; 3], direction: [f64; 3]) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&self.prefix());
        for v in at.iter().chain(&direction) {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(0);
        self.push(crate::features::WORK_AXIS, b)
    }

    /// A work plane's definition of type `t`: the header, an i32 −1, the
    /// plane and the records it is defined by.
    pub fn plane_definition(&mut self, t: [u8; 16], plane: usize, refs: &[usize]) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&(-1i32).to_le_bytes());
        b.extend_from_slice(&r(plane));
        for &x in refs {
            b.extend_from_slice(&r(x));
        }
        self.push(t, b)
    }

    /// A work plane through `origin` with the axes `x` and `y` (its last
    /// nine f64).
    pub fn work_plane(&mut self, origin: [f64; 3], x: [f64; 3], y: [f64; 3]) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&self.prefix());
        b.extend_from_slice(&[0; 8]);
        for v in origin.iter().chain(&x).chain(&y) {
            b.extend_from_slice(&v.to_le_bytes());
        }
        self.push(crate::features::WORK_PLANE, b)
    }

    /// A sketch on the XY plane of a 1 cm square from `(x0, 0)`, its
    /// label, and an extrusion of it by `distance`, a new body that lists
    /// `body` among its slots (as the files' features do, at slot 26).
    fn cube_at(
        &mut self,
        k: usize,
        x0: f64,
        distance: usize,
        taper: usize,
        body: usize,
    ) -> (usize, u32) {
        let sketch = self.sketch();
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let transform = self.transform(identity);
        let normal = self.direction([0.0, 0.0, 1.0]);
        let corners = [[x0, 0.0], [x0 + 1.0, 0.0], [x0 + 1.0, 1.0], [x0, 1.0]];
        let points: Vec<usize> = corners
            .iter()
            .map(|&c| self.sketch_point(sketch, c))
            .collect();
        let mut entities = points.clone();
        for i in 0..4 {
            entities.push(self.sketch_line(sketch, points[i], points[(i + 1) % 4], 0));
        }
        self.finish_sketch(sketch, &entities, transform, normal);
        self.label(sketch, &format!("Sketch{k}"), &[], [0; 16]);
        let op = self.enumeration(ENUM_OPERATION, 5, 1);
        let patch = self.patch(&[]);
        let dir = self.direction([0.0, 0.0, 1.0]);
        let rev = self.boolean(false);
        let ext = self.enumeration(ENUM_EXTENT, 12, 1);
        let sym = self.boolean(false);
        let bodies = self.list_of(crate::features::BODIES, &[body]);
        let mut slots = vec![
            Some(op),
            Some(patch),
            Some(dir),
            Some(rev),
            Some(distance),
            Some(taper),
            Some(ext),
            Some(sym),
        ];
        slots.resize(26, None);
        slots.push(Some(bodies));
        let (extrusion, node) = self.feature(&slots);
        self.label(extrusion, &format!("Extrusion{k}"), &[sketch], EXTRUSION);
        self.top.push(extrusion);
        (extrusion, node)
    }
}

impl DesignWriter {
    /// The surface of the face an extrusion extends to
    /// (`features::EXTENT_SURFACE`): after the prefix 16 zero bytes, a byte
    /// 1, the kind (`0x19` a plane: a u32 0, its origin, x and y axes;
    /// `0x49` a cone: u32 1 and 0, the cosine and sine of its half angle,
    /// a point on its axis, its radii, axis and reference direction).
    pub fn extent_surface(&mut self, kind: u32, values: &[f64]) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&self.prefix());
        b.extend_from_slice(&[0; 16]);
        b.push(1);
        b.extend_from_slice(&kind.to_le_bytes());
        if kind == 0x49 {
            b.extend_from_slice(&1u32.to_le_bytes());
        }
        b.extend_from_slice(&0u32.to_le_bytes());
        for v in values {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&[0; 8]);
        self.push(crate::features::EXTENT_SURFACE, b)
    }
}

/// The objects extrusions extend to: a 1 cm cube (`Solid1`), then four
/// joins without sketches, by `d0`: up to the next face (extent 4) with
/// `Solid1` in slot 24, up to the plane x = 2 cm and up to the cylinder of
/// radius 3 mm about the z axis through (1, 1) cm kept in slot 18 (extent
/// 7), and up to the work plane z = 3 cm in slot 11 (extent 7).
pub fn extents_design(major: u8) -> Vec<([u8; 16], Vec<u8>)> {
    use crate::features::{ENUM_OPERATION, TARGET_BODIES};
    let mut w = DesignWriter::new(major);
    let length = w.unit(LENGTH, 1.0);
    let mm = w.unit(METRE, 1e-3);
    let n = w.number(1.0, mm);
    let d0 = w.parameter("d0", length, n, 1.0);
    let angle = w.unit(ANGLE, 1.0);
    let deg = w.unit(DEGREE, 1.0);
    let n = w.number(0.0, deg);
    let taper = w.parameter("d1", angle, n, 0.0);
    let solid1 = w.body("Solid1", 1);
    let (_, n1) = w.cube_at(1, 0.0, d0, taper, solid1);
    let mut states = vec![(n1, 0, 1)];
    let plane = w.extent_surface(0x19, &[2.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
    let cylinder = w.extent_surface(
        0x49,
        &[
            1.0, 0.0, 1.0, 1.0, 0.0, 0.3, 0.3, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0,
        ],
    );
    let work_plane = w.work_plane([0.0, 0.0, 3.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let next = w.list_of(TARGET_BODIES, &[solid1]);
    for (k, (extent, slot, object)) in [
        (4, 24, next),
        (7, 18, plane),
        (7, 18, cylinder),
        (7, 11, work_plane),
    ]
    .into_iter()
    .enumerate()
    {
        let op = w.enumeration(ENUM_OPERATION, 5, 3);
        let patch = w.patch(&[]);
        let dir = w.direction([1.0, 0.0, 0.0]);
        let rev = w.boolean(false);
        let ext = w.enumeration(ENUM_EXTENT, 12, extent);
        let sym = w.boolean(false);
        let mut slots = vec![
            Some(op),
            Some(patch),
            Some(dir),
            Some(rev),
            Some(d0),
            Some(taper),
            Some(ext),
            Some(sym),
        ];
        slots.resize(slot + 1, None);
        slots[slot] = Some(object);
        let (extrusion, node) = w.feature(&slots);
        w.label(extrusion, &format!("Extrusion{}", k + 2), &[], EXTRUSION);
        w.top.push(extrusion);
        states.push((node, k as i64 + 1, k as i64 + 2));
    }
    w.states(&states);
    w.finish()
}

/// Two 1 cm cubes (`Solid1` from the origin, `Solid2` from x = 2 cm, the
/// extrusions by `d0` = 10 mm), then a combine joining `Solid2` into
/// `Solid1` (the tools not kept); with `more`, after it a shell of
/// `Solid1` (inside, `d2` = 1 mm, removing one face) and a split of its
/// bodies by the work plane z = 0.5 cm. History states 1 to 5.
pub fn combine_design(major: u8, more: bool) -> Vec<([u8; 16], Vec<u8>)> {
    use crate::features::{BODIES, ENUM_OPERATION, ENUM_SHELL, ENUM_SPLIT, FACES, TARGET_BODIES};
    let mut w = DesignWriter::new(major);
    let length = w.unit(LENGTH, 1.0);
    let mm = w.unit(METRE, 1e-3);
    let n = w.number(1.0, mm);
    let d0 = w.parameter("d0", length, n, 1.0);
    let angle = w.unit(ANGLE, 1.0);
    let deg = w.unit(DEGREE, 1.0);
    let n = w.number(0.0, deg);
    let taper = w.parameter("d1", angle, n, 0.0);
    let solid1 = w.body("Solid1", 1);
    let solid2 = w.body("Solid2", 2);
    let (_, n1) = w.cube_at(1, 0.0, d0, taper, solid1);
    let (_, n2) = w.cube_at(2, 2.0, d0, taper, solid2);
    let target = w.list_of(TARGET_BODIES, &[solid1]);
    let tools = w.list_of(BODIES, &[solid2]);
    let op = w.enumeration(ENUM_OPERATION, 5, 3);
    let keep = w.boolean(false);
    let (combine, n3) = w.feature(&[Some(target), Some(tools), Some(op), Some(keep)]);
    w.label(combine, "Combine1", &[], COMBINE);
    w.top.push(combine);
    let mut states = vec![(n1, 0, 1), (n2, 1, 2), (n3, 2, 3)];
    if more {
        let n = w.number(0.1, mm);
        let thickness = w.parameter("d2", length, n, 0.1);
        let direction = w.enumeration(ENUM_SHELL, 2, 0);
        // A face name (not decoded): any record.
        let face = w.boolean(false);
        let faces = w.list_of(FACES, &[face]);
        let body = w.list_of(BODIES, &[solid1]);
        let mut slots = vec![Some(direction), Some(faces), None, Some(thickness)];
        slots.resize(9, None);
        slots.push(Some(body));
        let (shell, n4) = w.feature(&slots);
        w.label(shell, "Shell1", &[], SHELL);
        w.top.push(shell);
        let kind = w.enumeration(ENUM_SPLIT, 3, 3);
        let plane = w.work_plane([0.0, 0.0, 0.5], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let bodies = w.list_of(TARGET_BODIES, &[solid1]);
        let (split, n5) = w.feature(&[
            Some(kind),
            None,
            None,
            None,
            None,
            Some(plane),
            None,
            Some(bodies),
        ]);
        w.label(split, "Split1", &[], SPLIT);
        w.top.push(split);
        states.extend([(n4, 3, 4), (n5, 4, 5)]);
    }
    w.states(&states);
    w.finish()
}
