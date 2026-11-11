// SPDX-License-Identifier: MIT
//! The in-plane adapter: planar constraints and dimensions kept by a sketch
//! that was converted from 2D (mitcad#1542), or made by a plane-bound
//! drawing tool of a 3D sketch (mitcad#1550: a rectangle or a polygon on
//! the XY, YZ or ZX plane), compiled by the planar solver's own row
//! construction ([`crate::prepare::compile_mapped`]) on two coordinate
//! columns of the spatial points.
//!
//! A shadow planar [`System`] mirrors the spatial points, lines, plane-bound
//! arcs, circles and ellipses and splines the relations use (a fitted
//! spline with the spatial spline's own basis and hidden control points),
//! and maps each of its unknowns to a spatial column. Each frame plane has
//! its own stand-ins: a point's in the plane square to the axis `k` takes
//! the columns of the two other axes in the frame's order ([`plane_axes`]:
//! XY's x and y, YZ's y and z, ZX's z and x), so a planar relation means in
//! that plane what it means in the sketch plane. The relations themselves
//! live in the shadow; only their rows, with mapped columns, enter the
//! spatial solve. Coincidences of the spatial system are mirrored too (they
//! join points for the planar tangent and direction rules), but their rows
//! stay the spatial ones. The planar direction graph runs over the shadow,
//! that is over the in-plane direction relations only.
//!
//! A rotated plane (mitcad#1661: a shape drawn on a rotated placement
//! plane) has stand-ins too, keyed after the three frame planes
//! ([`Shadow::add_frame`]): a point's planar x and y are its coordinates
//! `x.p` and `y.p` along the plane's unit in-plane axes. They are no
//! spatial columns: each maps to a virtual column ([`Shadow::virtual_of`])
//! that the in-plane rows of the spatial system evaluate from the point's
//! three columns (`RowKind::FramePlanar`), so the planar rows mean in
//! that plane what they mean in the sketch plane.

use std::collections::{HashMap, HashSet};

use crate::direction;
use crate::prepare::{Equation, compile_mapped};
use crate::system::{EntityData, System, constraint_entities, constraint_points};
use crate::types::{Constraint, ConstraintId, Dependency, EntityId, PointId};

/// The compiled in-plane rows, each with its spatial relation (index), and
/// the spatial fitted spline ends (curve index, end) an in-plane tangent
/// controls.
pub(super) type Compiled = (Vec<(usize, Equation)>, HashSet<(usize, usize)>);

/// Not a spatial column.
const NONE: usize = usize::MAX;

/// The first virtual column (mitcad#1661): a point's coordinate along an
/// in-plane axis of a rotated plane, `VIRTUAL + i` the `i`-th.
const VIRTUAL: usize = usize::MAX / 2;

