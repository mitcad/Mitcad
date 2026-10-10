// SPDX-License-Identifier: MIT
//! Class GUIDs of the design segment whose meaning is known (from the
//! timeline format study). GUIDs are stable across files and the
//! versions of their writer; they are compared in upper case.

pub const TIMELINE: &str = "2F4C1849-1A5A-4F6C-A086-8DD445CBF94B";
pub const FEATURE_MANAGER: &str = "C3423E04-7E85-4B44-8E31-A5FA9C100052";
pub const PARAMETER: &str = "7A6A3D31-BE74-4E19-9115-20642944C2E4";
pub const PARAMETER_LIST: &str = "B0556F58-EE09-46A7-B18F-E296F48E5B13";
pub const PARAMETER_HOLDER: &str = "D91D429C-1BB5-4640-B49F-69AF14F463E9";
pub const HEALTH: &str = "B7F34D7B-DAC4-41A5-B67D-D672537C3BFA";
pub const COMPONENT: &str = "E03784ED-5E19-4E14-B9F2-3B07017018CD";
pub const OCCURRENCE: &str = "CE2913AA-CFE0-4F04-9102-24424ED3BCFA";
/// A component's child occurrences.
pub const OCCURRENCE_CONTAINER: &str = "904F885E-D859-4755-A7EF-6EBED8732FAD";
/// Names a `BREP.<guid>.smb` blob (the GUID is written in this odd form).
pub const BREP_REF: &str = "40DCC3F41D-E94F-B3B1-3E95-9B6ABE572C";
/// A component's body blob holder: references the [`BREP_REF`] object and
/// the component (which references it back). Not in [`KNOWN`], whose
/// classes count as known bytes in the coverage table.
pub const BLOB_HOLDER: &str = "CD57BC48-50EC-47DC-975A-FB6DEA72F4DA";
pub const SKETCH: &str = "44A64366-4BD3-4B24-881A-F94C206E8F2D";
pub const SKETCH_FEATURE: &str = "8DA771B7-52ED-42FC-94E2-2353FE141373";
pub const SKETCH_TRANSFORM: &str = "F47A46FB-DA27-4AFA-8A72-1B38FA596E23";
pub const SKETCH_POINT: &str = "C2CEDAE7-1716-47C1-B7B1-07B70081D0FB";
pub const SKETCH_POINT_2: &str = "00D157C2-2332-4BD5-BB53-048C4081D1C3";
pub const SKETCH_LINE: &str = "DCA267ED-D615-4934-B64F-AD805E8003E2";
pub const SKETCH_LINE_2: &str = "16DEFC4D-1816-4FB0-8E39-9BDA23954248";
pub const SKETCH_LINE_3: &str = "AE42BAB6-643F-4169-A33C-529C8E0A4D84";
pub const SKETCH_ARC: &str = "F0130424-8B7E-4092-93C9-1CA807482534";
/// Base class of all sketch curves.
pub const SKETCH_CURVE: &str = "9FF71F46-205F-484B-A2B5-A245D0D8FE33";
pub const SKETCH_TEXT: &str = "E0618268-3A06-450E-9E94-7CF4C2E66802";
pub const SKETCH_CONSTRAINT: &str = "60403D47-0C49-49B0-BDE8-1679608164A2";
/// Base class of all sketch dimensions.
pub const SKETCH_DIMENSION: &str = "855F0A64-6286-4FFC-B918-BC394F600107";
pub const LINEAR_DIMENSION: &str = "8C780195-72C0-4A56-A911-E43AB14357F2";
pub const ANGULAR_DIMENSION: &str = "6EEAFDE5-59C4-424D-8EBE-77CDBC562017";
pub const RADIAL_DIMENSION: &str = "8EDAC24D-35D4-4591-B8E7-B3EA081C9DF0";
pub const DIAMETER_DIMENSION: &str = "664AD49F-207F-4CD5-8555-DD269E9F09EA";
pub const ORIGIN_PLANE: &str = "8D424E29-DE23-4B1F-A3AD-84C52A497D1A";
pub const ORIGIN_AXIS: &str = "19031004-3AB6-4D13-ABBF-C51EE214688D";
pub const ORIGIN_POINT: &str = "0E18EEF2-0D6F-480C-997E-5EBC7369AE5F";
/// Target of an entity reference (`u64` object id after its root part).
pub const REF_TARGET: &str = "90055C05-546C-4EE7-B3C9-3DD922AD0C9C";
/// Feature input: reference to a plane or axis.
pub const ENTITY_REF: &str = "5A1BF548-241F-46FD-9FB5-E4B05126EB9D";
/// Feature input: reference to faces or edges.
pub const FACE_REF: &str = "5662F619-2281-4AD9-93DC-1FCFDA1062E7";
/// Feature input: profile source (sketch object id as text).
pub const PROFILE_SOURCE: &str = "4BD53E5A-0B3E-45E5-AE8B-02044306485A";
/// Same layout as [`PROFILE_SOURCE`], for text profiles.
pub const PROFILE_SOURCE_2: &str = "92F9A5F4-09AA-4B1B-9EE6-DF574D1D183C";
pub const PROFILE: &str = "0897AF07-EF1B-42E5-BDD9-1C746A085CBC";
/// One selected profile of a [`PROFILE`] input.
pub const PROFILE_ID: &str = "C46D3EEB-40E1-43FA-83DC-9CB204335417";
pub const EXTRUDE: &str = "DD405BC2-D673-44F0-8833-5CB2A1C186C7";
pub const CONSTRUCTION_PLANE: &str = "D869265F-F339-4751-A066-91D30F81FF08";
pub const CONSTRUCTION_AXIS: &str = "803DEC43-AEA6-42CC-B8D0-F593B5124BDD";
/// An integer input of a feature (pattern quantities): root list
/// `[feature]`, then `u32 slot, u8 0, u8 1, u32 value` (T1b).
pub const INT_HOLDER: &str = "A085449A-5144-4B2B-B455-7F7035A40559";
/// A `Geometry` object, one per design, that a sketch refers to right
/// before its name and entity list (its own meaning is open); the sketch's
/// light bulb comes before that reference (mitcad#6). Not in [`KNOWN`].
pub const SKETCH_NAME_ANCHOR: &str = "5275CBA4-3D7D-40DC-A651-1DFF3FC8AFBF";
/// A sketch offset (mitcad#36): its curves, chains, dimension, constraint
/// and signed distance ([`super::sketch::offsets`]). Not in [`KNOWN`].
pub const OFFSET: &str = "AFC07C10-99AC-5304-AE1B-DBEE335B0E43";
/// One chain of a sketch offset: a root list of curves in chain order.
pub const OFFSET_CHAIN: &str = "04E598FF-2979-5E41-8BC0-5C435C810909";

