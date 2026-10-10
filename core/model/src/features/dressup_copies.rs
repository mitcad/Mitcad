// SPDX-License-Identifier: MIT
//! Fillets and chamfers among the objects of a pattern or mirror of
//! features (mitcad#105). A fillet or chamfer leaves no tool body to copy;
//! the pattern applies it again at each element instead, on the copies of
//! the edges it rounds, after the copies of the features before it in the
//! timeline (the objects go in timeline order when one is a fillet or a
//! chamfer).
//!
//! The copies' edges are found by their names first. In each edge name, a
//! face that a copied feature made is renamed as the pattern names that
//! feature's copy at the element (`<pattern>:inst<element>(<face>)`, the
//! inner instances of a pattern of patterns inside), and a face that a
//! fillet or chamfer among the objects made is named as its copy is: the
//! copies' new faces are the pattern's, `<pattern>:fillet(<the copy's
//! edge>)` (`chamfer`, `corner` alike). Faces of other features keep their
//! names, so a chamfer between a copied boss and the plate it stands on is
//! repeated between the boss' copy and the plate. Where the renaming
//! changes which face of an edge's name comes first (names are sorted), a
//! two-distance or distance-angle set without a reference face takes its
//! other face (`flip`), so the distances stay on the faces the original
//! measured them on.
//!
//! An edge whose copy no body has by that name (no face of it was copied,
//! as for a fillet patterned without the features it rounds, or its other
//! face is another one at the copy's place) is found by its place: the
//! edge of a body whose middle, direction and length are those of the
//! original edge (on the body before the fillet) moved by the element's
//! transform. An edge found neither way (a copy that reaches no body) is
//! left out with a warning.
//!
//! Each body gets the copies of all elements in one operation, else (when
//! that fails) one element after another. When that fails too, the pattern
//! applies all its objects one element after another
//! (`pattern.rs`, `Copying::by_elements`).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::bodies::BodySet;
use super::pattern::Element;
use super::{ChamferSizeDef, EvalContext, FeatureDef, FilletSizeDef};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{EdgeMiddle, Kernel};
use crate::topo::{EdgeName, FaceName, RoleKey, VertexName};
use crate::transform::{Instance, Transform};

/// Fillets and chamfers: what a pattern repeats on its copies' edges.
pub(crate) fn is_dressup(def: &FeatureDef) -> bool {
    matches!(def, FeatureDef::Fillet(_) | FeatureDef::Chamfer(_))
}

/// The features whose faces a pattern of `objects` copies: the objects, and
/// the features of the patterns and mirrors of features among them
/// (`nested`: a feature's objects when it is such a pattern, else none).
pub(crate) fn copied_features(
    objects: &[FeatureUid],
    nested: &mut dyn FnMut(FeatureUid) -> Vec<FeatureUid>,
) -> BTreeSet<FeatureUid> {
    let mut out = BTreeSet::new();
    let mut todo = objects.to_vec();
    while let Some(uid) = todo.pop() {
        if out.insert(uid) {
            todo.extend(nested(uid));
        }
    }
    out
}

/// The edges a fillet's or chamfer's sets name.
fn named_edges(def: &FeatureDef) -> Vec<EdgeName> {
    match def {
        FeatureDef::Fillet(f) => f.sets.iter().flat_map(|s| s.edges.clone()).collect(),
        FeatureDef::Chamfer(c) => c.sets.iter().flat_map(|s| s.edges.clone()).collect(),
        _ => Vec::new(),
    }
}

/// True when a body's edge `actual` is meant by the reference `reference`:
/// its faces match (pieces of them too) and, with `#k`, its number.
fn edge_matches(actual: &EdgeName, reference: &EdgeName) -> bool {
    let [a, b] = actual.faces();
    let [x, y] = reference.faces();
    let faces = (a.matches(x) && b.matches(y)) || (a.matches(y) && b.matches(x));
    faces && reference.index.is_none_or(|k| actual.index == Some(k))
}

/// How far (mm) an edge's middle may lie from the original's moved there
/// and still be its copy found by its place.
const PLACE: f64 = 1e-4;

