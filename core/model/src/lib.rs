// SPDX-License-Identifier: MIT
//! Mitcad parametric model: parameters, features in a timeline, the body
//! state they build, topological names, recompute with a cache, undo and
//! redo, the design file and the JSON command interface.
//!
//! The model does not depend on any geometry kernel. Solid operations go
//! through the [`Kernel`] trait, so the timeline and recompute logic is
//! tested without OCCT (see the mock kernel, the crate
//! `mitcad-model-testkit`).

pub use mitcad_model_base::analysis;
// Appearances: physically based looks of bodies (mitcad#46).
pub use mitcad_model_base::appearance;
pub mod assembly;
// Base64, fingerprints and SHA-256 of the model base, at the paths the
// model names them by; other Mitcad crates reach them through `internal`.
pub use mitcad_model_base::datum;
use mitcad_model_base::internal::base64;
mod document;
pub mod exchange;
// Parameter expressions with units: the crate mitcad-expr (mitcad#777).
pub use mitcad_expr as expr;
pub mod features;
pub mod file;
pub use mitcad_model_base::ids;
// Items other Mitcad crates use; not part of this crate's API (mitcad#811).
pub mod internal;
pub use mitcad_model_base::kernel;
// Materials as objects: density, appearance and filament (mitcad#640).
pub mod material;
pub use mitcad_model_base::parameters;
mod print_fits;
// The print settings' key map: printer, process and filament keys and
// their slicer dialects (mitcad#640); the crate mitcad-print-keys
// (mitcad#777).
pub use mitcad_model_base::profile;
pub use mitcad_print_keys as print_keys;
mod recompute;
// Features found by uid without a search (mitcad#841).
mod feature_index;
pub mod sheet;
// The general sketch, a crate below the core (mitcad#780).
pub use mitcad_model_base::topo;
pub use mitcad_model_base::transform;
pub use mitcad_sketch as sketch;

// Components and occurrences (F6).
#[cfg(test)]
mod document_tests;
/// Fixtures the model's tests share (the block document, feature
/// definitions, face and edge names); test crates use them through the
/// `testing` feature (mitcad#777).
#[cfg(any(test, feature = "testing"))]
pub mod test_support;
/// The mock kernel of the model tests; other crates' tests use it through
/// the `testing` feature (the .f3d importer, T1). The crate
/// mitcad-model-testkit (mitcad#777).
#[cfg(any(test, feature = "testing"))]
pub use mitcad_model_testkit as testing;

