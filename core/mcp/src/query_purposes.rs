// SPDX-License-Identifier: MIT
//! What each query of the model API is for (mitcad#2636): a sentence or two
//! per query that an external client may run, as a command's JSON Schema
//! gives a command its purpose. The generated API reference opens a query's
//! page with it and `model_reference {"query": …}` answers it as `purpose`.
//!
//! The sentences are for users and integrators: what the query tells, in
//! words, without ticket numbers, history, file names or notes on the code
//! inside. A new query of `mitcad_model_api::classes::external_queries`
//! appends its sentence at the end; the tests below and the API reference's
//! check (`api_reference_tests.rs`) fail without one.

/// The queries' purposes, in the order of the query classification
/// (`core/model-api/src/classes.rs`); new ones at the end.
pub const QUERY_PURPOSES: &[(&str, &str)] = &[
    (
        "document",
        "Gives the state of a document: its features and timeline marker, the number of bodies, the undo and redo labels, its units, its revision and whether it has unsaved changes.",
    ),
    (
        "timeline",
        "Lists the features in timeline order with their status, errors, warnings and notices, the timeline marker and the groups: what the timeline shows.",
    ),
    (
        "evaluate",
        "Evaluates an expression with the document's parameters and gives its value and its text in the document's unit, without changing anything.",
    ),
    (
        "formula_help",
        "Helps write an expression: the supported functions with their arguments, the argument the cursor is in and the value of the selected part. It never changes the expression or the parameters.",
    ),
    (
        "printed_fit_parameters",
        "Lists the parameters that hold the clearances of printed fits, with their values and the features and fields that use them.",
    ),
    (
        "parameters",
        "Lists the document's parameters (user parameters, the features' model parameters and the values of driven dimensions) with their expressions, values, units and comments.",
    ),
    (
        "bodies",
        "Lists the bodies at the timeline marker with their names, components, visibility, materials and appearances; on request also their volumes, areas, bounding boxes and shape versions.",
    ),
    (
        "profiles",
        "Lists the closed regions of the sketches that features such as Extrude take as profiles, or those of one sketch.",
    ),
    (
        "autoselect_profiles",
        "Gives the groups of profiles that Autoselect in Solid mode offers for the sketch being worked on: its drawn regions and those of projected geometry.",
    ),
    (
        "sheet_state",
        "Gives the state of a sheet-metal body: its panels with their outlines, its rules, whether it is valid, and what invalidated or blocks it. Without a body it gives every sheet-metal body's state.",
    ),
    (
        "sheet_rules",
        "Gives the resolved fabrication rules of a sheet-metal rules feature, such as its thickness, or those of every rules feature.",
    ),
    (
        "feature",
        "Gives one feature: its name, type, status, errors, warnings and notices, its visibility and its definition.",
    ),
    (
        "faces",
        "Lists the faces of a body with their topological names; on request also their geometry (surface type, area, normal and more), filtered and in pages.",
    ),
    (
        "edges",
        "Lists the edges of a body with their topological names; on request also their geometry (curve type, length, radius and the faces they lie between), filtered and in pages.",
    ),
    (
        "report",
        "Gives a summary of the design in one answer: the timeline and the bodies with their properties, and in a design with components also the components and the bodies as they are placed.",
    ),
    (
        "datums",
        "Lists the construction planes, axes and points with their geometry; on request also the origin's planes and axes.",
    ),
    (
        "datum",
        "Gives the geometry of one construction plane, axis or point, or of a plane or axis of the origin.",
    ),
    (
        "properties",
        "Gives the physical properties of bodies or components: volume, area, mass, center of mass and moments of inertia, from their materials' densities or from densities given.",
    ),
    (
        "measure",
        "Measures one selection (a body's volume, a face's area, an edge's length, a point, a circle, an axis or a plane) or two together: their minimum distance, offset and angle.",
    ),
    (
        "interference",
        "Finds the pairs of bodies that pass through each other and the volume each pair shares.",
    ),
    (
        "section",
        "Cuts bodies with a plane, without changing them, and gives the section's edges, faces, lengths and areas per body.",
    ),
    (
        "section_plane",
        "Gives what `section` gives for a section plane that may be tilted about an axis, and also that axis.",
    ),
    (
        "threads",
        "Lists the threads on the bodies (thread features and tapped holes) with their standards, sizes, diameters, pitches and the faces they are on.",
    ),
    (
        "sketch",
        "Gives a sketch: its plane and frame, its points, curves and texts, its constraints and dimensions, whether it is solved and its degrees of freedom.",
    ),
    (
        "components",
        "Lists the components, the root first, with their features, bodies, occurrences, links and design policies, and names the active one.",
    ),
    (
        "instances",
        "Lists the bodies as the design places them: each body once for every occurrence that places it, with its occurrence path and transform.",
    ),
    (
        "dependents",
        "Lists the features that refer to a feature, directly or through others: what deleting it with its dependents deletes too.",
    ),
    (
        "can_reorder",
        "Tells whether a feature can move to a place in the timeline, and why not when it cannot.",
    ),
    (
        "named_views",
        "Lists the document's named views with their cameras.",
    ),
    (
        "recompute_times",
        "Lists how long each feature the last recompute evaluated took, in timeline order, and their sum.",
    ),
    (
        "thread_sizes",
        "Lists the thread sizes of every thread standard, or of one, with their diameters and pitches.",
    ),
    (
        "cache",
        "Gives the statistics of the recompute cache: its memory use, its hits and misses per feature, and on request what its store on disk holds.",
    ),
    (
        "changes_since_saved",
        "Lists the changes made since the document was last saved, as the labels of their undo steps, or tells that they are not known.",
    ),
    (
        "analyses",
        "Lists the analyses kept in the design, such as section analyses, with their definitions, their visibility and where their planes are at the timeline marker.",
    ),
    (
        "appearances",
        "Lists the appearances of the library and of the document with their parameters.",
    ),
    (
        "appearance_image",
        "Gives an image of an appearance's texture set, such as its base color map, as base64 data.",
    ),
    (
        "render_settings",
        "Gives the document's render settings, every section and field.",
    ),
    (
        "joints",
        "Lists the joints and as-built joints in timeline order with their types, the occurrences they join, their motions, limits and positions and their state.",
    ),
    (
        "joint_dof",
        "Tells how an occurrence can move in its parent component: whether it is grounded, which occurrences move with it, and its remaining degrees of freedom and motions.",
    ),
    (
        "joint_drag",
        "Tells where the occurrences would move, and the joints' values, if an occurrence were dragged from a point to a target, without moving anything.",
    ),
    (
        "joint_frame",
        "Gives the frame a joint origin on some geometry gives at the timeline marker, in a component's coordinates: where a joint on that geometry would sit.",
    ),
    (
        "configurations",
        "Gives the design's configuration table with its options, rows and rules; given rules are checked against it without changing anything.",
    ),
    (
        "library_parts",
        "Lists the components inserted from component libraries with their library records, versions and occurrences.",
    ),
    (
        "parts_list",
        "Lists the components with their designations, quantities and library details: the rows of the Parts List.",
    ),
    (
        "constraint_row",
        "Tells what kinds of geometry two references are, which assembly constraint types fit them, the likeliest first, and their current offset, angle and twist.",
    ),
    (
        "move_to_component",
        "Tells what moving features into a component would do (the component, the bodies it would get and whether the design becomes a hybrid assembly), without changing anything.",
    ),
    (
        "feature_references",
        "Lists the features a feature definition refers to and the components they are in.",
    ),
    (
        "profile",
        "Gives the region a profile reference means now, also after its sketch changed, with the current keys of every region it means.",
    ),
    (
        "flat_pattern",
        "Gives the flat pattern of a sheet-metal body: every bend developed while the fixed panel stays, with its outlines and bend lines in the fixed panel's coordinates.",
    ),
    (
        "flat_point",
        "Maps a position on a sheet-metal body's flat pattern to the body, or a point of the body to the flat pattern, with the material depth from the front surface.",
    ),
    (
        "sheet_reference",
        "Tells what a picked face or edge of a sheet-metal body stands for: the panel, bend or wall and the boundary it belongs to.",
    ),
    (
        "sheet_seam",
        "Gives the two boundaries of the seam two panels of a sheet-metal body share, the stationary one first.",
    ),
    (
        "sheet_bodies",
        "Lists the sheet-metal bodies at the timeline marker with their rules, thickness, bend radius, panels and bends, and the state of their flat patterns.",
    ),
    (
        "sheet_corner_edges",
        "Gives the thickness edge that each stored corner of a sheet-metal corner round or chamfer lies on in the body at the timeline marker.",
    ),
    (
        "policy",
        "Gives a component's design policy, its settings and preset, and for each setting the items that break it; it can report as if another policy were set.",
    ),
    (
        "policies",
        "Lists the design policy of every component and how many items break each.",
    ),
    (
        "sketch_supports",
        "Tells which components' faces and planes a component's next sketch may not be attached to under the design policies, and why.",
    ),
    (
        "reparent_occurrence",
        "Tells what moving an occurrence to another parent would do (the occurrences that move with it and the joints and groups that hold them), without changing anything.",
    ),
    (
        "plane_geometry",
        "Lists the edges and vertices of the visible bodies that lie in a sketch's plane, in sketch coordinates: what the sketch tools snap to and pick.",
    ),
    (
        "tangent_chains",
        "Lists the tangent chains of a body's edges at the timeline marker: what a pick takes with Tangent Chain on.",
    ),
    (
        "thread_chamfer",
        "Gives the default end chamfer of a thread size.",
    ),
    (
        "build_preview",
        "Tells what placing a BUILD part, a panel or a connection would make (its frame, length and ends), without changing anything.",
    ),
    (
        "build_parts",
        "Lists the BUILD components with their roles, sources, designations, suppliers, lengths and ends.",
    ),
    (
        "build_junctions",
        "Lists where BUILD parts meet: each junction's kind and the ends that meet there.",
    ),
    (
        "build_cut_list",
        "Gives the cut list of the BUILD parts: identical pieces grouped with their lengths, ends and quantities, with the saw's kerf taken into account.",
    ),
    (
        "build_end_meets",
        "Lists the ends of the BUILD profiles cut to length and what each end meets.",
    ),
    (
        "build_host",
        "Gives the BUILD host: the component new BUILD parts go into.",
    ),
    (
        "library_usage",
        "Lists the library components the design uses, with their libraries and how many times each is placed.",
    ),
    (
        "motion_studies",
        "Lists the motion studies with their steps and keys.",
    ),
    (
        "study_values",
        "Gives the joints' values at a step of a motion study.",
    ),
    (
        "contact_sets",
        "Lists the contact sets with their occurrences and whether each is suppressed.",
    ),
    (
        "contact_check",
        "Lists the pairs of bodies in the active contact sets that pass through each other at the timeline marker.",
    ),
    (
        "motion_link_offset",
        "Gives the offset a motion link between two joints would take as built.",
    ),
    (
        "transfer_bodies",
        "Tells where bodies can be transferred to: every placed component by its occurrence path, and why a component cannot take them.",
    ),
    (
        "gear_values",
        "Gives a gear's derived values (module, pitch, diameters and more) from a gear feature or a definition.",
    ),
    (
        "gear_link",
        "Gives the motion link command that links a gear to its mate, or the reason it cannot.",
    ),
    (
        "property_sets",
        "Gives the properties of a body, an occurrence or a component at each level, the most specific first; on request also their computed values.",
    ),
    (
        "property_types",
        "Lists the property types, the standard ones and the design's, with their base types, units, defaults and uses.",
    ),
    (
        "property_search",
        "Finds the components, occurrences and bodies whose properties match a key, or a key with a comparison such as `Mass > 10 kg`.",
    ),
    (
        "mount_frames",
        "Lists the mount frames of a component and of the components placed in it, at the timeline marker.",
    ),
    (
        "pulley_values",
        "Gives a pulley's derived values (pitch diameter, toothed width and more) from a pulley feature or a definition, and the belt lengths that fit.",
    ),
    (
        "bearing_values",
        "Gives a printable bearing's derived values (bore, outside diameter, width and its rolling elements) from a bearing feature or a definition.",
    ),
    (
        "bearing_sizes",
        "Lists the printable bearing sizes, or those that fit a shaft or a hole of a diameter with a clearance.",
    ),
    (
        "seat_frame",
        "Gives where a seat on a shaft puts the seated item at the timeline marker: its origin and axis in the component's coordinates.",
    ),
    (
        "print_overhangs",
        "Finds the faces of bodies that overhang more than an angle in a print direction, with their areas and triangles.",
    ),
    (
        "print_thickness",
        "Finds where bodies are thinner than a minimum wall thickness for printing, and the thinnest place of each body.",
    ),
    (
        "print_estimate",
        "Estimates what printing bodies takes: volumes, filament length and mass, and cost.",
    ),
    (
        "materials",
        "Lists the materials: the built-in ones the design does not replace, then the design's own, with their fields.",
    ),
    (
        "body_material",
        "Gives the material of a body where an occurrence places it, and where it comes from: the body, its component's default or the print material.",
    ),
    (
        "material_conflicts",
        "Lists the bodies whose appearance was set by hand and would change with a material.",
    ),
    (
        "print_keys",
        "Lists the keys of the printer, process and filament settings in display order, with their names and kinds.",
    ),
    (
        "print_profiles",
        "Lists the design's print profiles with their fields and the owners that chose each.",
    ),
    (
        "print_settings",
        "Gives the print settings that apply to a body, an occurrence or a component, level by level, the nearest first, and the printer, process and filament that result.",
    ),
    (
        "build_volume",
        "Gives the build volume of the root's printer: its bed outline, excluded areas and height.",
    ),
    (
        "place_frame",
        "Gives where a placement puts an item at the timeline marker: its origin and axes.",
    ),
    (
        "gear_pair",
        "Designs a pair of gears for a ratio between two placements (their teeth, module and center distance), without changing anything.",
    ),
    (
        "belt_drive",
        "Designs a belt drive for a ratio between two placements (the pulleys' teeth, the center distance and the belt lengths that fit), without changing anything.",
    ),
    (
        "belt_check",
        "Checks a belt for two pulleys already made: the center distance from their axes and the belt lengths that fit.",
    ),
    (
        "measurements",
        "Lists the measurements kept in the design with their definitions and their values measured at the timeline marker.",
    ),
    (
        "annotations",
        "Lists the annotations kept in the design with their definitions and their values measured at the timeline marker, as `measurements` does.",
    ),
    (
        "geometric_characteristics",
        "Lists the geometric characteristics a feature control frame can state, such as flatness and position, with their symbols.",
    ),
    (
        "next_datum_letter",
        "Gives the letter a new datum label takes.",
    ),
    ("parameters_csv", "Gives all the parameters as a CSV table."),
    (
        "parameters_csv_preview",
        "Tells what importing a CSV table of parameters would do, without changing anything.",
    ),
    (
        "purge_volumes",
        "Gives the purge volumes between the materials of bodies printed together, with the values each material uses.",
    ),
    (
        "purge_defaults",
        "Gives the purge volumes that a material of a color takes when it has none of its own.",
    ),
    (
        "drawings",
        "Lists the design's drawings with their components and sheets.",
    ),
    (
        "drawing",
        "Gives a drawing: its sheets with their views, annotations and title block, and on request the views' drawn geometry.",
    ),
    (
        "drawing_pick",
        "Tells what a click at a place on a drawing sheet picks: a point, a line, a circle or a dimension of a view.",
    ),
    (
        "sketch_fragment",
        "Copies sketch geometry: the chosen curves, points and texts with what holds them, ready to paste into a sketch.",
    ),
    (
        "sketch_blocks",
        "Lists the sketch blocks with their instances, the blocks they hold and an outline for a preview.",
    ),
    (
        "image_references",
        "Lists the image references with their planes, placements, sizes and files.",
    ),
    (
        "image_reference_image",
        "Gives the image file of an image reference as base64 data.",
    ),
    (
        "image_reference_outline",
        "Tells where an image with a number of pixels would lie on a plane with a placement: its corners, for a preview.",
    ),
    (
        "configure",
        "Tells what choosing configuration options would give (each option's value and allowed values, the matching row and the problems), without changing anything.",
    ),
    (
        "check_rules",
        "Goes through the combinations of the configuration options and reports the values the rules never allow, the rules that never apply and the combinations left without a value.",
    ),
    (
        "tables",
        "Lists the design's tables with their columns and rows, the parameters that use them and their problems.",
    ),
    ("table_csv", "Gives a table as CSV."),
    (
        "table_csv_preview",
        "Tells what importing CSV into a table would do, without changing anything.",
    ),
    (
        "cut_sheet",
        "Gives a cut sheet: the parts laid out on it with their counts and outlines, and its warnings.",
    ),
    (
        "annotation_text_height",
        "Gives the text height of annotations: the one set, the one in use and the automatic one, with the series of heights to choose from.",
    ),
    (
        "hole_sizes",
        "Lists the clearance holes and recesses for screw sizes, or for one size or thread.",
    ),
    (
        "feature_inputs",
        "Gives the edges and faces a feature picked, as they were at its last build, as polylines and meshes for the view.",
    ),
    (
        "configuration_rows_csv",
        "Gives the rows of a closed configuration table as CSV.",
    ),
    (
        "configuration_rows_csv_preview",
        "Tells what importing CSV rows into a closed configuration table would do, without changing anything.",
    ),
    (
        "make_differences",
        "Lists the bodies whose material or print settings differ from their component's.",
    ),
    (
        "save_component_as_design",
        "Tells whether a component can be saved as a design of its own, what it refers to, and why it cannot when it cannot.",
    ),
    (
        "center_of_mass",
        "Gives the center of mass, mass and volume of the design's bodies, and how many of them have no material.",
    ),
    (
        "design_properties",
        "Gives the design's summary properties, such as its title, author and keywords, and its statistics, such as when it was created and last saved.",
    ),
    (
        "comments",
        "Lists the comments, or those with a status or on a target, with their targets and places.",
    ),
    (
        "comment_attachment",
        "Gives an image attached to a comment, as base64 data.",
    ),
    (
        "drawing_snap",
        "Tells where a drawing annotation being placed or dragged would snap to keep the spacing of the dimensions beside it, without changing anything.",
    ),
    (
        "print_plates",
        "Lists the print plates with their items, values and the bodies they print, and the bodies on no plate.",
    ),
    (
        "plate_layout",
        "Gives where the pieces of a print plate lie on its bed.",
    ),
    (
        "material_units",
        "Gives the units material properties are shown in: metric or imperial, and the temperature unit.",
    ),
    (
        "material_dataset",
        "Searches the built-in dataset of typical material properties and gives the matches with the dataset's license and source.",
    ),
    (
        "export_materials",
        "Gives materials as JSON or CSV for saving to a file.",
    ),
    (
        "drawing_corners",
        "Lists the points a click picks on the views of a drawing sheet: the ends of their edges.",
    ),
    (
        "feature_curves",
        "Lists the curves of the Intersection Curve features before the timeline marker, as polylines.",
    ),
    (
        "sketch_loop",
        "Gives the closed loop of a sketch that a curve belongs to.",
    ),
    (
        "sketch3d",
        "Gives a 3D sketch: its support, frame, points and curves, and on request a path through its curves.",
    ),
    (
        "design_review",
        "Reviews the design against modeling and printing rules and lists the findings with their levels, messages and advice.",
    ),
    (
        "print_min_gap",
        "Finds the pairs of bodies printed in place that are closer than a gap and could fuse, with their closest points.",
    ),
    (
        "filament_changes",
        "Gives the filament changes at print heights of a print plate or of the design.",
    ),
    (
        "plane_geometry_key",
        "Gives a key that changes whenever what `plane_geometry` reads for a sketch changes, so that an earlier answer can be reused while the key stays.",
    ),
    (
        "filament_change_preview",
        "Gives the colored bands that the filament changes paint on the printed bodies, as the view draws them while the changes are edited.",
    ),
    (
        "joint_poses",
        "Tells where the occurrences a joint moves are at given values of one of its motions: the frames of an animation.",
    ),
    (
        "flex",
        "Gives a Flex item's route: its item, nodes, length, smallest bend, polyline and on request its frames, and the problems found.",
    ),
    (
        "flex_items",
        "Lists the Flex items before the timeline marker with their bodies, lengths and the parameters that hold the lengths.",
    ),
    (
        "end_ports",
        "Lists the end ports that a Pipe or a Flex item can connect to: those of a feature, of a body or of the whole design.",
    ),
    (
        "flex_hang",
        "Gives the Flex item that hangs under gravity, or nothing when none does.",
    ),
    (
        "gravity",
        "Gives the design's gravity direction, where it comes from and which Flex item hangs.",
    ),
    (
        "cam_setups",
        "Lists the CAM setups with their operations, work coordinates and stock.",
    ),
    ("cam_operations", "Lists the operations of a CAM setup."),
    (
        "cam_toolpaths",
        "Gives the toolpaths of a CAM setup's operations: their moves, tools, spindle speeds and cut lengths.",
    ),
    (
        "cam_post",
        "Gives the G-code of a CAM setup in a machine's dialect and units.",
    ),
    (
        "cam_machines",
        "Lists the CAM machines with their tools and the setups that use them.",
    ),
    (
        "cam_dialects",
        "Lists the G-code dialects a CAM setup can be posted in.",
    ),
    (
        "cam_simulate",
        "Simulates cutting a CAM setup's stock and gives the material left, the volume removed and the rapid moves that cut.",
    ),
    (
        "parameter_range",
        "Tries a parameter at several values on a copy of the design and tells how the bodies and features change at each value, without changing the design.",
    ),
    (
        "form_cage",
        "Gives a Form's control cage: its vertices, edges, faces and creases, as an editor draws and picks them.",
    ),
    (
        "form_limit",
        "Gives the limit surface of a Form's control cage as a triangle mesh, for a quick preview.",
    ),
    (
        "library_source",
        "Gives the design file of a library source as the design keeps it, at the version its components are pinned to.",
    ),
    (
        "sketch_surface",
        "Gives a sketch's points on the face it lies on, front and back, for the view and the pointer.",
    ),
    (
        "joint_holder",
        "Tells which component holds a set of picks for a joint, and how it relates to the active component.",
    ),
    (
        "face_run",
        "Tells how a face's walls continue and whether the face is a feature's moving end, for Press Pull.",
    ),
    (
        "sheet_flange_corners",
        "Gives the corners a flange on several edges of a sheet-metal body would have, for choosing a miter per corner.",
    ),
    (
        "assembly_path",
        "Checks the way a body goes into place: it is slid along a direction or turned about an axis past other bodies, and the answer gives the first position where it runs into one of them.",
    ),
    (
        "sheet_metal_list",
        "Lists every sheet-metal body the design places with its quantity, thickness, material and developed size: the rows of the Sheet Metal list.",
    ),
    (
        "sheet_process",
        "Gives the sheet-metal process profile of a component and the commands that profile offers.",
    ),
    (
        "mesh_stats",
        "Gives the counts of a mesh body (vertices, triangles, open and non-manifold edges and holes), its area and whether it is closed.",
    ),
    (
        "print_job",
        "Tells what a print job would send for bodies and a slicer (their materials, plates and settings), before it is sent.",
    ),
    (
        "boundary_regions",
        "Lists the closed regions that tool bodies and planes enclose at the timeline marker and places points among them, for Boundary Fill.",
    ),
    (
        "parameter_groups",
        "Lists the groups of user parameters with their parameters, a parent before its subgroups.",
    ),
    (
        "draft_analysis",
        "Analyzes the draft angles of the faces of bodies for a pull direction and gives the areas with too little draft.",
    ),
    (
        "selection_sets",
        "Lists the selection sets with their members.",
    ),
    (
        "print_service_check",
        "Tells which bodies may be sent to a print service: a body placed by a library component whose license forbids printing is not printable, and the answer says why.",
    ),
    (
        "point_clouds",
        "Lists the point clouds with their placements, point counts and bounds.",
    ),
    (
        "point_cloud_points",
        "Gives the points of a point cloud as base64 data, with their transform into the design and their colors.",
    ),
    (
        "point_cloud_deviation",
        "Gives the statistics of how far a point cloud lies from the bodies.",
    ),
    (
        "decals",
        "Lists the decals with their faces, placements, sizes and files.",
    ),
    (
        "decal_image",
        "Gives the image file of a decal as base64 data.",
    ),
    (
        "decal_outline",
        "Tells where a decal of a size would lie on a face with a placement, for a preview.",
    ),
    (
        "build_end_caps",
        "Lists the end caps of the BUILD profiles; with ends and a thickness it also previews new caps without changing the design.",
    ),
    (
        "explode_views",
        "Lists the exploded views with their steps and the occurrences each step moves.",
    ),
    (
        "explode_view",
        "Gives an exploded view at a time of its animation: how far each step has gone and on request where the bodies are.",
    ),
    (
        "point_cloud_alignment",
        "Tells where aligning a point cloud would move it, without moving it.",
    ),
    (
        "cam_tool_shape",
        "Gives a CAM tool as it would be kept, with the outlines of a turning tool's insert and holder.",
    ),
    (
        "min_radius_analysis",
        "Finds the faces of bodies whose radius of curvature is smaller than a limit, such as places a cutter cannot reach.",
    ),
    (
        "construction_sets",
        "Lists the construction sets with their members and which one is active.",
    ),
    (
        "minimum_distance",
        "Gives the minimum distance between two selections, where it is and whether it keeps a clearance.",
    ),
    (
        "edge_continuity",
        "Measures the continuity across edges of bodies (gaps, tangent angles and changes of curvature) at samples along each edge.",
    ),
    (
        "surface_analysis",
        "Gives a surface quality analysis of the faces of bodies, such as zebra stripes or curvature, as colored meshes for the view and its numbers.",
    ),
    (
        "curvature_comb",
        "Gives the curvature combs of curves and edges: their teeth, for drawing them.",
    ),
    (
        "cut_sheet_cutting",
        "Gives the cutting paths of a cut sheet for a plasma torch: each part's loops moved by half the kerf, in the order they are cut.",
    ),
    (
        "gear_drive_values",
        "Gives the derived values of a gear drive, such as a planetary set's ring teeth, center distance and ratios.",
    ),
    (
        "gear_drive",
        "Gives the gear drive a component belongs to: its definition, what it made and its values.",
    ),
];

