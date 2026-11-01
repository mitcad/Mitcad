// SPDX-License-Identifier: MIT
//! The file's features as Mitcad features (the table in
//! `commands.md`, *.solidcraft import*). Each feature's inputs are its
//! slots' parameters; references by point are found on the bodies
//! ([`super::refs`]); bodies by the file's names.

use std::collections::BTreeMap;

use mitcad_model::{BodyUid, ComponentUid, FeatureUid, Kernel};
use mitcad_solidcraft::Feature;
use mitcad_solidcraft::inputs::{self, Kind};
use mitcad_solidcraft::kinds::{self as k, Direction, ExtrudeConvention, Operation};
use serde_json::{Value, json};

use super::planes::Frame;
use super::{Importer, MapResult, Mapped, NotMapped};
use mitcad_import_base::geom::{cross, dot, unit};

impl<K: Kernel> Importer<'_, '_, K> {
    /// The parameter name and expression of the input at `pointer`.
    pub(super) fn input(&self, f: &Feature, pointer: &str) -> (Option<String>, Option<String>) {
        let ins = inputs::inputs(&f.def);
        match ins.iter().position(|i| i.pointer == pointer) {
            Some(k) => (
                f.param_names.get(k).filter(|n| !n.is_empty()).cloned(),
                Some(ins[k].expr.clone()),
            ),
            None => (None, f.def.pointer(pointer).and_then(inputs::expr_text)),
        }
    }

    /// The slot value of the input at `pointer`.
    pub(super) fn slot(&mut self, f: &Feature, pointer: &str) -> Result<String, String> {
        let (name, expr) = self.input(f, pointer);
        let expr = expr.ok_or_else(|| format!("{} has no {pointer}", f.name))?;
        self.params.slot(self.doc, name.as_deref(), &expr)
    }

    /// The slot value of the input at `pointer` through `g` (Mitcad's
    /// slot means `g` of the file's value).
    pub(super) fn slot_of(
        &mut self,
        f: &Feature,
        pointer: &str,
        g: impl Fn(&str) -> String,
    ) -> Result<String, String> {
        let (name, expr) = self.input(f, pointer);
        let expr = expr.ok_or_else(|| format!("{} has no {pointer}", f.name))?;
        self.params.slot_of(self.doc, name.as_deref(), &expr, g)
    }

    /// The value of an expression now (mm, radians).
    pub(super) fn eval(&mut self, expr: &str, kind: Kind) -> Result<f64, String> {
        self.params.value(self.doc, expr, kind)
    }

    /// Bodies by the file's names, as Mitcad ids.
    pub(super) fn body_ids(&self, names: &[String]) -> Result<Vec<String>, String> {
        names
            .iter()
            .filter(|n| n.as_str() != "*")
            .map(|n| self.body(n).map(|b| b.to_string()))
            .collect()
    }

    /// The Mitcad features of the file's features by name (for patterns).
    fn features_named(&self, names: &[String]) -> Result<Vec<String>, String> {
        names
            .iter()
            .map(|n| {
                let f = self.file_feature_named(n)?;
                self.made
                    .get(&f.id)
                    .map(|m| {
                        m.features
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                    })
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| format!("{n} did not come in"))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|v| v.into_iter().flatten().collect())
    }

    /// A feature's operation and participants.
    fn operation(&self, op: Operation, targets: &[String]) -> Result<(Value, Value), String> {
        Ok((json!(op.as_str()), json!(self.body_ids(targets)?)))
    }

    /// The face at a point as a `{"type": "face", ...}` object of an extent.
    fn object_at(
        &self,
        component: ComponentUid,
        p: [f64; 3],
        normal: [f64; 3],
        name: Option<&str>,
    ) -> Value {
        match self.face_named(component, None, p, name) {
            Ok((body, face)) => json!({"type": "face", "body": body.to_string(), "face": face}),
            // A vertex or a point: the plane through it across the direction.
            Err(_) => json!({"type": "plane", "plane": {"origin": p, "normal": normal}}),
        }
    }
}

thread_local! {
    pub(crate) static CURRENT: std::cell::RefCell<Option<Feature>> = const { std::cell::RefCell::new(None) };
}

/// Maps one feature of the file.
pub(super) fn map<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    CURRENT.with(|c| *c.borrow_mut() = Some(f.clone()));
    let result = map_kind(im, f, component);
    if result.is_err() {
        im.params.forget_pending();
    }
    CURRENT.with(|c| *c.borrow_mut() = None);
    result
}

fn map_kind<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let mut notes = Vec::new();
    if f.participants.iter().any(|p| p == "*")
        && !im.design.components.is_empty()
        && matches!(
            f.def.get("operation").and_then(Value::as_str),
            Some("join" | "cut" | "intersect")
        )
    {
        notes.push(
            "reaches only its own component's bodies (the file lets it reach every body)"
                .to_owned(),
        );
    }
    let mut mapped = match f.kind.as_str() {
        "sketch" => im.sketch(f, component)?,
        "extrude" => extrude(im, f, component)?,
        "revolve" => revolve(im, f, component)?,
        "fillet" => fillet(im, f, component)?,
        "chamfer" => chamfer(im, f, component)?,
        "box" | "cylinder" | "sphere" | "torus" => primitive(im, f, component)?,
        "combine" => combine(im, f, component)?,
        "construction_plane" => construction_plane(im, f, component)?,
        "construction_axis" => construction_axis(im, f, component)?,
        "construction_point" => construction_point(im, f, component)?,
        "shell" => shell(im, f, component)?,
        "draft" => draft(im, f, component)?,
        "hole" => hole(im, f, component)?,
        "thread" => thread(im, f, component)?,
        "pattern" => pattern(im, f, component)?,
        "mirror" => mirror(im, f, component)?,
        "move" => move_bodies(im, f, component)?,
        "scale" => scale_bodies(im, f, component)?,
        "align" => align(im, f, component)?,
        "split" => split(im, f, component)?,
        "split_face" => split_face(im, f, component)?,
        "offset_face" | "delete_face" | "replace_face" => face_feature(im, f, component)?,
        "remove" => remove(im, f, component)?,
        "loft" => loft(im, f, component)?,
        "sweep" => sweep(im, f, component)?,
        "pipe" => pipe(im, f, component)?,
        "coil" => coil(im, f, component)?,
        "rib" => rib(im, f, component)?,
        "emboss" => emboss(im, f, component)?,
        "import" => im.import_step(f, component)?,
        "mesh_import" => im.import_meshes(f, component)?,
        "body_transfer" => im.body_transfer(f, component)?,
        // Boundary Fill (mitcad#457).
        "boundary_fill" => boundary_fill(im, f, component)?,
        // Convert Mesh (mitcad#462).
        "mesh_convert" => im.convert_mesh(f, component)?,
        kind if kind.starts_with("sheet_") => im.sheet_feature(f, component)?,
        "bounding_solid" => bounding_solid(im, f, component)?,
        // Patch, Stitch and Thicken (mitcad#456).
        "patch" | "stitch" | "thicken" => im.surface_feature(f, component)?,
        // Surface modelling (mitcad#455).
        "surface_extrude" | "surface_revolve" | "surface_sweep" | "surface_loft"
        | "surface_offset" | "surface_ruled" | "surface_extend" | "surface_trim"
        | "surface_untrim" | "surface_blend" | "surface_boundary" | "surface_network"
        | "surface_match" | "surface_fair" | "unstitch" | "reverse_normal" => {
            im.surface_modelling(f, component)?
        }
        // Plastic part features (mitcad#459).
        "boss" => super::plastic::boss(im, f, component)?,
        "lip" => super::plastic::lip(im, f, component)?,
        "snap_fit" => super::plastic::snap_fit(im, f, component)?,
        "rest" => super::plastic::rest(im, f, component)?,
        other => {
            return Err(NotMapped::Unmapped(format!(
                "`{other}` is not a feature kind this import knows"
            )));
        }
    };
    if !notes.is_empty() {
        notes.append(&mut mapped.notes);
        mapped = Mapped::partial(notes);
    }
    Ok(mapped)
}

