// SPDX-License-Identifier: MIT
//! Translation-only patterns driven by persistent point references (mitcad#197).

use std::marker::PhantomData;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::FeatureEntry;
use super::geom_ref::{GeomRef, Resolver, Want, resolve_axis, resolve_point};
use super::mapping::map_params;
use super::pattern::{Combine, ComputeOption, Element, PatternObjects, repeat};
use super::{
    CheckContext, Datum, EvalContext, Evaluate, FeatureDef, FeatureInfo, FeatureOutput,
    OccurrencePath, PlacementChange, References, SketchOutput,
};
use mitcad_vec3::sub;

use crate::assembly::{Assembly, inverse};
use crate::ids::{BodyUid, ComponentUid, FeatureUid};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::transform::Transform;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointReference {
    pub point: GeomRef,
    /// Absolute occurrence path of the point's coordinate frame. Omitted:
    /// the feature's component. An explicit empty path is the root frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<OccurrencePath>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointPatternDef<P = ParamId> {
    pub objects: PatternObjects,
    pub targets: Vec<PointReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<PointReference>,
    /// Absolute path of this feature's component in the displayed assembly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<OccurrencePath>,
    #[serde(default)]
    pub compute: ComputeOption,
    #[serde(skip)]
    pub marker: PhantomData<P>,
}

/// Preview metadata. The point is in feature coordinates; world is in
/// the selected context occurrence. Bounds include all selected sources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternAnchor {
    pub point: [f64; 3],
    pub world: [f64; 3],
    pub mode: String,
    pub policy: String,
}

/// Definition-state ownership and stable identity of a generated occurrence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternInstance {
    pub feature: FeatureUid,
    pub source: OccurrencePath,
    pub target: PointReference,
}

impl<P> PointPatternDef<P> {
    pub const TYPE: &'static str = "pattern_at_points";
    pub const BASE_NAME: &'static str = "PatternAtPoints";

    map_params! {
        PointPatternDef {
            objects: clone,
            targets: clone,
            anchor: clone,
            context: clone,
            compute: copy,
            marker: phantom,
        }
    }

    pub(crate) fn paths(&self) -> Vec<&OccurrencePath> {
        let mut paths: Vec<_> = self
            .targets
            .iter()
            .chain(self.anchor.iter())
            .filter_map(|p| p.occurrence.as_ref())
            .collect();
        paths.extend(self.context.iter());
        if let PatternObjects::Components { occurrences } = &self.objects {
            paths.extend(occurrences);
        }
        paths
    }

    pub(crate) fn context_path(
        &self,
        assembly: &Assembly,
        component: ComponentUid,
    ) -> Result<OccurrencePath, String> {
        let path = match &self.context {
            Some(path) => path.clone(),
            None => OccurrencePath(crate::links::first_path(assembly, component, None)?),
        };
        if crate::joints::path_component(assembly, ComponentUid::ROOT, &path.0)? != component {
            return Err("pattern context must place the feature's component".to_owned());
        }
        Ok(path)
    }

