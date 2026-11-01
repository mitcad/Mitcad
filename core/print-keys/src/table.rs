// SPDX-License-Identifier: MIT
// The key map's table: Mitcad's keys and their dialect keys, built from
// the slicers' option lists by meaning (see the module documentation).
// The rows are in display order (mitcad#655): by set, category and group
// as the enums list them, common keys first in each group.

use super::{B, C, DialectKey, Form, Group, KeyDef, Kind, O, P, PrintSet, Q, Unit};

/// Every key of the map: the printer's, the process' and the filament's.
pub(super) static KEYS: &[KeyDef] = &[
    key!(
        Printer,
        "bed_shape",
        Kind::Points,
        "Bed shape",
        PrintableSpace,
        common,
        [
            d!(B | O | Q, "printable_area"),
            d!(P, "bed_shape"),
            d!(C, "machine_width", Form::CuraBed),
        ]
    ),
    key!(
        Printer,
        "bed_exclude_area",
        Kind::Points,
        "Excluded bed areas",
        PrintableSpace,
        common,
        [d!(B | O | Q, "bed_exclude_area"),]
    ),
    key!(
        Printer,
        "max_print_height",
        Kind::Number(Unit::Mm),
        "Printable height",
        PrintableSpace,
        common,
        [
            d!(B | O | Q, "printable_height"),
            d!(P, "max_print_height"),
            d!(C, "machine_height"),
        ]
    ),
    key!(
        Printer,
        "printer_model",
        Kind::Text,
        "Printer model",
        PrinterPresets,
        advanced,
        per_dialect,
        [
            d!(B | O | Q, "printer_model"),
            d!(P, "printer_model"),
            d!(C, "machine_name"),
        ]
    ),
    key!(
        Printer,
        "printer_settings_id",
        Kind::Text,
        "Printer preset",
        PrinterPresets,
        advanced,
        per_dialect,
        [
            d!(B | O | Q, "printer_settings_id"),
            d!(P, "printer_settings_id"),
            d!(C, "definition"),
        ]
    ),
    key!(
        Printer,
        "nozzle_diameter",
        Kind::Number(Unit::Mm),
        "Nozzle diameter",
        Nozzle,
        common,
        [
            d!(B | O | Q, "nozzle_diameter", list),
            d!(P, "nozzle_diameter", list),
            d!(C, "machine_nozzle_size", list),
        ]
    ),
    key!(
        Printer,
        "filament_slots",
        Kind::Integer(Unit::None),
        "Filament slots",
        Nozzle,
        common,
        [
            d!(P, "nozzle_diameter", Form::Count),
            d!(C, "machine_extruder_count"),
        ]
    ),
    key!(
        Process,
        "layer_height",
        Kind::Number(Unit::Mm),
        "Layer height",
        LayerHeight,
        common,
        [
            d!(B | O | Q, "layer_height"),
            d!(P, "layer_height"),
            d!(C, "layer_height"),
        ]
    ),
    key!(
        Process,
        "first_layer_height",
        Kind::OrPercent(Unit::Mm, "layer_height"),
        "First layer height",
        LayerHeight,
        common,
        [
            d!(B | O | Q, "initial_layer_print_height", Form::Absolute),
            d!(P, "first_layer_height"),
            d!(C, "layer_height_0", Form::Absolute),
        ]
    ),
    key!(
        Process,
        "enable_mixed_color_sublayer",
        Kind::Boolean,
        "Mixed color sublayers",
        LayerHeight,
        advanced,
        [d!(B | O | Q, "enable_mixed_color_sublayer"),]
    ),
    key!(
        Process,
        "extrusion_width",
        Kind::OrPercent(Unit::Mm, "nozzle_diameter"),
        "Line width",
        LineWidth,
        common,
        [
            d!(B | Q, "line_width", Form::Absolute),
            d!(O, "line_width"),
            d!(P, "extrusion_width", Form::OrPercent("layer_height")),
            d!(C, "line_width", Form::Absolute),
        ]
    ),
    key!(
        Process,
        "first_layer_extrusion_width",
        Kind::OrPercent(Unit::Mm, "nozzle_diameter"),
        "First layer line width",
        LineWidth,
        common,
        [
            d!(B | Q, "initial_layer_line_width", Form::Absolute),
            d!(O, "initial_layer_line_width"),
            d!(
                P,
                "first_layer_extrusion_width",
                Form::OrPercent("first_layer_height")
            ),
            d!(
                C,
                "initial_layer_line_width_factor",
                list,
                Form::PercentOf("extrusion_width")
            ),
        ]
    ),
    key!(
        Process,
        "external_perimeter_extrusion_width",
        Kind::OrPercent(Unit::Mm, "nozzle_diameter"),
        "Outer wall line width",
        LineWidth,
        advanced,
        [
            d!(B | Q, "outer_wall_line_width", Form::Absolute),
            d!(O, "outer_wall_line_width"),
            d!(
                P,
                "external_perimeter_extrusion_width",
                Form::OrPercent("layer_height")
            ),
            d!(C, "wall_line_width_0", Form::Absolute),
        ]
    ),
    key!(
        Process,
        "perimeter_extrusion_width",
        Kind::OrPercent(Unit::Mm, "nozzle_diameter"),
        "Inner wall line width",
        LineWidth,
        advanced,
        [
            d!(B | Q, "inner_wall_line_width", Form::Absolute),
            d!(O, "inner_wall_line_width"),
            d!(
                P,
                "perimeter_extrusion_width",
                Form::OrPercent("layer_height")
            ),
            d!(C, "wall_line_width_x", Form::Absolute),
        ]
    ),
    key!(
        Process,
        "top_infill_extrusion_width",
        Kind::OrPercent(Unit::Mm, "nozzle_diameter"),
        "Top surface line width",
        LineWidth,
        advanced,
        [
            d!(B | Q, "top_surface_line_width", Form::Absolute),
            d!(O, "top_surface_line_width"),
            d!(
                P,
                "top_infill_extrusion_width",
                Form::OrPercent("layer_height")
            ),
            d!(C, "roofing_line_width", Form::Absolute),
        ]
    ),
    key!(
        Process,
        "infill_extrusion_width",
        Kind::OrPercent(Unit::Mm, "nozzle_diameter"),
        "Sparse infill line width",
        LineWidth,
        advanced,
        [
            d!(B | Q, "sparse_infill_line_width", Form::Absolute),
            d!(O, "sparse_infill_line_width"),
            d!(P, "infill_extrusion_width", Form::OrPercent("layer_height")),
            d!(C, "infill_line_width", Form::Absolute),
        ]
    ),
    key!(
        Process,
        "solid_infill_extrusion_width",
        Kind::OrPercent(Unit::Mm, "nozzle_diameter"),
        "Solid infill line width",
        LineWidth,
        advanced,
        [
            d!(B | Q, "internal_solid_infill_line_width", Form::Absolute),
            d!(O, "internal_solid_infill_line_width"),
            d!(
                P,
                "solid_infill_extrusion_width",
                Form::OrPercent("layer_height")
            ),
            d!(C, "skin_line_width", Form::Absolute),
        ]
    ),
    key!(
        Process,
        "support_material_extrusion_width",
        Kind::OrPercent(Unit::Mm, "nozzle_diameter"),
        "Support line width",
        LineWidth,
        advanced,
        [
            d!(B | Q, "support_line_width", Form::Absolute),
            d!(O, "support_line_width"),
            d!(
                P,
                "support_material_extrusion_width",
                Form::OrPercent("layer_height")
            ),
            d!(C, "support_line_width", list, Form::Absolute),
        ]
    ),
    key!(
        Process,
        "seam_position",
        Kind::Choice(&[
            ("nearest", "Nearest"),
            ("aligned", "Aligned"),
            ("rear", "Rear"),
            ("random", "Random"),
            ("aligned_back", "Aligned back")
        ]),
        "Seam position",
        Seam,
        common,
        [
            d!(
                B | Q,
                "seam_position",
                Form::Choices(&[
                    ("nearest", "nearest"),
                    ("aligned", "aligned"),
                    ("rear", "back"),
                    ("random", "random")
                ])
            ),
            d!(
                O,
                "seam_position",
                Form::Choices(&[
                    ("nearest", "nearest"),
                    ("aligned", "aligned"),
                    ("aligned_back", "aligned_back"),
                    ("rear", "back"),
                    ("random", "random")
                ])
            ),
            d!(
                P,
                "seam_position",
                Form::Choices(&[
                    ("random", "random"),
                    ("nearest", "nearest"),
                    ("aligned", "aligned"),
                    ("rear", "rear")
                ])
            ),
            d!(
                C,
                "z_seam_type",
                Form::Choices(&[
                    ("rear", "back"),
                    ("random", "random"),
                    ("nearest", "shortest")
                ])
            ),
        ]
    ),
    key!(
        Process,
        "scarf_seam_placement",
        Kind::Choice(&[
            ("nowhere", "None"),
            ("contours", "Contours"),
            ("everywhere", "Everywhere")
        ]),
        "Scarf seams",
        Seam,
        common,
        [
            d!(
                B | O | Q,
                "seam_slope_type",
                Form::Choices(&[
                    ("nowhere", "none"),
                    ("contours", "external"),
                    ("everywhere", "all")
                ])
            ),
            d!(P, "scarf_seam_placement"),
        ]
    ),
    key!(
        Process,
        "staggered_inner_seams",
        Kind::Boolean,
        "Staggered inner seams",
        Seam,
        advanced,
        [
            d!(O, "staggered_inner_seams"),
            d!(P, "staggered_inner_seams"),
        ]
    ),
    key!(
        Process,
        "seam_gap_distance",
        Kind::OrPercent(Unit::Mm, "nozzle_diameter"),
        "Seam gap",
        Seam,
        advanced,
        [
            d!(B | Q, "seam_gap", Form::PercentOf("nozzle_diameter")),
            d!(O, "seam_gap", Form::OrPercent("nozzle_diameter")),
            d!(P, "seam_gap_distance", Form::OrPercent("nozzle_diameter")),
        ]
    ),
    key!(
        Process,
        "seam_placement_away_from_overhangs",
        Kind::Boolean,
        "Seams away from overhangs",
        Seam,
        advanced,
        [d!(B | Q, "seam_placement_away_from_overhangs"),]
    ),
    key!(
        Process,
        "scarf_seam_only_on_smooth",
        Kind::Boolean,
        "Scarf seams only on smooth walls",
        Seam,
        advanced,
        [
            d!(B | O | Q, "seam_slope_conditional"),
            d!(P, "scarf_seam_only_on_smooth"),
        ]
    ),
    key!(
        Process,
        "scarf_angle_threshold",
        Kind::Integer(Unit::Degree),
        "Scarf angle threshold",
        Seam,
        advanced,
        [d!(B | O | Q, "scarf_angle_threshold"),]
    ),
    key!(
        Process,
        "scarf_seam_start_height",
        Kind::OrPercent(Unit::Mm, "layer_height"),
        "Scarf start height",
        Seam,
        advanced,
        [
            d!(B | O | Q, "seam_slope_start_height"),
            d!(
                P,
                "scarf_seam_start_height",
                Form::PercentOf("layer_height")
            ),
            d!(
                C,
                "scarf_joint_seam_start_height_ratio",
                list,
                Form::PercentOf("layer_height")
            ),
        ]
    ),
    key!(
        Process,
        "scarf_seam_entire_loop",
        Kind::Boolean,
        "Scarf the entire loop",
        Seam,
        advanced,
        [
            d!(B | O | Q, "seam_slope_entire_loop"),
            d!(P, "scarf_seam_entire_loop"),
        ]
    ),
    key!(
        Process,
        "scarf_seam_length",
        Kind::Number(Unit::Mm),
        "Scarf length",
        Seam,
        advanced,
        [
            d!(B | O | Q, "seam_slope_min_length"),
            d!(P, "scarf_seam_length"),
            d!(C, "scarf_joint_seam_length", list),
        ]
    ),
    key!(
        Process,
        "seam_slope_steps",
        Kind::Integer(Unit::None),
        "Scarf steps",
        Seam,
        advanced,
        [d!(B | O | Q, "seam_slope_steps"),]
    ),
    key!(
        Process,
        "seam_slope_gap",
        Kind::OrPercent(Unit::Mm, "nozzle_diameter"),
        "Scarf seam gap",
        Seam,
        advanced,
        [d!(B | Q, "seam_slope_gap"),]
    ),
    key!(
        Process,
        "scarf_seam_on_inner_perimeters",
        Kind::Boolean,
        "Scarf seams on inner walls",
        Seam,
        advanced,
        [
            d!(B | O | Q, "seam_slope_inner_walls"),
            d!(P, "scarf_seam_on_inner_perimeters"),
        ]
    ),
    key!(
        Process,
        "apply_scarf_seam_on_circles",
        Kind::Boolean,
        "Scarf seams on circles",
        Seam,
        advanced,
        [d!(B | Q, "apply_scarf_seam_on_circles"),]
    ),
    key!(
        Process,
        "override_filament_scarf_seam_setting",
        Kind::Boolean,
        "Process overrides filament scarf seams",
        Seam,
        advanced,
        [d!(B | Q, "override_filament_scarf_seam_setting"),]
    ),
    key!(
        Process,
        "wipe_speed",
        Kind::OrPercent(Unit::MmPerS, "travel_speed"),
        "Wipe speed",
        Seam,
        advanced,
        [
            d!(B | Q, "wipe_speed", Form::PercentOf("travel_speed")),
            d!(O, "wipe_speed"),
        ]
    ),
    key!(
        Process,
        "role_base_wipe_speed",
        Kind::Boolean,
        "Wipe speed by extrusion role",
        Seam,
        advanced,
        [
            d!(B | Q, "role_base_wipe_speed"),
            d!(O, "role_based_wipe_speed"),
        ]
    ),
    key!(
        Process,
        "xy_size_compensation",
        Kind::Number(Unit::Mm),
        "Outline compensation",
        Precision,
        common,
        [
            d!(B | O | Q, "xy_contour_compensation"),
            d!(P, "xy_size_compensation"),
            d!(C, "xy_offset"),
        ]
    ),
    key!(
        Process,
        "xy_hole_compensation",
        Kind::Number(Unit::Mm),
        "Hole compensation",
        Precision,
        common,
        [
            d!(B | O | Q, "xy_hole_compensation"),
            d!(C, "hole_xy_offset"),
        ]
    ),
    key!(
        Process,
        "elefant_foot_compensation",
        Kind::Number(Unit::Mm),
        "Elephant foot compensation",
        Precision,
        common,
        [
            d!(B | O | Q, "elefant_foot_compensation"),
            d!(P, "elefant_foot_compensation"),
            d!(C, "xy_offset_layer_0", Form::Linear(-1.0, 0.0)),
        ]
    ),
    key!(
        Process,
        "resolution",
        Kind::Number(Unit::Mm),
        "Model resolution",
        Precision,
        advanced,
        [d!(B | O | Q, "resolution"), d!(P, "resolution"),]
    ),
    key!(
        Process,
        "slice_closing_radius",
        Kind::Number(Unit::Mm),
        "Slice gap closing radius",
        Precision,
        advanced,
        [
            d!(B | O | Q, "slice_closing_radius"),
            d!(P, "slice_closing_radius"),
        ]
    ),
    key!(
        Process,
        "arc_fitting",
        Kind::Choice(&[("disabled", "Off"), ("emit_center", "Arcs")]),
        "Arc fitting",
        Precision,
        advanced,
        [
            d!(
                B | Q,
                "enable_arc_fitting",
                Form::Choices(&[("disabled", "0"), ("emit_center", "1")])
            ),
            d!(
                O,
                "enable_arc_fitting",
                Form::Choices(&[("disabled", "disabled"), ("emit_center", "enabled")])
            ),
            d!(P, "arc_fitting"),
        ]
    ),
    key!(
        Process,
        "precise_outer_wall",
        Kind::Boolean,
        "Precise outer wall",
        Precision,
        advanced,
        [d!(B | O | Q, "precise_outer_wall"),]
    ),
    key!(
        Process,
        "precise_z_height",
        Kind::Boolean,
        "Precise Z height",
        Precision,
        advanced,
        [d!(B | O | Q, "precise_z_height"),]
    ),
    key!(
        Process,
        "enable_circle_compensation",
        Kind::Boolean,
        "Circle compensation",
        Precision,
        advanced,
        [d!(B | Q, "enable_circle_compensation"),]
    ),
    key!(
        Process,
        "circle_compensation_manual_offset",
        Kind::Number(Unit::Mm),
        "Circle compensation offset",
        Precision,
        advanced,
        [d!(B | Q, "circle_compensation_manual_offset"),]
    ),
    key!(
        Process,
        "wall_maximum_resolution",
        Kind::Number(Unit::Mm),
        "Wall resolution",
        Precision,
        advanced,
        [
            d!(O, "wall_maximum_resolution"),
            d!(C, "meshfix_maximum_resolution"),
        ]
    ),
    key!(
        Process,
        "wall_maximum_deviation",
        Kind::Number(Unit::Mm),
        "Wall deviation",
        Precision,
        advanced,
        [
            d!(O, "wall_maximum_deviation"),
            d!(C, "meshfix_maximum_deviation"),
        ]
    ),
    key!(
        Process,
        "ironing_type",
        Kind::Choice(&[
            ("no_ironing", "None"),
            ("top", "Top surfaces"),
            ("topmost", "Topmost surface"),
            ("solid", "All solid layers")
        ]),
        "Ironing",
        Ironing,
        common,
        [
            d!(
                B | O | Q,
                "ironing_type",
                Form::Choices(&[
                    ("no_ironing", "no ironing"),
                    ("top", "top"),
                    ("topmost", "topmost"),
                    ("solid", "solid")
                ])
            ),
            d!(
                P,
                "ironing_type",
                Form::Switch {
                    key: "ironing",
                    off: "no_ironing",
                    choices: &[]
                }
            ),
            d!(
                C,
                "ironing_only_highest_layer",
                Form::Switch {
                    key: "ironing_enabled",
                    off: "no_ironing",
                    choices: &[("top", "False"), ("topmost", "True")]
                }
            ),
        ]
    ),
    key!(
        Process,
        "ironing_speed",
        Kind::Number(Unit::MmPerS),
        "Ironing speed",
        Ironing,
        common,
        [
            d!(B | O | Q, "ironing_speed"),
            d!(P, "ironing_speed"),
            d!(C, "speed_ironing"),
        ]
    ),
    key!(
        Process,
        "ironing_flowrate",
        Kind::Percent,
        "Ironing flow",
        Ironing,
        common,
        [
            d!(B | O | Q, "ironing_flow"),
            d!(P, "ironing_flowrate"),
            d!(C, "ironing_flow"),
        ]
    ),
    key!(
        Process,
        "ironing_spacing",
        Kind::Number(Unit::Mm),
        "Ironing line spacing",
        Ironing,
        common,
        [
            d!(B | O | Q, "ironing_spacing"),
            d!(P, "ironing_spacing"),
            d!(C, "ironing_line_spacing"),
        ]
    ),
    key!(
        Process,
        "ironing_pattern",
        Kind::Choice(&[
            ("concentric", "Concentric"),
            ("rectilinear", "Rectilinear"),
            ("ironingarchimedeanspiral", "Archimedean spiral")
        ]),
        "Ironing pattern",
        Ironing,
        advanced,
        [
            d!(
                B,
                "ironing_pattern",
                Form::Choices(&[
                    ("concentric", "concentric"),
                    ("rectilinear", "zig-zag"),
                    ("ironingarchimedeanspiral", "ironingarchimedeanspiral")
                ])
            ),
            d!(
                O,
                "ironing_pattern",
                Form::Choices(&[("rectilinear", "rectilinear"), ("concentric", "concentric")])
            ),
            d!(
                Q,
                "ironing_pattern",
                Form::Choices(&[("concentric", "concentric"), ("rectilinear", "zig-zag")])
            ),
            d!(
                C,
                "ironing_pattern",
                Form::Choices(&[("rectilinear", "zigzag"), ("concentric", "concentric")])
            ),
        ]
    ),
    key!(
        Process,
        "ironing_inset",
        Kind::Number(Unit::Mm),
        "Ironing inset",
        Ironing,
        advanced,
        [d!(B | O | Q, "ironing_inset"), d!(C, "ironing_inset"),]
    ),
    key!(
        Process,
        "ironing_direction",
        Kind::Number(Unit::Degree),
        "Ironing direction",
        Ironing,
        advanced,
        [d!(B | Q, "ironing_direction"), d!(O, "ironing_angle"),]
    ),
    key!(
        Process,
        "perimeter_generator",
        Kind::Choice(&[("classic", "Classic"), ("arachne", "Arachne")]),
        "Wall generator",
        WallGenerator,
        advanced,
        [
            d!(B | O | Q, "wall_generator"),
            d!(P, "perimeter_generator"),
        ]
    ),
    key!(
        Process,
        "wall_transition_length",
        Kind::OrPercent(Unit::Mm, "nozzle_diameter"),
        "Wall transition length",
        WallGenerator,
        advanced,
        [
            d!(
                B | O | Q,
                "wall_transition_length",
                Form::PercentOf("nozzle_diameter")
            ),
            d!(
                P,
                "wall_transition_length",
                Form::OrPercent("nozzle_diameter")
            ),
            d!(C, "wall_transition_length", Form::Absolute),
        ]
    ),
    key!(
        Process,
        "wall_transition_filter_deviation",
        Kind::OrPercent(Unit::Mm, "nozzle_diameter"),
        "Wall transition filter margin",
        WallGenerator,
        advanced,
        [
            d!(
                B | O | Q,
                "wall_transition_filter_deviation",
                Form::PercentOf("nozzle_diameter")
            ),
            d!(
                P,
                "wall_transition_filter_deviation",
                Form::OrPercent("nozzle_diameter")
            ),
            d!(C, "wall_transition_filter_deviation", Form::Absolute),
        ]
    ),
    key!(
        Process,
        "wall_transition_angle",
        Kind::Number(Unit::Degree),
        "Wall transition angle",
        WallGenerator,
        advanced,
        [
            d!(B | O | Q, "wall_transition_angle"),
            d!(P, "wall_transition_angle"),
            d!(C, "wall_transition_angle"),
        ]
    ),
    key!(
        Process,
        "wall_distribution_count",
        Kind::Integer(Unit::None),
        "Wall distribution count",
        WallGenerator,
        advanced,
        [
            d!(B | O | Q, "wall_distribution_count"),
            d!(P, "wall_distribution_count"),
            d!(C, "wall_distribution_count"),
        ]
    ),
    key!(
        Process,
        "min_feature_size",
        Kind::OrPercent(Unit::Mm, "nozzle_diameter"),
        "Smallest feature",
        WallGenerator,
        advanced,
        [
            d!(
                B | O | Q,
                "min_feature_size",
                Form::PercentOf("nozzle_diameter")
            ),
            d!(P, "min_feature_size", Form::OrPercent("nozzle_diameter")),
            d!(C, "min_feature_size", Form::Absolute),
        ]
    ),
    key!(
        Process,
        "min_bead_width",
        Kind::OrPercent(Unit::Mm, "nozzle_diameter"),
        "Narrowest wall",
        WallGenerator,
        advanced,
        [
            d!(
                B | O | Q,
                "min_bead_width",
                Form::PercentOf("nozzle_diameter")
            ),
            d!(P, "min_bead_width", Form::OrPercent("nozzle_diameter")),
            d!(C, "min_bead_width", Form::Absolute),
        ]
    ),
    key!(
        Process,
        "wall_sequence",
        Kind::Choice(&[
            ("inner_outer", "Inner, then outer"),
            ("outer_inner", "Outer, then inner"),
            ("inner_outer_inner", "Inner, outer, then inner")
        ]),
        "Wall order",
        OrderAndFlow,
        common,
        [
            d!(
                B | O | Q,
                "wall_sequence",
                Form::Choices(&[
                    ("inner_outer", "inner wall/outer wall"),
                    ("outer_inner", "outer wall/inner wall"),
                    ("inner_outer_inner", "inner-outer-inner wall")
                ])
            ),
            d!(
                P,
                "external_perimeters_first",
                Form::Choices(&[("inner_outer", "0"), ("outer_inner", "1")])
            ),
            d!(
                C,
                "inset_direction",
                Form::Choices(&[("inner_outer", "inside_out"), ("outer_inner", "outside_in")])
            ),
        ]
    ),
    key!(
        Process,
        "infill_first",
        Kind::Boolean,
        "Infill before walls",
        OrderAndFlow,
        advanced,
        [
            d!(B | O | Q, "is_infill_first"),
            d!(P, "infill_first"),
            d!(C, "infill_before_walls"),
        ]
    ),
    key!(
        Process,
        "print_flow_ratio",
        Kind::Number(Unit::None),
        "Object flow ratio",
        OrderAndFlow,
        advanced,
        [d!(B | O | Q, "print_flow_ratio"),]
    ),
    key!(
        Process,
        "initial_layer_flow_ratio",
        Kind::Number(Unit::None),
        "First layer flow",
        OrderAndFlow,
        advanced,
        [
            d!(B | Q, "initial_layer_flow_ratio"),
            d!(O, "first_layer_flow_ratio"),
            d!(C, "material_flow_layer_0", Form::Linear(100.0, 0.0)),
        ]
    ),
    key!(
        Process,
        "outer_wall_flow_ratio",
        Kind::Number(Unit::None),
        "Outer wall flow",
        OrderAndFlow,
        advanced,
        [
            d!(O, "outer_wall_flow_ratio"),
            d!(C, "wall_0_material_flow", Form::Linear(100.0, 0.0)),
        ]
    ),
    key!(
        Process,
        "inner_wall_flow_ratio",
        Kind::Number(Unit::None),
        "Inner wall flow",
        OrderAndFlow,
        advanced,
        [
            d!(O, "inner_wall_flow_ratio"),
            d!(C, "wall_x_material_flow", Form::Linear(100.0, 0.0)),
        ]
    ),
    key!(
        Process,
        "top_solid_infill_flow_ratio",
        Kind::Number(Unit::None),
        "Top surface flow",
        OrderAndFlow,
        advanced,
        [
            d!(B | O | Q, "top_solid_infill_flow_ratio", list),
            d!(C, "roofing_material_flow", Form::Linear(100.0, 0.0)),
        ]
    ),
    key!(
        Process,
        "bottom_solid_infill_flow_ratio",
        Kind::Number(Unit::None),
        "Bottom surface flow",
        OrderAndFlow,
        advanced,
        [
            d!(O, "bottom_solid_infill_flow_ratio"),
            d!(C, "flooring_material_flow", Form::Linear(100.0, 0.0)),
        ]
    ),
    key!(
        Process,
        "sparse_infill_flow_ratio",
        Kind::Number(Unit::None),
        "Sparse infill flow",
        OrderAndFlow,
        advanced,
        [
            d!(O, "sparse_infill_flow_ratio"),
            d!(C, "infill_material_flow", Form::Linear(100.0, 0.0)),
        ]
    ),
    key!(
        Process,
        "support_flow_ratio",
        Kind::Number(Unit::None),
        "Support flow",
        OrderAndFlow,
        advanced,
        [
            d!(O, "support_flow_ratio"),
            d!(C, "support_material_flow", list, Form::Linear(100.0, 0.0)),
        ]
    ),
    key!(
        Process,
        "support_interface_flow_ratio",
        Kind::Number(Unit::None),
        "Support interface flow",
        OrderAndFlow,
        advanced,
        [
            d!(O, "support_interface_flow_ratio"),
            d!(
                C,
                "support_interface_material_flow",
                list,
                Form::Linear(100.0, 0.0)
            ),
        ]
    ),
    key!(
        Process,
        "overhangs",
        Kind::Boolean,
        "Detect overhanging walls",
        BridgesAndOverhangs,
        advanced,
        [d!(B | O | Q, "detect_overhang_wall"), d!(P, "overhangs"),]
    ),
    key!(
        Process,
        "bridge_flow_ratio",
        Kind::Number(Unit::None),
        "Bridge flow",
        BridgesAndOverhangs,
        advanced,
        [
            d!(B | O | Q, "bridge_flow"),
            d!(P, "bridge_flow_ratio"),
            d!(C, "bridge_skin_material_flow", Form::Linear(100.0, 0.0)),
        ]
    ),
    key!(
        Process,
        "bridge_density",
        Kind::Percent,
        "Bridge density",
        BridgesAndOverhangs,
        advanced,
        [d!(O, "bridge_density"), d!(C, "bridge_skin_density"),]
    ),
    key!(
        Process,
        "thick_bridges",
        Kind::Boolean,
        "Thick bridges",
        BridgesAndOverhangs,
        advanced,
        [d!(B | O | Q, "thick_bridges"), d!(P, "thick_bridges"),]
    ),
    key!(
        Process,
        "counterbore_hole_bridging",
        Kind::Choice(&[
            ("none", "None"),
            ("partiallybridge", "Partial bridge"),
            ("sacrificiallayer", "Sacrificial layer")
        ]),
        "Counterbore hole bridging",
        BridgesAndOverhangs,
        advanced,
        [d!(B | O, "counterbore_hole_bridging"),]
    ),
    key!(
        Process,
        "extra_perimeters_on_overhangs",
        Kind::Boolean,
        "Extra walls on overhangs",
        BridgesAndOverhangs,
        advanced,
        [
            d!(O, "extra_perimeters_on_overhangs"),
            d!(P, "extra_perimeters_on_overhangs"),
        ]
    ),
    key!(
        Process,
        "make_overhang_printable",
        Kind::Boolean,
        "Make overhangs printable",
        BridgesAndOverhangs,
        advanced,
        [
            d!(O, "make_overhang_printable"),
            d!(C, "conical_overhang_enabled"),
        ]
    ),
    key!(
        Process,
        "make_overhang_printable_angle",
        Kind::Number(Unit::Degree),
        "Printable overhang angle",
        BridgesAndOverhangs,
        advanced,
        [
            d!(O, "make_overhang_printable_angle"),
            d!(C, "conical_overhang_angle"),
        ]
    ),
    key!(
        Process,
        "make_overhang_printable_hole_size",
        Kind::Number(Unit::Mm2),
        "Printable overhang hole size",
        BridgesAndOverhangs,
        advanced,
        [
            d!(O, "make_overhang_printable_hole_size"),
            d!(C, "conical_overhang_hole_size"),
        ]
    ),
    key!(
        Process,
        "avoid_crossing_perimeters",
        Kind::Boolean,
        "Avoid crossing walls",
        Travel,
        advanced,
        [
            d!(B | O | Q, "reduce_crossing_wall"),
            d!(P, "avoid_crossing_perimeters"),
        ]
    ),
    key!(
        Process,
        "avoid_crossing_perimeters_max_detour",
        Kind::OrPercent(Unit::Mm, ""),
        "Longest detour around walls",
        Travel,
        advanced,
        [
            d!(B | O | Q, "max_travel_detour_distance"),
            d!(P, "avoid_crossing_perimeters_max_detour"),
        ]
    ),
    key!(
        Process,
        "avoid_crossing_wall_includes_support",
        Kind::Boolean,
        "Avoid crossing supports too",
        Travel,
        advanced,
        [d!(B | Q, "avoid_crossing_wall_includes_support"),]
    ),
    key!(
        Process,
        "monotonic_travel_into_wall",
        Kind::Percent,
        "Monotonic travel into walls",
        Travel,
        advanced,
        [d!(B | Q, "monotonic_travel_into_wall"),]
    ),
    key!(
        Process,
        "perimeters",
        Kind::Integer(Unit::None),
        "Walls",
        Walls,
        common,
        [
            d!(B | O | Q, "wall_loops"),
            d!(P, "perimeters"),
            d!(C, "wall_line_count"),
        ]
    ),
    key!(
        Process,
        "thin_walls",
        Kind::Boolean,
        "Thin walls",
        Walls,
        common,
        [
            d!(B | O | Q, "detect_thin_wall"),
            d!(P, "thin_walls"),
            d!(C, "fill_outline_gaps"),
        ]
    ),
    key!(
        Process,
        "alternate_extra_wall",
        Kind::Boolean,
        "Alternate extra wall",
        Walls,
        advanced,
        [
            d!(B | O | Q, "alternate_extra_wall"),
            d!(C, "alternate_extra_perimeter"),
        ]
    ),
    key!(
        Process,
        "embedding_wall_into_infill",
        Kind::Boolean,
        "Embed walls into infill",
        Walls,
        advanced,
        [d!(B | Q, "embedding_wall_into_infill"),]
    ),
    key!(
        Process,
        "only_one_perimeter_first_layer",
        Kind::Boolean,
        "One wall on the first layer",
        Walls,
        advanced,
        [
            d!(B | O | Q, "only_one_wall_first_layer"),
            d!(P, "only_one_perimeter_first_layer"),
        ]
    ),
    key!(
        Process,
        "top_one_perimeter_type",
        Kind::Choice(&[
            ("none", "None"),
            ("top", "Top surfaces"),
            ("topmost", "Topmost surface")
        ]),
        "One wall on top surfaces",
        Walls,
        advanced,
        [
            d!(
                B | Q,
                "top_one_wall_type",
                Form::Choices(&[
                    ("none", "not apply"),
                    ("top", "all top"),
                    ("topmost", "topmost")
                ])
            ),
            d!(P, "top_one_perimeter_type"),
        ]
    ),
    key!(
        Process,
        "top_area_threshold",
        Kind::Percent,
        "Smallest top area for one wall",
        Walls,
        advanced,
        [d!(B | Q, "top_area_threshold"),]
    ),
    key!(
        Process,
        "ensure_vertical_shell_thickness",
        Kind::Choice(&[
            ("disabled", "Off"),
            ("partial", "Partial"),
            ("enabled", "On"),
            ("none", "None"),
            ("ensure_critical_only", "Critical areas only"),
            ("ensure_moderate", "Moderate"),
            ("ensure_all", "All")
        ]),
        "Ensure vertical shell thickness",
        Walls,
        advanced,
        [
            d!(
                B | Q,
                "ensure_vertical_shell_thickness",
                Form::Choices(&[
                    ("disabled", "disabled"),
                    ("partial", "partial"),
                    ("enabled", "enabled")
                ])
            ),
            d!(
                O,
                "ensure_vertical_shell_thickness",
                Form::Choices(&[
                    ("none", "none"),
                    ("ensure_critical_only", "ensure_critical_only"),
                    ("ensure_moderate", "ensure_moderate"),
                    ("ensure_all", "ensure_all")
                ])
            ),
            d!(
                P,
                "ensure_vertical_shell_thickness",
                Form::Choices(&[
                    ("disabled", "disabled"),
                    ("partial", "partial"),
                    ("enabled", "enabled")
                ])
            ),
        ]
    ),
    key!(
        Process,
        "detect_floating_vertical_shell",
        Kind::Boolean,
        "Detect floating vertical shells",
        Walls,
        advanced,
        [d!(B | Q, "detect_floating_vertical_shell"),]
    ),
    key!(
        Process,
        "top_solid_layers",
        Kind::Integer(Unit::None),
        "Top solid layers",
        TopAndBottomShells,
        common,
        [
            d!(B | O | Q, "top_shell_layers"),
            d!(P, "top_solid_layers"),
            d!(C, "top_layers"),
        ]
    ),
    key!(
        Process,
        "top_solid_min_thickness",
        Kind::Number(Unit::Mm),
        "Top shell thickness",
        TopAndBottomShells,
        common,
        [
            d!(B | O | Q, "top_shell_thickness"),
            d!(P, "top_solid_min_thickness"),
            d!(C, "top_thickness"),
        ]
    ),
    key!(
        Process,
        "top_fill_pattern",
        Kind::Choice(&[
            ("concentric", "Concentric"),
            ("rectilinear", "Rectilinear"),
            ("monotonic", "Monotonic"),
            ("monotonicline", "Monotonic line"),
            ("globalmonotonicline", "Global monotonic line"),
            ("alignedrectilinear", "Aligned rectilinear"),
            ("hilbertcurve", "Hilbert curve"),
            ("archimedeanchords", "Archimedean chords"),
            ("octagramspiral", "Octagram spiral"),
            ("spiralinset", "Spiral inset"),
            ("monotoniclines", "Monotonic lines")
        ]),
        "Top surface pattern",
        TopAndBottomShells,
        common,
        [
            d!(
                B,
                "top_surface_pattern",
                Form::Choices(&[
                    ("concentric", "concentric"),
                    ("rectilinear", "zig-zag"),
                    ("monotonic", "monotonic"),
                    ("monotonicline", "monotonicline"),
                    ("globalmonotonicline", "globalmonotonicline"),
                    ("alignedrectilinear", "alignedrectilinear"),
                    ("hilbertcurve", "hilbertcurve"),
                    ("archimedeanchords", "archimedeanchords"),
                    ("octagramspiral", "octagramspiral")
                ])
            ),
            d!(
                O,
                "top_surface_pattern",
                Form::Choices(&[
                    ("monotonic", "monotonic"),
                    ("monotonicline", "monotonicline"),
                    ("rectilinear", "rectilinear"),
                    ("alignedrectilinear", "alignedrectilinear"),
                    ("concentric", "concentric"),
                    ("spiralinset", "spiralinset"),
                    ("hilbertcurve", "hilbertcurve"),
                    ("archimedeanchords", "archimedeanchords"),
                    ("octagramspiral", "octagramspiral")
                ])
            ),
            d!(
                Q,
                "top_surface_pattern",
                Form::Choices(&[
                    ("concentric", "concentric"),
                    ("rectilinear", "zig-zag"),
                    ("monotonic", "monotonic"),
                    ("monotonicline", "monotonicline"),
                    ("alignedrectilinear", "alignedrectilinear"),
                    ("hilbertcurve", "hilbertcurve"),
                    ("archimedeanchords", "archimedeanchords"),
                    ("octagramspiral", "octagramspiral")
                ])
            ),
            d!(
                P,
                "top_fill_pattern",
                Form::Choices(&[
                    ("rectilinear", "rectilinear"),
                    ("monotonic", "monotonic"),
                    ("monotoniclines", "monotoniclines"),
                    ("alignedrectilinear", "alignedrectilinear"),
                    ("concentric", "concentric"),
                    ("hilbertcurve", "hilbertcurve"),
                    ("archimedeanchords", "archimedeanchords"),
                    ("octagramspiral", "octagramspiral")
                ])
            ),
            d!(
                C,
                "top_bottom_pattern",
                Form::Choices(&[("rectilinear", "lines"), ("concentric", "concentric")])
            ),
        ]
    ),
    key!(
        Process,
        "bottom_solid_layers",
        Kind::Integer(Unit::None),
        "Bottom solid layers",
        TopAndBottomShells,
        common,
        [
            d!(B | O | Q, "bottom_shell_layers"),
            d!(P, "bottom_solid_layers"),
            d!(C, "bottom_layers"),
        ]
    ),
    key!(
        Process,
        "bottom_solid_min_thickness",
        Kind::Number(Unit::Mm),
        "Bottom shell thickness",
        TopAndBottomShells,
        common,
        [
            d!(B | O | Q, "bottom_shell_thickness"),
            d!(P, "bottom_solid_min_thickness"),
            d!(C, "bottom_thickness"),
        ]
    ),
    key!(
        Process,
        "bottom_fill_pattern",
        Kind::Choice(&[
            ("concentric", "Concentric"),
            ("rectilinear", "Rectilinear"),
            ("grid", "Grid"),
            ("line", "Line"),
            ("cubic", "Cubic"),
            ("triangles", "Triangles"),
            ("stars", "Stars"),
            ("gyroid", "Gyroid"),
            ("honeycomb", "Honeycomb"),
            ("adaptivecubic", "Adaptive cubic"),
            ("monotonic", "Monotonic"),
            ("monotonicline", "Monotonic line"),
            ("alignedrectilinear", "Aligned rectilinear"),
            ("3dhoneycomb", "3D honeycomb"),
            ("hilbertcurve", "Hilbert curve"),
            ("archimedeanchords", "Archimedean chords"),
            ("octagramspiral", "Octagram spiral"),
            ("supportcubic", "Support cubic"),
            ("lightning", "Lightning"),
            ("crosshatch", "Cross hatch"),
            ("zigzag", "Zig-zag"),
            ("crosszag", "Cross zag"),
            ("lockedzag", "Locked zag"),
            ("2dlattice", "2D lattice"),
            ("ironingarchimedeanspiral", "Archimedean spiral"),
            ("globalmonotonicline", "Global monotonic line"),
            ("quartercubic", "Quarter cubic"),
            ("lateral-honeycomb", "Lateral honeycomb"),
            ("lateral-lattice", "Lateral lattice"),
            ("tpmsd", "TPMS-D"),
            ("tpmsfk", "TPMS-FK"),
            ("spiralinset", "Spiral inset"),
            ("monotoniclines", "Monotonic lines")
        ]),
        "Bottom surface pattern",
        TopAndBottomShells,
        common,
        [
            d!(
                B,
                "bottom_surface_pattern",
                Form::Choices(&[
                    ("concentric", "concentric"),
                    ("rectilinear", "zig-zag"),
                    ("grid", "grid"),
                    ("line", "line"),
                    ("cubic", "cubic"),
                    ("triangles", "triangles"),
                    ("stars", "tri-hexagon"),
                    ("gyroid", "gyroid"),
                    ("honeycomb", "honeycomb"),
                    ("adaptivecubic", "adaptivecubic"),
                    ("monotonic", "monotonic"),
                    ("monotonicline", "monotonicline"),
                    ("alignedrectilinear", "alignedrectilinear"),
                    ("3dhoneycomb", "3dhoneycomb"),
                    ("hilbertcurve", "hilbertcurve"),
                    ("archimedeanchords", "archimedeanchords"),
                    ("octagramspiral", "octagramspiral"),
                    ("supportcubic", "supportcubic"),
                    ("lightning", "lightning"),
                    ("crosshatch", "crosshatch"),
                    ("zigzag", "zigzag"),
                    ("crosszag", "crosszag"),
                    ("lockedzag", "lockedzag"),
                    ("2dlattice", "2dlattice"),
                    ("ironingarchimedeanspiral", "ironingarchimedeanspiral"),
                    ("globalmonotonicline", "globalmonotonicline")
                ])
            ),
            d!(
                O,
                "bottom_surface_pattern",
                Form::Choices(&[
                    ("monotonic", "monotonic"),
                    ("monotonicline", "monotonicline"),
                    ("rectilinear", "rectilinear"),
                    ("alignedrectilinear", "alignedrectilinear"),
                    ("zigzag", "zigzag"),
                    ("crosszag", "crosszag"),
                    ("lockedzag", "lockedzag"),
                    ("line", "line"),
                    ("grid", "grid"),
                    ("triangles", "triangles"),
                    ("stars", "tri-hexagon"),
                    ("cubic", "cubic"),
                    ("adaptivecubic", "adaptivecubic"),
                    ("quartercubic", "quartercubic"),
                    ("supportcubic", "supportcubic"),
                    ("lightning", "lightning"),
                    ("honeycomb", "honeycomb"),
                    ("3dhoneycomb", "3dhoneycomb"),
                    ("lateral-honeycomb", "lateral-honeycomb"),
                    ("lateral-lattice", "lateral-lattice"),
                    ("crosshatch", "crosshatch"),
                    ("tpmsd", "tpmsd"),
                    ("tpmsfk", "tpmsfk"),
                    ("gyroid", "gyroid"),
                    ("concentric", "concentric"),
                    ("spiralinset", "spiralinset"),
                    ("hilbertcurve", "hilbertcurve"),
                    ("archimedeanchords", "archimedeanchords"),
                    ("octagramspiral", "octagramspiral")
                ])
            ),
            d!(
                Q,
                "bottom_surface_pattern",
                Form::Choices(&[
                    ("concentric", "concentric"),
                    ("rectilinear", "zig-zag"),
                    ("grid", "grid"),
                    ("line", "line"),
                    ("cubic", "cubic"),
                    ("triangles", "triangles"),
                    ("stars", "tri-hexagon"),
                    ("gyroid", "gyroid"),
                    ("honeycomb", "honeycomb"),
                    ("adaptivecubic", "adaptivecubic"),
                    ("monotonic", "monotonic"),
                    ("monotonicline", "monotonicline"),
                    ("alignedrectilinear", "alignedrectilinear"),
                    ("3dhoneycomb", "3dhoneycomb"),
                    ("hilbertcurve", "hilbertcurve"),
                    ("archimedeanchords", "archimedeanchords"),
                    ("octagramspiral", "octagramspiral"),
                    ("supportcubic", "supportcubic"),
                    ("lightning", "lightning"),
                    ("crosshatch", "crosshatch"),
                    ("zigzag", "zigzag"),
                    ("crosszag", "crosszag"),
                    ("lockedzag", "lockedzag"),
                    ("2dlattice", "2dlattice")
                ])
            ),
            d!(
                P,
                "bottom_fill_pattern",
                Form::Choices(&[
                    ("rectilinear", "rectilinear"),
                    ("monotonic", "monotonic"),
                    ("monotoniclines", "monotoniclines"),
                    ("alignedrectilinear", "alignedrectilinear"),
                    ("grid", "grid"),
                    ("triangles", "triangles"),
                    ("stars", "stars"),
                    ("cubic", "cubic"),
                    ("line", "line"),
                    ("concentric", "concentric"),
                    ("honeycomb", "honeycomb"),
                    ("3dhoneycomb", "3dhoneycomb"),
                    ("gyroid", "gyroid"),
                    ("hilbertcurve", "hilbertcurve"),
                    ("archimedeanchords", "archimedeanchords"),
                    ("octagramspiral", "octagramspiral"),
                    ("adaptivecubic", "adaptivecubic"),
                    ("supportcubic", "supportcubic"),
                    ("lightning", "lightning"),
                    ("zigzag", "zigzag")
                ])
            ),
        ]
    ),
    key!(
        Process,
        "top_surface_density",
        Kind::Percent,
        "Top surface density",
        TopAndBottomShells,
        advanced,
        [d!(B | O | Q, "top_surface_density"),]
    ),
    key!(
        Process,
        "bottom_surface_density",
        Kind::Percent,
        "Bottom surface density",
        TopAndBottomShells,
        advanced,
        [d!(B | O | Q, "bottom_surface_density"),]
    ),
    key!(
        Process,
        "top_color_penetration_layers",
        Kind::Integer(Unit::None),
        "Top color depth",
        TopAndBottomShells,
        advanced,
        [d!(B | O | Q, "top_color_penetration_layers"),]
    ),
    key!(
        Process,
        "bottom_color_penetration_layers",
        Kind::Integer(Unit::None),
        "Bottom color depth",
        TopAndBottomShells,
        advanced,
        [d!(B | O | Q, "bottom_color_penetration_layers"),]
    ),
    key!(
        Process,
        "infill_instead_top_bottom_surfaces",
        Kind::Boolean,
        "Infill instead of top and bottom surfaces",
        TopAndBottomShells,
        advanced,
        [d!(B | Q, "infill_instead_top_bottom_surfaces"),]
    ),
    key!(
        Process,
        "internal_solid_infill_pattern",
        Kind::Choice(&[
            ("concentric", "Concentric"),
            ("rectilinear", "Rectilinear"),
            ("grid", "Grid"),
            ("line", "Line"),
            ("cubic", "Cubic"),
            ("triangles", "Triangles"),
            ("stars", "Stars"),
            ("gyroid", "Gyroid"),
            ("honeycomb", "Honeycomb"),
            ("adaptivecubic", "Adaptive cubic"),
            ("monotonic", "Monotonic"),
            ("monotonicline", "Monotonic line"),
            ("alignedrectilinear", "Aligned rectilinear"),
            ("3dhoneycomb", "3D honeycomb"),
            ("hilbertcurve", "Hilbert curve"),
            ("archimedeanchords", "Archimedean chords"),
            ("octagramspiral", "Octagram spiral"),
            ("supportcubic", "Support cubic"),
            ("lightning", "Lightning"),
            ("crosshatch", "Cross hatch"),
            ("zigzag", "Zig-zag"),
            ("crosszag", "Cross zag"),
            ("lockedzag", "Locked zag"),
            ("2dlattice", "2D lattice"),
            ("ironingarchimedeanspiral", "Archimedean spiral"),
            ("globalmonotonicline", "Global monotonic line"),
            ("quartercubic", "Quarter cubic"),
            ("lateral-honeycomb", "Lateral honeycomb"),
            ("lateral-lattice", "Lateral lattice"),
            ("tpmsd", "TPMS-D"),
            ("tpmsfk", "TPMS-FK"),
            ("spiralinset", "Spiral inset")
        ]),
        "Internal solid infill pattern",
        TopAndBottomShells,
        advanced,
        [
            d!(
                B,
                "internal_solid_infill_pattern",
                Form::Choices(&[
                    ("concentric", "concentric"),
                    ("rectilinear", "zig-zag"),
                    ("grid", "grid"),
                    ("line", "line"),
                    ("cubic", "cubic"),
                    ("triangles", "triangles"),
                    ("stars", "tri-hexagon"),
                    ("gyroid", "gyroid"),
                    ("honeycomb", "honeycomb"),
                    ("adaptivecubic", "adaptivecubic"),
                    ("monotonic", "monotonic"),
                    ("monotonicline", "monotonicline"),
                    ("alignedrectilinear", "alignedrectilinear"),
                    ("3dhoneycomb", "3dhoneycomb"),
                    ("hilbertcurve", "hilbertcurve"),
                    ("archimedeanchords", "archimedeanchords"),
                    ("octagramspiral", "octagramspiral"),
                    ("supportcubic", "supportcubic"),
                    ("lightning", "lightning"),
                    ("crosshatch", "crosshatch"),
                    ("zigzag", "zigzag"),
                    ("crosszag", "crosszag"),
                    ("lockedzag", "lockedzag"),
                    ("2dlattice", "2dlattice"),
                    ("ironingarchimedeanspiral", "ironingarchimedeanspiral"),
                    ("globalmonotonicline", "globalmonotonicline")
                ])
            ),
            d!(
                O,
                "internal_solid_infill_pattern",
                Form::Choices(&[
                    ("monotonic", "monotonic"),
                    ("monotonicline", "monotonicline"),
                    ("rectilinear", "rectilinear"),
                    ("alignedrectilinear", "alignedrectilinear"),
                    ("zigzag", "zigzag"),
                    ("crosszag", "crosszag"),
                    ("lockedzag", "lockedzag"),
                    ("line", "line"),
                    ("grid", "grid"),
                    ("triangles", "triangles"),
                    ("stars", "tri-hexagon"),
                    ("cubic", "cubic"),
                    ("adaptivecubic", "adaptivecubic"),
                    ("quartercubic", "quartercubic"),
                    ("supportcubic", "supportcubic"),
                    ("lightning", "lightning"),
                    ("honeycomb", "honeycomb"),
                    ("3dhoneycomb", "3dhoneycomb"),
                    ("lateral-honeycomb", "lateral-honeycomb"),
                    ("lateral-lattice", "lateral-lattice"),
                    ("crosshatch", "crosshatch"),
                    ("tpmsd", "tpmsd"),
                    ("tpmsfk", "tpmsfk"),
                    ("gyroid", "gyroid"),
                    ("concentric", "concentric"),
                    ("spiralinset", "spiralinset"),
                    ("hilbertcurve", "hilbertcurve"),
                    ("archimedeanchords", "archimedeanchords"),
                    ("octagramspiral", "octagramspiral")
                ])
            ),
            d!(
                Q,
                "internal_solid_infill_pattern",
                Form::Choices(&[
                    ("concentric", "concentric"),
                    ("rectilinear", "zig-zag"),
                    ("grid", "grid"),
                    ("line", "line"),
                    ("cubic", "cubic"),
                    ("triangles", "triangles"),
                    ("stars", "tri-hexagon"),
                    ("gyroid", "gyroid"),
                    ("honeycomb", "honeycomb"),
                    ("adaptivecubic", "adaptivecubic"),
                    ("monotonic", "monotonic"),
                    ("monotonicline", "monotonicline"),
                    ("alignedrectilinear", "alignedrectilinear"),
                    ("3dhoneycomb", "3dhoneycomb"),
                    ("hilbertcurve", "hilbertcurve"),
                    ("archimedeanchords", "archimedeanchords"),
                    ("octagramspiral", "octagramspiral"),
                    ("supportcubic", "supportcubic"),
                    ("lightning", "lightning"),
                    ("crosshatch", "crosshatch"),
                    ("zigzag", "zigzag"),
                    ("crosszag", "crosszag"),
                    ("lockedzag", "lockedzag"),
                    ("2dlattice", "2dlattice")
                ])
            ),
        ]
    ),
    key!(
        Process,
        "interface_shells",
        Kind::Boolean,
        "Shells between materials",
        TopAndBottomShells,
        advanced,
        [d!(B | O | Q, "interface_shells"), d!(P, "interface_shells"),]
    ),
    key!(
        Process,
        "fill_density",
        Kind::Percent,
        "Sparse infill density",
        SparseInfill,
        common,
        [
            d!(B | O | Q, "sparse_infill_density"),
            d!(P, "fill_density"),
            d!(C, "infill_sparse_density"),
        ]
    ),
    key!(
        Process,
        "fill_pattern",
        Kind::Choice(&[
            ("concentric", "Concentric"),
            ("rectilinear", "Rectilinear"),
            ("grid", "Grid"),
            ("line", "Line"),
            ("cubic", "Cubic"),
            ("triangles", "Triangles"),
            ("stars", "Stars"),
            ("gyroid", "Gyroid"),
            ("honeycomb", "Honeycomb"),
            ("adaptivecubic", "Adaptive cubic"),
            ("alignedrectilinear", "Aligned rectilinear"),
            ("3dhoneycomb", "3D honeycomb"),
            ("hilbertcurve", "Hilbert curve"),
            ("archimedeanchords", "Archimedean chords"),
            ("octagramspiral", "Octagram spiral"),
            ("supportcubic", "Support cubic"),
            ("lightning", "Lightning"),
            ("crosshatch", "Cross hatch"),
            ("zigzag", "Zig-zag"),
            ("crosszag", "Cross zag"),
            ("lockedzag", "Locked zag"),
            ("2dlattice", "2D lattice"),
            ("quartercubic", "Quarter cubic"),
            ("lateral-honeycomb", "Lateral honeycomb"),
            ("lateral-lattice", "Lateral lattice"),
            ("tpmsd", "TPMS-D"),
            ("tpmsfk", "TPMS-FK")
        ]),
        "Sparse infill pattern",
        SparseInfill,
        common,
        [
            d!(
                B | Q,
                "sparse_infill_pattern",
                Form::Choices(&[
                    ("concentric", "concentric"),
                    ("rectilinear", "zig-zag"),
                    ("grid", "grid"),
                    ("line", "line"),
                    ("cubic", "cubic"),
                    ("triangles", "triangles"),
                    ("stars", "tri-hexagon"),
                    ("gyroid", "gyroid"),
                    ("honeycomb", "honeycomb"),
                    ("adaptivecubic", "adaptivecubic"),
                    ("alignedrectilinear", "alignedrectilinear"),
                    ("3dhoneycomb", "3dhoneycomb"),
                    ("hilbertcurve", "hilbertcurve"),
                    ("archimedeanchords", "archimedeanchords"),
                    ("octagramspiral", "octagramspiral"),
                    ("supportcubic", "supportcubic"),
                    ("lightning", "lightning"),
                    ("crosshatch", "crosshatch"),
                    ("zigzag", "zigzag"),
                    ("crosszag", "crosszag"),
                    ("lockedzag", "lockedzag"),
                    ("2dlattice", "2dlattice")
                ])
            ),
            d!(
                O,
                "sparse_infill_pattern",
                Form::Choices(&[
                    ("rectilinear", "rectilinear"),
                    ("alignedrectilinear", "alignedrectilinear"),
                    ("zigzag", "zigzag"),
                    ("crosszag", "crosszag"),
                    ("lockedzag", "lockedzag"),
                    ("line", "line"),
                    ("grid", "grid"),
                    ("triangles", "triangles"),
                    ("stars", "tri-hexagon"),
                    ("cubic", "cubic"),
                    ("adaptivecubic", "adaptivecubic"),
                    ("quartercubic", "quartercubic"),
                    ("supportcubic", "supportcubic"),
                    ("lightning", "lightning"),
                    ("honeycomb", "honeycomb"),
                    ("3dhoneycomb", "3dhoneycomb"),
                    ("lateral-honeycomb", "lateral-honeycomb"),
                    ("lateral-lattice", "lateral-lattice"),
                    ("crosshatch", "crosshatch"),
                    ("tpmsd", "tpmsd"),
                    ("tpmsfk", "tpmsfk"),
                    ("gyroid", "gyroid"),
                    ("concentric", "concentric"),
                    ("hilbertcurve", "hilbertcurve"),
                    ("archimedeanchords", "archimedeanchords"),
                    ("octagramspiral", "octagramspiral")
                ])
            ),
            d!(
                P,
                "fill_pattern",
                Form::Choices(&[
                    ("rectilinear", "rectilinear"),
                    ("alignedrectilinear", "alignedrectilinear"),
                    ("grid", "grid"),
                    ("triangles", "triangles"),
                    ("stars", "stars"),
                    ("cubic", "cubic"),
                    ("line", "line"),
                    ("concentric", "concentric"),
                    ("honeycomb", "honeycomb"),
                    ("3dhoneycomb", "3dhoneycomb"),
                    ("gyroid", "gyroid"),
                    ("hilbertcurve", "hilbertcurve"),
                    ("archimedeanchords", "archimedeanchords"),
                    ("octagramspiral", "octagramspiral"),
                    ("adaptivecubic", "adaptivecubic"),
                    ("supportcubic", "supportcubic"),
                    ("lightning", "lightning"),
                    ("zigzag", "zigzag")
                ])
            ),
            d!(
                C,
                "infill_pattern",
                Form::Choices(&[
                    ("rectilinear", "lines"),
                    ("grid", "grid"),
                    ("triangles", "triangles"),
                    ("stars", "trihexagon"),
                    ("cubic", "cubic"),
                    ("concentric", "concentric"),
                    ("honeycomb", "honeycomb"),
                    ("gyroid", "gyroid"),
                    ("lightning", "lightning"),
                    ("zigzag", "zigzag"),
                    ("supportcubic", "cubicsubdiv")
                ])
            ),
        ]
    ),
    key!(
        Process,
        "fill_angle",
        Kind::Number(Unit::Degree),
        "Infill direction",
        SparseInfill,
        advanced,
        [d!(B | O | Q, "infill_direction"), d!(P, "fill_angle"),]
    ),
    key!(
        Process,
        "fill_multiline",
        Kind::Integer(Unit::None),
        "Infill line multiplier",
        SparseInfill,
        advanced,
        [d!(B | O | Q, "fill_multiline"), d!(C, "infill_multiplier"),]
    ),
    key!(
        Process,
        "infill_anchor",
        Kind::OrPercent(Unit::Mm, "infill_extrusion_width"),
        "Infill anchor length",
        SparseInfill,
        advanced,
        [
            d!(B | Q, "sparse_infill_anchor"),
            d!(O, "infill_anchor"),
            d!(P, "infill_anchor"),
        ]
    ),
    key!(
        Process,
        "infill_anchor_max",
        Kind::OrPercent(Unit::Mm, "infill_extrusion_width"),
        "Longest infill anchor",
        SparseInfill,
        advanced,
        [
            d!(
                B | Q,
                "sparse_infill_anchor_max",
                Form::OrPercent("infill_extrusion_width")
            ),
            d!(
                O,
                "infill_anchor_max",
                Form::OrPercent("infill_extrusion_width")
            ),
            d!(
                P,
                "infill_anchor_max",
                Form::OrPercent("infill_extrusion_width")
            ),
        ]
    ),
    key!(
        Process,
        "gap_fill_enabled",
        Kind::Boolean,
        "Fill gaps",
        SparseInfill,
        advanced,
        [
            d!(
                O,
                "gap_fill_target",
                Form::Choices(&[("true", "everywhere"), ("false", "nowhere")])
            ),
            d!(P, "gap_fill_enabled"),
        ]
    ),
    key!(
        Process,
        "filter_out_gap_fill",
        Kind::Number(Unit::Mm),
        "Shortest gap fill",
        SparseInfill,
        advanced,
        [d!(B | O | Q, "filter_out_gap_fill"),]
    ),
    key!(
        Process,
        "locked_skin_infill_pattern",
        Kind::Choice(&[
            ("concentric", "Concentric"),
            ("rectilinear", "Rectilinear"),
            ("grid", "Grid"),
            ("line", "Line"),
            ("cubic", "Cubic"),
            ("triangles", "Triangles"),
            ("stars", "Stars"),
            ("gyroid", "Gyroid"),
            ("honeycomb", "Honeycomb"),
            ("alignedrectilinear", "Aligned rectilinear"),
            ("3dhoneycomb", "3D honeycomb"),
            ("hilbertcurve", "Hilbert curve"),
            ("archimedeanchords", "Archimedean chords"),
            ("octagramspiral", "Octagram spiral"),
            ("crosshatch", "Cross hatch"),
            ("zigzag", "Zig-zag"),
            ("crosszag", "Cross zag")
        ]),
        "Locked skin infill pattern",
        SparseInfill,
        advanced,
        [d!(
            B | Q,
            "locked_skin_infill_pattern",
            Form::Choices(&[
                ("concentric", "concentric"),
                ("rectilinear", "zig-zag"),
                ("grid", "grid"),
                ("line", "line"),
                ("cubic", "cubic"),
                ("triangles", "triangles"),
                ("stars", "tri-hexagon"),
                ("gyroid", "gyroid"),
                ("honeycomb", "honeycomb"),
                ("alignedrectilinear", "alignedrectilinear"),
                ("3dhoneycomb", "3dhoneycomb"),
                ("hilbertcurve", "hilbertcurve"),
                ("archimedeanchords", "archimedeanchords"),
                ("octagramspiral", "octagramspiral"),
                ("crosshatch", "crosshatch"),
                ("zigzag", "zigzag"),
                ("crosszag", "crosszag")
            ])
        ),]
    ),
    key!(
        Process,
        "locked_skeleton_infill_pattern",
        Kind::Choice(&[
            ("concentric", "Concentric"),
            ("rectilinear", "Rectilinear"),
            ("grid", "Grid"),
            ("line", "Line"),
            ("cubic", "Cubic"),
            ("triangles", "Triangles"),
            ("stars", "Stars"),
            ("gyroid", "Gyroid"),
            ("honeycomb", "Honeycomb"),
            ("alignedrectilinear", "Aligned rectilinear"),
            ("3dhoneycomb", "3D honeycomb"),
            ("hilbertcurve", "Hilbert curve"),
            ("archimedeanchords", "Archimedean chords"),
            ("octagramspiral", "Octagram spiral"),
            ("crosshatch", "Cross hatch"),
            ("zigzag", "Zig-zag"),
            ("crosszag", "Cross zag")
        ]),
        "Locked skeleton infill pattern",
        SparseInfill,
        advanced,
        [d!(
            B | Q,
            "locked_skeleton_infill_pattern",
            Form::Choices(&[
                ("concentric", "concentric"),
                ("rectilinear", "zig-zag"),
                ("grid", "grid"),
                ("line", "line"),
                ("cubic", "cubic"),
                ("triangles", "triangles"),
                ("stars", "tri-hexagon"),
                ("gyroid", "gyroid"),
                ("honeycomb", "honeycomb"),
                ("alignedrectilinear", "alignedrectilinear"),
                ("3dhoneycomb", "3dhoneycomb"),
                ("hilbertcurve", "hilbertcurve"),
                ("archimedeanchords", "archimedeanchords"),
                ("octagramspiral", "octagramspiral"),
                ("crosshatch", "crosshatch"),
                ("zigzag", "zigzag"),
                ("crosszag", "crosszag")
            ])
        ),]
    ),
    key!(
        Process,
        "skin_infill_density",
        Kind::Percent,
        "Skin infill density",
        SparseInfill,
        advanced,
        [d!(B | O | Q, "skin_infill_density"),]
    ),
    key!(
        Process,
        "skeleton_infill_density",
        Kind::Percent,
        "Skeleton infill density",
        SparseInfill,
        advanced,
        [d!(B | O | Q, "skeleton_infill_density"),]
    ),
    key!(
        Process,
        "infill_lock_depth",
        Kind::Number(Unit::Mm),
        "Infill lock depth",
        SparseInfill,
        advanced,
        [d!(B | O | Q, "infill_lock_depth"),]
    ),
    key!(
        Process,
        "skin_infill_depth",
        Kind::Number(Unit::Mm),
        "Skin infill depth",
        SparseInfill,
        advanced,
        [d!(B | O | Q, "skin_infill_depth"),]
    ),
    key!(
        Process,
        "skin_infill_line_width",
        Kind::Number(Unit::Mm),
        "Skin infill line width",
        SparseInfill,
        advanced,
        [
            d!(B | Q, "skin_infill_line_width"),
            d!(
                O,
                "skin_infill_line_width",
                Form::OrPercent("nozzle_diameter")
            ),
        ]
    ),
    key!(
        Process,
        "skeleton_infill_line_width",
        Kind::Number(Unit::Mm),
        "Skeleton infill line width",
        SparseInfill,
        advanced,
        [
            d!(B | Q, "skeleton_infill_line_width"),
            d!(
                O,
                "skeleton_infill_line_width",
                Form::OrPercent("nozzle_diameter")
            ),
        ]
    ),
    key!(
        Process,
        "symmetric_infill_y_axis",
        Kind::Boolean,
        "Infill symmetric about Y",
        SparseInfill,
        advanced,
        [d!(B | O | Q, "symmetric_infill_y_axis"),]
    ),
    key!(
        Process,
        "infill_shift_step",
        Kind::Number(Unit::Mm),
        "Infill shift per layer",
        SparseInfill,
        advanced,
        [d!(B | O | Q, "infill_shift_step"),]
    ),
    key!(
        Process,
        "infill_rotate_step",
        Kind::Number(Unit::Degree),
        "Infill rotate step",
        SparseInfill,
        advanced,
        [d!(B | Q, "infill_rotate_step"),]
    ),
    key!(
        Process,
        "sparse_infill_lattice_angle_1",
        Kind::Number(Unit::Degree),
        "Lattice infill first angle",
        SparseInfill,
        advanced,
        [d!(B | Q, "sparse_infill_lattice_angle_1"),]
    ),
    key!(
        Process,
        "sparse_infill_lattice_angle_2",
        Kind::Number(Unit::Degree),
        "Lattice infill second angle",
        SparseInfill,
        advanced,
        [d!(B | Q, "sparse_infill_lattice_angle_2"),]
    ),
    key!(
        Process,
        "lightning_overhang_angle",
        Kind::Number(Unit::Degree),
        "Lightning overhang angle",
        SparseInfill,
        advanced,
        [
            d!(O, "lightning_overhang_angle"),
            d!(C, "lightning_infill_overhang_angle", list),
        ]
    ),
    key!(
        Process,
        "lightning_prune_angle",
        Kind::Number(Unit::Degree),
        "Lightning prune angle",
        SparseInfill,
        advanced,
        [
            d!(O, "lightning_prune_angle"),
            d!(C, "lightning_infill_prune_angle", list),
        ]
    ),
    key!(
        Process,
        "lightning_straightening_angle",
        Kind::Number(Unit::Degree),
        "Lightning straightening angle",
        SparseInfill,
        advanced,
        [
            d!(O, "lightning_straightening_angle"),
            d!(C, "lightning_infill_straightening_angle", list),
        ]
    ),
    key!(
        Process,
        "infill_overlap",
        Kind::OrPercent(Unit::Mm, "perimeter_extrusion_width"),
        "Infill and wall overlap",
        InfillDetails,
        advanced,
        [
            d!(
                B | O | Q,
                "infill_wall_overlap",
                Form::PercentOf("perimeter_extrusion_width")
            ),
            d!(P, "infill_overlap"),
            d!(
                C,
                "infill_overlap",
                Form::PercentOf("infill_extrusion_width")
            ),
        ]
    ),
    key!(
        Process,
        "top_bottom_infill_wall_overlap",
        Kind::OrPercent(Unit::Mm, "perimeter_extrusion_width"),
        "Solid infill and wall overlap",
        InfillDetails,
        advanced,
        [
            d!(
                O,
                "top_bottom_infill_wall_overlap",
                Form::PercentOf("perimeter_extrusion_width")
            ),
            d!(
                C,
                "skin_overlap",
                Form::PercentOf("solid_infill_extrusion_width")
            ),
        ]
    ),
    key!(
        Process,
        "bridge_angle",
        Kind::Number(Unit::Degree),
        "Bridge angle",
        InfillDetails,
        advanced,
        [d!(B | O | Q, "bridge_angle"), d!(P, "bridge_angle"),]
    ),
    key!(
        Process,
        "solid_infill_below_area",
        Kind::Number(Unit::Mm2),
        "Solid infill below area",
        InfillDetails,
        advanced,
        [
            d!(B | O | Q, "minimum_sparse_infill_area"),
            d!(P, "solid_infill_below_area"),
            d!(C, "min_infill_area"),
        ]
    ),
    key!(
        Process,
        "automatic_infill_combination",
        Kind::Boolean,
        "Combine infill layers",
        InfillDetails,
        advanced,
        [
            d!(B | O | Q, "infill_combination"),
            d!(P, "automatic_infill_combination"),
        ]
    ),
    key!(
        Process,
        "automatic_infill_combination_max_layer_height",
        Kind::OrPercent(Unit::Mm, "nozzle_diameter"),
        "Highest combined infill layer",
        InfillDetails,
        advanced,
        [
            d!(
                O,
                "infill_combination_max_layer_height",
                Form::OrPercent("nozzle_diameter")
            ),
            d!(
                P,
                "automatic_infill_combination_max_layer_height",
                Form::OrPercent("nozzle_diameter")
            ),
        ]
    ),
    key!(
        Process,
        "detect_narrow_internal_solid_infill",
        Kind::Boolean,
        "Detect narrow solid infill",
        InfillDetails,
        advanced,
        [d!(B | O | Q, "detect_narrow_internal_solid_infill"),]
    ),
    key!(
        Process,
        "internal_bridge_support_thickness",
        Kind::Number(Unit::Mm),
        "Internal bridge support thickness",
        InfillDetails,
        advanced,
        [d!(B | Q, "internal_bridge_support_thickness"),]
    ),
    key!(
        Process,
        "first_layer_speed",
        Kind::OrPercent(Unit::MmPerS, ""),
        "First layer speed",
        FirstLayer,
        common,
        [
            d!(B | O | Q, "initial_layer_speed", list, Form::Absolute),
            d!(P, "first_layer_speed"),
            d!(C, "speed_print_layer_0", Form::Absolute),
        ]
    ),
    key!(
        Process,
        "first_layer_infill_speed",
        Kind::OrPercent(Unit::MmPerS, ""),
        "First layer infill speed",
        FirstLayer,
        common,
        [
            d!(
                B | O | Q,
                "initial_layer_infill_speed",
                list,
                Form::Absolute
            ),
            d!(P, "first_layer_infill_speed"),
        ]
    ),
    key!(
        Process,
        "initial_layer_travel_speed",
        Kind::OrPercent(Unit::MmPerS, "travel_speed"),
        "First layer travel speed",
        FirstLayer,
        common,
        [
            d!(O, "initial_layer_travel_speed", list),
            d!(C, "speed_travel_layer_0", list, Form::Absolute),
        ]
    ),
    key!(
        Process,
        "slow_down_layers",
        Kind::Integer(Unit::Layers),
        "Slow first layers",
        FirstLayer,
        advanced,
        [d!(O, "slow_down_layers"), d!(C, "speed_slowdown_layers"),]
    ),
    key!(
        Process,
        "external_perimeter_speed",
        Kind::OrPercent(Unit::MmPerS, "perimeter_speed"),
        "Outer wall speed",
        OtherLayers,
        common,
        [
            d!(B | O | Q, "outer_wall_speed", list, Form::Absolute),
            d!(P, "external_perimeter_speed"),
            d!(C, "speed_wall_0", Form::Absolute),
        ]
    ),
    key!(
        Process,
        "perimeter_speed",
        Kind::Number(Unit::MmPerS),
        "Inner wall speed",
        OtherLayers,
        common,
        [
            d!(B | O | Q, "inner_wall_speed", list),
            d!(P, "perimeter_speed"),
            d!(C, "speed_wall_x"),
        ]
    ),
    key!(
        Process,
        "infill_speed",
        Kind::Number(Unit::MmPerS),
        "Sparse infill speed",
        OtherLayers,
        common,
        [
            d!(B | O | Q, "sparse_infill_speed", list),
            d!(P, "infill_speed"),
            d!(C, "speed_infill"),
        ]
    ),
    key!(
        Process,
        "solid_infill_speed",
        Kind::OrPercent(Unit::MmPerS, "infill_speed"),
        "Solid infill speed",
        OtherLayers,
        common,
        [
            d!(
                B | O | Q,
                "internal_solid_infill_speed",
                list,
                Form::Absolute
            ),
            d!(P, "solid_infill_speed"),
        ]
    ),
    key!(
        Process,
        "top_solid_infill_speed",
        Kind::OrPercent(Unit::MmPerS, "solid_infill_speed"),
        "Top surface speed",
        OtherLayers,
        common,
        [
            d!(B | O | Q, "top_surface_speed", list, Form::Absolute),
            d!(P, "top_solid_infill_speed"),
            d!(C, "speed_topbottom", Form::Absolute),
        ]
    ),
    key!(
        Process,
        "bridge_speed",
        Kind::Number(Unit::MmPerS),
        "Bridge speed",
        OtherLayers,
        common,
        [
            d!(B | O | Q, "bridge_speed", list),
            d!(P, "bridge_speed"),
            d!(C, "bridge_skin_speed"),
        ]
    ),
    key!(
        Process,
        "support_material_speed",
        Kind::Number(Unit::MmPerS),
        "Support speed",
        OtherLayers,
        common,
        [
            d!(B | O | Q, "support_speed", list),
            d!(P, "support_material_speed"),
            d!(C, "speed_support", list),
        ]
    ),
    key!(
        Process,
        "small_perimeter_speed",
        Kind::OrPercent(Unit::MmPerS, "external_perimeter_speed"),
        "Small wall speed",
        OtherLayers,
        advanced,
        [
            d!(B | O | Q, "small_perimeter_speed", list),
            d!(
                P,
                "small_perimeter_speed",
                Form::OrPercent("perimeter_speed")
            ),
        ]
    ),
    key!(
        Process,
        "small_perimeter_threshold",
        Kind::Number(Unit::Mm),
        "Small wall threshold",
        OtherLayers,
        advanced,
        [d!(B | O | Q, "small_perimeter_threshold", list),]
    ),
    key!(
        Process,
        "vertical_shell_speed",
        Kind::OrPercent(Unit::MmPerS, "solid_infill_speed"),
        "Vertical shell speed",
        OtherLayers,
        advanced,
        [d!(B | Q, "vertical_shell_speed", list),]
    ),
    key!(
        Process,
        "gap_fill_speed",
        Kind::Number(Unit::MmPerS),
        "Gap fill speed",
        OtherLayers,
        advanced,
        [
            d!(B | O | Q, "gap_infill_speed", list),
            d!(P, "gap_fill_speed"),
        ]
    ),
    key!(
        Process,
        "support_material_interface_speed",
        Kind::OrPercent(Unit::MmPerS, "support_material_speed"),
        "Support interface speed",
        OtherLayers,
        advanced,
        [
            d!(B | O | Q, "support_interface_speed", list, Form::Absolute),
            d!(P, "support_material_interface_speed"),
            d!(C, "speed_support_interface", list, Form::Absolute),
        ]
    ),
    key!(
        Process,
        "enable_dynamic_overhang_speeds",
        Kind::Boolean,
        "Slow down on overhangs",
        Overhangs,
        common,
        [
            d!(B | O | Q, "enable_overhang_speed", list),
            d!(P, "enable_dynamic_overhang_speeds"),
        ]
    ),
    key!(
        Process,
        "overhang_speed_0",
        Kind::OrPercent(Unit::MmPerS, "external_perimeter_speed"),
        "Overhang speed 75 to 100 %",
        Overhangs,
        advanced,
        [
            d!(B | Q, "overhang_4_4_speed", list, Form::Absolute),
            d!(O, "overhang_4_4_speed", list),
            d!(
                P,
                "overhang_speed_0",
                Form::OrPercent("external_perimeter_speed")
            ),
        ]
    ),
    key!(
        Process,
        "overhang_speed_1",
        Kind::OrPercent(Unit::MmPerS, "external_perimeter_speed"),
        "Overhang speed 50 to 75 %",
        Overhangs,
        advanced,
        [
            d!(B | Q, "overhang_3_4_speed", list, Form::Absolute),
            d!(O, "overhang_3_4_speed", list),
            d!(
                P,
                "overhang_speed_1",
                Form::OrPercent("external_perimeter_speed")
            ),
        ]
    ),
    key!(
        Process,
        "overhang_speed_2",
        Kind::OrPercent(Unit::MmPerS, "external_perimeter_speed"),
        "Overhang speed 25 to 50 %",
        Overhangs,
        advanced,
        [
            d!(B | Q, "overhang_2_4_speed", list, Form::Absolute),
            d!(O, "overhang_2_4_speed", list),
            d!(
                P,
                "overhang_speed_2",
                Form::OrPercent("external_perimeter_speed")
            ),
        ]
    ),
    key!(
        Process,
        "overhang_speed_3",
        Kind::OrPercent(Unit::MmPerS, "external_perimeter_speed"),
        "Overhang speed up to 25 %",
        Overhangs,
        advanced,
        [
            d!(B | Q, "overhang_1_4_speed", list, Form::Absolute),
            d!(O, "overhang_1_4_speed", list),
            d!(
                P,
                "overhang_speed_3",
                Form::OrPercent("external_perimeter_speed")
            ),
        ]
    ),
    key!(
        Process,
        "overhang_totally_speed",
        Kind::Number(Unit::MmPerS),
        "Fully overhanging speed",
        Overhangs,
        advanced,
        [d!(B | Q, "overhang_totally_speed", list),]
    ),
    key!(
        Process,
        "enable_height_slowdown",
        Kind::Boolean,
        "Slow down by height",
        SlowDownByHeight,
        advanced,
        [d!(B | Q, "enable_height_slowdown", list),]
    ),
    key!(
        Process,
        "slowdown_start_height",
        Kind::Number(Unit::Mm),
        "Slowdown start height",
        SlowDownByHeight,
        advanced,
        [d!(B | Q, "slowdown_start_height", list),]
    ),
    key!(
        Process,
        "slowdown_start_speed",
        Kind::Number(Unit::MmPerS),
        "Slowdown start speed",
        SlowDownByHeight,
        advanced,
        [d!(B | Q, "slowdown_start_speed", list),]
    ),
    key!(
        Process,
        "slowdown_start_acc",
        Kind::Number(Unit::MmPerS2),
        "Slowdown start acceleration",
        SlowDownByHeight,
        advanced,
        [d!(B | Q, "slowdown_start_acc", list),]
    ),
    key!(
        Process,
        "slowdown_end_height",
        Kind::Number(Unit::Mm),
        "Slowdown end height",
        SlowDownByHeight,
        advanced,
        [d!(B | Q, "slowdown_end_height", list),]
    ),
    key!(
        Process,
        "slowdown_end_speed",
        Kind::Number(Unit::MmPerS),
        "Slowdown end speed",
        SlowDownByHeight,
        advanced,
        [d!(B | Q, "slowdown_end_speed", list),]
    ),
    key!(
        Process,
        "slowdown_end_acc",
        Kind::Number(Unit::MmPerS2),
        "Slowdown end acceleration",
        SlowDownByHeight,
        advanced,
        [d!(B | Q, "slowdown_end_acc", list),]
    ),
    key!(
        Process,
        "travel_speed",
        Kind::Number(Unit::MmPerS),
        "Travel speed",
        TravelSpeed,
        common,
        [
            d!(B | O | Q, "travel_speed", list),
            d!(P, "travel_speed"),
            d!(C, "speed_travel", list),
        ]
    ),
    key!(
        Process,
        "travel_speed_z",
        Kind::Number(Unit::MmPerS),
        "Vertical travel speed",
        TravelSpeed,
        advanced,
        [
            d!(B | O | Q, "travel_speed_z", list),
            d!(P, "travel_speed_z"),
            d!(C, "speed_z_hop", list),
        ]
    ),
    key!(
        Process,
        "default_acceleration",
        Kind::Number(Unit::MmPerS2),
        "Default acceleration",
        Acceleration,
        common,
        [
            d!(B | O | Q, "default_acceleration", list),
            d!(P, "default_acceleration"),
            d!(C, "acceleration_print"),
        ]
    ),
    key!(
        Process,
        "first_layer_acceleration",
        Kind::Number(Unit::MmPerS2),
        "First layer acceleration",
        Acceleration,
        common,
        [
            d!(B | O | Q, "initial_layer_acceleration", list),
            d!(P, "first_layer_acceleration"),
            d!(C, "acceleration_print_layer_0"),
        ]
    ),
    key!(
        Process,
        "travel_acceleration",
        Kind::Number(Unit::MmPerS2),
        "Travel acceleration",
        Acceleration,
        common,
        [
            d!(B | O | Q, "travel_acceleration", list),
            d!(P, "travel_acceleration"),
            d!(C, "acceleration_travel"),
        ]
    ),
    key!(
        Process,
        "external_perimeter_acceleration",
        Kind::Number(Unit::MmPerS2),
        "Outer wall acceleration",
        Acceleration,
        advanced,
        [
            d!(B | O | Q, "outer_wall_acceleration", list),
            d!(P, "external_perimeter_acceleration"),
            d!(C, "acceleration_wall_0"),
        ]
    ),
    key!(
        Process,
        "perimeter_acceleration",
        Kind::Number(Unit::MmPerS2),
        "Inner wall acceleration",
        Acceleration,
        advanced,
        [
            d!(B | O | Q, "inner_wall_acceleration", list),
            d!(P, "perimeter_acceleration"),
            d!(C, "acceleration_wall_x"),
        ]
    ),
    key!(
        Process,
        "top_solid_infill_acceleration",
        Kind::Number(Unit::MmPerS2),
        "Top surface acceleration",
        Acceleration,
        advanced,
        [
            d!(B | O | Q, "top_surface_acceleration", list),
            d!(P, "top_solid_infill_acceleration"),
            d!(C, "acceleration_topbottom"),
        ]
    ),
    key!(
        Process,
        "infill_acceleration",
        Kind::OrPercent(Unit::MmPerS2, "default_acceleration"),
        "Sparse infill acceleration",
        Acceleration,
        advanced,
        [
            d!(B | O | Q, "sparse_infill_acceleration", list),
            d!(P, "infill_acceleration", Form::Absolute),
            d!(C, "acceleration_infill", Form::Absolute),
        ]
    ),
    key!(
        Process,
        "solid_infill_acceleration",
        Kind::OrPercent(Unit::MmPerS2, "default_acceleration"),
        "Solid infill acceleration",
        Acceleration,
        advanced,
        [
            d!(O, "internal_solid_infill_acceleration", list),
            d!(P, "solid_infill_acceleration", Form::Absolute),
        ]
    ),
    key!(
        Process,
        "bridge_acceleration",
        Kind::OrPercent(Unit::MmPerS2, "external_perimeter_acceleration"),
        "Bridge acceleration",
        Acceleration,
        advanced,
        [
            d!(O, "bridge_acceleration", list),
            d!(P, "bridge_acceleration", Form::Absolute),
        ]
    ),
    key!(
        Process,
        "initial_layer_travel_acceleration",
        Kind::OrPercent(Unit::MmPerS2, "travel_acceleration"),
        "First layer travel acceleration",
        Acceleration,
        advanced,
        [
            d!(
                B | Q,
                "initial_layer_travel_acceleration",
                list,
                Form::Absolute
            ),
            d!(O, "initial_layer_travel_acceleration", list),
            d!(C, "acceleration_travel_layer_0", list, Form::Absolute),
        ]
    ),
    key!(
        Process,
        "travel_short_distance_acceleration",
        Kind::Number(Unit::MmPerS2),
        "Short travel acceleration",
        Acceleration,
        advanced,
        [
            d!(B | Q, "travel_short_distance_acceleration", list),
            d!(P, "travel_short_distance_acceleration"),
        ]
    ),
    key!(
        Process,
        "wipe_tower_acceleration",
        Kind::Number(Unit::MmPerS2),
        "Prime tower acceleration",
        Acceleration,
        advanced,
        [
            d!(P, "wipe_tower_acceleration"),
            d!(C, "acceleration_prime_tower"),
        ]
    ),
    key!(
        Process,
        "accel_to_decel_enable",
        Kind::Boolean,
        "Limit deceleration",
        Acceleration,
        advanced,
        [d!(B | O | Q, "accel_to_decel_enable"),]
    ),
    key!(
        Process,
        "accel_to_decel_factor",
        Kind::Percent,
        "Deceleration share of acceleration",
        Acceleration,
        advanced,
        [d!(B | O | Q, "accel_to_decel_factor"),]
    ),
    key!(
        Process,
        "default_jerk",
        Kind::Number(Unit::MmPerS),
        "Default jerk",
        Jerk,
        advanced,
        [
            d!(B | Q, "default_jerk"),
            d!(O, "default_jerk", list),
            d!(C, "jerk_print"),
        ]
    ),
    key!(
        Process,
        "outer_wall_jerk",
        Kind::Number(Unit::MmPerS),
        "Outer wall jerk",
        Jerk,
        advanced,
        [
            d!(B | Q, "outer_wall_jerk"),
            d!(O, "outer_wall_jerk", list),
            d!(C, "jerk_wall_0"),
        ]
    ),
    key!(
        Process,
        "inner_wall_jerk",
        Kind::Number(Unit::MmPerS),
        "Inner wall jerk",
        Jerk,
        advanced,
        [
            d!(B | Q, "inner_wall_jerk"),
            d!(O, "inner_wall_jerk", list),
            d!(C, "jerk_wall_x"),
        ]
    ),
    key!(
        Process,
        "infill_jerk",
        Kind::Number(Unit::MmPerS),
        "Infill jerk",
        Jerk,
        advanced,
        [
            d!(B | Q, "infill_jerk"),
            d!(O, "infill_jerk", list),
            d!(C, "jerk_infill"),
        ]
    ),
    key!(
        Process,
        "top_surface_jerk",
        Kind::Number(Unit::MmPerS),
        "Top surface jerk",
        Jerk,
        advanced,
        [
            d!(B | Q, "top_surface_jerk"),
            d!(O, "top_surface_jerk", list),
            d!(C, "jerk_topbottom"),
        ]
    ),
    key!(
        Process,
        "initial_layer_jerk",
        Kind::Number(Unit::MmPerS),
        "First layer jerk",
        Jerk,
        advanced,
        [
            d!(B | Q, "initial_layer_jerk"),
            d!(O, "initial_layer_jerk", list),
            d!(C, "jerk_print_layer_0"),
        ]
    ),
    key!(
        Process,
        "travel_jerk",
        Kind::Number(Unit::MmPerS),
        "Travel jerk",
        Jerk,
        advanced,
        [
            d!(B | Q, "travel_jerk"),
            d!(O, "travel_jerk", list),
            d!(C, "jerk_travel"),
        ]
    ),
    key!(
        Process,
        "max_volumetric_extrusion_rate_slope_positive",
        Kind::Number(Unit::Mm3PerS2),
        "Flow increase limit",
        Smoothing,
        advanced,
        [
            d!(B | Q, "max_volumetric_extrusion_rate_slope_positive"),
            d!(P, "max_volumetric_extrusion_rate_slope_positive"),
        ]
    ),
    key!(
        Process,
        "max_volumetric_extrusion_rate_slope_negative",
        Kind::Number(Unit::Mm3PerS2),
        "Flow decrease limit",
        Smoothing,
        advanced,
        [
            d!(B | Q, "max_volumetric_extrusion_rate_slope_negative"),
            d!(P, "max_volumetric_extrusion_rate_slope_negative"),
        ]
    ),
    key!(
        Process,
        "smooth_speed_discontinuity_area",
        Kind::Boolean,
        "Smooth speed changes",
        Smoothing,
        advanced,
        [d!(B | Q, "smooth_speed_discontinuity_area"),]
    ),
    key!(
        Process,
        "smooth_coefficient",
        Kind::Number(Unit::None),
        "Speed smoothing factor",
        Smoothing,
        advanced,
        [d!(B | Q, "smooth_coefficient"),]
    ),
    key!(
        Process,
        "z_direction_outwall_speed_continuous",
        Kind::Boolean,
        "Smooth outer wall speed along Z",
        Smoothing,
        advanced,
        [d!(B | Q, "z_direction_outwall_speed_continuous"),]
    ),
    key!(
        Process,
        "support_material",
        Kind::Boolean,
        "Supports",
        Support,
        common,
        [
            d!(B | O | Q, "enable_support"),
            d!(P, "support_material"),
            d!(C, "support_enable"),
        ]
    ),
    key!(
        Process,
        "support_type",
        Kind::Choice(&[
            ("normal_auto", "Normal (auto)"),
            ("tree_auto", "Tree (auto)"),
            ("normal_manual", "Normal (manual)"),
            ("tree_manual", "Tree (manual)")
        ]),
        "Support type",
        Support,
        common,
        [
            d!(
                B | O | Q,
                "support_type",
                Form::Choices(&[
                    ("normal_auto", "normal(auto)"),
                    ("tree_auto", "tree(auto)"),
                    ("normal_manual", "normal(manual)"),
                    ("tree_manual", "tree(manual)")
                ])
            ),
            d!(
                C,
                "support_structure",
                Form::Choices(&[
                    ("normal_auto", "normal"),
                    ("tree_auto", "tree"),
                    ("normal_manual", "normal"),
                    ("tree_manual", "tree")
                ])
            ),
        ]
    ),
    key!(
        Process,
        "support_material_style",
        Kind::Choice(&[
            ("default", "Default"),
            ("grid", "Grid"),
            ("snug", "Snug"),
            ("tree_slim", "Tree (slim)"),
            ("tree_strong", "Tree (strong)"),
            ("tree_hybrid", "Tree (hybrid)"),
            ("tree_organic", "Tree (organic)"),
            ("organic", "Organic")
        ]),
        "Support style",
        Support,
        common,
        [
            d!(
                B | Q,
                "support_style",
                Form::Choices(&[
                    ("default", "default"),
                    ("grid", "grid"),
                    ("snug", "snug"),
                    ("tree_slim", "tree_slim"),
                    ("tree_strong", "tree_strong"),
                    ("tree_hybrid", "tree_hybrid"),
                    ("tree_organic", "tree_organic")
                ])
            ),
            d!(
                O,
                "support_style",
                Form::Choices(&[
                    ("default", "default"),
                    ("grid", "grid"),
                    ("snug", "snug"),
                    ("tree_organic", "organic"),
                    ("tree_slim", "tree_slim"),
                    ("tree_strong", "tree_strong"),
                    ("tree_hybrid", "tree_hybrid")
                ])
            ),
            d!(
                P,
                "support_material_style",
                Form::Choices(&[("grid", "grid"), ("snug", "snug"), ("organic", "organic")])
            ),
        ]
    ),
    key!(
        Process,
        "support_material_threshold",
        Kind::Integer(Unit::Degree),
        "Support threshold angle",
        Support,
        common,
        [
            d!(B | O | Q, "support_threshold_angle"),
            d!(P, "support_material_threshold"),
            d!(C, "support_angle", Form::Linear(-1.0, 90.0)),
        ]
    ),
    key!(
        Process,
        "support_material_buildplate_only",
        Kind::Boolean,
        "Supports on the bed only",
        Support,
        common,
        [
            d!(B | O | Q, "support_on_build_plate_only"),
            d!(P, "support_material_buildplate_only"),
            d!(
                C,
                "support_type",
                Form::Choices(&[("true", "buildplate"), ("false", "everywhere")])
            ),
        ]
    ),
    key!(
        Process,
        "support_critical_regions_only",
        Kind::Boolean,
        "Supports for critical regions only",
        Support,
        advanced,
        [d!(B | O | Q, "support_critical_regions_only"),]
    ),
    key!(
        Process,
        "support_remove_small_overhang",
        Kind::Boolean,
        "Ignore small overhangs",
        Support,
        advanced,
        [d!(B | O | Q, "support_remove_small_overhang"),]
    ),
    key!(
        Process,
        "support_material_enforce_layers",
        Kind::Integer(Unit::Layers),
        "Enforced support layers",
        Support,
        advanced,
        [
            d!(B | O | Q, "enforce_support_layers"),
            d!(P, "support_material_enforce_layers"),
        ]
    ),
    key!(
        Process,
        "raft_layers",
        Kind::Integer(Unit::Layers),
        "Raft layers",
        Raft,
        common,
        [d!(B | O | Q, "raft_layers"), d!(P, "raft_layers"),]
    ),
    key!(
        Process,
        "raft_contact_distance",
        Kind::Number(Unit::Mm),
        "Raft contact distance",
        Raft,
        advanced,
        [
            d!(B | O | Q, "raft_contact_distance"),
            d!(P, "raft_contact_distance"),
            d!(C, "raft_airgap", list),
        ]
    ),
    key!(
        Process,
        "raft_expansion",
        Kind::Number(Unit::Mm),
        "Raft expansion",
        Raft,
        advanced,
        [
            d!(B | O | Q, "raft_expansion"),
            d!(P, "raft_expansion"),
            d!(C, "raft_margin", list),
        ]
    ),
    key!(
        Process,
        "raft_first_layer_density",
        Kind::Percent,
        "Raft first layer density",
        Raft,
        advanced,
        [
            d!(B | O | Q, "raft_first_layer_density"),
            d!(P, "raft_first_layer_density"),
        ]
    ),
    key!(
        Process,
        "raft_first_layer_expansion",
        Kind::Number(Unit::Mm),
        "Raft first layer expansion",
        Raft,
        advanced,
        [
            // Bambu Studio's -1 (automatic) is out of the others' range.
            d!(B | Q, "raft_first_layer_expansion"),
            d!(O, "raft_first_layer_expansion", range(0.0, f64::INFINITY)),
            d!(P, "raft_first_layer_expansion", range(0.0, f64::INFINITY)),
        ]
    ),
    key!(
        Process,
        "support_material_contact_distance",
        Kind::Number(Unit::Mm),
        "Top contact distance",
        ContactAndInterface,
        common,
        [
            d!(B | O | Q, "support_top_z_distance"),
            d!(P, "support_material_contact_distance"),
            d!(C, "support_top_distance"),
        ]
    ),
    key!(
        Process,
        "support_material_xy_spacing",
        Kind::OrPercent(Unit::Mm, "external_perimeter_extrusion_width"),
        "Support and part gap",
        ContactAndInterface,
        common,
        [
            d!(B | O | Q, "support_object_xy_distance", Form::Absolute),
            d!(P, "support_material_xy_spacing"),
            d!(C, "support_xy_distance", Form::Absolute),
        ]
    ),
    key!(
        Process,
        "support_material_bottom_contact_distance",
        Kind::Number(Unit::Mm),
        "Bottom contact distance",
        ContactAndInterface,
        advanced,
        [
            d!(B | O | Q, "support_bottom_z_distance"),
            d!(P, "support_material_bottom_contact_distance"),
            d!(C, "support_bottom_distance"),
        ]
    ),
    key!(
        Process,
        "support_material_interface_layers",
        Kind::Integer(Unit::Layers),
        "Top interface layers",
        ContactAndInterface,
        advanced,
        [
            d!(B | O | Q, "support_interface_top_layers"),
            d!(P, "support_material_interface_layers"),
        ]
    ),
    key!(
        Process,
        "support_material_bottom_interface_layers",
        Kind::Integer(Unit::Layers),
        "Bottom interface layers",
        ContactAndInterface,
        advanced,
        [
            d!(B | O | Q, "support_interface_bottom_layers"),
            d!(P, "support_material_bottom_interface_layers"),
        ]
    ),
    key!(
        Process,
        "support_material_interface_pattern",
        Kind::Choice(&[
            ("auto", "Auto"),
            ("rectilinear", "Rectilinear"),
            ("concentric", "Concentric"),
            ("rectilinear_interlaced", "Rectilinear interlaced"),
            ("grid", "Grid"),
            ("spiralinset", "Spiral inset")
        ]),
        "Support interface pattern",
        ContactAndInterface,
        advanced,
        [
            d!(
                B | Q,
                "support_interface_pattern",
                Form::Choices(&[
                    ("auto", "auto"),
                    ("rectilinear", "rectilinear"),
                    ("concentric", "concentric"),
                    ("rectilinear_interlaced", "rectilinear_interlaced"),
                    ("grid", "grid")
                ])
            ),
            d!(O, "support_interface_pattern"),
            d!(
                P,
                "support_material_interface_pattern",
                Form::Choices(&[
                    ("auto", "auto"),
                    ("rectilinear", "rectilinear"),
                    ("concentric", "concentric")
                ])
            ),
            d!(
                C,
                "support_interface_pattern",
                list,
                Form::Choices(&[
                    ("rectilinear", "lines"),
                    ("concentric", "concentric"),
                    ("grid", "grid")
                ])
            ),
        ]
    ),
    key!(
        Process,
        "support_material_interface_spacing",
        Kind::Number(Unit::Mm),
        "Support interface spacing",
        ContactAndInterface,
        advanced,
        [
            d!(B | O | Q, "support_interface_spacing"),
            d!(P, "support_material_interface_spacing"),
        ]
    ),
    key!(
        Process,
        "support_bottom_interface_spacing",
        Kind::Number(Unit::Mm),
        "Bottom interface spacing",
        ContactAndInterface,
        advanced,
        [d!(B | O | Q, "support_bottom_interface_spacing"),]
    ),
    key!(
        Process,
        "support_material_interface_contact_loops",
        Kind::Boolean,
        "Interface loops",
        ContactAndInterface,
        advanced,
        [
            d!(B | O | Q, "support_interface_loop_pattern"),
            d!(P, "support_material_interface_contact_loops"),
        ]
    ),
    key!(
        Process,
        "top_z_overrides_xy_distance",
        Kind::Boolean,
        "Top distance wins over side gap",
        ContactAndInterface,
        advanced,
        [d!(B | Q, "top_z_overrides_xy_distance"),]
    ),
    key!(
        Process,
        "support_object_first_layer_gap",
        Kind::Number(Unit::Mm),
        "Support gap on the first layer",
        ContactAndInterface,
        advanced,
        [d!(B | O | Q, "support_object_first_layer_gap"),]
    ),
    key!(
        Process,
        "support_material_pattern",
        Kind::Choice(&[
            ("default", "Default"),
            ("rectilinear", "Rectilinear"),
            ("rectilinear-grid", "Rectilinear grid"),
            ("honeycomb", "Honeycomb"),
            ("lightning", "Lightning"),
            ("hollow", "Hollow")
        ]),
        "Support pattern",
        SupportBody,
        advanced,
        [
            d!(B | O | Q, "support_base_pattern"),
            d!(
                P,
                "support_material_pattern",
                Form::Choices(&[
                    ("rectilinear", "rectilinear"),
                    ("rectilinear-grid", "rectilinear-grid"),
                    ("honeycomb", "honeycomb")
                ])
            ),
            d!(
                C,
                "support_pattern",
                list,
                Form::Choices(&[
                    ("rectilinear", "lines"),
                    ("rectilinear-grid", "grid"),
                    ("honeycomb", "honeycomb"),
                    ("lightning", "lightning")
                ])
            ),
        ]
    ),
    key!(
        Process,
        "support_material_spacing",
        Kind::Number(Unit::Mm),
        "Support line spacing",
        SupportBody,
        advanced,
        [
            d!(B | O | Q, "support_base_pattern_spacing"),
            d!(P, "support_material_spacing"),
        ]
    ),
    key!(
        Process,
        "support_material_angle",
        Kind::Number(Unit::Degree),
        "Support pattern angle",
        SupportBody,
        advanced,
        [
            d!(B | O | Q, "support_angle"),
            d!(P, "support_material_angle"),
        ]
    ),
    key!(
        Process,
        "support_expansion",
        Kind::Number(Unit::Mm),
        "Support expansion",
        SupportBody,
        advanced,
        [d!(B | O | Q, "support_expansion"),]
    ),
    key!(
        Process,
        "dont_support_bridges",
        Kind::Boolean,
        "No support under bridges",
        SupportBody,
        advanced,
        [
            d!(B | O | Q, "bridge_no_support"),
            d!(P, "dont_support_bridges"),
        ]
    ),
    key!(
        Process,
        "max_bridge_length",
        Kind::Number(Unit::Mm),
        "Longest bridge",
        SupportBody,
        advanced,
        [d!(B | O | Q, "max_bridge_length"),]
    ),
    key!(
        Process,
        "independent_support_layer_height",
        Kind::Boolean,
        "Own layer height for supports",
        SupportBody,
        advanced,
        [d!(B | O | Q, "independent_support_layer_height"),]
    ),
    key!(
        Process,
        "support_tree_angle",
        Kind::Number(Unit::Degree),
        "Tree branch angle",
        TreeSupports,
        advanced,
        [
            d!(B | O | Q, "tree_support_branch_angle"),
            d!(P, "support_tree_angle"),
            d!(C, "support_tree_angle", list),
        ]
    ),
    key!(
        Process,
        "support_tree_angle_slow",
        Kind::Number(Unit::Degree),
        "Tree preferred branch angle",
        TreeSupports,
        advanced,
        [
            d!(O, "tree_support_angle_slow"),
            d!(P, "support_tree_angle_slow"),
            d!(C, "support_tree_angle_slow", list),
        ]
    ),
    key!(
        Process,
        "support_tree_branch_diameter",
        Kind::Number(Unit::Mm),
        "Tree branch diameter",
        TreeSupports,
        advanced,
        [
            d!(B | O | Q, "tree_support_branch_diameter"),
            d!(P, "support_tree_branch_diameter"),
            d!(C, "support_tree_branch_diameter", list),
        ]
    ),
    key!(
        Process,
        "support_tree_branch_diameter_angle",
        Kind::Number(Unit::Degree),
        "Tree branch diameter angle",
        TreeSupports,
        advanced,
        [
            d!(B | O | Q, "tree_support_branch_diameter_angle"),
            d!(P, "support_tree_branch_diameter_angle"),
            d!(C, "support_tree_branch_diameter_angle", list),
        ]
    ),
    key!(
        Process,
        "support_tree_tip_diameter",
        Kind::Number(Unit::Mm),
        "Tree tip diameter",
        TreeSupports,
        advanced,
        [
            d!(O, "tree_support_tip_diameter"),
            d!(P, "support_tree_tip_diameter"),
            d!(C, "support_tree_tip_diameter", list),
        ]
    ),
    key!(
        Process,
        "support_tree_top_rate",
        Kind::Percent,
        "Tree branch density",
        TreeSupports,
        advanced,
        [
            d!(O, "tree_support_top_rate"),
            d!(P, "support_tree_top_rate"),
            d!(C, "support_tree_top_rate", list),
        ]
    ),
    key!(
        Process,
        "tree_support_wall_count",
        Kind::Integer(Unit::None),
        "Tree support walls",
        TreeSupports,
        advanced,
        [
            // Bambu Studio's -1 (automatic) is out of OrcaSlicer's range.
            d!(B | Q, "tree_support_wall_count"),
            d!(O, "tree_support_wall_count", range(0.0, 2.0)),
        ]
    ),
    key!(
        Process,
        "tree_support_branch_distance",
        Kind::Number(Unit::Mm),
        "Tree branch distance",
        TreeSupports,
        advanced,
        [d!(B | O | Q, "tree_support_branch_distance"),]
    ),
    key!(
        Process,
        "support_material_extruder",
        Kind::Integer(Unit::None),
        "Support filament",
        SupportFilament,
        advanced,
        [
            d!(B | O | Q, "support_filament"),
            d!(P, "support_material_extruder"),
            d!(C, "support_extruder_nr", Form::Linear(1.0, -1.0)),
        ]
    ),
    key!(
        Process,
        "support_material_interface_extruder",
        Kind::Integer(Unit::None),
        "Support interface filament",
        SupportFilament,
        advanced,
        [
            d!(B | O | Q, "support_interface_filament"),
            d!(P, "support_material_interface_extruder"),
            d!(C, "support_interface_extruder_nr", Form::Linear(1.0, -1.0)),
        ]
    ),
    key!(
        Process,
        "support_interface_not_for_body",
        Kind::Boolean,
        "No interface filament against the part",
        SupportFilament,
        advanced,
        [d!(B | O | Q, "support_interface_not_for_body"),]
    ),
    key!(
        Process,
        "enable_support_ironing",
        Kind::Boolean,
        "Iron support interfaces",
        SupportIroning,
        advanced,
        [
            d!(B | Q, "enable_support_ironing"),
            d!(O, "support_ironing"),
        ]
    ),
    key!(
        Process,
        "support_ironing_pattern",
        Kind::Choice(&[("concentric", "Concentric"), ("rectilinear", "Rectilinear")]),
        "Support ironing pattern",
        SupportIroning,
        advanced,
        [
            d!(
                B | Q,
                "support_ironing_pattern",
                Form::Choices(&[("concentric", "concentric"), ("rectilinear", "zig-zag")])
            ),
            d!(O, "support_ironing_pattern"),
        ]
    ),
    key!(
        Process,
        "support_ironing_speed",
        Kind::Number(Unit::MmPerS),
        "Support ironing speed",
        SupportIroning,
        advanced,
        [d!(B | Q, "support_ironing_speed"),]
    ),
    key!(
        Process,
        "support_ironing_flow",
        Kind::Percent,
        "Support ironing flow",
        SupportIroning,
        advanced,
        [d!(B | O | Q, "support_ironing_flow"),]
    ),
    key!(
        Process,
        "support_ironing_spacing",
        Kind::Number(Unit::Mm),
        "Support ironing spacing",
        SupportIroning,
        advanced,
        [d!(B | O | Q, "support_ironing_spacing"),]
    ),
    key!(
        Process,
        "support_ironing_inset",
        Kind::Number(Unit::Mm),
        "Support ironing inset",
        SupportIroning,
        advanced,
        [d!(B | Q, "support_ironing_inset"),]
    ),
    key!(
        Process,
        "support_ironing_direction",
        Kind::Number(Unit::Degree),
        "Support ironing direction",
        SupportIroning,
        advanced,
        [d!(B | Q, "support_ironing_direction"),]
    ),
    key!(
        Process,
        "brim_type",
        Kind::Choice(&[
            ("auto_brim", "Auto"),
            ("brim_ears", "Brim ears"),
            ("outer_only", "Outer only"),
            ("inner_only", "Inner only"),
            ("outer_and_inner", "Outer and inner"),
            ("no_brim", "None"),
            ("painted", "Painted"),
            ("leading_edge_only", "Leading edge only")
        ]),
        "Brim type",
        BedAdhesion,
        common,
        [
            d!(
                B | Q,
                "brim_type",
                Form::Choices(&[
                    ("auto_brim", "auto_brim"),
                    ("brim_ears", "brim_ears"),
                    ("outer_only", "outer_only"),
                    ("inner_only", "inner_only"),
                    ("outer_and_inner", "outer_and_inner"),
                    ("no_brim", "no_brim")
                ])
            ),
            d!(O, "brim_type"),
            d!(
                P,
                "brim_type",
                Form::Choices(&[
                    ("no_brim", "no_brim"),
                    ("outer_only", "outer_only"),
                    ("inner_only", "inner_only"),
                    ("outer_and_inner", "outer_and_inner")
                ])
            ),
            d!(
                C,
                "brim_location",
                list,
                Form::Choices(&[
                    ("outer_only", "outside"),
                    ("inner_only", "inside"),
                    ("outer_and_inner", "everywhere")
                ])
            ),
        ]
    ),
    key!(
        Process,
        "brim_width",
        Kind::Number(Unit::Mm),
        "Brim width",
        BedAdhesion,
        common,
        [
            d!(B | O | Q, "brim_width"),
            d!(P, "brim_width"),
            d!(C, "brim_width", list),
        ]
    ),
    key!(
        Process,
        "brim_separation",
        Kind::Number(Unit::Mm),
        "Brim gap",
        BedAdhesion,
        common,
        [
            d!(B | O | Q, "brim_object_gap"),
            d!(P, "brim_separation"),
            d!(C, "brim_gap", list),
        ]
    ),
    key!(
        Process,
        "skirts",
        Kind::Integer(Unit::None),
        "Skirt loops",
        BedAdhesion,
        common,
        [
            d!(B | O | Q, "skirt_loops"),
            d!(P, "skirts"),
            d!(C, "skirt_line_count", list),
        ]
    ),
    key!(
        Process,
        "skirt_distance",
        Kind::Number(Unit::Mm),
        "Skirt distance",
        BedAdhesion,
        common,
        [
            d!(B | O | Q, "skirt_distance"),
            d!(P, "skirt_distance"),
            d!(C, "skirt_gap", list),
        ]
    ),
    key!(
        Process,
        "skirt_height",
        Kind::Integer(Unit::Layers),
        "Skirt height",
        BedAdhesion,
        common,
        [
            d!(B | O | Q, "skirt_height"),
            d!(P, "skirt_height"),
            d!(C, "skirt_height", list),
        ]
    ),
    key!(
        Process,
        "brim_flow_ratio",
        Kind::Number(Unit::None),
        "Brim flow",
        BedAdhesion,
        advanced,
        [
            d!(O, "brim_flow_ratio"),
            d!(
                C,
                "skirt_brim_material_flow",
                list,
                Form::Linear(100.0, 0.0)
            ),
        ]
    ),
    key!(
        Process,
        "skirt_speed",
        Kind::Number(Unit::MmPerS),
        "Skirt speed",
        BedAdhesion,
        advanced,
        [d!(O, "skirt_speed"), d!(C, "skirt_brim_speed", list),]
    ),
    key!(
        Process,
        "min_skirt_length",
        Kind::Number(Unit::Mm),
        "Shortest skirt",
        BedAdhesion,
        advanced,
        [d!(O, "min_skirt_length"), d!(P, "min_skirt_length"),]
    ),
    key!(
        Process,
        "skirt_per_object",
        Kind::Boolean,
        "Skirt per object",
        BedAdhesion,
        advanced,
        [d!(B | Q, "skirt_per_object"),]
    ),
    key!(
        Process,
        "draft_shield",
        Kind::Choice(&[
            ("disabled", "Off"),
            ("limited", "Limited"),
            ("enabled", "On")
        ]),
        "Draft shield",
        BedAdhesion,
        advanced,
        [
            d!(B | Q, "draft_shield"),
            d!(
                O,
                "draft_shield",
                Form::Choices(&[("disabled", "disabled"), ("enabled", "enabled")])
            ),
            d!(P, "draft_shield"),
            d!(
                C,
                "draft_shield_height_limitation",
                Form::Switch {
                    key: "draft_shield_enabled",
                    off: "disabled",
                    choices: &[("limited", "limited"), ("enabled", "full")]
                }
            ),
        ]
    ),
    key!(
        Process,
        "wipe_tower",
        Kind::Boolean,
        "Prime tower",
        PrimeTower,
        common,
        [
            d!(B | O | Q, "enable_prime_tower"),
            d!(P, "wipe_tower"),
            d!(C, "prime_tower_enable"),
        ]
    ),
    key!(
        Process,
        "wipe_tower_width",
        Kind::Number(Unit::Mm),
        "Prime tower width",
        PrimeTower,
        common,
        [
            d!(B | O | Q, "prime_tower_width"),
            d!(P, "wipe_tower_width"),
            d!(C, "prime_tower_size"),
        ]
    ),
    key!(
        Process,
        "wipe_tower_brim_width",
        Kind::Number(Unit::Mm),
        "Prime tower brim width",
        PrimeTower,
        advanced,
        [
            d!(B | O | Q, "prime_tower_brim_width"),
            // Bambu Studio's -1 (automatic) is out of PrusaSlicer's range.
            d!(P, "wipe_tower_brim_width", range(0.0, f64::INFINITY)),
        ]
    ),
    key!(
        Process,
        "wipe_tower_extruder",
        Kind::Integer(Unit::None),
        "Prime tower filament",
        PrimeTower,
        advanced,
        [d!(O, "wipe_tower_filament"), d!(P, "wipe_tower_extruder"),]
    ),
    key!(
        Process,
        "wipe_tower_cone_angle",
        Kind::Number(Unit::Degree),
        "Prime tower cone angle",
        PrimeTower,
        advanced,
        [
            d!(O, "wipe_tower_cone_angle"),
            d!(P, "wipe_tower_cone_angle"),
        ]
    ),
    key!(
        Process,
        "wipe_tower_extra_spacing",
        Kind::Percent,
        "Prime tower extra spacing",
        PrimeTower,
        advanced,
        [
            d!(O, "wipe_tower_extra_spacing"),
            d!(P, "wipe_tower_extra_spacing"),
        ]
    ),
    key!(
        Process,
        "wipe_tower_bridging",
        Kind::Number(Unit::Mm),
        "Prime tower bridging",
        PrimeTower,
        advanced,
        [d!(O, "wipe_tower_bridging"), d!(P, "wipe_tower_bridging"),]
    ),
    key!(
        Process,
        "wipe_tower_extra_flow",
        Kind::Percent,
        "Prime tower extra flow",
        PrimeTower,
        advanced,
        [
            d!(O, "wipe_tower_extra_flow"),
            d!(P, "wipe_tower_extra_flow"),
        ]
    ),
    key!(
        Process,
        "wipe_tower_no_sparse_layers",
        Kind::Boolean,
        "No sparse prime tower layers",
        PrimeTower,
        advanced,
        [
            d!(B | O | Q, "wipe_tower_no_sparse_layers"),
            d!(P, "wipe_tower_no_sparse_layers"),
        ]
    ),
    key!(
        Process,
        "prime_tower_enable_framework",
        Kind::Boolean,
        "Prime tower framework",
        PrimeTower,
        advanced,
        [d!(B | O | Q, "prime_tower_enable_framework"),]
    ),
    key!(
        Process,
        "prime_tower_skip_points",
        Kind::Boolean,
        "Prime tower skip points",
        PrimeTower,
        advanced,
        [d!(B | O | Q, "prime_tower_skip_points"),]
    ),
    key!(
        Process,
        "prime_tower_max_speed",
        Kind::Number(Unit::MmPerS),
        "Prime tower top speed",
        PrimeTower,
        advanced,
        [d!(B | Q, "prime_tower_max_speed"),]
    ),
    key!(
        Process,
        "enable_tower_interface_features",
        Kind::Boolean,
        "Prime tower interface features",
        PrimeTower,
        advanced,
        [d!(B | O | Q, "enable_tower_interface_features"),]
    ),
    key!(
        Process,
        "prime_tower_rib_wall",
        Kind::Boolean,
        "Prime tower rib wall",
        PrimeTower,
        advanced,
        [d!(B | Q, "prime_tower_rib_wall"),]
    ),
    key!(
        Process,
        "prime_tower_extra_rib_length",
        Kind::Number(Unit::Mm),
        "Prime tower extra rib length",
        PrimeTower,
        advanced,
        [d!(B | Q, "prime_tower_extra_rib_length"),]
    ),
    key!(
        Process,
        "prime_tower_rib_width",
        Kind::Number(Unit::Mm),
        "Prime tower rib width",
        PrimeTower,
        advanced,
        [d!(B | Q, "prime_tower_rib_width"),]
    ),
    key!(
        Process,
        "prime_tower_fillet_wall",
        Kind::Boolean,
        "Prime tower fillet wall",
        PrimeTower,
        advanced,
        [d!(B | Q, "prime_tower_fillet_wall"),]
    ),
    key!(
        Process,
        "prime_tower_infill_gap",
        Kind::Percent,
        "Prime tower infill gap",
        PrimeTower,
        advanced,
        [d!(B | O | Q, "prime_tower_infill_gap"),]
    ),
    key!(
        Process,
        "prime_tower_lift_speed",
        Kind::Number(Unit::None),
        "Prime tower lift speed",
        PrimeTower,
        advanced,
        [d!(B | Q, "prime_tower_lift_speed"),]
    ),
    key!(
        Process,
        "prime_tower_lift_height",
        Kind::Number(Unit::None),
        "Prime tower lift height",
        PrimeTower,
        advanced,
        [d!(B | Q, "prime_tower_lift_height"),]
    ),
    key!(
        Process,
        "prime_tower_flat_ironing",
        Kind::Boolean,
        "Prime tower flat ironing",
        PrimeTower,
        advanced,
        [d!(B | O | Q, "prime_tower_flat_ironing"),]
    ),
    // The purge volumes' multiplier (mitcad#705): the Bambu family keeps it
    // beside its purge matrix, which it multiplies; PrusaSlicer has none
    // (its matrix is written multiplied, the model's `print_purge`). Read as a
    // list (one per extruder in newer projects), written as one text, which
    // the family's loaders take for both forms (the export's purge table).
    key!(
        Process,
        "flush_multiplier",
        Kind::Number(Unit::None),
        "Purge multiplier",
        Filaments,
        common,
        [d!(B | O | Q, "flush_multiplier", list),]
    ),
    key!(
        Process,
        "perimeter_extruder",
        Kind::Integer(Unit::None),
        "Wall filament",
        Filaments,
        advanced,
        [
            d!(B | Q, "wall_filament"),
            // A 0, as Bambu Studio's projects have it, is out of PrusaSlicer's
            // range: left out, each object's extruder applies.
            d!(P, "perimeter_extruder", range(1.0, f64::INFINITY)),
            d!(C, "wall_extruder_nr", Form::Linear(1.0, -1.0)),
        ]
    ),
    key!(
        Process,
        "infill_extruder",
        Kind::Integer(Unit::None),
        "Sparse infill filament",
        Filaments,
        advanced,
        [
            d!(B | Q, "sparse_infill_filament"),
            // A 0, as Bambu Studio's projects have it, is out of PrusaSlicer's
            // range: left out, each object's extruder applies.
            d!(P, "infill_extruder", range(1.0, f64::INFINITY)),
            d!(C, "infill_extruder_nr", Form::Linear(1.0, -1.0)),
        ]
    ),
    key!(
        Process,
        "solid_infill_extruder",
        Kind::Integer(Unit::None),
        "Solid infill filament",
        Filaments,
        advanced,
        [
            d!(B | Q, "solid_infill_filament"),
            // A 0, as Bambu Studio's projects have it, is out of PrusaSlicer's
            // range: left out, each object's extruder applies.
            d!(P, "solid_infill_extruder", range(1.0, f64::INFINITY)),
        ]
    ),
    key!(
        Process,
        "toolchange_ordering",
        Kind::Choice(&[
            ("default", "Default"),
            ("cyclic", "Cyclic"),
            ("optimized", "Optimized")
        ]),
        "Filament change order",
        Filaments,
        advanced,
        [
            d!(
                O,
                "toolchange_ordering",
                Form::Choices(&[("default", "default"), ("cyclic", "cyclic")])
            ),
            d!(
                P,
                "toolchange_ordering",
                Form::Choices(&[("optimized", "optimized"), ("cyclic", "cyclic")])
            ),
        ]
    ),
    key!(
        Process,
        "single_extruder_multi_material_priming",
        Kind::Boolean,
        "Prime all filaments",
        Filaments,
        advanced,
        [
            d!(O, "single_extruder_multi_material_priming"),
            d!(P, "single_extruder_multi_material_priming"),
        ]
    ),
    key!(
        Process,
        "ooze_prevention",
        Kind::Boolean,
        "Ooze prevention",
        Filaments,
        advanced,
        [d!(B | O | Q, "ooze_prevention"), d!(P, "ooze_prevention"),]
    ),
    key!(
        Process,
        "standby_temperature_delta",
        Kind::Integer(Unit::Celsius),
        "Standby temperature drop",
        Filaments,
        advanced,
        [
            d!(B | O | Q, "standby_temperature_delta"),
            d!(P, "standby_temperature_delta"),
        ]
    ),
    key!(
        Process,
        "flush_into_infill",
        Kind::Boolean,
        "Purge into infill",
        Filaments,
        advanced,
        [d!(B | O | Q, "flush_into_infill"),]
    ),
    key!(
        Process,
        "flush_into_objects",
        Kind::Boolean,
        "Purge into objects",
        Filaments,
        advanced,
        [d!(B | O | Q, "flush_into_objects"),]
    ),
    key!(
        Process,
        "flush_into_support",
        Kind::Boolean,
        "Purge into support",
        Filaments,
        advanced,
        [d!(B | O | Q, "flush_into_support"),]
    ),
    key!(
        Process,
        "interlocking_beam",
        Kind::Boolean,
        "Interlocking beams",
        Interlocking,
        advanced,
        [
            d!(B | O | Q, "interlocking_beam"),
            d!(P, "interlocking_beam"),
            d!(C, "interlocking_enable"),
        ]
    ),
    key!(
        Process,
        "interlocking_beam_width",
        Kind::Number(Unit::Mm),
        "Interlocking beam width",
        Interlocking,
        advanced,
        [
            d!(B | O | Q, "interlocking_beam_width"),
            d!(P, "interlocking_beam_width"),
            d!(C, "interlocking_beam_width", list),
        ]
    ),
    key!(
        Process,
        "interlocking_orientation",
        Kind::Number(Unit::Degree),
        "Interlocking direction",
        Interlocking,
        advanced,
        [
            d!(B | O | Q, "interlocking_orientation"),
            d!(P, "interlocking_orientation"),
            d!(C, "interlocking_orientation"),
        ]
    ),
    key!(
        Process,
        "interlocking_beam_layer_count",
        Kind::Integer(Unit::None),
        "Interlocking beam layers",
        Interlocking,
        advanced,
        [
            d!(B | O | Q, "interlocking_beam_layer_count"),
            d!(P, "interlocking_beam_layer_count"),
            d!(C, "interlocking_beam_layer_count"),
        ]
    ),
    key!(
        Process,
        "interlocking_depth",
        Kind::Integer(Unit::None),
        "Interlocking depth",
        Interlocking,
        advanced,
        [
            d!(B | O | Q, "interlocking_depth"),
            d!(P, "interlocking_depth"),
            d!(C, "interlocking_depth"),
        ]
    ),
    key!(
        Process,
        "interlocking_boundary_avoidance",
        Kind::Integer(Unit::None),
        "Interlocking boundary distance",
        Interlocking,
        advanced,
        [
            d!(B | O | Q, "interlocking_boundary_avoidance"),
            d!(P, "interlocking_boundary_avoidance"),
            d!(C, "interlocking_boundary_avoidance"),
        ]
    ),
    key!(
        Process,
        "mmu_segmented_region_max_width",
        Kind::Number(Unit::Mm),
        "Painted region width limit",
        Interlocking,
        advanced,
        [
            d!(B | O | Q, "mmu_segmented_region_max_width"),
            d!(P, "mmu_segmented_region_max_width"),
        ]
    ),
    key!(
        Process,
        "mmu_segmented_region_interlocking_depth",
        Kind::Number(Unit::Mm),
        "Painted region interlocking depth",
        Interlocking,
        advanced,
        [
            d!(B | O | Q, "mmu_segmented_region_interlocking_depth"),
            d!(P, "mmu_segmented_region_interlocking_depth"),
        ]
    ),
    key!(
        Process,
        "spiral_vase",
        Kind::Boolean,
        "Spiral vase",
        SpecialModes,
        common,
        [
            d!(B | O | Q, "spiral_mode"),
            d!(P, "spiral_vase"),
            d!(C, "magic_spiralize"),
        ]
    ),
    key!(
        Process,
        "complete_objects",
        Kind::Boolean,
        "One object at a time",
        SpecialModes,
        common,
        [
            d!(
                B | O | Q,
                "print_sequence",
                Form::Choices(&[("true", "by object"), ("false", "by layer")])
            ),
            d!(P, "complete_objects"),
            d!(
                C,
                "print_sequence",
                Form::Choices(&[("true", "one_at_a_time"), ("false", "all_at_once")])
            ),
        ]
    ),
    key!(
        Process,
        "spiral_mode_smooth",
        Kind::Boolean,
        "Smooth spiral",
        SpecialModes,
        advanced,
        [
            d!(B | O | Q, "spiral_mode_smooth"),
            d!(C, "smooth_spiralized_contours"),
        ]
    ),
    key!(
        Process,
        "spiral_mode_max_xy_smoothing",
        Kind::OrPercent(Unit::Mm, "nozzle_diameter"),
        "Spiral XY smoothing limit",
        SpecialModes,
        advanced,
        [d!(B | O | Q, "spiral_mode_max_xy_smoothing"),]
    ),
    key!(
        Process,
        "slicing_mode",
        Kind::Choice(&[
            ("regular", "Regular"),
            ("even_odd", "Even-odd"),
            ("close_holes", "Close holes")
        ]),
        "Slicing mode",
        SpecialModes,
        advanced,
        [d!(B | O | Q, "slicing_mode"), d!(P, "slicing_mode"),]
    ),
    key!(
        Process,
        "timelapse_type",
        Kind::Choice(&[("0", "Traditional"), ("1", "Smooth")]),
        "Timelapse",
        SpecialModes,
        advanced,
        [d!(B | O | Q, "timelapse_type"),]
    ),
    key!(
        Process,
        "enable_wrapping_detection",
        Kind::Boolean,
        "Wrap detection",
        SpecialModes,
        advanced,
        [d!(B | O | Q, "enable_wrapping_detection"),]
    ),
    key!(
        Process,
        "enable_order_independent_overlap_carving",
        Kind::Boolean,
        "Overlap carving independent of order",
        SpecialModes,
        advanced,
        [d!(B | Q, "enable_order_independent_overlap_carving"),]
    ),
    key!(
        Process,
        "fuzzy_skin",
        Kind::Choice(&[
            ("none", "None"),
            ("external", "Contour"),
            ("all", "Contour and holes"),
            ("allwalls", "All walls"),
            ("disabled_fuzzy", "Disabled"),
            ("hole", "Holes")
        ]),
        "Fuzzy skin",
        FuzzySkin,
        common,
        [
            d!(
                B | Q,
                "fuzzy_skin",
                Form::Choices(&[
                    ("none", "none"),
                    ("external", "external"),
                    ("all", "all"),
                    ("allwalls", "allwalls"),
                    ("disabled_fuzzy", "disabled_fuzzy")
                ])
            ),
            d!(O, "fuzzy_skin"),
            d!(
                P,
                "fuzzy_skin",
                Form::Choices(&[("none", "none"), ("external", "external"), ("all", "all")])
            ),
            d!(
                C,
                "magic_fuzzy_skin_outside_only",
                Form::Switch {
                    key: "magic_fuzzy_skin_enabled",
                    off: "none",
                    choices: &[("external", "True"), ("all", "False")]
                }
            ),
        ]
    ),
    key!(
        Process,
        "fuzzy_skin_thickness",
        Kind::Number(Unit::Mm),
        "Fuzzy skin thickness",
        FuzzySkin,
        common,
        [
            d!(B | O | Q, "fuzzy_skin_thickness"),
            d!(P, "fuzzy_skin_thickness"),
            d!(C, "magic_fuzzy_skin_thickness"),
        ]
    ),
    key!(
        Process,
        "fuzzy_skin_point_dist",
        Kind::Number(Unit::Mm),
        "Fuzzy skin point distance",
        FuzzySkin,
        common,
        [
            d!(B | O | Q, "fuzzy_skin_point_distance"),
            d!(P, "fuzzy_skin_point_dist"),
            d!(C, "magic_fuzzy_skin_point_dist"),
        ]
    ),
    key!(
        Process,
        "fuzzy_skin_mode",
        Kind::Choice(&[
            ("displacement", "Displacement"),
            ("extrusion", "Extrusion"),
            ("combined", "Combined")
        ]),
        "Fuzzy skin mode",
        FuzzySkin,
        advanced,
        [d!(B | O | Q, "fuzzy_skin_mode"),]
    ),
    key!(
        Process,
        "fuzzy_skin_noise_type",
        Kind::Choice(&[
            ("classic", "Classic"),
            ("perlin", "Perlin"),
            ("billow", "Billow"),
            ("ridgedmulti", "Ridged multifractal"),
            ("voronoi", "Voronoi"),
            ("ripple", "Ripple")
        ]),
        "Fuzzy skin noise type",
        FuzzySkin,
        advanced,
        [
            d!(
                B | Q,
                "fuzzy_skin_noise_type",
                Form::Choices(&[
                    ("classic", "classic"),
                    ("perlin", "perlin"),
                    ("billow", "billow"),
                    ("ridgedmulti", "ridgedmulti"),
                    ("voronoi", "voronoi")
                ])
            ),
            d!(O, "fuzzy_skin_noise_type"),
        ]
    ),
    key!(
        Process,
        "fuzzy_skin_scale",
        Kind::Number(Unit::Mm),
        "Fuzzy skin scale",
        FuzzySkin,
        advanced,
        [d!(B | O | Q, "fuzzy_skin_scale"),]
    ),
    key!(
        Process,
        "fuzzy_skin_octaves",
        Kind::Integer(Unit::None),
        "Fuzzy skin octaves",
        FuzzySkin,
        advanced,
        [d!(B | O | Q, "fuzzy_skin_octaves"),]
    ),
    key!(
        Process,
        "fuzzy_skin_persistence",
        Kind::Number(Unit::None),
        "Fuzzy skin persistence",
        FuzzySkin,
        advanced,
        [d!(B | O | Q, "fuzzy_skin_persistence"),]
    ),
    key!(
        Process,
        "fuzzy_skin_first_layer",
        Kind::Boolean,
        "Fuzzy skin first layer",
        FuzzySkin,
        advanced,
        [d!(B | O | Q, "fuzzy_skin_first_layer"),]
    ),
    key!(
        Process,
        "exclude_object",
        Kind::Boolean,
        "Label objects for exclusion",
        GcodeOutput,
        advanced,
        [d!(B | O | Q, "exclude_object"),]
    ),
    key!(
        Process,
        "reduce_infill_retraction_mode",
        Kind::Choice(&[("disabled", "Off"), ("auto", "Auto"), ("enabled", "On")]),
        "Fewer retractions in infill",
        GcodeOutput,
        advanced,
        [d!(
            B | Q,
            "reduce_infill_retraction_mode",
            Form::Choices(&[
                ("disabled", "Disabled"),
                ("auto", "Auto"),
                ("enabled", "Enabled")
            ])
        ),]
    ),
    key!(
        Process,
        "print_settings_id",
        Kind::Text,
        "Process preset",
        ProcessPresets,
        advanced,
        per_dialect,
        [
            d!(B | O | Q, "print_settings_id"),
            d!(P, "print_settings_id"),
        ]
    ),
    key!(
        Filament,
        "filament_type",
        Kind::Text,
        "Filament type",
        FilamentBasics,
        common,
        [
            d!(B | O | Q, "filament_type", list),
            d!(P, "filament_type", list),
            d!(C, "material_type"),
        ]
    ),
    key!(
        Filament,
        "filament_vendor",
        Kind::Text,
        "Vendor",
        FilamentBasics,
        common,
        [
            d!(B | O | Q, "filament_vendor", list),
            d!(P, "filament_vendor"),
            d!(C, "material_brand"),
        ]
    ),
    key!(
        Filament,
        "filament_colour",
        Kind::Color,
        "Filament color",
        FilamentBasics,
        common,
        [
            d!(B | O | Q, "filament_colour", list),
            d!(P, "filament_colour", list),
        ]
    ),
    key!(
        Filament,
        "filament_diameter",
        Kind::Number(Unit::Mm),
        "Filament diameter",
        FilamentBasics,
        common,
        [
            d!(B | O | Q, "filament_diameter", list),
            d!(P, "filament_diameter", list),
            d!(C, "material_diameter", list),
        ]
    ),
    key!(
        Filament,
        "filament_density",
        Kind::Number(Unit::GPerCm3),
        "Filament density",
        FilamentBasics,
        common,
        [
            d!(B | O | Q, "filament_density", list),
            d!(P, "filament_density", list),
        ]
    ),
    key!(
        Filament,
        "filament_cost",
        Kind::Number(Unit::PerKg),
        "Filament price per kg",
        FilamentBasics,
        common,
        [
            d!(B | O | Q, "filament_cost", list),
            d!(P, "filament_cost", list),
        ]
    ),
    key!(
        Filament,
        "filament_soluble",
        Kind::Boolean,
        "Soluble filament",
        FilamentBasics,
        advanced,
        [
            d!(B | O | Q, "filament_soluble", list),
            d!(P, "filament_soluble", list),
        ]
    ),
    key!(
        Filament,
        "filament_is_support",
        Kind::Boolean,
        "Support filament",
        FilamentBasics,
        advanced,
        [
            d!(B | O | Q, "filament_is_support", list),
            d!(C, "material_is_support_material", list),
        ]
    ),
    key!(
        Filament,
        "filament_shrink",
        Kind::Percent,
        "Shrinkage in XY",
        FilamentBasics,
        advanced,
        [d!(B | O | Q, "filament_shrink", list),]
    ),
    key!(
        Filament,
        "filament_shrinkage_compensation_z",
        Kind::Percent,
        "Shrinkage compensation in Z",
        FilamentBasics,
        advanced,
        [
            d!(O, "filament_shrinkage_compensation_z", list),
            d!(P, "filament_shrinkage_compensation_z", list),
        ]
    ),
    key!(
        Filament,
        "temperature_vitrification",
        Kind::Integer(Unit::None),
        "Softening temperature",
        FilamentBasics,
        advanced,
        [d!(B | O | Q, "temperature_vitrification", list),]
    ),
    key!(
        Filament,
        "impact_strength_z",
        Kind::Number(Unit::None),
        "Impact strength along Z",
        FilamentBasics,
        advanced,
        [d!(B | Q, "impact_strength_z", list),]
    ),
    key!(
        Filament,
        "filament_adhesiveness_category",
        Kind::Integer(Unit::None),
        "Adhesiveness category",
        FilamentBasics,
        advanced,
        [d!(B | O | Q, "filament_adhesiveness_category", list),]
    ),
    key!(
        Filament,
        "filament_metal_stickiness",
        Kind::Choice(&[
            ("none", "None"),
            ("low", "Low"),
            ("medium", "Medium"),
            ("high", "High")
        ]),
        "Stickiness to metal",
        FilamentBasics,
        advanced,
        [d!(
            B | Q,
            "filament_metal_stickiness",
            list,
            Form::Choices(&[
                ("none", "None"),
                ("low", "Low"),
                ("medium", "Medium"),
                ("high", "High")
            ])
        ),]
    ),
    key!(
        Filament,
        "extrusion_multiplier",
        Kind::Number(Unit::None),
        "Flow ratio",
        Flow,
        common,
        [
            d!(B | O | Q, "filament_flow_ratio", list),
            d!(P, "extrusion_multiplier", list),
            d!(C, "material_flow", Form::Linear(100.0, 0.0)),
        ]
    ),
    key!(
        Filament,
        "enable_pressure_advance",
        Kind::Boolean,
        "Enable pressure advance",
        Flow,
        advanced,
        [d!(B | O | Q, "enable_pressure_advance", list),]
    ),
    key!(
        Filament,
        "pressure_advance",
        Kind::Number(Unit::None),
        "Pressure advance",
        Flow,
        advanced,
        [
            d!(B | O | Q, "pressure_advance", list),
            d!(C, "material_pressure_advance_factor", list),
        ]
    ),
    key!(
        Filament,
        "filament_velocity_adaptation_factor",
        Kind::Number(Unit::None),
        "Speed adaptation factor",
        Flow,
        advanced,
        [d!(B | Q, "filament_velocity_adaptation_factor", list),]
    ),
    key!(
        Filament,
        "first_layer_temperature",
        Kind::Number(Unit::Celsius),
        "First layer nozzle temperature",
        NozzleTemperature,
        common,
        [
            d!(
                B | O | Q,
                "nozzle_temperature_initial_layer",
                list,
                Form::Int
            ),
            d!(P, "first_layer_temperature", list, Form::Int),
            d!(C, "material_print_temperature_layer_0", list),
        ]
    ),
    key!(
        Filament,
        "temperature",
        Kind::Number(Unit::Celsius),
        "Nozzle temperature",
        NozzleTemperature,
        common,
        [
            d!(B | O | Q, "nozzle_temperature", list, Form::Int),
            d!(P, "temperature", list, Form::Int),
            d!(C, "material_print_temperature", list),
        ]
    ),
    key!(
        Filament,
        "nozzle_temperature_range_low",
        Kind::Integer(Unit::Celsius),
        "Lowest recommended nozzle temperature",
        NozzleTemperature,
        advanced,
        [d!(B | O | Q, "nozzle_temperature_range_low", list),]
    ),
    key!(
        Filament,
        "nozzle_temperature_range_high",
        Kind::Integer(Unit::Celsius),
        "Highest recommended nozzle temperature",
        NozzleTemperature,
        advanced,
        [d!(B | O | Q, "nozzle_temperature_range_high", list),]
    ),
    key!(
        Filament,
        "idle_temperature",
        Kind::Number(Unit::Celsius),
        "Idle temperature",
        NozzleTemperature,
        advanced,
        [
            d!(O, "idle_temperature", list, Form::Int),
            d!(P, "idle_temperature", list, Form::Int),
            d!(C, "material_standby_temperature", list),
        ]
    ),
    key!(
        Filament,
        "first_layer_bed_temperature",
        Kind::Number(Unit::Celsius),
        "First layer bed temperature",
        BedTemperature,
        common,
        [
            d!(P, "first_layer_bed_temperature", list, Form::Int),
            d!(C, "material_bed_temperature_layer_0"),
        ]
    ),
    key!(
        Filament,
        "bed_temperature",
        Kind::Number(Unit::Celsius),
        "Bed temperature",
        BedTemperature,
        common,
        [
            d!(P, "bed_temperature", list, Form::Int),
            d!(C, "material_bed_temperature"),
        ]
    ),
    key!(
        Filament,
        "cool_plate_temp_initial_layer",
        Kind::Number(Unit::Celsius),
        "Cool plate first layer temperature",
        BedTemperature,
        advanced,
        [d!(
            B | O | Q,
            "cool_plate_temp_initial_layer",
            list,
            Form::Int
        ),]
    ),
    key!(
        Filament,
        "cool_plate_temp",
        Kind::Number(Unit::Celsius),
        "Cool plate temperature",
        BedTemperature,
        advanced,
        [d!(B | O | Q, "cool_plate_temp", list, Form::Int),]
    ),
    key!(
        Filament,
        "eng_plate_temp_initial_layer",
        Kind::Number(Unit::Celsius),
        "Engineering plate first layer temperature",
        BedTemperature,
        advanced,
        [d!(
            B | O | Q,
            "eng_plate_temp_initial_layer",
            list,
            Form::Int
        ),]
    ),
    key!(
        Filament,
        "eng_plate_temp",
        Kind::Number(Unit::Celsius),
        "Engineering plate temperature",
        BedTemperature,
        advanced,
        [d!(B | O | Q, "eng_plate_temp", list, Form::Int),]
    ),
    key!(
        Filament,
        "hot_plate_temp_initial_layer",
        Kind::Number(Unit::Celsius),
        "High temperature plate first layer temperature",
        BedTemperature,
        advanced,
        [d!(
            B | O | Q,
            "hot_plate_temp_initial_layer",
            list,
            Form::Int
        ),]
    ),
    key!(
        Filament,
        "hot_plate_temp",
        Kind::Number(Unit::Celsius),
        "High temperature plate temperature",
        BedTemperature,
        advanced,
        [d!(B | O | Q, "hot_plate_temp", list, Form::Int),]
    ),
    key!(
        Filament,
        "textured_plate_temp_initial_layer",
        Kind::Number(Unit::Celsius),
        "Textured plate first layer temperature",
        BedTemperature,
        advanced,
        [d!(
            B | O | Q,
            "textured_plate_temp_initial_layer",
            list,
            Form::Int
        ),]
    ),
    key!(
        Filament,
        "textured_plate_temp",
        Kind::Number(Unit::Celsius),
        "Textured plate temperature",
        BedTemperature,
        advanced,
        [d!(B | O | Q, "textured_plate_temp", list, Form::Int),]
    ),
    key!(
        Filament,
        "adhesive_cool_plate_temp_initial_layer",
        Kind::Number(Unit::Celsius),
        "Adhesive cool plate first layer temperature",
        BedTemperature,
        advanced,
        [d!(
            B | O | Q,
            "supertack_plate_temp_initial_layer",
            list,
            Form::Int
        ),]
    ),
    key!(
        Filament,
        "adhesive_cool_plate_temp",
        Kind::Number(Unit::Celsius),
        "Adhesive cool plate temperature",
        BedTemperature,
        advanced,
        [d!(B | O | Q, "supertack_plate_temp", list, Form::Int),]
    ),
    key!(
        Filament,
        "chamber_temperature",
        Kind::Number(Unit::Celsius),
        "Chamber temperature",
        Chamber,
        advanced,
        [
            d!(B | Q, "chamber_temperatures", list, Form::Int),
            d!(O, "chamber_temperature", list, Form::Int),
            d!(P, "chamber_temperature", list, Form::Int),
            d!(C, "build_volume_temperature"),
        ]
    ),
    key!(
        Filament,
        "chamber_minimal_temperature",
        Kind::Number(Unit::Celsius),
        "Lowest chamber temperature",
        Chamber,
        advanced,
        [
            d!(O, "chamber_minimal_temperature", list, Form::Int),
            d!(P, "chamber_minimal_temperature", list, Form::Int),
        ]
    ),
    key!(
        Filament,
        "filament_max_volumetric_speed",
        Kind::Number(Unit::Mm3PerS),
        "Highest volumetric speed",
        VolumetricSpeed,
        common,
        [
            d!(B | O | Q, "filament_max_volumetric_speed", list),
            d!(P, "filament_max_volumetric_speed", list),
            d!(C, "material_max_flowrate", list),
        ]
    ),
    key!(
        Filament,
        "filament_adaptive_volumetric_speed",
        Kind::Boolean,
        "Adaptive volumetric speed",
        VolumetricSpeed,
        advanced,
        [d!(B | O | Q, "filament_adaptive_volumetric_speed", list),]
    ),
    key!(
        Filament,
        "filament_ramming_volumetric_speed",
        Kind::Number(Unit::Mm3PerS),
        "Ramming volumetric speed",
        VolumetricSpeed,
        advanced,
        [d!(B | O | Q, "filament_ramming_volumetric_speed", list),]
    ),
    key!(
        Filament,
        "filament_ramming_volumetric_speed_nc",
        Kind::Number(Unit::Mm3PerS),
        "Ramming volumetric speed on nozzle change",
        VolumetricSpeed,
        advanced,
        [d!(B | O | Q, "filament_ramming_volumetric_speed_nc", list),]
    ),
    key!(
        Filament,
        "filament_scarf_seam_type",
        Kind::Choice(&[
            ("none", "None"),
            ("external", "Contour"),
            ("all", "Contour and holes")
        ]),
        "Scarf seams (filament)",
        ScarfSeam,
        advanced,
        [d!(B | Q, "filament_scarf_seam_type", list),]
    ),
    key!(
        Filament,
        "filament_scarf_height",
        Kind::OrPercent(Unit::Mm, "layer_height"),
        "Scarf start height (filament)",
        ScarfSeam,
        advanced,
        [d!(B | Q, "filament_scarf_height", list),]
    ),
    key!(
        Filament,
        "filament_scarf_gap",
        Kind::OrPercent(Unit::Mm, "nozzle_diameter"),
        "Scarf gap (filament)",
        ScarfSeam,
        advanced,
        [d!(B | Q, "filament_scarf_gap", list),]
    ),
    key!(
        Filament,
        "filament_scarf_length",
        Kind::Number(Unit::Mm),
        "Scarf length (filament)",
        ScarfSeam,
        advanced,
        [d!(B | Q, "filament_scarf_length", list),]
    ),
    // Mitcad's own (mitcad#705, `MITCAD_OWN`): the export makes the
    // slicers' purge matrices of them.
    key!(
        Filament,
        "purge_into",
        Kind::Number(Unit::Mm3),
        "Purge into this filament",
        Purge,
        common,
        []
    ),
    key!(
        Filament,
        "purge_out",
        Kind::Number(Unit::Mm3),
        "Purge out of this filament",
        Purge,
        common,
        []
    ),
    key!(
        Filament,
        "filament_settings_id",
        Kind::Text,
        "Filament preset",
        FilamentPresets,
        advanced,
        per_dialect,
        [
            d!(B | O | Q, "filament_settings_id", list),
            d!(P, "filament_settings_id", list),
        ]
    ),
    key!(
        Filament,
        "min_fan_speed",
        Kind::Number(Unit::Percent),
        "Lowest fan speed",
        PartCoolingFan,
        common,
        [
            d!(B | Q, "fan_min_speed", list, Form::Int),
            d!(O, "fan_min_speed", list),
            d!(P, "min_fan_speed", list, Form::Int),
            d!(C, "cool_fan_speed_min", list),
        ]
    ),
    key!(
        Filament,
        "max_fan_speed",
        Kind::Number(Unit::Percent),
        "Highest fan speed",
        PartCoolingFan,
        common,
        [
            d!(B | Q, "fan_max_speed", list, Form::Int),
            d!(O, "fan_max_speed", list),
            d!(P, "max_fan_speed", list, Form::Int),
            d!(C, "cool_fan_speed_max", list),
        ]
    ),
    key!(
        Filament,
        "fan_below_layer_time",
        Kind::Number(Unit::Second),
        "Fan below layer time",
        PartCoolingFan,
        common,
        [
            d!(B | Q, "fan_cooling_layer_time", list, Form::Int),
            d!(O, "fan_cooling_layer_time", list),
            d!(P, "fan_below_layer_time", list, Form::Int),
        ]
    ),
    key!(
        Filament,
        "disable_fan_first_layers",
        Kind::Integer(Unit::Layers),
        "No fan for the first layers",
        PartCoolingFan,
        common,
        [
            d!(B | O | Q, "close_fan_the_first_x_layers", list),
            d!(P, "disable_fan_first_layers", list),
        ]
    ),
    key!(
        Filament,
        "full_fan_speed_layer",
        Kind::Integer(Unit::None),
        "Full fan speed from layer",
        PartCoolingFan,
        advanced,
        [
            d!(B | O | Q, "full_fan_speed_layer", list),
            d!(P, "full_fan_speed_layer", list),
            d!(C, "cool_fan_full_layer", list),
        ]
    ),
    key!(
        Filament,
        "initial_layer_fan_speed",
        Kind::Number(Unit::Percent),
        "First layer fan speed",
        PartCoolingFan,
        advanced,
        [
            d!(O, "initial_layer_fan_speed", list, Form::Int),
            // OrcaSlicer's -1 (not set) is no fan speed of Cura's.
            d!(C, "cool_fan_speed_0", list, range(0.0, 100.0)),
        ]
    ),
    key!(
        Filament,
        "first_x_layer_part_fan_speed",
        Kind::Integer(Unit::Percent),
        "Part fan speed on the first layers",
        PartCoolingFan,
        advanced,
        [d!(B | Q, "first_x_layer_part_fan_speed", list),]
    ),
    key!(
        Filament,
        "reduce_fan_stop_start_freq",
        Kind::Boolean,
        "Keep the fan running",
        PartCoolingFan,
        advanced,
        [d!(B | O | Q, "reduce_fan_stop_start_freq", list),]
    ),
    key!(
        Filament,
        "pre_start_fan_time",
        Kind::Number(Unit::Second),
        "Fan start ahead time",
        PartCoolingFan,
        advanced,
        [d!(B | Q, "pre_start_fan_time", list),]
    ),
    key!(
        Filament,
        "cooling",
        Kind::Boolean,
        "Slow down for cooling",
        SlowDownForCooling,
        common,
        [
            d!(B | O | Q, "slow_down_for_layer_cooling", list),
            d!(P, "cooling", list),
        ]
    ),
    key!(
        Filament,
        "slowdown_below_layer_time",
        Kind::Number(Unit::Second),
        "Slow down below layer time",
        SlowDownForCooling,
        common,
        [
            d!(B | Q, "slow_down_layer_time", list, Form::Int),
            d!(O, "slow_down_layer_time", list),
            d!(P, "slowdown_below_layer_time", list, Form::Int),
            d!(C, "cool_min_layer_time", list),
        ]
    ),
    key!(
        Filament,
        "min_print_speed",
        Kind::Number(Unit::MmPerS),
        "Lowest print speed",
        SlowDownForCooling,
        common,
        [
            d!(B | O | Q, "slow_down_min_speed", list),
            d!(P, "min_print_speed", list),
            d!(C, "cool_min_speed", list),
        ]
    ),
    key!(
        Filament,
        "no_slow_down_for_cooling_on_outwalls",
        Kind::Boolean,
        "No cooling slowdown on outer walls",
        SlowDownForCooling,
        advanced,
        [d!(B | Q, "no_slow_down_for_cooling_on_outwalls", list),]
    ),
    key!(
        Filament,
        "cooling_slowdown_logic",
        Kind::Choice(&[
            ("uniform_cooling", "Uniform cooling"),
            ("consistent_surface", "Consistent surface")
        ]),
        "Cooling slowdown logic",
        SlowDownForCooling,
        advanced,
        [
            d!(B | Q, "cooling_slowdown_logic", list),
            d!(P, "cooling_slowdown_logic", list),
        ]
    ),
    key!(
        Filament,
        "cooling_perimeter_transition_distance",
        Kind::Number(Unit::Mm),
        "Cooling wall transition distance",
        SlowDownForCooling,
        advanced,
        [
            d!(B | Q, "cooling_perimeter_transition_distance", list),
            d!(P, "cooling_perimeter_transition_distance", list),
        ]
    ),
    key!(
        Filament,
        "enable_overhang_bridge_fan",
        Kind::Boolean,
        "Fan for overhangs and bridges",
        OverhangsAndBridges,
        common,
        [d!(B | O | Q, "enable_overhang_bridge_fan", list),]
    ),
    key!(
        Filament,
        "overhang_fan_speed",
        Kind::Integer(Unit::Percent),
        "Overhang fan speed",
        OverhangsAndBridges,
        common,
        [d!(B | O | Q, "overhang_fan_speed", list),]
    ),
    key!(
        Filament,
        "bridge_fan_speed",
        Kind::Number(Unit::Percent),
        "Bridge fan speed",
        OverhangsAndBridges,
        common,
        [
            d!(P, "bridge_fan_speed", list, Form::Int),
            d!(C, "bridge_fan_speed"),
        ]
    ),
    key!(
        Filament,
        "overhang_fan_threshold",
        Kind::Choice(&[
            ("0", "0%"),
            ("10", "10%"),
            ("25", "25%"),
            ("50", "50%"),
            ("75", "75%"),
            ("95", "95% (bridges)")
        ]),
        "Overhang fan threshold",
        OverhangsAndBridges,
        advanced,
        [d!(
            B | O | Q,
            "overhang_fan_threshold",
            list,
            Form::Choices(&[
                ("0", "0%"),
                ("10", "10%"),
                ("25", "25%"),
                ("50", "50%"),
                ("75", "75%"),
                ("95", "95%")
            ])
        ),]
    ),
    // The slicers' values are those of the key above: 0 % to 75 % and
    // 95 % (bridges), no 100 % (mitcad#654). Mitcad's 100 stays a value a
    // design may hold; no slicer takes it, so an export warns.
    key!(
        Filament,
        "overhang_threshold_participating_cooling",
        Kind::Choice(&[
            ("0", "0%"),
            ("10", "10%"),
            ("25", "25%"),
            ("50", "50%"),
            ("75", "75%"),
            ("95", "95% (bridges)"),
            ("100", "100%")
        ]),
        "Overhangs slowed for cooling",
        OverhangsAndBridges,
        advanced,
        [d!(
            B | Q,
            "overhang_threshold_participating_cooling",
            list,
            Form::Choices(&[
                ("0", "0%"),
                ("10", "10%"),
                ("25", "25%"),
                ("50", "50%"),
                ("75", "75%"),
                ("95", "95%")
            ])
        ),]
    ),
    key!(
        Filament,
        "ironing_fan_speed",
        Kind::Integer(Unit::Percent),
        "Ironing fan speed",
        OverhangsAndBridges,
        advanced,
        [d!(B | O | Q, "ironing_fan_speed", list),]
    ),
    key!(
        Filament,
        "additional_cooling_fan_speed",
        Kind::Integer(Unit::Percent),
        "Auxiliary fan speed",
        AuxiliaryFan,
        advanced,
        [d!(B | O | Q, "additional_cooling_fan_speed", list),]
    ),
    key!(
        Filament,
        "close_additional_fan_first_x_layers",
        Kind::Integer(Unit::Layers),
        "No auxiliary fan for the first layers",
        AuxiliaryFan,
        advanced,
        [d!(B | O | Q, "close_additional_fan_first_x_layers", list),]
    ),
    key!(
        Filament,
        "first_x_layer_fan_speed",
        Kind::Number(Unit::Percent),
        "Fan speed on the first layers",
        AuxiliaryFan,
        advanced,
        [d!(B | O | Q, "first_x_layer_fan_speed", list),]
    ),
    key!(
        Filament,
        "additional_fan_full_speed_layer",
        Kind::Integer(Unit::None),
        "Auxiliary fan full speed from layer",
        AuxiliaryFan,
        advanced,
        [d!(B | O | Q, "additional_fan_full_speed_layer", list),]
    ),
    key!(
        Filament,
        "activate_air_filtration",
        Kind::Boolean,
        "Air filtration",
        ExhaustFan,
        advanced,
        [d!(B | O | Q, "activate_air_filtration", list),]
    ),
    key!(
        Filament,
        "during_print_exhaust_fan_speed",
        Kind::Integer(Unit::Percent),
        "Exhaust fan speed while printing",
        ExhaustFan,
        advanced,
        [d!(B | O | Q, "during_print_exhaust_fan_speed", list),]
    ),
    key!(
        Filament,
        "complete_print_exhaust_fan_speed",
        Kind::Integer(Unit::Percent),
        "Exhaust fan speed after printing",
        ExhaustFan,
        advanced,
        [d!(B | O | Q, "complete_print_exhaust_fan_speed", list),]
    ),
    key!(
        Filament,
        "filament_retract_length",
        Kind::Number(Unit::Mm),
        "Retraction length",
        Retraction,
        common,
        [
            d!(B | O | Q, "filament_retraction_length", list),
            d!(P, "filament_retract_length", list),
            d!(C, "retraction_amount", list),
        ]
    ),
    key!(
        Filament,
        "filament_retract_lift",
        Kind::Number(Unit::Mm),
        "Z hop",
        Retraction,
        common,
        [
            d!(B | O | Q, "filament_z_hop", list),
            d!(P, "filament_retract_lift", list),
            d!(C, "retraction_hop", list),
        ]
    ),
    key!(
        Filament,
        "filament_retract_speed",
        Kind::Number(Unit::MmPerS),
        "Retraction speed",
        Retraction,
        common,
        [
            d!(B | O | Q, "filament_retraction_speed", list),
            d!(P, "filament_retract_speed", list),
            d!(C, "retraction_retract_speed", list),
        ]
    ),
    key!(
        Filament,
        "filament_deretract_speed",
        Kind::Number(Unit::MmPerS),
        "Deretraction speed",
        Retraction,
        advanced,
        [
            d!(B | O | Q, "filament_deretraction_speed", list),
            d!(P, "filament_deretract_speed", list),
            d!(C, "retraction_prime_speed", list),
        ]
    ),
    key!(
        Filament,
        "filament_retract_restart_extra",
        Kind::Number(Unit::Mm),
        "Extra length on restart",
        Retraction,
        advanced,
        [
            d!(B | O | Q, "filament_retract_restart_extra", list),
            d!(P, "filament_retract_restart_extra", list),
        ]
    ),
    key!(
        Filament,
        "filament_retract_before_travel",
        Kind::Number(Unit::Mm),
        "Shortest travel to retract",
        Retraction,
        advanced,
        [
            d!(B | O | Q, "filament_retraction_minimum_travel", list),
            d!(P, "filament_retract_before_travel", list),
            d!(C, "retraction_min_travel", list),
        ]
    ),
    key!(
        Filament,
        "filament_retract_layer_change",
        Kind::Boolean,
        "Retract on layer change",
        Retraction,
        advanced,
        [
            d!(B | O | Q, "filament_retract_when_changing_layer", list),
            d!(P, "filament_retract_layer_change", list),
            d!(C, "retract_at_layer_change", list),
        ]
    ),
    key!(
        Filament,
        "filament_wipe",
        Kind::Boolean,
        "Wipe while retracting",
        Retraction,
        advanced,
        [
            d!(B | O | Q, "filament_wipe", list),
            d!(P, "filament_wipe", list),
        ]
    ),
    key!(
        Filament,
        "filament_wipe_distance",
        Kind::Number(Unit::Mm),
        "Wipe distance",
        Retraction,
        advanced,
        [d!(B | O | Q, "filament_wipe_distance", list),]
    ),
    key!(
        Filament,
        "filament_retract_before_wipe",
        Kind::Percent,
        "Retract before wipe",
        Retraction,
        advanced,
        [
            d!(B | O | Q, "filament_retract_before_wipe", list),
            d!(P, "filament_retract_before_wipe", list),
        ]
    ),
    key!(
        Filament,
        "filament_retract_lift_above",
        Kind::Number(Unit::Mm),
        "Z hop only above",
        Retraction,
        advanced,
        [
            d!(O, "filament_retract_lift_above", list),
            d!(P, "filament_retract_lift_above", list),
        ]
    ),
    key!(
        Filament,
        "filament_retract_lift_below",
        Kind::Number(Unit::Mm),
        "Z hop only below",
        Retraction,
        advanced,
        [
            d!(O, "filament_retract_lift_below", list),
            d!(P, "filament_retract_lift_below", list),
        ]
    ),
    key!(
        Filament,
        "filament_z_hop_types",
        Kind::Choice(&[
            ("auto_lift", "Auto"),
            ("normal_lift", "Normal"),
            ("slope_lift", "Slope"),
            ("spiral_lift", "Spiral")
        ]),
        "Z hop type",
        Retraction,
        advanced,
        [d!(
            B | O | Q,
            "filament_z_hop_types",
            list,
            Form::Choices(&[
                ("auto_lift", "Auto Lift"),
                ("normal_lift", "Normal Lift"),
                ("slope_lift", "Slope Lift"),
                ("spiral_lift", "Spiral Lift")
            ])
        ),]
    ),
    key!(
        Filament,
        "filament_long_retractions_when_cut",
        Kind::Boolean,
        "Long retraction when cutting",
        Retraction,
        advanced,
        [d!(B | O | Q, "filament_long_retractions_when_cut", list),]
    ),
    key!(
        Filament,
        "filament_retraction_distances_when_cut",
        Kind::Number(Unit::None),
        "Retraction length when cutting",
        Retraction,
        advanced,
        [d!(
            B | O | Q,
            "filament_retraction_distances_when_cut",
            list
        ),]
    ),
    key!(
        Filament,
        "filament_retract_length_toolchange",
        Kind::Number(Unit::Mm),
        "Retraction on filament change",
        ToolchangeRetraction,
        advanced,
        [
            d!(O, "filament_retract_length_toolchange", list),
            d!(P, "filament_retract_length_toolchange", list),
        ]
    ),
    key!(
        Filament,
        "filament_retract_restart_extra_toolchange",
        Kind::Number(Unit::Mm),
        "Extra length after filament change",
        ToolchangeRetraction,
        advanced,
        [
            d!(O, "filament_retract_restart_extra_toolchange", list),
            d!(P, "filament_retract_restart_extra_toolchange", list),
        ]
    ),
    key!(
        Filament,
        "filament_retract_length_nc",
        Kind::Number(Unit::Mm),
        "Retraction on nozzle change",
        ToolchangeRetraction,
        advanced,
        [d!(B | O | Q, "filament_retract_length_nc", list),]
    ),
    key!(
        Filament,
        "long_retractions_when_ec",
        Kind::Boolean,
        "Long retraction on extruder change",
        ToolchangeRetraction,
        advanced,
        [d!(B | O | Q, "long_retractions_when_ec", list),]
    ),
    key!(
        Filament,
        "retraction_distances_when_ec",
        Kind::Number(Unit::Mm),
        "Retraction length on extruder change",
        ToolchangeRetraction,
        advanced,
        [d!(B | O | Q, "retraction_distances_when_ec", list),]
    ),
    key!(
        Filament,
        "override_process_overhang_speed",
        Kind::Boolean,
        "Filament overrides overhang speeds",
        OverrideSpeed,
        advanced,
        [d!(B | Q, "override_process_overhang_speed", list),]
    ),
    key!(
        Filament,
        "filament_enable_overhang_speed",
        Kind::Boolean,
        "Filament overhang speeds",
        OverrideSpeed,
        advanced,
        [d!(B | Q, "filament_enable_overhang_speed", list),]
    ),
    key!(
        Filament,
        "filament_overhang_1_4_speed",
        Kind::Number(Unit::MmPerS),
        "Overhang speed up to 25 % (filament)",
        OverrideSpeed,
        advanced,
        [d!(B | Q, "filament_overhang_1_4_speed", list),]
    ),
    key!(
        Filament,
        "filament_overhang_2_4_speed",
        Kind::Number(Unit::MmPerS),
        "Overhang speed 25 to 50 % (filament)",
        OverrideSpeed,
        advanced,
        [d!(B | Q, "filament_overhang_2_4_speed", list),]
    ),
    key!(
        Filament,
        "filament_overhang_3_4_speed",
        Kind::Number(Unit::MmPerS),
        "Overhang speed 50 to 75 % (filament)",
        OverrideSpeed,
        advanced,
        [d!(B | Q, "filament_overhang_3_4_speed", list),]
    ),
    key!(
        Filament,
        "filament_overhang_4_4_speed",
        Kind::Number(Unit::MmPerS),
        "Overhang speed 75 to 100 % (filament)",
        OverrideSpeed,
        advanced,
        [d!(B | Q, "filament_overhang_4_4_speed", list),]
    ),
    key!(
        Filament,
        "filament_overhang_totally_speed",
        Kind::Number(Unit::MmPerS),
        "Fully overhanging speed (filament)",
        OverrideSpeed,
        advanced,
        [d!(B | Q, "filament_overhang_totally_speed", list),]
    ),
    key!(
        Filament,
        "filament_bridge_speed",
        Kind::Number(Unit::MmPerS),
        "Bridge speed (filament)",
        OverrideSpeed,
        advanced,
        [d!(B | Q, "filament_bridge_speed", list),]
    ),
    key!(
        Filament,
        "filament_flush_temp",
        Kind::Integer(Unit::Celsius),
        "Flush temperature",
        FilamentChange,
        advanced,
        [d!(B | O | Q, "filament_flush_temp", list),]
    ),
    key!(
        Filament,
        "filament_flush_temp_fast",
        Kind::Integer(Unit::Celsius),
        "Fast flush temperature",
        FilamentChange,
        advanced,
        [d!(B | O | Q, "filament_flush_temp_fast", list),]
    ),
    key!(
        Filament,
        "filament_flush_volumetric_speed",
        Kind::Number(Unit::Mm3PerS),
        "Flush volumetric speed",
        FilamentChange,
        advanced,
        [d!(B | O | Q, "filament_flush_volumetric_speed", list),]
    ),
    key!(
        Filament,
        "filament_minimal_purge_on_wipe_tower",
        Kind::Number(Unit::Mm3),
        "Least purge on the prime tower",
        FilamentChange,
        advanced,
        [
            d!(B | O | Q, "filament_minimal_purge_on_wipe_tower", list),
            d!(P, "filament_minimal_purge_on_wipe_tower", list),
            d!(C, "prime_tower_min_volume", list),
        ]
    ),
    key!(
        Filament,
        "filament_prime_volume",
        Kind::Number(Unit::Mm3),
        "Prime volume",
        FilamentChange,
        advanced,
        [d!(B | O | Q, "filament_prime_volume", list),]
    ),
    key!(
        Filament,
        "filament_prime_volume_nc",
        Kind::Number(Unit::Mm3),
        "Prime volume on nozzle change",
        FilamentChange,
        advanced,
        [d!(B | O | Q, "filament_prime_volume_nc", list),]
    ),
    key!(
        Filament,
        "filament_change_length",
        Kind::Number(Unit::Mm),
        "Filament change length",
        FilamentChange,
        advanced,
        [d!(B | O | Q, "filament_change_length", list),]
    ),
    key!(
        Filament,
        "filament_change_length_nc",
        Kind::Number(Unit::Mm),
        "Filament change length on nozzle change",
        FilamentChange,
        advanced,
        [d!(B | O | Q, "filament_change_length_nc", list),]
    ),
    key!(
        Filament,
        "filament_preheat_temperature_delta",
        Kind::Number(Unit::Celsius),
        "Preheat temperature drop",
        FilamentChange,
        advanced,
        [d!(B | O | Q, "filament_preheat_temperature_delta", list),]
    ),
    key!(
        Filament,
        "filament_cooling_before_tower",
        Kind::Number(Unit::Celsius),
        "Cooling before the prime tower",
        FilamentPrimeTower,
        advanced,
        [d!(B | O | Q, "filament_cooling_before_tower", list),]
    ),
    key!(
        Filament,
        "filament_tower_interface_pre_extrusion_dist",
        Kind::Number(Unit::Mm),
        "Prime tower interface pre-extrusion distance",
        FilamentPrimeTower,
        advanced,
        [d!(
            B | O | Q,
            "filament_tower_interface_pre_extrusion_dist",
            list
        ),]
    ),
    key!(
        Filament,
        "filament_tower_interface_pre_extrusion_length",
        Kind::Number(Unit::Mm),
        "Prime tower interface pre-extrusion length",
        FilamentPrimeTower,
        advanced,
        [d!(
            B | O | Q,
            "filament_tower_interface_pre_extrusion_length",
            list
        ),]
    ),
    key!(
        Filament,
        "filament_tower_ironing_area",
        Kind::Number(Unit::Mm2),
        "Prime tower ironing area",
        FilamentPrimeTower,
        advanced,
        [d!(B | O | Q, "filament_tower_ironing_area", list),]
    ),
    key!(
        Filament,
        "filament_tower_interface_purge_volume",
        Kind::Number(Unit::Mm),
        "Prime tower interface purge volume",
        FilamentPrimeTower,
        advanced,
        [d!(B | O | Q, "filament_tower_interface_purge_volume", list),]
    ),
    key!(
        Filament,
        "filament_tower_interface_print_temp",
        Kind::Integer(Unit::Celsius),
        "Prime tower interface temperature",
        FilamentPrimeTower,
        advanced,
        [d!(B | O | Q, "filament_tower_interface_print_temp", list),]
    ),
    key!(
        Filament,
        "filament_loading_speed",
        Kind::Number(Unit::MmPerS),
        "Loading speed",
        LoadingAndRamming,
        advanced,
        [
            d!(O, "filament_loading_speed", list),
            d!(P, "filament_loading_speed", list),
        ]
    ),
    key!(
        Filament,
        "filament_loading_speed_start",
        Kind::Number(Unit::MmPerS),
        "Loading speed at the start",
        LoadingAndRamming,
        advanced,
        [
            d!(O, "filament_loading_speed_start", list),
            d!(P, "filament_loading_speed_start", list),
        ]
    ),
    key!(
        Filament,
        "filament_unloading_speed",
        Kind::Number(Unit::MmPerS),
        "Unloading speed",
        LoadingAndRamming,
        advanced,
        [
            d!(O, "filament_unloading_speed", list),
            d!(P, "filament_unloading_speed", list),
        ]
    ),
    key!(
        Filament,
        "filament_unloading_speed_start",
        Kind::Number(Unit::MmPerS),
        "Unloading speed at the start",
        LoadingAndRamming,
        advanced,
        [
            d!(O, "filament_unloading_speed_start", list),
            d!(P, "filament_unloading_speed_start", list),
        ]
    ),
    key!(
        Filament,
        "filament_toolchange_delay",
        Kind::Number(Unit::Second),
        "Delay on filament change",
        LoadingAndRamming,
        advanced,
        [
            d!(O, "filament_toolchange_delay", list),
            d!(P, "filament_toolchange_delay", list),
        ]
    ),
    key!(
        Filament,
        "filament_cooling_moves",
        Kind::Integer(Unit::None),
        "Cooling moves",
        LoadingAndRamming,
        advanced,
        [
            d!(O, "filament_cooling_moves", list),
            d!(P, "filament_cooling_moves", list),
        ]
    ),
    key!(
        Filament,
        "filament_cooling_initial_speed",
        Kind::Number(Unit::MmPerS),
        "First cooling move speed",
        LoadingAndRamming,
        advanced,
        [
            d!(O, "filament_cooling_initial_speed", list),
            d!(P, "filament_cooling_initial_speed", list),
        ]
    ),
    key!(
        Filament,
        "filament_cooling_final_speed",
        Kind::Number(Unit::MmPerS),
        "Last cooling move speed",
        LoadingAndRamming,
        advanced,
        [
            d!(O, "filament_cooling_final_speed", list),
            d!(P, "filament_cooling_final_speed", list),
        ]
    ),
    key!(
        Filament,
        "filament_stamping_loading_speed",
        Kind::Number(Unit::None),
        "Stamping loading speed",
        LoadingAndRamming,
        advanced,
        [
            d!(O, "filament_stamping_loading_speed", list),
            d!(P, "filament_stamping_loading_speed", list),
        ]
    ),
    key!(
        Filament,
        "filament_stamping_distance",
        Kind::Number(Unit::None),
        "Stamping distance",
        LoadingAndRamming,
        advanced,
        [
            d!(O, "filament_stamping_distance", list),
            d!(P, "filament_stamping_distance", list),
        ]
    ),
    key!(
        Filament,
        "filament_ramming_parameters",
        Kind::Text,
        "Ramming parameters",
        LoadingAndRamming,
        advanced,
        [
            d!(O, "filament_ramming_parameters", list),
            d!(P, "filament_ramming_parameters", list),
        ]
    ),
    key!(
        Filament,
        "filament_ramming_travel_time",
        Kind::Number(Unit::Second),
        "Ramming travel time",
        LoadingAndRamming,
        advanced,
        [d!(B | O | Q, "filament_ramming_travel_time", list),]
    ),
    key!(
        Filament,
        "filament_ramming_travel_time_nc",
        Kind::Number(Unit::Second),
        "Ramming travel time on nozzle change",
        LoadingAndRamming,
        advanced,
        [d!(B | O | Q, "filament_ramming_travel_time_nc", list),]
    ),
    key!(
        Filament,
        "filament_pre_cooling_temperature",
        Kind::Integer(Unit::Celsius),
        "Pre-cooling temperature",
        LoadingAndRamming,
        advanced,
        [d!(B | O | Q, "filament_pre_cooling_temperature", list),]
    ),
    key!(
        Filament,
        "filament_pre_cooling_temperature_nc",
        Kind::Integer(Unit::Celsius),
        "Pre-cooling temperature on nozzle change",
        LoadingAndRamming,
        advanced,
        [d!(B | O | Q, "filament_pre_cooling_temperature_nc", list),]
    ),
    key!(
        Filament,
        "filament_multitool_ramming",
        Kind::Boolean,
        "Ramming on multi-tool printers",
        LoadingAndRamming,
        advanced,
        [
            d!(O, "filament_multitool_ramming", list),
            d!(P, "filament_multitool_ramming", list),
        ]
    ),
    key!(
        Filament,
        "filament_multitool_ramming_volume",
        Kind::Number(Unit::Mm3),
        "Multi-tool ramming volume",
        LoadingAndRamming,
        advanced,
        [
            d!(O, "filament_multitool_ramming_volume", list),
            d!(P, "filament_multitool_ramming_volume", list),
        ]
    ),
    key!(
        Filament,
        "filament_multitool_ramming_flow",
        Kind::Number(Unit::Mm3PerS),
        "Multi-tool ramming flow",
        LoadingAndRamming,
        advanced,
        [
            d!(O, "filament_multitool_ramming_flow", list),
            d!(P, "filament_multitool_ramming_flow", list),
        ]
    ),
    key!(
        Filament,
        "circle_compensation_speed",
        Kind::Number(Unit::MmPerS),
        "Circle compensation speed",
        CircleCompensation,
        advanced,
        [d!(B | Q, "circle_compensation_speed", list),]
    ),
    key!(
        Filament,
        "counter_coef_1",
        Kind::Number(Unit::None),
        "Outer circle coefficient 1",
        CircleCompensation,
        advanced,
        [d!(B | Q, "counter_coef_1", list),]
    ),
    key!(
        Filament,
        "counter_coef_2",
        Kind::Number(Unit::None),
        "Outer circle coefficient 2",
        CircleCompensation,
        advanced,
        [d!(B | Q, "counter_coef_2", list),]
    ),
    key!(
        Filament,
        "counter_coef_3",
        Kind::Number(Unit::Mm),
        "Outer circle coefficient 3",
        CircleCompensation,
        advanced,
        [d!(B | Q, "counter_coef_3", list),]
    ),
    key!(
        Filament,
        "hole_coef_1",
        Kind::Number(Unit::None),
        "Hole coefficient 1",
        CircleCompensation,
        advanced,
        [d!(B | Q, "hole_coef_1", list),]
    ),
    key!(
        Filament,
        "hole_coef_2",
        Kind::Number(Unit::None),
        "Hole coefficient 2",
        CircleCompensation,
        advanced,
        [d!(B | Q, "hole_coef_2", list),]
    ),
    key!(
        Filament,
        "hole_coef_3",
        Kind::Number(Unit::Mm),
        "Hole coefficient 3",
        CircleCompensation,
        advanced,
        [d!(B | Q, "hole_coef_3", list),]
    ),
    key!(
        Filament,
        "counter_limit_min",
        Kind::Number(Unit::Mm),
        "Outer circle compensation minimum",
        CircleCompensation,
        advanced,
        [d!(B | Q, "counter_limit_min", list),]
    ),
    key!(
        Filament,
        "counter_limit_max",
        Kind::Number(Unit::Mm),
        "Outer circle compensation maximum",
        CircleCompensation,
        advanced,
        [d!(B | Q, "counter_limit_max", list),]
    ),
    key!(
        Filament,
        "hole_limit_min",
        Kind::Number(Unit::Mm),
        "Hole compensation minimum",
        CircleCompensation,
        advanced,
        [d!(B | Q, "hole_limit_min", list),]
    ),
    key!(
        Filament,
        "hole_limit_max",
        Kind::Number(Unit::Mm),
        "Hole compensation maximum",
        CircleCompensation,
        advanced,
        [d!(B | Q, "hole_limit_max", list),]
    ),
    key!(
        Filament,
        "diameter_limit",
        Kind::Number(Unit::Mm),
        "Compensated diameter limit",
        CircleCompensation,
        advanced,
        [d!(B | Q, "diameter_limit", list),]
    ),
];
