// SPDX-License-Identifier: MIT
//! The design dump IR: the JSON of `core/import/SCHEMA.md`
//! (schema version 2; version 1 dumps read as well) as Rust types.
//!
//! The `.f3d` stream decoder writes it (`mitcad_f3d::design::Design`, `source.mode`
//! `"f3d_stream"`), and so may external tools (the reference dumps of the
//! tests). The importer reads either through these types.
//!
//! Conventions:
//! - Every key is optional (`Option`); the decoder leaves out what it
//!   cannot decode. `Option<Option<T>>` fields tell a JSON `null`
//!   (`Some(None)`) from an absent key (`None`).
//! - Units are the file's internal ones: cm, rad (SCHEMA.md §1).
//! - Unmodelled keys, and modelled keys whose value has another shape (an
//!   `{"_error": ...}` of an external dump, say), are kept in each
//!   record's `other` map, so a dump survives a read-write round trip.
//! - `_f3d` keys hold decoder-only data (class GUIDs, object ids, raw flags);
//!   schema consumers may ignore them.
//! - Feature `detail` fields are typed for the features the decoder
//!   writes and the main fields of SCHEMA.md §5.3; other feature
//!   types stay [`Detail::Other`].

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

mod record;
// An item's state from the state bytes of its tail (mitcad#180).
pub mod state;

use crate::record::record;
pub use crate::state::{ItemState, Suppression, item_state, item_suppression};

/// `[x, y, z]`.
pub use mitcad_vec3::Vec3;
/// Row-major 4x4 matrix (`[row][column]`, translation in column 3).
pub type Mat4 = [[f64; 4]; 4];

/// The identity matrix.
pub const IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// Value of `schema`.
pub const SCHEMA_NAME: &str = "mitcad-f3d-dump";
/// Value of `schema_version` written by the decoder.
pub const SCHEMA_VERSION: i64 = 2;

record! {
    /// A design dump (SCHEMA.md §5).
    pub struct Dump {
        "schema" => schema: String,
        "schema_version" => schema_version: i64,
        "generator" => generator: Generator,
        "units" => units: BTreeMap<String, String>,
        "source" => source: Source,
        "options" => options: Value,
        "warnings" => warnings: Vec<String>,
        "document" => document: Document,
        "parameters" => parameters: Parameters,
        "components" => components: Vec<Component>,
        "occurrences" => occurrences: Vec<OccurrenceNode>,
        "timeline" => timeline: Timeline,
        "steps" => steps: Value,
        "final" => final_state: Value,
        "errors" => errors: Vec<SectionError>,
        "timing" => timing: Value,
        "exports" => exports: Value,
        /// Stream decoder data (format, coverage, blobs).
        "_f3d" => f3d: DumpF3d,
    }
}

impl Dump {
    /// Reads a dump (an external one or the decoder's) from JSON text.
    pub fn from_json(text: &str) -> Result<Dump, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// Timeline items, empty if there is no timeline.
    pub fn timeline_items(&self) -> &[TimelineItem] {
        self.timeline
            .as_ref()
            .and_then(|t| t.items.as_deref())
            .unwrap_or(&[])
    }

    /// All parameters: user parameters first, then model parameters.
    pub fn all_parameters(&self) -> impl Iterator<Item = &Parameter> {
        let p = self.parameters.as_ref();
        let user = p.and_then(|p| p.user.as_deref()).unwrap_or(&[]);
        let model = p.and_then(|p| p.model.as_deref()).unwrap_or(&[]);
        user.iter().chain(model)
    }
}

record! {
    /// Who wrote the dump.
    pub struct Generator {
        "addin" => addin: String,
        "addin_version" => addin_version: String,
        "app_version" => app_version: String,
        "python" => python: String,
        "os" => os: String,
        "created_utc" => created_utc: String,
        /// Stream decoder: its name.
        "decoder" => decoder: String,
        /// Stream decoder: the version of the application that wrote the file.
        "writer" => writer: String,
    }
}

record! {
    /// Where the design came from (SCHEMA.md §5.1).
    pub struct Source {
        /// `local_path`, `data_file_id`, `active`, `script` (external) or
        /// `f3d_stream` (decoder).
        "mode" => mode: String,
        /// Decoder: the file (`name.f3d`, `pkg.f3z!<doc>`).
        "file" => file: String,
        /// Decoder: the segment folder (`Design1`, `FusionDesignSegmentType1`).
        "segment" => segment: String,
        "document_owned_by_addin" => document_owned_by_addin: bool,
        "job_id" => job_id: String,
        "export_name" => export_name: String,
    }
}

record! {
    /// A failed dump section.
    pub struct SectionError {
        "section" => section: String,
        "error" => error: String,
    }
}

record! {
    /// SCHEMA.md §5.1.
    pub struct Document {
        "name" => name: String,
        "isModified" => is_modified: bool,
        "app_version" => app_version: String,
        /// `ParametricDesignType` or `DirectDesignType`.
        "design_type" => design_type: String,
        "default_length_units" => default_length_units: String,
        "distance_display_units" => distance_display_units: String,
        "root_component" => root_component: String,
        "component_count" => component_count: i64,
        "data_file" => data_file: Value,
    }
}

record! {
    /// User and model parameters (SCHEMA.md §5.2).
    pub struct Parameters {
        "user" => user: Vec<Parameter>,
        "model" => model: Vec<Parameter>,
    }
}

record! {
    /// A user or model parameter.
    pub struct Parameter {
        "name" => name: String,
        /// The expression as typed, e.g. `( 13 / 3 ) * 1 mm`.
        "expression" => expression: String,
        /// Value in internal units (cm, rad).
        "value" => value: f64,
        /// The unit string (`mm`, `deg`, `""`, `Text`).
        "unit" => unit: String,
        "comment" => comment: String,
        "isFavorite" => is_favorite: bool,
        /// Parameters whose expression uses this one.
        "dependents" => dependents: Vec<String>,
        /// Model parameters: the role, e.g. `AlongDistance`.
        "role" => role: String,
        /// Model parameters: the feature or sketch dimension that owns it.
        "createdBy" => created_by: Reference,
        "component" => component: String,
    }
}

record! {
    /// A component (SCHEMA.md §5.2).
    pub struct Component {
        "name" => name: Option<String>,
        "id" => id: String,
        "partNumber" => part_number: String,
        "description" => description: String,
        "is_root" => is_root: bool,
        "bodies" => bodies: Vec<Value>,
        "sketches" => sketches: Vec<String>,
        "occurrence_count" => occurrence_count: i64,
        "feature_count" => feature_count: i64,
        "_f3d" => f3d: ComponentF3d,
    }
}

record! {
    /// Decoder data of a component.
    pub struct ComponentF3d {
        /// The component object (`E03784ED`); `_f3d.brep_blobs` and
        /// occurrences refer to it.
        "object_id" => object_id: u64,
    }
}

record! {
    /// An occurrence and its children (SCHEMA.md §5.2).
    pub struct OccurrenceNode {
        "name" => name: String,
        "fullPathName" => full_path_name: String,
        /// Component name (`null` if the decoder found no name).
        "component" => component: Option<String>,
        /// Decoder: top-level occurrences only (relative to the root).
        "transform" => transform: Mat4,
        "transform2" => transform2: Mat4,
        "initialTransform" => initial_transform: Mat4,
        "isGrounded" => is_grounded: bool,
        "isGroundToParent" => is_ground_to_parent: bool,
        "isVisible" => is_visible: bool,
        "isLightBulbOn" => is_light_bulb_on: bool,
        /// The component lives in another document.
        "isReferencedComponent" => is_referenced_component: bool,
        "entityToken" => entity_token: String,
        "children" => children: Vec<OccurrenceNode>,
        "_f3d" => f3d: OccurrenceF3d,
    }
}

record! {
    /// Decoder data of an occurrence.
    pub struct OccurrenceF3d {
        "object_id" => object_id: u64,
        "component_object" => component_object: u64,
        /// External document key (hex) of a linked component.
        "external_key" => external_key: String,
        /// URN of that document.
        "external_document" => external_document: String,
        /// The GUIDs (`str16`) in the occurrence object's data, lower case:
        /// occurrence paths of captured positions and relationships name
        /// it by one of them, also from a design that inserted this one
        /// (mitcad#196).
        "guids" => guids: Vec<String>,
        /// Transform relative to the parent component (nested occurrences;
        /// the full-path composition is not verified).
        "local_transform" => local_transform: Mat4,
        /// Where the items of the timeline put the occurrence (its path's
        /// placement in the root component after each, mitcad#81), in
        /// timeline order: the item that made it first.
        "placements" => placements: Vec<PlacementStep>,
    }
}

record! {
    /// An occurrence's placement after an item that placed it (mitcad#81).
    pub struct PlacementStep {
        /// The item's timeline index (absent: not a timeline item).
        "index" => index: i64,
        "object_id" => object_id: u64,
        /// The occurrence path's placement in the root component (cm).
        "transform" => transform: Mat4,
    }
}

