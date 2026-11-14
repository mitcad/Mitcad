// SPDX-License-Identifier: MIT
//! Design file versions 2 (a single-file design) and 3 (a design in a
//! project; see the module documentation of `file`): version 3 is version 2
//! with B-rep data in a project's store.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{FORMAT, FileError, SINGLE_VERSION, VERSION, has_references, legacy_name};
use crate::appearance::{Appearance, BASE_COLOR, ImageData, share_images, slot_name};
use crate::assembly::{
    Assembly, ComponentDef, ExternalLink, Occurrence, ROOT_NAME, from_rows_straightened,
    matrix_rows, saved_placement,
};
use crate::configurations::Configurations;
use crate::document::{
    Analysis, Annotation, BodyAttributes, Comment, DisplayState, DocState, ImageReference,
    NamedView, TimelineGroup, check_components,
};
use crate::expr::ParamLimits;
use crate::expr::{
    DEFAULT_DECIMALS, EvalContext, ParamId, ParamSpec, TableError, Unit, format_number,
    is_newly_reserved, rename_names, value_to_expression,
};
use crate::features::{CheckContext, FeatureDef, FeatureEntry, HelixConstruction, is_false};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::motion::{ContactSet, MotionStudy};
use crate::parameters::{Parameters, default_context, describe, slot_unit};
use crate::policy::Policy;
use crate::render_settings::RenderSettings;
use crate::sketch::legacy::{LegacyShape, convert};
use crate::transform::Transform;
use mitcad_sketch::internal::settle;

// Writing: field order is the order in the file.

#[derive(Serialize)]
struct FileOut<'a> {
    format: &'static str,
    version: u64,
    units: UnitsOut,
    parameters: Vec<ParameterOut<'a>>,
    // Components and occurrences (F6), when there are any.
    #[serde(skip_serializing_if = "Option::is_none")]
    root_component: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    components: Vec<ComponentOut<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    occurrences: Vec<OccurrenceOut>,
    // The number the next occurrence takes where it is not one past the
    // largest listed (the largest was deleted, mitcad#1320).
    #[serde(skip_serializing_if = "Option::is_none")]
    next_occurrence: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_component: Option<ComponentUid>,
    features: Vec<FeatureOut<'a>>,
    // The number the next feature takes where it is not one past the
    // largest listed (mitcad#1320).
    #[serde(skip_serializing_if = "Option::is_none")]
    next_feature: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    bodies: Vec<BodyOut<'a>>,
    // Named views (U5), when there are any.
    #[serde(skip_serializing_if = "<[NamedView]>::is_empty")]
    views: &'a [NamedView],
    // The Origin folder and Isolate (P9), when not the defaults.
    #[serde(skip_serializing_if = "DisplayState::is_default")]
    display: &'a DisplayState,
    // Timeline groups (P9), when there are any.
    #[serde(skip_serializing_if = "<[TimelineGroup]>::is_empty")]
    groups: &'a [TimelineGroup],
    // Analyses (mitcad#41), when there are any.
    #[serde(skip_serializing_if = "<[Analysis]>::is_empty")]
    analyses: &'a [Analysis],
    // The document's appearances (mitcad#46), when there are any.
    #[serde(skip_serializing_if = "no_appearances")]
    appearances: Cow<'a, [Appearance]>,
    // Their embedded images, once each, when the textures refer to them by
    // digest (mitcad#313).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    images: Vec<ImageOut>,
    // Render settings (mitcad#47), when not the defaults.
    #[serde(skip_serializing_if = "RenderSettings::is_default")]
    render: &'a RenderSettings,
    // The configuration table (mitcad#64), when there is one.
    #[serde(skip_serializing_if = "Configurations::is_empty")]
    configurations: &'a Configurations,
    // A hybrid assembly allowed without asking (mitcad#171), when it is.
    #[serde(skip_serializing_if = "is_false")]
    hybrid_assembly: bool,
    // The joins' tolerance an importer set (mitcad#298), when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    join_tolerance: Option<f64>,
    // Flat Patterns of sheet-metal bodies (mitcad#208), when there are any.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    flat_patterns: Vec<FlatPatternOut<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    marker: Option<usize>,
    // The root component's policy (mitcad#220), when not the defaults.
    #[serde(skip_serializing_if = "Policy::is_default")]
    root_policy: Policy,
    // The library parts BUILD placed (mitcad#375), when there are any.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    library_sources: Vec<crate::build::sources::SourceOut<'a>>,
    // Motion studies and contact sets (mitcad#454), when there are any.
    #[serde(skip_serializing_if = "<[MotionStudy]>::is_empty")]
    motion_studies: &'a [MotionStudy],
    #[serde(skip_serializing_if = "<[ContactSet]>::is_empty")]
    contact_sets: &'a [ContactSet],
    // Own property types and properties (mitcad#496), when there are any.
    #[serde(skip_serializing_if = "<[crate::properties::PropertyType]>::is_empty")]
    property_types: &'a [crate::properties::PropertyType],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    properties: Vec<crate::properties::OwnerProperties>,
    // The design's materials and the components' default materials
    // (mitcad#640), when there are any.
    #[serde(skip_serializing_if = "<[crate::material::Material]>::is_empty")]
    materials: &'a [crate::material::Material],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    component_materials: Vec<ComponentMaterial>,
    // The design's printer and process profiles (mitcad#640), when there
    // are any.
    #[serde(skip_serializing_if = "<[crate::print_profile::PrintProfile]>::is_empty")]
    print_profiles: &'a [crate::print_profile::PrintProfile],
    // Annotations (mitcad#702; the measurement objects of mitcad#649),
    // when there are any: as `measurements` while they all have only what
    // those had, which older versions read; else as `annotations`.
    #[serde(skip_serializing_if = "<[Annotation]>::is_empty")]
    measurements: &'a [Annotation],
    #[serde(skip_serializing_if = "<[Annotation]>::is_empty")]
    annotations: &'a [Annotation],
    // The height of the annotations' texts set for the design (mitcad#894),
    // when it is not the automatic one.
    #[serde(skip_serializing_if = "Option::is_none")]
    annotation_text_height: Option<f64>,
    // Purge volumes set by hand (mitcad#705), when there are any.
    #[serde(skip_serializing_if = "<[crate::print_purge::PurgeOverride]>::is_empty")]
    purge_volumes: &'a [crate::print_purge::PurgeOverride],
    // Drawings of the root and the components (mitcad#707), when there are
    // any.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    drawings: Vec<&'a crate::drawing::def::Drawing>,
    // Image references with their images (mitcad#782), when there are any.
    #[serde(skip_serializing_if = "<[ImageReference]>::is_empty")]
    image_references: &'a [ImageReference],
    // Tables of data that expressions look values up in (mitcad#667),
    // when there are any.
    #[serde(skip_serializing_if = "crate::tables::Tables::is_empty")]
    tables: &'a crate::tables::Tables,
    // Comments (mitcad#1119), when there are any, with the number the next
    // one takes where it is not one past the largest listed (the largest
    // was deleted). A design without comments leaves both out.
    #[serde(skip_serializing_if = "<[Comment]>::is_empty")]
    comments: &'a [Comment],
    #[serde(skip_serializing_if = "Option::is_none")]
    next_comment: Option<u32>,
    // The print plates (mitcad#1050), when there are any.
    #[serde(skip_serializing_if = "<[crate::print_plates::PrintPlate]>::is_empty")]
    print_plates: &'a [crate::print_plates::PrintPlate],
    // Filament changes at print heights (mitcad#1596), when there are any.
    #[serde(skip_serializing_if = "<[crate::print_changes::ChangeSchedule]>::is_empty")]
    filament_changes: &'a [crate::print_changes::ChangeSchedule],
    // CAM setups, their operations and the machines they use
    // (mitcad#1013), when there are any.
    #[serde(skip_serializing_if = "crate::cam::CamState::is_empty")]
    cam: &'a crate::cam::CamState,
    // The design's statistics (mitcad#1088), when anything of them is
    // known. Read apart from the definition (`file/mod.rs`, `load_parts`,
    // which takes it out before the unknown fields), so not in
    // `TOP_FIELDS`.
    #[serde(skip_serializing_if = "super::DesignStatistics::is_empty")]
    statistics: &'a super::DesignStatistics,
    // The design's preview (mitcad#1539), when it has one: last, after
    // the definition. Read apart from it like the statistics.
    #[serde(skip_serializing_if = "Option::is_none")]
    preview: Option<&'a super::DesignPreview>,
}

/// A component's default material (mitcad#640).
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ComponentMaterial {
    component: ComponentUid,
    material: String,
    /// The appearance of the material its bodies get (mitcad#1055); left
    /// out: the material's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    appearance: Option<String>,
}

/// A Flat Pattern: its body and settings.
#[derive(Serialize)]
struct FlatPatternOut<'a> {
    body: BodyUid,
    #[serde(flatten)]
    def: &'a crate::sheet::flat::FlatPatternDef,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FlatPatternIn {
    body: BodyUid,
    stationary: crate::sheet::PanelId,
    #[serde(default)]
    x_axis: crate::sheet::flat::FlatXAxis,
    #[serde(default)]
    reference_side: Option<crate::sheet::SheetReferenceSide>,
    #[serde(default)]
    shown: bool,
}

#[derive(Serialize)]
struct ComponentOut<'a> {
    uid: ComponentUid,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_by: Option<FeatureUid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    link: Option<&'a ExternalLink>,
    // The library it came from (mitcad#64).
    #[serde(skip_serializing_if = "Option::is_none")]
    library: Option<&'a crate::library::LibraryRef>,
    // Its policy (mitcad#220), when not the general defaults.
    #[serde(skip_serializing_if = "Policy::is_default")]
    policy: Policy,
    // What BUILD made it of (mitcad#375).
    #[serde(skip_serializing_if = "Option::is_none")]
    build: Option<&'a crate::build::BuildRecord>,
}

#[derive(Serialize)]
struct OccurrenceOut {
    uid: OccurrenceUid,
    component: ComponentUid,
    parent: ComponentUid,
    number: u32,
    /// Rows of the rotation and the translation; left out for the
    /// identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    transform: Option<[[f64; 4]; 3]>,
    #[serde(skip_serializing_if = "is_false")]
    grounded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pattern: Option<crate::features::point_pattern::PatternInstance>,
    // The condition it is shown under (mitcad#800).
    #[serde(skip_serializing_if = "Option::is_none")]
    visible_when: Option<String>,
    // Drawn see-through (mitcad#1093).
    #[serde(skip_serializing_if = "is_false")]
    transparent: bool,
}

fn is_root(component: &ComponentUid) -> bool {
    component.is_root()
}

#[derive(Serialize)]
struct UnitsOut {
    length: &'static str,
    angle: &'static str,
}

