// SPDX-License-Identifier: MIT
//! Primitives, PartDesign's additive and subtractive ones and the Part
//! workbench's, as Mitcad features at their placements (FreeCAD's local
//! z the primitive's axis):
//!
//! - Box, Cylinder, Sphere, Torus → Mitcad's primitives on a fixed plane
//!   at the placement.
//! - Cone, and parts of a turn (Cylinder's and Cone's `Angle`, Sphere's
//!   latitudes `Angle1`, `Angle2` and turn `Angle3`, Torus's section
//!   sector `Angle1` to `Angle2` and turn `Angle3`) → `revolve` of the
//!   section in a plane through the axis (a fixed construction plane and a
//!   sketch), about the axis by the angle, counter-clockwise from the local
//!   x axis as FreeCAD turns.
//! - Prism → `extrude` of its polygon (a sketch on a fixed plane, the
//!   first corner on the local x axis); a skewed one (`FirstAngle`,
//!   `SecondAngle`; also a skewed cylinder) → `sweep` of it along a fixed
//!   line, without turning.
//! - Wedge → a ruled `loft` from its rectangle at `Ymin` to the one at
//!   `Ymax` (a point where that one shrinks to a point).
//! - Ellipsoid → a `sphere` of `Radius2` at the origin (a part of one,
//!   `Angle1` to `Angle2` and `Angle3` round: its section turned about the
//!   z axis), scaled along the axes by `Radius3 / Radius2` and
//!   `Radius1 / Radius2` (`scale`, non-uniform, as FreeCAD scales a part of
//!   a sphere), moved to the placement (`move`) and, in a Body, joined to
//!   or cut from its body (`combine`).
//!
//! Sizes and angles that go into features' values are the properties'
//! parameters or expressions, and so are a cone's radii and height and a
//! part of a cylinder's radius and height: dimensions of their sections
//! (the first corner fixed at the axis, the sides along the axes
//! horizontal or vertical). The other sections' sketches are numbers.

use mitcad_freecad::Object;
use mitcad_model::{Kernel, Transform};
use serde_json::{Value, json};

use super::Importer;
use super::history::{Candidate, Target};
use super::params::{Kind, Q};
use super::sketches::Placed;
use crate::geom::{add, scale};

/// The PartDesign primitives: Mitcad's kind and whether it adds.
pub(super) fn primitive_kind(t: &str) -> Option<(&'static str, bool)> {
    let (rest, additive) = if let Some(r) = t.strip_prefix("PartDesign::Additive") {
        (r, true)
    } else {
        (t.strip_prefix("PartDesign::Subtractive")?, false)
    };
    Some((part_kind(rest)?, additive))
}

/// A primitive's kind by its type's name without the workbench
/// (`Box`, `Prism`).
pub(super) fn part_kind(name: &str) -> Option<&'static str> {
    Some(match name {
        "Box" => "box",
        "Cylinder" => "cylinder",
        "Sphere" => "sphere",
        "Torus" => "torus",
        "Cone" => "cone",
        "Ellipsoid" => "ellipsoid",
        "Prism" => "prism",
        "Wedge" => "wedge",
        _ => return None,
    })
}

/// The properties a primitive's definition needs.
pub(super) fn primitive_sizes(kind: &str) -> &'static [&'static str] {
    match kind {
        "box" => &["Length", "Width", "Height"],
        "cylinder" => &["Radius", "Height"],
        "sphere" => &["Radius"],
        "torus" => &["Radius1", "Radius2"],
        "cone" => &["Radius1", "Radius2", "Height"],
        "ellipsoid" => &["Radius1", "Radius2"],
        "prism" => &["Polygon", "Circumradius", "Height"],
        "wedge" => &[
            "Xmin", "Xmax", "Ymin", "Ymax", "Zmin", "Zmax", "X2min", "X2max", "Z2min", "Z2max",
        ],
        _ => &[],
    }
}