/// Classes whose meaning is known, with the reference decoder's names.
pub const KNOWN: &[(&str, &str)] = &[
    (TIMELINE, "Timeline"),
    (FEATURE_MANAGER, "FeatureManager"),
    (PARAMETER, "ModelParameter"),
    (PARAMETER_LIST, "ParameterList"),
    ("E276745D-D84E-40D5-88BC-8C94DD4ACCDF", "ParameterNameMap"),
    (PARAMETER_HOLDER, "ParameterHolder"),
    (COMPONENT, "Component"),
    (OCCURRENCE, "Occurrence"),
    (SKETCH, "Sketch"),
    (SKETCH_POINT, "SketchPoint"),
    (SKETCH_LINE, "SketchLine"),
    (SKETCH_ARC, "SketchArc"),
    (SKETCH_CONSTRAINT, "SketchConstraint"),
    (LINEAR_DIMENSION, "SketchLinearDimension"),
    (ANGULAR_DIMENSION, "SketchAngularDimension"),
    (RADIAL_DIMENSION, "SketchRadialDimension"),
    (DIAMETER_DIMENSION, "SketchDimension(664AD49F)"),
    ("D3937028-C20C-4E65-B010-94AD418A5C20", "Body"),
    (BREP_REF, "BrepBlobRef"),
    (ORIGIN_PLANE, "OriginPlane"),
    (ORIGIN_AXIS, "OriginAxis"),
    (ORIGIN_POINT, "OriginPoint"),
    (SKETCH_FEATURE, "SketchFeature"),
    (EXTRUDE, "ExtrudeFeature"),
    ("E3849A15-2FC6-42A0-AF3A-2F1D7273B406", "RevolveFeature"),
    ("FCBB1707-4450-46B3-9E65-0F61682EA8CA", "SweepFeature"),
    ("73615646-8752-4EE6-AD02-0B90804B7CD7", "PipeFeature"),
    ("A07D5F17-68CB-464D-9935-BF68E98A865F", "FilletFeature"),
    ("F757D611-217B-4B72-9C41-B617BDCE43DA", "ChamferFeature"),
    ("1C037A07-4A15-43F6-ABFC-BBF61B9038D4", "HoleFeature"),
    ("584D9526-EF22-4F6F-9FE1-5FCF1F3CE575", "ThreadFeature"),
    ("2A94257F-2020-4B19-9A68-103A2672F1B7", "CombineFeature"),
    (
        "11F1A5CE-2B57-4476-8480-6994621493C9",
        "CircularPatternFeature",
    ),
    (
        "AE12AD3F-BB81-4B59-9D34-A599E58E4BF6",
        "RectangularPatternFeature",
    ),
    ("D1728651-3640-4CCF-8083-AF7703013978", "MirrorFeature"),
    (CONSTRUCTION_PLANE, "ConstructionPlane"),
    ("803DEC43-AEA6-42CC-B8D0-F593B5124BDD", "ConstructionAxis"),
    ("78B9DEFC-DA1C-4ED0-A395-40890BDAE590", "CreateComponent"),
    (
        "428B5C6E-8F2C-42D7-83B7-2C09BD91F83E",
        "PlaceComponentInstance",
    ),
    ("D7015B81-AB4D-4C89-BED7-AA4411E1964A", "BaseFeature"),
    ("65CA3206-E33A-471E-8E40-2C72B57C6181", "SplitBodyFeature"),
    ("64E40C8E-E564-469C-8690-5C4E77C1B02C", "MoveFeature"),
    ("4A782808-FBDD-4351-A10A-F7AE5693D330", "RemoveBodyFeature"),
    ("29EF07D3-7BAF-4FA1-A28A-43A36279EA6D", "DeleteFaceFeature"),
    ("D087EFE5-2D28-42E6-BB45-61739E7D0204", "OffsetFacesFeature"),
    ("21B67709-045F-4903-B3FA-FD82E3DC3ADA", "MoveFaceFeature"),
    ("E8063467-BC05-4688-8196-AECB90F0EFEE", "ShellFeature"),
    ("DB2E4EDC-ADC2-4220-8A0C-011B6FE41C4F", "CoilFeature"),
    ("02E8AA49-08B8-439C-A8C7-221BE20143D8", "JointOriginFeature"),
    ("8DFDD543-F823-43AA-BDAF-C638E023F0A0", "JointFeature"),
    (
        "AA8B3D40-B67D-4AF1-B3CF-E1E00279B5F0",
        "AsBuiltJointFeature",
    ),
    ("89FB64C8-E332-4EC4-8994-F4494B486865", "ContextFeature"),
    (
        "66E375EA-40F1-4115-8D83-0F23DD30593E",
        "DerivedInstanceFeature",
    ),
    ("D57B1A22-9D5A-4D7D-A909-E08C579612CA", "CopyPasteFeature"),
    (
        "D0F69AAA-7BA0-4C9C-8A6D-7D01ADB39593",
        "GeometricRelationshipFeature",
    ),
    ("2931E994-7A40-490D-A9E0-BE2A17EBF07D", "FastenerFeature"),
    (
        "BA3059E4-4B23-4D02-9D18-E7BB95C991F3",
        "CutPasteBodiesFeature",
    ),
    (
        "56225BC7-69B7-446A-9AEA-6AA25BC7806E",
        "ComponentFromBodiesFeature",
    ),
    ("8EE00B00-76BB-49AB-8C25-E837FEC5BDA5", "SnapshotFeature"),
    ("62E501B2-B398-4176-87AD-28F56269D7C9", "Feature(62E501B2)"),
];