#[derive(Serialize)]
struct ParameterOut<'a> {
    name: &'a str,
    expression: &'a str,
    unit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expression_units: Option<UnitsOut>,
    /// The value when saved (mm, rad), for readers; loading evaluates the
    /// expression.
    value: f64,
    comment: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<FeatureUid>,
    // Favourite parameters (P9).
    #[serde(skip_serializing_if = "is_false")]
    favorite: bool,
    // Limits (mitcad#665).
    #[serde(skip_serializing_if = "Option::is_none")]
    min: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    step: Option<&'a str>,
    #[serde(skip_serializing_if = "is_false")]
    strict: bool,
    // Allowed values (mitcad#798).
    #[serde(skip_serializing_if = "Option::is_none")]
    values: Option<&'a [String]>,
}

#[derive(Serialize)]
struct FeatureOut<'a> {
    uid: FeatureUid,
    name: &'a str,
    #[serde(skip_serializing_if = "is_false")]
    suppressed: bool,
    // The condition it is suppressed under (mitcad#800).
    #[serde(skip_serializing_if = "Option::is_none")]
    suppressed_when: Option<&'a str>,
    #[serde(skip_serializing_if = "is_root")]
    component: ComponentUid,
    /// Set by the user for sketches and construction features (U3).
    #[serde(skip_serializing_if = "Option::is_none")]
    visible: Option<bool>,
    /// How large a construction plane or axis is drawn, mm, where its
    /// resize handles set it (mitcad#1748).
    #[serde(skip_serializing_if = "Option::is_none")]
    display_size: Option<f64>,
    /// What the importer that made it said (mitcad#170).
    #[serde(skip_serializing_if = "Option::is_none")]
    import_note: Option<&'a str>,
    /// What it remembers of the pieces it split bodies into (mitcad#408).
    #[serde(skip_serializing_if = "Option::is_none")]
    split_pieces: Option<&'a [crate::features::PieceGroup]>,
    #[serde(flatten)]
    def: FeatureDef<String>,
}

#[derive(Serialize)]
struct BodyOut<'a> {
    uid: BodyUid,
    name: &'a str,
    // Attributes, when not the default.
    #[serde(skip_serializing_if = "Option::is_none")]
    visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    material: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    appearance: Option<&'a str>,
    // Appearances of single faces by face name (mitcad#53).
    #[serde(skip_serializing_if = "Option::is_none")]
    face_appearances: Option<&'a BTreeMap<String, String>>,
    // The face that lies on the print bed (mitcad#731).
    #[serde(skip_serializing_if = "Option::is_none")]
    print_bed_face: Option<&'a str>,
    // Expressions the values above follow (mitcad#800).
    #[serde(skip_serializing_if = "Option::is_none")]
    appearance_expr: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    material_expr: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visible_when: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    face_appearance_exprs: Option<&'a BTreeMap<String, String>>,
    // Drawn see-through (mitcad#1093).
    #[serde(skip_serializing_if = "is_false")]
    transparent: bool,
}

pub(super) fn save(state: &DocState) -> String {
    save_with(state, &super::DesignStatistics::default(), None)
}

/// [`save`] with the design's statistics (mitcad#1088), left out while
/// nothing of them is known, and its preview (mitcad#1539), if it has one.
pub(super) fn save_with(
    state: &DocState,
    statistics: &super::DesignStatistics,
    preview: Option<&super::DesignPreview>,
) -> String {
    let (appearances, images) = appearances_out(&state.appearances);
    let parameters = &state.parameters;
    let context = parameters.context();
    let assembly = &state.assembly;
    // Annotations as older versions read them while they can (mitcad#702).
    let old_form = state
        .annotations
        .iter()
        .all(Annotation::is_measurement_form);
    let file = FileOut {
        format: FORMAT,
        // Version 3 only when base features refer to a project's B-rep store.
        version: if has_references(state) {
            VERSION
        } else {
            SINGLE_VERSION
        },
        units: UnitsOut {
            length: context.default_length_unit.symbol(),
            angle: context.default_angle_unit.symbol(),
        },
        parameters: parameters
            .iter()
            .map(|p| ParameterOut {
                name: p.name(),
                expression: p.expression(),
                unit: p.unit().to_string(),
                expression_units: p.expression_context().map(|context| UnitsOut {
                    length: context.default_length_unit.symbol(),
                    angle: context.default_angle_unit.symbol(),
                }),
                value: p.value(),
                comment: p.comment(),
                owner: parameters.owner(p.id()),
                favorite: state.favorites.contains(&p.id()),
                min: p.limits().min.as_deref(),
                max: p.limits().max.as_deref(),
                step: p.limits().step.as_deref(),
                strict: p.limits().strict,
                values: p.limits().values.as_deref(),
            })
            .collect(),
        root_component: (assembly.root_name != ROOT_NAME).then_some(assembly.root_name.as_str()),
        components: assembly
            .components
            .iter()
            .map(|c| ComponentOut {
                uid: c.uid,
                name: &c.name,
                created_by: c.created_by,
                link: c.link.as_ref(),
                library: c.library.as_ref(),
                policy: c.policy,
                build: c.build.as_ref(),
            })
            .collect(),
        occurrences: assembly
            .occurrences
            .iter()
            .map(|o| OccurrenceOut {
                uid: o.uid,
                component: o.component,
                parent: o.parent,
                number: o.number,
                // Never one that opening would refuse (mitcad#1664).
                transform: {
                    let placed = saved_placement(&o.transform);
                    (!placed.is_identity()).then(|| matrix_rows(&placed))
                },
                grounded: o.grounded,
                visible: (!o.visible).then_some(false),
                pattern: o.pattern.clone(),
                visible_when: state.bindings.occurrence_visible_when.get(&o.uid).cloned(),
                transparent: o.transparent,
            })
            .collect(),
        next_occurrence: next_occurrence_out(assembly),
        active_component: (!assembly.active.is_root()).then_some(assembly.active),
        features: state
            .features
            .iter()
            .map(|f| FeatureOut {
                uid: f.uid,
                name: &f.name,
                suppressed: f.suppressed,
                suppressed_when: state
                    .bindings
                    .suppressed_when
                    .get(&f.uid)
                    .map(String::as_str),
                component: f.component,
                visible: state.feature_visibility.get(&f.uid).copied(),
                display_size: state.datum_display_sizes.get(&f.uid).copied(),
                import_note: state.import_notes.get(&f.uid).map(String::as_str),
                split_pieces: state.split_pieces.get(&f.uid).map(|g| &g[..]),
                def: f
                    .def
                    .map_params(&mut |_, id| Ok::<_, ()>(parameters.name(*id)))
                    .expect("names never fail"),
            })
            .collect(),
        next_feature: next_feature_out(state),
        bodies: state
            .body_names
            .iter()
            .map(|(uid, name)| {
                let attributes = state.body_attributes.get(uid);
                BodyOut {
                    uid: *uid,
                    name,
                    visible: attributes.filter(|a| !a.visible).map(|_| false),
                    material: attributes.and_then(|a| a.material.as_deref()),
                    appearance: attributes.and_then(|a| a.appearance.as_deref()),
                    face_appearances: attributes
                        .map(|a| &a.face_appearances)
                        .filter(|faces| !faces.is_empty()),
                    print_bed_face: attributes.and_then(|a| a.print_bed_face.as_deref()),
                    appearance_expr: attributes.and_then(|a| a.appearance_expr.as_deref()),
                    material_expr: attributes.and_then(|a| a.material_expr.as_deref()),
                    visible_when: attributes.and_then(|a| a.visible_when.as_deref()),
                    face_appearance_exprs: attributes
                        .map(|a| &a.face_appearance_exprs)
                        .filter(|faces| !faces.is_empty()),
                    transparent: attributes.is_some_and(|a| a.transparent),
                }
            })
            .collect(),
        views: &state.named_views,
        display: &state.display,
        groups: &state.groups,
        analyses: &state.analyses,
        appearances,
        images,
        render: &state.render,
        configurations: &state.configurations,
        hybrid_assembly: state.hybrid_assembly,
        join_tolerance: state.join_tolerance,
        flat_patterns: state
            .flat_patterns
            .iter()
            .map(|(body, def)| FlatPatternOut { body: *body, def })
            .collect(),
        marker: (state.marker < state.features.len()).then_some(state.marker),
        root_policy: assembly.root_policy,
        library_sources: state
            .library_sources
            .iter()
            .map(crate::build::sources::SourceOut::of)
            .collect(),
        motion_studies: &state.motion_studies,
        contact_sets: &state.contact_sets,
        property_types: &state.properties.types,
        // Only the owners the design has (computed values are never
        // stored).
        properties: state
            .properties
            .file_owners(|owner| state.property_owner_kept(owner)),
        materials: &state.materials,
        // Only the components the design has.
        component_materials: state
            .component_materials
            .iter()
            .filter(|(c, _)| c.is_root() || assembly.component(**c).is_some())
            .map(|(c, m)| ComponentMaterial {
                component: *c,
                material: m.clone(),
                appearance: state.component_appearances.get(c).cloned(),
            })
            .collect(),
        print_profiles: &state.print_profiles,
        measurements: if old_form {
            state.annotations.as_slice()
        } else {
            &[]
        },
        annotations: if old_form {
            &[]
        } else {
            state.annotations.as_slice()
        },
        annotation_text_height: state.annotation_text_height,
        purge_volumes: &state.purge_volumes,
        // Only the components the design has.
        drawings: state
            .drawings
            .iter()
            .filter(|d| assembly.exists(d.component))
            .collect(),
        image_references: &state.image_references,
        tables: &state.tables,
        comments: &state.comments,
        next_comment: next_comment_out(state),
        print_plates: &state.print_plates,
        filament_changes: &state.filament_changes,
        cam: &state.cam,
        statistics,
        preview,
    };
    let mut json =
        serde_json::to_string_pretty(&file).expect("the design file schema always serializes");
    json.push('\n');
    json
}

/// The `next_comment` a file keeps (mitcad#1119): only with comments, and
/// only where it is not one past the largest of them, which reading takes.
fn next_comment_out(state: &DocState) -> Option<u32> {
    let largest = state.comments.iter().map(|c| c.id).max()?;
    (state.next_comment > largest.saturating_add(1)).then_some(state.next_comment)
}

/// The `next_feature` a file keeps (mitcad#1320): only where it is not one
/// past the largest feature uid listed, which reading takes, so that the
/// uid of a deleted last feature is not given out again after a reload (a
/// plate item, a comment or a drawing's recorded body keeps it on purpose).
fn next_feature_out(state: &DocState) -> Option<u64> {
    let largest: u64 = state.features.iter().map(|f| f.uid.0).max().unwrap_or(0);
    (state.next_uid > largest.saturating_add(1)).then_some(state.next_uid)
}

/// The `next_occurrence` a file keeps (mitcad#1320), as [`next_feature_out`]
/// for occurrences.
fn next_occurrence_out(assembly: &Assembly) -> Option<u32> {
    let largest: u32 = assembly
        .occurrences
        .iter()
        .map(|o| o.uid.0)
        .max()
        .unwrap_or(0);
    (assembly.next_occurrence > largest.saturating_add(1)).then_some(assembly.next_occurrence)
}

fn no_appearances(appearances: &[Appearance]) -> bool {
    appearances.is_empty()
}