/// A part of a sphere's section between the latitudes `a1` and `a2`
/// (degrees): the meridian from `a1` to `a2`, closed along the parallels'
/// planes to the axis and along the axis (x out from the axis, y along it).
fn sphere_section(r: f64, a1: f64, a2: f64) -> Result<Vec<Value>, String> {
    if a1 >= a2 {
        return Err("a sphere's latitudes are the wrong way round".to_owned());
    }
    let at = |a: f64| [r * a.to_radians().cos(), r * a.to_radians().sin()];
    let (bottom, top) = (at(a1), at(a2));
    let mut entities = vec![
        point("p1", [0.0, 0.0]),
        point("p2", if same(a1, -90.0) { [0.0, -r] } else { bottom }),
        point("p3", if same(a2, 90.0) { [0.0, r] } else { top }),
        json!({"id": "c4", "type": "arc", "center": "p1", "start": "p2", "end": "p3"}),
    ];
    let mut last = "p3".to_owned();
    if !same(a2, 90.0) {
        entities.push(point("p5", [0.0, top[1]]));
        entities.push(line("c6", "p3", "p5"));
        last = "p5".to_owned();
    }
    let mut first = "p2".to_owned();
    if !same(a1, -90.0) {
        entities.push(point("p7", [0.0, bottom[1]]));
        entities.push(line("c8", "p7", "p2"));
        first = "p7".to_owned();
    }
    entities.push(line("c9", &last, &first));
    Ok(entities)
}

/// An entity of a section sketch.
fn point(id: &str, at: [f64; 2]) -> Value {
    json!({"id": id, "type": "point", "at": at})
}

fn line(id: &str, start: &str, end: &str) -> Value {
    json!({"id": id, "type": "line", "start": start, "end": end})
}

/// A closed polygon's entities: points `p1`… and lines between them.
fn polygon(corners: &[[f64; 2]]) -> Vec<Value> {
    let n = corners.len();
    let mut out: Vec<Value> = corners
        .iter()
        .enumerate()
        .map(|(i, c)| point(&format!("p{}", i + 1), *c))
        .collect();
    for i in 0..n {
        out.push(line(
            &format!("c{}", n + i + 1),
            &format!("p{}", i + 1),
            &format!("p{}", (i + 1) % n + 1),
        ));
    }
    out
}

/// A section sketch's entities, constraints and dimensions: a polygon from
/// its first corner (fixed), its sides along the sketch's axes horizontal
/// or vertical, and `lengths` (side i from corner i to the next, a value)
/// as length dimensions: a section that follows its primitive's sizes.
fn dimensioned(corners: &[[f64; 2]], lengths: &[(usize, Value)]) -> Value {
    let n = corners.len();
    let mut entities = polygon(corners);
    entities[0]["fixed"] = json!(true);
    let side = |i: usize| format!("c{}", n + i + 1);
    let mut next = 2 * n + 1;
    let mut constraints = Vec::new();
    for i in 0..n {
        let (a, b) = (corners[i], corners[(i + 1) % n]);
        let kind = if (a[1] - b[1]).abs() < 1e-12 {
            "horizontal"
        } else if (a[0] - b[0]).abs() < 1e-12 {
            "vertical"
        } else {
            continue;
        };
        constraints.push(json!({"id": format!("k{next}"), "type": kind, "line": side(i)}));
        next += 1;
    }
    let mut dimensions = Vec::new();
    for (i, value) in lengths {
        dimensions.push(
            json!({"id": format!("k{next}"), "type": "length", "line": side(*i),
                               "value": value}),
        );
        next += 1;
    }
    json!({"entities": entities, "constraints": constraints, "dimensions": dimensions})
}

/// A fixed construction plane through `origin` with axes `x` and `y`.
fn fixed_plane(origin: [f64; 3], x: [f64; 3], y: [f64; 3]) -> Value {
    json!({"type": "construction_plane", "definition": {"type": "fixed",
        "origin": origin, "x_axis": x, "y_axis": y}})
}

/// The one region of the sketch added `index`-th, as a profile.
fn region_of(index: usize) -> Value {
    json!({"sketch": format!("${index}"), "region": format!("${index}:region")})
}

