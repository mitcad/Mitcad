// SPDX-License-Identifier: MIT
//! `model_render`'s inspection options in the design (mitcad#1487):
//! `explode` moves the placements apart, `colors` and `highlight` colour
//! their faces and find the highlighted edges' lines. A reference that
//! names nothing drawn is a warning, not a refusal.
//!
//! - `explode`: the groups are the top-level occurrences and the root's
//!   bodies; with one group only, each placed body. Each group moves from
//!   the drawn bodies' centre by `factor` times its own centre's distance
//!   from it (the centres of their bounding boxes); 0 moves nothing.
//! - `colors`: each key names placements as `bodies` does; a placement
//!   several keys name takes the colour of the key naming the fewest (the
//!   most specific), then of the key later in sorted order.
//! - `highlight`: a body (any id or name `bodies` takes) has all its faces
//!   in the accent colour; a face by its topological name has its faces
//!   (each face one of whose names matches it) in the accent colour; an
//!   edge by its name is drawn as a line over everything.

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use mitcad_preview::raster::ACCENT_RGB;
use mitcad_preview::{EntityKind, Highlight, Inspect};
use mitcad_vec3::{Vec3, add, scale, sub};

use crate::document::{Document, InstanceView};
use crate::kernel::Kernel;
use crate::topo::{EdgeName, FaceName};
use crate::transform::Transform;

/// The highlighted edges' lines: no farther than this from the edge, by
/// the body's size (as its mesh), and turning at most this much.
const LINE_ANGLE_DEGREES: f64 = 12.0;

/// How one placement is coloured.
#[derive(Debug, Clone, Default)]
pub(super) struct Look {
    /// Every face, from `colors` (sRGB, 0 to 1).
    pub color: Option<[f64; 3]>,
    /// Every face in the accent colour.
    pub accent: bool,
    /// These faces (by their index) in the accent colour.
    pub accent_faces: BTreeSet<usize>,
}

impl Look {
    /// The faces' colours from the body's own.
    pub(super) fn face_colors(&self, own: &[[f64; 3]]) -> Vec<[f64; 3]> {
        let mut colors: Vec<[f64; 3]> = match self.color {
            Some(color) => vec![color; own.len().max(1)],
            None => own.to_vec(),
        };
        for (i, c) in colors.iter_mut().enumerate() {
            if self.accent || self.accent_faces.contains(&i) {
                *c = ACCENT_RGB;
            }
        }
        colors
    }
}

/// Each placement's look and the highlighted edges.
#[derive(Debug, Clone, Default)]
pub(super) struct Looks {
    pub instances: Vec<Look>,
    /// In the design's coordinates.
    pub lines: Vec<Vec<Vec3>>,
}

impl<K: Kernel> Document<K> {
    /// The placements named, None (with a warning) when none is.
    fn named_or_warn(
        &self,
        what: &str,
        id: &str,
        instances: &[InstanceView],
        warnings: &mut Vec<String>,
    ) -> Option<Vec<bool>> {
        match self.named_instances(id, instances) {
            Ok(mask) => Some(mask),
            Err(_) => {
                warnings.push(format!("{what}: {id} names no body drawn"));
                None
            }
        }
    }

    /// `explode`: each placement's offset (see the module's text).
    pub(super) fn explode_offsets(&self, instances: &[InstanceView], factor: f64) -> Vec<Vec3> {
        let mut offsets = vec![[0.0; 3]; instances.len()];
        if factor == 0.0 || instances.is_empty() {
            return offsets;
        }
        let key = |i: &InstanceView| match i.path.first() {
            Some(top) => format!("o{top}"),
            None => format!("b{}", i.body),
        };
        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (index, instance) in instances.iter().enumerate() {
            groups.entry(key(instance)).or_default().push(index);
        }
        if groups.len() < 2 {
            groups = (0..instances.len())
                .map(|index| (format!("{index:08}"), vec![index]))
                .collect();
        }
        let all: Vec<&InstanceView> = instances.iter().collect();
        let Some(whole) = self.instances_box(&all) else {
            return offsets;
        };
        let centre = whole.center();
        for members in groups.values() {
            let placed: Vec<&InstanceView> = members.iter().map(|&m| &instances[m]).collect();
            let Some(own) = self.instances_box(&placed) else {
                continue;
            };
            let offset = scale(sub(own.center(), centre), factor);
            for &m in members {
                offsets[m] = offset;
            }
        }
        offsets
    }

