// SPDX-License-Identifier: MIT
//! Shapes the decoded streams into the dump IR (SCHEMA.md), as the
//! reference decoder does: only what the streams support is written;
//! decoder-only data goes under `_f3d`.

use std::collections::{BTreeMap, HashMap};

use super::classes::{self, *};
use super::decode::{self, Decoded, ParameterEntry, TimelineEntry};
use super::ir::*;
use super::sketch::{self, LightBulb, SketchInfo};
use super::stream::{Segment, f64s_at, header_end, hex, py_sum, str16_at, str16s, u32_at};

/// Name of this decoder in `generator.decoder`.
pub const DECODER_NAME: &str = concat!("mitcad-f3d ", env!("CARGO_PKG_VERSION"));

fn paramref(p: &ParameterEntry) -> Reference {
    Reference::Parameter(ParameterRef {
        name: Some(p.name.clone()),
        expression: Some(p.expression.clone()),
        value: Some(p.value),
        unit: Some(p.unit.clone()),
        ..ParameterRef::default()
    })
}

/// A count that is no parameter (pattern quantities), as a unitless value.
fn count_ref(value: u32) -> Reference {
    Reference::Parameter(ParameterRef {
        expression: Some(value.to_string()),
        value: Some(f64::from(value)),
        unit: Some(String::new()),
        ..ParameterRef::default()
    })
}

/// `(slot, value)` of a feature's integer inputs ([`INT_HOLDER`]), by slot.
/// Like the parameter holders, their root part is only the list
/// `[feature]` (no attribute flag): `u8 1, u32 1, ref`, then `u32 slot,
/// u8 0, u8 1, u32 value`.
fn int_inputs(seg: &Segment, fid: u64) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for h in inputs_of(seg, fid, INT_HOLDER) {
        let d = seg.data_of(h);
        let Some(p) = header_end(d) else {
            continue;
        };
        if d.get(p..p + 5) != Some(&[1, 1, 0, 0, 0][..]) {
            continue;
        }
        let Some(r) = seg.ref_at(d, p + 5) else {
            continue;
        };
        let q = r.end;
        if d.get(q + 4) != Some(&0) || d.get(q + 5) != Some(&1) {
            continue;
        }
        if let (Some(slot), Some(value)) = (u32_at(d, q), u32_at(d, q + 6)) {
            out.push((slot, value));
        }
    }
    out.sort_unstable();
    out
}

/// Ids of the references in an object to objects of class `guid`, once
/// each, in order.
fn inputs_of(seg: &Segment, id: u64, guid: &str) -> Vec<u64> {
    let mut out = Vec::new();
    for r in seg.ref_ids(seg.data_of(id)) {
        if seg.guid_of(r) == Some(guid) && !out.contains(&r) {
            out.push(r);
        }
    }
    out
}

/// A construction plane's light bulb (mitcad#6): in its plane object
/// (`8D424E29`), the reference back to the feature is followed by `u8 0 |
/// u8 1 | str16 name (empty when not renamed) | u32 0 | u8 light bulb` (1
/// on, 0 off). It equals the reference dumps' `isLightBulbOn` for every
/// construction plane of the reference models; the corpus' planes have the
/// same layout. None when it is not found exactly once.
fn plane_light_bulb(seg: &Segment, feature: u64) -> Option<LightBulb> {
    let mut found = None;
    for plane in inputs_of(seg, feature, ORIGIN_PLANE) {
        let d = seg.data_of(plane);
        let Some(start) = header_end(d) else {
            continue;
        };
        for (_, r) in seg.refs_in(d, start, d.len()) {
            if r.id != feature || d.get(r.end..r.end + 2) != Some(&[0, 1][..]) {
                continue;
            }
            let Some((_, e)) = str16_at(d, r.end + 2) else {
                continue;
            };
            if u32_at(d, e) != Some(0) {
                continue;
            }
            let Some(&b) = d.get(e + 4) else {
                continue;
            };
            if b > 1 || found.is_some() {
                return None;
            }
            found = Some(LightBulb {
                on: b == 1,
                raw: d[e..e + 5].to_vec(),
            });
        }
    }
    found
}

/// `XY`/`XZ`/`YZ`, `X`/`Y`/`Z` or `Origin` for origin geometry.
fn origin_name(seg: &Segment, id: u64) -> Option<String> {
    let o = seg.object(id)?;
    let d = seg.data(o);
    match seg.guid(o)? {
        ORIGIN_PLANE => str16s(d)
            .into_iter()
            .find(|s| matches!(s.as_str(), "XY" | "XZ" | "YZ")),
        ORIGIN_AXIS => ["X", "Y", "Z"]
            .into_iter()
            .find(|a| {
                let needle = [1, 0, 0, 0, a.as_bytes()[0]];
                d.windows(5).any(|w| w == needle)
            })
            .map(str::to_string),
        ORIGIN_POINT => Some("Origin".to_string()),
        _ => None,
    }
}

