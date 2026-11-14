// SPDX-License-Identifier: MIT
//! Faces and edges of a sheet-metal body picked in a view, as the semantic
//! references the sheet features take (mitcad#210).
//!
//! The kernel names a sheet body's faces by their material domains:
//! `<F>:sheet_front(<slot>)` and `sheet_back` for a panel,
//! `sheet_bend_front`/`sheet_bend_back` for a bend, and
//! `<F>:sheet_side<slot>(<curve>)` (`sheet_bend_side` on a bend) for the wall
//! along a boundary curve, whose key is `<S>:sheet_curve(<segment>)` for a
//! sketch curve or `<G>:sheet_curve(<slot>)` for a generated one. An edge
//! is named by the two faces that meet there. A pick therefore names its
//! domain and, on a wall, its boundary; the sheet state says which pieces of
//! the curve exist, and the point picked chooses among several.

use serde_json::{Value, json};

use super::folded::develop::chart_coordinates;
use super::*;
use crate::features::sheet_folded::SheetCorner;
use crate::topo::SegmentKey;

/// What one face name of a sheet body stands for.
#[derive(Debug, Clone, PartialEq)]
pub enum PickedFace {
    /// A front or back surface of a domain.
    Surface(SheetDomainRef),
    /// The thickness wall along a boundary curve of a domain.
    Wall(SheetDomainRef, SheetCurveId),
}

impl PickedFace {
    pub fn domain(&self) -> SheetDomainRef {
        match self {
            Self::Surface(d) | Self::Wall(d, _) => *d,
        }
    }
}

/// The curve of a wall's key: `F2:sheet_curve(c2[c1,c3])` or
/// `F4:sheet_curve(13)`.
fn curve_of(key: &str) -> Option<SheetCurveId> {
    let (feature, rest) = key.split_once(":sheet_curve(")?;
    let inner = rest.strip_suffix(')')?;
    let feature = feature.parse().ok()?;
    if let Ok(slot) = inner.parse::<u32>() {
        return Some(SheetCurveId::Generated { feature, slot });
    }
    Some(SheetCurveId::Sketch {
        sketch: feature,
        segment: inner.parse::<SegmentKey>().ok()?,
    })
}

/// A copy's face name without the copies' wrappers: `F6:inst1(n)` is `n`
/// (a mirror's or pattern's copy of a sheet body keeps its domains,
/// mitcad#1450).
fn without_instances(mut name: &str) -> &str {
    loop {
        let Some((feature, rest)) = name.split_once(':') else {
            return name;
        };
        let numbered = |text: &str, prefix: &str| {
            text.strip_prefix(prefix)
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        };
        let Some(open) = rest.find('(') else {
            return name;
        };
        if !numbered(feature, "F") || !numbered(&rest[..open], "inst") {
            return name;
        }
        let Some(inner) = rest[open + 1..].strip_suffix(')') else {
            return name;
        };
        name = inner;
    }
}

/// The domain and wall a face name of a sheet body stands for, or `None`
/// for a name of another kind. Piece numbers (`#k`) are ignored, and so
/// are the wrappers of copies (`F6:inst1(...)`).
pub fn parse_face(name: &str) -> Option<PickedFace> {
    // `#k` suffixes follow the closing parenthesis of the key (inside a
    // copy's wrapper too).
    let pieceless = |name: &'_ str| match name.rfind(')') {
        Some(close) => name[..=close].to_owned(),
        None => name.to_owned(),
    };
    let outer = pieceless(name);
    let name = pieceless(without_instances(&outer));
    let name = name.as_str();
    let (feature, rest) = name.split_once(':')?;
    let feature = feature.parse().ok()?;
    let open = rest.find('(')?;
    let (role, key) = (&rest[..open], rest[open + 1..].strip_suffix(')')?);
    let slot = |text: &str| text.parse::<u32>().ok();
    let panel = |slot| SheetDomainRef::Panel(PanelId { feature, slot });
    let bend = |slot| SheetDomainRef::Bend(BendId { feature, slot });
    match role {
        "sheet_front" | "sheet_back" => Some(PickedFace::Surface(panel(slot(key)?))),
        "sheet_bend_front" | "sheet_bend_back" => Some(PickedFace::Surface(bend(slot(key)?))),
        _ => {
            if let Some(n) = role.strip_prefix("sheet_bend_side") {
                Some(PickedFace::Wall(bend(slot(n)?), curve_of(key)?))
            } else if let Some(n) = role.strip_prefix("sheet_side") {
                Some(PickedFace::Wall(panel(slot(n)?), curve_of(key)?))
            } else {
                None
            }
        }
    }
}