/// An embedded image in the file's `images` (mitcad#313).
#[derive(Serialize)]
struct ImageOut {
    sha256: String,
    data: ImageData,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImageIn {
    sha256: String,
    data: ImageData,
}

/// The appearances as the file keeps them, and the file's `images`: as
/// before mitcad#313 (each embedded image in its texture's `data`, no
/// `images`) unless an image of a map other than the base colour is
/// embedded or one image is embedded more than once; then each image is
/// kept once in `images`, in the order of first use, and the textures
/// refer to it by its digest (`image`).
fn appearances_out(appearances: &[Appearance]) -> (Cow<'_, [Appearance]>, Vec<ImageOut>) {
    let mut uses: HashMap<&str, usize> = HashMap::new();
    let mut maps = false;
    for texture in appearances.iter().filter_map(|a| a.texture.as_ref()) {
        for (map, data) in texture.images() {
            *uses.entry(data.sha256()).or_default() += 1;
            maps |= map != BASE_COLOR;
        }
    }
    if !maps && uses.values().all(|n| *n == 1) {
        return (Cow::Borrowed(appearances), Vec::new());
    }
    let mut images: Vec<ImageOut> = Vec::new();
    let mut listed = HashSet::new();
    let mut out = appearances.to_vec();
    for texture in out.iter_mut().filter_map(|a| a.texture.as_mut()) {
        for slot in texture.slots_mut() {
            if let Some(data) = slot.data.take() {
                let sha256 = data.sha256().to_owned();
                if listed.insert(sha256.clone()) {
                    images.push(ImageOut {
                        sha256: sha256.clone(),
                        data,
                    });
                }
                *slot.image = Some(sha256);
            }
        }
    }
    (Cow::Owned(out), images)
}

/// The file's `images` by digest, each checked: a PNG or JPEG file of at
/// most 32 MB whose SHA-256 is the one given, listed once.
fn images_in(top: &mut Map<String, Value>) -> Result<HashMap<String, ImageData>, FileError> {
    let mut images = HashMap::new();
    for (i, image) in array(top, "images", false)?.into_iter().enumerate() {
        let image: ImageIn =
            serde_json::from_value(image).map_err(|e| schema(format!("images[{i}]: {e}")))?;
        image
            .data
            .bytes()
            .map_err(|e| invalid(format!("images[{i}]: {e}")))?;
        if image.data.sha256() != image.sha256 {
            return Err(invalid(format!(
                "images[{i}]: the image's SHA-256 is {}, not {}",
                image.data.sha256(),
                image.sha256
            )));
        }
        if images.insert(image.sha256.clone(), image.data).is_some() {
            return Err(invalid(format!(
                "images[{i}]: image {} is listed more than once",
                image.sha256
            )));
        }
    }
    Ok(images)
}

/// An appearance of the file, its textures' images (`image`) taken from
/// the file's `images`, shared.
fn appearance_in(
    i: usize,
    value: Value,
    images: &HashMap<String, ImageData>,
) -> Result<Appearance, FileError> {
    let mut appearance = serde_json::from_value::<Appearance>(value)
        .map_err(|e| schema(format!("appearances[{i}]: {e}")))?;
    if let Some(texture) = appearance.texture.as_mut() {
        for slot in texture.slots_mut() {
            let Some(digest) = slot.image.take() else {
                continue;
            };
            let what = slot_name(slot.map);
            if slot.data.is_some() {
                return Err(schema(format!(
                    "appearances[{i}]: {what}: both `data` and `image`"
                )));
            }
            *slot.data = Some(images.get(&digest).cloned().ok_or_else(|| {
                invalid(format!(
                    "appearances[{i}]: {what}: image {digest} is not in `images`"
                ))
            })?);
        }
    }
    appearance
        .check()
        .map_err(|e| invalid(format!("appearances[{i}]: {e}")))?;
    Ok(appearance)
}

// Reading: the JSON value is walked, so every error can say where it is.

/// A parameter: `expression` and `unit`, or only a `value` (millimetres or
/// radians) in files written before expressions.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParameterIn {
    name: String,
    #[serde(default)]
    expression: Option<String>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    expression_units: Option<UnitsIn>,
    #[serde(default)]
    value: Option<f64>,
    #[serde(default)]
    comment: String,
    #[serde(default)]
    owner: Option<FeatureUid>,
    #[serde(default)]
    favorite: bool,
    // Limits (mitcad#665).
    #[serde(default)]
    min: Option<String>,
    #[serde(default)]
    max: Option<String>,
    #[serde(default)]
    step: Option<String>,
    #[serde(default)]
    strict: bool,
    // Allowed values (mitcad#798): a list, or a table column as its one
    // entry, also written as the column alone (`"Screws.Size"`, mitcad#806).
    #[serde(default, deserialize_with = "values_list")]
    values: Option<Vec<String>>,
}