/// Plane object `8D424E29` (section 10.4): `... f64 umin, vmin | u8 0 |
/// f64 origin[3] | u[3] | v[3] | f64 umax, vmax`, found by its shape
/// (unit, orthogonal u and v; increasing bounds).
fn plane_geometry(seg: &Segment, id: u64) -> Option<Geometry> {
    let d = seg.data_of(id);
    let start = header_end(d)?;
    let mut q = start;
    while (q as isize) < d.len() as isize - 8 * 13 {
        let (Some(b), Some(v)) = (f64s_at(d, q, 2), f64s_at(d, q + 17, 11)) else {
            break;
        };
        let (umin, vmin) = (b[0], b[1]);
        let (o, u, w) = ([v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]);
        let (umax, vmax) = (v[9], v[10]);
        let nu = py_sum(u.iter().map(|x| x * x));
        let nv = py_sum(w.iter().map(|x| x * x));
        let dot = py_sum((0..3).map(|i| u[i] * w[i]));
        if (nu - 1.0).abs() < 1e-9
            && (nv - 1.0).abs() < 1e-9
            && dot.abs() < 1e-9
            && d[q + 16] == 0
            && umax > umin
            && vmax > vmin
        {
            let n = [
                u[1] * w[2] - u[2] * w[1],
                u[2] * w[0] - u[0] * w[2],
                u[0] * w[1] - u[1] * w[0],
            ];
            return Some(Geometry {
                geometry_type: Some("Plane".into()),
                origin: Some(o),
                normal: Some(n),
                u_direction: Some(u),
                v_direction: Some(w),
                ..Geometry::default()
            });
        }
        q += 1;
    }
    None
}

/// An occurrence `CE2913AA` (section 12.1): its last component reference
/// (possibly through an assembly context), the transform flag and matrix
/// after it, and its last container (`904F885E`) reference.
#[derive(Clone, Debug, PartialEq)]
pub struct OccurrenceInfo {
    pub object_id: u64,
    pub component: u64,
    /// External document key of a linked component.
    pub context: Option<u64>,
    /// The parent component's child-occurrence container.
    pub container: Option<u64>,
    /// `None`: no transform decoded; `Some(None)`: identity.
    pub matrix: Option<Option<[f64; 16]>>,
}

pub fn occurrence_info(seg: &Segment, id: u64) -> Option<OccurrenceInfo> {
    let d = seg.data_of(id);
    let start = header_end(d)?;
    let refs = seg.refs_in(d, start, d.len());
    let comp = refs
        .iter()
        .rev()
        .find(|(_, r)| seg.guid_of(r.id) == Some(COMPONENT))?
        .1;
    let container = refs
        .iter()
        .rev()
        .find(|(_, r)| seg.guid_of(r.id) == Some(OCCURRENCE_CONTAINER))
        .map(|(_, r)| r.id);
    let q = comp.end;
    let matrix = match d.get(q) {
        Some(1) => Some(None),
        Some(0) if q + 129 <= d.len() => f64s_at(d, q + 1, 16).and_then(|m| {
            let ok = (m[15] - 1.0).abs() < 1e-12 && m[12].abs() + m[13].abs() + m[14].abs() < 1e-12;
            ok.then(|| {
                let mut a = [0.0; 16];
                a.copy_from_slice(&m);
                Some(a)
            })
        }),
        _ => None,
    };
    Some(OccurrenceInfo {
        object_id: id,
        component: comp.id,
        context: comp.context,
        container,
        matrix,
    })
}

fn mat4(m: &Option<[f64; 16]>) -> Mat4 {
    match m {
        None => IDENTITY,
        Some(m) => {
            let mut out = [[0.0; 4]; 4];
            for (r, row) in out.iter_mut().enumerate() {
                for (c, x) in row.iter_mut().enumerate() {
                    *x = m[4 * r + c];
                }
            }
            out
        }
    }
}

/// The component that owns the non-empty timeline: timeline -> feature
/// manager -> component (in the manager's sub-chunk).
fn root_component(seg: &Segment) -> Option<u64> {
    for t in seg.objects_of(TIMELINE) {
        let refs = seg.ref_ids(seg.data(t));
        if refs.len() < 2 || seg.guid_of(refs[0]) != Some(FEATURE_MANAGER) {
            continue;
        }
        if let Some(c) = seg
            .ref_ids(seg.data_of(refs[0]))
            .into_iter()
            .find(|&r| seg.guid_of(r) == Some(COMPONENT))
        {
            return Some(c);
        }
    }
    None
}

/// Names of parameters whose expression mentions `p` (as a name token).
fn dependents(p: &ParameterEntry, params: &[ParameterEntry]) -> Vec<String> {
    params
        .iter()
        .filter(|q| !std::ptr::eq(*q, p) && name_tokens(&q.expression).any(|t| t == p.name))
        .map(|q| q.name.clone())
        .collect()
}

/// `[A-Za-z_][A-Za-z_0-9]*` tokens of an expression, left to right.
fn name_tokens(s: &str) -> impl Iterator<Item = &str> {
    let b = s.as_bytes();
    let mut i = 0;
    std::iter::from_fn(move || {
        while i < b.len() {
            if b[i].is_ascii_alphabetic() || b[i] == b'_' {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                return Some(&s[start..i]);
            }
            i += 1;
        }
        None
    })
}