record! {
    /// The timeline (SCHEMA.md §5.3).
    pub struct Timeline {
        "available" => available: bool,
        "count" => count: Option<i64>,
        "original_marker_position" => original_marker_position: Option<i64>,
        "restored_marker_position" => restored_marker_position: Option<i64>,
        "groups" => groups: Vec<Value>,
        "items" => items: Vec<TimelineItem>,
    }
}

record! {
    /// The group of a timeline item.
    pub struct GroupRef {
        "name" => name: String,
        "index" => index: i64,
    }
}

record! {
    @custom_de
    /// A timeline item. `detail` is typed by `objectType`.
    pub struct TimelineItem {
        "index" => index: i64,
        "group" => group: Option<GroupRef>,
        /// Display name (`null` if the decoder found none).
        "name" => name: Option<String>,
        /// `Sketch`, `ExtrudeFeature`, ... (absent: unknown type).
        "objectType" => object_type: Option<String>,
        "entityToken" => entity_token: String,
        "component" => component: Option<String>,
        "isRolledBack" => is_rolled_back: bool,
        "isSuppressed" => is_suppressed: bool,
        "healthState" => health_state: String,
        "errorOrWarningMessage" => error_or_warning_message: Option<String>,
        "detail_marker" => detail_marker: i64,
        "detail" => detail: Detail,
        /// Other entity properties. The decoder writes only
        /// `isLightBulbOn` of sketches and construction planes, where it
        /// decodes it (mitcad#6): the light bulb itself; `isVisible`
        /// (external dumps) is also off under a hidden occurrence.
        "props" => props: Value,
        "outputs" => outputs: Value,
        "_f3d" => f3d: ItemF3d,
    }
}

impl<'de> Deserialize<'de> for TimelineItem {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let mut item = TimelineItem::from_map(Map::deserialize(d)?);
        if let Some(Detail::Other(map)) = item.detail.take() {
            let t = item.object_type.clone().flatten();
            item.detail = Some(Detail::typed(t.as_deref(), map));
        }
        Ok(item)
    }
}

impl TimelineItem {
    /// The display name, if known.
    pub fn name(&self) -> Option<&str> {
        self.name.as_ref().and_then(|n| n.as_deref())
    }

    /// The object type, if known.
    pub fn object_type(&self) -> Option<&str> {
        self.object_type.as_ref().and_then(|n| n.as_deref())
    }

    /// A sketch's or construction feature's own light bulb, if known:
    /// `props.isLightBulbOn` (external dumps' and the decoder's), else a
    /// sketch detail's `isVisible` (external dumps').
    pub fn light_bulb(&self) -> Option<bool> {
        let props = self.props.as_ref().and_then(|p| p.get("isLightBulbOn"));
        let visible = match &self.detail {
            Some(Detail::Sketch(sketch)) => sketch.is_visible,
            _ => None,
        };
        props.and_then(Value::as_bool).or(visible)
    }

    /// The item's state in the file (mitcad#180): suppressed where
    /// `isSuppressed` says so (the stream decoder sets it from the state
    /// bytes, external dumps have their own), else what `_f3d`'s result
    /// number and state bytes say ([`item_state`]).
    pub fn file_state(&self) -> ItemState {
        if self.is_suppressed == Some(true) {
            return ItemState::Suppressed;
        }
        match self.result_and_flags() {
            Some((result_no, flags)) => item_state(result_no, flags),
            None => ItemState::Normal,
        }
    }

    /// How the item is suppressed in the file, when it is,
    /// [`Suppression`]): from `_f3d`'s state bytes. An
    /// item an external dump says is suppressed (`isSuppressed` without
    /// bytes) is suppressed directly.
    pub fn suppression(&self) -> Option<Suppression> {
        match self.result_and_flags() {
            Some((result_no, flags)) => item_suppression(result_no, flags)
                .or_else(|| (self.is_suppressed == Some(true)).then_some(Suppression::Direct)),
            None => (self.is_suppressed == Some(true)).then_some(Suppression::Direct),
        }
    }

    /// `_f3d`'s result number and three state bytes, when it has both.
    fn result_and_flags(&self) -> Option<(i32, [u8; 3])> {
        let f3d = self.f3d.as_ref()?;
        let flags = f3d.flags.as_deref().and_then(|f| {
            let bytes = (0..3)
                .map(|i| u8::from_str_radix(f.get(2 * i..2 * i + 2)?, 16).ok())
                .collect::<Option<Vec<u8>>>()?;
            (f.len() == 6).then(|| [bytes[0], bytes[1], bytes[2]])
        })?;
        Some((f3d.result_no?, flags))
    }
}

record! {
    /// Decoder data of a timeline item.
    pub struct ItemF3d {
        /// Class GUID (upper case).
        "class" => class: String,
        "class_version" => class_version: u32,
        "object_id" => object_id: u64,
        /// The component that owns the item (`components[]._f3d.object_id`):
        /// its feature, sketch or construction geometry (mitcad#37).
        "component" => component: u64,
        /// Default name base (`Extrude`); display name = base + index.
        "base_name" => base_name: String,
        "index" => index: u32,
        /// Name given by the user, empty if none.
        "custom_name" => custom_name: String,
        "result_no" => result_no: i32,
        /// The three state bytes (hex): with result -1, `010000` or
        /// `000100` suppressed, `000001` the change all in parts in other
        /// components (`decode::item_state`, mitcad#180).
        "flags" => flags: String,
        /// Two more state bytes (hex):
        /// `0000` on a healthy item, the first 1 suppressed, the
        /// second after the timeline marker.
        "after" => after: String,
        /// Sub-items (a hole's thread, say); `None` for null references.
        "sub_features" => sub_features: Vec<Option<u64>>,
        /// Raw extrude fields.
        "extrude" => extrude: ExtrudeF3d,
        /// The bytes of a sketch's or construction plane's light bulb
        /// (hex; mitcad#6): a sketch's `u32 k | u8 0 | u8 on`, a plane's
        /// `u32 0 | u8 on`. The item's `props.isLightBulbOn` gives it.
        "light_bulb" => light_bulb: String,
        /// A component made from bodies (mitcad#156): the result number
        /// of the base feature that put its bodies into the new component,
        /// that component's history operation for it (its own
        /// `result_no` is -1).
        "made_result_no" => made_result_no: i32,
        /// A part of a timeline item in another component (mitcad#169):
        /// the item's index. The part is a feature of that component that
        /// makes the item's change there; it follows the item, with an
        /// index after the timeline's items.
        "part_of" => part_of: i64,
        /// A construction plane's or point's stored definition code (the
        /// `u32` after its root part and seven bytes; mitcad#186).
        "datum_code" => datum_code: u32,
        /// The result number of the item's last operation where a sub-item
        /// makes one of its own after the item's (mitcad#719: a tapped
        /// hole's modelled thread, which cuts the thread into the bore of
        /// the hole's own operation).
        "last_result_no" => last_result_no: i32,
    }
}

record! {
    /// Raw fields of an extrude (timeline format study, 9.2).
    pub struct ExtrudeF3d {
        /// 1 Join, 2 Cut, 4 NewBody.
        "operation_code" => operation_code: u32,
        "operation" => operation: Option<String>,
        /// 1 OneSide, 2 TwoSides, 3 unknown.
        "extent_a" => extent_a: u32,
        "extent_b" => extent_b: u32,
        /// +1.0 / -1.0; `null` if not found.
        "direction" => direction: Option<f64>,
        /// The extrusion's direction in the component's coordinates (unit
        /// vector), where it is decoded.
        "direction_vector" => direction_vector: Vec3,
        /// The byte after the extent codes (mitcad#96): 1 on one-sided
        /// extrusions through all or up to an object that go against their
        /// sketch's normal.
        "flag" => flag: u32,
        /// The roles of the item's input slots in order (mitcad#96): 65
        /// profile, 8 participants, 5 a side through all, 17 and 18 a side
        /// up to an object.
        "slot_roles" => slot_roles: Vec<u32>,
        /// A symmetric extent's length (mitcad#96): the whole length or half
        /// of it each way.
        "full_length" => full_length: bool,
        /// A solid (`false`: a surface) extrusion (mitcad#154).
        "solid" => solid: bool,
        /// Where the profile starts (mitcad#154): 0 its plane, 1 a plane
        /// offset from it, 2 a selected face.
        "start" => start: u32,
        /// Each side's extent kind (mitcad#154): 0 none, 1 a distance, 2 up
        /// to an object, 3 up to the next face, 4 through all.
        "sides" => sides: Vec<u32>,
        /// A thin extrusion (mitcad#186): the stream's thin-wall flag, read
        /// on class version 8 where the block is consistent.
        "thin" => thin: bool,
        /// The thin wall's location: 0 side one, 1 centre, 2 side two.
        "thin_location" => thin_location: u32,
        /// The object id of the thin wall's thickness parameter holder.
        "thin_holder" => thin_holder: u64,
    }
}