/// Whether two angles in degrees are the same.
fn same(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

impl<K: Kernel> Importer<'_, '_, K> {
    /// A primitive at its placement; `body` is the PartDesign Body's
    /// operation (additive, and its bodies), None for a new body.
    pub(super) fn primitive_candidates(
        &mut self,
        o: &Object,
        place: &Placed,
        kind: &str,
        body: Option<(bool, &Target)>,
    ) -> Result<Vec<Candidate>, String> {
        let p = place
            .transform
            .after(&super::transform(&super::placement_of(o)));
        let m = p.linear;
        let column = |c: usize| [m[0][c], m[1][c], m[2][c]];
        let (x, y, z, origin) = (column(0), column(1), column(2), p.translation);
        let plane = json!({"origin": origin, "normal": z, "x_axis": x});
        let size = |name: &str| o.f64(name).ok_or_else(|| format!("no {name}"));
        let has = |name: &str, value: f64| o.f64(name).is_none_or(|v| same(v, value));
        let (operation, participants) = match body {
            None => (json!("new_body"), None),
            Some((additive, target)) => {
                let (operation, participants) = self.operation(target, additive)?;
                let p = participants
                    .as_array()
                    .is_some_and(|p| !p.is_empty())
                    .then_some(participants);
                (operation, p)
            }
        };
        let operate = |def: &mut Value| {
            def["operation"] = operation.clone();
            if let Some(p) = &participants {
                def["participants"] = p.clone();
            }
        };
        let skewed = !(has("FirstAngle", 0.0) && has("SecondAngle", 0.0));
        // The turn of a part of a turn (degrees), when it is one.
        let turn = |name: &str| o.f64(name).filter(|a| !same(*a, 360.0));
        let mut def = match kind {
            "box" => json!({"type": "box", "plane": plane, "corner": [0.0, 0.0],
                "length": self.size(o, "Length", 1.0)?, "width": self.size(o, "Width", 1.0)?,
                "height": self.size(o, "Height", 1.0)?}),
            "cylinder" if skewed => {
                if turn("Angle").is_some() {
                    return Err("a skewed part of a cylinder is not translated".to_owned());
                }
                let r = size("Radius")?;
                let section = json!({"type": "sketch", "plane": "$0", "entities": [
                    point("p1", [0.0, 0.0]),
                    json!({"id": "c2", "type": "circle", "center": "p1", "radius": r})]});
                return self.skewed(o, &p, section, operate, "a skewed cylinder");
            }
            "cylinder" if turn("Angle").is_none() => {
                json!({"type": "cylinder", "plane": plane, "center": [0.0, 0.0],
                       "diameter": self.size(o, "Radius", 2.0)?, "height": self.size(o, "Height", 1.0)?})
            }
            "cylinder" => {
                let (r, h) = (size("Radius")?, size("Height")?);
                let (radius, height) = (
                    self.prop(o, "Radius", Kind::Length, 0.0).json(),
                    self.prop(o, "Height", Kind::Length, 0.0).json(),
                );
                let section = dimensioned(
                    &[[0.0, 0.0], [r, 0.0], [r, h], [0.0, h]],
                    &[(0, radius), (3, height)],
                );
                return self.turned(o, &p, section, "Angle", operate, "a part of a cylinder");
            }
            "cone" => {
                if skewed {
                    return Err("a skewed cone is not translated".to_owned());
                }
                let (r1, r2, h) = (size("Radius1")?, size("Radius2")?, size("Height")?);
                if r1 <= 0.0 && r2 <= 0.0 || h <= 0.0 {
                    return Err("a degenerate cone".to_owned());
                }
                // A trapezoid (a triangle to a point) from the axis out,
                // its radii and height dimensions of the sizes.
                let mut corners = vec![[0.0, 0.0]];
                let mut lengths = Vec::new();
                if r1 > 0.0 {
                    lengths.push((corners.len() - 1, self.size(o, "Radius1", 1.0)?));
                    corners.push([r1, 0.0]);
                }
                if r2 > 0.0 {
                    corners.push([r2, h]);
                    lengths.push((corners.len() - 1, self.size(o, "Radius2", 1.0)?));
                }
                corners.push([0.0, h]);
                lengths.push((corners.len() - 1, self.size(o, "Height", 1.0)?));
                let note = if turn("Angle").is_some() {
                    "a part of a cone: its section turned about its axis"
                } else {
                    "a cone: its section turned about its axis"
                };
                let section = dimensioned(&corners, &lengths);
                return self.turned(o, &p, section, "Angle", operate, note);
            }
            "sphere" if has("Angle1", -90.0) && has("Angle2", 90.0) && has("Angle3", 360.0) => {
                json!({"type": "sphere", "plane": plane, "center": [0.0, 0.0],
                       "diameter": self.size(o, "Radius", 2.0)?})
            }
            "sphere" => {
                let r = size("Radius")?;
                let (a1, a2) = (
                    o.f64("Angle1").unwrap_or(-90.0),
                    o.f64("Angle2").unwrap_or(90.0),
                );
                return self.turned(
                    o,
                    &p,
                    json!({"entities": sphere_section(r, a1, a2)?}),
                    "Angle3",
                    operate,
                    "a part of a sphere",
                );
            }
            "torus" if has("Angle1", -180.0) && has("Angle2", 180.0) && has("Angle3", 360.0) => {
                json!({"type": "torus", "plane": plane, "center": [0.0, 0.0],
                       "diameter": self.size(o, "Radius1", 2.0)?,
                       "section_diameter": self.size(o, "Radius2", 2.0)?, "position": "on_center"})
            }
            "torus" => {
                let (r1, r2) = (size("Radius1")?, size("Radius2")?);
                let (a1, a2) = (
                    o.f64("Angle1").unwrap_or(-180.0),
                    o.f64("Angle2").unwrap_or(180.0),
                );
                if a1 >= a2 {
                    return Err("a torus's section angles are the wrong way round".to_owned());
                }
                let center = [r1, 0.0];
                // FreeCAD's section angles turn from the radius away from
                // the axis's direction.
                let at = |a: f64| [r1 + r2 * a.to_radians().cos(), -r2 * a.to_radians().sin()];
                // The section's sector from Angle1 to Angle2 about its
                // centre (FreeCAD closes a part of the circle with its
                // radii), or the whole circle.
                let entities = if a2 - a1 >= 360.0 - 1e-9 {
                    vec![
                        point("p1", center),
                        json!({"id": "c2", "type": "circle", "center": "p1", "radius": r2}),
                    ]
                } else {
                    vec![
                        point("p1", center),
                        point("p2", at(a2)),
                        point("p3", at(a1)),
                        json!({"id": "c4", "type": "arc", "center": "p1", "start": "p2", "end": "p3"}),
                        line("c5", "p3", "p1"),
                        line("c6", "p1", "p2"),
                    ]
                };
                return self.turned(
                    o,
                    &p,
                    json!({"entities": entities}),
                    "Angle3",
                    operate,
                    "a part of a torus",
                );
            }
            "prism" => {
                let n = o.i64("Polygon").ok_or("no Polygon")?;
                if !(3..=1000).contains(&n) {
                    return Err(format!("a prism of {n} sides"));
                }
                let r = size("Circumradius")?;
                let corners: Vec<[f64; 2]> = (0..n)
                    .map(|k| {
                        let a = std::f64::consts::TAU * k as f64 / n as f64;
                        [r * a.cos(), r * a.sin()]
                    })
                    .collect();
                let section =
                    json!({"type": "sketch", "plane": "$0", "entities": polygon(&corners)});
                if skewed {
                    return self.skewed(o, &p, section, operate, "a skewed prism");
                }
                let mut extruded = json!({"type": "extrude", "profiles": [region_of(1)],
                    "extent": {"type": "distance", "distance": self.size(o, "Height", 1.0)?},
                    "flip": false});
                operate(&mut extruded);
                return Ok(vec![
                    Candidate::several(vec![
                        (fixed_plane(origin, x, y), " (plane)"),
                        (section, " (section)"),
                        (extruded, ""),
                    ])
                    .note("a prism: its polygon extruded"),
                ]);
            }
            "wedge" => return self.wedge(o, &p, operate),
            "ellipsoid" => return self.ellipsoid(o, &p, operate, body),
            other => return Err(format!("a {other} primitive is not translated yet")),
        };
        operate(&mut def);
        Ok(vec![Candidate::new(def)])
    }

    /// A size's value or expression (a diameter: twice the radius).
    fn size(&mut self, o: &Object, name: &str, factor: f64) -> Result<Value, String> {
        o.f64(name).ok_or_else(|| format!("no {name}"))?;
        Ok(self.prop(o, name, Kind::Length, 0.0).scale(factor).json())
    }

    /// A section in the plane through the axis (x along the local x axis,
    /// y along the axis; its entities, and constraints and dimensions when
    /// it has them) turned about the axis by the angle property `angle` (a
    /// whole turn when it has 360 degrees).
    fn turned(
        &mut self,
        o: &Object,
        p: &Transform,
        mut section: Value,
        angle: &str,
        operate: impl Fn(&mut Value),
        note: &str,
    ) -> Result<Vec<Candidate>, String> {
        let m = p.linear;
        let column = |c: usize| [m[0][c], m[1][c], m[2][c]];
        let (x, z, origin) = (column(0), column(2), p.translation);
        let degrees = o.f64(angle).unwrap_or(360.0);
        if degrees <= 0.0 || degrees > 360.0 + 1e-9 {
            return Err(format!("a turn of {degrees} degrees"));
        }
        let extent = if same(degrees, 360.0) {
            json!({"type": "full"})
        } else {
            json!({"type": "angle", "angle": self.prop(o, angle, Kind::Angle, 360.0).json()})
        };
        section["type"] = json!("sketch");
        section["plane"] = json!("$0");
        let mut turned = json!({"type": "revolve", "profiles": [region_of(1)],
            "axis": {"origin": origin, "direction": z}, "extent": extent});
        operate(&mut turned);
        Ok(vec![
            Candidate::several(vec![
                (fixed_plane(origin, x, z), " (plane)"),
                (section, " (section)"),
                (turned, ""),
            ])
            .note(note),
        ])
    }

    /// A section on the placement's plane swept along the skew direction
    /// (`FirstAngle` towards local x, `SecondAngle` towards local y) to
    /// the height.
    fn skewed(
        &mut self,
        o: &Object,
        p: &Transform,
        section: Value,
        operate: impl Fn(&mut Value),
        note: &str,
    ) -> Result<Vec<Candidate>, String> {
        let m = p.linear;
        let column = |c: usize| [m[0][c], m[1][c], m[2][c]];
        let (x, y, origin) = (column(0), column(1), p.translation);
        let h = o.f64("Height").ok_or("no Height")?;
        let tan = |name: &str| o.f64(name).unwrap_or(0.0).to_radians().tan();
        let (a, b) = (tan("FirstAngle"), tan("SecondAngle"));
        if !(a.is_finite() && b.is_finite()) || h <= 0.0 {
            return Err(format!("{note} of no height"));
        }
        let along = p.apply_vector([h * a, h * b, h]);
        let mut swept = json!({"type": "sweep", "profiles": [region_of(1)],
            "path": {"start": origin, "end": add(origin, along)}, "orientation": "parallel"});
        operate(&mut swept);
        Ok(vec![
            Candidate::several(vec![
                (fixed_plane(origin, x, y), " (plane)"),
                (section, " (section)"),
                (swept, ""),
            ])
            .note(format!("{note}: its section swept along the skew")),
        ])
    }

    /// A wedge: a ruled loft between its rectangles at Ymin and Ymax.
    fn wedge(
        &mut self,
        o: &Object,
        p: &Transform,
        operate: impl Fn(&mut Value),
    ) -> Result<Vec<Candidate>, String> {
        let v = |name: &str| o.f64(name).ok_or_else(|| format!("no {name}"));
        let (x0, x1, y0, y1, z0, z1) = (
            v("Xmin")?,
            v("Xmax")?,
            v("Ymin")?,
            v("Ymax")?,
            v("Zmin")?,
            v("Zmax")?,
        );
        let (u0, u1, w0, w1) = (v("X2min")?, v("X2max")?, v("Z2min")?, v("Z2max")?);
        if x0 >= x1 || z0 >= z1 || y0 >= y1 || u0 > u1 || w0 > w1 {
            return Err("a degenerate wedge".to_owned());
        }
        let m = p.linear;
        let column = |c: usize| [m[0][c], m[1][c], m[2][c]];
        let (x, y, z) = (column(0), column(1), column(2));
        let at = |h: f64| add(p.translation, scale(y, h));
        let rectangle = |a: f64, b: f64, c: f64, d: f64| polygon(&[[a, c], [b, c], [b, d], [a, d]]);
        let bottom =
            json!({"type": "sketch", "plane": "$0", "entities": rectangle(x0, x1, z0, z1)});
        let mut defs = vec![
            (fixed_plane(at(y0), x, z), " (bottom plane)"),
            (bottom, " (bottom)"),
        ];
        let mut sections = vec![json!({"type": "profile", "sketch": "$1", "region": "$1:region"})];
        let top_point = same(u0, u1) && same(w0, w1);
        if top_point {
            let top = add(at(y1), add(scale(x, u0), scale(z, w0)));
            sections.push(json!({"type": "point", "point": {"point": top}}));
        } else if same(u0, u1) || same(w0, w1) {
            return Err("a wedge whose top is a line is not translated".to_owned());
        } else {
            defs.push((fixed_plane(at(y1), x, z), " (top plane)"));
            defs.push((
                json!({"type": "sketch", "plane": "$2", "entities": rectangle(u0, u1, w0, w1)}),
                " (top)",
            ));
            sections.push(json!({"type": "profile", "sketch": "$3", "region": "$3:region"}));
        }
        let mut lofted =
            json!({"type": "loft", "sections": sections, "ruled": true, "closed": false});
        operate(&mut lofted);
        defs.push((lofted, ""));
        Ok(vec![
            Candidate::several(defs).note("a wedge: a ruled loft between its rectangles"),
        ])
    }

    /// An ellipsoid: a sphere of `Radius2` (a part of one: its section
    /// turned about the z axis, as FreeCAD scales a part of a sphere)
    /// scaled along the axes, moved to the placement, then combined with
    /// the Body's body.
    fn ellipsoid(
        &mut self,
        o: &Object,
        p: &Transform,
        operate: impl Fn(&mut Value),
        body: Option<(bool, &Target)>,
    ) -> Result<Vec<Candidate>, String> {
        let full = |name: &str, value: f64| o.f64(name).is_none_or(|v| same(v, value));
        let whole = full("Angle1", -90.0) && full("Angle2", 90.0) && full("Angle3", 360.0);
        for name in ["Radius1", "Radius2"] {
            if o.f64(name).is_none_or(|r| r <= 0.0) {
                return Err(format!("no {name}"));
            }
        }
        let r1 = self.prop(o, "Radius1", Kind::Length, 0.0);
        let r2 = self.prop(o, "Radius2", Kind::Length, 0.0);
        let ratio = |a: &Q, b: &Q| {
            Q::combine(&[a, b], a.value / b.value, Kind::Number, |t| {
                format!("{} / {}", t[0], t[1])
            })
        };
        // FreeCAD's third radius, 0 for the second's.
        let y = if o.f64("Radius3").is_some_and(|r| r > 1e-7) {
            let r3 = self.prop(o, "Radius3", Kind::Length, 0.0);
            ratio(&r3, &r2).json()
        } else {
            json!(1.0)
        };
        let mut defs = if whole {
            vec![(
                json!({"type": "sphere", "plane": "xy", "center": [0.0, 0.0],
                       "diameter": r2.scale(2.0).json(), "operation": "new_body"}),
                " (sphere)",
            )]
        } else {
            // The part of the sphere: its section in the XZ plane turned
            // about the z axis from the x axis by Angle3.
            let (a1, a2, a3) = (
                o.f64("Angle1").unwrap_or(-90.0),
                o.f64("Angle2").unwrap_or(90.0),
                o.f64("Angle3").unwrap_or(360.0),
            );
            if a3 <= 0.0 || a3 > 360.0 + 1e-9 {
                return Err(format!("a turn of {a3} degrees"));
            }
            let mut section = json!({"entities": sphere_section(r2.value, a1, a2)?});
            section["type"] = json!("sketch");
            section["plane"] = json!("$0");
            let extent = if same(a3, 360.0) {
                json!({"type": "full"})
            } else {
                json!({"type": "angle", "angle": self.prop(o, "Angle3", Kind::Angle, 360.0).json()})
            };
            vec![
                (
                    fixed_plane([0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
                    " (plane)",
                ),
                (section, " (section)"),
                (
                    json!({"type": "revolve", "profiles": [region_of(1)],
                           "axis": {"origin": [0.0, 0.0, 0.0], "direction": [0.0, 0.0, 1.0]},
                           "extent": extent, "operation": "new_body"}),
                    " (sphere)",
                ),
            ]
        };
        // The sphere's (or its part's) body.
        let sphere = defs.len() - 1;
        let made = format!("${sphere}.b0");
        defs.push((
            json!({"type": "scale", "bodies": [made], "point": "origin",
                   "scale": {"type": "non_uniform", "x": 1.0, "y": y, "z": ratio(&r1, &r2).json()}}),
            " (scale)",
        ));
        if !p.is_identity() {
            let m = p.linear;
            let t = p.translation;
            let matrix: Vec<[f64; 4]> = (0..3).map(|i| [m[i][0], m[i][1], m[i][2], t[i]]).collect();
            defs.push((
                json!({"type": "move", "bodies": [made],
                       "transform": {"type": "free", "matrix": matrix}}),
                " (placement)",
            ));
        }
        // In a Body: joined to or cut from its body, unless it is the
        // Body's first solid (then the sphere is named after the label).
        let mut operation = json!({});
        operate(&mut operation);
        match (operation["operation"].as_str(), body) {
            (Some("join" | "cut"), Some((additive, target))) => {
                let target = self.body(target)?;
                defs.push((
                    json!({"type": "combine", "target": target.to_string(), "tools": [made],
                           "operation": if additive { "join" } else { "cut" }, "keep_tools": false}),
                    "",
                ));
            }
            _ => defs[sphere].1 = "",
        }
        let note = if whole {
            "an ellipsoid: a sphere scaled along its axes"
        } else {
            "a part of an ellipsoid: a part of a sphere (its section turned) scaled along its axes"
        };
        Ok(vec![Candidate::several(defs).note(note)])
    }
}