    /// `colors` and `highlight` of the placements drawn.
    pub(super) fn preview_looks(
        &self,
        instances: &[InstanceView],
        inspect: &Inspect,
        warnings: &mut Vec<String>,
    ) -> Looks {
        let mut looks = Looks {
            instances: vec![Look::default(); instances.len()],
            lines: Vec::new(),
        };
        if inspect.colors.is_empty() && inspect.highlight.is_empty() {
            return looks;
        }
        // The broadest key first, so that the most specific is applied last.
        let mut colored: Vec<(usize, Vec<bool>, [u8; 3])> = Vec::new();
        for (id, rgb) in &inspect.colors {
            if let Some(mask) = self.named_or_warn("colors", id, instances, warnings) {
                let count = mask.iter().filter(|m| **m).count();
                colored.push((count, mask, *rgb));
            }
        }
        colored.sort_by_key(|(count, _, _)| std::cmp::Reverse(*count));
        for (_, mask, rgb) in colored {
            let color = rgb.map(|c| f64::from(c) / 255.0);
            for (look, hit) in looks.instances.iter_mut().zip(mask) {
                if hit {
                    look.color = Some(color);
                }
            }
        }
        let mut faces_of = BTreeMap::new();
        for item in &inspect.highlight {
            self.highlight(item, instances, &mut looks, &mut faces_of, warnings);
        }
        looks
    }

    /// One highlight's placements, faces or edge lines.
    fn highlight(
        &self,
        item: &Highlight,
        instances: &[InstanceView],
        looks: &mut Looks,
        faces_of: &mut BTreeMap<crate::ids::BodyUid, Vec<Vec<FaceName>>>,
        warnings: &mut Vec<String>,
    ) {
        let every = vec![true; instances.len()];
        // What it names: placements and, for a face or an edge, its name.
        let (mask, kind, name, shown) = match item {
            Highlight::Text(text) => {
                if let Ok(mask) = self.named_instances(text, instances) {
                    (mask, EntityKind::Body, String::new(), text.clone())
                } else if let Some(kind) = entity_kind(text) {
                    (every, kind, text.clone(), text.clone())
                } else {
                    // `<body>/<face or edge name>`.
                    let split =
                        text.char_indices()
                            .filter(|(_, c)| *c == '/')
                            .find_map(|(at, _)| {
                                let (body, name) = (&text[..at], &text[at + 1..]);
                                let kind = entity_kind(name)?;
                                let mask = self.named_instances(body, instances).ok()?;
                                Some((mask, kind, name.to_owned()))
                            });
                    match split {
                        Some((mask, kind, name)) => (mask, kind, name, text.clone()),
                        None => {
                            warnings.push(format!(
                                "highlight: {text} names no body, face or edge drawn"
                            ));
                            return;
                        }
                    }
                }
            }
            Highlight::Entity {
                kind,
                body,
                occurrence,
                name,
            } => {
                let shown = match body {
                    Some(body) if name.is_empty() => body.clone(),
                    Some(body) => format!("{body}/{name}"),
                    None => name.clone(),
                };
                let mut mask = every;
                for selector in [body, occurrence].into_iter().flatten() {
                    let Some(named) =
                        self.named_or_warn("highlight", selector, instances, warnings)
                    else {
                        return;
                    };
                    for (m, n) in mask.iter_mut().zip(named) {
                        *m &= n;
                    }
                }
                if !mask.iter().any(|m| *m) {
                    warnings.push(format!("highlight: {shown} names no body drawn"));
                    return;
                }
                if *kind != EntityKind::Body && entity_kind(name) != Some(*kind) {
                    warnings.push(format!(
                        "highlight: {name} is no {} name",
                        if *kind == EntityKind::Face {
                            "face"
                        } else {
                            "edge"
                        }
                    ));
                    return;
                }
                (mask, *kind, name.clone(), shown)
            }
        };
        let found = match kind {
            EntityKind::Body => {
                for (look, hit) in looks.instances.iter_mut().zip(&mask) {
                    look.accent |= *hit;
                }
                true
            }
            EntityKind::Face => {
                let Ok(reference) = FaceName::from_str(&name) else {
                    return;
                };
                let mut found = false;
                for (index, instance) in instances.iter().enumerate() {
                    if !mask[index] {
                        continue;
                    }
                    let faces = faces_of
                        .entry(instance.body)
                        .or_insert_with(|| self.face_names(instance.body));
                    for (f, names) in faces.iter().enumerate() {
                        if names.iter().any(|n| n.matches(&reference)) {
                            looks.instances[index].accent_faces.insert(f);
                            found = true;
                        }
                    }
                }
                found
            }
            EntityKind::Edge => {
                let Ok(edge) = EdgeName::from_str(&name) else {
                    return;
                };
                let mut lines: BTreeMap<crate::ids::BodyUid, Option<Vec<Vec<Vec3>>>> =
                    BTreeMap::new();
                let mut found = false;
                for (index, instance) in instances.iter().enumerate() {
                    if !mask[index] {
                        continue;
                    }
                    let own = lines
                        .entry(instance.body)
                        .or_insert_with(|| self.edge_lines(instance.body, &edge));
                    if let Some(own) = own {
                        found = true;
                        looks
                            .lines
                            .extend(own.iter().map(|line| placed(line, &instance.transform)));
                    }
                }
                found
            }
        };
        if !found {
            warnings.push(format!(
                "highlight: no body drawn has {} {shown}",
                if kind == EntityKind::Face {
                    "a face"
                } else {
                    "an edge"
                }
            ));
        }
    }