/// The two face names of an edge name `E{A|B}` (with an optional `#k`).
pub fn edge_faces(name: &str) -> Option<(String, String)> {
    let inner = name.strip_prefix("E{")?;
    let close = inner.rfind('}')?;
    let inner = &inner[..close];
    let mut depth = 0i32;
    for (i, c) in inner.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '|' if depth == 0 => return Some((inner[..i].to_owned(), inner[i + 1..].to_owned())),
            _ => {}
        }
    }
    None
}

fn chart(state: &SheetState, domain: SheetDomainRef) -> Result<&SheetPatch, String> {
    state
        .charts
        .iter()
        .find(|p| p.domain == domain)
        .ok_or_else(|| format!("{} is not a domain of sheet {}", label(domain), state.body))
}

fn label(domain: SheetDomainRef) -> String {
    super::folded::develop::domain_label(domain)
}

/// Every edge of a chart with its loop and position in it.
fn edges(patch: &SheetPatch) -> impl Iterator<Item = (&MaterialLoop, usize, &MaterialEdge)> {
    patch
        .regions
        .iter()
        .flat_map(|r| &r.loops)
        .flat_map(|l| l.edges.iter().enumerate().map(move |(i, e)| (l, i, e)))
}

/// The distance of a chart point from a bounded curve (sampled).
fn distance(curve: &BoundedCurve2, uv: [f64; 2]) -> f64 {
    let Ok(support) = curve.support() else {
        return f64::INFINITY;
    };
    (0..=64)
        .map(|i| {
            let t =
                curve.range.first + (curve.range.last - curve.range.first) * f64::from(i) / 64.0;
            let p = support.point(t);
            (p[0] - uv[0]).hypot(p[1] - uv[1])
        })
        .fold(f64::INFINITY, f64::min)
}

/// The chart coordinates of a picked point of the body, if it lies on the
/// domain's chart.
fn chart_uv(patch: &SheetPatch, at: Option<[f64; 3]>, thickness: f64) -> Option<[f64; 2]> {
    chart_coordinates(&patch.embedding, at?, thickness).map(|(uv, _)| uv)
}

fn is_line(edge: &MaterialEdge) -> bool {
    matches!(
        edge.curve.geometry,
        crate::profile::SegmentGeometry::Line { .. }
    )
}

/// The boundary of a picked wall: its panel, curve and, when the curve has
/// several pieces there, the piece nearest the point picked.
fn boundary(
    state: &SheetState,
    thickness: f64,
    domain: SheetDomainRef,
    curve: &SheetCurveId,
    at: Option<[f64; 3]>,
) -> Result<Value, String> {
    let SheetDomainRef::Panel(panel) = domain else {
        return Ok(Value::Null); // bends have no boundary references
    };
    let patch = chart(state, domain)?;
    let pieces: Vec<&MaterialEdge> = edges(patch)
        .map(|(_, _, e)| e)
        .filter(|e| &e.piece.curve == curve)
        .collect();
    if pieces.is_empty() {
        return Err(format!(
            "{} has no boundary along {curve} at the timeline marker",
            label(domain)
        ));
    }
    let chosen = if pieces.len() == 1 {
        Some(pieces[0])
    } else {
        chart_uv(patch, at, thickness).and_then(|uv| {
            pieces
                .iter()
                .min_by(|a, b| distance(&a.curve, uv).total_cmp(&distance(&b.curve, uv)))
                .copied()
        })
    };
    let mut reference = json!({"panel": panel, "curve": curve});
    if pieces.len() > 1
        && let Some(edge) = chosen
    {
        reference["piece"] = json!(edge.piece.piece);
    }
    let mut answer = json!({
        "boundary": reference,
        "pieces": pieces.len(),
        "straight": chosen.is_some_and(is_line),
    });
    // Where a flange's offsets and widths are measured from (mitcad#242):
    // the boundary's ends on the front surface along the canonical axial
    // direction `e` (`e x outward = material normal`), start first.
    if let Some(edge) = chosen.filter(|e| is_line(e))
        && let Ok((front, parity)) = super::folded::plane(patch)
        && let Ok((a, b)) = edge.curve.ends()
    {
        let (first, last) = if parity == MaterialParity::Positive {
            (b, a)
        } else {
            (a, b)
        };
        answer["axial"] = json!({"start": front.point(first), "end": front.point(last)});
    }
    Ok(answer)
}

