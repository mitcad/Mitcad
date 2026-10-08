// SPDX-License-Identifier: MIT
//! A writer of definitions segment records as the files store them (see
//! `dc`, `params`, `sketch`, `features`), for tests here and of the
//! import: no third-party file is needed to test the decoding.

use crate::dc::{COLLECTION, DOCUMENT, LABEL};
use crate::design::{FEATURE, SKETCH, STATE_TABLE};
use crate::features::{BOOLEAN, BOUNDARY_PATCH, DIRECTION, ENUM_EXTENT, ENUM_OPERATION};
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

    /// A parameter of the part's table.
    pub fn parameter(&mut self, name: &str, unit: usize, formula: usize, value: f64) -> usize {
        let table = self.table;
        let mut b = self.header(Some(table));
        b.extend_from_slice(&[0xFF; 8]);
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
        b.extend_from_slice(&[0xFF; 8]);
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
        b.extend_from_slice(&[0xFF; 8]);
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
        b.extend_from_slice(&[0xFF; 8]);
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
        b.extend_from_slice(&[0xFF; 8]);
        b.extend_from_slice(&kind.to_le_bytes());
        b.extend_from_slice(&value.to_le_bytes());
        self.push(t, b)
    }

    pub fn boolean(&mut self, value: bool) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&[0xFF; 8]);
        b.extend_from_slice(&[0; 8]);
        b.push(u8::from(value));
        self.push(BOOLEAN, b)
    }

    pub fn direction(&mut self, v: [f64; 3]) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&[0xFF; 8]);
        b.extend_from_slice(&[0; 12]);
        for x in v {
            b.extend_from_slice(&x.to_le_bytes());
        }
        self.push(DIRECTION, b)
    }

    /// A list property (a boundary patch of profile selections).
    pub fn patch(&mut self, selections: &[usize]) -> usize {
        let mut b = self.header(None);
        b.extend_from_slice(&[0xFF; 8]);
        b.extend_from_slice(&list(2, selections));
        self.push(BOUNDARY_PATCH, b)
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
    /// join; extent 1 distance, 5 through all) along `direction`.
    pub fn extrusion(
        &mut self,
        operation: u16,
        extent: u16,
        distance: usize,
        taper: usize,
        direction: [f64; 3],
        reversed: bool,
    ) -> (usize, u32) {
        let op = self.enumeration(ENUM_OPERATION, 5, operation);
        let patch = self.patch(&[]);
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
        let mut b = self.header(None);
        let mark = [
            0x02, 0x00, 0x00, 0x30, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x08, 0x01,
            0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01,
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
        .map(|i| w.sketch_line(sketch, points[i], points[(i + 1) % 4], 0))
        .collect();
    let mut entities = points.clone();
    entities.extend(&lines);
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

    let (extrusion, node) = w.extrusion(1, 1, d2, taper, [0.0, 0.0, 1.0], false);
    w.label(extrusion, "Extrusion1", &[sketch], EXTRUSION);
    w.top.push(extrusion);
    w.states(&[(node, 2, 1)]);
    w.finish()
}