fn extrude<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let e: k::Extrude = f.typed()?;
    let def = extrude_def(im, f, component, &e, None)?;
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::partial(Vec::new()))
}

/// The definition of an extrude; `profiles` instead of the file's
/// selection (a surface extrude's, mitcad#455).
pub(super) fn extrude_def<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
    e: &k::Extrude,
    profiles: Option<Vec<Value>>,
) -> Result<Value, NotMapped> {
    let sketch = im
        .sketches
        .get(&e.sketch)
        .cloned()
        .ok_or("the sketch did not come in")?;
    // A thin extrude's wall follows its curves; its profiles are not used.
    let (profiles, thin) = match (&e.thin, profiles) {
        (Some(t), _) => (Vec::new(), Some(thin_wall(im, f, t, &sketch)?)),
        (None, Some(profiles)) => (profiles, None),
        (None, None) => (im.profiles(e.sketch, &e.profiles)?, None),
    };
    let normal = sketch.frame.normal();
    let x = &e.extent;
    let taper = match x.taper {
        Some(_) => Some(im.slot(f, "/extent/taper")?),
        None => None,
    };
    let with_taper = |mut v: Value, taper: &Option<String>| {
        if let Some(t) = taper {
            v["taper"] = json!(t);
        }
        v
    };
    let mut flip = false;
    let extent = if x.through_all {
        match x.direction {
            Direction::Symmetric => json!({"type": "through_all", "both_sides": true}),
            Direction::Negative => {
                flip = true;
                json!({"type": "through_all"})
            }
            Direction::Positive => json!({"type": "through_all"}),
        }
    } else if let Some(to) = x.to {
        // Towards the object, whichever way the direction says (SolidCraft
        // extrudes to an object below the sketch too).
        flip = dot(mitcad_import_base::geom::sub(to, sketch.frame.o), normal) < 0.0;
        let mut object = json!({"type": "to_object", "object": im.object_at(component, to, normal, f.face_names.first().map(String::as_str))});
        if x.to_offset.is_some() {
            object["offset"] = json!(im.slot(f, "/extent/to_offset")?);
        }
        with_taper(object, &taper)
    } else if x.distance2.is_some() {
        if x.direction == Direction::Negative {
            flip = true;
        }
        let d1 = im.slot(f, "/extent/distance")?;
        let d2 = im.slot(f, "/extent/distance2")?;
        json!({"type": "two_sides",
            "side1": with_taper(json!({"type": "distance", "distance": d1}), &taper),
            "side2": with_taper(json!({"type": "distance", "distance": d2}), &taper)})
    } else {
        let d = im.slot(f, "/extent/distance")?;
        match x.direction {
            Direction::Positive => with_taper(json!({"type": "distance", "distance": d}), &taper),
            Direction::Negative => {
                flip = true;
                with_taper(json!({"type": "distance", "distance": d}), &taper)
            }
            Direction::Symmetric => with_taper(
                json!({"type": "symmetric", "distance": d,
                    "full_length": x.convention == ExtrudeConvention::TotalDistance}),
                &taper,
            ),
        }
    };
    let (operation, participants) = im.operation(e.operation, &e.targets)?;
    let mut def = json!({"type": "extrude", "extent": extent, "flip": flip,
        "operation": operation, "participants": participants});
    match thin {
        Some(thin) => def["thin"] = thin,
        None => def["profiles"] = json!(profiles),
    }
    if x.start_offset.is_some() {
        def["start"] = json!({"type": "offset", "offset": im.slot(f, "/extent/start_offset")?});
    }
    if let Some(from) = x.from {
        let index = usize::from(x.to.is_some());
        let object = im.object_at(
            component,
            from,
            normal,
            f.face_names.get(index).map(String::as_str),
        );
        let offset = match x.start_offset {
            Some(_) => json!(im.slot(f, "/extent/start_offset")?),
            None => json!(im.params.unnamed(im.doc, "0")),
        };
        def["start"] = json!({"type": "object", "object": object, "offset": offset});
    }
    Ok(def)
}

/// A thin extrude's wall (mitcad#463) as Mitcad's `thin` along the curves.
/// The file's inside of a wall (the area a closed chain encloses, the side
/// an open one bends toward) is side 2 of Mitcad's, which orders the
/// chains the same way; its outside is side 1.
fn thin_wall<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    t: &k::ThinWall,
    sketch: &super::sketch::SketchMade,
) -> Result<Value, String> {
    if t.curves.is_empty() {
        return Err("the thin extrude has no curves".to_owned());
    }
    let curves = t
        .curves
        .iter()
        .map(|c| sketch.curve(c))
        .collect::<Result<Vec<_>, _>>()?;
    let mut wall = match t.side {
        k::ThinSide::Center => {
            json!({"location": "center", "thickness": im.slot(f, "/thin/thickness")?})
        }
        k::ThinSide::Inside => {
            json!({"location": "side2", "thickness": im.slot(f, "/thin/thickness")?})
        }
        k::ThinSide::Outside => {
            json!({"location": "side1", "thickness": im.slot(f, "/thin/thickness")?})
        }
        // `thickness` inside, `thickness2` outside.
        k::ThinSide::TwoSides => json!({"location": "both",
            "thickness": im.slot(f, "/thin/thickness2")?,
            "thickness2": im.slot(f, "/thin/thickness")?}),
    };
    wall["curves"] = json!({"sketch": sketch.uid.to_string(), "curves": curves});
    Ok(wall)
}

/// The axis of a revolve in the sketch: a reference and its direction in
/// component coordinates (from a point on it).
fn revolve_axis<K: Kernel>(
    im: &Importer<'_, '_, K>,
    sketch: u64,
    axis: &k::AxisRef,
) -> Result<(Value, [f64; 3], [f64; 3]), String> {
    let s = im
        .sketches
        .get(&sketch)
        .ok_or("the sketch did not come in")?;
    let frame = s.frame;
    Ok(match axis {
        k::AxisRef::SketchLine { curve } => {
            let i = s
                .sketch
                .curve_index(curve)
                .ok_or_else(|| format!("no line {curve}"))?;
            let mitcad_sketch::CurveKind::Line { a, b } = s.sketch.curves[i].kind else {
                return Err(format!("{curve} is not a line"));
            };
            let (pa, pb) = (s.sketch.points[a].pos, s.sketch.points[b].pos);
            let o = frame.point(pa);
            let d = frame.direction([pb[0] - pa[0], pb[1] - pa[1]]);
            (
                json!({"sketch": s.uid.to_string(), "curve": s.curve(curve)?}),
                o,
                d,
            )
        }
        k::AxisRef::SketchAxis { axis } => {
            let d = if axis.eq_ignore_ascii_case("y") {
                frame.y
            } else {
                frame.x
            };
            (json!({"origin": frame.o, "direction": d}), frame.o, d)
        }
        k::AxisRef::World { axis } => {
            let d = match axis.to_ascii_uppercase().as_str() {
                "X" => [1.0, 0.0, 0.0],
                "Y" => [0.0, 1.0, 0.0],
                _ => [0.0, 0.0, 1.0],
            };
            (json!(axis.to_ascii_lowercase()), [0.0; 3], d)
        }
        k::AxisRef::Line { origin, dir } => {
            (json!({"origin": origin, "direction": dir}), *origin, *dir)
        }
    })
}

use mitcad_solidcraft::sketch as mitcad_sketch;

fn revolve<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let r: k::Revolve = f.typed()?;
    let def = revolve_def(im, f, component, &r, None)?;
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::partial(Vec::new()))
}