/// A query's purpose, when the catalog has one.
pub fn query_purpose(name: &str) -> Option<&'static str> {
    QUERY_PURPOSES
        .iter()
        .find(|(query, _)| *query == name)
        .map(|(_, purpose)| *purpose)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::api_reference::clean;

    /// The longest purpose, in characters: a sentence or two.
    const PURPOSE_CHARS: usize = 300;

    #[test]
    fn every_external_query_has_a_purpose_and_no_other_name_has_one() {
        let external: BTreeSet<&str> = mitcad_model_api::classes::external_queries()
            .into_iter()
            .collect();
        let listed: BTreeSet<&str> = QUERY_PURPOSES.iter().map(|(name, _)| *name).collect();
        assert_eq!(listed.len(), QUERY_PURPOSES.len(), "a query listed twice");
        let missing: Vec<&&str> = external.difference(&listed).collect();
        assert!(
            missing.is_empty(),
            "queries without a purpose in query_purposes.rs: {missing:?}"
        );
        let unknown: Vec<&&str> = listed.difference(&external).collect();
        assert!(
            unknown.is_empty(),
            "purposes of names that are no external query: {unknown:?}"
        );
    }

    #[test]
    fn every_purpose_is_a_clean_sentence() {
        for (name, purpose) in QUERY_PURPOSES {
            assert!(
                purpose.chars().next().is_some_and(char::is_uppercase),
                "{name}: {purpose}"
            );
            assert!(purpose.ends_with('.'), "{name}: {purpose}");
            assert!(
                purpose.chars().count() <= PURPOSE_CHARS,
                "{name}: longer than {PURPOSE_CHARS} characters"
            );
            assert!(
                !purpose.starts_with("A query of"),
                "{name}: says nothing of what it is for"
            );
            // The API reference's cleaning keeps it whole: no ticket, no
            // history, no file or test name, no note on the code inside.
            let mut dropped = Vec::new();
            assert_eq!(
                clean::clean_text(purpose, &mut dropped),
                *purpose,
                "{name}: the cleaning changes it"
            );
            assert!(dropped.is_empty(), "{name}: the cleaning leaves out a part");
        }
    }

    #[test]
    fn a_purpose_is_found_by_its_query() {
        assert!(query_purpose("bodies").is_some_and(|text| text.contains("bodies")));
        assert!(query_purpose("measure").is_some_and(|text| text.starts_with("Measures")));
        assert_eq!(query_purpose("no_such_query"), None);
    }
}
