// SPDX-License-Identifier: MIT
//! The material a history state removed from a replayed body (the
//! history-based hole guess, mitcad#85).
//!
//! The replayed body and the state's body are the same part but for what
//! the feature changed: almost every face of one lies on a face of the
//! other, nearly but not exactly. A boolean of the two whole bodies
//! intersects every such pair of faces, which takes minutes for free-form
//! faces and often gives a wrong result. [`Kernel::removed_material`] cuts
//! only where the bodies differ by more than [`SLACK`].

use mitcad_model::{BooleanOp, Kernel, KernelError};

/// How far apart the replayed body's faces and the state's may lie and
/// still count as the same faces, mm. Replays of free-form faces (fillets
/// and drafts of the `.ipt` import's test parts) lie up to 0.07 mm from
/// the stored ones; the walls of a hole lie about half its depth from the
/// faces of the body before it.
pub(crate) const SLACK: f64 = 0.1;

/// A piece of removed material: its volume, centre and shape.
pub(crate) type Removed<S> = (f64, [f64; 3], S);

/// Whether `MITCAD_NO_REMOVED_REGIONS=1` asks for the whole bodies to be
/// cut, as before mitcad#85 (for comparisons).
fn whole_bodies() -> bool {
    static WHOLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WHOLE.get_or_init(|| std::env::var("MITCAD_NO_REMOVED_REGIONS").is_ok_and(|v| v == "1"))
}

/// The material of `before` that `after` lacks, as pieces with their
/// volumes and centres; the slivers along the faces both have left out.
/// Kernels without [`Kernel::removed_material`] cut the whole bodies.
pub(crate) fn removed_material<K: Kernel>(
    kernel: &K,
    before: &K::Shape,
    after: &K::Shape,
) -> Result<Vec<Removed<K::Shape>>, String> {
    let found = if whole_bodies() {
        Err(KernelError::Unsupported("removed material"))
    } else {
        kernel.removed_material(before, after, SLACK)
    };
    let pieces = match found {
        Ok(pieces) => pieces,
        Err(KernelError::Unsupported(_)) => kernel
            .boolean(BooleanOp::Cut, &[before], after)
            .map_err(|e| e.to_string())?
            .pieces
            .into_iter()
            .map(|p| p.shape)
            .collect(),
        Err(e) => return Err(e.to_string()),
    };
    let pieces: Vec<Removed<K::Shape>> = pieces
        .into_iter()
        .filter_map(|s| {
            // (A kernel call per piece, mitcad#82.)
            crate::tick();
            let m = kernel.mass_properties(&s).ok()?;
            Some((m.volume, m.center, s))
        })
        .collect();
    // Slivers along the faces both share are no holes.
    let largest = pieces.iter().map(|p| p.0).fold(0.0, f64::max);
    Ok(pieces
        .into_iter()
        .filter(|(v, _, _)| *v > 1e-3 * largest && *v > 1e-9)
        .collect())
}
