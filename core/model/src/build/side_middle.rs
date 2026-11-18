// SPDX-License-Identifier: MIT
//! The middle of a profile's side between its slots (mitcad#2002). A side
//! with two or more slots (the long sides of a 30x60) has a frame that no
//! library lists, `side_<side>` (`side_x+`): the side's first slot frame
//! moved across the side to the middle between its outermost slots. A
//! profile placed on it stands centred on the side, so that two matching
//! sections have flush side faces and one centre plane, and its joint
//! names it as it names a slot frame (`F7:mount(side_x+)`). It is worked
//! out from the slot frames where they are, so a part on it stays centred
//! when the profile's section changes.
//!
//! Slot frames are named as the profile layout says (`docs/libraries.md`):
//! `slot_<side>_<n>`, the side `x+`, `x-`, `y+` or `y-`, `n` counting from
//! the side's lower coordinate.

use std::collections::BTreeMap;

use super::geometry::{add, dot, scale, sub, x_axis, z_axis};
use crate::transform::Transform;

/// The sides a profile's slot frames are on.
const SIDES: [&str; 4] = ["x+", "x-", "y+", "y-"];

/// The name of a side's middle: `side_x+`.
pub fn side_middle_name(side: &str) -> String {
    format!("side_{side}")
}

/// The side a side's middle is of (`side_x+`: `x+`); None for another
/// name.
pub fn side_middle_side(name: &str) -> Option<&str> {
    let side = name.strip_prefix("side_")?;
    SIDES.contains(&side).then_some(side)
}

/// The side a slot frame is on (`slot_x+_2`: `x+`); None for another name.
pub fn slot_frame_side(name: &str) -> Option<&str> {
    let (side, number) = name.strip_prefix("slot_")?.rsplit_once('_')?;
    let counted = !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit());
    (counted && SIDES.contains(&side)).then_some(side)
}

/// The side of a slot frame or of a side's middle.
pub fn frame_side(name: &str) -> Option<&str> {
    slot_frame_side(name).or_else(|| side_middle_side(name))
}

/// Whether profiles on two side frames of a part can stand in each
/// other's way across the side: on the same frame, or one on a side's
/// middle and the other on that side's middle or one of its slots (a
/// profile centred on the side covers its slots).
pub fn same_lane(a: &str, b: &str) -> bool {
    a == b
        || ((side_middle_side(a).is_some() || side_middle_side(b).is_some())
            && frame_side(a).is_some()
            && frame_side(a) == frame_side(b))
}

/// The middle of a side among a part's frames: the side's first slot frame
/// with its origin halfway between the first and the last slot frames' (by
/// number). None when `name` names no side's middle, when the side has
/// fewer than two slot frames, or when they do not lie in one plane facing
/// one way with one direction along the profile.
pub fn side_middle_frame(mounts: &BTreeMap<String, Transform>, name: &str) -> Option<Transform> {
    let side = side_middle_side(name)?;
    let mut slots: Vec<(u32, &Transform)> = mounts
        .iter()
        .filter(|(n, _)| slot_frame_side(n) == Some(side))
        .filter_map(|(n, t)| Some((n.rsplit_once('_')?.1.parse::<u32>().ok()?, t)))
        .collect();
    if slots.len() < 2 {
        return None;
    }
    slots.sort_by_key(|(number, _)| *number);
    let first = *slots[0].1;
    let last = *slots[slots.len() - 1].1;
    let (x, z) = (x_axis(&first), z_axis(&first));
    let one_plane = slots.iter().all(|(_, t)| {
        dot(z_axis(t), z) > 1.0 - 1e-6
            && dot(x_axis(t), x) > 1.0 - 1e-6
            && dot(sub(t.translation, first.translation), z).abs() <= 1e-6
    });
    if !one_plane {
        return None;
    }
    let mut middle = first;
    middle.translation = add(
        first.translation,
        scale(sub(last.translation, first.translation), 0.5),
    );
    Some(middle)
}

/// Every side's middle among a part's frames, by name (none whose name
/// the design gives a frame of its own).
pub fn side_middles(mounts: &BTreeMap<String, Transform>) -> BTreeMap<String, Transform> {
    SIDES
        .into_iter()
        .map(side_middle_name)
        .filter(|name| !mounts.contains_key(name))
        .filter_map(|name| Some((name.clone(), side_middle_frame(mounts, &name)?)))
        .collect()
}

/// A part's frame by name: one of `mounts`, else a side's middle.
pub fn frame_or_side_middle(mounts: &BTreeMap<String, Transform>, name: &str) -> Option<Transform> {
    mounts
        .get(name)
        .copied()
        .or_else(|| side_middle_frame(mounts, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(origin: [f64; 3], z: [f64; 3]) -> Transform {
        super::super::geometry::frame(origin, [0.0, 0.0, 1.0], z)
    }

    #[test]
    fn a_side_with_two_slots_has_its_middle_between_them() {
        let mut mounts = BTreeMap::new();
        mounts.insert(
            "slot_x+_1".to_owned(),
            slot([15.0, -15.0, 0.0], [1.0, 0.0, 0.0]),
        );
        mounts.insert(
            "slot_x+_2".to_owned(),
            slot([15.0, 15.0, 0.0], [1.0, 0.0, 0.0]),
        );
        mounts.insert(
            "slot_y+_1".to_owned(),
            slot([0.0, 30.0, 0.0], [0.0, 1.0, 0.0]),
        );
        let middle = side_middle_frame(&mounts, "side_x+").unwrap();
        assert!(
            middle
                .translation
                .iter()
                .zip([15.0, 0.0, 0.0])
                .all(|(a, b)| (a - b).abs() < 1e-9)
        );
        assert!(dot(z_axis(&middle), [1.0, 0.0, 0.0]) > 1.0 - 1e-9);
        assert!(dot(x_axis(&middle), [0.0, 0.0, 1.0]) > 1.0 - 1e-9);
        // A side with one slot has none; the slot frame is the side's.
        assert!(side_middle_frame(&mounts, "side_y+").is_none());
        assert_eq!(side_middles(&mounts).len(), 1);
        assert_eq!(frame_side("slot_x+_2"), Some("x+"));
        assert_eq!(frame_side("side_y-"), Some("y-"));
        assert_eq!(frame_side("end_a"), None);
        assert_eq!(frame_side("side_top"), None);
        assert!(same_lane("side_x+", "slot_x+_1") && same_lane("slot_x+_2", "side_x+"));
        assert!(!same_lane("slot_x+_1", "slot_x+_2") && !same_lane("side_x+", "slot_y+_1"));
    }
}