pub use document::{
    Added, BodyView, ComponentCopy, DocState, Document, FeatureError, ModelError, PreviewReport,
    ProfileView, RecomputeStats, ShapeAdded, TimelineItem,
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
// The faces a run of features made, copied by a pattern.
pub use kernel::{AddedMaterial, FeatureChanges};
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
// Bodies handed out as soon as no later feature changes them (mitcad#881).
pub use recompute::{FinalBodies, FinalBody};
pub use topo::{EdgeName, FaceName, RegionKey, SegmentEnd, SegmentKey, TopoName, VertexName};
pub use transform::{Instance, PrimitiveShape, PrimitiveSpec, Transform};

// Face operations (F2).
pub use kernel::{DraftSpec, FilletSet, FilletSize, ShellSpec, ToolInput};
// Construction geometry, analysis and body attributes (F5).
pub use datum::{Datum, DatumAxis, DatumKind, DatumPlane, DatumPoint, OriginDatum};
pub use document::{BodyAttributes, DatumView, DeletedAppearance, FaceAppearance};
// Appearance, material, suppression and visibility from expressions
// (mitcad#800).
pub use document::{BindingKind, Bindings};
pub use features::geom_ref::{GeomRef, PathRef};
// Sweeps, lofts, pipes, coils, ribs and webs (F3).
pub use mitcad_model_base::sweeps;
pub use sweeps::{
    CoilPosition, CoilSection, CoilSpec, LoftEnd, LoftEndKind, LoftSection, LoftSpec, PathCurve,
    PipeSection, PipeSpec, ProfileScaling, RibSpec, SweepGuide, SweepOrientation, SweepSpec,
    ThicknessLocation,
};
// FreeCAD helices (mitcad#4).
pub use sweeps::HelixSpec;
// Sweeps' rail points (mitcad#469).
pub use sweeps::RailPoint;
// Components and occurrences (F6).
pub use document::{InsertOptions, InstanceView};
pub use ids::{ComponentUid, OccurrenceUid};
// Named views (U5).
pub use document::NamedView;
// Analyses kept in the document (mitcad#41).
pub use document::{Analysis, AnalysisDef, SectionAnalysis, SectionPlane};
// Sketch text (P3).
pub use kernel::{ConeFace, FontGlyphs, FontRequest, FontSource, Glyph};
pub use topo::CurveId;
// Sweeps along guide surfaces (mitcad#157).
pub use sweeps::GuideSurface;
// Progress and cancellation of recomputes (P7).
pub use mitcad_model_base::monitor;
pub use monitor::{Cancelled, Progress, RecomputeMonitor};
// Deterministic result versions and the result store (P7d).
use mitcad_model_base::internal::fingerprint;
pub mod store;
pub use store::{GcReport, PersistReport, ResultStore, StoreStats};
#[cfg(test)]
mod version_tests;
// Design files of version 3 and their projects' B-rep store (P12a).
pub use file::{BlobStore, DesignError, FileFormat, FsStore, MemoryStore, Project, Sha256};
use mitcad_model_base::internal::sha256;
// Render settings of the document (mitcad#47).
pub use mitcad_model_base::render_settings;
pub use render_settings::RenderSettings;
// Joints between occurrences (mitcad#55).
pub mod joints;

// Configuration tables and library components (mitcad#64, mitcad#63).
pub use mitcad_model_base::configurations;
pub mod library;
pub use configurations::{ConfigurationRow, ConfigurationRule, Configurations, RuleAction};
pub use document::{ConfigurationsSet, LibraryChange, LibraryPart, PartsListRow};
pub use library::{LibraryRef, LibraryRequest, LibrarySource, LinkResolver, set_link_resolver};

// Geometry of other components in sketches (mitcad#100).
pub mod links;
pub use links::OccurrenceLink;

// An operation's new faces (the .f3d import's geometric check, mitcad#138).
pub use kernel::NewFace;

// Assembly constraints (mitcad#148).
pub mod constraints;

// Move Features into a Component (mitcad#171).
pub use document::AutoselectProfiles;
pub use document::{MoveConflict, MoveGroup, MovePlan, MovedGroup, PlannedGroup};

// Design policy: per-component creation and link permissions (mitcad#220).
pub mod policy;
pub use document::{PolicyItem, PolicyReport};
pub use policy::{Policy, Setting};
// Bodies from their boundaries and from triangles (the IFC import,
// mitcad#273).
pub use document::FeatureBatch;
pub use mitcad_model_base::boundary;
// Extrude's automatic operation: the contacts of its tool (mitcad#289).
pub use document::AutoOperation;
pub use kernel::ToolContact;
// The BUILD tab: library sources, derived parts, bound lengths and the
// placement rules of slot-profile assemblies (mitcad#375).
pub mod build;
// Section curves of sketches (mitcad#389).
pub use kernel::SectionCurve;
// Body ids of a split's pieces after upstream edits (mitcad#408).
#[cfg(test)]
mod piece_ids_tests;
// Motion studies and contact sets of assemblies (mitcad#454).
pub mod contact;
pub mod motion;
pub use contact::{ContactBody, ContactHit};
pub use document::{Driven, JointValues, MotionOutcome};
pub use kernel::PlacedPair;
pub use motion::{ContactSet, MotionStudy, StudyKey};
// Motion links between joints: gears, racks, screws and cams (mitcad#466).
pub mod motion_links;
pub use motion_links::LinkResult;
// Emboss: profiles raised or sunk on faces, wrapped on cylinders (mitcad#458).
#[cfg(test)]
mod emboss_tests;
pub use kernel::EmbossWrapSpec;
// Boundary Fill: solids of the cells tool bodies enclose (mitcad#457).
pub use kernel::BoundaryCell;
#[cfg(test)]
mod boundary_fill_tests;
// Patch, Stitch and Thicken: surface bodies filled, sewn and made solid
// (mitcad#456).
pub use mitcad_model_base::surfaces;
pub use surfaces::{OpenBoundary, PatchBoundary, PatchContinuity, PatchLoopSpec};
#[cfg(test)]
mod surface_tests;
// Surface modelling: surface bodies and their features (mitcad#455).
pub use kernel::Unstitched;
pub use mitcad_model_base::surface_modelling;
pub use surface_modelling::{
    BlendEnd, BlendSpec, BodyEdges, BoundaryCurve, BoundaryCurves, BoundarySpec, ExtendMode,
    Fairing, OpeningEdges, RuledCorners, RuledMode, RuledSpec, TrimSpec, TrimTool,
};
// Plastic part features and plastic rules (mitcad#459).
pub use features::plastic::PlasticValues;
pub use kernel::RimBandSpec;
// Bodies moved into another component on the timeline (mitcad#465).
pub use document::TransferDestination;
// Drawings: views with dimensions and a hole table as PDF, SVG or DXF
// (mitcad#471).
pub mod drawing;
// Gears: spur, helical, herringbone, ring, rack and bevel (mitcad#472).
pub use kernel::{GearConeSpec, GearStep, GearSweepSpec};
// Typed property sets on components, occurrences and bodies (mitcad#496).
pub use document::PropertyTypeChange;
pub use mitcad_model_base::properties;
// Reorient Design: the whole design turned by quarter turns (mitcad#502).
pub mod reorient;
pub use document::ReorientReport;
// Mount frames of any part and placing a part by one (mitcad#525).
pub use document::{
    GridPlace, Inserted, MountAt, MountFrame, MountInput, MountPlan, MountPreview, MountSource,
};
pub use features::joint::{MountKind, MountSpec};
// Extrude: a planar body face as the profile (mitcad#535).
pub use kernel::FaceExtrudeSpec;
// Revolve: a planar body face as the profile (mitcad#539).
pub use kernel::FaceRevolveSpec;
#[cfg(test)]
mod face_revolve_tests;
// Local replay of modern scoped Extrude Cuts (mitcad#569).
#[cfg(test)]
mod scoped_cut_replay_tests;
// Print Analysis: overhangs, thin walls and a print estimate on the
// bodies' triangle meshes (mitcad#620).
pub mod print_analysis;
pub use document::{EstimateCheck, OverhangCheck, PrintAnalysis, ThinWallCheck};
#[cfg(test)]
mod print_analysis_tests;
// Materials as objects and components' default materials (mitcad#640).
pub use document::{AppearanceChoice, DeletedMaterial, MaterialSource};
// Printer and process profiles and what a body prints with (mitcad#640,
// part 3).
pub mod print_profile;
pub use document::{EffectivePrint, PrintFrom, PrintTarget};
// Libraries of materials and print profiles: base, user and project
// (mitcad#640, part 4).
pub mod print_library;
pub use document::LibraryUpdate;
// Slicers' settings files: import and export by dialect (mitcad#640, part
// 5).
pub mod print_files;
pub use document::read_print_file;
// Purge volumes on filament changes: the materials' values, the computed
// table and the design's overrides (mitcad#705).
pub mod print_purge;
// Annotations kept in the design (mitcad#702), grown from the measurement
// objects of mitcad#649, and the ISO 286 fit codes of their tolerances.
pub use document::{
    AUTO_COLOR, Annotation, AnnotationFormat, AnnotationPlane, Characteristic, DatumNeed,
    GeometricTolerance, MAX_DECIMALS, MaterialCondition, MeasureKind, MeasurePick, Measurement,
    NewMeasurement, OLD_DEFAULT_COLORS, TEXT_HEIGHTS, Tolerance, automatic_text_height,
    check_text_height, datum_letter,
};
pub use mitcad_model_base::iso286;
// The parameters as CSV: Mitcad's table and the `.f3d` application's
// parameter table (mitcad#666).
pub mod parameters_csv;
pub use document::{CsvAction, CsvImport, CsvLimits, CsvRowOutcome};
#[cfg(test)]
mod parameters_csv_tests;
// A body's print bed face, which the 3D print export turns down onto the
// plate (mitcad#731).
pub use document::PrintBedFace;
// Types of the document's methods that other crates name (mitcad#811).
pub use document::{Isolated, PlaneShape, PurgeChange, SketchPlaneChange};
// Image references: images placed on planes and faces (mitcad#782), and
// the sizes of image files from their headers.
pub use document::{
    CalibrationPoint, EmbeddedImage, ImageEdit, ImageReference, ImageSize, ImageSource, ImageTarget,
};
pub use mitcad_model_base::image_header;
// The user presets of an installed Bambu-family slicer: what a ticked
// preset of a name the target has does (mitcad#830).
pub use document::{PresetChoice, SameName};
// Options of inserted components: library parts, linked designs and BUILD
// parts as expressions of the design's parameters (mitcad#801).
pub mod part_options;
pub use document::{ComponentOptions, OptionsChange, OptionsChanged};
// Tables of data that parameter expressions look values up in
// (mitcad#667, `docs/table-lookups.md`).
pub mod tables;
pub use tables::{Table, TableCell, TableColumn, Tables};
#[cfg(test)]
mod tables_tests;
// A table of the design as CSV: one file per table, units in the header
// (mitcad#804).
pub mod tables_csv;
pub use document::{TableCsvChange, TableCsvFailure, TableCsvImport};
#[cfg(test)]
mod tables_csv_tests;
// Readers of files the user chose, a panic of theirs an error, not an
// abort of the application (mitcad#922).
pub use mitcad_model_base::reader_guard;