/// The corner where the walls along two curves of a panel meet: the
/// directed contour's incoming and outgoing curves.
fn corner(
    state: &SheetState,
    thickness: f64,
    panel: PanelId,
    a: &SheetCurveId,
    b: &SheetCurveId,
    at: Option<[f64; 3]>,
) -> Result<Value, String> {
    let domain = SheetDomainRef::Panel(panel);
    let patch = chart(state, domain)?;
    let count = |curve: &SheetCurveId| {
        edges(patch)
            .filter(|(_, _, e)| &e.piece.curve == curve)
            .count()
    };
    let mut found = Vec::new();
    for (contour, i, edge) in edges(patch) {
        let next = &contour.edges[(i + 1) % contour.edges.len()];
        let pair = (&edge.piece.curve, &next.piece.curve);
        if pair == (a, b) || pair == (b, a) {
            found.push((edge, next));
        }
    }
    let uv = chart_uv(patch, at, thickness);
    let (incoming, outgoing) = match (found.len(), uv) {
        (0, _) => {
            return Err(format!(
                "{a} and {b} do not meet at a corner of {}",
                label(domain)
            ));
        }
        (1, _) | (_, None) => found[0],
        (_, Some(uv)) => *found
            .iter()
            .min_by(|x, y| {
                let end =
                    |e: &MaterialEdge| e.curve.point(e.curve.range.last).unwrap_or([f64::NAN; 2]);
                let d = |p: [f64; 2]| (p[0] - uv[0]).hypot(p[1] - uv[1]);
                d(end(x.0)).total_cmp(&d(end(y.0)))
            })
            .expect("several corners"),
    };
    let mut corner = json!({"panel": panel, "incoming": incoming.piece.curve,
                            "outgoing": outgoing.piece.curve});
    if count(&incoming.piece.curve) > 1 {
        corner["incoming_piece"] = json!(incoming.piece.piece);
    }
    if count(&outgoing.piece.curve) > 1 {
        corner["outgoing_piece"] = json!(outgoing.piece.piece);
    }
    Ok(json!({"corner": corner, "straight": is_line(incoming) && is_line(outgoing)}))
}

/// The corner of `panel` where its wall along `curve` meets its tangent
/// line to `bend` (mitcad#1765: the thickness edge between that wall and
/// the bend's side), None when they do not meet there.
fn corner_beside_bend(
    state: &SheetState,
    thickness: f64,
    panel: PanelId,
    bend: SheetDomainRef,
    curve: &SheetCurveId,
    at: Option<[f64; 3]>,
) -> Option<Value> {
    let domain = SheetDomainRef::Panel(panel);
    let patch = chart(state, domain).ok()?;
    let tangent = |edge: &MaterialEdge| {
        state.joins.iter().any(|port| {
            [(&port.first, &port.second), (&port.second, &port.first)]
                .iter()
                .any(|(own, other)| {
                    own.boundary.domain == domain
                        && own.boundary.piece == edge.piece
                        && other.boundary.domain == bend
                })
        })
    };
    let mut line = None;
    for (contour, i, edge) in edges(patch) {
        if &edge.piece.curve != curve {
            continue;
        }
        let n = contour.edges.len();
        for next in [&contour.edges[(i + 1) % n], &contour.edges[(i + n - 1) % n]] {
            if tangent(next) {
                line.get_or_insert_with(|| next.piece.curve.clone());
            }
        }
    }
    corner(state, thickness, panel, curve, &line?, at).ok()
}