/// The allowed values of a parameter in the file: a list of expressions,
/// or a table column (`"Screws.Size"`) read as a list of that one entry.
fn values_list<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<String>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Values {
        List(Vec<String>),
        Column(String),
    }
    Ok(match Option::<Values>::deserialize(deserializer)? {
        None => None,
        Some(Values::List(list)) => Some(list),
        Some(Values::Column(column)) => Some(vec![column]),
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnitsIn {
    #[serde(default)]
    length: Option<String>,
    #[serde(default)]
    angle: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BodyIn {
    uid: BodyUid,
    name: String,
    #[serde(default = "visible")]
    visible: bool,
    #[serde(default)]
    material: Option<String>,
    #[serde(default)]
    appearance: Option<String>,
    #[serde(default)]
    face_appearances: BTreeMap<String, String>,
    #[serde(default)]
    print_bed_face: Option<String>,
    // Bindings (mitcad#800).
    #[serde(default)]
    appearance_expr: Option<String>,
    #[serde(default)]
    material_expr: Option<String>,
    #[serde(default)]
    visible_when: Option<String>,
    #[serde(default)]
    face_appearance_exprs: BTreeMap<String, String>,
    // Drawn see-through (mitcad#1093).
    #[serde(default)]
    transparent: bool,
}

fn visible() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ComponentIn {
    uid: ComponentUid,
    name: String,
    #[serde(default)]
    created_by: Option<FeatureUid>,
    #[serde(default)]
    link: Option<ExternalLink>,
    #[serde(default)]
    library: Option<crate::library::LibraryRef>,
    #[serde(default)]
    policy: Policy,
    #[serde(default)]
    build: Option<crate::build::BuildRecord>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OccurrenceIn {
    uid: OccurrenceUid,
    component: ComponentUid,
    #[serde(default)]
    parent: ComponentUid,
    number: u32,
    #[serde(default)]
    transform: Option<Vec<Vec<f64>>>,
    #[serde(default)]
    grounded: bool,
    #[serde(default = "visible")]
    visible: bool,
    #[serde(default)]
    pattern: Option<crate::features::point_pattern::PatternInstance>,
    #[serde(default)]
    visible_when: Option<String>,
    // Drawn see-through (mitcad#1093).
    #[serde(default)]
    transparent: bool,
}

struct FeatureIn {
    uid: FeatureUid,
    name: String,
    suppressed: bool,
    /// The condition it is suppressed under (mitcad#800).
    suppressed_when: Option<String>,
    component: ComponentUid,
    visible: Option<bool>,
    /// How large a construction plane or axis is drawn (mitcad#1748).
    display_size: Option<f64>,
    /// What the importer that made it said (mitcad#170).
    import_note: Option<String>,
    /// What it remembers of the pieces of its splits (mitcad#408).
    split_pieces: Vec<crate::features::PieceGroup>,
    def: FeatureDef<String>,
    /// A sketch of rectangles and circles (`"shapes"`, early version 2):
    /// converted again with the parameter values once they are loaded, so
    /// that the stored positions are exact.
    shapes: Option<Value>,
}

const TOP_FIELDS: [&str; 45] = [
    "format",
    "version",
    "units",
    "parameters",
    "root_component",
    "components",
    "occurrences",
    "active_component",
    "features",
    "bodies",
    "marker",
    "views",
    "display",
    "groups",
    "analyses",
    "appearances",
    "render",
    // The configuration table (mitcad#64).
    "configurations",
    // A hybrid assembly allowed without asking (mitcad#171).
    "hybrid_assembly",
    // The joins' tolerance an importer set (mitcad#298).
    "join_tolerance",
    // Flat Patterns of sheet-metal bodies (mitcad#208).
    "flat_patterns",
    // The root component's policy (mitcad#220), when not the defaults.
    "root_policy",
    // Embedded images by digest (mitcad#313).
    "images",
    // The library parts BUILD placed (mitcad#375).
    "library_sources",
    // Motion studies and contact sets (mitcad#454).
    "motion_studies",
    "contact_sets",
    // Own property types and properties (mitcad#496).
    "property_types",
    "properties",
    // The design's materials and the components' defaults (mitcad#640).
    "materials",
    "component_materials",
    // The design's printer and process profiles (mitcad#640).
    "print_profiles",
    // Measurement objects (mitcad#649), read as annotations.
    "measurements",
    // Annotations (mitcad#702).
    "annotations",
    // Purge volumes set by hand (mitcad#705).
    "purge_volumes",
    // Drawings of the root and the components (mitcad#707).
    "drawings",
    // Image references (mitcad#782).
    "image_references",
    // Tables of data that expressions look values up in (mitcad#667).
    "tables",
    // The height of the annotations' texts (mitcad#894).
    "annotation_text_height",
    // Comments and the next comment's number (mitcad#1119).
    "comments",
    "next_comment",
    // The print plates (mitcad#1050).
    "print_plates",
    // The numbers the next feature and occurrence take (mitcad#1320).
    "next_feature",
    "next_occurrence",
    // Filament changes at print heights (mitcad#1596).
    "filament_changes",
    // CAM setups, operations and machines (mitcad#1013).
    "cam",
];

fn schema(message: impl Into<String>) -> FileError {
    FileError::Schema(message.into())
}

fn invalid(message: impl Into<String>) -> FileError {
    FileError::Invalid(message.into())
}

fn array(
    top: &mut Map<String, Value>,
    field: &str,
    required: bool,
) -> Result<Vec<Value>, FileError> {
    match top.remove(field) {
        Some(Value::Array(items)) => Ok(items),
        Some(_) => Err(schema(format!("\"{field}\" must be an array"))),
        None if required => Err(schema(format!("missing field `{field}`"))),
        None => Ok(Vec::new()),
    }
}

fn feature_in(i: usize, value: Value) -> Result<FeatureIn, FileError> {
    let Value::Object(mut fields) = value else {
        return Err(schema(format!("features[{i}]: expected an object")));
    };
    let text = |fields: &mut Map<String, Value>, field: &str| match fields.remove(field) {
        Some(Value::String(text)) => Ok(text),
        Some(_) => Err(schema(format!(
            "features[{i}]: \"{field}\" must be a string"
        ))),
        None => Err(schema(format!("features[{i}]: missing field `{field}`"))),
    };
    let uid_text = text(&mut fields, "uid")?;
    let name = text(&mut fields, "name")?;
    let uid = uid_text
        .parse()
        .map_err(|e| schema(format!("features[{i}] ({name}): {e}")))?;
    let suppressed = match fields.remove("suppressed") {
        None => false,
        Some(Value::Bool(value)) => value,
        Some(_) => {
            return Err(schema(format!(
                "features[{i}] ({name}): \"suppressed\" must be true or false"
            )));
        }
    };
    let suppressed_when = match fields.remove("suppressed_when") {
        None => None,
        Some(Value::String(text)) => Some(text).filter(|t| !t.trim().is_empty()),
        Some(_) => {
            return Err(schema(format!(
                "features[{i}] ({name}): \"suppressed_when\" must be a string"
            )));
        }
    };
    let component = match fields.remove("component") {
        None => ComponentUid::ROOT,
        Some(Value::String(text)) => text
            .parse()
            .map_err(|e| schema(format!("features[{i}] ({name}): {e}")))?,
        Some(_) => {
            return Err(schema(format!(
                "features[{i}] ({name}): \"component\" must be a component id"
            )));
        }
    };
    let visible = match fields.remove("visible") {
        None => None,
        Some(Value::Bool(value)) => Some(value),
        Some(_) => {
            return Err(schema(format!(
                "features[{i}] ({name}): \"visible\" must be true or false"
            )));
        }
    };
    let display_size = match fields.remove("display_size") {
        None => None,
        Some(Value::Number(size)) => match size.as_f64() {
            Some(size) if size.is_finite() && size > 0.0 => Some(size),
            _ => {
                return Err(schema(format!(
                    "features[{i}] ({name}): \"display_size\" must be greater than zero"
                )));
            }
        },
        Some(_) => {
            return Err(schema(format!(
                "features[{i}] ({name}): \"display_size\" must be a number of millimetres"
            )));
        }
    };
    let import_note = match fields.remove("import_note") {
        None => None,
        Some(Value::String(note)) => Some(note).filter(|n| !n.trim().is_empty()),
        Some(_) => {
            return Err(schema(format!(
                "features[{i}] ({name}): \"import_note\" must be a string"
            )));
        }
    };
    let split_pieces = match fields.remove("split_pieces") {
        None => Vec::new(),
        Some(value) => serde_json::from_value(value)
            .map_err(|e| schema(format!("features[{i}] ({name}): split_pieces: {e}")))?,
    };
    let shapes = fields.get("shapes").cloned();
    let def = serde_json::from_value(Value::Object(fields))
        .map_err(|e| schema(format!("features[{i}] ({name}): {e}")))?;
    Ok(FeatureIn {
        uid,
        name,
        suppressed,
        suppressed_when,
        component,
        visible,
        display_size,
        import_note,
        split_pieces,
        def,
        shapes,
    })
}

/// The components and occurrences of a file; files without them have
/// only the root component, which holds every feature. Also the conditions
/// occurrences are shown under (mitcad#800). The occurrences whose
/// placements were a little off a rotation and were straightened
/// (mitcad#1664) are added to `straightened`.
fn assembly_in(
    top: &mut Map<String, Value>,
    straightened: &mut Vec<OccurrenceUid>,
) -> Result<(Assembly, BTreeMap<OccurrenceUid, String>), FileError> {
    let mut assembly = Assembly::default();
    let mut visible_when = BTreeMap::new();
    match top.remove("root_component") {
        None => {}
        Some(Value::String(name)) if !name.trim().is_empty() => assembly.root_name = name,
        Some(_) => return Err(schema("\"root_component\" must be a name")),
    }
    // Files without it have the general defaults (mitcad#220).
    if let Some(policy) = top.remove("root_policy") {
        assembly.root_policy =
            serde_json::from_value(policy).map_err(|e| schema(format!("root_policy: {e}")))?;
    }
    for (i, c) in array(top, "components", false)?.into_iter().enumerate() {
        let c: ComponentIn =
            serde_json::from_value(c).map_err(|e| schema(format!("components[{i}]: {e}")))?;
        if assembly.exists(c.uid) {
            return Err(invalid(format!(
                "components[{i}]: component {} is listed more than once",
                c.uid
            )));
        }
        assembly.next_component = assembly.next_component.max(c.uid.0 + 1);
        assembly.components.push(ComponentDef {
            uid: c.uid,
            name: c.name,
            created_by: c.created_by,
            link: c.link,
            library: c.library,
            policy: c.policy,
            build: c.build,
        });
    }
    for (i, o) in array(top, "occurrences", false)?.into_iter().enumerate() {
        let o: OccurrenceIn =
            serde_json::from_value(o).map_err(|e| schema(format!("occurrences[{i}]: {e}")))?;
        if assembly.occurrence(o.uid).is_some() {
            return Err(invalid(format!(
                "occurrences[{i}]: occurrence {} is listed more than once",
                o.uid
            )));
        }
        let transform = match &o.transform {
            None => Transform::IDENTITY,
            Some(rows) => {
                let (transform, repaired) = from_rows_straightened(rows)
                    .map_err(|e| invalid(format!("occurrences[{i}]: {e}")))?;
                if repaired {
                    straightened.push(o.uid);
                }
                transform
            }
        };
        assembly.next_occurrence = assembly.next_occurrence.max(o.uid.0 + 1);
        if let Some(text) = o.visible_when.filter(|t| !t.trim().is_empty()) {
            visible_when.insert(o.uid, text);
        }
        assembly.occurrences.push(Occurrence {
            uid: o.uid,
            component: o.component,
            parent: o.parent,
            transform,
            grounded: o.grounded,
            visible: o.visible,
            transparent: o.transparent,
            number: o.number,
            pattern: o.pattern,
        });
    }
    // The number the next occurrence takes, kept where the largest was
    // deleted (mitcad#1320); older files have none, and a smaller one
    // than the largest listed counts on from that.
    if let Some(value) = top.remove("next_occurrence") {
        let next: u32 = value
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n >= 1)
            .ok_or_else(|| schema("\"next_occurrence\" must be a number from 1"))?;
        assembly.next_occurrence = assembly.next_occurrence.max(next);
    }
    match top.remove("active_component") {
        None => {}
        Some(value) => {
            assembly.active = serde_json::from_value(value)
                .map_err(|e| schema(format!("active_component: {e}")))?;
        }
    }
    Ok((assembly, visible_when))
}

/// Takes the top-level fields this build does not know out of a design
/// file's object and returns their names, sorted (mitcad#1132). A newer MitCad wrote them: they are ignored, so that a
/// field added later does not keep this build from opening the design,
/// and the caller warns that they were not read (saving over the file
/// drops them). Fields inside the known ones are still checked strictly.
pub(super) fn take_unknown_fields(value: &mut Value) -> Vec<String> {
    let Some(top) = value.as_object_mut() else {
        return Vec::new();
    };
    let unknown: Vec<String> = top
        .keys()
        .filter(|k| !TOP_FIELDS.contains(&k.as_str()))
        .cloned()
        .collect();
    for field in &unknown {
        top.remove(field);
    }
    unknown
}

/// Reads a version 2 or 3 design file's object whose unknown top-level
/// fields [`take_unknown_fields`] took out.
pub(super) fn load(value: Value) -> Result<(DocState, Vec<String>), FileError> {
    let Value::Object(mut top) = value else {
        return Err(FileError::NotADesign);
    };
    let context = match top.remove("units") {
        None => default_context(),
        Some(units) => {
            let units: UnitsIn =
                serde_json::from_value(units).map_err(|e| schema(format!("units: {e}")))?;
            units_context(&units)?
        }
    };
    let parameters = array(&mut top, "parameters", true)?
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            serde_json::from_value::<ParameterIn>(p)
                .map_err(|e| schema(format!("parameters[{i}]: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut straightened = Vec::new();
    let (assembly, occurrence_visible_when) = assembly_in(&mut top, &mut straightened)?;
    let features = array(&mut top, "features", true)?
        .into_iter()
        .enumerate()
        .map(|(i, f)| feature_in(i, f))
        .collect::<Result<Vec<_>, _>>()?;
    let bodies = array(&mut top, "bodies", false)?
        .into_iter()
        .enumerate()
        .map(|(i, b)| {
            serde_json::from_value::<BodyIn>(b).map_err(|e| schema(format!("bodies[{i}]: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let views = array(&mut top, "views", false)?
        .into_iter()
        .enumerate()
        .map(|(i, v)| {
            let view = serde_json::from_value::<NamedView>(v)
                .map_err(|e| schema(format!("views[{i}]: {e}")))?;
            view.check()
                .map_err(|e| invalid(format!("views[{i}]: {e}")))?;
            Ok(view)
        })
        .collect::<Result<Vec<_>, FileError>>()?;
    let display = match top.remove("display") {
        None => DisplayState::default(),
        Some(value) => serde_json::from_value::<DisplayState>(value)
            .map_err(|e| schema(format!("display: {e}")))?,
    };
    let groups = array(&mut top, "groups", false)?
        .into_iter()
        .enumerate()
        .map(|(i, g)| {
            serde_json::from_value::<TimelineGroup>(g)
                .map_err(|e| schema(format!("groups[{i}]: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let analyses = array(&mut top, "analyses", false)?
        .into_iter()
        .enumerate()
        .map(|(i, a)| {
            let analysis = serde_json::from_value::<Analysis>(a)
                .map_err(|e| schema(format!("analyses[{i}]: {e}")))?;
            analysis
                .check()
                .map_err(|e| invalid(format!("analyses[{i}]: {e}")))?;
            Ok(analysis)
        })
        .collect::<Result<Vec<_>, FileError>>()?;
    let images = images_in(&mut top)?;
    let mut appearances = array(&mut top, "appearances", false)?
        .into_iter()
        .enumerate()
        .map(|(i, a)| appearance_in(i, a, &images))
        .collect::<Result<Vec<_>, FileError>>()?;
    // One copy of each image, also of one embedded twice in an older file.
    let mut shared = HashMap::new();
    for texture in appearances.iter_mut().filter_map(|a| a.texture.as_mut()) {
        share_images(texture, &mut shared);
    }
    let render = match top.remove("render") {
        None => RenderSettings::default(),
        Some(value) => {
            let render = serde_json::from_value::<RenderSettings>(value)
                .map_err(|e| schema(format!("render: {e}")))?;
            render
                .check()
                .map_err(|e| invalid(format!("render: {e}")))?;
            render
        }
    };
    let configurations = match top.remove("configurations") {
        None => Configurations::default(),
        Some(value) => serde_json::from_value::<Configurations>(value)
            .map_err(|e| schema(format!("configurations: {e}")))?,
    };
    let hybrid_assembly = match top.remove("hybrid_assembly") {
        None => false,
        Some(Value::Bool(allowed)) => allowed,
        Some(_) => return Err(schema("\"hybrid_assembly\" must be true or false")),
    };
    let join_tolerance = match top.remove("join_tolerance") {
        None => None,
        Some(value) => match value.as_f64() {
            Some(t) if t.is_finite() && t > 0.0 => Some(t),
            _ => return Err(schema("\"join_tolerance\" must be a positive number")),
        },
    };
    let flat_patterns = array(&mut top, "flat_patterns", false)?
        .into_iter()
        .enumerate()
        .map(|(i, f)| {
            serde_json::from_value::<FlatPatternIn>(f)
                .map_err(|e| schema(format!("flat_patterns[{i}]: {e}")))
        })
        .collect::<Result<Vec<_>, FileError>>()?;
    let mut library_sources = crate::build::LibrarySources::default();
    for (i, s) in array(&mut top, "library_sources", false)?
        .into_iter()
        .enumerate()
    {
        let s: crate::build::sources::SourceIn =
            serde_json::from_value(s).map_err(|e| schema(format!("library_sources[{i}]: {e}")))?;
        let source = s
            .source()
            .map_err(|e| invalid(format!("library_sources[{i}]: {e}")))?;
        if library_sources.get(source.id).is_some() {
            return Err(invalid(format!(
                "library_sources[{i}]: source {} is listed more than once",
                source.id
            )));
        }
        library_sources.0.push(source);
    }
    // Motion studies and contact sets (mitcad#454).
    let motion_studies = array(&mut top, "motion_studies", false)?
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let study = serde_json::from_value::<MotionStudy>(s)
                .map_err(|e| schema(format!("motion_studies[{i}]: {e}")))?;
            study
                .check()
                .map_err(|e| invalid(format!("motion_studies[{i}]: {e}")))?;
            Ok(study)
        })
        .collect::<Result<Vec<_>, FileError>>()?;
    let contact_sets = array(&mut top, "contact_sets", false)?
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let set = serde_json::from_value::<ContactSet>(s)
                .map_err(|e| schema(format!("contact_sets[{i}]: {e}")))?;
            set.check()
                .map_err(|e| invalid(format!("contact_sets[{i}]: {e}")))?;
            Ok(set)
        })
        .collect::<Result<Vec<_>, FileError>>()?;
    // Own property types and properties (mitcad#496).
    let property_types = array(&mut top, "property_types", false)?
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            serde_json::from_value::<crate::properties::PropertyType>(t)
                .map_err(|e| schema(format!("property_types[{i}]: {e}")))
        })
        .collect::<Result<Vec<_>, FileError>>()?;
    let property_owners = array(&mut top, "properties", false)?
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            serde_json::from_value::<crate::properties::OwnerProperties>(p)
                .map_err(|e| schema(format!("properties[{i}]: {e}")))
        })
        .collect::<Result<Vec<_>, FileError>>()?;
    let properties = crate::properties::PropertyStore::from_file(property_types, property_owners)
        .map_err(invalid)?;
    // The design's materials and the components' defaults (mitcad#640).
    let materials = array(&mut top, "materials", false)?
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let material = serde_json::from_value::<crate::material::Material>(m)
                .map_err(|e| schema(format!("materials[{i}]: {e}")))?;
            material
                .check()
                .map_err(|e| invalid(format!("materials[{i}]: {e}")))?;
            Ok(material)
        })
        .collect::<Result<Vec<_>, FileError>>()?;
    let component_materials = array(&mut top, "component_materials", false)?
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            serde_json::from_value::<ComponentMaterial>(c)
                .map_err(|e| schema(format!("component_materials[{i}]: {e}")))
        })
        .collect::<Result<Vec<_>, FileError>>()?;
    let known_material = |id: &str| {
        crate::material::built_in_material(id).is_some() || materials.iter().any(|m| m.id == id)
    };
    // The design's printer and process profiles (mitcad#640): each valid,
    // each id once.
    let print_profiles = array(&mut top, "print_profiles", false)?
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            let profile = serde_json::from_value::<crate::print_profile::PrintProfile>(p)
                .map_err(|e| schema(format!("print_profiles[{i}]: {e}")))?;
            profile
                .check()
                .map_err(|e| invalid(format!("print_profiles[{i}]: {e}")))?;
            Ok(profile)
        })
        .collect::<Result<Vec<_>, FileError>>()?;
    for (i, profile) in print_profiles.iter().enumerate() {
        if print_profiles[..i].iter().any(|p| p.id == profile.id) {
            return Err(invalid(format!(
                "print_profiles[{i}]: profile {} is listed more than once",
                profile.id
            )));
        }
    }
    // Annotations (mitcad#702), and the measurement objects older files
    // keep as `measurements` (mitcad#649), read as annotations unchanged:
    // their picks are resolved when they are asked for, so one whose pick
    // is gone loads and says why.
    let mut annotations: Vec<Annotation> = Vec::new();
    for key in ["measurements", "annotations"] {
        for (i, a) in array(&mut top, key, false)?.into_iter().enumerate() {
            let annotation = serde_json::from_value::<Annotation>(a)
                .map_err(|e| schema(format!("{key}[{i}]: {e}")))?;
            annotation
                .check()
                .map_err(|e| invalid(format!("{key}[{i}]: {e}")))?;
            if annotations.iter().any(|m| m.name == annotation.name) {
                return Err(invalid(format!(
                    "{key}[{i}]: annotation {} is listed more than once",
                    annotation.name
                )));
            }
            // Each datum letter on one label, as every edit keeps it.
            if let Some(letter) = &annotation.letter
                && let Some(other) = annotations
                    .iter()
                    .find(|m| m.letter.as_ref() == Some(letter))
            {
                return Err(invalid(format!(
                    "{key}[{i}]: datum {letter} is lettered on {} and on {}",
                    other.name, annotation.name
                )));
            }
            annotations.push(annotation);
        }
    }
    // A frame under no dimension (one deleted or renamed by an older
    // version) stands on its leader.
    crate::document::settle_frames(&mut annotations);
    // The six colours new annotations took in turn before mitcad#894 were
    // the program's, not the user's: read as Auto.
    crate::document::read_old_defaults(&mut annotations);
    let annotation_text_height = match top.remove("annotation_text_height") {
        None => None,
        Some(value) => match value.as_f64() {
            Some(h) if crate::document::check_text_height(h).is_ok() => Some(h),
            _ => {
                return Err(schema(
                    "\"annotation_text_height\" must be a height from 0.1 to 1000 mm",
                ));
            }
        },
    };
    // Comments (mitcad#1119): each as a command could have made it, each id
    // once; the next id one past the largest ever used. Their targets are
    // found when they are asked for, so one whose target is gone loads.
    let mut file_comments: Vec<Comment> = Vec::new();
    for (i, c) in array(&mut top, "comments", false)?.into_iter().enumerate() {
        let comment = serde_json::from_value::<Comment>(c)
            .map_err(|e| schema(format!("comments[{i}]: {e}")))?;
        comment
            .check()
            .map_err(|e| invalid(format!("comments[{i}] ({}): {e}", comment.name())))?;
        if file_comments.iter().any(|c| c.id == comment.id) {
            return Err(invalid(format!(
                "comments[{i}]: comment {} is listed more than once",
                comment.name()
            )));
        }
        file_comments.push(comment);
    }
    let next_comment = match top.remove("next_comment") {
        None => 1,
        Some(value) => value
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n >= 1)
            .ok_or_else(|| schema("\"next_comment\" must be a number from 1"))?,
    };
    let next_comment = file_comments
        .iter()
        .map(|c| c.id.saturating_add(1))
        .fold(next_comment, u32::max);
    // Purge volumes set by hand (mitcad#705): each valid, each pair once. A
    // pair of a material the design no longer has is kept as it is.
    let purge_volumes = array(&mut top, "purge_volumes", false)?
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            let pair = serde_json::from_value::<crate::print_purge::PurgeOverride>(p)
                .map_err(|e| schema(format!("purge_volumes[{i}]: {e}")))?;
            pair.check()
                .map_err(|e| invalid(format!("purge_volumes[{i}]: {e}")))?;
            Ok(pair)
        })
        .collect::<Result<Vec<_>, FileError>>()?;
    for (i, pair) in purge_volumes.iter().enumerate() {
        if purge_volumes[..i]
            .iter()
            .any(|p| p.from == pair.from && p.to == pair.to)
        {
            return Err(invalid(format!(
                "purge_volumes[{i}]: from {} to {} is listed more than once",
                pair.from, pair.to
            )));
        }
    }
    // The print plates (mitcad#1050): each valid, each id and name once.
    // An item whose body or occurrence is not in the design is kept.
    let print_plates = array(&mut top, "print_plates", false)?
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            let plate = serde_json::from_value::<crate::print_plates::PrintPlate>(p)
                .map_err(|e| schema(format!("print_plates[{i}]: {e}")))?;
            plate
                .check()
                .map_err(|e| invalid(format!("print_plates[{i}]: {e}")))?;
            Ok(plate)
        })
        .collect::<Result<Vec<_>, FileError>>()?;
    for (i, plate) in print_plates.iter().enumerate() {
        if print_plates[..i]
            .iter()
            .any(|p| p.id == plate.id || p.name.trim().eq_ignore_ascii_case(plate.name.trim()))
        {
            return Err(invalid(format!(
                "print_plates[{i}]: plate {} is listed more than once",
                plate.name
            )));
        }
    }
    // Filament changes at print heights (mitcad#1596): each schedule
    // valid, one per plate, of a plate the design has. A material the
    // design no longer has is kept (the export says so).
    let filament_changes = array(&mut top, "filament_changes", false)?
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let schedule = serde_json::from_value::<crate::print_changes::ChangeSchedule>(s)
                .map_err(|e| schema(format!("filament_changes[{i}]: {e}")))?;
            schedule
                .check()
                .map_err(|e| invalid(format!("filament_changes[{i}]: {e}")))?;
            Ok(schedule)
        })
        .collect::<Result<Vec<_>, FileError>>()?;
    for (i, schedule) in filament_changes.iter().enumerate() {
        if filament_changes[..i]
            .iter()
            .any(|s| s.plate == schedule.plate)
        {
            return Err(invalid(format!(
                "filament_changes[{i}]: the schedule of {} is listed more than once",
                schedule.plate.as_deref().unwrap_or("the design")
            )));
        }
        if let Some(plate) = schedule.plate.as_deref()
            && !print_plates.iter().any(|p| p.id == plate)
        {
            return Err(invalid(format!(
                "filament_changes[{i}]: there is no print plate {plate}"
            )));
        }
    }
    // CAM (mitcad#1013): read tolerantly, so that fields a newer MitCad
    // adds inside the section are ignored; each machine and setup checked.
    // An input whose body or sketch is gone is kept (its toolpath says so).
    let cam = match top.remove("cam") {
        None | Some(Value::Null) => crate::cam::CamState::default(),
        Some(value) => {
            let mut cam = serde_json::from_value::<crate::cam::CamState>(value)
                .map_err(|e| schema(format!("cam: {e}")))?;
            cam.check().map_err(|e| invalid(format!("cam: {e}")))?;
            cam
        }
    };
    // Drawings (mitcad#707): definitions only; their views are projected
    // when they are asked for, so one whose subject is gone loads and says
    // why.
    let drawings = array(&mut top, "drawings", false)?
        .into_iter()
        .enumerate()
        .map(|(i, d)| {
            let drawing = serde_json::from_value::<crate::drawing::def::Drawing>(d)
                .map_err(|e| schema(format!("drawings[{i}]: {e}")))?;
            drawing
                .check()
                .map_err(|e| invalid(format!("drawings[{i}]: {e}")))?;
            Ok(drawing)
        })
        .collect::<Result<Vec<_>, FileError>>()?;
    for (i, drawing) in drawings.iter().enumerate() {
        if drawings[..i].iter().any(|d| d.uid == drawing.uid) {
            return Err(invalid(format!(
                "drawings[{i}]: drawing {} is listed more than once",
                drawing.uid
            )));
        }
        if drawings[..i]
            .iter()
            .any(|d| d.component == drawing.component && d.name == drawing.name)
        {
            return Err(invalid(format!(
                "drawings[{i}]: two drawings of {} are named {}",
                drawing.component, drawing.name
            )));
        }
    }
    // Image references (mitcad#782): their planes are resolved when they
    // are asked for, so one whose plane is gone loads and says why.
    let image_references = array(&mut top, "image_references", false)?
        .into_iter()
        .enumerate()
        .map(|(i, value)| {
            let image = serde_json::from_value::<ImageReference>(value)
                .map_err(|e| schema(format!("image_references[{i}]: {e}")))?;
            image
                .check_image()
                .map_err(|e| invalid(format!("image_references[{i}]: {e}")))?;
            Ok(image)
        })
        .collect::<Result<Vec<_>, FileError>>()?;
    for (i, image) in image_references.iter().enumerate() {
        if image_references[..i].iter().any(|o| o.name == image.name) {
            return Err(invalid(format!(
                "image_references[{i}]: image reference {} is listed more than once",
                image.name
            )));
        }
    }
    let marker = match top.remove("marker") {
        None => None,
        Some(value) => Some(
            value
                .as_u64()
                .and_then(|m| usize::try_from(m).ok())
                .ok_or_else(|| schema("\"marker\" must be a feature count"))?,
        ),
    };
    // The number the next feature takes, kept where the largest was
    // deleted (mitcad#1320); older files have none.
    let next_feature: u64 = match top.remove("next_feature") {
        None => 1,
        Some(value) => value
            .as_u64()
            .filter(|n| *n >= 1)
            .ok_or_else(|| schema("\"next_feature\" must be a number from 1"))?,
    };
    // Tables (mitcad#667), before the parameters that look values up in
    // them.
    let mut tables = Vec::new();
    for (i, table) in array(&mut top, "tables", false)?.into_iter().enumerate() {
        let table = serde_json::from_value::<crate::tables::Table>(table)
            .map_err(|e| schema(format!("tables[{i}]: {e}")))?;
        table
            .check()
            .map_err(|e| invalid(format!("tables[{i}]: {e}")))?;
        tables.push(table);
    }
    let tables =
        crate::tables::Tables::from_list(tables).map_err(|e| invalid(format!("tables: {e}")))?;

    let mut state = DocState {
        tables,
        ..DocState::default()
    };
    let mut warnings = Vec::new();
    let renamed = load_parameters(&mut state, context, &parameters, &features, &mut warnings)?;

    // Uids first, so references can tell a missing feature from a later
    // one. Names may repeat (mitcad#1107): references go by uid.
    let mut uids = HashSet::new();
    for (i, f) in features.iter().enumerate() {
        if !uids.insert(f.uid) {
            return Err(invalid(format!(
                "features[{i}] ({}): uid {} is used more than once",
                f.name, f.uid
            )));
        }
        if f.name.trim().is_empty() {
            return Err(invalid(format!("features[{i}]: the name is empty")));
        }
    }
    let mut entries = Vec::with_capacity(features.len());
    for (i, f) in features.iter().enumerate() {
        let params = &state.parameters;
        let mut def = f
            .def
            .map_params(&mut |slot, name: &String| {
                let actual = renamed.get(name).unwrap_or(name);
                params
                    .find(actual)
                    .ok_or_else(|| format!("{slot}: parameter {name} does not exist"))
            })
            .map_err(|e| invalid(format!("features[{i}] ({}): {e}", f.name)))?;
        // A growing helix of a file written before mitcad#83 is FreeCAD's.
        if let FeatureDef::Helix(helix) = &mut def {
            helix.settle_construction(HelixConstruction::Freecad);
        }
        // A modelled thread of a file written before mitcad#1300 keeps its
        // flat ends, so that what refers to them keeps working.
        if let FeatureDef::Thread(thread) = &mut def {
            thread.settle_ends(crate::features::thread::ThreadEnds::Flat);
        }
        // So does a hole's modelled thread (mitcad#1307): a countersink
        // above it keeps its lip.
        if let FeatureDef::Hole(hole) = &mut def {
            hole.settle_thread_ends(crate::features::thread::ThreadEnds::Flat);
        }
        if let FeatureDef::SheetFace(face) = &mut def {
            face.settle(None);
        }
        crate::features::sheet_folded::settle(&mut def, None);
        if let (Some(shapes), FeatureDef::Sketch(sketch)) = (&f.shapes, &mut def)
            && let Ok(shapes) = serde_json::from_value::<Vec<LegacyShape<String>>>(shapes.clone())
        {
            let size = |name: &String| {
                params
                    .find(renamed.get(name).unwrap_or(name))
                    .and_then(|id| params.value(id))
            };
            let exact = convert(shapes, &size);
            for (entity, placed) in sketch.entities.iter_mut().zip(exact.entities) {
                entity.kind = placed.kind;
            }
            // Values that do not solve load and fail at recompute.
            let _ = settle(sketch, params);
        }
        entries.push(Arc::new(FeatureEntry {
            uid: f.uid,
            name: f.name.clone(),
            suppressed: f.suppressed,
            component: f.component,
            def,
        }));
        if let Some(visible) = f.visible {
            state.feature_visibility.insert(f.uid, visible);
        }
        // Only a construction plane's or axis's display size is kept
        // (mitcad#1748); one on another feature is left out.
        if let Some(size) = f.display_size
            && crate::document::has_display_size(&entries.last().expect("pushed above").def)
        {
            state.datum_display_sizes.insert(f.uid, size);
        }
        if let Some(text) = &f.suppressed_when {
            state.bindings.suppressed_when.insert(f.uid, text.clone());
        }
        if let Some(note) = &f.import_note {
            state.import_notes.insert(f.uid, note.clone());
        }
        if !f.split_pieces.is_empty() {
            state
                .split_pieces
                .insert(f.uid, Arc::new(f.split_pieces.clone()));
        }
    }
    let feature_uids: BTreeSet<FeatureUid> = uids.iter().copied().collect();
    assembly.check(&feature_uids).map_err(invalid)?;
    // Placements a little off a rotation, straightened (mitcad#1664).
    for uid in straightened {
        warnings.push(format!(
            "the placement of {} was distorted and has been straightened \
             to a rotation and a translation",
            assembly.occurrence_name(uid)
        ));
    }
    // (The features by uid: a search per check made loading a large
    // design quadratic in its features, mitcad#841.)
    let index = crate::feature_index::FeatureIndex::default();
    for (i, entry) in entries.iter().enumerate() {
        let ctx = CheckContext {
            uid: entry.uid,
            earlier: &entries[..i],
            all: &entries,
            index: &index,
        };
        entry
            .def
            .info()
            .check(&ctx)
            .and_then(|()| crate::joints::check_generated_references(&assembly, &ctx, &entry.def))
            .and_then(|()| match &entry.def {
                FeatureDef::Extrude(extrude) => {
                    extrude.active_cut_scope().map_or(Ok(()), |scope| {
                        scope.check_components(&assembly, &ctx, entry.component)
                    })
                }
                _ => Ok(()),
            })
            .and_then(|()| check_components(&assembly, &entries, &index, entry))
            .map_err(|e| invalid(format!("features[{i}] ({}): {e}", entry.name)))?;
    }
    // Derived parts are made from the file's sources (mitcad#375).
    for (i, entry) in entries.iter().enumerate() {
        if let FeatureDef::DerivedPart(part) = &entry.def
            && library_sources.get(part.source).is_none()
        {
            return Err(invalid(format!(
                "features[{i}] ({}): library source {} is not in the file",
                entry.name, part.source
            )));
        }
    }
    state.library_sources = library_sources;
    state.features = entries;
    state.assembly = assembly;

    for (i, p) in parameters.iter().enumerate() {
        if let Some(owner) = p.owner.filter(|o| !uids.contains(o)) {
            return Err(invalid(format!(
                "parameters[{i}]: the owner {owner} is not a feature of the file"
            )));
        }
    }
    for (i, body) in bodies.into_iter().enumerate() {
        if !uids.contains(&body.uid.feature) {
            return Err(invalid(format!(
                "bodies[{i}]: feature {} does not exist",
                body.uid.feature
            )));
        }
        if body.name.trim().is_empty() {
            return Err(invalid(format!("bodies[{i}]: the name is empty")));
        }
        if state.body_names.insert(body.uid, body.name).is_some() {
            return Err(invalid(format!(
                "bodies[{i}]: body {} is listed more than once",
                body.uid
            )));
        }
        if let Some(id) = body.material.as_deref().filter(|id| !known_material(id)) {
            return Err(invalid(format!("bodies[{i}]: unknown material {id}")));
        }
        if let Some(face) = body
            .face_appearances
            .keys()
            .find(|face| face.parse::<crate::topo::FaceName>().is_err())
        {
            return Err(invalid(format!(
                "bodies[{i}]: {face} is no face name in face_appearances"
            )));
        }
        if let Some(face) = body
            .face_appearance_exprs
            .keys()
            .find(|face| face.parse::<crate::topo::FaceName>().is_err())
        {
            return Err(invalid(format!(
                "bodies[{i}]: {face} is no face name in face_appearance_exprs"
            )));
        }
        if let Some(face) = body
            .print_bed_face
            .as_deref()
            .filter(|face| face.parse::<crate::topo::FaceName>().is_err())
        {
            return Err(invalid(format!(
                "bodies[{i}]: {face} is no face name in print_bed_face"
            )));
        }
        let attributes = BodyAttributes {
            visible: body.visible,
            material: body.material,
            appearance: body.appearance,
            face_appearances: body.face_appearances,
            print_bed_face: body.print_bed_face,
            appearance_expr: body.appearance_expr.filter(|t| !t.trim().is_empty()),
            material_expr: body.material_expr.filter(|t| !t.trim().is_empty()),
            visible_when: body.visible_when.filter(|t| !t.trim().is_empty()),
            face_appearance_exprs: body.face_appearance_exprs,
            transparent: body.transparent,
        };
        if !attributes.is_default() {
            state.body_attributes.insert(body.uid, attributes);
        }
    }
    state.marker = match marker {
        Some(m) if m > state.features.len() => {
            return Err(invalid(format!(
                "the marker {m} is past the end of the timeline ({} features)",
                state.features.len()
            )));
        }
        Some(m) => m,
        None => state.features.len(),
    };
    // One past the largest uid listed, or the file's `next_feature` when it
    // is past that (mitcad#1320): a deleted last feature's uid stays unused.
    state.next_uid = state
        .features
        .iter()
        .map(|f| f.uid.0 + 1)
        .max()
        .unwrap_or(1)
        .max(next_feature);
    for (i, view) in views.iter().enumerate() {
        if views[..i].iter().any(|v| v.name == view.name) {
            return Err(invalid(format!(
                "views[{i}]: named view {} is listed more than once",
                view.name
            )));
        }
    }
    state.named_views = views;
    state.display = display;
    for (i, group) in groups.iter().enumerate() {
        if let Some(missing) = group
            .features
            .iter()
            .find(|uid| state.position(**uid).is_none())
        {
            return Err(invalid(format!(
                "groups[{i}] ({}): feature {missing} does not exist",
                group.name
            )));
        }
    }
    state.groups = groups;
    state.normalize_groups();
    // Analyses (mitcad#41): their references are resolved when they are
    // asked for, so one whose plane is gone loads and says why.
    for (i, analysis) in analyses.iter().enumerate() {
        if analyses[..i].iter().any(|a| a.name == analysis.name) {
            return Err(invalid(format!(
                "analyses[{i}]: analysis {} is listed more than once",
                analysis.name
            )));
        }
    }
    state.analyses = analyses;
    // Motion studies and contact sets (mitcad#454): names once each, keys
    // of joints' free motions, occurrences where their paths say.
    for (i, study) in motion_studies.iter().enumerate() {
        if motion_studies[..i].iter().any(|s| s.name == study.name) {
            return Err(invalid(format!(
                "motion_studies[{i}]: motion study {} is listed more than once",
                study.name
            )));
        }
        for key in &study.keys {
            let motions = state
                .entry(key.joint)
                .and_then(|e| crate::document::joint_motions(&e.def));
            if !motions.is_some_and(|m| m.contains(&key.motion)) {
                return Err(invalid(format!(
                    "motion_studies[{i}] ({}): {} has no free motion {}",
                    study.name, key.joint, key.motion
                )));
            }
        }
    }
    for (i, set) in contact_sets.iter().enumerate() {
        if contact_sets[..i].iter().any(|s| s.name == set.name) {
            return Err(invalid(format!(
                "contact_sets[{i}]: contact set {} is listed more than once",
                set.name
            )));
        }
        for path in &set.occurrences {
            crate::joints::path_component(&state.assembly, ComponentUid::ROOT, &path.0)
                .map_err(|e| invalid(format!("contact_sets[{i}] ({}): {e}", set.name)))?;
        }
    }
    state.motion_studies = motion_studies;
    state.contact_sets = contact_sets;
    state.properties = properties;
    // Appearances (mitcad#46): ids of their own, not the library's; one the
    // library took only later (the filament colours) gets a free id at the
    // end (`free_library_appearance_ids`, mitcad#1133).
    for (i, appearance) in appearances.iter().enumerate() {
        let id = &appearance.id;
        if !crate::appearance::valid_id(id) {
            return Err(invalid(format!(
                "appearances[{i}]: {id} is no appearance id"
            )));
        }
        if crate::appearance::library_appearance(id).is_some()
            && !crate::appearance::library_id_taken_later(id)
        {
            return Err(invalid(format!(
                "appearances[{i}]: {id} is a library appearance's id"
            )));
        }
        if appearances[..i].iter().any(|a| a.id == *id) {
            return Err(invalid(format!(
                "appearances[{i}]: appearance {id} is listed more than once"
            )));
        }
    }
    state.appearances = appearances;
    // Materials (mitcad#640): each id once; components' defaults of
    // components the file has, by materials it knows.
    for (i, material) in materials.iter().enumerate() {
        if materials[..i].iter().any(|m| m.id == material.id) {
            return Err(invalid(format!(
                "materials[{i}]: material {} is listed more than once",
                material.id
            )));
        }
    }
    for (i, c) in component_materials.into_iter().enumerate() {
        if !c.component.is_root() && state.assembly.component(c.component).is_none() {
            return Err(invalid(format!(
                "component_materials[{i}]: there is no component {}",
                c.component
            )));
        }
        if !known_material(&c.material) {
            return Err(invalid(format!(
                "component_materials[{i}]: unknown material {}",
                c.material
            )));
        }
        // An appearance the design and the library do not have (a file
        // edited by hand) is dropped with a warning, as setting it refuses
        // it: the component shows its material's appearance.
        if let Some(look) = c.appearance {
            if state.look(&look).is_some() {
                state.component_appearances.insert(c.component, look);
            } else {
                warnings.push(format!(
                    "component_materials[{i}]: there is no appearance {look}; the component \
                     shows its material's"
                ));
            }
        }
        if state
            .component_materials
            .insert(c.component, c.material)
            .is_some()
        {
            return Err(invalid(format!(
                "component_materials[{i}]: component {} is listed more than once",
                c.component
            )));
        }
    }
    state.materials = materials;
    // A material's own filament colour from before (mitcad#1055): an
    // appearance of its list.
    let coloured: Vec<usize> = state
        .materials
        .iter()
        .enumerate()
        .filter(|(_, m)| m.filament.contains_key("filament_colour"))
        .map(|(i, _)| i)
        .collect();
    for i in coloured {
        let mut material = state.materials[i].clone();
        state.adopt_filament_colour(&mut material, false);
        state.materials[i] = material;
    }
    state.print_profiles = print_profiles;
    state.annotations = annotations;
    state.annotation_text_height = annotation_text_height;
    state.comments = file_comments;
    state.next_comment = next_comment;
    state.purge_volumes = purge_volumes;
    state.print_plates = print_plates;
    state.filament_changes = filament_changes;
    state.cam = cam;
    for (i, drawing) in drawings.iter().enumerate() {
        if !state.assembly.exists(drawing.component) {
            return Err(invalid(format!(
                "drawings[{i}]: there is no component {}",
                drawing.component
            )));
        }
    }
    state.drawings = drawings;
    state.image_references = image_references;
    state.render = render;
    // The configuration table (mitcad#64): rows of the file's parameters,
    // under the names they were loaded with.
    let mut configurations = configurations;
    if !renamed.is_empty() {
        for name in configurations
            .parameters
            .iter_mut()
            .chain(&mut configurations.options)
        {
            if let Some(new) = renamed.get(name) {
                new.clone_into(name);
            }
        }
        // The rules' conditions, options and expressions (mitcad#799).
        for rule in &mut configurations.rules {
            if let Some(when) = rule.when.as_mut() {
                *when = rename_names(when, &renamed);
            }
            for action in rule.then.iter_mut().chain(&mut rule.otherwise) {
                if let Some(option) = action.option.as_mut()
                    && let Some(new) = renamed.get(option.as_str())
                {
                    new.clone_into(option);
                }
                // The options of an `in_table` action (mitcad#806).
                if let Some(columns) = action.columns.as_mut() {
                    *columns = std::mem::take(columns)
                        .into_iter()
                        .map(|(option, column)| {
                            (renamed.get(&option).cloned().unwrap_or(option), column)
                        })
                        .collect();
                }
                for list in [&mut action.values, &mut action.exclude]
                    .into_iter()
                    .flatten()
                {
                    for text in list.iter_mut() {
                        *text = rename_names(text, &renamed);
                    }
                }
                for text in [
                    &mut action.min,
                    &mut action.max,
                    &mut action.step,
                    &mut action.require,
                ]
                .into_iter()
                .flatten()
                {
                    *text = rename_names(text, &renamed);
                }
            }
        }
        for row in &mut configurations.rows {
            row.values = std::mem::take(&mut row.values)
                .into_iter()
                .map(|(name, expression)| {
                    let name = renamed.get(&name).cloned().unwrap_or(name);
                    (name, rename_names(&expression, &renamed))
                })
                .collect();
        }
    }
    // A table of the old form (selectors, before mitcad#798) opens as a
    // closed table whose options are new text parameters.
    let (_, migrated) = crate::document::configurations::migrate(
        &mut configurations,
        &mut state.parameters,
        &Configurations::default(),
    )
    .map_err(|e| invalid(format!("configurations: {e}")))?;
    warnings.extend(migrated);
    if let Some(problem) = configurations.problems(&state.parameters).first() {
        return Err(invalid(format!("configurations: {problem}")));
    }
    state.configurations = configurations;
    state.hybrid_assembly = hybrid_assembly;
    state.join_tolerance = join_tolerance;
    // Flat Patterns (mitcad#208): evaluated when asked for, so one whose
    // body or panel is gone loads and says why.
    for (i, f) in flat_patterns.into_iter().enumerate() {
        let def = crate::sheet::flat::FlatPatternDef {
            stationary: f.stationary,
            x_axis: f.x_axis,
            reference_side: f
                .reference_side
                .unwrap_or(crate::sheet::SheetReferenceSide::Front),
            shown: f.shown,
        };
        if state.flat_patterns.insert(f.body, def).is_some() {
            return Err(invalid(format!(
                "flat_patterns[{i}]: body {} is listed more than once",
                f.body
            )));
        }
    }
    // One shown at a time: the first shown one.
    let mut shown = false;
    for analysis in &mut state.analyses {
        analysis.visible = analysis.visible && !shown;
        shown = shown || analysis.visible;
    }
    settle_unitless_slots(&mut state, &mut warnings);
    // Bindings (mitcad#800): those of occurrences the file has, and every
    // bound value as its expression gives it with the file's parameters.
    state.bindings.occurrence_visible_when = occurrence_visible_when;
    if !renamed.is_empty() {
        state.edit_binding_texts(|text| *text = rename_names(text, &renamed));
    }
    free_library_appearance_ids(&mut state, &mut warnings);
    state.settle_bindings();
    Ok((state, warnings))
}

