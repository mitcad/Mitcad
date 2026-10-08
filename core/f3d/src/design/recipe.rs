// SPDX-License-Identifier: MIT
//! The names by which feature inputs refer to B-rep entities.
//!
//! A face, edge or body input of a feature (`5662F619`, `9716F783`) refers
//! to a recipe object (`7ACC2A03`) that names the entity the way the bodies
//! in the file name theirs: each face and body of the ASM blobs carries
//! `generic_tag_attrib_def` names (see [`crate::names`]), a tag chosen by
//! the operation that made it and the operations that made and changed it.
//! The recipe lists entities by such names. Layout after the root part
//! (both flags 0) *(verified on the corpus and the reference models)*:
//!
//! ```text
//! u32 1 | u32 3 | u32 n
//! n × entity: u32 m, m × name
//!   name: str8 tag | u32 kind | u32 c | c × i32 op | u32 0
//! u32 0 | str8 "<type>_recipe_data" | ... (more data, not read)
//! ```
//!
//! Types and entities *(meaning inferred from the reference models, where
//! the add-in dumps name the same entities)*:
//! - `edge_recipe_data`: the two faces along the edge, then the faces at its
//!   ends;
//! - `face_recipe_data`: the face;
//! - `bounded_face_recipe_data`: the face, then the faces around it;
//! - `body_recipe_data`: the body (tags of type 3 in the blobs);
//! - `vertex_recipe_data`: the faces at the vertex.

use super::ir::EntityName;
use super::stream::{Segment, i32_at, u32_at};

/// Recipe objects: they name entities.
pub const RECIPE: &str = "7ACC2A03-0261-4879-A14A-A93D661A5BDC";

/// A decoded recipe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recipe {
    /// `edge`, `face`, `bounded_face`, `body`, `vertex` (the type without
    /// `_recipe_data`).
    pub kind: String,
    /// The named entities, each by its names.
    pub entities: Vec<Vec<EntityName>>,
}

/// At most this many entities, names per entity and operations per name.
const MAX_ITEMS: u32 = 100_000;

fn str8_at(d: &[u8], p: usize) -> Option<(String, usize)> {
    let n = u32_at(d, p)? as usize;
    let b = d.get(p + 4..p.checked_add(4 + n)?)?;
    Some((b.iter().map(|&c| c as char).collect(), p + 4 + n))
}

fn name_at(d: &[u8], p: usize) -> Option<(EntityName, usize)> {
    let (tag, p) = str8_at(d, p)?;
    let kind = u32_at(d, p)?;
    let c = u32_at(d, p + 4)?;
    if c > MAX_ITEMS {
        return None;
    }
    let mut q = p + 8;
    let mut ops = Vec::with_capacity(c as usize);
    for _ in 0..c {
        ops.push(i64::from(i32_at(d, q)?));
        q += 4;
    }
    if u32_at(d, q)? != 0 {
        return None;
    }
    let name = EntityName {
        tag,
        kind: i64::from(kind),
        ops,
    };
    Some((name, q + 4))
}

/// Decodes a recipe from an object's data (header and root part first).
pub fn parse(seg: &Segment, d: &[u8]) -> Option<Recipe> {
    parse_after(d, seg.root_part(d)?.end)
}

/// Decodes a recipe from object data whose root part ends at `p`.
fn parse_after(d: &[u8], mut p: usize) -> Option<Recipe> {
    if u32_at(d, p)? != 1 || u32_at(d, p + 4)? != 3 {
        return None;
    }
    let n = u32_at(d, p + 8)?;
    if n > MAX_ITEMS {
        return None;
    }
    p += 12;
    let mut entities = Vec::new();
    for _ in 0..n {
        let m = u32_at(d, p)?;
        if m > MAX_ITEMS {
            return None;
        }
        p += 4;
        let mut names = Vec::new();
        for _ in 0..m {
            let (name, q) = name_at(d, p)?;
            names.push(name);
            p = q;
        }
        entities.push(names);
    }
    if u32_at(d, p)? != 0 {
        return None;
    }
    let (kind, _) = str8_at(d, p + 4)?;
    let kind = kind.strip_suffix("_recipe_data")?.to_owned();
    Some(Recipe { kind, entities })
}

/// The recipe of a feature input (a reference object that refers to a
/// recipe object), if it has one that decodes.
pub fn of_input(seg: &Segment, input: u64) -> Option<Recipe> {
    let r = seg
        .ref_ids(seg.data_of(input))
        .into_iter()
        .find(|&r| seg.guid_of(r) == Some(RECIPE))?;
    parse(seg, seg.data_of(r))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(tag: &str, ops: &[i32]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend((tag.len() as u32).to_le_bytes());
        b.extend(tag.as_bytes());
        b.extend(0u32.to_le_bytes());
        b.extend((ops.len() as u32).to_le_bytes());
        for o in ops {
            b.extend(o.to_le_bytes());
        }
        b.extend(0u32.to_le_bytes());
        b
    }

    /// A recipe object's data: header, empty root part, entities, type.
    pub(crate) fn recipe_data(id: u64, kind: &str, entities: &[&[(&str, &[i32])]]) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend(3u32.to_le_bytes());
        d.extend(b"300");
        d.extend(id.to_le_bytes());
        d.extend(0u32.to_le_bytes());
        d.extend([0, 0]);
        d.extend(1u32.to_le_bytes());
        d.extend(3u32.to_le_bytes());
        d.extend((entities.len() as u32).to_le_bytes());
        for e in entities {
            d.extend((e.len() as u32).to_le_bytes());
            for (tag, ops) in e.iter() {
                d.extend(name(tag, ops));
            }
        }
        d.extend(0u32.to_le_bytes());
        let t = format!("{kind}_recipe_data");
        d.extend((t.len() as u32).to_le_bytes());
        d.extend(t.as_bytes());
        d.extend([0xff; 8]);
        d
    }

    #[test]
    fn parses_an_edge_recipe() {
        let d = recipe_data(
            7,
            "edge",
            &[
                &[("3", &[301])],
                &[("4", &[301]), ("2", &[303, -308])],
                &[("1", &[301])],
                &[("2", &[301])],
            ],
        );
        let r = parse_after(&d, 21).unwrap();
        assert_eq!(r.kind, "edge");
        assert_eq!(r.entities.len(), 4);
        assert_eq!(r.entities[1].len(), 2);
        assert_eq!(r.entities[1][1].tag, "2");
        assert_eq!(r.entities[1][1].ops, vec![303, -308]);
        assert_eq!(r.entities[3][0].kind, 0);
    }

    #[test]
    fn refuses_other_layouts() {
        let mut d = recipe_data(7, "face", &[&[("1", &[301])]]);
        // A name whose closing word is not 0.
        let at = d.len() - 8 - 16 - 4 - 4 - 4;
        d[at] = 5;
        assert_eq!(parse_after(&d, 21), None);
        let d = recipe_data(7, "face", &[]);
        assert_eq!(parse_after(&d, 21).unwrap().entities.len(), 0);
        let mut d = recipe_data(7, "face", &[]);
        let n = d.len();
        d[n - 8 - 1] = b'X'; // "..._recipe_datX"
        assert_eq!(parse_after(&d, 21), None);
    }
}