/// The definition of a revolve; `profiles` instead of the file's selection
/// (a surface revolve's, mitcad#455).
pub(super) fn revolve_def<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
    r: &k::Revolve,
    profiles: Option<Vec<Value>>,
) -> Result<Value, NotMapped> {
    let profiles = match profiles {
        Some(profiles) => profiles,
        None => im.profiles(r.sketch, &r.profiles)?,
    };
    let (axis, origin, dir) = revolve_axis(im, r.sketch, &r.axis)?;
    let s = im
        .sketches
        .get(&r.sketch)
        .cloned()
        .ok_or("the sketch did not come in")?;
    // The way round: SolidCraft's newer revolves turn their profiles
    // towards the sketch normal (`canonical_revolve_axis`); older ones by
    // the axis' own sign, as Mitcad does.
    let mut sign = 1.0;
    if r.convention == k::RevolveConvention::SketchNormal {
        let centre = profile_centre(&s.sketch, &s.frame);
        let v = mitcad_import_base::geom::sub(centre, origin);
        if dot(cross(dir, v), s.frame.normal()) < 0.0 {
            sign = -1.0;
        }
    }
    let angle = r.angle.as_str();
    let full = mitcad_solidcraft::expr::literal(angle, std::f64::consts::PI / 180.0)
        .is_some_and(|a| (a.abs() - std::f64::consts::TAU).abs() < 1e-9);
    let extent = if let Some(to) = r.to {
        json!({"type": "to_object", "object": im.object_at(component, to, s.frame.normal(), f.face_names.first().map(String::as_str))})
    } else if full && r.angle2.is_none() {
        json!({"type": "full"})
    } else if r.symmetric {
        json!({"type": "symmetric", "angle": im.slot_of(f, "/angle", |n| format!("{n} / 2"))?})
    } else if r.angle2.is_some() {
        let (a1, a2) = (im.slot(f, "/angle")?, im.slot(f, "/angle2")?);
        if sign < 0.0 {
            json!({"type": "two_sides", "angle1": a2, "angle2": a1})
        } else {
            json!({"type": "two_sides", "angle1": a1, "angle2": a2})
        }
    } else if sign < 0.0 {
        json!({"type": "angle", "angle": im.slot_of(f, "/angle", |n| format!("-{n}"))?})
    } else {
        json!({"type": "angle", "angle": im.slot(f, "/angle")?})
    };
    let (operation, participants) = im.operation(r.operation, &r.targets)?;
    Ok(
        json!({"type": "revolve", "profiles": profiles, "axis": axis, "extent": extent,
        "operation": operation, "participants": participants}),
    )
}

/// The centre of a sketch's non-construction points, in component
/// coordinates (which side of an axis its profiles lie on).
fn profile_centre(s: &mitcad_sketch::Sketch, frame: &Frame) -> [f64; 3] {
    let pts: Vec<[f64; 2]> = s
        .curves
        .iter()
        .filter(|c| !c.is_construction())
        .flat_map(|c| match &c.kind {
            mitcad_sketch::CurveKind::Line { a, b } => vec![*a, *b],
            mitcad_sketch::CurveKind::Circle { c, .. }
            | mitcad_sketch::CurveKind::Ellipse { c, .. } => vec![*c],
            mitcad_sketch::CurveKind::Arc { a, b, .. } => vec![*a, *b],
            mitcad_sketch::CurveKind::Spline { pts, .. } => pts.clone(),
            mitcad_sketch::CurveKind::Conic { a, b, .. } => vec![*a, *b],
        })
        .filter_map(|i| s.points.get(i).map(|p| p.pos))
        .collect();
    let n = pts.len().max(1) as f64;
    let c = pts
        .iter()
        .fold([0.0, 0.0], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n]);
    frame.point(c)
}

/// Edges by their picks, grouped by body (in pick order).
fn edges_by_body<K: Kernel>(
    im: &Importer<'_, '_, K>,
    component: ComponentUid,
    body: Option<&str>,
    picks: &[[f64; 3]],
    names: &[String],
) -> Result<BTreeMap<String, (BodyUid, Vec<String>)>, String> {
    let mut out: BTreeMap<String, (BodyUid, Vec<String>)> = BTreeMap::new();
    for (k, p) in picks.iter().enumerate() {
        let (b, e) = im.edge_named(component, body, *p, names.get(k).map(String::as_str))?;
        let entry = out.entry(b.to_string()).or_insert((b, Vec::new()));
        if !entry.1.contains(&e) {
            entry.1.push(e);
        }
    }
    Ok(out)
}

fn fillet<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let fl: k::Fillet = f.typed()?;
    let mut notes = Vec::new();
    let groups = edges_by_body(im, component, fl.body.as_deref(), &fl.edges, &f.edge_names)?;
    let radius = im.slot(f, "/radius")?;
    let size = match &fl.style {
        k::FilletStyle::Constant | k::FilletStyle::Curvature => {
            json!({"type": "constant", "radius": radius})
        }
        k::FilletStyle::Chord => json!({"type": "chord_length", "length": radius}),
        k::FilletStyle::Variable { mid, smooth, .. } => {
            let end = im.slot(f, "/style/radius2")?;
            let mut mids = Vec::new();
            for (i, (at, _)) in mid.iter().enumerate() {
                mids.push(
                    json!({"position": at, "radius": im.slot(f, &format!("/style/mid/{i}/1"))?}),
                );
            }
            if !smooth && !mid.is_empty() {
                notes.push(
                    "the radius varies smoothly between the mid radii (straight in the file)"
                        .to_owned(),
                );
            }
            notes.push("the variable radius starts at the kernel's end of the chain".to_owned());
            json!({"type": "variable", "start": radius, "end": end, "mid": mids})
        }
        k::FilletStyle::Spline {
            start,
            knots,
            radii,
        } => {
            let mut controls = vec![radius];
            for i in 0..radii.len() {
                controls.push(im.slot(f, &format!("/style/radii/{i}"))?);
            }
            spline_fillet(im, f, component, &groups, *start, knots, &controls)?;
            if let Some(fit) = fl.fit.as_deref().filter(|fit| *fit != "exact") {
                notes.push(format!(
                    "the radius is exact (the file reduces or tapers it where it does not fit: `{fit}`)"
                ));
            }
            return Ok(Mapped::partial(notes));
        }
    };
    if fl.fit.as_deref().is_some_and(|f| f != "exact") {
        notes.push(format!(
            "the radius is exact (the file reduces or tapers it where it does not fit: `{}`)",
            fl.fit.as_deref().unwrap_or_default()
        ));
    }
    let curvature = matches!(fl.style, k::FilletStyle::Curvature);
    let corners = fl.corner.as_deref() == Some("g2");
    if corners {
        notes.push(
            "corners where three roundings meet are setback patches (G2 corners in the file)"
                .to_owned(),
        );
    }
    let several = groups.len() > 1;
    for (i, (b, edges)) in groups.values().enumerate() {
        let mut set = json!({"edges": edges, "size": size.clone()});
        if curvature {
            set["continuity"] = json!("curvature");
        }
        let mut def = json!({"type": "fillet", "body": b.to_string(), "sets": [set]});
        if corners {
            def["rolling_ball_corners"] = json!(false);
        }
        let name = if several {
            format!("{} ({})", f.name, i + 1)
        } else {
            f.name.clone()
        };
        im.add(&def, Some(&name), component)?;
    }
    Ok(Mapped::partial(notes))
}

/// A fillet whose radius along each edge is a B-spline of the position
/// (mitcad#464): the file rounds its edges one at a time, each from its end
/// nearest `start`, so each edge is a Mitcad fillet of its own with a
/// spline radius starting at that end. `radii` are the slots of the control
/// radii (the file's `radius` and its `radii`).
fn spline_fillet<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
    groups: &BTreeMap<String, (BodyUid, Vec<String>)>,
    start: [f64; 3],
    knots: &[f64],
    radii: &[String],
) -> Result<(), NotMapped> {
    let count: usize = groups.values().map(|(_, edges)| edges.len()).sum();
    let mut k = 0;
    for (b, edges) in groups.values() {
        for edge in edges {
            k += 1;
            let vertex = im.edge_end_vertex(*b, edge, start)?;
            let set = json!({"edges": [edge], "size": {"type": "spline", "start_vertex": vertex,
                "knots": knots, "radii": radii}, "tangent_chain": false});
            let def = json!({"type": "fillet", "body": b.to_string(), "sets": [set]});
            let name = if count > 1 {
                format!("{} ({k})", f.name)
            } else {
                f.name.clone()
            };
            im.add(&def, Some(&name), component)?;
        }
    }
    Ok(())
}

