// SPDX-License-Identifier: MIT
//! Mitcad parametric model: parameters, features in a timeline, the body
//! state they build, topological names, recompute with a cache, undo and
//! redo, the project file and the JSON command interface.
//!
//! The model does not depend on any geometry kernel. Solid operations go
//! through the [`Kernel`] trait, so the timeline and recompute logic is
//! tested without OCCT (see the mock kernel in `testing.rs`).

pub mod analysis;
pub mod api;
// Appearances: physically based looks of bodies (mitcad#46).
pub mod appearance;
pub mod assembly;
mod base64;
pub mod datum;
mod document;
pub mod exchange;
pub mod expr;
pub mod features;
pub mod file;
mod ids;
mod kernel;
mod parameters;
mod profile;
mod recompute;
pub mod sketch;
pub mod topo;
mod transform;

// Components and occurrences (F6).
#[cfg(test)]
mod component_tests;
#[cfg(test)]
mod document_tests;
/// The mock kernel of the model tests; other crates' tests use it through
/// the `testing` feature (the .f3d importer, T1).
#[cfg(any(test, feature = "testing"))]
pub mod testing;
// Profile features (F1).
#[cfg(test)]
mod profile_feature_tests;

pub use document::{
    Added, BodyView, DocState, Document, FeatureError, ModelError, PreviewReport, ProfileView,
    RecomputeStats, ShapeAdded, TimelineItem,
};
pub use exchange::{BaseInput, BodyKind, ImportBody, Imported};
pub use features::{FeatureDef, FeatureEntry, ValueInput};
pub use file::FileError;
pub use ids::{BodyUid, EntityUid, FeatureUid, IdError};
pub use kernel::{
    BooleanOp, BooleanOutput, BooleanPiece, BoundingBox, Chamfer, ChamferSize, Curve3, EdgeInfo,
    ExtrudeSpec, FaceInfo, Kernel, KernelError, MassProperties,
};
// .f3d import (T1d).
pub use kernel::EdgeMiddle;
// Replace face of curved faces (P5).
pub use kernel::FacePoints;
// FreeCAD import (.FCStd).
pub use kernel::{ElementKind, IndexedElement};
// Profile features (F1).
pub use kernel::{
    Axis, Bound, Cylinder, ExtrudeFeatureSpec, ExtrudeSide, ExtrudeStart, HoleEnd, HoleShape,
    HoleSpec, Plane, RevolveSpec, SideEnd, Target, ThinWall, ThreadSpec, WallLocation,
};
pub use parameters::{ChangeSet, ParamId, Parameter, ParameterError, Parameters};
pub use profile::{ProfileLoop, ProfileRegion, ProfileSegment, SegmentGeometry, SketchFrame};
pub use recompute::FeatureStatus;
pub use topo::{EdgeName, FaceName, RegionKey, SegmentEnd, SegmentKey, TopoName, VertexName};
pub use transform::{Instance, PrimitiveShape, PrimitiveSpec, Transform};

// Face operations (F2).
pub use kernel::{DraftSpec, FilletSet, FilletSize, ShellSpec, ToolInput};
// Construction geometry, analysis and body attributes (F5).
pub use datum::{Datum, DatumAxis, DatumKind, DatumPlane, DatumPoint, OriginDatum};
pub use document::{BodyAttributes, DatumView, DeletedAppearance, FaceAppearance};
pub use features::geom_ref::{GeomRef, PathRef};
// Sweeps, lofts, pipes, coils, ribs and webs (F3).
pub mod sweeps;
pub use sweeps::{
    CoilPosition, CoilSection, CoilSpec, LoftEnd, LoftEndKind, LoftSection, LoftSpec, PathCurve,
    PipeSection, PipeSpec, ProfileScaling, RibSpec, SweepGuide, SweepOrientation, SweepSpec,
    ThicknessLocation,
};
// FreeCAD helices (mitcad#4).
pub use sweeps::HelixSpec;
// Components and occurrences (F6).
pub use document::{InsertOptions, InstanceView};
pub use ids::{ComponentUid, OccurrenceUid};
// Named views (U5).
pub use document::NamedView;
// Analyses kept in the document (mitcad#41).
pub use document::{Analysis, AnalysisDef, SectionAnalysis, SectionPlane};
// Sketch text (P3).
pub use kernel::{FontGlyphs, FontRequest, Glyph};
pub use topo::CurveId;
// Progress and cancellation of recomputes (P7).
pub mod monitor;
pub use monitor::{Cancelled, Progress, RecomputeMonitor};
#[cfg(test)]
mod monitor_tests;
// Deterministic result versions and the result store (P7d).
mod fingerprint;
pub mod store;
pub use store::{GcReport, PersistReport, ResultStore, StoreStats};
#[cfg(test)]
mod cache_tests;
#[cfg(test)]
mod store_tests;
#[cfg(test)]
mod version_tests;
// Project files of version 3 and their B-rep store (P12a).
mod sha256;
pub use file::{BlobStore, FileFormat, FsStore, MemoryStore, Project, ProjectError, Sha256};
// Comparison of two designs or versions (P12c).
pub mod diff;
pub use diff::DesignDiff;
// Render settings of the document (mitcad#47).
pub mod render_settings;
pub use render_settings::RenderSettings;
// Joints between occurrences (mitcad#55).
#[cfg(test)]
mod joint_solver_tests;
#[cfg(test)]
mod joint_tests;
pub mod joints;

// Configuration tables and library components (mitcad#64, mitcad#63).
pub mod configurations;
pub mod library;
pub use configurations::{ConfigurationRow, Configurations};
pub use document::{LibraryChange, LibraryPart, PartsListRow};
pub use library::{LibraryRef, LibraryRequest, LibrarySource, LinkResolver, set_link_resolver};
#[cfg(test)]
mod library_tests;

// Geometry of other components in sketches (mitcad#100).
pub mod links;
pub use links::OccurrenceLink;
#[cfg(test)]
mod link_tests;

// An operation's new faces (the .f3d import's geometric check, mitcad#138).
pub use kernel::NewFace;