/// `detail` of a timeline item, by its `objectType`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Detail {
    Sketch(Box<SketchDetail>),
    Extrude(Box<ExtrudeDetail>),
    Revolve(Box<RevolveDetail>),
    Sweep(Box<SweepDetail>),
    Pipe(Box<PipeDetail>),
    Fillet(Box<FilletDetail>),
    Chamfer(Box<ChamferDetail>),
    Hole(Box<HoleDetail>),
    Thread(Box<ThreadDetail>),
    CircularPattern(Box<CircularPatternDetail>),
    RectangularPattern(Box<RectangularPatternDetail>),
    Shell(Box<ShellDetail>),
    OffsetFaces(Box<OffsetFacesDetail>),
    Coil(Box<CoilDetail>),
    Primitive(Box<PrimitiveDetail>),
    ConstructionPlane(Box<ConstructionPlaneDetail>),
    JointOrigin(Box<JointOriginDetail>),
    ConstructionPoint(Box<ConstructionPointDetail>),
    ConstructionAxis(Box<ConstructionAxisDetail>),
    // Emboss (mitcad#458).
    Emboss(Box<EmbossDetail>),
    // Moved faces (mitcad#343).
    MoveFace(Box<MoveFaceDetail>),
    // Drafts (mitcad#1363).
    Draft(Box<DraftDetail>),
    /// Any other type (or an unknown one): the JSON object as is.
    Other(Map<String, Value>),
}

impl Detail {
    /// Types a detail object by the item's `objectType`.
    pub fn typed(object_type: Option<&str>, map: Map<String, Value>) -> Detail {
        match object_type {
            Some("Sketch") => Detail::Sketch(Box::new(SketchDetail::from_map(map))),
            Some("ExtrudeFeature") => Detail::Extrude(Box::new(ExtrudeDetail::from_map(map))),
            Some("RevolveFeature") => Detail::Revolve(Box::new(RevolveDetail::from_map(map))),
            Some("SweepFeature") => Detail::Sweep(Box::new(SweepDetail::from_map(map))),
            Some("PipeFeature") => Detail::Pipe(Box::new(PipeDetail::from_map(map))),
            Some("FilletFeature") => Detail::Fillet(Box::new(FilletDetail::from_map(map))),
            Some("ChamferFeature") => Detail::Chamfer(Box::new(ChamferDetail::from_map(map))),
            Some("HoleFeature") => Detail::Hole(Box::new(HoleDetail::from_map(map))),
            Some("ThreadFeature") => Detail::Thread(Box::new(ThreadDetail::from_map(map))),
            Some("CircularPatternFeature") => {
                Detail::CircularPattern(Box::new(CircularPatternDetail::from_map(map)))
            }
            Some("RectangularPatternFeature") => {
                Detail::RectangularPattern(Box::new(RectangularPatternDetail::from_map(map)))
            }
            Some("ShellFeature") => Detail::Shell(Box::new(ShellDetail::from_map(map))),
            Some("OffsetFacesFeature") => {
                Detail::OffsetFaces(Box::new(OffsetFacesDetail::from_map(map)))
            }
            Some("CoilFeature") => Detail::Coil(Box::new(CoilDetail::from_map(map))),
            Some("BoxFeature" | "CylinderFeature" | "SphereFeature" | "TorusFeature") => {
                Detail::Primitive(Box::new(PrimitiveDetail::from_map(map)))
            }
            Some("ConstructionPlane") => {
                Detail::ConstructionPlane(Box::new(ConstructionPlaneDetail::from_map(map)))
            }
            Some("JointOrigin") => Detail::JointOrigin(Box::new(JointOriginDetail::from_map(map))),
            Some("ConstructionPoint") => {
                Detail::ConstructionPoint(Box::new(ConstructionPointDetail::from_map(map)))
            }
            Some("ConstructionAxis") => {
                Detail::ConstructionAxis(Box::new(ConstructionAxisDetail::from_map(map)))
            }
            Some("EmbossFeature") => Detail::Emboss(Box::new(EmbossDetail::from_map(map))),
            Some("MoveFaceFeature") => Detail::MoveFace(Box::new(MoveFaceDetail::from_map(map))),
            Some("DraftFeature") => Detail::Draft(Box::new(DraftDetail::from_map(map))),
            _ => Detail::Other(map),
        }
    }

    /// An empty detail (`{}`).
    pub fn empty() -> Detail {
        Detail::Other(Map::new())
    }
}

/// Without the item's type, a detail stays [`Detail::Other`];
/// [`TimelineItem`] types it.
impl<'de> Deserialize<'de> for Detail {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Map::deserialize(d).map(Detail::Other)
    }
}

record! {
    /// Sketch detail (SCHEMA.md §5.3). Coordinates in sketch space, cm.
    pub struct SketchDetail {
        "name" => name: Option<String>,
        "component" => component: String,
        "isVisible" => is_visible: bool,
        "isParametric" => is_parametric: bool,
        "is3D" => is_3d: bool,
        "isFullyConstrained" => is_fully_constrained: bool,
        "areProfilesShown" => are_profiles_shown: bool,
        "areDimensionsShown" => are_dimensions_shown: bool,
        "areConstraintsShown" => are_constraints_shown: bool,
        "arePointsShown" => are_points_shown: bool,
        "isComputeDeferred" => is_compute_deferred: bool,
        /// The plane or face the sketch is on.
        "referencePlane" => reference_plane: Reference,
        /// The sketch's stored transform; its direction is not
        /// settled (SCHEMA.md §1). The decoder does not write it.
        "transform" => transform: Mat4,
        /// Model space.
        "origin" => origin: Vec3,
        "xDirection" => x_direction: Vec3,
        "yDirection" => y_direction: Vec3,
        /// Sketch space to model space: the mapping to use.
        "model_frame" => model_frame: ModelFrame,
        /// Point projected from the component origin.
        "originPoint" => origin_point: Option<String>,
        "counts" => counts: SketchCounts,
        "points" => points: Vec<SketchPoint>,
        "curves" => curves: Vec<SketchCurve>,
        "constraints" => constraints: Vec<SketchConstraint>,
        "dimensions" => dimensions: Vec<SketchDimension>,
        "texts" => texts: Vec<Value>,
        "profiles" => profiles: Vec<SketchProfile>,
    }
}

record! {
    /// A sketch's placement in model space (SCHEMA.md §5.3): the images of
    /// the sketch origin and unit axes; model = `sketch_to_model` · sketch.
    pub struct ModelFrame {
        "origin" => origin: Vec3,
        "x_axis" => x_axis: Vec3,
        "y_axis" => y_axis: Vec3,
        /// The sketch normal.
        "z_axis" => z_axis: Vec3,
        "sketch_to_model" => sketch_to_model: Mat4,
        /// External dumps: which reading of `transform` agrees.
        "transform_matches" => transform_matches: Vec<String>,
    }
}

record! {
    /// A profile of a sketch (external dumps only).
    pub struct SketchProfile {
        "index" => index: i64,
        "loops" => loops: Vec<ProfileLoop>,
        "area" => area: f64,
        "centroid" => centroid: Vec3,
        "perimeter" => perimeter: f64,
        "bbox" => bbox: Value,
        "isOnSketchPlane" => is_on_sketch_plane: bool,
        "plane" => plane: Geometry,
    }
}

record! {
    /// A loop of a profile: the sketch curves it lies on.
    pub struct ProfileLoop {
        "isOuter" => is_outer: bool,
        /// Sketch profile: `{curve, geometry}` items; profile reference:
        /// sketch-local curve ids (or null).
        "curves" => curves: Vec<Value>,
    }
}

record! {
    /// Entity counts of a sketch.
    pub struct SketchCounts {
        "points" => points: i64,
        "curves" => curves: i64,
        "texts" => texts: i64,
        "constraints" => constraints: i64,
        "dimensions" => dimensions: i64,
        "profiles" => profiles: i64,
    }
}

record! {
    /// A sketch point (`id` = `p<i>`).
    pub struct SketchPoint {
        "id" => id: String,
        "xyz" => xyz: Vec3,
        "isFixed" => is_fixed: bool,
        "isReference" => is_reference: bool,
        "isFullyConstrained" => is_fully_constrained: bool,
        /// Ids of the curves using the point.
        "connected" => connected: Option<Vec<String>>,
    }
}