fn chamfer<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let c: k::Chamfer = f.typed()?;
    let notes = Vec::new();
    let groups = edges_by_body(im, component, c.body.as_deref(), &c.edges, &f.edge_names)?;
    let d = im.slot(f, "/distance")?;
    let size = if c.distance2.is_some() {
        json!({"type": "two_distances", "distance1": d, "distance2": im.slot(f, "/distance2")?})
    } else if c.angle.is_some() {
        json!({"type": "distance_angle", "distance": d, "angle": im.slot(f, "/angle")?})
    } else {
        json!({"type": "equal_distance", "distance": d})
    };
    let unequal = c.distance2.is_some() || c.angle.is_some();
    let several = groups.len() > 1;
    for (i, (b, edges)) in groups.values().enumerate() {
        let sets: Vec<Value> = if unequal {
            // The first distance is on the face that faces up most (the
            // other with `flip`): each edge's own reference face.
            edges
                .iter()
                .map(|e| {
                    let reference = up_face(im, *b, e, c.flip);
                    let mut set = json!({"edges": [e], "size": size.clone()});
                    if let Some(r) = reference {
                        set["reference_face"] = json!(r);
                    }
                    set
                })
                .collect()
        } else {
            vec![json!({"edges": edges, "size": size.clone()})]
        };
        let def = json!({"type": "chamfer", "body": b.to_string(), "sets": sets});
        let name = if several {
            format!("{} ({})", f.name, i + 1)
        } else {
            f.name.clone()
        };
        im.add(&def, Some(&name), component)?;
    }
    Ok(Mapped::partial(notes))
}

/// Of an edge's two faces, the one whose normal at the edge points up
/// most (the other with `flip`).
fn up_face<K: Kernel>(
    im: &Importer<'_, '_, K>,
    body: BodyUid,
    edge: &str,
    flip: bool,
) -> Option<String> {
    let name: mitcad_model::EdgeName = edge.parse().ok()?;
    let shape = im.doc.body_shape(body)?;
    let kernel = im.doc.kernel();
    let mid = kernel
        .path_point(
            shape,
            std::slice::from_ref(&name),
            mitcad_model::datum::PathParameter::Fraction(0.5),
        )
        .ok()?
        .point;
    let faces = name.faces();
    let up = |face: &mitcad_model::FaceName| {
        kernel
            .face_point_normal(shape, face, mid)
            .map(|s| s.normal[2])
            .unwrap_or(0.0)
    };
    let (a, b) = (up(&faces[0]), up(&faces[1]));
    let first = if a >= b { 0 } else { 1 };
    let pick = if flip { 1 - first } else { first };
    Some(faces[pick].to_string())
}

/// A primitive on the plane across its axis through its base point.
fn primitive<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let mut notes = Vec::new();
    let (corner_key, axis, op) = match f.kind.as_str() {
        "box" => {
            let b: k::BoxPrimitive = f.typed()?;
            ("corner", [0.0, 0.0, 1.0], b.operation)
        }
        "cylinder" => {
            let c: k::Cylinder = f.typed()?;
            ("base", c.axis, c.operation)
        }
        "sphere" => {
            let s: k::Sphere = f.typed()?;
            ("center", [0.0, 0.0, 1.0], s.operation)
        }
        _ => {
            let t: k::Torus = f.typed()?;
            ("center", [0.0, 0.0, 1.0], t.operation)
        }
    };
    let at: Vec<String> = (0..3)
        .map(|i| {
            f.def[corner_key]
                .get(i)
                .and_then(inputs::expr_text)
                .unwrap_or_else(|| "0".to_owned())
        })
        .collect();
    let p = [
        im.eval(&at[0], Kind::Length)?,
        im.eval(&at[1], Kind::Length)?,
        im.eval(&at[2], Kind::Length)?,
    ];
    if at
        .iter()
        .any(|e| mitcad_solidcraft::expr::literal(e, 1.0).is_none())
    {
        notes.push("its position is fixed where its expressions put it now".to_owned());
    }
    // The plane across the axis through the point: an origin plane when
    // the axis is one of the model's, moved along its normal.
    let axis = unit(axis).ok_or("the axis has no direction")?;
    let frame = if (axis[2] - 1.0).abs() < 1e-12 {
        ("xy", Frame::XY)
    } else if (axis[1] - 1.0).abs() < 1e-12 {
        (
            "xz",
            Frame {
                o: [0.0; 3],
                x: [1.0, 0.0, 0.0],
                y: [0.0, 0.0, -1.0],
            },
        )
    } else if (axis[0] - 1.0).abs() < 1e-12 {
        (
            "yz",
            Frame {
                o: [0.0; 3],
                x: [0.0, 0.0, -1.0],
                y: [0.0, 1.0, 0.0],
            },
        )
    } else {
        ("", Frame::XY)
    };
    let (plane, uv) = if frame.0.is_empty() {
        let x = unit(cross([0.0, 0.0, 1.0], axis)).unwrap_or([1.0, 0.0, 0.0]);
        let def = json!({"type": "construction_plane",
            "definition": {"type": "fixed", "origin": p, "x_axis": x, "y_axis": cross(axis, x)}});
        let uid = im.add_helper(&def, component)?;
        (json!(uid.to_string()), [0.0, 0.0])
    } else {
        let fr = frame.1;
        let n = fr.normal();
        let height = dot(p, n);
        let uv = [dot(p, fr.x), dot(p, fr.y)];
        if height.abs() < 1e-12 {
            (json!(frame.0), uv)
        } else {
            let def = json!({"type": "construction_plane",
                "definition": {"type": "offset", "plane": frame.0, "distance": height}});
            let uid = im.add_helper(&def, component)?;
            (json!(uid.to_string()), uv)
        }
    };
    let (operation, _) = im.operation(op, &[])?;
    let def = match f.kind.as_str() {
        "box" => json!({"type": "box", "plane": plane, "corner": uv,
            "length": im.slot(f, "/length")?, "width": im.slot(f, "/width")?,
            "height": im.slot(f, "/height")?, "operation": operation}),
        "cylinder" => json!({"type": "cylinder", "plane": plane, "center": uv,
            "diameter": im.slot_of(f, "/radius", |n| format!("2 * {n}"))?,
            "height": im.slot(f, "/height")?, "operation": operation}),
        "sphere" => json!({"type": "sphere", "plane": plane, "center": uv,
            "diameter": im.slot_of(f, "/radius", |n| format!("2 * {n}"))?, "operation": operation}),
        _ => json!({"type": "torus", "plane": plane, "center": uv,
            "diameter": im.slot_of(f, "/major", |n| format!("2 * {n}"))?,
            "section_diameter": im.slot_of(f, "/minor", |n| format!("2 * {n}"))?,
            "position": "on_center", "operation": operation}),
    };
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::partial(notes))
}

fn combine<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let c: k::Combine = f.typed()?;
    if c.operation == Operation::NewBody {
        return Err(NotMapped::Failed(
            "a combine needs join, cut or intersect".to_owned(),
        ));
    }
    let target = im.body(&c.target)?;
    let tools = im.body_ids(&c.tools)?;
    let def = json!({"type": "combine", "target": target.to_string(), "tools": tools,
        "operation": c.operation.as_str(), "keep_tools": c.keep_tools});
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::parametric())
}

fn construction_plane<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let c: k::ConstructionPlane = f.typed()?;
    let names = im.input_names(f, "/plane");
    let mut it = names.iter().map(String::as_str);
    let made = im.plane_ref(&c.plane, &mut it, component, Some(&f.name))?;
    // A plane that is a reference already (a face's plane taken as it is)
    // is made a construction plane by plane_ref; its frame is kept.
    if let Some(w) = made.world {
        im.planes.insert(f.id, w);
    }
    Ok(Mapped::partial(made.notes))
}

