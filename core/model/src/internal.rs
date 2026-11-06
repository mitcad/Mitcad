// SPDX-License-Identifier: MIT
//! Items other Mitcad crates use; not part of this crate's API.
//!
//! The JSON api (`mitcad-model-api`), BUILD planning
//! (`mitcad-model-build`), drawings (`mitcad-model-drawing`), the design
//! comparison (`mitcad-model-diff`) and the model's test crates were split
//! out of this crate (mitcad#777). What they need of its insides is
//! re-exported here, one module per module of the model it comes from
//! (mitcad#811), so that the crate root and its documented modules show
//! the model's API alone.

// The model base's own internal items: Base64, fingerprints, SHA-256 and
// the helpers of its modules.
pub use mitcad_model_base::internal::*;

/// BUILD's vector helpers, what BUILD reads of a placed part and how it
/// reads a command's value.
pub mod build {
    pub use crate::build::geometry::*;
    pub use crate::build::part_info::{
        PartInfo, TypedSection, designation, part_number, part_values, typed_section,
    };
    pub use crate::build::{round, value_in};
}

/// The angular tolerance of assembly constraints.
pub mod constraints {
    pub use crate::constraints::ANGULAR;
}

/// The contact pairs of a motion study.
pub mod contact {
    pub use crate::contact::ContactPairs;
}

/// The document's helpers and the types of its methods the api reads.
pub mod document {
    #[cfg(any(test, feature = "testing"))]
    pub use crate::document::ResultVersions;
    pub use crate::document::{
        Carried, DocResolver, FrameAt, MaterialWrite, MoveDestination, MovedDestination,
        ReparentPlan, TimelineBefore, check_minimum_wall, check_overhang_angle, deleted_views,
        feature_level, feature_notices, feature_notices_placed, library_kind, normalized_color,
        noted_message, noted_status, print_material, purge_defaults_json, status_message,
        status_text, with_warnings, writable_library,
    };
}

/// The drawing definitions' sizes and checks, and a date from days.
pub mod drawing {
    pub use crate::drawing::annotate_def::{DIMENSION_TEXT, check_height};
    pub use crate::drawing::civil_from_days;
    pub use crate::drawing::template_def::REVISION_ROW;
}

/// Data exchange: the sketch drawing of the DXF export and USDZ colours.
pub mod exchange {
    pub use crate::exchange::sketch_drawing;
    pub use crate::exchange::usdz::linear;
}

/// Features' helpers: derived parts, regions, found faces, mesh bodies.
pub mod features {
    pub use crate::features::bound_length::measure_to as measure_bound_length;
    pub use crate::features::construction::plane::tangent;
    pub use crate::features::derived_part::{
        Evaluated, configured, end_plane, end_tool, evaluate_source,
    };
    #[cfg(any(test, feature = "testing"))]
    pub use crate::features::extrude::selected_regions;
    pub use crate::features::geom_ref::Resolver;
    pub use crate::features::renamed::{Lost, find_face, with_edge_suggestions};
    pub use crate::features::slot_label;
    pub use crate::features::surface_bodies::is_mesh;
    pub use crate::features::thread::CREST_ON_FACE;
}

/// A design file's state read alone.
pub mod file {
    pub use crate::file::load_state;
}

/// Joint frames, occurrence paths and the joints' state.
pub mod joints {
    pub use crate::joints::{
        ActiveJoint, JointsState, MotionSpec, alignment, apply_override, find_path_from, frame,
        frame_parts, limit_values, origin_frame, path_component, referenced_occurrences,
    };
}

/// Links to other components' geometry.
pub mod links {
    pub use crate::links::{first_path, link_for};
}

/// The active motion links.
pub mod motion_links {
    pub use crate::motion_links::ActiveLink;
}

/// The print analysis' cache.
pub mod print_analysis {
    pub use crate::print_analysis::PrintCache;
}

/// Slicer settings files: colours and per-filament keys.
pub mod print_files {
    pub use crate::print_files::hex_rgb;
    pub use crate::print_files::source::per_filament_keys;
}

/// Printed fits: the parameters they add and their table.
pub mod print_fits {
    pub use crate::print_fits::{evaluation_parameters, json};
}

/// The recompute's cache and results.
pub mod recompute {
    pub use crate::recompute::{
        Cache, CacheStats, CacheUsage, Change, FeatureResult, Flag, Output, Read, Recomputed,
        Versioned,
    };
}
