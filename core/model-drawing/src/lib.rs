// SPDX-License-Identifier: MIT
//! The drawing layout and writers of Mitcad (mitcad#471, mitcad#707), a
//! crate of its own above the model (mitcad#777): orthographic views of
//! bodies with hidden lines removed, their dimensions and annotations,
//! hole tables and parts lists, a frame and a title block from a sheet
//! template on a sheet, written as PDF, SVG, DXF or DWG (the
//! `export_drawing` command and the drawings kept in the design,
//! `model-api/src/drawing.rs` and `drawings.rs`, commands.md).
//!
//! The kernel projects the views (`Kernel::project_view`) and finds the
//! round holes (`Kernel::round_holes`); this crate lays them out on the
//! sheet ([`compose`]) and writes the files ([`svg`], [`pdf`], [`dxf`],
//! [`dwg`]) without any geometry library. Detail views and breaks cut the
//! projected lines in the view's plane ([`clip`]).
//!
//! What a design keeps of a drawing (its definition, [`def`], with the
//! stored annotations, tables and templates) stays in the model
//! (`mitcad_model::drawing`), which every design file reads; this crate
//! re-exports all of it under the same paths, so `mitcad_model_drawing::…`
//! names everything of drawings, and adds the layout to the modules
//! [`annotate`], [`tables`] and [`template`].

pub mod annotate;
// B-splines cut and turned exactly (mitcad#853).
pub mod bspline;
pub mod clip;
pub mod compose;
// Details smaller than the pen as sharp corners, threads by ISO 6410
// (mitcad#1112).
pub mod convention;
// Cut sheets: flat parts' outlines at 1:1 as DXF and SVG (mitcad#853).
pub mod cut_sheet;
pub mod dwg;
pub mod dxf;
pub mod pdf;
// A title block's images as PDF images (mitcad#1625).
pub mod pdf_image;
pub mod sheet;
// Dimensions snapped to the spacing of the one placed before
// (mitcad#1173).
pub mod spacing;
pub mod svg;
// A title block's SVG images as paths for PDF, DXF and DWG (mitcad#1627).
pub mod svg_image;
pub mod tables;
pub mod template;

use std::sync::Arc;

use mitcad_model::Document;
use mitcad_model::kernel::Kernel;

// The model's drawing definitions, the command's options and the view
// types the kernel works out, under the paths they had in the model.
pub use mitcad_model::drawing::*;

// --- Views projected before -------------------------------------------------

/// How many projections (and shadings) [`ProjectionCache`] keeps.
const CACHED_PROJECTIONS: usize = 64;
/// How many bodies' edges and round faces it keeps: one per body a sheet
/// shows, so a bound of its own, well above the views' (mitcad#717: a
/// sheet of more than 64 bodies threw them out on every draw). A body's
/// older versions go when a new one is kept (mitcad#718), so this bounds
/// bodies, not versions.
const CACHED_GEOMETRIES: usize = 4096;
/// How many placed bodies' round holes it keeps (hole tables and the
/// automatic annotations, mitcad#718).
const CACHED_HOLES: usize = 1024;
/// How many answers of one body on one section, detail or broken view it
/// keeps (what a parts list counts and a pick's cut faces, mitcad#722):
/// one per body and view, so a bound of its own like the geometries', and
/// apart from the views' projections, which they would push out.
const CACHED_BODY_VIEWS: usize = 4096;

type Entries<T> = std::sync::Mutex<std::collections::VecDeque<(u128, std::sync::Arc<T>)>>;
/// Bodies' geometries, each with its body's own key and its version's.
type BodyEntries = std::sync::Mutex<
    std::collections::VecDeque<(u128, u128, std::sync::Arc<annotate::BodyGeometry>)>,
>;