record! {
    /// A sketch curve (`id` = `c<i>`).
    pub struct SketchCurve {
        "id" => id: String,
        /// `SketchLine`, `SketchArc`, `SketchCircle`, ...
        "type" => curve_type: String,
        "isConstruction" => is_construction: bool,
        "isFixed" => is_fixed: bool,
        "isReference" => is_reference: bool,
        "isCenterLine" => is_center_line: bool,
        "isLinked" => is_linked: bool,
        "isFullyConstrained" => is_fully_constrained: bool,
        "isVisible" => is_visible: bool,
        "startSketchPoint" => start_sketch_point: Option<String>,
        "endSketchPoint" => end_sketch_point: Option<String>,
        "centerSketchPoint" => center_sketch_point: Option<String>,
        "apexSketchPoint" => apex_sketch_point: Option<String>,
        "fitPoints" => fit_points: Vec<String>,
        "controlPoints" => control_points: Vec<String>,
        "radius" => radius: f64,
        "majorAxisRadius" => major_axis_radius: f64,
        "minorAxisRadius" => minor_axis_radius: f64,
        "rhoValue" => rho_value: f64,
        "degree" => degree: i64,
        "isClosed" => is_closed: bool,
        "length" => length: f64,
        "majorAxis" => major_axis: Vec3,
        /// Curve geometry in sketch space.
        "geometry" => geometry: Geometry,
        "referencedEntity" => referenced_entity: Reference,
        /// Decoder: class GUID of a curve whose type is not decoded.
        "_f3d_class" => f3d_class: String,
        /// Decoder: where a whole circle starts in the file (radians,
        /// counter-clockwise about the sketch normal from the sketch x
        /// axis; its seam), read from the closed edge the first feature
        /// that uses the circle made in the stored bodies. Absent when
        /// no stored body holds the circle.
        "_f3d_seam" => f3d_seam: f64,
    }
}

record! {
    /// Curve or surface geometry (SCHEMA.md §4); the keys the type has.
    pub struct Geometry {
        /// `Line3D`, `Arc3D`, `Circle3D`, `Plane`, `Cylinder`, ...
        "type" => geometry_type: String,
        "startPoint" => start_point: Vec3,
        "endPoint" => end_point: Vec3,
        "center" => center: Vec3,
        "origin" => origin: Vec3,
        "direction" => direction: Vec3,
        "normal" => normal: Vec3,
        "referenceVector" => reference_vector: Vec3,
        "majorAxis" => major_axis: Vec3,
        "uDirection" => u_direction: Vec3,
        "vDirection" => v_direction: Vec3,
        "axis" => axis: Vec3,
        "majorAxisDirection" => major_axis_direction: Vec3,
        "radius" => radius: f64,
        "majorRadius" => major_radius: f64,
        "minorRadius" => minor_radius: f64,
        "halfAngle" => half_angle: f64,
        "startAngle" => start_angle: f64,
        "endAngle" => end_angle: f64,
        "nurbs" => nurbs: Value,
    }
}

record! {
    /// A geometric constraint (`id` = `k<i>`).
    pub struct SketchConstraint {
        "id" => id: String,
        /// `CoincidentConstraint`, `HorizontalConstraint`, ...
        "type" => constraint_type: String,
        /// Entities by property name (decoder: `entities`).
        "refs" => refs: BTreeMap<String, RefValue>,
        "props" => props: Map<String, Value>,
        "_f3d" => f3d: ConstraintF3d,
    }
}

record! {
    /// Decoder data of a constraint.
    pub struct ConstraintF3d {
        /// Type bit mask (timeline format study, 10.3).
        "mask" => mask: u64,
    }
}

record! {
    /// A sketch dimension (`id` = `d<i>`).
    pub struct SketchDimension {
        "id" => id: String,
        /// `SketchLinearDimension`, `SketchRadialDimension`, ...
        "type" => dimension_type: String,
        /// `null` for some driven dimensions.
        "parameter" => parameter: Option<Reference>,
        "isDriving" => is_driving: bool,
        /// The dimension's value, internal units (external dumps).
        "value" => value: f64,
        "textPosition" => text_position: Vec3,
        "refs" => refs: BTreeMap<String, RefValue>,
        /// E.g. `orientation`.
        "props" => props: Map<String, Value>,
        "_f3d" => f3d: DimensionF3d,
    }
}

record! {
    /// Decoder data of a dimension.
    pub struct DimensionF3d {
        "class" => class: String,
        /// The flag bytes before the parameter holder (hex).
        "flags" => flags: Option<String>,
        /// A driven dimension's shown value, as stored (it has no
        /// parameter; mitcad#178).
        "expression" => expression: String,
        /// An ellipse's major or minor dimension whose value is the whole
        /// axis, not the radius (flag 4 or 8 of the last eight flag bytes,
        /// where the reference models' radius dimensions have 1 or 2).
        "axisLength" => axis_length: bool,
    }
}

/// An entry of a constraint's or dimension's `refs`: a sketch-local id, a
/// list, or a reference to something outside the sketch.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum RefValue {
    Id(String),
    List(Vec<RefValue>),
    Ref(Reference),
    Other(Value),
}

impl<'de> Deserialize<'de> for RefValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(RefValue::from_value(Value::deserialize(d)?))
    }
}

impl RefValue {
    pub fn from_value(v: Value) -> RefValue {
        match v {
            Value::String(s) => RefValue::Id(s),
            Value::Array(a) => RefValue::List(a.into_iter().map(RefValue::from_value).collect()),
            Value::Object(m) => RefValue::Ref(Reference::from_map(m)),
            v => RefValue::Other(v),
        }
    }

    /// The sketch-local id, if this is one.
    pub fn id(&self) -> Option<&str> {
        match self {
            RefValue::Id(s) => Some(s),
            _ => None,
        }
    }
}

/// A reference (SCHEMA.md §3), by `kind`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reference {
    Parameter(ParameterRef),
    Profile(ProfileRef),
    SketchEntity(SketchEntityRef),
    SketchDimension(SketchEntityRef),
    SketchConstraint(SketchEntityRef),
    Sketch(SketchRef),
    ConstructionPlane(Box<ConstructionRef>),
    ConstructionAxis(Box<ConstructionRef>),
    ConstructionPoint(Box<ConstructionRef>),
    Component(NamedRef),
    Occurrence(OccurrenceRef),
    Feature(FeatureRef),
    Face(Box<Fingerprint>),
    Edge(Box<Fingerprint>),
    Vertex(Box<Fingerprint>),
    Body(Box<Fingerprint>),
    Loop(Box<Fingerprint>),
    Coedge(Box<Fingerprint>),
    Lump(Box<Fingerprint>),
    Shell(Box<Fingerprint>),
    Document(NamedRef),
    Appearance(NamedRef),
    Material(NamedRef),
    /// The reference itself failed.
    Error(ErrorRef),
    /// Another kind, or no kind (e.g. `{"_error": ...}`): as is.
    #[serde(untagged)]
    Other(Map<String, Value>),
}

impl<'de> Deserialize<'de> for Reference {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Map::deserialize(d).map(Reference::from_map)
    }
}

impl Reference {
    pub fn from_map(mut map: Map<String, Value>) -> Reference {
        let Some(Value::String(kind)) = map.get("kind") else {
            return Reference::Other(map);
        };
        let kind = kind.clone();
        let mut take = || {
            let mut m = std::mem::take(&mut map);
            m.remove("kind");
            m
        };
        match kind.as_str() {
            "parameter" => Reference::Parameter(ParameterRef::from_map(take())),
            "profile" => Reference::Profile(ProfileRef::from_map(take())),
            "sketch_entity" => Reference::SketchEntity(SketchEntityRef::from_map(take())),
            "sketch_dimension" => Reference::SketchDimension(SketchEntityRef::from_map(take())),
            "sketch_constraint" => Reference::SketchConstraint(SketchEntityRef::from_map(take())),
            "sketch" => Reference::Sketch(SketchRef::from_map(take())),
            "construction_plane" => {
                Reference::ConstructionPlane(Box::new(ConstructionRef::from_map(take())))
            }
            "construction_axis" => {
                Reference::ConstructionAxis(Box::new(ConstructionRef::from_map(take())))
            }
            "construction_point" => {
                Reference::ConstructionPoint(Box::new(ConstructionRef::from_map(take())))
            }
            "component" => Reference::Component(NamedRef::from_map(take())),
            "occurrence" => Reference::Occurrence(OccurrenceRef::from_map(take())),
            "feature" => Reference::Feature(FeatureRef::from_map(take())),
            "face" => Reference::Face(Box::new(Fingerprint::from_map(take()))),
            "edge" => Reference::Edge(Box::new(Fingerprint::from_map(take()))),
            "vertex" => Reference::Vertex(Box::new(Fingerprint::from_map(take()))),
            "body" => Reference::Body(Box::new(Fingerprint::from_map(take()))),
            "loop" => Reference::Loop(Box::new(Fingerprint::from_map(take()))),
            "coedge" => Reference::Coedge(Box::new(Fingerprint::from_map(take()))),
            "lump" => Reference::Lump(Box::new(Fingerprint::from_map(take()))),
            "shell" => Reference::Shell(Box::new(Fingerprint::from_map(take()))),
            "document" => Reference::Document(NamedRef::from_map(take())),
            "appearance" => Reference::Appearance(NamedRef::from_map(take())),
            "material" => Reference::Material(NamedRef::from_map(take())),
            "error" => Reference::Error(ErrorRef::from_map(take())),
            _ => Reference::Other(map),
        }
    }