/// The design's own appearances whose ids the library took only later
/// (mitcad#1133: the `filament_*` colours,
/// `appearance::library_id_taken_later`; another library id is refused
/// above) keep their values under a free id, the first of `<id>_2`, `<id>_3`, …
/// that no appearance and no reference uses, and under a free name when
/// the library's has theirs (`<name> (2)`). Every reference follows: the
/// bodies' and faces' appearances, the materials' default and list, the
/// components' appearances and the bindings' quoted texts (`'filament_red'`).
/// One warning per renamed appearance. A parameter's text that spells the
/// old id is a value of its own and names the library's from now on.
fn free_library_appearance_ids(state: &mut DocState, warnings: &mut Vec<String>) {
    use crate::appearance::{library, library_id_taken_later};
    let clashing: Vec<String> = state
        .appearances
        .iter()
        .filter(|a| library_id_taken_later(&a.id))
        .map(|a| a.id.clone())
        .collect();
    if clashing.is_empty() {
        return;
    }
    let mut ids: BTreeSet<String> = library()
        .iter()
        .chain(&state.appearances)
        .map(|a| a.id.clone())
        .collect();
    ids.extend(state.body_attributes.values().flat_map(|attributes| {
        attributes
            .appearance
            .iter()
            .chain(attributes.face_appearances.values())
            .cloned()
    }));
    ids.extend(state.materials.iter().flat_map(|m| m.appearance_list()));
    ids.extend(state.component_appearances.values().cloned());
    let mut names: BTreeSet<String> = library()
        .iter()
        .chain(&state.appearances)
        .map(|a| a.name.clone())
        .collect();
    for old in clashing {
        let new = (2..)
            .map(|n| format!("{old}_{n}"))
            .find(|id| ids.insert(id.clone()))
            .expect("an unused appearance id exists");
        let follow = |id: &mut String| {
            if *id == old {
                new.clone_into(id);
            }
        };
        for appearance in &mut state.appearances {
            if appearance.id != old {
                continue;
            }
            appearance.id.clone_from(&new);
            if library().iter().any(|a| a.name == appearance.name) {
                let name = appearance.name.clone();
                appearance.name = (2..)
                    .map(|n| format!("{name} ({n})"))
                    .find(|candidate| names.insert(candidate.clone()))
                    .expect("an unused appearance name exists");
            }
        }
        for attributes in state.body_attributes.values_mut() {
            attributes.appearance.iter_mut().for_each(follow);
            attributes.face_appearances.values_mut().for_each(follow);
        }
        for material in &mut state.materials {
            material.appearance.iter_mut().for_each(follow);
            material.appearances.iter_mut().for_each(follow);
        }
        state.component_appearances.values_mut().for_each(follow);
        let quoted: Vec<(String, String)> = ['\'', '"']
            .into_iter()
            .map(|q| (format!("{q}{old}{q}"), format!("{q}{new}{q}")))
            .collect();
        state.edit_binding_texts(|text| {
            for (from, to) in &quoted {
                if text.contains(from.as_str()) {
                    *text = text.replace(from.as_str(), to);
                }
            }
        });
        warnings.push(format!(
            "appearance {old} of this design is now {new}: the library has an appearance {old}"
        ));
    }
}