/// The sketch whose object id a profile source (`4BD53E5A`) holds as text.
fn source_sketch(seg: &Segment, src: u64) -> Option<u64> {
    str16s(seg.data_of(src)).into_iter().find_map(|s| {
        if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let id: u64 = s.parse().ok()?;
        (seg.guid_of(id) == Some(SKETCH)).then_some(id)
    })
}

struct Builder<'a> {
    seg: &'a Segment,
    dec: &'a Decoded,
    pos_of: HashMap<u64, usize>,
    name_of: HashMap<u64, Option<String>>,
    sk_of_feat: HashMap<u64, u64>,
    feat_of_sk: HashMap<u64, u64>,
    local_ids: HashMap<u64, (u64, String)>,
    curve_types: HashMap<(u64, String), String>,
    plane_feature: HashMap<u64, u64>,
}

impl Builder<'_> {
    fn name(&self, f: Option<u64>) -> Option<String> {
        f.and_then(|f| self.name_of.get(&f).cloned().flatten())
    }

    fn pos(&self, f: Option<u64>) -> Option<i64> {
        f.and_then(|f| self.pos_of.get(&f)).map(|&p| p as i64)
    }

    fn feature_ref(&self, fid: u64) -> Reference {
        Reference::Feature(FeatureRef {
            object_type: self
                .seg
                .guid_of(fid)
                .and_then(classes::object_type)
                .map(str::to_string),
            name: Some(self.name(Some(fid))),
            timeline_index: Some(self.pos(Some(fid))),
            ..FeatureRef::default()
        })
    }

    fn plane_ref(&self, entity_ref: u64) -> Option<Reference> {
        let tid = decode::target_id(self.seg, entity_ref)?;
        let on = origin_name(self.seg, tid);
        let geo = if self.seg.guid_of(tid) == Some(ORIGIN_PLANE) {
            plane_geometry(self.seg, tid)
        } else {
            None
        };
        let mut out = if let Some(on) = on {
            ConstructionRef {
                name: Some(Some(on.clone())),
                origin: Some(Some(on)),
                timeline_index: Some(None),
                ..ConstructionRef::default()
            }
        } else if let Some(&f) = self.plane_feature.get(&tid) {
            ConstructionRef {
                name: Some(self.name(Some(f))),
                origin: Some(None),
                timeline_index: Some(self.pos(Some(f))),
                ..ConstructionRef::default()
            }
        } else {
            return None;
        };
        out.geometry = geo;
        Some(Reference::ConstructionPlane(Box::new(out)))
    }

    fn axis_ref(&self, entity_ref: u64) -> Option<Reference> {
        let tid = decode::target_id(self.seg, entity_ref)?;
        if let Some(on) =
            origin_name(self.seg, tid).filter(|o| matches!(o.as_str(), "X" | "Y" | "Z"))
        {
            return Some(Reference::ConstructionAxis(Box::new(ConstructionRef {
                name: Some(Some(on.clone())),
                origin: Some(Some(on)),
                timeline_index: Some(None),
                ..ConstructionRef::default()
            })));
        }
        let (sid, lid) = self.local_ids.get(&tid)?;
        let f = self.feat_of_sk.get(sid).copied();
        Some(Reference::SketchEntity(SketchEntityRef {
            sketch: Some(self.name(f)),
            sketch_timeline_index: Some(self.pos(f)),
            id: Some(Some(lid.clone())),
            object_type: self.curve_types.get(&(*sid, lid.clone())).cloned(),
            ..SketchEntityRef::default()
        }))
    }

    /// One profile reference per selected profile, with the sketch from
    /// the profile source (the profile itself is not decoded).
    fn profiles(&self, fid: u64) -> Option<Vec<Reference>> {
        let seg = self.seg;
        let mut sketch_id = None;
        let sources = inputs_of(seg, fid, PROFILE_SOURCE)
            .into_iter()
            .chain(inputs_of(seg, fid, PROFILE_SOURCE_2));
        for src in sources {
            if let Some(sid) = source_sketch(seg, src) {
                sketch_id = Some(sid);
            }
        }
        let count: usize = inputs_of(seg, fid, PROFILE)
            .into_iter()
            .map(|pr| {
                seg.ref_ids(seg.data_of(pr))
                    .into_iter()
                    .filter(|&r| seg.guid_of(r) == Some(PROFILE_ID))
                    .count()
            })
            .sum();
        let sketch_id = sketch_id?;
        if count == 0 {
            return None;
        }
        let f = self.feat_of_sk.get(&sketch_id).copied();
        let r = Reference::Profile(ProfileRef {
            sketch: Some(self.name(f)),
            sketch_timeline_index: Some(self.pos(f)),
            ..ProfileRef::default()
        });
        Some(vec![r; count])
    }

    fn roles(&self, fid: u64) -> BTreeMap<&str, Vec<&ParameterEntry>> {
        let mut out: BTreeMap<&str, Vec<&ParameterEntry>> = BTreeMap::new();
        for p in &self.dec.parameters {
            if p.owner_feature == Some(fid) {
                out.entry(p.role.as_str()).or_default().push(p);
            }
        }
        out
    }

    fn detail(&self, it: &TimelineEntry, sketches: &HashMap<u64, &SketchInfo>) -> Detail {
        let seg = self.seg;
        let fid = it.id;
        let rl = self.roles(fid);
        let pr = |role: &str| rl.get(role).and_then(|v| v.first()).map(|p| paramref(p));
        let def = |t: &str, f: fn(&mut Definition, Reference), p: Reference| {
            let mut d = Definition {
                definition_type: Some(t.into()),
                ..Definition::default()
            };
            f(&mut d, p);
            d
        };
        let distance = |d: &mut Definition, p| d.distance = Some(p);
        let offset = |d: &mut Definition, p| d.offset = Some(p);
        let angle = |d: &mut Definition, p| d.angle = Some(p);
        match it.type_name {
            Some("SketchFeature") if self.sk_of_feat.contains_key(&fid) => {
                let sid = self.sk_of_feat[&fid];
                let mut sd = sketches
                    .get(&sid)
                    .map(|s| s.detail.clone())
                    .unwrap_or_default();
                sd.name = Some(it.name.clone());
                if let Some(e) = inputs_of(seg, fid, ENTITY_REF).first() {
                    sd.reference_plane = self.plane_ref(*e);
                }
                if sd.reference_plane.is_none() {
                    // A sketch on a profile of another sketch.
                    for src in inputs_of(seg, fid, PROFILE_SOURCE) {
                        if let Some(s2) = source_sketch(seg, src)
                            && s2 != sid
                        {
                            let f = self.feat_of_sk.get(&s2).copied();
                            sd.reference_plane = Some(Reference::Profile(ProfileRef {
                                sketch: Some(self.name(f)),
                                sketch_timeline_index: Some(self.pos(f)),
                                ..ProfileRef::default()
                            }));
                            break;
                        }
                    }
                }
                if sd.reference_plane.is_none() && !inputs_of(seg, fid, FACE_REF).is_empty() {
                    sd.reference_plane = Some(Reference::Face(Box::default()));
                }
                Detail::Sketch(Box::new(sd))
            }
            Some("ExtrudeFeature") => {
                let mut x = ExtrudeDetail::default();
                if let Some(ex) = &it.extrude {
                    x.operation = match ex.operation_code {
                        1 => Some("JoinFeatureOperation".into()),
                        2 => Some("CutFeatureOperation".into()),
                        4 => Some("NewBodyFeatureOperation".into()),
                        _ => None,
                    };
                    x.extent_type = match ex.extent_a {
                        1 => Some("OneSideFeatureExtentType".into()),
                        2 => Some("TwoSidesFeatureExtentType".into()),
                        _ => None,
                    };
                }
                if let Some(p) = pr("AlongDistance") {
                    x.extent_one = Some(def("DistanceExtentDefinition", distance, p));
                } else if let Some(p) = pr("Side1Offset") {
                    x.extent_one = Some(def("ToEntityExtentDefinition", offset, p));
                }
                if let Some(p) = pr("AgainstDistance") {
                    x.extent_two = Some(def("DistanceExtentDefinition", distance, p));
                }
                x.taper_angle_one = pr("TaperAngle");
                x.taper_angle_two = pr("Side2TaperAngle");
                if let Some(p) = pr("ProfileOffset") {
                    x.start_extent = Some(def("OffsetStartDefinition", offset, p));
                }
                x.profile = self.profiles(fid);
                Detail::Extrude(Box::new(x))
            }
            Some("RevolveFeature") => {
                let mut x = RevolveDetail::default();
                if let Some(p) = pr("AlongAngle") {
                    x.extent_definition = Some(def("AngleExtentDefinition", angle, p));
                }
                for e in inputs_of(seg, fid, ENTITY_REF) {
                    if let Some(a) = self.axis_ref(e) {
                        x.axis = Some(a);
                    }
                }
                x.profile = self.profiles(fid);
                Detail::Revolve(Box::new(x))
            }
            Some("FilletFeature") => {
                let mut x = FilletDetail::default();
                if let Some(radii) = rl.get("Radius").filter(|r| !r.is_empty()) {
                    x.edge_sets = Some(
                        radii
                            .iter()
                            .map(|p| EdgeSet {
                                radius: Some(paramref(p)),
                                ..EdgeSet::default()
                            })
                            .collect(),
                    );
                }
                Detail::Fillet(Box::new(x))
            }
            Some("ChamferFeature") => {
                let mut x = ChamferDetail::default();
                if rl.contains_key("Distance 1") && rl.contains_key("Distance 2") {
                    x.chamfer_type = Some("TwoDistancesChamferType".into());
                    x.edge_sets = Some(vec![EdgeSet {
                        distance_one: pr("Distance 1"),
                        distance_two: pr("Distance 2"),
                        ..EdgeSet::default()
                    }]);
                } else if rl.contains_key("Rotate Angle") && rl.contains_key("Distance") {
                    x.chamfer_type = Some("DistanceAndAngleChamferType".into());
                    x.edge_sets = Some(vec![EdgeSet {
                        distance: pr("Distance"),
                        angle: pr("Rotate Angle"),
                        ..EdgeSet::default()
                    }]);
                } else if let Some(ds) = rl.get("Distance").filter(|d| !d.is_empty()) {
                    x.chamfer_type = Some("EqualDistanceChamferType".into());
                    x.edge_sets = Some(
                        ds.iter()
                            .map(|p| EdgeSet {
                                distance: Some(paramref(p)),
                                ..EdgeSet::default()
                            })
                            .collect(),
                    );
                }
                Detail::Chamfer(Box::new(x))
            }
            Some("HoleFeature") => Detail::Hole(Box::new(HoleDetail {
                hole_diameter: pr("HoleDiameter"),
                tip_angle: pr("TipAngle"),
                countersink_diameter: pr("CSDiameter"),
                countersink_angle: pr("CSAngle"),
                extent_definition: pr("HoleDepth")
                    .map(|p| def("DistanceExtentDefinition", distance, p)),
                ..HoleDetail::default()
            })),
            Some("ThreadFeature") => Detail::Thread(Box::new(ThreadDetail {
                thread_length: pr("ThreadLength"),
                thread_offset: pr("ThreadOffset"),
                ..ThreadDetail::default()
            })),
            Some("CircularPatternFeature") => {
                let mut x = CircularPatternDetail {
                    total_angle: pr("TotalAngle"),
                    ..CircularPatternDetail::default()
                };
                for (slot, value) in int_inputs(seg, fid) {
                    if slot == 0 {
                        x.quantity = Some(count_ref(value));
                    }
                }
                for e in inputs_of(seg, fid, ENTITY_REF) {
                    if let Some(a) = self.axis_ref(e) {
                        x.axis = Some(a);
                    }
                }
                Detail::CircularPattern(Box::new(x))
            }
            Some("RectangularPatternFeature") => {
                let mut x = RectangularPatternDetail {
                    distance_one: pr("uSpaceDistance"),
                    distance_two: pr("vSpaceDistance"),
                    ..RectangularPatternDetail::default()
                };
                for (slot, value) in int_inputs(seg, fid) {
                    match slot {
                        0 => x.quantity_one = Some(count_ref(value)),
                        1 => x.quantity_two = Some(count_ref(value)),
                        _ => {}
                    }
                }
                Detail::RectangularPattern(Box::new(x))
            }
            Some("MirrorFeature") => {
                // The mirror plane when it is an origin or construction
                // plane (the entity reference, as for sketches).
                let mut map = serde_json::Map::new();
                for e in inputs_of(seg, fid, ENTITY_REF) {
                    if let Some(p) = self.plane_ref(e) {
                        map.insert(
                            "mirrorPlane".to_owned(),
                            serde_json::to_value(p).expect("serializes"),
                        );
                    }
                }
                Detail::Other(map)
            }
            Some("ShellFeature") => Detail::Shell(Box::new(ShellDetail {
                inside_thickness: pr("innerThickness"),
                ..ShellDetail::default()
            })),
            Some("OffsetFacesFeature") => Detail::OffsetFaces(Box::new(OffsetFacesDetail {
                distance: pr("distance"),
                ..OffsetFacesDetail::default()
            })),
            Some("CoilFeature") => Detail::Coil(Box::new(CoilDetail {
                diameter: pr("Diameter"),
                pitch: pr("Pitch"),
                revolutions: pr("Revolutions"),
                section_size: pr("SectionSize"),
                angle: pr("TaperAngle"),
                ..CoilDetail::default()
            })),
            Some("PipeFeature") => Detail::Pipe(Box::new(PipeDetail {
                section_size: pr("SectionSize"),
                section_thickness: pr("SectionThickness"),
                distance_one: pr("AlongDistance"),
                distance_two: pr("AgainstDistance"),
                ..PipeDetail::default()
            })),
            Some("SweepFeature") => Detail::Sweep(Box::new(SweepDetail {
                distance_one: pr("AlongDistance"),
                distance_two: pr("AgainstDistance"),
                taper_angle: pr("TaperAngle"),
                twist_angle: pr("TwistAngle"),
                ..SweepDetail::default()
            })),
            Some("ConstructionPlane") => {
                let mut x = ConstructionPlaneDetail::default();
                if let Some(p) = pr("AlongDistance") {
                    x.definition = Some(def("ConstructionPlaneOffsetDefinition", offset, p));
                } else if let Some(p) = pr("RotateAngle") {
                    x.definition = Some(def("ConstructionPlaneAtAngleDefinition", angle, p));
                } else if let Some(p) = pr("PathDistance") {
                    x.definition = Some(def(
                        "ConstructionPlaneDistanceOnPathDefinition",
                        distance,
                        p,
                    ));
                }
                if let Some(r) = seg
                    .ref_ids(seg.data_of(fid))
                    .into_iter()
                    .find(|&r| seg.guid_of(r) == Some(ORIGIN_PLANE))
                {
                    x.geometry = plane_geometry(seg, r);
                }
                Detail::ConstructionPlane(Box::new(x))
            }
            Some("JointOriginFeature") => Detail::JointOrigin(Box::new(JointOriginDetail {
                offset_x: pr("OffsetX"),
                offset_y: pr("OffsetY"),
                offset_z: pr("OffsetZ"),
                angle: pr("AngleZ"),
                ..JointOriginDetail::default()
            })),
            _ => Detail::empty(),
        }
    }
}

