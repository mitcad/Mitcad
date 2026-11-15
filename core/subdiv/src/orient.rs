// SPDX-License-Identifier: MIT
//! Faces of a control net that another program made, turned to run one
//! way (mitcad#1757): what the imports do before a net becomes a Form's
//! cage, whose faces go round counter-clockwise seen from outside
//! (`crate::cage`).

use std::collections::HashMap;

use mitcad_vec3::{Vec3, cross, dot};

/// Turns faces round (each a loop of indices into `points`) where they run
/// against their neighbours, so that every edge between two faces is run
/// along both ways, and turns each closed piece round whose faces point
/// into it (its signed volume negative). An open piece keeps the way its
/// first face runs. Err in words when the faces of a piece cannot all face
/// one way (a one-sided surface). Faces need three indices or more, each
/// into `points`.
pub fn orient(points: &[Vec3], faces: &mut [Vec<usize>]) -> Result<(), String> {
    let mut by_edge: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (f, face) in faces.iter().enumerate() {
        let n = face.len();
        for k in 0..n {
            let (a, b) = (face[k], face[(k + 1) % n]);
            by_edge.entry((a.min(b), a.max(b))).or_default().push(f);
        }
    }
    let runs = |face: &[usize], a: usize, b: usize| {
        let n = face.len();
        (0..n).any(|k| face[k] == a && face[(k + 1) % n] == b)
    };
    let mut flip: Vec<Option<bool>> = vec![None; faces.len()];
    for seed in 0..faces.len() {
        if flip[seed].is_some() {
            continue;
        }
        flip[seed] = Some(false);
        let mut piece = vec![seed];
        let mut stack = vec![seed];
        let mut closed = true;
        while let Some(f) = stack.pop() {
            let flipped = flip[f] == Some(true);
            let face = &faces[f];
            let n = face.len();
            for k in 0..n {
                let (mut a, mut b) = (face[k], face[(k + 1) % n]);
                if flipped {
                    std::mem::swap(&mut a, &mut b);
                }
                let shared = &by_edge[&(a.min(b), a.max(b))];
                if shared.len() < 2 {
                    closed = false;
                }
                for &g in shared {
                    if g == f {
                        continue;
                    }
                    // g must run from b to a: as it is, or turned round.
                    let wanted = !runs(&faces[g], b, a);
                    match flip[g] {
                        None => {
                            flip[g] = Some(wanted);
                            piece.push(g);
                            stack.push(g);
                        }
                        Some(was) if was != wanted => {
                            return Err("the faces of the net cannot all face one way \
                                        (its surface is one-sided)"
                                .to_owned());
                        }
                        Some(_) => {}
                    }
                }
            }
        }
        if closed {
            let mut volume = 0.0;
            for &f in &piece {
                let mut face = faces[f].clone();
                if flip[f] == Some(true) {
                    face.reverse();
                }
                let p0 = points[face[0]];
                for k in 1..face.len().saturating_sub(1) {
                    volume += dot(p0, cross(points[face[k]], points[face[k + 1]]));
                }
            }
            if volume < 0.0 {
                for &f in &piece {
                    flip[f] = flip[f].map(|x| !x);
                }
            }
        }
    }
    for (face, flipped) in faces.iter_mut().zip(flip) {
        if flipped == Some(true) {
            face.reverse();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cube of 2 from the origin, its faces counter-clockwise seen from
    /// outside.
    fn cube() -> (Vec<Vec3>, Vec<Vec<usize>>) {
        let points = vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 2.0, 0.0],
            [0.0, 2.0, 0.0],
            [0.0, 0.0, 2.0],
            [2.0, 0.0, 2.0],
            [2.0, 2.0, 2.0],
            [0.0, 2.0, 2.0],
        ];
        let faces = vec![
            vec![0, 3, 2, 1],
            vec![4, 5, 6, 7],
            vec![0, 1, 5, 4],
            vec![1, 2, 6, 5],
            vec![2, 3, 7, 6],
            vec![3, 0, 4, 7],
        ];
        (points, faces)
    }

    #[test]
    fn faces_are_turned_to_run_one_way_and_out() {
        let (points, good) = cube();
        let mut faces = good.clone();
        orient(&points, &mut faces).unwrap();
        assert_eq!(faces, good);
        // One face against its neighbours: turned round.
        let mut faces = good.clone();
        faces[3].reverse();
        orient(&points, &mut faces).unwrap();
        assert_eq!(faces, good);
        // All pointing in: all turned out.
        let mut faces = good.clone();
        for face in &mut faces {
            face.reverse();
        }
        orient(&points, &mut faces).unwrap();
        assert_eq!(faces, good);
        // An open piece keeps its first face's way.
        let mut open: Vec<Vec<usize>> = good[..5].to_vec();
        open[0].reverse();
        orient(&points, &mut open).unwrap();
        assert_eq!(open[0], {
            let mut f = good[0].clone();
            f.reverse();
            f
        });
        for (k, face) in open.iter().enumerate().skip(1) {
            let mut turned = good[k].clone();
            turned.reverse();
            assert_eq!(*face, turned);
        }
    }

    #[test]
    fn a_one_sided_strip_is_refused() {
        // A strip of three quads, the last joined to the first with a half
        // turn.
        let points: Vec<Vec3> = (0..6).map(|k| [f64::from(k), 0.0, 0.0]).collect();
        let mut faces = vec![vec![0, 1, 4, 3], vec![1, 2, 5, 4], vec![2, 3, 0, 5]];
        let error = orient(&points, &mut faces).unwrap_err();
        assert!(error.contains("one-sided"), "{error}");
    }
}