    pub(crate) fn check_components(
        &self,
        assembly: &Assembly,
        features: &[Arc<FeatureEntry>],
        entry: &FeatureEntry,
    ) -> Result<(), String> {
        if let Some(path) = &self.context
            && let Ok(component) =
                crate::joints::path_component(assembly, ComponentUid::ROOT, &path.0)
            && component != entry.component
        {
            return Err("pattern context must place the feature's component".to_owned());
        }
        for path in self.paths() {
            for uid in &path.0 {
                if let Some(instance) = assembly.occurrence(*uid).and_then(|o| o.pattern.as_ref()) {
                    // Generated occurrence dependencies cannot be represented solely by
                    // a body UID. Require their producing feature to precede this one.
                    let before = features
                        .iter()
                        .take_while(|f| f.uid != entry.uid)
                        .any(|f| f.uid == instance.feature);
                    if instance.feature == entry.uid || !before {
                        return Err(
                            "a pattern cannot depend on its own or a later instance".to_owned()
                        );
                    }
                }
            }
        }
        for point in self.targets.iter().chain(self.anchor.iter()) {
            let component = match &point.occurrence {
                Some(p) => {
                    match crate::joints::path_component(assembly, ComponentUid::ROOT, &p.0) {
                        Ok(component) => component,
                        // Missing paths survive save/reopen and fail during recompute.
                        Err(_) => continue,
                    }
                }
                None => entry.component,
            };
            let mut refs = References::default();
            point.point.add_to(&mut refs);
            for uid in refs.features {
                if let Some(other) = features.iter().find(|f| f.uid == uid)
                    && crate::joints::placed_geometry(&other.def)
                    && other.component != component
                {
                    return Err(format!(
                        "{} is not in the point's occurrence component",
                        other.name
                    ));
                }
            }
        }
        let mut refs = References::default();
        self.objects.add_references(&mut refs);
        for uid in refs.features {
            if let Some(other) = features.iter().find(|f| f.uid == uid)
                && other.component != entry.component
            {
                return Err(format!("{} is not in the pattern's component", other.name));
            }
        }
        if let PatternObjects::Components { occurrences } = &self.objects {
            for path in occurrences {
                let Ok(component) =
                    crate::joints::path_component(assembly, ComponentUid::ROOT, &path.0)
                else {
                    continue;
                };
                if component.is_root() || assembly.contains(component, entry.component) {
                    return Err("the repeated component would contain itself".to_owned());
                }
            }
        }
        Ok(())
    }
}

impl FeatureInfo for PointPatternDef {
    fn references(&self) -> References {
        let mut refs = References::default();
        self.objects.add_references(&mut refs);
        for point in self.targets.iter().chain(self.anchor.iter()) {
            point.point.add_to(&mut refs);
        }
        refs
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        match &self.objects {
            PatternObjects::Components { occurrences } => {
                if occurrences.is_empty() {
                    return Err("no components selected".to_owned());
                }
                for (i, path) in occurrences.iter().enumerate() {
                    if path.is_empty() || occurrences[..i].contains(path) {
                        return Err(
                            "component occurrences must be nonempty and distinct".to_owned()
                        );
                    }
                }
            }
            PatternObjects::Faces { .. } => {
                return Err("select bodies, features or components".to_owned());
            }
            _ => self.objects.check(ctx)?,
        }
        if self.targets.is_empty() || self.targets.len() > 10_000 {
            return Err("select between 1 and 10000 target points".to_owned());
        }
        for (i, target) in self.targets.iter().enumerate() {
            target.point.check(ctx, Want::Point)?;
            if self.targets[..i].contains(target) {
                return Err(format!("target point {} is listed more than once", i + 1));
            }
        }
        if let Some(anchor) = &self.anchor {
            anchor.point.check(ctx, Want::Point)?;
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        !matches!(self.objects, PatternObjects::Components { .. })
    }
}

/// Where the occurrence path (absolute, from the root) places its component
/// in the design at this point of the timeline. Also used by moves'
/// points and axes picked in occurrences (mitcad#699).
pub(crate) fn placement<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    path: &OccurrencePath,
) -> Result<Transform, String> {
    let mut result = Transform::IDENTITY;
    for uid in &path.0 {
        let placed = ctx.placement(*uid);
        if ctx
            .env
            .assembly
            .occurrence(*uid)
            .is_some_and(|o| o.pattern.is_some())
            && !ctx.env.placements.contains_key(uid)
        {
            return Err(format!(
                "pattern occurrence {uid} is unavailable at this timeline position"
            ));
        }
        result = result.after(&placed);
    }
    crate::joints::path_component(ctx.env.assembly, ComponentUid::ROOT, &path.0)?;
    Ok(result)
}