/// The planar x and y of the plane square to frame axis `k` (0, 1, 2): the
/// two other axes in the frame's order, so that x × y points along `k`.
pub(super) fn plane_axes(k: usize) -> [usize; 2] {
    match k {
        0 => [1, 2],
        1 => [2, 0],
        _ => [0, 1],
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct Shadow {
    pub sys: System,
    /// The shadow point of each spatial point (by index) in each plane (by
    /// the axis square to it).
    point: HashMap<(usize, usize), PointId>,
    /// The shadow entity of each spatial line and curve (by index) in each
    /// plane.
    line: HashMap<(usize, usize), EntityId>,
    curve: HashMap<(usize, usize), EntityId>,
    /// The spatial column of each shadow unknown ([`NONE`]: none).
    col: Vec<usize>,
    /// The spatial relation (index) of each in-plane shadow constraint.
    owner: HashMap<usize, usize>,
    /// Shadow coincidences mirroring the spatial ones, and their points.
    mirrors: Vec<ConstraintId>,
    mirrored: Vec<(PointId, PointId)>,
    /// The rotated planes' unit in-plane axes x and y (mitcad#1661), the
    /// plane keyed `3 + index`.
    frames: Vec<[[f64; 3]; 2]>,
    /// Each virtual column's point (its first spatial column) and axis.
    virt: Vec<(usize, [f64; 3])>,
}

impl Shadow {
    /// True when no in-plane relation exists (the shadow is not used).
    pub fn is_empty(&self) -> bool {
        self.owner.is_empty()
    }

    fn map(&mut self, g: usize, col: usize) {
        if self.col.len() <= g {
            self.col.resize(g + 1, NONE);
        }
        self.col[g] = col;
    }

    /// The spatial column of a shadow unknown.
    pub fn column(&self, g: usize) -> usize {
        self.col.get(g).copied().unwrap_or(NONE)
    }

    /// Copies the mapped spatial values into the shadow (a virtual
    /// column's coordinate along its axis).
    pub fn sync(&mut self, params: &[f64]) {
        for (g, &c) in self.col.iter().enumerate() {
            if c == NONE {
                continue;
            }
            self.sys.params[g] = match self.virtual_of(c) {
                Some((col, e)) => {
                    e[0] * params[col] + e[1] * params[col + 1] + e[2] * params[col + 2]
                }
                None => params[c],
            };
        }
    }

    /// The number of planes with stand-ins: the three frame planes and the
    /// rotated ones (mitcad#1661).
    pub fn planes(&self) -> usize {
        3 + self.frames.len()
    }

    /// The key of the rotated plane with the unit in-plane axes `x` and
    /// `y` (mitcad#1661): an existing one's when the axes agree within
    /// 1e-12, else a new one's.
    pub fn add_frame(&mut self, x: [f64; 3], y: [f64; 3]) -> usize {
        let near = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() <= 1e-12);
        if let Some(i) = self
            .frames
            .iter()
            .position(|f| near(f[0], x) && near(f[1], y))
        {
            return 3 + i;
        }
        self.frames.push([x, y]);
        2 + self.frames.len()
    }

    /// The point (its first spatial column) and the in-plane axis of a
    /// virtual column; none for a spatial column.
    pub fn virtual_of(&self, c: usize) -> Option<(usize, [f64; 3])> {
        if c == NONE || c < VIRTUAL {
            return None;
        }
        self.virt.get(c - VIRTUAL).copied()
    }

    /// A new virtual column of a point's coordinate along `e`.
    fn add_virtual(&mut self, col: usize, e: [f64; 3]) -> usize {
        self.virt.push((col, e));
        VIRTUAL + self.virt.len() - 1
    }

    /// The shadow point of spatial point `index` in the plane square to
    /// axis `k` (the point's first column `col`).
    pub fn point(&mut self, index: usize, k: usize, col: usize, params: &[f64]) -> PointId {
        if let Some(&p) = self.point.get(&(index, k)) {
            return p;
        }
        if k >= 3 {
            // A rotated plane's (mitcad#1661): along its in-plane axes.
            let [x, y] = self.frames[k - 3];
            let at =
                |e: [f64; 3]| e[0] * params[col] + e[1] * params[col + 1] + e[2] * params[col + 2];
            let p = self.sys.add_point(at(x), at(y));
            let d = self.sys.pd(p.index());
            let (vx, vy) = (self.add_virtual(col, x), self.add_virtual(col, y));
            self.map(d.x, vx);
            self.map(d.y, vy);
            self.point.insert((index, k), p);
            return p;
        }
        let [i, j] = plane_axes(k);
        let p = self.sys.add_point(params[col + i], params[col + j]);
        let d = self.sys.pd(p.index());
        self.map(d.x, col + i);
        self.map(d.y, col + j);
        self.point.insert((index, k), p);
        p
    }

    pub fn line_entity(&self, index: usize, k: usize) -> Option<EntityId> {
        self.line.get(&(index, k)).copied()
    }

    pub fn curve_entity(&self, index: usize, k: usize) -> Option<EntityId> {
        self.curve.get(&(index, k)).copied()
    }

    pub fn set_line(&mut self, index: usize, k: usize, e: EntityId) {
        self.line.insert((index, k), e);
    }

    /// True when an in-plane relation uses a stand-in of spatial line
    /// `index` (in any plane).
    pub fn uses_line(&self, index: usize) -> bool {
        (0..self.planes()).any(|k| {
            self.line_entity(index, k)
                .is_some_and(|e| self.uses_entity(e))
        })
    }

    /// True when an in-plane relation uses a stand-in of spatial curve
    /// `index` (in any plane).
    pub fn uses_curve(&self, index: usize) -> bool {
        (0..self.planes()).any(|k| {
            self.curve_entity(index, k)
                .is_some_and(|e| self.uses_entity(e))
        })
    }

    /// Records the shadow entity of spatial curve `index` in the plane
    /// square to `k`, mapping its radius (a circle's), minor radius (an
    /// ellipse's) or hidden control points (a fitted spline's) to the
    /// spatial columns `cols` (a radius column, or the first column of each
    /// hidden control point).
    pub fn set_curve(&mut self, index: usize, k: usize, e: EntityId, cols: &[usize]) {
        let data = self.sys.ent(e).expect("new entity").clone();
        match data {
            EntityData::Circle { r, .. } => self.map(r, cols[0]),
            EntityData::Ellipse { b, .. } => self.map(b, cols[0]),
            EntityData::Fitted { ctrl, .. } => {
                let [i, j] = plane_axes(k);
                for (h, &c) in ctrl[1..ctrl.len() - 1].iter().zip(cols) {
                    let d = self.sys.pd(*h);
                    self.map(d.x, c + i);
                    self.map(d.y, c + j);
                }
            }
            _ => {}
        }
        self.curve.insert((index, k), e);
    }

    /// Adds an in-plane relation of spatial relation `owner`; returns the
    /// shadow constraint and the shadow unknowns it owns (curve parameters,
    /// for the caller to map to new spatial columns).
    pub fn add(
        &mut self,
        c: Constraint,
        owner: usize,
    ) -> Result<(ConstraintId, Vec<usize>), crate::Error> {
        let id = self.sys.add_constraint(c)?;
        self.owner.insert(id.index(), owner);
        let params = self.sys.constraints[id.index()]
            .as_ref()
            .map(|r| r.params.clone())
            .unwrap_or_default();
        Ok((id, params))
    }

    pub fn map_param(&mut self, g: usize, col: usize) {
        self.map(g, col);
    }

    pub fn remove(&mut self, id: ConstraintId) {
        self.owner.remove(&id.index());
        let _ = self.sys.remove_constraint(id);
    }

    /// True when an in-plane relation uses spatial point `index` (its
    /// stand-in in any plane).
    pub fn uses_point(&self, index: usize) -> bool {
        (0..self.planes()).any(|k| {
            self.point
                .get(&(index, k))
                .is_some_and(|p| self.relations().any(|c| constraint_points(c).contains(p)))
        })
    }

    /// True when an in-plane relation uses the shadow entity `e`.
    pub fn uses_entity(&self, e: EntityId) -> bool {
        self.relations()
            .any(|c| constraint_entities(c).contains(&e))
    }

    fn relations(&self) -> impl Iterator<Item = &Constraint> {
        self.owner
            .keys()
            .filter_map(|&c| self.sys.constraint(ConstraintId(c as u32)))
    }

    /// Forgets the shadow of a removed spatial point, line or curve.
    pub fn remove_point(&mut self, index: usize) {
        for k in 0..self.planes() {
            if let Some(p) = self.point.remove(&(index, k)) {
                let _ = self.sys.remove_point(p);
            }
        }
    }

    pub fn remove_line(&mut self, index: usize) {
        for k in 0..self.planes() {
            if let Some(e) = self.line.remove(&(index, k)) {
                let _ = self.sys.remove_entity(e);
            }
        }
    }

    pub fn remove_curve(&mut self, index: usize) {
        for k in 0..self.planes() {
            if let Some(e) = self.curve.remove(&(index, k)) {
                let _ = self.sys.remove_entity(e);
            }
        }
    }

    /// Mirrors the spatial coincidences between shadow points (of the same
    /// plane).
    pub fn mirror(&mut self, pairs: &[(usize, usize)]) {
        let point = &self.point;
        let wanted: Vec<(PointId, PointId)> = (0..self.planes())
            .flat_map(|k| {
                pairs
                    .iter()
                    .filter_map(move |(a, b)| Some((*point.get(&(*a, k))?, *point.get(&(*b, k))?)))
            })
            .collect();
        if wanted == self.mirrored {
            return;
        }
        for id in self.mirrors.drain(..) {
            let _ = self.sys.remove_constraint(id);
        }
        for &(a, b) in &wanted {
            if let Ok(id) = self.sys.add_constraint(Constraint::Coincident(a, b)) {
                self.mirrors.push(id);
            }
        }
        self.mirrored = wanted;
    }

    /// The rows of the in-plane relations (each with its spatial relation),
    /// their columns spatial, and the spatial fitted spline ends (curve
    /// index, end) an in-plane tangent controls.
    pub fn compile(&self) -> Compiled {
        let (eqs, controlled) = compile_mapped(
            &self.sys,
            |c| self.owner.contains_key(&c),
            |g| self.column(g),
        );
        let rows = eqs
            .into_iter()
            .filter_map(|e| Some((*self.owner.get(&e.constraint()?)?, e)))
            .collect();
        let curve_of: HashMap<usize, usize> = self
            .curve
            .iter()
            .map(|(&(i, _), e)| (e.index(), i))
            .collect();
        let controlled = controlled
            .into_iter()
            .filter_map(|(e, end)| Some((*curve_of.get(&e)?, end)))
            .collect();
        (rows, controlled)
    }

    /// The planar direction graph over the in-plane relations: the
    /// contradicting and the redundant sets, as spatial relations
    /// (indices).
    pub fn directions(&self) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
        if self.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let (conflicts, redundant) = direction::check(&self.sys, &self.sys.point_classes());
        let map = |deps: Vec<Dependency>| -> Vec<Vec<usize>> {
            deps.into_iter()
                .map(|d| {
                    d.involved
                        .iter()
                        .filter_map(|c| self.owner.get(&c.index()).copied())
                        .collect()
                })
                .filter(|v: &Vec<usize>| !v.is_empty())
                .collect()
        };
        (map(conflicts), map(redundant))
    }

    /// The current value of an in-plane dimension (the shadow is synced).
    pub fn measure(&self, id: ConstraintId) -> Option<f64> {
        self.sys.measure(id)
    }
}
