// SPDX-License-Identifier: MIT
//! The spindle and what it carries (docs/cam.md, "Spindle assembly"): the
//! spindle body, the collet and its nut or a tool holder, a tool changer's
//! moving parts on the spindle, the spindle's mount, and the tool, each a
//! named part that cuts (the tool's flutes) or does not (everything else),
//! for the collision check along a program (`collision`).
//!
//! A [`SpindleModel`] is the machine's spindle without a tool, in the
//! frame of its gauge line: the axis along +Z, `z = 0` where a tool's
//! stick-out is measured from (the face of the collet nut or the holder),
//! the spindle above it. It is read from a MitCad design of the spindle
//! ([`SpindleModel::from_bodies`], its components named by what they are),
//! or made from a machine's one-cylinder nose ([`SpindleModel::from_nose`]).
//! A [`SpindleAssembly`] is a model with a tool in it, in the frame of the
//! tool's tip: the tip at the origin, the axis along +Z, the model raised by
//! the tool's stick-out.
//!
//! A part's shape is exact where it is a solid of revolution about the
//! axis (a stack of cylinders and cones, [`Frustum`]), else its triangles.
//! A part read from a mesh keeps the mesh's deviation as its `margin`: the
//! part is within that distance of its shape, which the check counts
//! against it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::CamError;
use crate::machine::Spindle;
use crate::tool::{Tool, ToolShape};
use crate::toolpath::P3;

/// What a part of the spindle assembly is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartKind {
    /// The tool's flutes: the only part that cuts.
    Cutter,
    /// The tool above its flutes up to the collet.
    Shank,
    Collet,
    /// The collet nut.
    Nut,
    /// A tool holder (a changer's taper holder, a shrink-fit holder).
    Holder,
    /// The spindle's body and nose.
    Spindle,
    /// The mount or clamp that holds the spindle on the Z axis.
    Mount,
    /// A tool changer's moving parts that the spindle carries.
    Changer,
    /// Any other part on the spindle (a dust shoe, a probe).
    Other,
}

impl PartKind {
    /// The part in words, for messages.
    pub fn words(self) -> &'static str {
        match self {
            PartKind::Cutter => "the cutter",
            PartKind::Shank => "the tool's shank",
            PartKind::Collet => "the collet",
            PartKind::Nut => "the collet nut",
            PartKind::Holder => "the tool holder",
            PartKind::Spindle => "the spindle",
            PartKind::Mount => "the spindle mount",
            PartKind::Changer => "the tool changer",
            PartKind::Other => "a part on the spindle",
        }
    }

    /// Whether the part cuts (only the tool's flutes do).
    pub fn cutting(self) -> bool {
        self == PartKind::Cutter
    }

    /// What a component of a spindle design is, by its name (any case):
    /// a name with `nut`, `collet`, `holder`, `changer`, `mount`, `bracket`
    /// or `clamp`, `spindle`, `body` or `motor` in it is that part, in this
    /// order (a "Spindle nut" is a nut, a "Tool holder" a holder); a name
    /// with `tool`, `cutter`, `bit` or `mill` is the tool, which is `None`:
    /// the tool comes from the tool definition of each operation. Any other
    /// name is [`PartKind::Other`].
    pub fn from_component_name(name: &str) -> Option<PartKind> {
        let n = name.to_lowercase();
        let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
        if has(&["nut"]) {
            Some(PartKind::Nut)
        } else if has(&["collet"]) {
            Some(PartKind::Collet)
        } else if has(&["holder"]) {
            Some(PartKind::Holder)
        } else if has(&["changer"]) {
            Some(PartKind::Changer)
        } else if has(&["mount", "bracket", "clamp"]) {
            Some(PartKind::Mount)
        } else if has(&["spindle", "body", "motor"]) {
            Some(PartKind::Spindle)
        } else if has(&["tool", "cutter", "bit", "mill"]) {
            None
        } else {
            Some(PartKind::Other)
        }
    }
}

/// A slice of a solid of revolution about the axis: a cylinder or a cone
/// from `z0` (radius `r0`) up to `z1` (radius `r1`), mm.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frustum {
    pub z0: f64,
    pub z1: f64,
    pub r0: f64,
    pub r1: f64,
}

impl Frustum {
    pub fn cylinder(z0: f64, z1: f64, r: f64) -> Frustum {
        Frustum {
            z0,
            z1,
            r0: r,
            r1: r,
        }
    }
}