/// The corner whose thickness edge runs from `start` to `end` (mitcad#242:
/// an edge named after an ordinary feature, of a body whose domains the
/// sheet-domain adapter re-derived): the panel contour vertex where one
/// end lies on the front surface and the other on the back, over it.
pub fn corner_at(
    state: &SheetState,
    thickness: f64,
    start: [f64; 3],
    end: [f64; 3],
) -> Result<Value, String> {
    let mut found = Vec::new();
    for patch in &state.charts {
        let SheetDomainRef::Panel(panel) = patch.domain else {
            continue;
        };
        let (Some((a, da)), Some((b, db))) = (
            chart_coordinates(&patch.embedding, start, thickness),
            chart_coordinates(&patch.embedding, end, thickness),
        ) else {
            continue;
        };
        let through = (da.abs() <= 1e-6 && (db - thickness).abs() <= 1e-6)
            || (db.abs() <= 1e-6 && (da - thickness).abs() <= 1e-6);
        if !through || (a[0] - b[0]).hypot(a[1] - b[1]) > 1e-6 {
            continue;
        }
        for (contour, i, edge) in edges(patch) {
            let Ok(vertex) = edge.curve.point(edge.curve.range.last) else {
                continue;
            };
            if (vertex[0] - a[0]).hypot(vertex[1] - a[1]) <= 1e-6 {
                let next = &contour.edges[(i + 1) % contour.edges.len()];
                found.push((panel, edge.piece.curve.clone(), next.piece.curve.clone()));
            }
        }
    }
    let [(panel, a, b)] = found.as_slice() else {
        return Err(format!(
            "the edge is the thickness edge of {} panel corners",
            found.len()
        ));
    };
    corner(state, thickness, *panel, a, b, Some(start))
}

/// The panel a picked panel surface stands for at the point picked. The
/// kernel joins the coplanar caps of panels that meet at a seam into one
/// face, named after one of them: a point outside the named panel's
/// material that lies on another panel's surface picks that panel.
fn surface_at(
    state: &SheetState,
    thickness: f64,
    named: SheetDomainRef,
    at: Option<[f64; 3]>,
) -> Result<SheetDomainRef, String> {
    let (SheetDomainRef::Panel(_), Some(at)) = (named, at) else {
        return Ok(named);
    };
    let on_material = |patch: &SheetPatch| {
        chart_coordinates(&patch.embedding, at, thickness).is_some_and(|(uv, depth)| {
            let surface = depth.abs().min((depth - thickness).abs()) <= 1e-3 * thickness.max(1.0);
            surface && patch.regions.iter().any(|r| super::flat::contains(r, uv))
        })
    };
    if on_material(chart(state, named)?) {
        return Ok(named);
    }
    Ok(state
        .charts
        .iter()
        .find(|p| matches!(p.domain, SheetDomainRef::Panel(_)) && on_material(p))
        .map_or(named, |p| p.domain))
}

