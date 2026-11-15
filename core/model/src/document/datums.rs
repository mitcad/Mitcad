// SPDX-License-Identifier: MIT
//! The datums of construction features at the timeline marker, and
//! references resolved against the document (for analysis queries).

use std::sync::Arc;

use super::Document;
use crate::datum::{Datum, DatumAxis, DatumPlane, Vec3};
use crate::features::SketchOutput;
use crate::features::construction::drawn::DEFAULT_SIZE;
use crate::features::construction::{DrawnExtent, DrawnSource};
use crate::features::geom_ref::{self, GeomRef, Resolver};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::Kernel;
use crate::sweeps::PathCurve;

/// An Intersection Curve's curve (mitcad#1405), its pieces in order along
/// it, in its component's coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct FeatureCurveView {
    pub uid: FeatureUid,
    pub name: String,
    pub curve: Arc<Vec<PathCurve>>,
}

/// A construction feature's datum.
#[derive(Debug, Clone, PartialEq)]
pub struct DatumView {
    pub uid: FeatureUid,
    pub name: String,
    pub datum: Datum,
}

impl<K: Kernel> Document<K> {
    /// The datum of a construction feature before the marker, if it
    /// evaluated.
    pub fn datum(&self, uid: FeatureUid) -> Option<Datum> {
        self.result.datums.get(&uid).map(|d| d.value)
    }

    /// The datums before the marker, in timeline order.
    pub fn datums(&self) -> Vec<DatumView> {
        self.state
            .features
            .iter()
            .filter_map(|entry| {
                Some(DatumView {
                    uid: entry.uid,
                    name: entry.name.clone(),
                    datum: self.datum(entry.uid)?,
                })
            })
            .collect()
    }

    /// The curve of an Intersection Curve before the marker (mitcad#1405),
    /// its pieces in order along it, in its component's coordinates; None
    /// when it did not evaluate or failed.
    pub fn feature_curve(&self, uid: FeatureUid) -> Option<Arc<Vec<PathCurve>>> {
        let output = self.feature_result(uid)?.output.as_deref()?;
        if output.result.is_err() {
            return None;
        }
        output.curve.clone()
    }

    /// The curves of the Intersection Curves (mitcad#1405) and Helix Curves
    /// (mitcad#1777) before the marker, in timeline order, with their names.
    pub fn feature_curves(&self) -> Vec<FeatureCurveView> {
        self.state
            .features
            .iter()
            .filter(|entry| entry.def.makes_curve())
            .filter_map(|entry| {
                Some(FeatureCurveView {
                    uid: entry.uid,
                    name: entry.name.clone(),
                    curve: self.feature_curve(entry.uid)?,
                })
            })
            .collect()
    }

    /// Where a construction plane or axis before the marker is drawn
    /// (mitcad#1827): at its resize handles' size where they set one
    /// (mitcad#1748), about the middle of what it is made from; else over
    /// what it is made from; else the default size
    /// ([`DEFAULT_SIZE`]) centred on its origin. None
    /// for another datum (a point, a joint origin) or a feature without a
    /// datum.
    pub fn datum_drawn(&self, uid: FeatureUid) -> Option<(DrawnExtent, DrawnSource)> {
        let entry = self.feature(uid)?;
        if !super::has_display_size(&entry.def) {
            return None;
        }
        let datum = self.datum(uid)?;
        let fitted = self
            .feature_result(uid)
            .and_then(|result| result.output.as_deref())
            .and_then(|output| output.drawn);
        drawn_at(&datum, fitted, self.datum_display_size(uid))
    }

    /// Where the previewed construction plane or axis is drawn, as
    /// [`Self::datum_drawn`] gives the finished one: an edited feature at
    /// its own display size where it has one.
    pub fn preview_drawn(&self) -> Option<(DrawnExtent, DrawnSource)> {
        let preview = self.preview.as_ref()?;
        let datum = preview.result.datums.get(&preview.uid).map(|d| d.value)?;
        let fitted = preview
            .result
            .result(preview.uid)
            .and_then(|result| result.output.as_deref())
            .and_then(|output| output.drawn);
        let stored = self.datum_display_size(preview.uid);
        drawn_at(&datum, fitted, stored)
    }

    /// The datum of the previewed construction feature.
    pub fn preview_datum(&self) -> Option<Datum> {
        let preview = self.preview.as_ref()?;
        preview.result.datums.get(&preview.uid).map(|d| d.value)
    }

    /// The curve of the previewed Intersection Curve (mitcad#1496), as
    /// [`Self::feature_curve`] gives the finished one: its pieces in order
    /// along it, in its component's coordinates; None for another feature
    /// or when the preview failed.
    pub fn preview_curve(&self) -> Option<Arc<Vec<PathCurve>>> {
        let preview = self.preview.as_ref()?;
        let output = preview.result.result(preview.uid)?.output.as_deref()?;
        if output.result.is_err() {
            return None;
        }
        output.curve.clone()
    }

