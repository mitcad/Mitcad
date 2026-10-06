// SPDX-License-Identifier: MIT
//! The datums of construction features at the timeline marker, and
//! references resolved against the document (for analysis queries).

use std::sync::Arc;

use super::Document;
use crate::datum::{Datum, DatumAxis, DatumPlane, Vec3};
use crate::features::SketchOutput;
use crate::features::geom_ref::{self, GeomRef, Resolver};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::Kernel;

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

    /// The datum of the previewed construction feature.
    pub fn preview_datum(&self) -> Option<Datum> {
        let preview = self.preview.as_ref()?;
        preview.result.datums.get(&preview.uid).map(|d| d.value)
    }

    pub(crate) fn resolver(&self) -> DocResolver<'_, K> {
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

/// Resolves references at the timeline marker.
pub(crate) struct DocResolver<'a, K: Kernel> {
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
}