    /// Each face's names, in the order the kernel lists the faces (the
    /// display mesh's).
    fn face_names(&self, body: crate::ids::BodyUid) -> Vec<Vec<FaceName>> {
        self.body_shape(body)
            .and_then(|shape| self.kernel().faces(shape).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|face| {
                face.names
                    .iter()
                    .filter_map(|n| FaceName::from_str(n).ok())
                    .collect()
            })
            .collect()
    }

    /// An edge's lines in the body's coordinates; None when the body has
    /// no such edge.
    fn edge_lines(&self, body: crate::ids::BodyUid, edge: &EdgeName) -> Option<Vec<Vec<Vec3>>> {
        let shape = self.body_shape(body)?;
        let diagonal = match self.kernel().bounding_box(shape) {
            Ok(Some(b)) => mitcad_vec3::norm(sub(b.max, b.min)),
            _ => 0.0,
        };
        let lines = self
            .kernel()
            .edge_polylines(
                shape,
                std::slice::from_ref(edge),
                &[],
                (diagonal * 1e-3).clamp(0.01, 0.5),
                LINE_ANGLE_DEGREES.to_radians(),
            )
            .ok()?;
        (!lines.is_empty()).then_some(lines)
    }
}

/// A face or edge name's kind, None for neither.
fn entity_kind(text: &str) -> Option<EntityKind> {
    if EdgeName::from_str(text).is_ok() {
        Some(EntityKind::Edge)
    } else if FaceName::from_str(text).is_ok() {
        Some(EntityKind::Face)
    } else {
        None
    }
}

fn placed(line: &[Vec3], transform: &Transform) -> Vec<Vec3> {
    line.iter().map(|&p| transform.apply_point(p)).collect()
}

/// A placement moved by `offset`.
pub(super) fn moved(instance: &mut InstanceView, offset: Vec3) {
    instance.transform.translation = add(instance.transform.translation, offset);
}