/// The edges of `middles` at the place of `original` moved by `transform`:
/// the same middle, direction and length.
fn at_place(original: &EdgeMiddle, transform: &Transform, middles: &[EdgeMiddle]) -> Vec<EdgeName> {
    let point = transform.apply_point(original.point);
    let tangent = transform.apply_vector(original.tangent);
    middles
        .iter()
        .filter(|m| {
            let d = (0..3)
                .map(|i| (m.point[i] - point[i]).powi(2))
                .sum::<f64>()
                .sqrt();
            let dot: f64 = (0..3).map(|i| m.tangent[i] * tangent[i]).sum();
            d <= PLACE
                && dot.abs() >= 1.0 - 1e-6
                && (m.length - original.length).abs() <= PLACE + 1e-9 * original.length
        })
        .filter_map(|m| m.name.parse().ok())
        .collect()
}

/// The shape of `body` that fillet or chamfer `uid` started from: the last
/// one the features before it set.
fn input_shape<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    uid: FeatureUid,
    body: BodyUid,
) -> Option<K::Shape> {
    let mut shape = None;
    for result in ctx.env.history {
        if result.uid == uid {
            break;
        }
        if let Some(s) = result.shape_set(body) {
            shape = Some(s);
        }
    }
    shape.cloned()
}

/// The names of a fillet's or chamfer's references on one copy.
pub(crate) struct CopyNames<'a> {
    /// The pattern or mirror.
    pub pattern: FeatureUid,
    /// The features whose faces it copies ([`copied_features`]).
    pub copied: &'a BTreeSet<FeatureUid>,
    /// The fillets and chamfers among them.
    pub dressups: &'a BTreeSet<FeatureUid>,
    /// The instances that name the copy, innermost first: the inner
    /// patterns' (patterns of patterns), then the element's.
    pub instances: Vec<Instance>,
    /// Copies of edges found by their place, by original edge, where the
    /// names do not find them.
    pub placed: HashMap<EdgeName, Vec<EdgeName>>,
}

impl CopyNames<'_> {
    pub fn face(&self, face: &FaceName) -> FaceName {
        if self.dressups.contains(&face.feature) {
            FaceName {
                feature: self.pattern,
                role: face.role.clone(),
                key: face.key.as_ref().map(|k| self.key(k)),
                split: face.split.clone(),
            }
        } else if self.copied.contains(&face.feature) {
            self.instances
                .iter()
                .fold(face.clone(), |name, instance| instance.face(name))
        } else {
            face.clone()
        }
    }

    fn key(&self, key: &RoleKey) -> RoleKey {
        match key {
            RoleKey::Edge(edge) => RoleKey::Edge(Box::new(self.edge(edge))),
            RoleKey::Vertex(vertex) => RoleKey::Vertex(Box::new(self.vertex(vertex))),
            RoleKey::Face(face) => RoleKey::Face(Box::new(self.face(face))),
            other => other.clone(),
        }
    }

    pub fn edge(&self, edge: &EdgeName) -> EdgeName {
        let [a, b] = edge.faces();
        let mut copy = EdgeName::new(self.face(a), self.face(b));
        copy.index = edge.index;
        copy
    }

    pub fn vertex(&self, vertex: &VertexName) -> VertexName {
        let mut copy = VertexName::new(vertex.faces().iter().map(|f| self.face(f)))
            .expect("a vertex has faces");
        copy.index = vertex.index;
        copy
    }

    /// The copies of edges (found by their place, else those with a copied
    /// face), split into those whose first face stays first and, when
    /// `sides` (distances measured on the first face), those whose first
    /// face is now the second (edges found by their place keep theirs).
    fn edges(&self, edges: &[EdgeName], sides: bool) -> (Vec<EdgeName>, Vec<EdgeName>) {
        let (mut kept, mut swapped) = (Vec::new(), Vec::new());
        for edge in edges {
            if let Some(placed) = self.placed.get(edge) {
                kept.extend(placed.iter().cloned());
                continue;
            }
            let copy = self.edge(edge);
            if copy == *edge {
                continue;
            }
            if sides && copy.faces()[0] != self.face(&edge.faces()[0]) {
                swapped.push(copy);
            } else {
                kept.push(copy);
            }
        }
        (kept, swapped)
    }

    /// The copies of faces (those a copied feature made).
    fn faces(&self, faces: &[FaceName]) -> Vec<FaceName> {
        faces
            .iter()
            .map(|f| self.face(f))
            .filter(|copy| !faces.contains(copy))
            .collect()
    }

    /// The fillet or chamfer on this copy: its sets with the copies' edges
    /// and faces; None when it rounds nothing copied.
    pub fn definition(&self, def: &FeatureDef) -> Option<FeatureDef> {
        match def {
            FeatureDef::Fillet(fillet) => {
                let mut copy = fillet.clone();
                copy.sets = Vec::new();
                for set in &fillet.sets {
                    let sides = matches!(set.size, FilletSizeDef::Asymmetric { .. })
                        && set.reference_face.is_none();
                    let (kept, swapped) = self.edges(&set.edges, sides);
                    let mut same = set.clone();
                    same.edges = kept;
                    same.faces = self.faces(&set.faces);
                    same.reference_face = set.reference_face.as_ref().map(|f| self.face(f));
                    if let FilletSizeDef::Variable {
                        start_vertex: Some(vertex),
                        ..
                    } = &mut same.size
                    {
                        *vertex = self.vertex(vertex);
                    }
                    if !swapped.is_empty() {
                        let mut other = same.clone();
                        other.edges = swapped;
                        other.faces = Vec::new();
                        if let FilletSizeDef::Asymmetric { flip, .. } = &mut other.size {
                            *flip = !*flip;
                        }
                        copy.sets.push(other);
                    }
                    if !same.edges.is_empty() || !same.faces.is_empty() {
                        copy.sets.push(same);
                    }
                }
                (!copy.sets.is_empty()).then_some(FeatureDef::Fillet(copy))
            }
            FeatureDef::Chamfer(chamfer) => {
                let mut copy = chamfer.clone();
                copy.sets = Vec::new();
                for set in &chamfer.sets {
                    let sides = !matches!(set.size, ChamferSizeDef::EqualDistance { .. })
                        && set.reference_face.is_none();
                    let (kept, swapped) = self.edges(&set.edges, sides);
                    let mut same = set.clone();
                    same.edges = kept;
                    same.faces = self.faces(&set.faces);
                    same.reference_face = set.reference_face.as_ref().map(|f| self.face(f));
                    if !swapped.is_empty() {
                        let mut other = same.clone();
                        other.edges = swapped;
                        other.faces = Vec::new();
                        other.flip = !other.flip;
                        copy.sets.push(other);
                    }
                    if !same.edges.is_empty() || !same.faces.is_empty() {
                        copy.sets.push(same);
                    }
                }
                (!copy.sets.is_empty()).then_some(FeatureDef::Chamfer(copy))
            }
            _ => None,
        }
    }
}