fn construction_axis<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let c: k::ConstructionAxis = f.typed()?;
    let mut notes = Vec::new();
    let definition = match &c.def {
        k::AxisDef::ThroughFace { face } => {
            json!({"type": "circular_face", "face": im.geo(face, component, &mut notes)?})
        }
        k::AxisDef::NormalToFace { face, point } => json!({"type": "normal_to_face_at_point",
            "face": im.geo(face, component, &mut notes)?, "point": im.geo(point, component, &mut notes)?}),
        k::AxisDef::TwoPlanes { a, b } => json!({"type": "two_planes",
            "plane1": im.geo(a, component, &mut notes)?, "plane2": im.geo(b, component, &mut notes)?}),
        k::AxisDef::TwoPoints { a, b } => json!({"type": "two_points",
            "point1": im.geo(a, component, &mut notes)?, "point2": im.geo(b, component, &mut notes)?}),
        k::AxisDef::Edge { edge } => {
            json!({"type": "edge", "edge": im.geo(edge, component, &mut notes)?})
        }
    };
    im.add(
        &json!({"type": "construction_axis", "definition": definition}),
        Some(&f.name),
        component,
    )?;
    Ok(Mapped::partial(notes))
}

fn construction_point<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let c: k::ConstructionPoint = f.typed()?;
    let mut notes = Vec::new();
    let definition = match &c.def {
        k::PointDef::At { point } => {
            json!({"type": "point", "point": im.geo(point, component, &mut notes)?})
        }
        k::PointDef::TwoEdges { a, b } => json!({"type": "two_edges",
            "line1": im.geo(a, component, &mut notes)?, "line2": im.geo(b, component, &mut notes)?}),
        k::PointDef::ThreePlanes { a, b, c } => json!({"type": "three_planes",
            "plane1": im.geo(a, component, &mut notes)?, "plane2": im.geo(b, component, &mut notes)?,
            "plane3": im.geo(c, component, &mut notes)?}),
        k::PointDef::Center { of } => {
            json!({"type": "center", "entity": im.geo(of, component, &mut notes)?})
        }
        k::PointDef::EdgeAndPlane { edge, plane } => json!({"type": "edge_and_plane",
            "line": im.geo(edge, component, &mut notes)?, "plane": im.geo(plane, component, &mut notes)?}),
        k::PointDef::AlongPath { path, .. } => json!({"type": "along_path",
            "path": im.path_of(path, component, &mut notes)?, "distance": im.slot(f, "/def/t")?, "physical": false}),
    };
    im.add(
        &json!({"type": "construction_point", "definition": definition}),
        Some(&f.name),
        component,
    )?;
    Ok(Mapped::partial(notes))
}

/// Faces by their picks, all on one body.
fn faces_on_one_body<K: Kernel>(
    im: &Importer<'_, '_, K>,
    component: ComponentUid,
    body: Option<&str>,
    picks: &[[f64; 3]],
    names: &[String],
) -> Result<(Option<BodyUid>, Vec<String>), String> {
    let mut b = body.map(|n| im.body(n)).transpose()?;
    let mut faces = Vec::new();
    for (k, p) in picks.iter().enumerate() {
        let (on, face) = im.face_named(component, body, *p, names.get(k).map(String::as_str))?;
        if b.is_some_and(|b| b != on) {
            return Err("the faces are on several bodies".to_owned());
        }
        b = Some(on);
        if !faces.contains(&face) {
            faces.push(face);
        }
    }
    Ok((b, faces))
}

fn shell<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let s: k::Shell = f.typed()?;
    let (body, faces) =
        faces_on_one_body(im, component, s.body.as_deref(), &s.faces, &f.face_names)?;
    let body = match body {
        Some(b) => b,
        None => *im
            .component_bodies(component)
            .first()
            .ok_or("no body to shell")?,
    };
    let (inside, outside) = match s.direction.as_str() {
        "outside" => (
            json!(im.params.unnamed(im.doc, "0")),
            json!(im.slot(f, "/thickness")?),
        ),
        "both" => {
            let half = im.slot_of(f, "/thickness", |n| format!("{n} / 2"))?;
            let other = im.slot_of(f, "/thickness", |n| format!("{n} / 2"))?;
            (json!(half), json!(other))
        }
        _ => {
            let t = im.slot(f, "/thickness")?;
            (json!(t), json!(im.params.unnamed(im.doc, "0")))
        }
    };
    let def = json!({"type": "shell", "body": body.to_string(), "faces": faces,
        "inside": inside, "outside": outside, "tangent_chain": s.tangent_chain});
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::parametric())
}

fn draft<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let d: k::Draft = f.typed()?;
    let mut notes = Vec::new();
    let (body, faces) =
        faces_on_one_body(im, component, d.body.as_deref(), &d.faces, &f.face_names)?;
    let body = body.ok_or("no faces to draft")?;
    let plane = im.feature_plane(f, &d.neutral, "/neutral", component, &mut notes)?;
    let frame = im.mitcad_frame(&plane)?;
    let flip = dot(frame.normal(), d.pull) < 0.0;
    let def = json!({"type": "draft", "body": body.to_string(), "faces": faces, "plane": plane,
        "angle": im.slot(f, "/angle")?, "flip": flip});
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::partial(notes))
}

fn hole<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let h: k::Hole = f.typed()?;
    let mut notes = Vec::new();
    let (placement, flip) = match &h.points {
        Some(points) => {
            let s = im
                .sketches
                .get(&points.sketch)
                .cloned()
                .ok_or("the sketch did not come in")?;
            let ids: Vec<String> = points
                .ids
                .iter()
                .map(|p| s.point(p))
                .collect::<Result<_, _>>()?;
            (
                json!({"type": "sketch_points", "sketch": s.uid.to_string(), "points": ids}),
                points.flip,
            )
        }
        None => {
            let position = match &h.at {
                Some(at) => {
                    if at
                        .iter()
                        .any(|e| mitcad_solidcraft::expr::literal(e.as_str(), 1.0).is_none())
                    {
                        notes.push(
                            "its position is fixed where its expressions put it now".to_owned(),
                        );
                    }
                    [
                        im.eval(at[0].as_str(), Kind::Length)?,
                        im.eval(at[1].as_str(), Kind::Length)?,
                        im.eval(at[2].as_str(), Kind::Length)?,
                    ]
                }
                None => h.position,
            };
            let (body, face) = im.face_named(
                component,
                None,
                position,
                f.face_names.first().map(String::as_str),
            )?;
            (
                json!({"type": "face", "body": body.to_string(), "face": face, "points": [position]}),
                false,
            )
        }
    };
    let diameter = im.slot(f, "/diameter")?;
    let extent = if let Some(to) = h.to {
        json!({"type": "to_object", "object": im.object_at(component, to, h.direction, f.face_names.get(usize::from(h.points.is_none())).map(String::as_str))})
    } else if h.depth.is_some() {
        json!({"type": "distance", "depth": im.slot(f, "/depth")?})
    } else {
        json!({"type": "through_all"})
    };
    let mut def = json!({"type": "hole", "placement": placement, "diameter": diameter, "extent": extent,
        "flip": flip});
    match &h.hole {
        k::HoleKind::Simple => def["flat"] = json!(true),
        k::HoleKind::Drilled { .. } => def["tip_angle"] = json!(im.slot(f, "/hole/tip_angle")?),
        k::HoleKind::Counterbore { .. } => {
            def["flat"] = json!(true);
            def["kind"] = json!({"type": "counterbore", "diameter": im.slot(f, "/hole/cb_diameter")?,
                "depth": im.slot(f, "/hole/cb_depth")?});
        }
        k::HoleKind::Countersink { .. } => {
            def["flat"] = json!(true);
            def["kind"] = json!({"type": "countersink", "diameter": im.slot(f, "/hole/cs_diameter")?,
                "angle": im.slot(f, "/hole/cs_angle")?});
        }
    }
    if let Some(t) = &h.thread {
        notes.push(format!(
            "a cosmetic thread {t} is a note: the hole keeps its diameter"
        ));
    }
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::partial(notes))
}