/// The reference decoder's name of a known class.
pub fn known_name(guid: &str) -> Option<&'static str> {
    KNOWN.iter().find(|(g, _)| *g == guid).map(|(_, n)| *n)
}

/// The IR `objectType` of a timeline entity, by the decoder's type name.
pub const OBJECT_TYPES: &[(&str, &str)] = &[
    ("SketchFeature", "Sketch"),
    ("ExtrudeFeature", "ExtrudeFeature"),
    ("RevolveFeature", "RevolveFeature"),
    ("SweepFeature", "SweepFeature"),
    ("PipeFeature", "PipeFeature"),
    ("FilletFeature", "FilletFeature"),
    ("ChamferFeature", "ChamferFeature"),
    ("HoleFeature", "HoleFeature"),
    ("ThreadFeature", "ThreadFeature"),
    ("CombineFeature", "CombineFeature"),
    ("CircularPatternFeature", "CircularPatternFeature"),
    ("RectangularPatternFeature", "RectangularPatternFeature"),
    ("MirrorFeature", "MirrorFeature"),
    ("ConstructionPlane", "ConstructionPlane"),
    ("ConstructionAxis", "ConstructionAxis"),
    ("BaseFeature", "BaseFeature"),
    ("SplitBodyFeature", "SplitBodyFeature"),
    ("MoveFeature", "MoveFeature"),
    ("RemoveBodyFeature", "RemoveFeature"),
    ("DeleteFaceFeature", "DeleteFaceFeature"),
    ("OffsetFacesFeature", "OffsetFacesFeature"),
    ("ShellFeature", "ShellFeature"),
    ("CoilFeature", "CoilFeature"),
    ("JointOriginFeature", "JointOrigin"),
    ("JointFeature", "Joint"),
    ("AsBuiltJointFeature", "AsBuiltJoint"),
];