/// The part of a copy's definition `shape` has: the sets' edges and faces
/// it has, the sets whose reference face it has; None when nothing is
/// left. True when something was left out.
fn present<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    def: &FeatureDef,
) -> Option<(FeatureDef, bool)> {
    let has_edge = |e: &EdgeName| kernel.count_edges(shape, e) > 0;
    // A kernel that cannot tell leaves the face to the operation.
    let has_face = |f: &FaceName| !matches!(kernel.count_faces(shape, f), Ok(0));
    let mut lost = false;
    let mut keep =
        |edges: &mut Vec<EdgeName>, faces: &mut Vec<FaceName>, reference: Option<&FaceName>| {
            let before = edges.len() + faces.len();
            edges.retain(|e| has_edge(e));
            faces.retain(|f| has_face(f));
            let reference = reference.is_none_or(has_face);
            let kept = reference && !(edges.is_empty() && faces.is_empty());
            lost |= !kept || edges.len() + faces.len() < before;
            kept
        };
    let def = match def {
        FeatureDef::Fillet(fillet) => {
            let mut copy = fillet.clone();
            copy.sets.retain_mut(|s| {
                let reference = s.reference_face.clone();
                keep(&mut s.edges, &mut s.faces, reference.as_ref())
            });
            (!copy.sets.is_empty()).then_some(FeatureDef::Fillet(copy))
        }
        FeatureDef::Chamfer(chamfer) => {
            let mut copy = chamfer.clone();
            copy.sets.retain_mut(|s| {
                let reference = s.reference_face.clone();
                keep(&mut s.edges, &mut s.faces, reference.as_ref())
            });
            (!copy.sets.is_empty()).then_some(FeatureDef::Chamfer(copy))
        }
        _ => None,
    }?;
    Some((def, lost))
}