fn thread<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let t: k::Thread = f.typed()?;
    let mut notes = Vec::new();
    let (body, face) = im.face_named(
        component,
        None,
        t.face,
        f.face_names.first().map(String::as_str),
    )?;
    // The length is measured from the end of the face nearest the pick.
    let location = match im.doc.body_shape(body).and_then(|s| {
        let name: mitcad_model::FaceName = face.parse().ok()?;
        im.doc.kernel().face_cylinder(s, &name).ok()
    }) {
        Some(c) => {
            let along = dot(
                mitcad_import_base::geom::sub(t.face, c.axis.origin),
                c.axis.direction,
            );
            if along >= c.length / 2.0 {
                "high_end"
            } else {
                "low_end"
            }
        }
        None => "high_end",
    };
    let designation = t.designation.clone().unwrap_or_default();
    if designation.is_empty() {
        return Err(NotMapped::Failed(
            "a thread without a designation".to_owned(),
        ));
    }
    let mut def = json!({"type": "thread", "faces": [{"body": body.to_string(), "face": face}],
        "thread": {"designation": designation}, "modeled": t.modeled, "location": location});
    if t.length.is_some() {
        def["length"] = json!(im.slot(f, "/length")?);
    }
    if t.modeled {
        notes.push(
            "the modeled thread's profile is ISO 68-1's at the size's basic diameters".to_owned(),
        );
    }
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::partial(notes))
}

/// The objects of a pattern or mirror: its features' Mitcad features, or
/// its bodies.
fn objects<K: Kernel>(
    im: &Importer<'_, '_, K>,
    features: &[String],
    bodies: &[String],
) -> Result<Value, String> {
    if !bodies.is_empty() {
        Ok(json!({"type": "bodies", "bodies": im.body_ids(bodies)?}))
    } else {
        Ok(json!({"type": "features", "features": im.features_named(features)?}))
    }
}

fn pattern<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let p: k::Pattern = f.typed()?;
    let mut notes = Vec::new();
    let objects = objects(im, &p.features, &p.bodies)?;
    let def = match &p.pattern {
        k::PatternKind::Rectangular {
            dir1,
            dir2,
            alignment,
            alignment2,
            reversed,
            reversed2,
            ..
        } => {
            let direction = |im: &mut Importer<'_, '_, K>,
                             dir: [f64; 3],
                             count: &str,
                             spacing: &str,
                             alignment: &Option<String>,
                             reversed: bool,
                             notes: &mut Vec<String>|
             -> Result<Value, String> {
                let quantity = im.slot(f, count)?;
                let symmetric = alignment.as_deref() == Some("center");
                let back = reversed ^ (alignment.as_deref() == Some("left"));
                if symmetric && reversed {
                    notes.push(
                        "a centered direction keeps its extra copy on the positive side".to_owned(),
                    );
                }
                let distance = if back && !symmetric {
                    im.slot_of(f, spacing, |n| format!("-{n}"))?
                } else {
                    im.slot(f, spacing)?
                };
                Ok(
                    json!({"axis": {"origin": [0, 0, 0], "direction": dir}, "quantity": quantity,
                    "distance": distance, "symmetric": symmetric}),
                )
            };
            let d1 = direction(
                im,
                *dir1,
                "/pattern/count1",
                "/pattern/spacing1",
                alignment,
                *reversed,
                &mut notes,
            )?;
            let mut def = json!({"type": "rectangular_pattern", "objects": objects, "direction1": d1,
                "distance_type": "spacing"});
            if let Some(dir2) = dir2
                && f.def.pointer("/pattern/count2").is_some()
            {
                def["direction2"] = direction(
                    im,
                    *dir2,
                    "/pattern/count2",
                    "/pattern/spacing2",
                    alignment2,
                    *reversed2,
                    &mut notes,
                )?;
            }
            def
        }
        k::PatternKind::Circular {
            origin,
            axis,
            alignment,
            reversed,
            ..
        } => {
            let angle = if *reversed {
                im.slot_of(f, "/pattern/angle", |n| format!("-{n}"))?
            } else {
                im.slot(f, "/pattern/angle")?
            };
            json!({"type": "circular_pattern", "objects": objects,
                "axis": {"origin": origin, "direction": axis}, "quantity": im.slot(f, "/pattern/count")?,
                "angle": angle, "symmetric": alignment.as_deref() == Some("center")})
        }
        k::PatternKind::Path {
            path_sketch,
            path,
            extent,
            flip,
            orient,
            ..
        } => {
            let s = im
                .sketches
                .get(path_sketch)
                .cloned()
                .ok_or("the path's sketch did not come in")?;
            let curves: Vec<String> = path.iter().map(|c| s.curve(c)).collect::<Result<_, _>>()?;
            // One curve, or a chain of them (mitcad#464), from the free end
            // of the first towards the second, as the file runs it.
            let path = match curves.as_slice() {
                [curve] => json!({"sketch": s.uid.to_string(), "curve": curve}),
                _ => json!({"sketch": s.uid.to_string(), "curves": curves}),
            };
            // Flipped, the file runs the path from its end: the original
            // sits there and the copies go back along it.
            json!({"type": "path_pattern", "objects": objects, "path": path,
                "quantity": im.slot(f, "/pattern/count")?, "distance": im.slot(f, "/pattern/spacing")?,
                "distance_type": if *extent { "extent" } else { "spacing" },
                "start": if *flip { 1 } else { 0 }, "flip": flip, "along_path": orient})
        }
    };
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::partial(notes))
}

fn mirror<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let m: k::Mirror = f.typed()?;
    let mut notes = Vec::new();
    let objects = objects(im, &m.features, &m.bodies)?;
    let plane = im.feature_plane(f, &m.plane, "/plane", component, &mut notes)?;
    let def = json!({"type": "mirror", "objects": objects, "plane": plane, "combine": m.combine});
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::partial(notes))
}

/// Move: turned about the axis through the origin, then moved (SolidCraft's
/// `kernel::transform`): a rotate and a translate.
fn move_bodies<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let m: k::Move = f.typed()?;
    let bodies = im.body_ids(&m.bodies)?;
    let mut moved = bodies.clone();
    let mut copy = m.copy;
    if m.angle.is_some() {
        let axis = m.rotate_axis.unwrap_or([0.0, 0.0, 1.0]);
        let def = json!({"type": "move", "bodies": moved, "copy": copy,
            "transform": {"type": "rotate", "axis": {"origin": [0, 0, 0], "direction": axis},
                "angle": im.slot(f, "/angle")?}});
        let name = format!("{} (turn)", f.name);
        let before: Vec<BodyUid> = im.doc.bodies().iter().map(|b| b.uid).collect();
        let uid: FeatureUid = im.add(&def, Some(&name), component)?;
        if copy {
            moved = im
                .doc
                .bodies()
                .iter()
                .filter(|b| !before.contains(&b.uid) && b.uid.feature == uid)
                .map(|b| b.uid.to_string())
                .collect();
            copy = false;
        }
    }
    let def = json!({"type": "move", "bodies": moved, "copy": copy,
        "transform": {"type": "translate_xyz", "x": im.slot(f, "/translate/0")?,
            "y": im.slot(f, "/translate/1")?, "z": im.slot(f, "/translate/2")?}});
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::parametric())
}

fn scale_bodies<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let s: k::Scale = f.typed()?;
    let bodies = im.body_ids(&s.bodies)?;
    let scale = if s.factors.is_some() {
        json!({"type": "non_uniform", "x": im.slot(f, "/factors/0")?, "y": im.slot(f, "/factors/1")?,
            "z": im.slot(f, "/factors/2")?})
    } else {
        json!({"type": "uniform", "factor": im.slot(f, "/factor")?})
    };
    let def =
        json!({"type": "scale", "bodies": bodies, "point": {"point": s.origin}, "scale": scale});
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::parametric())
}