/// Count, factor and fraction slots are unitless (mitcad#662). A file from
/// before keeps their parameters in a length unit, whose value in
/// millimetres each slot read as its number. A slot's own parameter that
/// only unitless slots of its feature use, that no other parameter refers
/// to and that no configuration sets becomes unitless with that number:
/// its expression stays when it gives the same number without a unit,
/// else the number replaces it (with a warning when the expression named
/// parameters). Any other parameter keeps its unit with a warning, and the
/// slot reads its number as before.
fn settle_unitless_slots(state: &mut DocState, warnings: &mut Vec<String>) {
    // Each parameter's uses: the feature, its name, the slot and whether
    // the slot is unitless.
    let mut uses: BTreeMap<ParamId, Vec<(FeatureUid, String, String, bool)>> = BTreeMap::new();
    for entry in &state.features {
        let _ = entry.def.map_params(&mut |slot, id: &ParamId| {
            uses.entry(*id).or_default().push((
                entry.uid,
                entry.name.clone(),
                slot.to_owned(),
                slot_unit(slot).is_unitless(),
            ));
            Ok::<_, ()>(())
        });
    }
    for (id, uses) in uses {
        let Some((_, feature, slot, _)) = uses.iter().find(|u| u.3) else {
            continue;
        };
        let Some(param) = state.parameters.get(id) else {
            continue;
        };
        if param.unit().is_unitless() {
            continue;
        }
        let name = param.name().to_owned();
        let value = param.value();
        let owner = state.parameters.owner(id);
        let own = uses
            .iter()
            .all(|(uid, _, _, unitless)| *unitless && Some(*uid) == owner);
        if !own
            || !state.parameters.table().dependents(id).is_empty()
            || state.configurations.parameters.contains(&name)
        {
            warnings.push(format!(
                "{feature} {slot}: parameter {name} is {} ({}) in a slot that takes a \
                 unitless number; the slot reads it as {}: give the parameter no unit, or use \
                 another one",
                describe(param.unit()),
                param.formatted_value(DEFAULT_DECIMALS),
                short_number(value)
            ));
            continue;
        }
        let expression = param.expression().to_owned();
        let references = !param.expr().is_constant();
        let same = state
            .parameters
            .table()
            .evaluate(&expression, Unit::NONE)
            .is_ok_and(|q| (q.value - value).abs() <= 1e-9 * value.abs().max(1.0));
        let number = short_number(value);
        let new = if same { &expression } else { &number };
        // Limits in a length unit (mitcad#665) keep it a length.
        if let Err(error) = state.parameters.update(id, new, Unit::NONE) {
            warnings.push(format!(
                "{feature} {slot}: parameter {name} keeps its unit in a slot that takes a \
                 unitless number ({error}); the slot reads it as {number}"
            ));
            continue;
        }
        if !same && references {
            warnings.push(format!(
                "{feature} {slot}: parameter {name} is now unitless with the number it gave, \
                 {number}; its expression “{expression}” gives another number without a unit"
            ));
        }
    }
}