/// The shape of a part in its assembly's frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PartShape {
    /// A solid of revolution about the axis, as slices from the bottom up.
    Revolved { sections: Vec<Frustum> },
    /// The triangles of the part's surface.
    Mesh { triangles: Vec<[P3; 3]> },
}

impl PartShape {
    /// The shape raised by `dz` along the axis.
    fn raised(&self, dz: f64) -> PartShape {
        match self {
            PartShape::Revolved { sections } => PartShape::Revolved {
                sections: sections
                    .iter()
                    .map(|s| Frustum {
                        z0: s.z0 + dz,
                        z1: s.z1 + dz,
                        ..*s
                    })
                    .collect(),
            },
            PartShape::Mesh { triangles } => PartShape::Mesh {
                triangles: triangles
                    .iter()
                    .map(|t| t.map(|p| [p[0], p[1], p[2] + dz]))
                    .collect(),
            },
        }
    }
}

/// A part of the spindle assembly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpindlePart {
    /// The component's name in the spindle design, or the part's own for
    /// the parts made here (for agents; messages use [`PartKind::words`]).
    pub name: String,
    pub kind: PartKind,
    /// Whether it cuts (the tool's flutes).
    pub cutting: bool,
    pub shape: PartShape,
    /// How far the real part may be outside its shape (mm): a mesh's
    /// deviation from the surface it was made of; 0 for exact shapes.
    #[serde(default)]
    pub margin: f64,
}

/// The machine's spindle without a tool, in its gauge line's frame (the
/// module's documentation).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SpindleModel {
    pub parts: Vec<SpindlePart>,
}

/// A body of a spindle design for [`SpindleModel::from_bodies`]: the name
/// of its component and its triangle mesh in the spindle's frame (mm).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DesignBody {
    pub component: String,
    pub vertices: Vec<P3>,
    /// Indices into `vertices`.
    pub triangles: Vec<[u32; 3]>,
}

impl SpindleModel {
    /// The spindle as a machine without a modelled spindle describes it:
    /// its nose (nut, holder and body) as one cylinder up from the gauge
    /// line.
    pub fn from_nose(spindle: &Spindle) -> SpindleModel {
        SpindleModel {
            parts: vec![SpindlePart {
                name: "Spindle".into(),
                kind: PartKind::Spindle,
                cutting: false,
                shape: PartShape::Revolved {
                    sections: vec![Frustum::cylinder(
                        0.0,
                        spindle.nose_length,
                        spindle.nose_diameter / 2.0,
                    )],
                },
                margin: 0.0,
            }],
        }
    }

    /// The spindle model of a design's bodies, meshed within `deviation`
    /// (mm) and placed in the gauge line's frame: each body is a part of
    /// the kind its component's name says ([`PartKind::from_component_name`];
    /// the tool's bodies are left out). A body round about the axis (its
    /// box centred on the axis and reaching its largest radius on all four
    /// sides) becomes slices of revolution that hold it, the others their
    /// triangles.
    pub fn from_bodies(bodies: &[DesignBody], deviation: f64) -> Result<SpindleModel, CamError> {
        if !(deviation.is_finite() && deviation >= 0.0) {
            return Err(CamError::new(
                "The spindle's mesh deviation must be zero or more.",
            ));
        }
        let mut parts = Vec::new();
        for body in bodies {
            let Some(kind) = PartKind::from_component_name(&body.component) else {
                continue;
            };
            if body
                .triangles
                .iter()
                .flatten()
                .any(|&i| i as usize >= body.vertices.len())
                || body.vertices.iter().flatten().any(|v| !v.is_finite())
            {
                return Err(CamError::new(
                    "A body of the spindle design has a mesh that is not valid.",
                ));
            }
            if body.triangles.is_empty() {
                continue;
            }
            let shape = match revolved_sections(&body.vertices, &body.triangles, deviation) {
                Some(sections) => PartShape::Revolved { sections },
                None => PartShape::Mesh {
                    triangles: body
                        .triangles
                        .iter()
                        .map(|t| t.map(|i| body.vertices[i as usize]))
                        .collect(),
                },
            };
            parts.push(SpindlePart {
                name: body.component.clone(),
                kind,
                cutting: false,
                shape,
                margin: deviation,
            });
        }
        if parts.is_empty() {
            return Err(CamError::new("The spindle design has no spindle parts."));
        }
        Ok(SpindleModel { parts })
    }

    /// The assembly with `tool` in the spindle.
    pub fn assembly(&self, tool: &Tool) -> SpindleAssembly {
        SpindleAssembly::new(self, tool)
    }
}

