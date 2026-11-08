// SPDX-License-Identifier: MIT
//! Items other Mitcad crates use; not part of this crate's API.
//!
//! The model (`mitcad-model`), the sketch (`mitcad-sketch`) and the mock
//! kernel (`mitcad-model-testkit`) share these helpers of the base with it
//! (mitcad#777). They are re-exported here, one module per module of the
//! base they come from (mitcad#811), so that the crate root and its
//! documented modules show the base's API alone. `mitcad_model::internal`
//! re-exports this module's contents.

/// The images of an appearance in JSON.
pub mod appearance {
    pub use crate::appearance::each_image_json;
}

/// Base64 of embedded images and files.
pub mod base64 {
    pub use crate::base64::{decode, encode};
}

/// Configuration tables' values applied to the parameters.
pub mod configurations {
    pub use crate::configurations::apply_values;
}

/// Datum tolerances, rotations and the vector helpers datums use.
pub mod datum {
    pub use crate::datum::{
        ANGULAR, LINEAR, add, canonical_axis, default_x_axis, norm, rotate, rotate_onto, scale,
        sub, unit,
    };
}

/// Deterministic result versions.
pub mod fingerprint {
    pub use crate::fingerprint::{Fingerprint, of_bytes};
}

/// Ids' text forms in files and commands.
pub mod ids {
    pub use crate::ids::{curve_serde, parse_number};
    pub use crate::string_serde;
}

/// The kernel operations built of others.
pub mod kernel {
    pub use crate::kernel::simple_extrude;
}

/// Parameters' slots, counts and expressions.
pub mod parameters {
    pub use crate::parameters::{count_error, expression_dims, expression_is_text, one_turn_slot};
}

/// The vector products profiles use.
pub mod profile {
    pub use crate::profile::{cross, dot};
}

/// SHA-256 of design files' content.
pub mod sha256 {
    pub use crate::sha256::{Hasher, Sha256};
}

/// Rotations between directions.
pub mod transform {
    pub use crate::transform::{perpendicular, rotation_between};
}

/// Values in commands resolved with the parameters.
pub mod values {
    pub use crate::values::{resolve_value, resolve_value_as, split_named, value_uses_parameter};
}