/// The `sheet_reference` query's answer for a face or an edge of the body
/// (`at`: a point of the pick in the body's coordinates).
pub fn reference(
    state: &SheetState,
    thickness: f64,
    face: Option<&str>,
    edge: Option<&str>,
    at: Option<[f64; 3]>,
) -> Result<Value, String> {
    let not_sheet = |name: &str| format!("{name} is not a face or edge of sheet {}", state.body);
    let mut answer = json!({"body": state.body});
    match (face, edge) {
        (Some(face), None) => {
            let picked = parse_face(face).ok_or_else(|| not_sheet(face))?;
            chart(state, picked.domain())?;
            answer["domain"] = json!(match &picked {
                PickedFace::Surface(domain) => surface_at(state, thickness, *domain, at)?,
                PickedFace::Wall(domain, _) => *domain,
            });
            if let PickedFace::Wall(domain, curve) = &picked
                && let Value::Object(found) = boundary(state, thickness, *domain, curve, at)?
            {
                answer.as_object_mut().expect("an object").extend(found);
            }
        }
        (None, Some(edge)) => {
            let (a, b) = edge_faces(edge).ok_or_else(|| not_sheet(edge))?;
            let (a, b) = (
                parse_face(&a).ok_or_else(|| not_sheet(edge))?,
                parse_face(&b).ok_or_else(|| not_sheet(edge))?,
            );
            match (&a, &b) {
                (PickedFace::Wall(da, ca), PickedFace::Wall(db, cb)) if da == db => {
                    let SheetDomainRef::Panel(panel) = *da else {
                        return Err(format!("{edge} is not a corner of a panel"));
                    };
                    answer["domain"] = json!(da);
                    if let Value::Object(found) = corner(state, thickness, panel, ca, cb, at)? {
                        answer.as_object_mut().expect("an object").extend(found);
                    }
                }
                (PickedFace::Wall(domain, curve), PickedFace::Surface(surface))
                | (PickedFace::Surface(surface), PickedFace::Wall(domain, curve))
                    if surface == domain =>
                {
                    answer["domain"] = json!(domain);
                    if let Value::Object(found) = boundary(state, thickness, *domain, curve, at)? {
                        answer.as_object_mut().expect("an object").extend(found);
                    }
                }
                // The thickness edge where a panel's wall meets the side of
                // a bend (mitcad#1765): the corner of the panel between
                // that wall and its tangent line to the bend, which Corner
                // Round and Corner Chamfer trim in the developed outline.
                (PickedFace::Wall(SheetDomainRef::Panel(panel), curve), other)
                | (other, PickedFace::Wall(SheetDomainRef::Panel(panel), curve))
                    if matches!(other, PickedFace::Wall(SheetDomainRef::Bend(_), _)) =>
                {
                    answer["domain"] = json!(SheetDomainRef::Panel(*panel));
                    if let Some(Value::Object(found)) =
                        corner_beside_bend(state, thickness, *panel, other.domain(), curve, at)
                    {
                        answer.as_object_mut().expect("an object").extend(found);
                    }
                }
                // A seam, or the edge between a panel and a bend: the
                // first panel's domain.
                _ => {
                    let domain = [a.domain(), b.domain()]
                        .into_iter()
                        .find(|d| matches!(d, SheetDomainRef::Panel(_)))
                        .unwrap_or(a.domain());
                    answer["domain"] = json!(domain);
                }
            }
        }
        _ => return Err("give either a face or an edge".into()),
    }
    Ok(answer)
}

/// The face name of the wall along `curve` of `panel`:
/// `F3:sheet_side0(F2:sheet_curve(c2[c1,c3]))`.
fn wall_name(panel: PanelId, curve: &SheetCurveId) -> String {
    let key = match curve {
        SheetCurveId::Sketch { sketch, segment } => format!("{sketch}:sheet_curve({segment})"),
        SheetCurveId::Generated { feature, slot } => format!("{feature}:sheet_curve({slot})"),
    };
    format!("{}:sheet_side{}({key})", panel.feature, panel.slot)
}

/// Whether a corner `reference` found is the stored one: the same panel
/// and curves, and the same pieces where both name them.
fn same_corner(found: &SheetCorner, stored: &SheetCorner) -> bool {
    let piece = |a: Option<u32>, b: Option<u32>| a.is_none() || b.is_none() || a == b;
    found.panel == stored.panel
        && found.incoming == stored.incoming
        && found.outgoing == stored.outgoing
        && piece(found.incoming_piece, stored.incoming_piece)
        && piece(found.outgoing_piece, stored.outgoing_piece)
}