/// Views the kernel projected (and shaded views' faces), by the
/// fingerprint of what they show and how (the bodies' shape versions and
/// placements, the view's spec), so that asking for a drawing again
/// projects only what changed (mitcad#707). Separate from modelling
/// results: drawing changes no state. The least recently used go first.
#[derive(Default)]
#[doc(hidden)]
pub struct ProjectionCache {
    inner: Entries<ViewProjection>,
    shading: Entries<Shading>,
    /// The bodies' named edges and round faces (annotations, mitcad#707
    /// phase 2), by body and shape version; each with the body's own key,
    /// so that a new version replaces the old one.
    geometry: BodyEntries,
    /// A placed body's round holes with their threads, by body, shape
    /// version, placement and threads; found without a warning (a failure
    /// is tried again, mitcad#722).
    holes: Entries<Holes>,
    /// A body's volume, by body and shape version (a title block's mass,
    /// mitcad#707 phase 4).
    volumes: Entries<f64>,
    /// What one body shows on a section, a detail or a broken view, by
    /// body, shape version, placement, view spec, circle and breaks
    /// (mitcad#722).
    body_views: Entries<BodyInView>,
    /// Cut sheets' parts worked out (mitcad#853), by body, shape version,
    /// placement and the face chosen.
    part_views: Entries<cut_sheet::PartView>,
    /// The `threads` query's answer with the recompute it was asked after
    /// (mitcad#1112: every view draws its threads by convention, and
    /// measuring a modelled thread's face is slow, mitcad#878).
    threads: std::sync::Mutex<Option<(u64, std::sync::Arc<serde_json::Value>)>>,
}

/// What one body shows on a section, a detail or a broken view, from its
/// own projection there: whether any line of it is left (a parts list
/// counts it, mitcad#718) and its cut faces as the view shows them (picks
/// on a section's cut faces).
#[derive(Debug, Clone, Default, PartialEq)]
#[doc(hidden)]
pub struct BodyInView {
    pub seen: bool,
    pub cuts: Vec<CutFace>,
}

/// A shaded view's faces: polygons in view coordinates with their greys,
/// the farthest first.
#[doc(hidden)]
pub type Shading = Vec<(Vec<P2>, f64)>;

/// A placed body's round holes.
pub(crate) type Holes = Vec<compose::HoleInput>;

fn cached<T>(entries: &Entries<T>, key: u128) -> Option<std::sync::Arc<T>> {
    // Least recently used first: a hit moves to the back.
    let mut entries = entries.lock().ok()?;
    let i = entries.iter().position(|(k, _)| *k == key)?;
    let entry = entries.remove(i)?;
    let value = std::sync::Arc::clone(&entry.1);
    entries.push_back(entry);
    Some(value)
}

fn keep<T>(entries: &Entries<T>, key: u128, value: std::sync::Arc<T>, limit: usize) {
    if let Ok(mut entries) = entries.lock() {
        entries.retain(|(k, _)| *k != key);
        entries.push_back((key, value));
        while entries.len() > limit {
            entries.pop_front();
        }
    }
}

impl ProjectionCache {
    pub fn projection(&self, key: u128) -> Option<std::sync::Arc<ViewProjection>> {
        cached(&self.inner, key)
    }

    pub fn keep_projection(&self, key: u128, value: std::sync::Arc<ViewProjection>) {
        keep(&self.inner, key, value, CACHED_PROJECTIONS);
    }

    pub fn shading(&self, key: u128) -> Option<std::sync::Arc<Shading>> {
        cached(&self.shading, key)
    }

    pub fn keep_shading(&self, key: u128, value: std::sync::Arc<Shading>) {
        keep(&self.shading, key, value, CACHED_PROJECTIONS);
    }

    pub fn geometry(&self, key: u128) -> Option<std::sync::Arc<annotate::BodyGeometry>> {
        let mut entries = self.geometry.lock().ok()?;
        let i = entries.iter().position(|(_, k, _)| *k == key)?;
        let entry = entries.remove(i)?;
        let value = std::sync::Arc::clone(&entry.2);
        entries.push_back(entry);
        Some(value)
    }