struct PointResolver<'a, 'b, K: Kernel> {
    ctx: &'a mut EvalContext<'b, K>,
    component: ComponentUid,
}
impl<K: Kernel> Resolver for PointResolver<'_, '_, K> {
    type Kernel = K;
    fn kernel(&self) -> &K {
        self.ctx.kernel
    }
    fn name(&self, uid: FeatureUid) -> String {
        self.ctx.feature_name(uid)
    }
    fn datum(&mut self, uid: FeatureUid) -> Result<Datum, String> {
        self.ctx.datum(uid)
    }
    fn shape(&mut self, body: BodyUid) -> Result<K::Shape, String> {
        self.ctx.body_in(self.component, body)
    }
    fn sketch(&mut self, uid: FeatureUid) -> Result<Arc<SketchOutput>, String> {
        self.ctx.sketch(uid)
    }
    fn renamed(&mut self, what: &str, stored: &str, found: &str, maker: FeatureUid) {
        self.ctx.found_renamed(what, &stored, &found, maker);
    }
}

fn point<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    reference: &PointReference,
    from_world: &Transform,
) -> Result<[f64; 3], String> {
    let Some(path) = &reference.occurrence else {
        return resolve_point(ctx, &reference.point);
    };
    point_in(ctx, &reference.point, path, from_world)
}

/// A point of the component the occurrence path (absolute) ends in, where
/// that occurrence places it, moved by `from_world` (out of the design's
/// coordinates).
pub(crate) fn point_in<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    point: &GeomRef,
    path: &OccurrencePath,
    from_world: &Transform,
) -> Result<[f64; 3], String> {
    let component = crate::joints::path_component(ctx.env.assembly, ComponentUid::ROOT, &path.0)?;
    let transform = from_world.after(&placement(ctx, path)?);
    let p = resolve_point(&mut PointResolver { ctx, component }, point)?;
    Ok(transform.apply_point(p))
}

/// An axis of the component the occurrence path (absolute) ends in, as
/// [`point_in`] places a point.
pub(crate) fn axis_in<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    axis: &GeomRef,
    path: &OccurrencePath,
    from_world: &Transform,
) -> Result<crate::datum::DatumAxis, String> {
    let component = crate::joints::path_component(ctx.env.assembly, ComponentUid::ROOT, &path.0)?;
    let transform = from_world.after(&placement(ctx, path)?);
    let a = resolve_axis(&mut PointResolver { ctx, component }, axis)?;
    Ok(crate::datum::DatumAxis {
        origin: transform.apply_point(a.origin),
        direction: transform.apply_vector(a.direction),
    })
}

fn centre(points: &[[f64; 3]]) -> Result<[f64; 3], String> {
    let Some(first) = points.first() else {
        return Err("the sources have no usable anchor geometry".to_owned());
    };
    let mut min = *first;
    let mut max = *first;
    for p in points {
        for k in 0..3 {
            min[k] = min[k].min(p[k]);
            max[k] = max[k].max(p[k]);
        }
    }
    Ok(std::array::from_fn(|k| (min[k] + max[k]) * 0.5))
}

impl PointPatternDef {
    fn automatic<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        from_world: &Transform,
    ) -> Result<([f64; 3], &'static str), String> {
        let mut points = Vec::new();
        if let PatternObjects::Components { occurrences } = &self.objects {
            for path in occurrences {
                points.push(
                    from_world
                        .after(&placement(ctx, path)?)
                        .apply_point([0.0; 3]),
                );
            }
            return Ok((centre(&points)?, "component_origins"));
        }
        if let PatternObjects::Features { features } = &self.objects {
            for uid in features {
                if let FeatureDef::Hole(hole) = &ctx.feature_entry(*uid)?.def {
                    points.extend(hole.positions(ctx)?.iter().map(|axis| axis.origin));
                }
            }
            if !points.is_empty() {
                return Ok((centre(&points)?, "hole_placements"));
            }
        }
        let mut shapes = Vec::new();
        match &self.objects {
            PatternObjects::Bodies { bodies } => {
                for body in bodies {
                    shapes.push(ctx.body(*body)?);
                }
            }
            PatternObjects::Features { features } => {
                for uid in features {
                    if !super::dressup_copies::is_dressup(&ctx.feature_entry(*uid)?.def) {
                        shapes.push(ctx.feature_tool(*uid)?);
                    }
                }
                // Threads alone (mitcad#733): the middles of the faces they
                // thread, on the bodies before them.
                let mut threads_alone = true;
                for uid in features {
                    threads_alone &= matches!(ctx.feature_entry(*uid)?.def, FeatureDef::Thread(_));
                }
                if threads_alone {
                    for uid in features {
                        let FeatureDef::Thread(thread) = &ctx.feature_entry(*uid)?.def else {
                            continue;
                        };
                        for face in &thread.faces {
                            let Some(shape) =
                                super::dressup_copies::input_shape(ctx, *uid, face.body)
                            else {
                                continue;
                            };
                            if let Ok(c) = ctx.kernel.face_cylinder(&shape, &face.face) {
                                points.push(std::array::from_fn(|i| {
                                    c.axis.origin[i] + c.length / 2.0 * c.axis.direction[i]
                                }));
                            }
                        }
                    }
                    if !points.is_empty() {
                        return Ok((centre(&points)?, "thread_faces"));
                    }
                }
                if shapes.is_empty() {
                    for uid in features {
                        for body in ctx.feature_changed_bodies(*uid)? {
                            shapes.push(ctx.body(body)?);
                        }
                    }
                }
            }
            _ => unreachable!("checked objects"),
        }
        for shape in shapes {
            if let Some(bounds) = ctx.kernel.bounding_box(&shape).map_err(|e| e.to_string())? {
                points.extend([bounds.min, bounds.max]);
            }
        }
        Ok((centre(&points)?, "source_bounds"))
    }
}