/// The shortest decimal within 1e-12 (relative) of `value`: a count saved
/// in inches as `0.11811023622047245 in` is 3, not 3.0000000000000004.
fn short_number(value: f64) -> String {
    (0..16)
        .map(|decimals| format_number(value, decimals))
        .find(|text| {
            text.parse::<f64>()
                .is_ok_and(|v| (v - value).abs() <= 1e-12 * value.abs().max(1.0))
        })
        .unwrap_or_else(|| value_to_expression(value, Unit::NONE))
}

/// The default units of a file.
fn units_context(units: &UnitsIn) -> Result<EvalContext, FileError> {
    let mut context = default_context();
    if let Some(text) = &units.length {
        context.default_length_unit = Unit::parse(text)
            .ok()
            .and_then(|u| match (u.length(), u.angle()) {
                (Some((unit, 1)), None) => Some(unit),
                _ => None,
            })
            .ok_or_else(|| invalid(format!("units: “{text}” is not a length unit")))?;
    }
    if let Some(text) = &units.angle {
        context.default_angle_unit = Unit::parse(text)
            .ok()
            .and_then(|u| match (u.length(), u.angle()) {
                (None, Some((unit, 1))) => Some(unit),
                _ => None,
            })
            .ok_or_else(|| invalid(format!("units: “{text}” is not an angle unit")))?;
    }
    Ok(context)
}