/// The IR `objectType` of a timeline entity of class `guid`.
pub fn object_type(guid: &str) -> Option<&'static str> {
    let by_name = known_name(guid).and_then(|name| {
        OBJECT_TYPES
            .iter()
            .chain(ITEM_TYPES)
            .find(|(n, _)| *n == name)
            .map(|(_, t)| *t)
    });
    by_name.or_else(|| {
        ITEM_CLASSES
            .iter()
            .find(|(g, _)| *g == guid)
            .map(|(_, t)| *t)
    })
}

/// Timeline items the import names without translating them
/// (mitcad#43), by the decoder's type name: the IR `objectType`. Their
/// own data is not decoded; the import says what they do and why they are
/// left out.
pub const ITEM_TYPES: &[(&str, &str)] = &[
    // Assembly items, which place occurrences or relate them.
    ("PlaceComponentInstance", "ComponentInsert"),
    ("FastenerFeature", "Fastener"),
    ("CopyPasteFeature", "CopyPasteOccurrence"),
    ("DerivedInstanceFeature", "DerivedInstance"),
    ("GeometricRelationshipFeature", "GeometricRelationship"),
    ("Feature(62E501B2)", "AssemblyRelationship"),
    ("SnapshotFeature", "Snapshot"),
    ("ContextFeature", "Context"),
    // Items that move or copy bodies between components.
    ("ComponentFromBodiesFeature", "ComponentFromBodies"),
    ("CutPasteBodiesFeature", "CopyPasteBodies"),
    ("MoveFaceFeature", "MoveFaceFeature"),
];