    /// The parameter, if this references one.
    pub fn parameter(&self) -> Option<&ParameterRef> {
        match self {
            Reference::Parameter(p) => Some(p),
            _ => None,
        }
    }
}

record! {
    /// `kind: parameter`.
    pub struct ParameterRef {
        "name" => name: String,
        "expression" => expression: String,
        /// Internal units (cm, rad).
        "value" => value: f64,
        "unit" => unit: String,
    }
}

record! {
    /// `kind: profile`.
    pub struct ProfileRef {
        /// Sketch name (`null` if not found).
        "sketch" => sketch: Option<String>,
        "sketch_timeline_index" => sketch_timeline_index: Option<i64>,
        /// Position in `sketch.profiles` (not decoded from streams).
        "profile_index" => profile_index: Option<i64>,
        /// The profile's loops with the sketch curve of each profile curve.
        "loops" => loops: Vec<ProfileLoop>,
        "area" => area: f64,
        "centroid" => centroid: Vec3,
    }
}

record! {
    /// `kind: sketch_entity`, `sketch_dimension`, `sketch_constraint`.
    pub struct SketchEntityRef {
        "objectType" => object_type: String,
        "sketch" => sketch: Option<String>,
        "sketch_timeline_index" => sketch_timeline_index: Option<i64>,
        /// Sketch-local id (`p0`, `c3`, `d1`, ...).
        "id" => id: Option<String>,
        "geometry" => geometry: Value,
    }
}

record! {
    /// `kind: sketch`.
    pub struct SketchRef {
        "name" => name: Option<String>,
        "timeline_index" => timeline_index: Option<i64>,
        "component" => component: Option<String>,
    }
}

record! {
    /// `kind: construction_plane`, `construction_axis`, `construction_point`.
    pub struct ConstructionRef {
        "name" => name: Option<String>,
        /// `XY`, `XZ`, `YZ`, `X`, `Y`, `Z`, `Origin` for origin geometry.
        "origin" => origin: Option<String>,
        /// `null` for origin geometry.
        "timeline_index" => timeline_index: Option<i64>,
        "component" => component: Option<String>,
        "geometry" => geometry: Geometry,
    }
}

record! {
    /// `kind: component`, `document`, `appearance`, `material`.
    pub struct NamedRef {
        "name" => name: Option<String>,
        "id" => id: Option<String>,
    }
}

record! {
    /// `kind: occurrence`.
    pub struct OccurrenceRef {
        "name" => name: Option<String>,
        "fullPathName" => full_path_name: Option<String>,
        "component" => component: Option<String>,
    }
}

record! {
    /// `kind: feature` (any timeline entity).
    pub struct FeatureRef {
        "objectType" => object_type: String,
        "name" => name: Option<String>,
        "timeline_index" => timeline_index: Option<i64>,
    }
}

record! {
    /// `kind: error`.
    pub struct ErrorRef {
        "objectType" => object_type: Option<String>,
        "_error" => error: String,
    }
}

record! {
    /// B-rep fingerprint (SCHEMA.md §4): `face`, `edge`, `vertex`, `body`, ...
    pub struct Fingerprint {
        "objectType" => object_type: String,
        "entityToken" => entity_token: String,
        "tempId" => temp_id: i64,
        "body" => body: Option<String>,
        "component" => component: Option<String>,
        "occurrence" => occurrence: Option<String>,
        "geometry" => geometry: Geometry,
        "name" => name: String,
        "area" => area: f64,
        "length" => length: f64,
        "volume" => volume: f64,
        "point" => point: Vec3,
        "point_on_face" => point_on_face: Vec3,
        "normal_at_point" => normal_at_point: Vec3,
        "start_point" => start_point: Option<Vec3>,
        "end_point" => end_point: Option<Vec3>,
        "mid_point" => mid_point: Vec3,
        "isSolid" => is_solid: bool,
        "isOuter" => is_outer: bool,
        "isParamReversed" => is_param_reversed: bool,
        "isDegenerate" => is_degenerate: bool,
        "bbox" => bbox: Value,
        "edge_count" => edge_count: i64,
        "face_count" => face_count: i64,
        "loop_count" => loop_count: i64,
        "vertex_count" => vertex_count: i64,
        /// Topology (SCHEMA.md §4): a face's loops.
        "loops" => loops: Vec<Value>,
        /// Topology: an edge's or loop's coedges.
        "coedges" => coedges: Vec<Value>,
        /// Stream decoder: the entity's name in the file.
        "_f3d" => f3d: FingerprintF3d,
    }
}

/// One name the file gives a B-rep entity: a tag the operation that made it
/// chose, and the operations that made and changed it (their ASM state
/// numbers; a negative number is stored as it is, its meaning is open).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityName {
    pub tag: String,
    /// A number stored with the tag (0 on faces; meaning open).
    pub kind: i64,
    pub ops: Vec<i64>,
}

record! {
    /// Stream decoder data of a B-rep reference (a feature input): the
    /// entity named as the file names it (a recipe of names), and what the
    /// ASM history gave for it.
    pub struct FingerprintF3d {
        /// The reference object (`5662F619`, `9716F783`).
        "object_id" => object_id: u64,
        /// The recipe's type: `edge`, `face`, `bounded_face`, `body`,
        /// `vertex`.
        "recipe" => recipe: String,
        /// The entities of the recipe, each by its names: an edge's two
        /// faces, then the faces at its ends; a face, then its neighbours
        /// (`bounded_face`); a body.
        "entities" => entities: Vec<Vec<EntityName>>,
        /// How the entity was found in the ASM history (`state N`), or why
        /// it was not.
        "found" => found: String,
        /// A body found: the middle points of some of its edges (cm), by
        /// which the import finds it among its own bodies.
        "edge_points" => edge_points: Vec<Vec3>,
        /// An edge found: the unit tangent at its middle point along its
        /// own direction in the file (the face whose coedge runs that way
        /// lies on its left; mitcad#96).
        "direction" => direction: Vec3,
        /// A body: the timeline index of the item that made it (its body
        /// record's producer, mitcad#96), by which the import finds it
        /// among the bodies that item made. A fillet's or chamfer's edge
        /// gives the producer of the body it is on the same way.
        "producer" => producer: i64,
        /// A body: its index among the bodies its producer made (also
        /// for the edge of a fillet or chamfer: its body's).
        "body_index" => body_index: i64,
        /// An edge or face found in another component's history than
        /// the item's (mitcad#161): that component (`components[]._f3d.
        /// object_id`), in whose coordinates its geometry is.
        "component" => component: u64,
        /// A free-form face found (mitcad#161): more points inside it,
        /// spread over it (cm), by which the import finds every piece of
        /// it where the replay splits the face.
        "face_points" => face_points: Vec<Vec3>,
        /// A cylindrical face found that goes all the way round (a thread's):
        /// the unit direction of its surface's parameter origin, its seam,
        /// where a modelled thread's helix starts.
        "seam" => seam: Vec3,
        /// An edge: the entities (indices into `entities`) of the faces
        /// round each of its end vertices, the start first, in cyclic
        /// order as the recipe lists them (mitcad#106: they tell mirror-image
        /// edges between the same two faces apart).
        "end_faces" => end_faces: Vec<Vec<i64>>,
        /// The GUID of the document that owns the entity (lower case,
        /// `mitcad_f3d::design::recipe::owner_guid`): that of one of `_f3d.
        /// external_documents` where the entity is in a component of
        /// another document, whose own history names it (mitcad#196).
        "document" => document: String,
        /// A body found in another document's history: its name there, the
        /// blob and the ASM record of the body in the last state
        /// (`BREP.<guid>.smbh#<record>`), as that document's import names its
        /// stored bodies (mitcad#196).
        "stored" => stored: String,
        /// An entity of another component (or document) that a plane or an
        /// extent names: the transform of its component's occurrence in the
        /// design when it was selected (cm), as the slot that holds the
        /// entity carries it (`decode::slot_contexts`).
        "context" => context: Mat4,
        /// An edge input: the curve of the edge as the input saw it, which
        /// the input stores beside its recipe (mitcad#306,
        /// `mitcad_f3d::design::curve`): `Line3D`, `Arc3D` or `NurbsCurve3D` (cm).
        "curves" => curves: Vec<Geometry>,
        /// An edge found: how it lies to its stored curve
        /// (`mitcad_f3d::design::curve::CurveFit::describe`).
        "curve_fit" => curve_fit: String,
    }
}