/// Builds the parameters. Files from before expressions store values: they
/// become expressions in millimetres, or degrees when a feature uses the
/// parameter as an angle, and names that are now reserved (`m`, `E`, `in`,
/// ...) are renamed with a warning. Returns the renames.
fn load_parameters(
    state: &mut DocState,
    context: EvalContext,
    parameters: &[ParameterIn],
    features: &[FeatureIn],
    warnings: &mut Vec<String>,
) -> Result<HashMap<String, String>, FileError> {
    let mut angles = HashSet::new();
    for f in features {
        let _ = f.def.map_params(&mut |slot, name: &String| {
            if slot_unit(slot) == Unit::DEG {
                angles.insert(name.clone());
            }
            Ok::<_, ()>(())
        });
    }
    let mut taken: HashSet<String> = parameters.iter().map(|p| p.name.clone()).collect();
    let mut renamed = HashMap::new();
    let mut seen = HashSet::new();
    let mut specs = Vec::with_capacity(parameters.len());
    for (i, p) in parameters.iter().enumerate() {
        let at = |message: String| invalid(format!("parameters[{i}]: {message}"));
        if p.name.is_empty() {
            return Err(at("the name is empty".to_owned()));
        }
        if !seen.insert(p.name.as_str()) {
            return Err(at(format!("parameter {} already exists", p.name)));
        }
        // A name that is now reserved: in files from before expressions a
        // unit, function or constant (`m`, `E`); with an expression only a
        // name reserved since mitcad#663 (`Max`, `pi`, `sum`), as any other
        // reserved name (`mm`, `sin`) was never valid there and is refused.
        let renamable = p.expression.is_none() || is_newly_reserved(&p.name);
        let legacy = if renamable {
            legacy_name(&p.name, &taken)
        } else {
            None
        };
        let name = match legacy {
            Some(new) => {
                warnings.push(format!(
                    "parameter {} is renamed to {new}: {} is a unit, function \
                     or constant in expressions",
                    p.name, p.name
                ));
                taken.insert(new.clone());
                renamed.insert(p.name.clone(), new.clone());
                new
            }
            None => p.name.clone(),
        };
        let spec = match (&p.expression, p.value) {
            (Some(expression), _) => {
                let unit = Unit::parse(p.unit.as_deref().unwrap_or(""))
                    .map_err(|e| at(format!("unit: {e}")))?;
                ParamSpec::user(&name, expression, unit)
            }
            (None, Some(value)) => {
                if p.unit.is_some() {
                    return Err(at("a unit needs an expression".to_owned()));
                }
                let unit = if angles.contains(&p.name) {
                    Unit::DEG
                } else {
                    Unit::MM
                };
                ParamSpec::user(&name, &value_to_expression(value, unit), unit)
            }
            (None, None) => {
                return Err(schema(format!(
                    "parameters[{i}]: missing field `expression`"
                )));
            }
        };
        let expression_context = p
            .expression_units
            .as_ref()
            .map(units_context)
            .transpose()
            .map_err(|e| at(format!("expression_units: {e}")))?;
        // Limits (mitcad#665).
        let limits = ParamLimits {
            min: p.min.clone(),
            max: p.max.clone(),
            step: p.step.clone(),
            strict: p.strict,
            values: p.values.clone(),
        };
        specs.push((
            spec.with_comment(&p.comment)
                .with_expression_context(expression_context)
                .with_limits(limits),
            p.owner,
        ));
    }
    // The expressions use the new names.
    if !renamed.is_empty() {
        for (spec, _) in &mut specs {
            spec.expression = rename_names(&spec.expression, &renamed);
        }
    }
    let index_of = |name: &str| {
        parameters
            .iter()
            .position(|p| renamed.get(&p.name).unwrap_or(&p.name) == name)
    };
    // They look values up in the design's tables (mitcad#667).
    let tables = state.tables.source();
    state.parameters = Parameters::build_with_tables(context, specs, tables).map_err(|e| {
        let name = match &e {
            TableError::InvalidName(name)
            | TableError::ReservedName(name)
            | TableError::DuplicateName(name)
            | TableError::TableName(name) => Some(name.as_str()),
            TableError::Parse { parameter, .. } | TableError::Eval { parameter, .. } => {
                Some(parameter.as_str())
            }
            TableError::Limit { parameter, .. } => Some(parameter.as_str()),
            _ => None,
        };
        match name.and_then(index_of) {
            Some(i) => invalid(format!("parameters[{i}]: {e}")),
            None => invalid(format!("parameters: {e}")),
        }
    })?;
    // Favourites (P9).
    for p in parameters.iter().filter(|p| p.favorite) {
        if let Some(id) = state
            .parameters
            .find(renamed.get(&p.name).unwrap_or(&p.name))
        {
            state.favorites.insert(id);
        }
    }
    Ok(renamed)
}