/// The copies of several elements as one definition: all their sets.
fn merged(copies: &[(u32, FeatureDef)]) -> FeatureDef {
    let mut all = copies[0].1.clone();
    for (_, copy) in &copies[1..] {
        match (&mut all, copy) {
            (FeatureDef::Fillet(all), FeatureDef::Fillet(copy)) => {
                all.sets.extend(copy.sets.iter().cloned());
            }
            (FeatureDef::Chamfer(all), FeatureDef::Chamfer(copy)) => {
                all.sets.extend(copy.sets.iter().cloned());
            }
            _ => {}
        }
    }
    all
}

/// Rounds or bevels `shape` by a copy's definition, its new faces named
/// after `pattern`.
fn dress<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    pattern: FeatureUid,
    def: &FeatureDef,
    shape: &K::Shape,
) -> Result<K::Shape, String> {
    match def {
        FeatureDef::Fillet(fillet) => fillet.round(ctx, pattern, shape),
        FeatureDef::Chamfer(chamfer) => chamfer.bevel(ctx, pattern, shape),
        _ => Err("only fillets and chamfers are repeated on copies".to_owned()),
    }
}

/// Fillet or chamfer `uid` applied again on its copies at the elements
/// (see the module's documentation). `steps`: the transforms and instances
/// of the inner patterns that took it to this place (patterns of
/// patterns), innermost first. The copies go on the bodies that have
/// their edges, the fillet's or chamfer's own body first. False when no
/// copy finds an edge; fails when the operation fails on a copy.
pub(crate) fn repeat_dressup<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    set: &mut BodySet<K::Shape>,
    uid: FeatureUid,
    steps: &[(Transform, Instance)],
    elements: &[Element],
    copied: &BTreeSet<FeatureUid>,
    dressups: &BTreeSet<FeatureUid>,
) -> Result<bool, String> {
    let entry = ctx.feature_entry(uid)?;
    // Its result read, so that its changes recompute the pattern (and its
    // input body with them); one that failed is not repeated.
    ctx.feature_changed_bodies(uid)?;
    let home = match &entry.def {
        FeatureDef::Fillet(fillet) => fillet.body,
        FeatureDef::Chamfer(chamfer) => chamfer.body,
        _ => return Err(format!("{} is not a fillet or chamfer", entry.name)),
    };
    let pattern = ctx.uid;
    // The bodies to look on: the fillet's own first, then those the
    // pattern changed or made (copied faces are only there), then the
    // others (edges found by their place).
    let mut changed = set.changed();
    if let Some(at) = changed.iter().position(|b| *b == home) {
        changed[..=at].rotate_right(1);
    }
    let mut bodies = changed.clone();
    if !bodies.contains(&home) && set.get(home).is_some() {
        bodies.insert(0, home);
    }
    bodies.extend(
        set.uids()
            .into_iter()
            .filter(|b| !changed.contains(b) && *b != home),
    );
    // Where the edges were before the fillet, and the bodies' edges, for
    // the copies found by their place (measured when needed).
    let named = named_edges(&entry.def);
    let mut originals: Option<Vec<EdgeMiddle>> = None;
    let mut middles: BTreeMap<BodyUid, Vec<EdgeMiddle>> = BTreeMap::new();
    let at = steps
        .iter()
        .fold(Transform::IDENTITY, |done, (t, _)| t.after(&done));
    let mut work: Vec<(BodyUid, Vec<(u32, FeatureDef)>)> = Vec::new();
    let mut missing = Vec::new();
    for element in elements {
        let mut instances: Vec<Instance> = steps.iter().map(|(_, i)| *i).collect();
        instances.push(Instance {
            feature: pattern,
            index: element.index,
        });
        let mut names = CopyNames {
            pattern,
            copied,
            dressups,
            instances,
            placed: HashMap::new(),
        };
        let transform = element.transform.after(&at);
        let mut unfound = false;
        for edge in &named {
            let copy = names.edge(edge);
            let by_name = copy != *edge
                && changed.iter().any(|b| {
                    set.get(*b)
                        .is_some_and(|s| ctx.kernel.count_edges(s, &copy) > 0)
                });
            if by_name {
                continue;
            }
            let originals = originals.get_or_insert_with(|| {
                input_shape(ctx, uid, home)
                    .and_then(|s| ctx.kernel.edge_middles(&s).ok())
                    .unwrap_or_default()
            });
            let mut placed = Vec::new();
            for original in originals.iter().filter(|m| {
                m.name
                    .parse::<EdgeName>()
                    .is_ok_and(|n| edge_matches(&n, edge))
            }) {
                for body in &bodies {
                    let Some(shape) = set.get(*body) else {
                        continue;
                    };
                    let on = middles
                        .entry(*body)
                        .or_insert_with(|| ctx.kernel.edge_middles(shape).unwrap_or_default());
                    let found = at_place(original, &transform, on);
                    if !found.is_empty() {
                        placed.extend(found);
                        break;
                    }
                }
            }
            if placed.is_empty() {
                unfound = true;
            } else {
                names.placed.insert(edge.clone(), placed);
            }
        }
        let Some(copy) = names.definition(&entry.def) else {
            missing.push(element.index);
            continue;
        };
        let found = bodies.iter().copied().find_map(|body| {
            let shape = set.get(body)?;
            present(ctx.kernel, shape, &copy).map(|(def, lost)| (body, def, lost))
        });
        let Some((body, def, lost)) = found else {
            missing.push(element.index);
            continue;
        };
        if lost || unfound {
            missing.push(element.index);
        }
        match work.iter_mut().find(|(b, _)| *b == body) {
            Some((_, copies)) => copies.push((element.index, def)),
            None => work.push((body, vec![(element.index, def)])),
        }
    }
    if work.is_empty() {
        return Ok(false);
    }
    if !missing.is_empty() {
        let list: Vec<String> = missing.iter().map(u32::to_string).collect();
        ctx.warn(format!(
            "{} is not repeated where the copies lack its edges (element{} {})",
            entry.name,
            if missing.len() > 1 { "s" } else { "" },
            list.join(", ")
        ));
    }
    for (body, copies) in work {
        let shape = set.get(body).cloned().expect("the body has the copies");
        let dressed = match dress(ctx, pattern, &merged(&copies), &shape) {
            Ok(dressed) => dressed,
            Err(e) if copies.len() == 1 => {
                return Err(format!("{} at element {}: {e}", entry.name, copies[0].0));
            }
            // One element after another, where the copies' roundings get
            // in each other's way at once.
            Err(_) => {
                let mut dressed = shape;
                for (index, copy) in &copies {
                    dressed = dress(ctx, pattern, copy, &dressed)
                        .map_err(|e| format!("{} at element {index}: {e}", entry.name))?;
                }
                dressed
            }
        };
        set.set(body, dressed);
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::{ChamferCorner, ChamferDef, ChamferSetDef};
    use crate::parameters::ParamId;

    fn name(text: &str) -> FaceName {
        text.parse().unwrap()
    }

    fn edge(a: &str, b: &str) -> EdgeName {
        EdgeName::new(name(a), name(b))
    }

    #[test]
    fn copied_faces_take_the_instances_and_dressup_faces_the_pattern() {
        let copied: BTreeSet<FeatureUid> = [FeatureUid(4), FeatureUid(5), FeatureUid(7)].into();
        let dressups: BTreeSet<FeatureUid> = [FeatureUid(5)].into();
        let names = CopyNames {
            pattern: FeatureUid(9),
            copied: &copied,
            dressups: &dressups,
            instances: vec![
                Instance {
                    feature: FeatureUid(7),
                    index: 1,
                },
                Instance {
                    feature: FeatureUid(9),
                    index: 2,
                },
            ],
            placed: HashMap::new(),
        };
        // A face of a copied feature, inner instance inside the outer one;
        // the plate's face stays.
        assert_eq!(
            names
                .edge(&edge("F4:side(c1)#1", "F2:end(r{c1})"))
                .to_string(),
            "E{F2:end(r{c1})|F9:inst2(F7:inst1(F4:side(c1)#1))}"
        );
        // A face the copied chamfer F5 made: the pattern's, its edge copied.
        assert_eq!(
            names
                .face(&name("F5:chamfer(E{F2:end(r{c1})|F4:side(c1)})#0"))
                .to_string(),
            "F9:chamfer(E{F2:end(r{c1})|F9:inst2(F7:inst1(F4:side(c1)))})#0"
        );
    }

    #[test]
    fn edges_without_copied_faces_need_their_place_and_sides_follow_their_faces() {
        let copied: BTreeSet<FeatureUid> = [FeatureUid(3), FeatureUid(5)].into();
        let dressups: BTreeSet<FeatureUid> = [FeatureUid(5)].into();
        let names = CopyNames {
            pattern: FeatureUid(6),
            copied: &copied,
            dressups: &dressups,
            instances: vec![Instance {
                feature: FeatureUid(6),
                index: 1,
            }],
            placed: HashMap::new(),
        };
        let two = |edges: Vec<EdgeName>| ChamferSetDef {
            edges,
            faces: Vec::new(),
            size: ChamferSizeDef::TwoDistances {
                distance1: ParamId::from_raw(1),
                distance2: ParamId::from_raw(2),
            },
            reference_face: None,
            flip: false,
            tangent_chain: true,
        };
        // F3's side comes first in the original's name, the copy of it
        // after F4's face: the copy takes the other face. F2's edge has no
        // copied face.
        let swapped = edge("F3:side(c1)", "F4:end(r{c2})");
        let kept = edge("F2:end(r{c1})", "F3:side(c1)");
        let own = edge("F2:end(r{c1})", "F2:side(c1)");
        let chamfer = FeatureDef::Chamfer(ChamferDef {
            body: "F2.b0".parse().unwrap(),
            sets: vec![two(vec![swapped, kept, own.clone()])],
            corner: ChamferCorner::Chamfer,
        });
        let FeatureDef::Chamfer(copy) = names.definition(&chamfer).unwrap() else {
            panic!("a chamfer");
        };
        let sets: Vec<(Vec<String>, bool)> = copy
            .sets
            .iter()
            .map(|s| (s.edges.iter().map(ToString::to_string).collect(), s.flip))
            .collect();
        assert_eq!(
            sets,
            [
                (
                    vec!["E{F4:end(r{c2})|F6:inst1(F3:side(c1))}".to_owned()],
                    true
                ),
                (
                    vec!["E{F2:end(r{c1})|F6:inst1(F3:side(c1))}".to_owned()],
                    false
                ),
            ]
        );
        // A chamfer of F2's edge only rounds nothing copied by name; found
        // by its place, the edge there is taken.
        let plate = FeatureDef::Chamfer(ChamferDef {
            body: "F2.b0".parse().unwrap(),
            sets: vec![two(vec![own.clone()])],
            corner: ChamferCorner::Chamfer,
        });
        assert!(names.definition(&plate).is_none());
        let there = edge("F2:end(r{c1})", "F2:side(c3)");
        let names = CopyNames {
            placed: [(own, vec![there.clone()])].into(),
            ..names
        };
        let FeatureDef::Chamfer(copy) = names.definition(&plate).unwrap() else {
            panic!("a chamfer");
        };
        assert_eq!(copy.sets[0].edges, [there]);
    }

    #[test]
    fn edges_are_found_by_their_place() {
        let middle = |name: &str, point: [f64; 3]| EdgeMiddle {
            name: name.to_owned(),
            point,
            tangent: [0.0, 1.0, 0.0],
            length: 4.0,
        };
        let original = middle("E{F2:a|F2:b}", [1.0, 2.0, 3.0]);
        let body = [
            middle("E{F2:a|F2:c}", [11.0, 2.0, 3.0]),
            middle("E{F2:a|F2:d}", [21.0, 2.0, 3.0]),
            EdgeMiddle {
                length: 5.0,
                ..middle("E{F2:a|F2:e}", [31.0, 2.0, 3.0])
            },
        ];
        let moved = |x: f64| Transform::translation([x, 0.0, 0.0]);
        let found = |x: f64| -> Vec<String> {
            at_place(&original, &moved(x), &body)
                .iter()
                .map(ToString::to_string)
                .collect()
        };
        assert_eq!(found(10.0), ["E{F2:a|F2:c}"]);
        assert_eq!(found(20.0 + 5e-5), ["E{F2:a|F2:d}"]);
        // Too far, or another length.
        assert!(found(20.001).is_empty());
        assert!(found(30.0).is_empty());
        // Matching names: pieces of faces, numbers.
        let reference = edge("F2:a", "F2:b");
        assert!(edge_matches(&edge("F2:b#1", "F2:a"), &reference));
        assert!(edge_matches(&reference.clone().with_index(2), &reference));
        assert!(!edge_matches(&reference, &reference.clone().with_index(2)));
    }
}