impl<K: Kernel> Evaluate<K> for PointPatternDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        // Record even missing paths, so undo/restoration invalidates a cached
        // failure. A missing occurrence must never resolve to the origin.
        for path in self.paths() {
            for uid in &path.0 {
                ctx.placement(*uid);
            }
        }
        let context = self.context_path(ctx.env.assembly, ctx.component())?;
        let world = placement(ctx, &context)?;
        let local = inverse(&world);
        let (anchor, policy) = match &self.anchor {
            Some(reference) => (point(ctx, reference, &local)?, "selected_point"),
            None => self.automatic(ctx, &local)?,
        };
        let mut positions = vec![anchor];
        let mut elements = vec![Element {
            index: 0,
            transform: Transform::IDENTITY,
        }];
        for (i, reference) in self.targets.iter().enumerate() {
            let p = point(ctx, reference, &local)?;
            if !p.iter().all(|x| x.is_finite()) {
                return Err("a target point is not finite".to_owned());
            }
            if positions
                .iter()
                .any(|q| sub(p, *q).iter().map(|x| x * x).sum::<f64>() < 1e-14)
            {
                return Err(format!(
                    "target {} coincides with the source anchor or another target",
                    i + 1
                ));
            }
            positions.push(p);
            elements.push(Element {
                index: (i + 1) as u32,
                transform: Transform::translation(sub(p, anchor)),
            });
        }
        ctx.elements = elements.iter().map(|e| e.transform).collect();
        ctx.pattern_anchor = Some(PatternAnchor {
            point: anchor,
            world: world.apply_point(anchor),
            mode: if self.anchor.is_some() {
                "selected"
            } else {
                "automatic"
            }
            .to_owned(),
            policy: policy.to_owned(),
        });
        if let PatternObjects::Components { occurrences } = &self.objects {
            let mut placements = Vec::new();
            for source in occurrences {
                let original = local.after(&placement(ctx, source)?);
                for (target, element) in self.targets.iter().zip(&elements[1..]) {
                    let instance = ctx
                        .env
                        .assembly
                        .occurrences
                        .iter()
                        .find(|o| {
                            o.pattern.as_ref().is_some_and(|p| {
                                p.feature == ctx.uid && &p.source == source && &p.target == target
                            })
                        })
                        .ok_or_else(|| "pattern occurrence definition is missing".to_owned())?;
                    placements.push(PlacementChange::Set(
                        instance.uid,
                        element.transform.after(&original),
                    ));
                }
            }
            return Ok(FeatureOutput {
                placements,
                ..FeatureOutput::default()
            });
        }
        repeat(
            ctx,
            &self.objects,
            self.compute,
            false,
            &elements[1..],
            Combine::No,
            None,
            &[],
        )
    }
}