record! {
    /// An extent, start or plane definition (`_type` names its type).
    pub struct Definition {
        /// `DistanceExtentDefinition`, `ToEntityExtentDefinition`,
        /// `AngleExtentDefinition`, `OffsetStartDefinition`,
        /// `ConstructionPlaneOffsetDefinition`, ...
        "_type" => definition_type: String,
        "distance" => distance: Reference,
        "offset" => offset: Reference,
        "angle" => angle: Reference,
        "isSymmetric" => is_symmetric: bool,
        "isPositiveDirection" => is_positive_direction: bool,
        "entity" => entity: Reference,
        "planarEntity" => planar_entity: Reference,
        "isChained" => is_chained: bool,
        "isMinimumSolution" => is_minimum_solution: bool,
        "directionHint" => direction_hint: Vec3,
        "isFullLength" => is_full_length: bool,
        "taperAngle" => taper_angle: Reference,
        "angleOne" => angle_one: Reference,
        "angleTwo" => angle_two: Reference,
        /// A start definition's plane (`ProfilePlaneStartDefinition`): the
        /// profile's plane.
        "profilePlane" => profile_plane: Geometry,
    }
}

record! {
    /// A fillet or chamfer edge set.
    pub struct EdgeSet {
        "_type" => edge_set_type: String,
        "radius" => radius: Reference,
        "distance" => distance: Reference,
        "distanceOne" => distance_one: Reference,
        "distanceTwo" => distance_two: Reference,
        "angle" => angle: Reference,
        "edges" => edges: Vec<Reference>,
        "isTangentChain" => is_tangent_chain: bool,
        "isFlipped" => is_flipped: bool,
        "continuity" => continuity: String,
        "tangencyWeight" => tangency_weight: Reference,
        "startRadius" => start_radius: Reference,
        "endRadius" => end_radius: Reference,
        "midRadii" => mid_radii: Vec<Reference>,
        "midPositions" => mid_positions: Vec<Reference>,
        /// Asymmetric fillet sets (11/2025): the two distances from the edge.
        "offsetOne" => offset_one: Reference,
        "offsetTwo" => offset_two: Reference,
    }
}

record! {
    /// `ExtrudeFeature` detail.
    pub struct ExtrudeDetail {
        /// `JoinFeatureOperation`, `CutFeatureOperation`,
        /// `NewBodyFeatureOperation`, ...
        "operation" => operation: String,
        /// Selected profiles (the decoder gives the sketch only).
        "profile" => profile: Vec<Reference>,
        /// `OneSideFeatureExtentType`, `TwoSidesFeatureExtentType`, ...
        "extentType" => extent_type: String,
        "extentOne" => extent_one: Definition,
        "extentTwo" => extent_two: Definition,
        "hasTwoExtents" => has_two_extents: bool,
        "symmetricExtent" => symmetric_extent: Definition,
        "startExtent" => start_extent: Definition,
        "taperAngleOne" => taper_angle_one: Reference,
        "taperAngleTwo" => taper_angle_two: Reference,
        "participantBodies" => participant_bodies: Vec<Reference>,
        "isSolid" => is_solid: bool,
        "isThinExtrude" => is_thin_extrude: bool,
        "thinExtrudeWallLocationOne" => thin_extrude_wall_location_one: String,
        "thinExtrudeWallLocationTwo" => thin_extrude_wall_location_two: String,
        "thinExtrudeWallThicknessOne" => thin_extrude_wall_thickness_one: Reference,
        "thinExtrudeWallThicknessTwo" => thin_extrude_wall_thickness_two: Reference,
    }
}

record! {
    /// `RevolveFeature` detail.
    pub struct RevolveDetail {
        "operation" => operation: String,
        "profile" => profile: Vec<Reference>,
        "axis" => axis: Reference,
        /// The extent (schema version 2; the decoder writes the angle).
        "extentDefinition" => extent_definition: Definition,
        /// Schema version 1 only.
        "extentType" => extent_type: String,
        /// Schema version 1 only (the angle extent).
        "extentOne" => extent_one: Definition,
        /// Schema version 1 only.
        "extentTwo" => extent_two: Definition,
        "isSolid" => is_solid: bool,
        "participantBodies" => participant_bodies: Vec<Reference>,
        "isProjectAxis" => is_project_axis: bool,
    }
}

record! {
    /// `SweepFeature` detail.
    pub struct SweepDetail {
        "profile" => profile: Vec<Reference>,
        "path" => path: Value,
        "guideRail" => guide_rail: Value,
        "operation" => operation: String,
        "orientation" => orientation: String,
        "distanceOne" => distance_one: Reference,
        "distanceTwo" => distance_two: Reference,
        "taperAngle" => taper_angle: Reference,
        "twistAngle" => twist_angle: Reference,
        "isSolid" => is_solid: bool,
        "profileScaling" => profile_scaling: String,
        "participantBodies" => participant_bodies: Vec<Reference>,
    }
}

record! {
    /// `PipeFeature` detail.
    pub struct PipeDetail {
        "path" => path: Value,
        "sectionType" => section_type: String,
        "sectionSize" => section_size: Reference,
        "sectionThickness" => section_thickness: Reference,
        "isHollow" => is_hollow: bool,
        "operation" => operation: String,
        "distanceOne" => distance_one: Reference,
        "distanceTwo" => distance_two: Reference,
        "participantBodies" => participant_bodies: Vec<Reference>,
    }
}

record! {
    /// `FilletFeature` detail (decoder: one edge set per radius parameter,
    /// edges not decoded).
    pub struct FilletDetail {
        "filletFeatureType" => fillet_feature_type: String,
        "edgeSets" => edge_sets: Vec<EdgeSet>,
        "ruleFilletSettings" => rule_fillet_settings: Value,
        "fullRoundFilletFaceSets" => full_round_fillet_face_sets: Value,
        "isRollingBallCorner" => is_rolling_ball_corner: bool,
        "isG2" => is_g2: bool,
        "isTangentChain" => is_tangent_chain: bool,
        "cornerType" => corner_type: String,
    }
}

record! {
    /// `ChamferFeature` detail.
    pub struct ChamferDetail {
        "edgeSets" => edge_sets: Vec<EdgeSet>,
        "edges" => edges: Vec<Reference>,
        /// `EqualDistanceChamferType`, `TwoDistancesChamferType`,
        /// `DistanceAndAngleChamferType`.
        "chamferType" => chamfer_type: String,
        "chamferTypeDefinition" => chamfer_type_definition: Value,
        "isTangentChain" => is_tangent_chain: bool,
        "isFlipped" => is_flipped: bool,
        "cornerType" => corner_type: String,
    }
}

record! {
    /// `HoleFeature` detail.
    pub struct HoleDetail {
        "holeType" => hole_type: String,
        "holeTapType" => hole_tap_type: String,
        "holeDiameter" => hole_diameter: Reference,
        "tipAngle" => tip_angle: Reference,
        "counterboreDiameter" => counterbore_diameter: Reference,
        "counterboreDepth" => counterbore_depth: Reference,
        "countersinkDiameter" => countersink_diameter: Reference,
        "countersinkAngle" => countersink_angle: Reference,
        "isDefaultDirection" => is_default_direction: bool,
        "extentDefinition" => extent_definition: Definition,
        "position" => position: Vec3,
        "direction" => direction: Vec3,
        "holePositionDefinition" => hole_position_definition: HolePositionDefinition,
        "participantBodies" => participant_bodies: Vec<Reference>,
        "tappedHoleInfo" => tapped_hole_info: Value,
        "clearanceHoleInfo" => clearance_hole_info: Value,
        /// The hole's thread feature (reflected).
        "thread" => thread: Value,
        /// Schema version 1 only.
        "isTapped" => is_tapped: bool,
        /// Schema version 1 only.
        "threadInfo" => thread_info: Value,
    }
}

record! {
    /// A hole's position definition (`PlaneAndOffsetsHolePositionDefinition`:
    /// a face and the distances to two edges; others keep their keys in
    /// `other`).
    pub struct HolePositionDefinition {
        "_type" => definition_type: String,
        "planarEntity" => planar_entity: Reference,
        "edgeOne" => edge_one: Reference,
        "edgeTwo" => edge_two: Reference,
        "offsetOne" => offset_one: Reference,
        "offsetTwo" => offset_two: Reference,
    }
}

record! {
    /// `ThreadFeature` detail.
    pub struct ThreadDetail {
        "threadInfo" => thread_info: Value,
        "isModeled" => is_modeled: bool,
        "isFullLength" => is_full_length: bool,
        "threadLength" => thread_length: Reference,
        "threadOffset" => thread_offset: Reference,
        "threadLocation" => thread_location: String,
        "isRightHanded" => is_right_handed: bool,
        "hole" => hole: Reference,
        "inputCylindricalFace" => input_cylindrical_face: Reference,
        "inputCylindricalFaces" => input_cylindrical_faces: Vec<Reference>,
    }
}

