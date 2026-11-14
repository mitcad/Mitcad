// SPDX-License-Identifier: MIT
//! The edges that may have taken the place of an edge that is gone
//! (mitcad#1784): a linked projection's source edge that was merged with,
//! or split from, other edges on the same curve. They lie between the same
//! faces as it did: each face of its name as the shape names it now, its
//! pieces (`#k`) or the whole face of which it was a piece, or the face a
//! sketch change renamed it as (`renamed.rs`, rule 2). Which of them it
//! follows the sketch decides by their curves
//! (`mitcad_sketch::successor`).

use std::str::FromStr;

use crate::kernel::Kernel;
use crate::topo::{EdgeName, FaceName};

/// A face name without its piece numbers: the face whose pieces it names.
fn whole(face: &FaceName) -> FaceName {
    let mut whole = face.clone();
    whole.split.clear();
    whole
}

/// The whole face a face of the lost edge's name stands for on the shape:
/// itself where the shape has it or pieces of it, else the face a sketch
/// change renamed it as; None when neither is there.
fn found_face<K: Kernel>(kernel: &K, shape: &K::Shape, face: &FaceName) -> Option<FaceName> {
    let base = whole(face);
    match super::renamed::find_face(kernel, shape, &base) {
        Ok(None) => Some(base),
        Ok(Some(found)) => Some(whole(&found)),
        Err(_) => None,
    }
}

/// Whether `face` is the face `whole` names or a piece of it.
fn of_face(face: &FaceName, whole: &FaceName) -> bool {
    face.feature == whole.feature && face.role == whole.role && face.key == whole.key
}

/// The edges of the shape between the faces `edge` lay between (each as
/// [`found_face`] finds it), other than `edge` itself, in name order; none
/// when a face is not there or the kernel cannot list edges.
pub(crate) fn successor_edges<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    edge: &EdgeName,
) -> Vec<EdgeName> {
    let [a, b] = edge.faces();
    let (Some(a), Some(b)) = (found_face(kernel, shape, a), found_face(kernel, shape, b)) else {
        return Vec::new();
    };
    let Ok(listed) = kernel.edges(shape) else {
        return Vec::new();
    };
    let mut out: Vec<EdgeName> = Vec::new();
    for info in listed {
        let Some(name) = info.name else {
            continue;
        };
        let Ok(candidate) = EdgeName::from_str(&name) else {
            continue;
        };
        let [p, q] = candidate.faces();
        let between = (of_face(p, &a) && of_face(q, &b)) || (of_face(p, &b) && of_face(q, &a));
        if between && candidate != *edge && !out.contains(&candidate) {
            out.push(candidate);
        }
    }
    out.sort();
    out
}