/// The spindle with a tool in it, in the frame of the tool's tip (the
/// module's documentation).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpindleAssembly {
    pub parts: Vec<SpindlePart>,
}

impl SpindleAssembly {
    /// The tool's parts from its definition (the flutes, which cut, and
    /// the shank above them up to the gauge line, of the tool's diameter),
    /// and the model's parts raised by the tool's stick-out.
    pub fn new(model: &SpindleModel, tool: &Tool) -> SpindleAssembly {
        let mut parts = tool_parts(tool);
        parts.extend(model.parts.iter().map(|p| SpindlePart {
            shape: p.shape.raised(tool.length),
            ..p.clone()
        }));
        SpindleAssembly { parts }
    }
}

/// The tool as parts: the cutting end by its shape (a drill's point and a
/// V-bit's cone as cones, the other ends as the full diameter, which holds
/// a ball or a rounded corner), up to the flutes' length; the shank of the
/// same diameter above them up to the stick-out.
fn tool_parts(tool: &Tool) -> Vec<SpindlePart> {
    let r = tool.radius().max(0.0);
    let length = tool.length.max(0.0);
    let flutes = tool.flute_length.clamp(0.0, length);
    // The cone of a pointed end: its radius at the tip and its height.
    let cone = |tip: f64, angle: f64| {
        let half = (angle.clamp(1.0, 179.0) / 2.0).to_radians();
        let tip = tip.clamp(0.0, r);
        (tip, (r - tip) / half.tan())
    };
    let point = match tool.shape {
        ToolShape::Drill { point_angle } => Some(cone(0.0, point_angle)),
        ToolShape::VBit {
            angle,
            tip_diameter,
        } => Some(cone(tip_diameter / 2.0, angle)),
        _ => None,
    };
    let mut sections = Vec::new();
    match point {
        Some((tip, h)) if h > 0.0 => {
            if h < flutes {
                sections.push(Frustum {
                    z0: 0.0,
                    z1: h,
                    r0: tip,
                    r1: r,
                });
                sections.push(Frustum::cylinder(h, flutes, r));
            } else {
                sections.push(Frustum {
                    z0: 0.0,
                    z1: flutes,
                    r0: tip,
                    r1: tip + (r - tip) * flutes / h,
                });
            }
        }
        _ => sections.push(Frustum::cylinder(0.0, flutes, r)),
    }
    let mut parts = vec![SpindlePart {
        name: tool.name.clone(),
        kind: PartKind::Cutter,
        cutting: true,
        shape: PartShape::Revolved { sections },
        margin: 0.0,
    }];
    if length > flutes + 1e-9 {
        parts.push(SpindlePart {
            name: tool.name.clone(),
            kind: PartKind::Shank,
            cutting: false,
            shape: PartShape::Revolved {
                sections: vec![Frustum::cylinder(flutes, length, r)],
            },
            margin: 0.0,
        });
    }
    parts
}

/// Heights closer than this are one level of a mesh.
const LEVEL_EPS: f64 = 1e-9;

