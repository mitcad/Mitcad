// SPDX-License-Identifier: MIT
//! Conservative coedge traversal for edge-order diagnostics (mitcad#106).

use std::collections::{HashMap, HashSet};

use super::{NamedState, Walker};

#[derive(Clone, Copy)]
struct Use {
    next: usize,
    previous: usize,
    partner: Option<usize>,
    edge: usize,
    face: usize,
    reversed: bool,
}

struct Topology {
    uses: HashMap<usize, Use>,
    by_edge: HashMap<usize, Vec<usize>>,
}

impl Topology {
    /// Retain coedge occurrences: NamedEdge's unique faces lose seam uses.
    fn read(state: &NamedState, body: usize) -> Result<Self, String> {
        if !state.file.complete || state.file.truncated.is_some() {
            return Err("ASM file is incomplete".to_owned());
        }
        let w = Walker {
            file: state.file,
            view: state.view,
        };
        let edge_index: HashMap<usize, usize> = state
            .edges
            .iter()
            .enumerate()
            .filter(|(_, e)| e.body == body)
            .map(|(i, e)| (e.entity, i))
            .collect();
        let mut topology = Self {
            uses: HashMap::new(),
            by_edge: HashMap::new(),
        };
        let missing = || "incomplete coedge topology".to_owned();
        for (fi, face) in state
            .faces
            .iter()
            .enumerate()
            .filter(|(_, f)| f.body == body)
        {
            let mut c = w.cursor(face.entity).ok_or_else(missing)?;
            let _next = w.ptr(&mut c).ok_or_else(missing)?;
            let mut lp = w.ptr(&mut c).ok_or_else(missing)?;
            let mut loops = HashSet::new();
            while let Some(loop_entity) = lp {
                if loops.len() >= state.file.records.len()
                    || !loops.insert(loop_entity)
                    || w.base_type(loop_entity) != Some("loop")
                {
                    return Err("invalid face loop list".to_owned());
                }
                let mut c = w.cursor(loop_entity).ok_or_else(missing)?;
                lp = w.ptr(&mut c).ok_or_else(missing)?;
                let first = w.ptr(&mut c).ok_or_else(missing)?.ok_or_else(missing)?;
                if w.ptr(&mut c).ok_or_else(missing)? != Some(face.entity) {
                    return Err("loop owner disagrees with face".to_owned());
                }
                let mut at = first;
                let mut ring = HashSet::new();
                loop {
                    if ring.len() >= state.file.records.len()
                        || !ring.insert(at)
                        || w.base_type(at) != Some("coedge")
                    {
                        return Err("unsupported or invalid coedge loop".to_owned());
                    }
                    let mut c = w.cursor(at).ok_or_else(missing)?;
                    let next = w.ptr(&mut c).ok_or_else(missing)?.ok_or_else(missing)?;
                    let previous = w.ptr(&mut c).ok_or_else(missing)?.ok_or_else(missing)?;
                    let partner = w.ptr(&mut c).ok_or_else(missing)?;
                    let edge_entity = w.ptr(&mut c).ok_or_else(missing)?.ok_or_else(missing)?;
                    let edge = *edge_index.get(&edge_entity).ok_or_else(missing)?;
                    let reversed = c.boolean().map_err(|_| missing())?;
                    if w.ptr(&mut c).ok_or_else(missing)? != Some(loop_entity) {
                        return Err("coedge owner disagrees with loop".to_owned());
                    }
                    let use_ = Use {
                        next,
                        previous,
                        partner,
                        edge,
                        face: fi,
                        reversed,
                    };
                    if topology.uses.insert(at, use_).is_some() {
                        return Err("coedge occurs in multiple loops".to_owned());
                    }
                    topology.by_edge.entry(edge).or_default().push(at);
                    at = next;
                    if at == first {
                        break;
                    }
                }
                for at in ring {
                    let use_ = &topology.uses[&at];
                    if topology
                        .uses
                        .get(&use_.next)
                        .is_none_or(|n| n.previous != at)
                        || topology
                            .uses
                            .get(&use_.previous)
                            .is_none_or(|p| p.next != at)
                    {
                        return Err("nonreciprocal coedge loop links".to_owned());
                    }
                }
            }
        }
        Ok(topology)
    }