    pub fn resolver(&self) -> DocResolver<'_, K> {
        DocResolver { document: self }
    }

    /// A plane reference at the marker (see [`GeomRef`]).
    pub fn resolve_plane(&self, reference: &GeomRef) -> Result<DatumPlane, String> {
        geom_ref::resolve_plane(&mut self.resolver(), reference)
    }

    /// An axis reference at the marker.
    pub fn resolve_axis(&self, reference: &GeomRef) -> Result<DatumAxis, String> {
        geom_ref::resolve_axis(&mut self.resolver(), reference)
    }

    /// A point reference at the marker.
    pub fn resolve_point(&self, reference: &GeomRef) -> Result<Vec3, String> {
        geom_ref::resolve_point(&mut self.resolver(), reference)
    }
}

/// Where a datum is drawn: at the stored size about the middle of its
/// fitted extent (or its origin), else its fitted extent, else the default
/// size about its origin (mitcad#1827).
fn drawn_at(
    datum: &Datum,
    fitted: Option<DrawnExtent>,
    stored: Option<f64>,
) -> Option<(DrawnExtent, DrawnSource)> {
    use DrawnSource as S;
    match (fitted, stored) {
        (Some(fitted), Some(size)) => Some((fitted.sized(size), S::DisplaySize)),
        (None, Some(size)) => DrawnExtent::centred(datum, size).map(|e| (e, S::DisplaySize)),
        (Some(fitted), None) => Some((fitted, S::Fitted)),
        (None, None) => DrawnExtent::centred(datum, DEFAULT_SIZE).map(|e| (e, S::Default)),
    }
}

/// Resolves references at the timeline marker.
pub struct DocResolver<'a, K: Kernel> {
    document: &'a Document<K>,
}

impl<K: Kernel> Resolver for DocResolver<'_, K> {
    type Kernel = K;

    fn kernel(&self) -> &K {
        self.document.kernel()
    }

    fn name(&self, uid: FeatureUid) -> String {
        self.document
            .feature(uid)
            .map_or_else(|| uid.to_string(), |f| f.name.clone())
    }

    fn datum(&mut self, uid: FeatureUid) -> Result<Datum, String> {
        self.document.datum(uid).ok_or_else(|| {
            let name = self.name(uid);
            match self.document.feature(uid) {
                None => format!("feature {uid} does not exist"),
                Some(_) => format!("{name} has no result at the timeline marker"),
            }
        })
    }

    fn shape(&mut self, body: BodyUid) -> Result<K::Shape, String> {
        self.document
            .body_shape(body)
            .cloned()
            .ok_or_else(|| format!("body {body} does not exist at the timeline marker"))
    }

    fn sketch(&mut self, uid: FeatureUid) -> Result<Arc<SketchOutput>, String> {
        self.document
            .result
            .sketches
            .get(&uid)
            .map(|s| s.value.clone())
            .ok_or_else(|| format!("{} has no result at the timeline marker", self.name(uid)))
    }

    fn mount(
        &mut self,
        feature: FeatureUid,
        name: &str,
    ) -> Result<crate::transform::Transform, String> {
        let label = self.name(feature);
        crate::features::derived_part::mount_frame(
            &self.document.result.results,
            feature,
            name,
            &label,
        )
    }

    fn feature_curve(&mut self, feature: FeatureUid) -> Result<Arc<Vec<PathCurve>>, String> {
        self.document
            .feature_curve(feature)
            .ok_or_else(|| format!("{} has no curve at the timeline marker", self.name(feature)))
    }

    // A 3D sketch's curves and points (mitcad#1550).
    fn spatial_path(
        &mut self,
        path: &crate::features::spatial_path::SpatialPath,
    ) -> Result<Vec<PathCurve>, String> {
        use crate::features::spatial_path::{pieces, route};
        let name = self.name(path.sketch);
        let (Some(def), Some(output)) = (
            self.document.spatial_sketch_def(path.sketch),
            self.document.spatial_sketch(path.sketch),
        ) else {
            return Err(format!("{name} has no result at the timeline marker"));
        };
        let legs = route(def, path, &name, &|p| output.point(p))?;
        pieces(output, &legs, &name)
    }

    fn spatial_entity(
        &mut self,
        sketch: FeatureUid,
        entity: crate::sketch::Ref,
    ) -> Option<Result<crate::kernel::Curve3, String>> {
        self.document.spatial_sketch_def(sketch)?;
        let name = self.name(sketch);
        Some(
            self.document
                .spatial_sketch(sketch)
                .and_then(|output| output.entity_curve(entity))
                .ok_or_else(|| format!("{name} has no {entity} at the timeline marker")),
        )
    }
}