/// Timeline item classes beyond [`KNOWN`] (mitcad#43), with their IR
/// `objectType`. Not in [`KNOWN`], whose classes count as known bytes in
/// the coverage table: only the item's tail is decoded.
pub const ITEM_CLASSES: &[(&str, &str)] = &[
    // The item of a new or placed occurrence (no name of its own; the
    // second one belongs to an electronics design).
    (OCCURRENCE_ITEM, "Occurrence"),
    ("8DBAF917-44EA-4D28-8410-201D9454C98B", "Occurrence"),
    // Patterns of occurrences: inputs and copies are occurrences.
    (
        "E8816367-E309-4C27-A220-7862EE1A3498",
        "RectangularOccurrencePattern",
    ),
    (
        "85B1FC69-B27E-45F0-A531-385861034644",
        "CircularOccurrencePattern",
    ),
    ("21E03EC1-0E53-406D-A852-6D088D4D1E95", "GroundOccurrence"),
    ("71BBEDD5-81F8-4585-816B-05908C207204", "MirrorComponent"),
    ("428956BD-1956-4BAF-A4F8-D01D0C465FF1", "DerivedContext"),
    (GROUP, "Group"),
    ("0EE4A83E-464B-4235-A62F-984F15834C27", "Canvas"),
    ("5F40832C-61AA-4A05-95B1-EFA0BA483138", "ConstructionPoint"),
    // Modelling features.
    ("B6678572-C095-4DD1-9348-614677DD0B4A", "LoftFeature"),
    ("F4163A07-9E71-4C46-BD92-ACE5B5B40BED", "DraftFeature"),
    ("6AA4C36C-52A2-48D2-A062-69FE123B84D1", "CylinderFeature"),
    ("04A353F2-AA37-4163-9D4D-A07A7AA9AFA5", "SphereFeature"),
    ("C3A9ECCE-7D25-4432-A645-3A0D9687F22A", "EmbossFeature"),
    // Sheet metal flanges (the base names are those of the flange types).
    ("7AFA0A65-B53C-472C-ACA4-5C7C0A65001C", "FlangeFeature"),
    ("B0EEF272-DF39-419E-905C-38FDB5032817", "PCBFeature"),
    ("99F6967E-ED35-4222-B906-5CCF0AC70B53", "MeshFeature"),
];

/// The timeline item of a new or placed occurrence: the base class of
/// component inserts, fasteners, pasted and derived occurrences.
pub const OCCURRENCE_ITEM: &str = "54F9ACE8-B5B8-4A6E-B582-64F629511DE4";
/// Refers to an occurrence (its first [`OCCURRENCE`] reference) and the
/// item that made it.
pub const OCCURRENCE_REF: &str = "2D6E13A1-BBEF-4FB3-9277-00D99F634136";
/// A timeline group: `u32 n`, `n` references to its items (which follow
/// it in the timeline), `str16 name` (empty for some).
pub const GROUP: &str = "A917FE80-93D6-45AD-B741-93BB1ACA5E61";

/// A component's list of its timeline items (mitcad#37): it refers to the
/// component's [`FEATURE_MANAGER`] and to every item the component owns
/// (features, sketches, construction geometry and the occurrences placed
/// in it). Not in [`KNOWN`].
pub const FEATURE_LIST: &str = "3A6D1E62-99F8-4E32-BAAA-DBA0A2CFB27C";

/// Feature input naming bodies (`9716F783`): refers to a body recipe
/// ([`super::recipe`]).
pub const BODY_REF: &str = "9716F783-676D-42E0-93F9-EBF273E7C035";
/// Feature input: a body, or a fillet's or chamfer's edge set (in an item's
/// input list it comes before the edges and parameters of its set).
pub const BODY_INPUT: &str = "2CA5A1CD-C99B-4C9B-91AF-57989148E841";
/// A thread: a timeline item, or a tapped hole's sub-item (mitcad#35).
pub const THREAD: &str = "584D9526-EF22-4F6F-9FE1-5FCF1F3CE575";

// Sweeps, pipes and lofts (mitcad#34). Their paths, rails and sections
// are [`BODY_INPUT`] lists (`u32 n | n refs` after the root part) of
// entity inputs ([`ENTITY_REF`]) and edge inputs ([`FACE_REF`]).

/// A loft (class version 9; not in [`KNOWN`]).
pub const LOFT: &str = "B6678572-C095-4DD1-9348-614677DD0B4A";
/// One section of a loft: `u64 0 | ref loft | ref list input | u32 kind`
/// (2 a profile, 4 a point, 7 edges).
pub const LOFT_SECTION: &str = "2F3200BA-FDFB-48C6-97D4-71A4A0D86190";
/// The sketch curve an entity input names: `u64 secondary id | u64 sketch
/// | u64 primary id` after its root part, the curve's `crv_secondary_id`,
/// sketch object and `crv_primary_id`.
pub const SKETCH_CURVE_ID: &str = "E2CEFD18-D755-4E09-8E7F-953A2F6D43F8";
/// The sketch point an entity input names: `u64 sketch | u64 point tag`
/// (the point's `pt_tag`).
pub const SKETCH_POINT_ID: &str = "D95DBAC0-238B-4429-A0AD-4AE5BEE79ADF";

