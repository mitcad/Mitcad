// SPDX-License-Identifier: MIT
//! SolidCraft's persistent face and edge names (`crates/doc/src/naming.rs`,
//! `lineage.rs`) as Mitcad's topological names.
//!
//! SolidCraft looks a reference up by its name first and by its pick
//! point only when the name finds nothing; the picks are where the face or
//! edge was when it was picked, so a design saved after a parameter change
//! keeps picks that no longer lie on their faces. The import does the
//! same: a name it can translate gives the faces it may mean (the
//! feature's own faces of that role), and of those the one nearest the
//! pick is taken; a name it cannot translate leaves the pick alone.
//!
//! - `F<f>:start`, `:top`, `:bottom` of an extrusion: its caps on the
//!   sketch plane, above it and below it (Mitcad's `start(…)`/`end(…)`,
//!   told apart by their height over the sketch); of a revolve, sweep or
//!   loft `start`/`end`: the caps.
//! - `F<f>:side:<curve>`: the side swept from the sketch curve
//!   (`side(<segment>)` of the curve's Mitcad id).
//! - A box's `+x`, `-x`, `+y`, `-y`, `+z`, `-z`; a cylinder's and a cone's `top`,
//!   `bottom`, `side`.
//! - `F<f>:blend:<edge>`: the rounding or bevel of a fillet or chamfer.
//! - A base flange's `F<b>:top` and `:bottom` (format 62): its panel's
//!   surfaces (`sheet_front(…)`, `sheet_back…`, the pick telling them
//!   apart); `F<b>:side:<line>`: the wall along the sketch line
//!   (`sheet_side<k>(…:sheet_curve(<segment>))`).
//! - Edges `<face>|<face>` (`~k` for several): the edges between faces of
//!   the two names.
//! - A legacy mark (`?`), piece qualifiers (`#2`, `#[…]`) and pattern
//!   copies (`@F6.2`) are left out of the lookup: the pick tells the pieces
//!   and copies apart.

use mitcad_model::{BodyUid, ComponentUid, EdgeName, FaceName, FeatureUid, Kernel};

use super::Importer;
use mitcad_import_base::geom::{distance, dot, sub};

impl<K: Kernel> Importer<'_, '_, K> {
    /// The faces of the bodies (one by name, else the component's) that a
    /// SolidCraft face name may mean; None when the name is not one the
    /// import translates.
    fn named_faces(
        &self,
        component: ComponentUid,
        body: Option<&str>,
        name: &str,
    ) -> Option<Vec<(BodyUid, String)>> {
        // A name the import does not translate at all: the pick decides.
        self.want(name)?;
        let bodies: Vec<BodyUid> = match body.filter(|b| !b.is_empty()) {
            Some(n) => vec![self.body(n).ok()?],
            None => self.component_bodies(component),
        };
        let kernel = self.doc.kernel();
        let mut out = Vec::new();
        for b in bodies {
            let Some(shape) = self.doc.body_shape(b) else {
                continue;
            };
            let Ok(faces) = kernel.faces(shape) else {
                continue;
            };
            for face in faces {
                if let Some(n) = face
                    .names
                    .iter()
                    .find(|n| self.matches(n, name, n, shape, false))
                {
                    out.push((b, n.clone()));
                }
            }
        }
        Some(out)
    }