/// The thickness edge a stored corner of Corner Round or Corner Chamfer
/// lies on in the body (the `sheet_corner_edges` query, mitcad#1766):
/// among the body's edges (`listed`, with `at` a point on one), the edge
/// between a wall of the corner's panel along one of its curves and
/// another wall (of that panel, or the side of a bend for a corner beside
/// a bend) that `reference` answers as this corner. A kernel that lists no
/// edge between two walls gives the edge between the walls of the corner's
/// two curves when `reference` answers that as the corner. None when the body
/// has no edge there (a corner its round or bevel has taken away).
pub fn corner_edge(
    state: &SheetState,
    thickness: f64,
    corner: &SheetCorner,
    listed: &[String],
    at: &dyn Fn(&str) -> Option<[f64; 3]>,
) -> Option<String> {
    let panel = SheetDomainRef::Panel(corner.panel);
    let own = |face: &PickedFace| match face {
        PickedFace::Wall(domain, curve) => {
            *domain == panel && (curve == &corner.incoming || curve == &corner.outgoing)
        }
        PickedFace::Surface(_) => false,
    };
    let answers = |name: &str, point: Option<[f64; 3]>| {
        let Ok(answer) = reference(state, thickness, None, Some(name), point) else {
            return false;
        };
        let found: Option<SheetCorner> = serde_json::from_value(answer["corner"].clone()).ok();
        found.is_some_and(|c| same_corner(&c, corner))
    };
    // The listed edges between two walls, with the faces they join.
    let between_walls: Vec<(&str, PickedFace, PickedFace)> = listed
        .iter()
        .filter_map(|name| {
            let (a, b) = edge_faces(name)?;
            let (a, b) = (parse_face(&a)?, parse_face(&b)?);
            let walls = matches!(a, PickedFace::Wall(..)) && matches!(b, PickedFace::Wall(..));
            walls.then_some((name.as_str(), a, b))
        })
        .collect();
    if between_walls.is_empty() {
        let mut walls = [
            wall_name(corner.panel, &corner.incoming),
            wall_name(corner.panel, &corner.outgoing),
        ];
        walls.sort();
        let name = format!("E{{{}|{}}}", walls[0], walls[1]);
        return answers(&name, None).then_some(name);
    }
    between_walls
        .into_iter()
        .find(|(name, a, b)| (own(a) || own(b)) && answers(name, at(name)))
        .map(|(name, _, _)| name.to_owned())
}

/// The `sheet_seam` query: the boundaries of a registered seam between two
/// panels (a coplanar seam that a Bend can fold), stationary first.
pub fn seam(state: &SheetState, stationary: PanelId, moving: PanelId) -> Result<Value, String> {
    let (s, m) = (
        SheetDomainRef::Panel(stationary),
        SheetDomainRef::Panel(moving),
    );
    let found: Vec<(&ShapePortSide, &ShapePortSide)> = state
        .joins
        .iter()
        .filter_map(|port| {
            if port.first.boundary.domain == s && port.second.boundary.domain == m {
                Some((&port.first, &port.second))
            } else if port.first.boundary.domain == m && port.second.boundary.domain == s {
                Some((&port.second, &port.first))
            } else {
                None
            }
        })
        .collect();
    let [(a, b)] = found.as_slice() else {
        return Err(if found.is_empty() {
            format!(
                "{} and {} share no seam in sheet {}",
                label(s),
                label(m),
                state.body
            )
        } else {
            format!("{} and {} share several seams", label(s), label(m))
        });
    };
    let side = |side: &ShapePortSide, panel: PanelId| -> Result<Value, String> {
        let patch = chart(state, side.boundary.domain)?;
        let pieces = edges(patch)
            .filter(|(_, _, e)| e.piece.curve == side.boundary.piece.curve)
            .count();
        let mut boundary = json!({"panel": panel, "curve": side.boundary.piece.curve});
        if pieces > 1 {
            boundary["piece"] = json!(side.boundary.piece.piece);
        }
        Ok(boundary)
    };
    Ok(json!({"stationary": side(a, stationary)?, "moving": side(b, moving)?}))
}

#[cfg(test)]
#[path = "pick_tests.rs"]
mod tests;