    /// Keeps a body's geometry (`body`: the body's own key, `key`: its
    /// version's), throwing out the body's older versions.
    pub fn keep_geometry(
        &self,
        body: u128,
        key: u128,
        value: std::sync::Arc<annotate::BodyGeometry>,
    ) {
        if let Ok(mut entries) = self.geometry.lock() {
            entries.retain(|(b, k, _)| *b != body && *k != key);
            entries.push_back((body, key, value));
            while entries.len() > CACHED_GEOMETRIES {
                entries.pop_front();
            }
        }
    }

    /// How many bodies' geometries it keeps.
    pub fn geometries(&self) -> usize {
        self.geometry.lock().map_or(0, |e| e.len())
    }

    pub fn holes(&self, key: u128) -> Option<std::sync::Arc<Holes>> {
        cached(&self.holes, key)
    }

    pub fn keep_holes(&self, key: u128, value: std::sync::Arc<Holes>) {
        keep(&self.holes, key, value, CACHED_HOLES);
    }

    pub fn volume(&self, key: u128) -> Option<f64> {
        cached(&self.volumes, key).map(|v| *v)
    }

    pub fn keep_volume(&self, key: u128, volume: f64) {
        keep(
            &self.volumes,
            key,
            std::sync::Arc::new(volume),
            CACHED_GEOMETRIES,
        );
    }

    pub fn body_view(&self, key: u128) -> Option<std::sync::Arc<BodyInView>> {
        cached(&self.body_views, key)
    }

    pub fn keep_body_view(&self, key: u128, value: std::sync::Arc<BodyInView>) {
        keep(&self.body_views, key, value, CACHED_BODY_VIEWS);
    }

    pub fn part_view(&self, key: u128) -> Option<std::sync::Arc<cut_sheet::PartView>> {
        cached(&self.part_views, key)
    }

    pub fn keep_part_view(&self, key: u128, value: std::sync::Arc<cut_sheet::PartView>) {
        keep(&self.part_views, key, value, CACHED_BODY_VIEWS);
    }

    /// The `threads` query's answer kept after the recompute `recompute`.
    pub fn threads(&self, recompute: u64) -> Option<std::sync::Arc<serde_json::Value>> {
        let kept = self.threads.lock().ok()?;
        kept.as_ref()
            .filter(|(r, _)| *r == recompute)
            .map(|(_, v)| std::sync::Arc::clone(v))
    }

    pub fn keep_threads(&self, recompute: u64, value: std::sync::Arc<serde_json::Value>) {
        if let Ok(mut kept) = self.threads.lock() {
            *kept = Some((recompute, value));
        }
    }

    /// How many views' projections it keeps.
    pub fn projections(&self) -> usize {
        self.inner.lock().map_or(0, |e| e.len())
    }

    /// How many bodies' answers on partial views it keeps.
    pub fn body_views(&self) -> usize {
        self.body_views.lock().map_or(0, |e| e.len())
    }
}

/// The drawing views a document projected before (mitcad#707), kept in
/// its side cache (mitcad#777: [`Document::side_cache`]).
pub trait DrawingCache {
    /// The drawing views projected before.
    fn drawing_cache(&self) -> Arc<ProjectionCache>;
}

impl<K: Kernel> DrawingCache for Document<K> {
    fn drawing_cache(&self) -> Arc<ProjectionCache> {
        self.side_cache()
    }
}

/// Items other Mitcad crates use; not part of this crate's API.
///
/// The JSON api (`mitcad-model-api`) keeps the projections and body views
/// a drawing works out, and lays out hole callouts and sketch dimensions
/// with these helpers (mitcad#811).
pub mod internal {
    pub use crate::compose::{
        CalloutStyle, HoleInput, HoleText, hole_callout, hole_text, sketch_dimension,
    };
    pub use crate::{BodyInView, ProjectionCache, Shading};
}