    /// What a SolidCraft face name (without its marks) asks for.
    fn want(&self, name: &str) -> Option<Want> {
        let name = name.trim_start_matches('?');
        let name = cut_piece(name);
        // A shell's inside of a face (whose name may be a copy's):
        // `F3:inner:F2:top`, or `F3:inner(F2:top)` since format 52.
        if let Some((id, role)) = name.strip_prefix('F').and_then(|r| r.split_once(':'))
            && let Some(inner) = nested(role, "inner")
            && let Ok(id) = id.parse::<u64>()
            && self.design.feature(id).is_some_and(|f| f.kind == "shell")
        {
            let uids = self.made.get(&id)?.features.clone();
            self.want(inner)?;
            return Some(Want::Own(
                uids,
                Role::Wraps("offset(", inner.to_owned()),
                None,
            ));
        }
        // A copy of a pattern or mirror: `<face>@F<p>.<k>`.
        if let Some(at) = name.rfind("@F") {
            let (inner, copy) = (&name[..at], &name[at + 2..]);
            let p: u64 = copy.split('.').next()?.parse().ok()?;
            let uids = self.made.get(&p)?.features.clone();
            self.want(inner)?;
            return Some(Want::Copy(uids, inner.to_owned()));
        }
        let rest = name.strip_prefix('F')?;
        let (id, role) = rest.split_once(':')?;
        let id: u64 = id.parse().ok()?;
        let f = self.design.feature(id)?;
        let uids = self.made.get(&id)?.features.clone();
        let sketch = f
            .def
            .get("sketch")
            .and_then(serde_json::Value::as_u64)
            .and_then(|s| self.sketches.get(&s));
        let role = match (f.kind.as_str(), role) {
            ("extrude" | "surface_extrude", "start") => Role::Cap(0),
            ("extrude" | "surface_extrude", "top") => Role::Cap(1),
            ("extrude" | "surface_extrude", "bottom") => Role::Cap(-1),
            ("revolve" | "sweep" | "loft", "start") => Role::Exact(vec!["start"]),
            ("revolve" | "sweep" | "loft", "end") => Role::Exact(vec!["end"]),
            ("extrude" | "revolve" | "sweep" | "loft", side) if side.starts_with("side:") => {
                let curve = side.strip_prefix("side:")?;
                Role::Side(sketch?.curves.get(curve)?.clone())
            }
            ("box", r) => Role::Exact(vec![match r {
                "+x" => "side1",
                "-x" => "side3",
                "+y" => "side2",
                "-y" => "side0",
                "+z" => "top",
                "-z" => "bottom",
                _ => return None,
            }]),
            // A base flange's panel (format 62: the model's names of a
            // sheet's faces): its top and bottom are MitCad's panel
            // surfaces, the pick telling them apart; its rim along a sketch
            // line the wall along that curve.
            ("sheet_base", "top" | "bottom") => Role::Prefixes(vec!["sheet_front(", "sheet_back"]),
            ("sheet_base", side) if side.starts_with("side:") => {
                let curve = side.strip_prefix("side:")?;
                Role::SheetSide(sketch?.curves.get(curve)?.clone())
            }
            ("cylinder", "top") => Role::Exact(vec!["top"]),
            ("cylinder", "bottom") => Role::Exact(vec!["bottom"]),
            ("cylinder", "side") => Role::Exact(vec!["side0"]),
            // A cone's as a cylinder's (mitcad#2239).
            ("cone", "top") => Role::Exact(vec!["top"]),
            ("cone", "bottom") => Role::Exact(vec!["bottom"]),
            ("cone", "side") => Role::Exact(vec!["side0"]),
            ("fillet", r) if nested(r, "blend").is_some() => Role::Prefix("fillet("),
            ("chamfer", r) if nested(r, "blend").is_some() => Role::Prefix("chamfer("),
            // A shell's inside of a face: Mitcad's `offset(<face>)`.
            ("shell", r) if nested(r, "inner").is_some() => {
                let inner = nested(r, "inner")?;
                self.want(inner)?;
                Role::Wraps("offset(", inner.to_owned())
            }
            _ => return None,
        };
        Some(Want::Own(uids, role, sketch.map(|s| s.frame)))
    }