/// The slices of revolution about the Z axis that hold a body round about
/// it, or `None` when it is not: its box must be centred on the axis and
/// reach its largest radius on all four sides within twice the deviation.
///
/// Between two neighbouring heights of the mesh's vertices, each
/// triangle that spans them is cut in a segment whose ends move linearly
/// with the height, so its largest radius is convex in the height and
/// stays under the line between its values at the two heights; the slice
/// takes the largest of those values at each end, so it holds every
/// triangle's section and with them the body's (the disc of a section's
/// largest radius holds the section).
pub(crate) fn revolved_sections(
    vertices: &[P3],
    triangles: &[[u32; 3]],
    deviation: f64,
) -> Option<Vec<Frustum>> {
    let used: Vec<P3> = {
        let mut seen = vec![false; vertices.len()];
        for t in triangles {
            for &i in t {
                seen[i as usize] = true;
            }
        }
        vertices
            .iter()
            .zip(seen)
            .filter(|(_, s)| *s)
            .map(|(v, _)| *v)
            .collect()
    };
    if used.is_empty() {
        return None;
    }
    let rmax = used.iter().map(|v| v[0].hypot(v[1])).fold(0.0, f64::max);
    let tol = 2.0 * deviation + 1e-6;
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for v in &used {
        for k in 0..2 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    let round = (0..2).all(|k| (hi[k] - rmax).abs() <= tol && (lo[k] + rmax).abs() <= tol);
    if !round || rmax <= tol {
        return None;
    }
    let mut levels: Vec<f64> = used.iter().map(|v| v[2]).collect();
    levels.sort_by(f64::total_cmp);
    levels.dedup_by(|a, b| (*a - *b).abs() <= LEVEL_EPS);
    let tris: Vec<[P3; 3]> = triangles
        .iter()
        .map(|t| t.map(|i| vertices[i as usize]))
        .collect();
    let mut sections: Vec<Frustum> = Vec::new();
    for pair in levels.windows(2) {
        let (za, zb) = (pair[0], pair[1]);
        let (mut ra, mut rb) = (0.0_f64, 0.0_f64);
        for t in &tris {
            let tmin = t.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
            let tmax = t.iter().map(|p| p[2]).fold(f64::NEG_INFINITY, f64::max);
            if tmax <= za + LEVEL_EPS || tmin >= zb - LEVEL_EPS {
                continue;
            }
            ra = ra.max(section_radius(t, za));
            rb = rb.max(section_radius(t, zb));
        }
        if ra == 0.0 && rb == 0.0 {
            // Nothing between the levels: two bodies stacked with a gap.
            continue;
        }
        let next = Frustum {
            z0: za,
            z1: zb,
            r0: ra,
            r1: rb,
        };
        match sections.last_mut() {
            // One cylinder cut by levels of other faces: one slice.
            Some(last)
                if (last.z1 - za).abs() <= LEVEL_EPS
                    && (last.r0 - last.r1).abs() <= LEVEL_EPS
                    && (last.r1 - ra).abs() <= LEVEL_EPS
                    && (ra - rb).abs() <= LEVEL_EPS =>
            {
                last.z1 = zb;
            }
            _ => sections.push(next),
        }
    }
    if sections.is_empty() {
        None
    } else {
        Some(sections)
    }
}

/// The largest distance from the axis of the triangle's section at height
/// `z` (within its heights).
fn section_radius(t: &[P3; 3], z: f64) -> f64 {
    let mut r = 0.0_f64;
    for k in 0..3 {
        let (p, q) = (t[k], t[(k + 1) % 3]);
        if (p[2] - z).abs() <= LEVEL_EPS {
            r = r.max(p[0].hypot(p[1]));
        }
        if (p[2] - z) * (q[2] - z) < 0.0 {
            let s = (z - p[2]) / (q[2] - p[2]);
            let x = p[0] + (q[0] - p[0]) * s;
            let y = p[1] + (q[1] - p[1]) * s;
            r = r.max(x.hypot(y));
        }
    }
    r
}

/// Groups bodies of one component (for callers that read a design body by
/// body): the components in the order they first come.
pub fn group_by_component(bodies: Vec<DesignBody>) -> Vec<DesignBody> {
    let mut order: Vec<String> = Vec::new();
    let mut merged: BTreeMap<String, DesignBody> = BTreeMap::new();
    for b in bodies {
        let entry = merged.entry(b.component.clone()).or_insert_with(|| {
            order.push(b.component.clone());
            DesignBody {
                component: b.component.clone(),
                ..DesignBody::default()
            }
        });
        let base = entry.vertices.len() as u32;
        entry.vertices.extend(b.vertices);
        entry
            .triangles
            .extend(b.triangles.iter().map(|t| t.map(|i| i + base)));
    }
    order
        .into_iter()
        .filter_map(|c| merged.remove(&c))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    /// A closed mesh of a cylinder about the axis from `z0` to `z1`, its
    /// circles of `n` vertices (the first on +X).
    fn cylinder_mesh(z0: f64, z1: f64, r: f64, n: usize, component: &str) -> DesignBody {
        let mut vertices = Vec::new();
        for z in [z0, z1] {
            for i in 0..n {
                let a = TAU * i as f64 / n as f64;
                vertices.push([r * a.cos(), r * a.sin(), z]);
            }
        }
        vertices.push([0.0, 0.0, z0]);
        vertices.push([0.0, 0.0, z1]);
        let (bottom, top) = (2 * n as u32, 2 * n as u32 + 1);
        let mut triangles = Vec::new();
        for i in 0..n as u32 {
            let j = (i + 1) % n as u32;
            let n32 = n as u32;
            triangles.push([i, j, j + n32]);
            triangles.push([i, j + n32, i + n32]);
            triangles.push([bottom, j, i]);
            triangles.push([top, i + n32, j + n32]);
        }
        DesignBody {
            component: component.into(),
            vertices,
            triangles,
        }
    }

    #[test]
    fn component_names_say_what_the_part_is() {
        assert_eq!(
            PartKind::from_component_name("ER20 Nut"),
            Some(PartKind::Nut)
        );
        assert_eq!(
            PartKind::from_component_name("Spindle nut"),
            Some(PartKind::Nut)
        );
        assert_eq!(
            PartKind::from_component_name("Tool holder"),
            Some(PartKind::Holder)
        );
        assert_eq!(
            PartKind::from_component_name("Tool changer fork"),
            Some(PartKind::Changer)
        );
        assert_eq!(
            PartKind::from_component_name("Spindle"),
            Some(PartKind::Spindle)
        );
        assert_eq!(PartKind::from_component_name("Tool"), None);
        assert_eq!(
            PartKind::from_component_name("Dust shoe"),
            Some(PartKind::Other)
        );
    }

    #[test]
    fn a_stepped_round_body_becomes_slices_that_hold_it() {
        // A nose of radius 20 from 0 to 12, a body of radius 40 from 12 to
        // 200, as one mesh of 48-sided circles.
        let nose = cylinder_mesh(0.0, 12.0, 20.0, 48, "Spindle");
        let body = cylinder_mesh(12.0, 200.0, 40.0, 48, "Spindle");
        let merged = group_by_component(vec![nose, body]);
        assert_eq!(merged.len(), 1);
        let model = SpindleModel::from_bodies(&merged, 0.01).unwrap();
        assert_eq!(model.parts.len(), 1);
        let PartShape::Revolved { sections } = &model.parts[0].shape else {
            panic!("not round: {:?}", model.parts[0].shape);
        };
        let want = [
            Frustum::cylinder(0.0, 12.0, 20.0),
            Frustum::cylinder(12.0, 200.0, 40.0),
        ];
        assert_eq!(sections.len(), want.len(), "{sections:?}");
        for (s, w) in sections.iter().zip(want) {
            for (a, b) in [(s.z0, w.z0), (s.z1, w.z1), (s.r0, w.r0), (s.r1, w.r1)] {
                assert!((a - b).abs() < 1e-9, "{sections:?}");
            }
        }
        assert_eq!(model.parts[0].margin, 0.01);
        assert_eq!(model.parts[0].kind, PartKind::Spindle);
    }

    #[test]
    fn a_body_off_the_axis_keeps_its_triangles_and_the_tool_is_left_out() {
        let mut drawbar = cylinder_mesh(50.0, 90.0, 8.0, 24, "Changer cylinder");
        for v in &mut drawbar.vertices {
            v[0] += 50.0;
        }
        let tool = cylinder_mesh(-20.0, 0.0, 3.0, 24, "Tool");
        let model = SpindleModel::from_bodies(&[drawbar, tool], 0.01).unwrap();
        assert_eq!(model.parts.len(), 1);
        assert_eq!(model.parts[0].kind, PartKind::Changer);
        assert!(matches!(model.parts[0].shape, PartShape::Mesh { .. }));
        assert!(SpindleModel::from_bodies(&[], 0.01).is_err());
    }

    #[test]
    fn the_assembly_puts_the_tool_under_the_raised_spindle() {
        let mut tool = Tool::flat_end_mill(1, 6.0);
        tool.flute_length = 15.0;
        tool.length = 25.0;
        let model = SpindleModel::from_nose(&Spindle {
            min_rpm: 0.0,
            max_rpm: 24000.0,
            spin_up_seconds: 0.0,
            nose_diameter: 35.0,
            nose_length: 20.0,
        });
        let a = model.assembly(&tool);
        let kinds: Vec<PartKind> = a.parts.iter().map(|p| p.kind).collect();
        assert_eq!(
            kinds,
            vec![PartKind::Cutter, PartKind::Shank, PartKind::Spindle]
        );
        assert!(a.parts[0].cutting && !a.parts[1].cutting && !a.parts[2].cutting);
        assert_eq!(
            a.parts[2].shape,
            PartShape::Revolved {
                sections: vec![Frustum::cylinder(25.0, 45.0, 17.5)]
            }
        );
        let mut v = Tool::flat_end_mill(2, 6.0);
        v.shape = ToolShape::VBit {
            angle: 90.0,
            tip_diameter: 0.0,
        };
        let PartShape::Revolved { sections } = &tool_parts(&v)[0].shape else {
            panic!("a V-bit is round");
        };
        // A 90 degree cone of radius 3 is 3 high.
        assert!((sections[0].z1 - 3.0).abs() < 1e-12 && sections[0].r0 == 0.0);
        assert_eq!(sections[1], Frustum::cylinder(sections[0].z1, 18.0, 3.0));
    }
}