    fn partner(&self, at: usize) -> Result<usize, String> {
        let use_ = &self.uses[&at];
        let pair = self.by_edge.get(&use_.edge).ok_or("edge has no coedges")?;
        if pair.len() != 2 {
            return Err("boundary or nonmanifold edge in vertex fan".to_owned());
        }
        let other = use_.partner.ok_or("missing coedge partner")?;
        let partner = self
            .uses
            .get(&other)
            .ok_or("coedge partner is outside body")?;
        if other == at
            || partner.partner != Some(at)
            || partner.edge != use_.edge
            || partner.face == use_.face
            || partner.reversed == use_.reversed
        {
            return Err("seam or inconsistent coedge partners".to_owned());
        }
        Ok(other)
    }

    fn cycle(
        &self,
        state: &NamedState,
        first: usize,
        vertex: usize,
        outgoing: bool,
    ) -> Result<Vec<usize>, String> {
        let mut at = first;
        let mut seen = HashSet::new();
        let mut faces = Vec::new();
        loop {
            if seen.len() >= self.uses.len() || !seen.insert(at) {
                return Err("vertex fan does not close on its anchor".to_owned());
            }
            let use_ = &self.uses[&at];
            let edge = &state.edges[use_.edge];
            let endpoint = if outgoing {
                use_.reversed
            } else {
                !use_.reversed
            };
            if edge.vertices[0] == edge.vertices[1]
                || edge.vertices[usize::from(endpoint)] != Some(vertex)
            {
                return Err("coedge loop does not connect at vertex".to_owned());
            }
            if faces.contains(&use_.face) {
                return Err("face occurs more than once in vertex fan".to_owned());
            }
            faces.push(use_.face);
            let partner = self.partner(at)?;
            // At a start, the partner arrives at the vertex: its next
            // coedge leaves it. At an end, its previous coedge arrives.
            // Thus every step crosses one edge and stays at this vertex.
            let across = &self.uses[&partner];
            at = if outgoing {
                across.next
            } else {
                across.previous
            };
            if !self.uses.contains_key(&at) {
                return Err("coedge loop leaves the body".to_owned());
            }
            if at == first {
                break;
            }
        }
        let body = state.edges[self.uses[&first].edge].body;
        let occurrences: HashSet<usize> = self
            .uses
            .iter()
            .filter(|(_, use_)| {
                let endpoint = if outgoing {
                    use_.reversed
                } else {
                    !use_.reversed
                };
                state.edges[use_.edge].vertices[usize::from(endpoint)] == Some(vertex)
            })
            .map(|(&at, _)| at)
            .collect();
        let expected: HashSet<usize> = state
            .edges
            .iter()
            .filter(|e| e.body == body && e.vertices.contains(&Some(vertex)))
            .flat_map(|e| e.faces.iter().copied())
            .collect();
        if seen != occurrences || faces.iter().copied().collect::<HashSet<_>>() != expected {
            return Err("disconnected or incomplete vertex fan".to_owned());
        }
        Ok(faces)
    }
}