record! {
    /// `CircularPatternFeature` detail.
    pub struct CircularPatternDetail {
        "inputEntities" => input_entities: Vec<Reference>,
        "patternEntityType" => pattern_entity_type: String,
        "axis" => axis: Reference,
        "quantity" => quantity: Reference,
        "totalAngle" => total_angle: Reference,
        "isSymmetric" => is_symmetric: bool,
        "patternComputeOption" => pattern_compute_option: String,
        "suppressedElementsIds" => suppressed_elements_ids: Vec<i64>,
    }
}

record! {
    /// `RectangularPatternFeature` detail.
    pub struct RectangularPatternDetail {
        "inputEntities" => input_entities: Vec<Reference>,
        "patternEntityType" => pattern_entity_type: String,
        "directionOneEntity" => direction_one_entity: Reference,
        "directionTwoEntity" => direction_two_entity: Reference,
        "directionOne" => direction_one: Vec3,
        "directionTwo" => direction_two: Vec3,
        "quantityOne" => quantity_one: Reference,
        "quantityTwo" => quantity_two: Reference,
        "distanceOne" => distance_one: Reference,
        "distanceTwo" => distance_two: Reference,
        "patternDistanceType" => pattern_distance_type: String,
        "isSymmetricInDirectionOne" => is_symmetric_in_direction_one: bool,
        "isSymmetricInDirectionTwo" => is_symmetric_in_direction_two: bool,
        "patternComputeOption" => pattern_compute_option: String,
        "suppressedElementsIds" => suppressed_elements_ids: Vec<i64>,
    }
}

record! {
    /// `ShellFeature` detail.
    pub struct ShellDetail {
        "inputEntities" => input_entities: Vec<Reference>,
        "insideThickness" => inside_thickness: Reference,
        "outsideThickness" => outside_thickness: Reference,
        "isTangentChain" => is_tangent_chain: bool,
        "shellType" => shell_type: String,
    }
}

record! {
    /// `OffsetFacesFeature` detail.
    pub struct OffsetFacesDetail {
        "inputFaces" => input_faces: Vec<Reference>,
        "distance" => distance: Reference,
        /// Schema version 1 only (an output, renamed `inputFaces`).
        "faces" => faces: Vec<Reference>,
    }
}

record! {
    /// `CoilFeature` detail.
    pub struct CoilDetail {
        "coilType" => coil_type: String,
        "diameter" => diameter: Reference,
        "pitch" => pitch: Reference,
        "revolutions" => revolutions: Reference,
        "height" => height: Reference,
        "angle" => angle: Reference,
        "sectionType" => section_type: String,
        "sectionPosition" => section_position: String,
        "sectionSize" => section_size: Reference,
        "isClockwise" => is_clockwise: bool,
        "operation" => operation: String,
        /// The plane or face the coil stands on (mitcad#25).
        "plane" => plane: Reference,
        /// The frame input's matrix (the plane's frame); absent: the
        /// identity (mitcad#25).
        "frame" => frame: Mat4,
        /// The coil's own placement in that frame; absent: the identity
        /// (mitcad#25).
        "placement" => placement: Mat4,
        "participantBodies" => participant_bodies: Vec<Reference>,
    }
}

record! {
    /// A box, cylinder, sphere or torus primitive (mitcad#25), the
    /// `objectType` `BoxFeature`, `CylinderFeature`, `SphereFeature` or
    /// `TorusFeature`. The file stores the placement as a matrix in the
    /// frame of the plane or face the primitive stands on: a point `p` of
    /// the primitive's own frame is at `frame * placement * p` in the
    /// component's coordinates (the frame carries the plane's origin). The
    /// own frame has the box's
    /// width along x, its length along y and its height along z; a
    /// cylinder's axis, a sphere's centre and a torus' axis and centre
    /// are its origin and z axis.
    pub struct PrimitiveDetail {
        "operation" => operation: String,
        /// The plane or face the primitive stands on.
        "plane" => plane: Reference,
        /// The frame input's matrix (cm); absent: the identity.
        "frame" => frame: Mat4,
        /// The primitive's placement in that frame (cm); absent: the
        /// identity.
        "placement" => placement: Mat4,
        "participantBodies" => participant_bodies: Vec<Reference>,
        /// A box's size along its own x axis (its width).
        "distanceX" => distance_x: Reference,
        /// A box's size along its own y axis (its length).
        "distanceY" => distance_y: Reference,
        /// A box's height.
        "distanceZ" => distance_z: Reference,
        "offsetDistanceX" => offset_distance_x: Reference,
        "offsetDistanceY" => offset_distance_y: Reference,
        /// A cylinder's height (signed).
        "height" => height: Reference,
        /// A cylinder's or sphere's diameter.
        "diameter" => diameter: Reference,
        /// A torus' diameter (of its centre line, or its inner or outer
        /// side: `torusPosition`).
        "majorDiameter" => major_diameter: Reference,
        /// A torus' section diameter.
        "minorDiameter" => minor_diameter: Reference,
        /// `CenterTorusPosition`, `InsideTorusPosition` or
        /// `OutsideTorusPosition`.
        "torusPosition" => torus_position: String,
    }
}

record! {
    /// `ConstructionPlane` detail.
    pub struct ConstructionPlaneDetail {
        "definition" => definition: Definition,
        "geometry" => geometry: Geometry,
        "transform" => transform: Mat4,
        "isParametric" => is_parametric: bool,
    }
}

record! {
    /// `ConstructionPoint` detail: what the point is made from
    /// (`ConstructionPointPointDefinition`: `pointEntity`, a vertex) and
    /// where it is (cm).
    pub struct ConstructionPointDetail {
        "definition" => definition: Definition,
        "geometry" => geometry: Vec3,
        "isParametric" => is_parametric: bool,
    }
}

record! {
    /// `ConstructionAxis` detail: what the axis is made from and its line.
    pub struct ConstructionAxisDetail {
        "definition" => definition: Definition,
        "geometry" => geometry: Geometry,
        "isParametric" => is_parametric: bool,
    }
}

record! {
    /// `JointOrigin` detail.
    pub struct JointOriginDetail {
        "geometry" => geometry: Value,
        "offsetX" => offset_x: Reference,
        "offsetY" => offset_y: Reference,
        "offsetZ" => offset_z: Reference,
        "angle" => angle: Reference,
        "isFlipped" => is_flipped: bool,
        "xAxisEntity" => x_axis_entity: Reference,
        "zAxisEntity" => z_axis_entity: Reference,
    }
}

record! {
    /// Decoder data of the whole design.
    pub struct DumpF3d {
        "format" => format: Format,
        "next_object_id" => next_object_id: Option<u64>,
        /// Documents the design links to (external components).
        "external_documents" => external_documents: Vec<ExternalDocument>,
        /// Form B MetaStream tail: (`Application`, n), (`Server`, n).
        "feature_set_versions" => feature_set_versions: Vec<(String, u32)>,
        /// Objects naming a body blob `BREP.<guid>.smb`.
        "brep_blobs" => brep_blobs: Vec<BrepBlob>,
        "coverage" => coverage: Coverage,
        "timelines_total" => timelines_total: u64,
        "parameter_failures" => parameter_failures: u64,
        /// Paths of values that were not finite (written as `null`).
        "non_finite" => non_finite: Vec<String>,
        /// The appearances and physical materials of the bodies, faces and
        /// occurrences (mitcad#228).
        "appearance" => appearance: AppearanceData,
        /// The mesh bodies and their containers (mitcad#1366); absent
        /// without any.
        "mesh_bodies" => mesh_bodies: Vec<MeshBody>,
        /// The control cages of the document's Form bodies (mitcad#1365,
        /// [`cage`]).
        "cages" => cages: Vec<cage::FormCage>,
    }
}

record! {
    /// Stream format of the design segment.
    pub struct Format {
        "meta_magic" => meta_magic: u32,
        "meta_version" => meta_version: Vec<u32>,
        "writer_build" => writer_build: Option<u32>,
        /// Version of the application that wrote the file (`2.0.20476`,
        /// `2704.1.36`).
        "writer" => writer: String,
        "long_refs" => long_refs: bool,
        "classes" => classes: u64,
        "objects" => objects: u64,
        "bulk_bytes" => bulk_bytes: u64,
    }
}

record! {
    /// An external document.
    pub struct ExternalDocument {
        /// Key (16 hex digits) that references to it carry.
        "key" => key: String,
        "urn" => urn: String,
        /// The version the URN names (`?version=N`).
        "version" => version: u32,
        /// The two GUIDs before the URN (occurrences of its components
        /// carry the first, mitcad#196).
        "guids" => guids: Vec<String>,
        /// The GUID after the URN: one of the GUIDs the referenced file's
        /// `Manifest.dat` lists, which tells the file (mitcad#196).
        "document_guid" => document_guid: String,
    }
}