    /// Whether the Mitcad face name `n` (of the face `face` of `shape`) is
    /// one the SolidCraft name means. A face a later feature made in place
    /// of one (a shell's rim of a removed face, `offset_cap(…)`) counts as
    /// that face.
    fn matches(&self, n: &str, sc: &str, face: &str, shape: &K::Shape, copied: bool) -> bool {
        let Some(want) = self.want(sc) else {
            return false;
        };
        let Some((owner, local)) = n.split_once(':') else {
            return false;
        };
        if let Some(inner) = local
            .strip_prefix("offset_cap(")
            .and_then(|x| x.strip_suffix(')'))
            && self.matches(inner, sc, face, shape, copied)
        {
            return true;
        }
        let local = cut_piece(local);
        let owned = |uids: &[FeatureUid]| uids.iter().any(|u| u.to_string() == owner);
        match want {
            Want::Copy(uids, inner) => {
                owned(&uids)
                    && local
                        .strip_prefix("inst")
                        .and_then(|x| x.split_once('('))
                        .and_then(|(k, rest)| k.parse::<u32>().ok().and(rest.strip_suffix(')')))
                        .is_some_and(|orig| self.matches(orig, &inner, face, shape, true))
            }
            Want::Own(uids, role, frame) => {
                if !owned(&uids) {
                    return false;
                }
                match role {
                    Role::Exact(roles) => roles.contains(&local),
                    Role::Prefix(p) => local.starts_with(p),
                    Role::Prefixes(ps) => ps.iter().any(|p| local.starts_with(p)),
                    Role::SheetSide(curve) => {
                        local.starts_with("sheet_side")
                            && local.split_once(":sheet_curve(").is_some_and(|(_, k)| {
                                k.strip_prefix(curve.as_str())
                                    .is_some_and(|t| t.starts_with('[') || t.starts_with(')'))
                            })
                    }
                    Role::Side(curve) => local.strip_prefix("side(").is_some_and(|k| {
                        k.strip_prefix(curve.as_str())
                            .is_some_and(|t| t.starts_with('[') || t.starts_with(')'))
                    }),
                    Role::Wraps(prefix, inner) => local
                        .strip_prefix(prefix)
                        .and_then(|x| x.strip_suffix(')'))
                        .is_some_and(|orig| self.matches(orig, &inner, face, shape, copied)),
                    Role::Cap(h) => {
                        let cap = local.starts_with("start(") || local.starts_with("end(");
                        // A copy lies elsewhere: any cap of its role.
                        let Some(frame) = frame.filter(|_| !copied) else {
                            return cap;
                        };
                        cap && face
                            .parse::<FaceName>()
                            .ok()
                            .and_then(|fname| self.doc.kernel().face_plane(shape, &fname).ok())
                            .is_some_and(|p| {
                                let height = dot(sub(p.origin, frame.o), frame.normal());
                                let tol = 1e-6 * (1.0 + height.abs()) + 1e-6;
                                match h {
                                    0 => height.abs() <= tol,
                                    1 => height > tol,
                                    _ => height < -tol,
                                }
                            })
                    }
                }
            }
        }
    }

    /// The face a pick and a SolidCraft name mean: of the faces the name
    /// translates to, the one nearest the pick; else the face at the pick.
    pub(crate) fn face_named(
        &self,
        component: ComponentUid,
        body: Option<&str>,
        pick: [f64; 3],
        name: Option<&str>,
    ) -> Result<(BodyUid, String), String> {
        if let Some(candidates) = name.and_then(|n| self.named_faces(component, body, n))
            && !candidates.is_empty()
        {
            let kernel = self.doc.kernel();
            let nearest = candidates
                .into_iter()
                .filter_map(|(b, f)| {
                    let shape = self.doc.body_shape(b)?;
                    let parsed: FaceName = f.parse().ok()?;
                    let near = kernel.face_point_normal(shape, &parsed, pick).ok()?;
                    Some((distance(near.point, pick), b, f))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, b, f)) = nearest {
                return Ok((b, f));
            }
        }
        self.face_at(component, body, pick)
    }

    /// The edge a pick and a SolidCraft edge name (`<face>|<face>`) mean:
    /// of the edges between the faces the two names translate to, the one
    /// nearest the pick; else the edge at the pick.
    pub(crate) fn edge_named(
        &self,
        component: ComponentUid,
        body: Option<&str>,
        pick: [f64; 3],
        name: Option<&str>,
    ) -> Result<(BodyUid, String), String> {
        if let Some(name) = name {
            let name = name.trim_start_matches('?');
            let name = name.split('~').next().unwrap_or(name);
            // The `|` between the faces (a blend's name holds an edge's: in
            // parentheses since format 52, where its `|` is not one of
            // these).
            let splits: Vec<usize> = top_level_bars(name);
            for i in splits {
                let (a, b) = (&name[..i], &name[i + 1..]);
                let (Some(fa), Some(fb)) = (
                    self.named_faces(component, body, a),
                    self.named_faces(component, body, b),
                ) else {
                    continue;
                };
                if fa.is_empty() || fb.is_empty() {
                    continue;
                }
                if let Some(found) = self.edge_between(&fa, &fb, pick) {
                    return Ok(found);
                }
            }
        }
        self.edge_at(component, body, pick)
    }