/// Align: `from` onto `to`, with normals the faces meet face to face.
fn align<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let bodies: Vec<String> =
        serde_json::from_value(f.def["bodies"].clone()).map_err(|e| e.to_string())?;
    let from: [f64; 3] =
        serde_json::from_value(f.def["from"].clone()).map_err(|e| e.to_string())?;
    let to: [f64; 3] = serde_json::from_value(f.def["to"].clone()).map_err(|e| e.to_string())?;
    let from_normal: Option<[f64; 3]> =
        serde_json::from_value(f.def["from_normal"].clone()).unwrap_or(None);
    let to_normal: Option<[f64; 3]> =
        serde_json::from_value(f.def["to_normal"].clone()).unwrap_or(None);
    let flip = f.def["flip"].as_bool().unwrap_or(false);
    let bodies = im.body_ids(&bodies)?;
    let (a, b) = match (from_normal, to_normal) {
        (Some(n1), Some(n2)) => (
            json!({"type": "plane", "plane": {"origin": from, "normal": n1}}),
            json!({"type": "plane", "plane": {"origin": to, "normal": n2}}),
        ),
        _ => (
            json!({"type": "point", "point": {"point": from}}),
            json!({"type": "point", "point": {"point": to}}),
        ),
    };
    let def = json!({"type": "align", "bodies": bodies, "from": a, "to": b, "flip": flip, "angle": im.params.unnamed(im.doc, "0")});
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::partial(vec![
        "aligned by the file's points (fixed)".to_owned(),
    ]))
}

fn split<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let s: k::Split = f.typed()?;
    let mut notes = Vec::new();
    let body = im.body(&s.body)?;
    let tool = if let (Some(sketch), Some(line)) = (s.sketch, &s.line) {
        let made = im
            .sketches
            .get(&sketch)
            .cloned()
            .ok_or("the sketch did not come in")?;
        json!({"sketch": made.uid.to_string(), "curves": [made.curve(line)?]})
    } else if let Some(tool) = &s.tool {
        let (b, face) = im.face_at(component, Some(&tool.body), tool.point)?;
        json!({"body": b.to_string(), "face": face})
    } else {
        im.feature_plane(f, &s.plane, "/plane", component, &mut notes)?
    };
    let def =
        json!({"type": "split_body", "bodies": [body.to_string()], "tool": tool, "extend": true});
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::partial(notes))
}

fn split_face<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let s: k::SplitFace = f.typed()?;
    let mut notes = Vec::new();
    let (body, faces) =
        faces_on_one_body(im, component, s.body.as_deref(), &s.faces, &f.face_names)?;
    let body = body.ok_or("no faces to split")?;
    let tool = if let Some(sketch) = s.sketch {
        let made = im
            .sketches
            .get(&sketch)
            .cloned()
            .ok_or("the sketch did not come in")?;
        json!({"sketch": made.uid.to_string(), "curves": []})
    } else if let Some(tool) = &s.tool {
        let (b, face) = im.face_at(component, Some(&tool.body), tool.point)?;
        json!({"body": b.to_string(), "face": face})
    } else {
        im.feature_plane(f, &s.plane, "/plane", component, &mut notes)?
    };
    let def = json!({"type": "split_face", "body": body.to_string(), "faces": faces, "tool": tool, "extend": true});
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::partial(notes))
}

fn face_feature<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let mut notes = Vec::new();
    let def = match f.kind.as_str() {
        "offset_face" => {
            let o: k::OffsetFace = f.typed()?;
            let (body, faces) =
                faces_on_one_body(im, component, o.body.as_deref(), &o.faces, &f.face_names)?;
            json!({"type": "offset_face", "body": body.ok_or("no faces")?.to_string(), "faces": faces,
                "distance": im.slot(f, "/distance")?})
        }
        "delete_face" => {
            let d: k::DeleteFace = f.typed()?;
            if !d.heal {
                return Err(NotMapped::Unmapped(
                    "delete face leaving the gap open (a surface body)".to_owned(),
                ));
            }
            let (body, faces) =
                faces_on_one_body(im, component, d.body.as_deref(), &d.faces, &f.face_names)?;
            json!({"type": "delete_face", "body": body.ok_or("no faces")?.to_string(), "faces": faces})
        }
        _ => {
            let r: k::ReplaceFace = f.typed()?;
            let (body, faces) =
                faces_on_one_body(im, component, r.body.as_deref(), &r.faces, &f.face_names)?;
            let target = im.feature_plane(f, &r.target, "/target", component, &mut notes)?;
            json!({"type": "replace_face", "body": body.ok_or("no faces")?.to_string(), "faces": faces,
                "target": target, "tangent_chain": false})
        }
    };
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::partial(notes))
}

fn remove<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let r: k::Remove = f.typed()?;
    let def = json!({"type": "remove_body", "bodies": im.body_ids(&r.bodies)?});
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::parametric())
}

fn loft<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let l: k::Loft = f.typed()?;
    let mut sections = Vec::new();
    for s in &l.sections {
        match &s.point {
            Some(p) => {
                let made = im
                    .sketches
                    .get(&s.sketch)
                    .cloned()
                    .ok_or("a section's sketch did not come in")?;
                sections.push(json!({"type": "point", "point": {"sketch": made.uid.to_string(), "point": made.point(p)?}}));
            }
            None => {
                let regions = im.profiles(s.sketch, &s.profiles)?;
                let [region] = regions.as_slice() else {
                    return Err(NotMapped::Failed(format!(
                        "a loft section of {} profiles",
                        regions.len()
                    )));
                };
                sections.push(json!({"type": "profile", "sketch": region["sketch"], "region": region["region"]}));
            }
        }
    }
    let (operation, participants) = im.operation(l.operation, &l.targets)?;
    let def = json!({"type": "loft", "sections": sections, "ruled": true,
        "operation": operation, "participants": participants});
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::parametric())
}

fn sweep<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let s: k::Sweep = f.typed()?;
    let mut notes = Vec::new();
    let def = sweep_def(im, f, &s, None, &mut notes)?;
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::partial(notes))
}

/// The definition of a sweep; `profiles` instead of the file's selection
/// (a surface sweep's, mitcad#455).
pub(super) fn sweep_def<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    s: &k::Sweep,
    profiles: Option<Vec<Value>>,
    notes: &mut Vec<String>,
) -> Result<Value, NotMapped> {
    let profiles = match profiles {
        Some(profiles) => profiles,
        None => im.profiles(s.sketch, &s.profiles)?,
    };
    let ps = im
        .sketches
        .get(&s.path_sketch)
        .cloned()
        .ok_or("the path's sketch did not come in")?;
    let curves: Vec<String> = s
        .path
        .iter()
        .map(|c| ps.curve(c))
        .collect::<Result<_, _>>()?;
    let orientation = match &s.orientation {
        Some(Value::String(o)) if o == "parallel" => "parallel",
        Some(Value::Object(_)) => {
            notes.push("the profile's turn about the path follows the path, not the file's fixed direction".to_owned());
            "perpendicular"
        }
        _ => "perpendicular",
    };
    let (operation, participants) = im.operation(s.operation, &s.targets)?;
    let mut def = json!({"type": "sweep", "profiles": profiles,
        "path": {"sketch": ps.uid.to_string(), "curves": curves}, "orientation": orientation,
        "operation": operation, "participants": participants});
    if s.twist.is_some() {
        def["twist_angle"] = json!(im.slot(f, "/twist")?);
    }
    // The end scale and the second rail (mitcad#464).
    if s.scale.is_some() {
        def["end_scale"] = json!(im.slot(f, "/scale")?);
    }
    let mut rails = Vec::new();
    for rail in s.rails.iter().take(3) {
        let rs = im
            .sketches
            .get(&rail.sketch)
            .cloned()
            .ok_or("the rail's sketch did not come in")?;
        let rc: Vec<String> = rail
            .curves
            .iter()
            .map(|c| rs.curve(c))
            .collect::<Result<_, _>>()?;
        rails.push(json!({"sketch": rs.uid.to_string(), "curves": rc}));
    }
    match rails.as_slice() {
        [] => {}
        [rail] | [rail, _] => {
            def["guide_rail"] = rail.clone();
            if let Some(second) = rails.get(1) {
                def["guide_rail2"] = second.clone();
            }
            def["profile_scaling"] = json!(s.rail_fit.as_deref().unwrap_or("scale"));
            // A rail's point is where it crosses the plane square to the
            // path (mitcad#469), as the file's sweep takes it.
            def["rail_point"] = json!("plane");
        }
        _ => {
            return Err(NotMapped::Failed(
                "a sweep of more than two rails".to_owned(),
            ));
        }
    }
    Ok(def)
}