/// Sketch point classes.
pub fn is_point_class(guid: Option<&str>) -> bool {
    matches!(guid, Some(SKETCH_POINT | SKETCH_POINT_2))
}

/// Sketch line classes (line, construction and centre lines).
pub fn is_line_class(guid: Option<&str>) -> bool {
    matches!(guid, Some(SKETCH_LINE | SKETCH_LINE_2 | SKETCH_LINE_3))
}

// Joints and grounding (mitcad#66). Their own data: [`super::build`]'s
// `joints` module.

/// A joint item.
pub const JOINT: &str = "8DFDD543-F823-43AA-BDAF-C638E023F0A0";
/// An as-built joint item.
pub const AS_BUILT_JOINT: &str = "AA8B3D40-B67D-4AF1-B3CF-E1E00279B5F0";
/// A joint origin.
pub const JOINT_ORIGIN: &str = "02E8AA49-08B8-439C-A8C7-221BE20143D8";
/// A ground item: grounds the occurrence its placement names.
pub const GROUND_OCCURRENCE: &str = "21E03EC1-0E53-406D-A852-6D088D4D1E95";
/// A joint's or as-built joint's state: the occurrences it relates, its
/// motion's type and current values, an ordinal.
pub const JOINT_STATE: &str = "7F5A0426-1D2A-4DC4-9F06-58631F69D2F5";
/// A placement: an occurrence (through a [`CONTEXT_PATH`]) and a frame
/// in its component.
pub const PLACEMENT: &str = "2E3AC990-483D-4A55-8662-C064B976A957";
/// An occurrence path: `u8 | u32 n | n refs` to [`CONTEXT_LEVEL`]s.
pub const CONTEXT_PATH: &str = "FF415D89-A1DB-44AC-B824-E9A772B86ED4";
/// One level of an occurrence path: the occurrence's, its document's and
/// its component's GUIDs, then the GUIDs of the document and component it
/// sits in.
pub const CONTEXT_LEVEL: &str = "96CBEA21-17E0-4BCB-BA77-04EA87B47934";
/// A frame input (an input of [`BODY_INPUT`]'s kind): a matrix and the
/// key point and direction inputs it was built from. Sketches and joint
/// geometry use it.
pub const FRAME_INPUT: &str = "7B6C3C2D-4096-4E6D-B225-699E8C2D9355";
/// A key point input: a point (component space, cm), a key point code and
/// the entity it lies on.
pub const KEY_POINT: &str = "69EE2FA7-BCC7-449E-9CA9-976CEFDFED44";
/// A direction input: a point, a direction (component space), a code and
/// the entity it follows.
pub const DIRECTION_INPUT: &str = "F2A7590D-6654-4674-B393-A2AEF4FEC48A";
/// A body record: the body object and the feature that made it, which
/// lists its records in the order it made the bodies (mitcad#96,
/// `build/producers.rs`). A body input refers to its body's record.
pub const BODY_RECORD: &str = "D26351F0-5940-4D23-AA20-2C35475A6D9E";

// Captured positions (mitcad#75).

/// A captured position (`SnapshotFeature`): the placements of the
/// occurrences it captured, each a [`PLACEMENT`] and a matrix.
pub const SNAPSHOT: &str = "8EE00B00-76BB-49AB-8C25-E837FEC5BDA5";

// Occurrence placements (mitcad#81).

/// Where the items of the timeline put an occurrence: its path (the
/// [`OCCURRENCE`]s, or the item that made it), the component the path
/// starts in, and per item that placed it the path's placement.
pub const OCCURRENCE_PLACEMENTS: &str = "549FBB80-B890-473E-A5C0-415D3D9BF4E6";

// Inputs read from the learning dump (mitcad#96).

/// The loops of the profiles a profile source ([`PROFILE_SOURCE`])
/// selected, by the ids of their curves ([`super::build`]'s `profiles`
/// module).
pub const PROFILE_LOOPS: &str = "0D57BD2F-D09B-43FC-AD57-1E89A118C453";
/// A revolution's profile input: `u32 n | n` [`PROFILE_ID`] references, as
/// an extrusion's [`PROFILE`].
pub const REVOLVE_PROFILE: &str = "63D2920D-7EC2-49C4-9F08-315197F9E657";