record! {
    /// An object that names a body blob.
    pub struct BrepBlob {
        "id" => id: u64,
        /// `BREP.<guid>.smb`.
        "file" => file: String,
        /// The blob holder object (`CD57BC48`).
        "holder" => holder: u64,
        /// The component whose bodies the blob holds (its object id, see
        /// `components[]._f3d.object_id`).
        "component" => component: Option<u64>,
    }
}

record! {
    /// A mesh body of the design (mitcad#1366): its container, the
    /// timeline item that made it, its component and its placement.
    pub struct MeshBody {
        /// The mesh body record.
        "id" => id: u64,
        /// The container's entry name `ParaMeshGeometry.<uuid>.paramesh`.
        "file" => file: String,
        /// The GUID the container's registry repeats (lower case).
        "key" => key: String,
        /// The timeline item that made it (`timeline.items[]._f3d.object_id`).
        "feature" => feature: u64,
        /// The component that holds it (`components[]._f3d.object_id`).
        "component" => component: u64,
        "name" => name: String,
        /// From the container's coordinates to the design's centimetres,
        /// row-major; absent for the identity.
        "transform" => transform: [[f64; 4]; 4],
        /// Why its placement is not read.
        "error" => error: String,
    }
}

record! {
    /// BulkStream bytes explained by the decoder (timeline format study, 14).
    pub struct Coverage {
        "bulk_bytes" => bulk_bytes: u64,
        /// Object headers and root parts.
        "framing_bytes" => framing_bytes: u64,
        "root_part_ok" => root_part_ok: u64,
        "objects" => objects: u64,
        /// Objects of known classes.
        "known_class_bytes" => known_class_bytes: u64,
        /// Fully decoded fields (parameters, timelines, item tails).
        "decoded_bytes" => decoded_bytes: u64,
        /// Framing plus validated references and plausible strings.
        "token_bytes" => token_bytes: u64,
    }
}

// Appearances and physical materials (mitcad#228, `mitcad_f3d::design::appearance`).

record! {
    /// The appearances and physical materials the design gives its bodies,
    /// faces and occurrences, with the assets they name.
    pub struct AppearanceData {
        /// The assets the bodies, faces and occurrences name, their
        /// textures and their materials' aspects.
        "assets" => assets: Vec<AssetDef>,
        "bodies" => bodies: Vec<BodyLook>,
        /// Face appearances kept as face references (class `BFB592FE`)
        /// rather than in their body's record; `mitcad_f3d::design::appearance::resolve`
        /// moves those it finds to their bodies' `faces`.
        "faces" => faces: Vec<FaceLook>,
        /// Occurrences whose appearance overrides their bodies'.
        "occurrences" => occurrences: Vec<OccurrenceLook>,
        /// What did not decode or join.
        "warnings" => warnings: Vec<String>,
        /// The mesh bodies' physical materials and appearances, from their
        /// presentation envelopes (mitcad#1376): `entity` is the mesh body
        /// record (`_f3d.mesh_bodies[].id`); absent without any.
        "meshes" => meshes: Vec<BodyLook>,
    }
}

record! {
    /// An asset of the document's packages (`mitcad_f3d::protein`).
    pub struct AssetDef {
        /// The asset's GUID.
        "id" => id: String,
        /// `PrismOpaqueSchema`, `PhysMatSchema`, `UnifiedBitmapSchema`, ...
        "schema" => schema: String,
        /// The asset's name: a library preset (`Prism-093`) or its own id.
        "name" => name: String,
        "library" => library: String,
        /// From the package's descriptions.
        "ui_name" => ui_name: String,
        "category" => category: String,
        "description" => description: String,
        /// Every property: booleans, numbers, strings, colours (RGBA
        /// arrays), quantities (`{"unit": tag, "value": v}`), texture
        /// paths (`{"kind", "paths"}`), references (null), lists.
        "properties" => properties: BTreeMap<String, Value>,
        /// The assets connected to properties (texture GUIDs).
        "connections" => connections: BTreeMap<String, Vec<String>>,
    }
}

record! {
    /// A body's appearance and material.
    pub struct BodyLook {
        /// The body's design entity (its record's object id).
        "entity" => entity: u64,
        "name" => name: String,
        /// The blob that holds the body (`BREP.<guid>.smbh`) and the body's
        /// selector in it (the body map's).
        "blob" => blob: String,
        "selector" => selector: u64,
        /// The body's ASM record in the blob (the selector resolved).
        "record" => record: u64,
        /// The appearance: the asset GUID the design names (its token), the
        /// asset found for it, the library and preset named with it.
        "appearance" => appearance: AssetRef,
        /// The physical material.
        "material" => material: AssetRef,
        /// Faces with an appearance of their own.
        "faces" => faces: Vec<FaceLook>,
        /// The body's light bulb (false: hidden), where the record gives
        /// it (mitcad#1175).
        "light_bulb" => light_bulb: bool,
        /// The stored body as the replay names it (`StoredBody::source`),
        /// where it is not `blob#record`: an `.ipt` body is `<B-rep
        /// record>/<ASM body record>` (mitcad#1367).
        "source" => source: String,
    }
}

record! {
    /// How the design names an asset.
    pub struct AssetRef {
        /// The token: the asset GUID as written.
        "token" => token: String,
        "library" => library: String,
        "preset" => preset: String,
        /// The display name stored with it.
        "display" => display: String,
        /// The asset of the package it names (`assets[].id`).
        "asset" => asset: String,
    }
}

record! {
    /// A face's own appearance.
    pub struct FaceLook {
        /// The GUID the face's B-rep attribute carries.
        "guid" => guid: String,
        "appearance" => appearance: AssetRef,
        /// The colour stored with the assignment (RGBA).
        "color" => color: [f64; 4],
        /// The faces of the body that carry the GUID in the stored design
        /// (a face split by later operations, each piece; their geometry
        /// as for a feature's input, cm, in the component's coordinates).
        "faces" => faces: Vec<Fingerprint>,
        /// Why the face was not found.
        "why" => why: String,
        /// The face reference object it came from (none: its body's
        /// record).
        "object_id" => object_id: u64,
    }
}

record! {
    /// An occurrence's appearance override, or a component's own (no
    /// occurrence: the root component's covers the whole design).
    pub struct OccurrenceLook {
        /// The occurrence object (`occurrences[]._f3d.object_id`).
        "object_id" => object_id: u64,
        "component" => component: u64,
        "appearance" => appearance: AssetRef,
    }
}

record! {
    /// `EmbossFeature` detail (mitcad#458): the profiles (one reference
    /// per selected profile, their loops in `_f3d_profile_loops` as an
    /// extrusion's), the face they lie on, the depth (negative: sunk into
    /// the face) and the profiles' placement on the face.
    pub struct EmbossDetail {
        "profile" => profile: Vec<Reference>,
        "inputFaces" => input_faces: Vec<Reference>,
        "depth" => depth: Reference,
        "horizontalDistance" => horizontal_distance: Reference,
        "verticalDistance" => vertical_distance: Reference,
        "rotationAngle" => rotation_angle: Reference,
    }
}

record! {
    /// `MoveFaceFeature` detail (mitcad#343): the moved faces; the motion,
    /// a rigid 4x4 matrix (row-major, cm) as a move's `transform`, stays in
    /// `other` under `transform`.
    pub struct MoveFaceDetail {
        "faces" => faces: Vec<Reference>,
    }
}

record! {
    /// `DraftFeature` detail (mitcad#1363): the drafted faces, the neutral
    /// plane (a planar face, or an origin or construction plane), the
    /// tangent-chain flag and the first angle as `draftDefinition.angle`
    /// (radians; its sign with the plane gives the pull direction,
    /// SCHEMA.md). The stream decoder adds the second angle scalar, the
    /// faces of a neutral group of more than one face, and a parting-line
    /// draft's pull plane and parting tool.
    pub struct DraftDetail {
        "inputFaces" => input_faces: Vec<Reference>,
        "plane" => plane: Reference,
        "isTangentChain" => is_tangent_chain: bool,
        "isDirectionFlipped" => is_direction_flipped: bool,
        "draftDefinition" => draft_definition: Definition,
        /// The second angle scalar (the opposite side's angle).
        "_f3d_angle_two" => angle_two: Reference,
        /// The neutral group's faces where it has more than one.
        "_f3d_neutral_faces" => neutral_faces: Vec<Reference>,
        /// A parting-line draft: the plane whose normal is the pull
        /// direction, and the faces of the parting tool.
        "_f3d_pull_plane" => pull_plane: Reference,
        "_f3d_parting_tool" => parting_tool: Vec<Reference>,
    }
}

// The control cages of Form bodies (mitcad#1365): `_f3d.cages`.
pub mod cage;
pub use cage::FormCage;