fn pipe<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let p: k::Pipe = f.typed()?;
    let s = im
        .sketches
        .get(&p.path_sketch)
        .cloned()
        .ok_or("the path's sketch did not come in")?;
    let curves: Vec<String> = p
        .path
        .iter()
        .map(|c| s.curve(c))
        .collect::<Result<_, _>>()?;
    let (operation, participants) = im.operation(p.operation, &p.targets)?;
    let mut def = json!({"type": "pipe", "path": {"sketch": s.uid.to_string(), "curves": curves},
        "section": "circular", "size": im.slot(f, "/diameter")?,
        "operation": operation, "participants": participants});
    if p.wall.is_some() {
        def["thickness"] = json!(im.slot(f, "/wall")?);
    }
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::parametric())
}

fn coil<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let c: k::Coil = f.typed()?;
    let mut notes = Vec::new();
    let base = [
        im.eval(c.base[0].as_str(), Kind::Length)?,
        im.eval(c.base[1].as_str(), Kind::Length)?,
        im.eval(c.base[2].as_str(), Kind::Length)?,
    ];
    let axis = unit(c.axis).ok_or("the coil's axis has no direction")?;
    // SolidCraft starts at +x (a perpendicular direction when the axis is
    // x) and turns about the axis; the plane's x axis is that direction.
    let x = if axis[0].abs() > 0.9 {
        unit(cross(axis, [0.0, 0.0, 1.0])).unwrap_or([0.0, 1.0, 0.0])
    } else {
        unit(mitcad_import_base::geom::sub(
            [1.0, 0.0, 0.0],
            mitcad_import_base::geom::scale(axis, axis[0]),
        ))
        .unwrap_or([1.0, 0.0, 0.0])
    };
    let def = json!({"type": "construction_plane",
        "definition": {"type": "fixed", "origin": base, "x_axis": x, "y_axis": cross(axis, x)}});
    let plane = im.add_helper(&def, component)?;
    if c.base
        .iter()
        .any(|e| mitcad_solidcraft::expr::literal(e.as_str(), 1.0).is_none())
    {
        notes.push("its base point is fixed where its expressions put it now".to_owned());
    }
    if c.start_angle.is_some() {
        notes.push("the start angle is left out".to_owned());
    }
    let section = match c.section.as_deref() {
        Some("square") => "square",
        _ => "circular",
    };
    let position = match c.position {
        -1 => "inside",
        1 => "outside",
        _ => "on_center",
    };
    let (operation, participants) = im.operation(c.operation, &c.targets)?;
    let def = json!({"type": "coil", "plane": plane.to_string(), "center": [0, 0],
        "diameter": im.slot(f, "/diameter")?,
        "helix": {"type": "revolutions_and_pitch", "revolutions": im.slot(f, "/turns")?, "pitch": im.slot(f, "/pitch")?},
        "angle": im.params.unnamed(im.doc, "0"), "clockwise": c.clockwise, "section": section, "section_position": position,
        "section_size": im.slot(f, "/section_size")?, "operation": operation, "participants": participants});
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::partial(notes))
}

fn rib<K: Kernel>(im: &mut Importer<'_, '_, K>, f: &Feature, component: ComponentUid) -> MapResult {
    let r: k::Rib = f.typed()?;
    let mut notes = Vec::new();
    let s = im
        .sketches
        .get(&r.sketch)
        .cloned()
        .ok_or("the sketch did not come in")?;
    let curves: Vec<String> = r
        .curves
        .iter()
        .map(|c| s.curve(c))
        .collect::<Result<_, _>>()?;
    let extent = match r.depth {
        Some(_) => json!({"type": "depth", "depth": im.slot(f, "/depth")?}),
        None => json!({"type": "to_next"}),
    };
    let kind = if r.web || r.normal { "web" } else { "rib" };
    if r.normal && !r.web {
        notes.push("a rib along the sketch normal is a web".to_owned());
    }
    let mut def = json!({"type": kind, "curves": {"sketch": s.uid.to_string(), "curves": curves},
        "thickness": im.slot(f, "/thickness")?, "extent": extent, "flip": r.flip});
    if kind == "rib" {
        def["thickness_location"] = json!("symmetric");
    }
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::partial(notes))
}

/// An emboss (mitcad#458): Mitcad's emboss on the cylindrical wall the
/// file's face picks (found by its persistent name or at its point), else
/// from the sketch plane; a deboss sinks.
fn emboss<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let e: k::Emboss = f.typed()?;
    let mut notes = Vec::new();
    let profiles = im.profiles(e.sketch, &e.profiles)?;
    let mut def = json!({"type": "emboss", "profiles": profiles, "depth": im.slot(f, "/depth")?,
        "mode": if e.deboss { "sink" } else { "raise" }});
    if let Some(face) = &e.face {
        if !face.path.is_empty() || !face.author_path.is_empty() {
            notes.push(
                "the wall is found at its point in the feature's component (the file's \
                 occurrence paths to it are not translated)"
                    .to_owned(),
            );
        }
        let (body, name) = im.face_named(
            component,
            Some(&face.body),
            face.at,
            f.face_names.first().map(String::as_str),
        )?;
        def["face"] = json!({"body": body.to_string(), "face": name});
    } else if !e.targets.is_empty() {
        def["participants"] = json!(im.body_ids(&e.targets)?);
    }
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::partial(notes))
}

/// A box around bodies grown by a margin (mitcad#460): `bounding_solid`
/// over the bodies the file names, or every body of the component when
/// it names none, the margin its input's parameter. Its body is named as
/// SolidCraft names it (`Importer::new_body_name`).
fn bounding_solid<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let b: k::BoundingSolid = f.typed()?;
    let mut notes = Vec::new();
    if b.bodies.is_empty()
        && im.doc.bodies().iter().any(|body| {
            im.doc
                .body_component(body.uid)
                .is_some_and(|c| c != component)
        })
    {
        notes
            .push("bounds only its own component's bodies (the file bounds every body)".to_owned());
    }
    let def = json!({"type": "bounding_solid", "bodies": im.body_ids(&b.bodies)?,
        "margin": im.slot(f, "/margin")?});
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::partial(notes))
}

/// Boundary Fill (mitcad#457): `boundary_fill` with the file's tools and
/// points. Its join, cut or intersection works on the feature's
/// participants (it has no targets of its own; `*`, none, means all
/// bodies); the tools it removes are gone before the operation, so they
/// are no participants.
fn boundary_fill<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let b: k::BoundaryFill = f.typed()?;
    let tools = im.body_ids(&b.tools)?;
    let mut def = json!({"type": "boundary_fill", "tools": tools, "cells": b.cells,
        "operation": b.operation.as_str(), "remove_tools": b.remove_tools});
    if b.operation != Operation::NewBody {
        let mut participants = im.body_ids(&f.participants)?;
        if b.remove_tools {
            participants.retain(|p| !tools.contains(p));
        }
        def["participants"] = json!(participants);
    }
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::parametric())
}