    fn edge_between(
        &self,
        fa: &[(BodyUid, String)],
        fb: &[(BodyUid, String)],
        pick: [f64; 3],
    ) -> Option<(BodyUid, String)> {
        let kernel = self.doc.kernel();
        let mut best: Option<(f64, BodyUid, String)> = None;
        let mut bodies: Vec<BodyUid> = fa.iter().map(|(b, _)| *b).collect();
        bodies.dedup();
        for b in bodies {
            let shape = self.doc.body_shape(b)?;
            let Ok(edges) = kernel.edges(shape) else {
                continue;
            };
            for e in edges {
                let Some(n) = e.name else { continue };
                let Ok(parsed) = n.parse::<EdgeName>() else {
                    continue;
                };
                let [x, y] = parsed.faces();
                let (x, y) = (x.to_string(), y.to_string());
                let has = |set: &[(BodyUid, String)], f: &str| {
                    set.iter().any(|(bb, ff)| *bb == b && ff == f)
                };
                if !((has(fa, &x) && has(fb, &y)) || (has(fa, &y) && has(fb, &x))) {
                    continue;
                }
                let Ok(near) = kernel.path_point(
                    shape,
                    std::slice::from_ref(&parsed),
                    mitcad_model::datum::PathParameter::Near(pick),
                ) else {
                    continue;
                };
                let d = distance(near.point, pick);
                if best.as_ref().is_none_or(|(bd, ..)| d < *bd) {
                    best = Some((d, b, n));
                }
            }
        }
        best.map(|(_, b, n)| (b, n))
    }
}

/// What a SolidCraft face name asks for.
enum Want {
    /// A face of the feature's own Mitcad features of a role, with the
    /// feature's sketch frame (caps by height).
    Own(Vec<FeatureUid>, Role, Option<super::planes::Frame>),
    /// A pattern's or mirror's copy of a face (`<face>@F<p>.<k>`).
    Copy(Vec<FeatureUid>, String),
}

enum Role {
    /// A cap at a height over the sketch: 0 on it, 1 above, -1 below.
    Cap(i8),
    Side(String),
    Exact(Vec<&'static str>),
    Prefix(&'static str),
    /// Any of these prefixes.
    Prefixes(Vec<&'static str>),
    /// A sheet's wall along the sketch curve (MitCad's
    /// `sheet_side<k>(<sketch>:sheet_curve(<curve>…))`).
    SheetSide(String),
    /// `<prefix><face>)` of the face of a SolidCraft name.
    Wraps(&'static str, String),
}

/// Where an edge name's `|` stand outside parentheses and brackets.
fn top_level_bars(name: &str) -> Vec<usize> {
    let mut depth = 0i32;
    let mut out = Vec::new();
    for (i, c) in name.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '|' if depth == 0 => out.push(i),
            _ => {}
        }
    }
    out
}

/// The name a role of a face name holds: `<role>:<name>` (designs before
/// format 52) or `<role>(<name>)` (format 52, `nested-face-name`), a piece
/// qualifier after the parentheses left out.
fn nested<'a>(role: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = role.strip_prefix(prefix)?;
    if let Some(old) = rest.strip_prefix(':') {
        return Some(old);
    }
    cut_piece(rest).strip_prefix('(')?.strip_suffix(')')
}

/// A name without its piece qualifier (`#2`, `#[…]`) at the end.
fn cut_piece(name: &str) -> &str {
    let mut depth = 0i32;
    for (i, c) in name.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '#' if depth == 0 => return &name[..i],
            _ => {}
        }
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A face name that holds another's reads in both forms: after a colon
    /// (before format 52) and in parentheses (format 52), a character
    /// outside ASCII read whole.
    #[test]
    fn nested_face_names_read_in_both_forms() {
        assert_eq!(nested("inner:F2:top", "inner"), Some("F2:top"));
        assert_eq!(nested("inner(F2:top)", "inner"), Some("F2:top"));
        assert_eq!(nested("inner(F2:top)#2", "inner"), Some("F2:top"));
        assert_eq!(
            nested("blend(F2:side:曲|F2:top)", "blend"),
            Some("F2:side:曲|F2:top")
        );
        assert_eq!(
            nested("blend:F2:side:l2|F2:top", "blend"),
            Some("F2:side:l2|F2:top")
        );
        assert_eq!(nested("side:l2", "blend"), None);
        assert_eq!(nested("inner(F2:top", "inner"), None);
    }

    /// An edge's two faces split at the `|` outside parentheses only: a
    /// blend's name (format 52) holds the edge it rounds.
    #[test]
    fn an_edge_name_splits_outside_the_names_it_holds() {
        let name = "F5:blend(F2:side:l2|F2:top)|F2:side:l3";
        let bars = top_level_bars(name);
        assert_eq!(bars.len(), 1);
        assert_eq!(&name[bars[0] + 1..], "F2:side:l3");
        // The older form splits at every bar.
        assert_eq!(
            top_level_bars("F5:blend:F2:side:l2|F2:top|F2:side:l3").len(),
            2
        );
    }
}