pub(super) fn endpoint_face_cycles(
    state: &NamedState,
    edge: usize,
    face: usize,
) -> Result<[Vec<usize>; 2], String> {
    let edge_index = edge;
    let edge = state.edges.get(edge).ok_or("edge index is out of range")?;
    if !edge.faces.contains(&face) || edge.faces.len() != 2 {
        return Err("anchor is not one of two distinct edge faces".to_owned());
    }
    let topology = Topology::read(state, edge.body)?;
    let anchors: Vec<usize> = topology
        .by_edge
        .get(&edge_index)
        .ok_or("edge has no coedges")?
        .iter()
        .copied()
        .filter(|at| topology.uses[at].face == face)
        .collect();
    let [first] = anchors[..] else {
        return Err("edge has multiple coedges on the anchor face".to_owned());
    };
    let reversed = topology.uses[&first].reversed;
    let start = edge.vertices[usize::from(reversed)].ok_or("edge has no start vertex")?;
    let end = edge.vertices[usize::from(!reversed)].ok_or("edge has no end vertex")?;
    if start == end {
        return Err("closed edge endpoint order is unsupported".to_owned());
    }
    Ok([
        topology.cycle(state, first, start, true)?,
        topology.cycle(state, first, end, false)?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asm::{AsmFile, Token};
    use crate::names::NamedEdge;
    use crate::testdata;

    #[test]
    fn cube_cycles_follow_the_anchor_coedge_at_both_ends() {
        let file = AsmFile::parse(&testdata::named_cube_blob()).unwrap();
        let state = NamedState::new(&file, None);
        for (ei, edge) in state.edges.iter().enumerate() {
            for &face in &edge.faces {
                let cycles = state.edge_endpoint_face_cycles(ei, face).unwrap();
                let ends = state.ends(ei, Some(face));
                let other = *edge.faces.iter().find(|&&f| f != face).unwrap();
                for (cycle, end) in cycles.iter().zip(&ends) {
                    assert_eq!(cycle.len(), 3);
                    assert_eq!(&cycle[..2], &[face, other]);
                    assert_eq!(cycle.iter().copied().collect::<HashSet<_>>(), *end);
                }
            }
        }
        assert!(state.edge_endpoint_face_cycles(usize::MAX, 0).is_err());
        assert!(state.edge_endpoint_face_cycles(0, usize::MAX).is_err());
    }

    /// Four oriented corner neighborhoods around one vertex. Other
    /// corners are deliberately absent: this tests the local walk only.
    fn four_face_fan<'a>(file: &'a AsmFile) -> (NamedState<'a>, Topology) {
        let mut state = NamedState::new(file, None);
        state.faces.truncate(4);
        state.edges = (0..4)
            .map(|i| NamedEdge {
                entity: i,
                body: 0,
                faces: vec![i, (i + 1) % 4],
                reversed: vec![false, true],
                vertices: [Some(0), Some(i + 1)],
                names: Vec::new(),
            })
            .collect();
        let mut topology = Topology {
            uses: HashMap::new(),
            by_edge: HashMap::new(),
        };
        for i in 0..4 {
            let outgoing = 2 * i;
            let incoming = outgoing + 1;
            let previous = (i + 3) % 4;
            topology.uses.insert(
                outgoing,
                Use {
                    next: incoming,
                    previous: incoming,
                    partner: Some(2 * ((i + 1) % 4) + 1),
                    edge: i,
                    face: i,
                    reversed: false,
                },
            );
            topology.uses.insert(
                incoming,
                Use {
                    next: outgoing,
                    previous: outgoing,
                    partner: Some(2 * previous),
                    edge: previous,
                    face: i,
                    reversed: true,
                },
            );
            topology
                .by_edge
                .insert(i, vec![outgoing, 2 * ((i + 1) % 4) + 1]);
        }
        (state, topology)
    }

    #[test]
    fn four_face_order_has_a_direction_and_rotates_with_its_anchor() {
        let file = AsmFile::parse(&testdata::named_cube_blob()).unwrap();
        let (state, topology) = four_face_fan(&file);
        assert_eq!(
            topology.cycle(&state, 0, 0, true).unwrap(),
            vec![0, 1, 2, 3]
        );
        assert_eq!(
            topology.cycle(&state, 1, 0, false).unwrap(),
            vec![0, 3, 2, 1]
        );
        assert_eq!(
            topology.cycle(&state, 4, 0, true).unwrap(),
            vec![2, 3, 0, 1]
        );
    }

    #[test]
    fn incomplete_nonmanifold_and_disconnected_fans_are_refused() {
        let file = AsmFile::parse(&testdata::named_cube_blob()).unwrap();
        let (mut state, mut topology) = four_face_fan(&file);
        topology.by_edge.get_mut(&0).unwrap().pop();
        assert!(topology.cycle(&state, 0, 0, true).is_err());
        topology.by_edge.get_mut(&0).unwrap().extend([3, 9]);
        assert!(topology.cycle(&state, 0, 0, true).is_err());
        topology.by_edge.get_mut(&0).unwrap().pop();
        topology.uses.get_mut(&3).unwrap().partner = None;
        assert!(topology.cycle(&state, 0, 0, true).is_err());
        topology.uses.get_mut(&3).unwrap().partner = Some(0);
        topology.uses.get_mut(&3).unwrap().reversed = false;
        assert!(topology.cycle(&state, 0, 0, true).is_err());
        topology.uses.get_mut(&3).unwrap().reversed = true;
        topology.uses.get_mut(&3).unwrap().face = 0;
        assert!(topology.cycle(&state, 0, 0, true).is_err());
        topology.uses.get_mut(&3).unwrap().face = 1;
        let disconnected = topology.uses[&0];
        topology.uses.insert(9, disconnected);
        assert!(topology.cycle(&state, 0, 0, true).is_err());
        topology.uses.remove(&9);
        state.edges[1].vertices[0] = Some(99);
        assert!(topology.cycle(&state, 0, 0, true).is_err());
    }

    #[test]
    fn broken_asm_loop_links_and_history_absence_are_refused() {
        let mut file = AsmFile::parse(&testdata::named_cube_blob()).unwrap();
        // Cube fixture's first coedge is record 16; next is field 3.
        let token = file.records[16].fields.start + 3;
        let original = file.tokens[token].clone();
        file.tokens[token] = Token::Ptr(-1);
        assert!(Topology::read(&NamedState::new(&file, None), 0).is_err());
        file.tokens[token] = original;
        let view = HashMap::from([(16, None)]);
        assert!(Topology::read(&NamedState::new(&file, Some(&view)), 0).is_err());
        file.complete = false;
        assert!(Topology::read(&NamedState::new(&file, None), 0).is_err());
    }

    #[test]
    fn history_remaps_coedge_fields_and_point_loops_are_refused() {
        let mut file = AsmFile::parse(&testdata::named_cube_blob()).unwrap();
        let expected = NamedState::new(&file, None)
            .edge_endpoint_face_cycles(0, 0)
            .unwrap();
        let mut copy = file.records[16].clone();
        let fields = file.tokens[copy.fields.clone()].to_vec();
        let start = file.tokens.len();
        file.tokens.extend(fields);
        copy.fields = start..file.tokens.len();
        copy.in_history = true;
        let copied_record = file.records.len();
        file.records.push(copy);
        let view = HashMap::from([(16, Some(copied_record))]);
        assert_eq!(
            NamedState::new(&file, Some(&view))
                .edge_endpoint_face_cycles(0, 0)
                .unwrap(),
            expected
        );
        // Field 5 is the partner pointer. Only the history copy changes.
        file.tokens[start + 5] = Token::Ptr(-1);
        assert!(
            NamedState::new(&file, Some(&view))
                .edge_endpoint_face_cycles(0, 0)
                .is_err()
        );
        assert_eq!(
            NamedState::new(&file, None)
                .edge_endpoint_face_cycles(0, 0)
                .unwrap(),
            expected
        );
        // A loop's first-use pointer naming a vertex is a point loop.
        let loop_first = file.records[10].fields.start + 4;
        file.tokens[loop_first] = Token::Ptr(52);
        assert!(Topology::read(&NamedState::new(&file, None), 0).is_err());
    }
}