/// The sketch dimension that a parameter holder belongs to.
struct DimensionOwner {
    /// The sketch feature.
    feature: Option<u64>,
    /// Sketch-local id (`d<i>`).
    id: Option<String>,
    dimension_type: Option<String>,
}

/// Builds the SCHEMA.md dump of a decoded design segment. `file` and
/// `segment_dir` go to `source`.
pub fn build(seg: &Segment, dec: &Decoded, file: &str, segment_dir: &str) -> Dump {
    let items = &dec.timeline;
    let mut b = Builder {
        seg,
        dec,
        pos_of: items.iter().map(|it| (it.id, it.pos)).collect(),
        name_of: items.iter().map(|it| (it.id, it.name.clone())).collect(),
        sk_of_feat: HashMap::new(),
        feat_of_sk: HashMap::new(),
        local_ids: HashMap::new(),
        curve_types: HashMap::new(),
        plane_feature: HashMap::new(),
    };
    for s in seg.objects_of(SKETCH) {
        for r in seg.ref_ids(seg.data(s)) {
            if seg.guid_of(r) == Some(SKETCH_FEATURE) {
                b.sk_of_feat.insert(r, s.id);
                b.feat_of_sk.insert(s.id, r);
            }
        }
    }
    let (mut sketch_infos, local_ids) = sketch::sketch_details(seg);
    b.local_ids = local_ids;
    for s in &sketch_infos {
        for c in s.detail.curves.iter().flatten() {
            if let (Some(id), Some(t)) = (&c.id, &c.curve_type) {
                b.curve_types.insert((s.id, id.clone()), t.clone());
            }
        }
    }
    for f in seg.objects_of(CONSTRUCTION_PLANE) {
        for r in seg.ref_ids(seg.data(f)) {
            if seg.guid_of(r) == Some(ORIGIN_PLANE) {
                b.plane_feature.insert(r, f.id);
            }
        }
    }

    // Dimension parameters: holder -> (sketch feature, dimension id, type).
    let mut dim_of_holder: HashMap<u64, DimensionOwner> = HashMap::new();
    for s in &sketch_infos {
        let f = b.feat_of_sk.get(&s.id).copied();
        for (dm, h) in s.detail.dimensions.iter().flatten().zip(&s.holders) {
            if let Some(h) = h {
                let owner = DimensionOwner {
                    feature: f,
                    id: dm.id.clone(),
                    dimension_type: dm.dimension_type.clone(),
                };
                dim_of_holder.insert(*h, owner);
            }
        }
    }
    let mut param_of_holder = HashMap::new();
    for p in &dec.parameters {
        if let Some(h) = p.holder {
            param_of_holder.insert(h, p);
        }
    }
    for s in &mut sketch_infos {
        let holders = s.holders.clone();
        for (dm, h) in s.detail.dimensions.iter_mut().flatten().zip(holders) {
            if let Some(p) = h.and_then(|h| param_of_holder.get(&h)) {
                dm.parameter = Some(Some(paramref(p)));
            }
        }
    }
    let sketches: HashMap<u64, &SketchInfo> = sketch_infos.iter().map(|s| (s.id, s)).collect();

    let mut tl_items = Vec::new();
    for it in items {
        let mut f3d = ItemF3d {
            class: Some(it.class.clone()),
            class_version: Some(it.class_version),
            object_id: Some(it.id),
            ..ItemF3d::default()
        };
        if let Some(t) = &it.tail {
            f3d.base_name = Some(t.base_name.clone());
            f3d.index = Some(t.index);
            f3d.custom_name = Some(t.custom_name.clone());
            f3d.result_no = Some(t.result_no);
            f3d.flags = Some(super::stream::hex(&t.flags));
            f3d.sub_features = Some(t.sub_features.clone());
        }
        if let Some(ex) = &it.extrude {
            f3d.extrude = Some(ExtrudeF3d {
                operation_code: Some(ex.operation_code),
                operation: Some(ex.operation().map(str::to_string)),
                extent_a: Some(ex.extent_a),
                extent_b: Some(ex.extent_b),
                direction: Some(ex.direction),
                ..ExtrudeF3d::default()
            });
        }
        // The light bulb (mitcad#6), where it is decoded; beyond the
        // reference decoder, so checked against the reference dumps of the
        // reference models instead (tests/design_models.rs).
        let bulb = match it.type_name {
            Some("SketchFeature") => b
                .sk_of_feat
                .get(&it.id)
                .and_then(|s| seg.object(*s))
                .and_then(|s| sketch::light_bulb(seg, s)),
            Some("ConstructionPlane") => plane_light_bulb(seg, it.id),
            _ => None,
        };
        let props = bulb.map(|bulb| {
            f3d.light_bulb = Some(hex(&bulb.raw));
            serde_json::json!({"isLightBulbOn": bulb.on})
        });
        tl_items.push(TimelineItem {
            index: Some(it.pos as i64),
            name: Some(it.name.clone()),
            object_type: it
                .type_name
                .and_then(|n| classes::OBJECT_TYPES.iter().find(|(k, _)| *k == n))
                .map(|(_, t)| Some(t.to_string())),
            detail: Some(b.detail(it, &sketches)),
            props,
            f3d: Some(f3d),
            ..TimelineItem::default()
        });
    }

    // Parameters: user and model, createdBy for model parameters.
    let (mut user, mut model) = (Vec::new(), Vec::new());
    for p in &dec.parameters {
        let mut rec = Parameter {
            name: Some(p.name.clone()),
            expression: Some(p.expression.clone()),
            value: Some(p.value),
            unit: Some(p.unit.clone()),
            comment: p.comment.clone(),
            dependents: Some(dependents(p, &dec.parameters)),
            ..Parameter::default()
        };
        let Some(h) = p.holder else {
            user.push(rec);
            continue;
        };
        rec.role = Some(p.role.clone());
        if let Some(DimensionOwner {
            feature: f,
            id,
            dimension_type: typ,
        }) = dim_of_holder.get(&h)
        {
            rec.created_by = Some(Reference::SketchDimension(SketchEntityRef {
                sketch: Some(b.name(*f)),
                sketch_timeline_index: Some(b.pos(*f)),
                id: Some(id.clone()),
                object_type: typ.clone(),
                ..SketchEntityRef::default()
            }));
        } else if let Some(f) = p.owner_feature {
            rec.created_by = Some(b.feature_ref(f));
        }
        model.push(rec);
    }

    // Components and occurrences.
    let comps = &dec.components;
    let mut root_id = root_component(seg);
    let infos: Vec<OccurrenceInfo> = seg
        .objects_of(OCCURRENCE)
        .filter_map(|o| occurrence_info(seg, o.id))
        .collect();
    if root_id.is_none() {
        // No parametric timeline: the root occurrence names the root component.
        root_id = infos
            .iter()
            .find(|i| i.container.is_none() && i.context.is_none())
            .map(|i| i.component);
    }
    let root = comps
        .iter()
        .find(|c| Some(c.id) == root_id)
        .and_then(|c| c.name.clone())
        .filter(|n| !n.is_empty());
    let xrefs: HashMap<String, String> = seg
        .meta
        .external
        .iter()
        .map(|x| (format!("{:016x}", x.key), x.urn.clone()))
        .collect();
    let comp_names: HashMap<u64, Option<String>> =
        comps.iter().map(|c| (c.id, c.name.clone())).collect();
    let occurrences = occurrence_tree(seg, &infos, &comp_names, &xrefs, root_id);

    let format = Format {
        meta_magic: Some(seg.meta.magic),
        meta_version: Some(seg.meta.version.clone()),
        writer_build: Some(seg.meta.writer_build()),
        writer: Some(seg.meta.writer()),
        long_refs: Some(seg.long_refs),
        classes: Some(seg.meta.classes.len() as u64),
        objects: Some(seg.objects.len() as u64),
        bulk_bytes: Some(seg.bulk.len() as u64),
        ..Format::default()
    };
    let cv = &dec.coverage;
    let f3d = DumpF3d {
        format: Some(format),
        next_object_id: Some(seg.meta.next_id),
        external_documents: Some(
            seg.meta
                .external
                .iter()
                .map(|x| ExternalDocument {
                    key: Some(format!("{:016x}", x.key)),
                    urn: Some(x.urn.clone()),
                    ..ExternalDocument::default()
                })
                .collect(),
        ),
        feature_set_versions: Some(seg.meta.feature_versions.clone()),
        brep_blobs: Some(
            dec.brep_blobs
                .iter()
                .map(|b| BrepBlob {
                    id: Some(b.id),
                    file: Some(b.file.clone()),
                    holder: b.holder,
                    component: b.holder.map(|_| b.component),
                    ..BrepBlob::default()
                })
                .collect(),
        ),
        coverage: Some(Coverage {
            bulk_bytes: Some(cv.bulk_bytes),
            framing_bytes: Some(cv.framing_bytes),
            root_part_ok: Some(cv.root_part_ok),
            objects: Some(cv.objects),
            known_class_bytes: Some(cv.known_class_bytes),
            decoded_bytes: Some(cv.decoded_bytes),
            token_bytes: Some(cv.token_bytes),
            ..Coverage::default()
        }),
        timelines_total: Some(dec.timelines_total as u64),
        parameter_failures: Some(dec.parameter_failures as u64),
        ..DumpF3d::default()
    };

    let units = [
        ("length", "cm"),
        ("angle", "rad"),
        ("area", "cm^2"),
        ("volume", "cm^3"),
        ("mass", "kg"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let mut dump = Dump {
        schema: Some(SCHEMA_NAME.into()),
        schema_version: Some(SCHEMA_VERSION),
        generator: Some(Generator {
            decoder: Some(DECODER_NAME.into()),
            writer: Some(seg.meta.writer()),
            ..Generator::default()
        }),
        units: Some(units),
        source: Some(Source {
            mode: Some("f3d_stream".into()),
            file: Some(file.into()),
            segment: Some(segment_dir.into()),
            ..Source::default()
        }),
        document: Some(Document {
            design_type: Some(if items.is_empty() {
                "DirectDesignType".into()
            } else {
                "ParametricDesignType".into()
            }),
            component_count: Some(comps.len() as i64),
            root_component: root,
            ..Document::default()
        }),
        parameters: Some(Parameters {
            user: Some(user),
            model: Some(model),
            ..Parameters::default()
        }),
        components: Some(
            comps
                .iter()
                .map(|c| Component {
                    name: Some(c.name.clone()),
                    is_root: root_id.filter(|&r| r != 0).map(|r| r == c.id),
                    f3d: Some(ComponentF3d {
                        object_id: Some(c.id),
                        ..ComponentF3d::default()
                    }),
                    ..Component::default()
                })
                .collect(),
        ),
        occurrences,
        timeline: Some(Timeline {
            available: Some(!items.is_empty()),
            count: Some(Some(items.len() as i64)),
            items: Some(tl_items),
            ..Timeline::default()
        }),
        errors: Some(Vec::new()),
        f3d: Some(f3d),
        ..Dump::default()
    };
    let bad = super::nonfinite::paths(&dump);
    if !bad.is_empty()
        && let Some(f) = dump.f3d.as_mut()
    {
        f.non_finite = Some(bad);
    }
    dump
}

/// The occurrence tree under the root component (SCHEMA.md §5.2). The
/// stored matrix is relative to the parent component: it is the
/// `transform` of top-level occurrences; nested ones keep it as
/// `_f3d.local_transform` (full-path composition not verified).
fn occurrence_tree(
    seg: &Segment,
    infos: &[OccurrenceInfo],
    comp_names: &HashMap<u64, Option<String>>,
    xrefs: &HashMap<String, String>,
    mut root_id: Option<u64>,
) -> Option<Vec<OccurrenceNode>> {
    let mut owner = HashMap::new();
    for c in seg.objects_of(OCCURRENCE_CONTAINER) {
        for r in seg.ref_ids(seg.data(c)) {
            if seg.guid_of(r) == Some(COMPONENT) {
                owner.insert(c.id, r);
            }
        }
    }
    let mut by_parent: HashMap<Option<u64>, Vec<&OccurrenceInfo>> = HashMap::new();
    for i in infos {
        match i.container {
            None => {
                if root_id.is_none() {
                    root_id = Some(i.component);
                }
            }
            Some(c) => by_parent.entry(owner.get(&c).copied()).or_default().push(i),
        }
    }

    let root_id = root_id?;
    let tree = Tree {
        by_parent,
        comp_names,
        xrefs,
        nodes: std::cell::Cell::new(0),
    };
    Some(
        tree.by_parent
            .get(&Some(root_id))
            .map_or(&[][..], |v| v)
            .iter()
            .map(|i| tree.node(i, 0, true))
            .collect(),
    )
}

/// Occurrences by parent component, for [`occurrence_tree`].
struct Tree<'a> {
    by_parent: HashMap<Option<u64>, Vec<&'a OccurrenceInfo>>,
    comp_names: &'a HashMap<u64, Option<String>>,
    xrefs: &'a HashMap<String, String>,
    /// Nodes made so far: a cyclic file cannot blow the tree up.
    nodes: std::cell::Cell<usize>,
}

/// At most this many occurrence nodes.
const MAX_OCCURRENCE_NODES: usize = 100_000;

impl Tree<'_> {
    fn node(&self, i: &OccurrenceInfo, depth: usize, top: bool) -> OccurrenceNode {
        let ext = i.context.is_some();
        let mut f3d = OccurrenceF3d {
            object_id: Some(i.object_id),
            component_object: Some(i.component),
            ..OccurrenceF3d::default()
        };
        let mut rec = OccurrenceNode {
            is_referenced_component: Some(ext),
            ..OccurrenceNode::default()
        };
        if !ext && let Some(n) = self.comp_names.get(&i.component) {
            rec.component = Some(n.clone());
        }
        if let Some(key) = i.context.map(|c| format!("{c:016x}")) {
            f3d.external_document = self.xrefs.get(&key).cloned();
            f3d.external_key = Some(key);
        }
        if let Some(m) = &i.matrix {
            if top {
                rec.transform = Some(mat4(m));
            } else {
                f3d.local_transform = Some(mat4(m));
            }
        }
        self.nodes.set(self.nodes.get() + 1);
        let full = self.nodes.get() >= MAX_OCCURRENCE_NODES;
        let kids: &[&OccurrenceInfo] = if ext || depth > 32 || full {
            &[]
        } else {
            self.by_parent.get(&Some(i.component)).map_or(&[], |v| v)
        };
        rec.children = Some(
            kids.iter()
                .map(|k| self.node(k, depth + 1, false))
                .collect(),
        );
        rec.f3d = Some(f3d);
        rec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_tokens_follow_the_regex() {
        let t: Vec<&str> = name_tokens("d1 * 2 + 1e5 mm/_x9").collect();
        assert_eq!(t, vec!["d1", "e5", "mm", "_x9"]);
        let t: Vec<&str> = name_tokens("12ab3 (Länge)").collect();
        assert_eq!(t, vec!["ab3", "L", "nge"]);
    }
}
