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
/// An integer input of a feature (pattern quantities): root list
/// `[feature]`, then `u32 slot, u8 0, u8 1, u32 value` (T1b).
pub const INT_HOLDER: &str = "A085449A-5144-4B2B-B455-7F7035A40559";
/// A `Geometry` object, one per design, that a sketch refers to right
/// before its name and entity list (its own meaning is open); the sketch's
/// light bulb comes before that reference (mitcad#6). Not in [`KNOWN`].
pub const SKETCH_NAME_ANCHOR: &str = "5275CBA4-3D7D-40DC-A651-1DFF3FC8AFBF";

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
    let name = known_name(guid)?;
    OBJECT_TYPES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| *t)
}

/// Sketch point classes.
pub fn is_point_class(guid: Option<&str>) -> bool {
    matches!(guid, Some(SKETCH_POINT | SKETCH_POINT_2))
}

/// Sketch line classes (line, construction and centre lines).
pub fn is_line_class(guid: Option<&str>) -> bool {
    matches!(guid, Some(SKETCH_LINE | SKETCH_LINE_2 | SKETCH_LINE_3))
}
