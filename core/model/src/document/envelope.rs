// SPDX-License-Identifier: MIT
//! Envelopes in the document (mitcad#1271).
//!
//! An Envelope feature added: its body takes the appearance of its source
//! in the same undo step, the first source body that has one set (the
//! listed bodies in their order, else the bodies of the source's component
//! before those of the components placed in it), so that the envelope
//! shows in one colour of the source. Changing the source's appearance
//! later leaves the envelope's as it is. The body is named after its
//! feature (`Envelope1`, `display_names`), which marks it in the browser.
//!
//! A linked component shown by its envelope ([`Document::set_link_envelope`]):
//! its base feature holds one body, the envelope of the linked design's
//! visible bodies, instead of those bodies, so the design neither draws nor
//! keeps their full geometry; the link records the detail size
//! (`ExternalLink::envelope`), and reading the file again when it changed
//! (`update_links`) makes the envelope again. Shown in full again, the base
//! feature takes the file's bodies back.
//!
//! Each occurrence chooses (mitcad#1276): an occurrence of a component
//! placed more than once that is shown otherwise than its component goes
//! into another component linking the same file (`Machine Envelope`, made
//! for it when there is none shown so), so that the components still hold
//! either the design's bodies or one envelope. The occurrence keeps its
//! uid, its placement and its number where it is free; a component left
//! without occurrences goes when nothing else uses it.
//!
//! Joints to the design's faces, edges and vertices keep working
//! (`joints::envelope_side`): an occurrence in a component of its own
//! resolves them in the component whose bodies they name when that one
//! shows the design in full; a component switched to its envelope keeps the
//! frames they had in full (`ExternalLink::frames`) and they use those.

use std::path::Path;
use std::sync::Arc;

use super::components::{digest, file_label, resolve_path};
use super::mounts::SourceMount;
use super::{DocState, Document, ModelError, invalid};
use crate::assembly::{EnvelopeFrame, ExternalLink};
use crate::exchange::ImportBody;
use crate::features::envelope::{Sources, build, check_detail};
use crate::features::{BaseBody, BaseDef, FeatureDef, ValueInput};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::transform::Transform;

/// A linked component switched by [`Document::set_link_envelope`].
#[derive(Debug, Clone, PartialEq)]
pub struct LinkEnvelope {
    pub component: ComponentUid,
    /// The detail size its envelope was made with; None when it is shown
    /// in full.
    pub detail: Option<f64>,
    /// False when it was already shown so (nothing changed, no undo step).
    pub changed: bool,
}

impl<K: Kernel> Document<K> {
    /// The appearance the body of a new feature `def` in `component`
    /// takes: an Envelope's source's (see the module docs); None for
    /// other features and a source without one.
    pub(super) fn made_appearance(
        &self,
        def: &FeatureDef<ValueInput>,
        component: ComponentUid,
    ) -> Option<String> {
        let FeatureDef::Envelope(envelope) = def else {
            return None;
        };
        let assembly = &self.state.assembly;
        let at = if envelope.occurrence.is_empty() {
            component
        } else {
            crate::joints::path_component(assembly, component, &envelope.occurrence.0).ok()?
        };
        let mut sources: Vec<(bool, BodyUid)> = if envelope.bodies.is_empty() {
            self.bodies_where(|c| c == at || assembly.contains(at, c))
                .iter()
                .map(|b| (self.body_component(b.uid) != Some(at), b.uid))
                .collect()
        } else {
            envelope.bodies.iter().map(|b| (false, *b)).collect()
        };
        // The source's own bodies first, each group in its order.
        sources.sort_by_key(|(nested, _)| *nested);
        sources.iter().find_map(|(_, uid)| {
            self.state
                .body_attributes
                .get(uid)
                .and_then(|a| a.appearance.clone())
        })
    }

    /// The envelope of another design's visible bodies, placed in its
    /// coordinates, as one base-feature body, with the design's mount
    /// frames and the detail size used (None: the default of
    /// [`crate::features::envelope::default_detail`]). The design's
    /// bodies are dropped once the envelope is made.
    pub(super) fn design_envelope(
        &self,
        source: &DocState,
        path: &str,
        detail: Option<f64>,
    ) -> Result<(Vec<BaseBody>, Vec<SourceMount>, f64), ModelError> {
        let (bodies, mounts) = self.design_shapes(source, path)?;
        let sources: Sources<K::Shape> = bodies
            .into_iter()
            .map(|body| (body.shape, Transform::IDENTITY))
            .collect();
        let built = build(&self.kernel, &sources, detail)
            .map_err(|e| invalid(format!("{path}: the envelope: {e}")))?;
        drop(sources);
        let envelope = ImportBody {
            name: Some("Envelope".to_owned()),
            color: None,
            shape: built.shape,
        };
        Ok((self.base_bodies(&[envelope])?, mounts, built.detail))
    }

    /// Shows the occurrence `occurrence` of a linked component by its
    /// envelope at `detail` (mm; None: the detail size the component, or
    /// another component of the same file, is shown at, else 1 % of the
    /// design's diagonal, at least 5 mm) when `show`, else by the linked
    /// design's bodies again (see the module docs), in one undo step. With
    /// `all`, or when it is the component's only occurrence, the component
    /// itself is switched, every occurrence of it: the file is read again,
    /// from its path against `base` (a directory; None: the design file's),
    /// and its base feature takes the envelope or the bodies with the
    /// file's mount frames. Otherwise only this occurrence changes: it goes
    /// into a component of the same file shown so, one made for it when
    /// there is none. A library part, a design linked with options and a
    /// component that is not linked are refused.
    pub fn set_link_envelope(
        &mut self,
        occurrence: OccurrenceUid,
        show: bool,
        detail: Option<f64>,
        base: Option<&Path>,
        all: bool,
    ) -> Result<LinkEnvelope, ModelError> {
        let placed = self.state.require_occurrence(occurrence)?;
        self.state.editable(placed.parent)?;
        let component = placed.component;
        let name = self.state.assembly.occurrence_name(occurrence);
        let link = self.plain_link(component, &name)?;
        match (show, detail) {
            (true, Some(detail)) => check_detail(detail).map_err(invalid)?,
            (false, Some(_)) => {
                return Err(invalid(
                    "a detail size goes with showing the envelope, not with showing the design \
                     in full",
                ));
            }
            _ => {}
        }
        // Shown so already: its own detail size or the component's.
        if (!show && link.envelope.is_none())
            || (show && link.envelope.is_some() && (detail.is_none() || link.envelope == detail))
        {
            return Ok(LinkEnvelope {
                component,
                detail: link.envelope,
                changed: false,
            });
        }
        let siblings = self.link_siblings(component, &link);
        let wanted = if show {
            detail
                .or(link.envelope)
                .or_else(|| siblings.iter().find_map(|(_, e)| *e))
        } else {
            None
        };
        let shown_so = siblings
            .iter()
            .find(|(_, e)| {
                if show {
                    wanted.is_some() && *e == wanted
                } else {
                    e.is_none()
                }
            })
            .map(|(c, _)| *c);
        let alone = self.state.assembly.occurrences_of(component).count() <= 1;
        let plan = match shown_so {
            _ if all => Plan::Switch,
            Some(to) if alone && self.used_only_in(component) => Plan::Move { to, drop: true },
            _ if alone => Plan::Switch,
            Some(to) => Plan::Move { to, drop: false },
            None if show => Plan::Twin,
            None => Plan::Split {
                to: siblings
                    .iter()
                    .find(|(_, e)| *e == link.envelope)
                    .map(|(c, _)| *c),
            },
        };
        let label = if show {
            format!("Show {name} as Envelope")
        } else {
            format!("Show {name} in Full")
        };
        // The joints' frames on the design's geometry while it is shown in
        // full, for the joints to keep (mitcad#1276).
        let frames = match plan {
            Plan::Switch if show && link.envelope.is_none() => {
                crate::joints::envelope_frames(&self.kernel, &self.state, &self.result, component)
            }
            Plan::Switch if show => link.frames.clone(),
            _ => Vec::new(),
        };
        // What the file gives: the component's own bodies (Switch, Split)
        // and those of a component made for the occurrences (Twin, Split).
        let (own, made): (Option<LinkParts>, Option<LinkParts>) = match plan {
            Plan::Move { .. } => (None, None),
            Plan::Switch => {
                let (text, source) = self.read_link(&link, base)?;
                let parts = if show {
                    let (bodies, mounts, used) =
                        self.design_envelope(&source, &link.path, wanted)?;
                    (bodies, mounts, Some(used))
                } else {
                    let (bodies, mounts) = self.design_parts(&source, &link.path)?;
                    (bodies, mounts, None)
                };
                (Some((parts, digest(&text))), None)
            }
            Plan::Twin => {
                let (text, source) = self.read_link(&link, base)?;
                let (bodies, mounts, used) = self.design_envelope(&source, &link.path, wanted)?;
                (None, Some(((bodies, mounts, Some(used)), digest(&text))))
            }
            Plan::Split { to } => {
                let (text, source) = self.read_link(&link, base)?;
                let (bodies, mounts) = self.design_parts(&source, &link.path)?;
                let made = match to {
                    Some(_) => None,
                    None => {
                        let (bodies, mounts, used) =
                            self.design_envelope(&source, &link.path, link.envelope)?;
                        Some(((bodies, mounts, Some(used)), digest(&text)))
                    }
                };
                (Some(((bodies, mounts, None), digest(&text))), made)
            }
        };
        self.apply(|state| {
            if let Some(parts) = own {
                state.switch_link(component, &link, parts, frames)?;
            }
            let twin = match made {
                Some(parts) => Some(state.add_link_twin(component, &link, parts)?),
                None => None,
            };
            let now = match plan {
                Plan::Switch => component,
                Plan::Move { to, drop } => {
                    state.move_occurrence_to(occurrence, to);
                    if drop {
                        state.delete_components(std::collections::BTreeSet::from([component]));
                    }
                    to
                }
                Plan::Twin => {
                    let twin = twin.expect("made");
                    state.move_occurrence_to(occurrence, twin);
                    twin
                }
                Plan::Split { to } => {
                    let to = to.or(twin).expect("a component shown by its envelope");
                    let others: Vec<OccurrenceUid> = state
                        .assembly
                        .occurrences_of(component)
                        .map(|o| o.uid)
                        .filter(|o| *o != occurrence)
                        .collect();
                    for other in others {
                        state.move_occurrence_to(other, to);
                    }
                    // Shown in full, it loses the name of a component
                    // made for envelopes.
                    let current = state.assembly.name(component);
                    if unnumbered(&current).ends_with(&format!(" {TWIN_SUFFIX}")) {
                        let name = state.free_component_name(plain_name(&current));
                        if let Some(c) = state.assembly.component_mut(component) {
                            c.name = name;
                        }
                    }
                    component
                }
            };
            let detail = state
                .assembly
                .component(now)
                .and_then(|c| c.link.as_ref())
                .and_then(|l| l.envelope);
            Ok((
                label,
                LinkEnvelope {
                    component: now,
                    detail,
                    changed: true,
                },
            ))
        })
    }

    /// The link of a component that can be shown by its envelope: linked
    /// as it is, not a library part nor a design linked with options.
    fn plain_link(&self, component: ComponentUid, name: &str) -> Result<ExternalLink, ModelError> {
        let def = self
            .state
            .assembly
            .component(component)
            .ok_or_else(|| invalid(format!("component {component} does not exist")))?;
        if def.library.is_some() || self.state.options_part(component).is_some() {
            return Err(invalid(format!(
                "{name} is a library part or a design linked with options; only a design linked \
                 as it is can be shown by its envelope"
            )));
        }
        def.link.clone().ok_or_else(|| {
            invalid(format!(
                "{name} is not a linked component; only a linked design can be shown by its \
                 envelope"
            ))
        })
    }

    /// The other components that link the same file as it is, each with
    /// the detail size it is shown at (None: in full), in the order they
    /// were made.
    fn link_siblings(
        &self,
        component: ComponentUid,
        link: &ExternalLink,
    ) -> Vec<(ComponentUid, Option<f64>)> {
        self.state
            .assembly
            .components
            .iter()
            .filter(|c| c.uid != component && c.library.is_none())
            .filter(|c| self.state.options_part(c.uid).is_none())
            .filter_map(|c| {
                let other = c.link.as_ref()?;
                (other.path == link.path).then_some((c.uid, other.envelope))
            })
            .collect()
    }

    /// Whether nothing outside the component uses its features (its bodies
    /// named by joints or features of other components), so that it can go
    /// once its last occurrence went into another component of the file.
    fn used_only_in(&self, component: ComponentUid) -> bool {
        let own: std::collections::BTreeSet<FeatureUid> = self
            .state
            .features
            .iter()
            .filter(|f| f.component == component)
            .map(|f| f.uid)
            .collect();
        self.state.dependents(&own).iter().all(|f| {
            self.state
                .entry(*f)
                .is_none_or(|e| e.component == component)
        })
    }

    /// The linked design's text and state, read again from its path
    /// against `base` (a directory; None: the design file's).
    fn read_link(
        &self,
        link: &ExternalLink,
        base: Option<&Path>,
    ) -> Result<(String, DocState), ModelError> {
        let base = base
            .map(Path::to_path_buf)
            .or_else(|| self.source_directory());
        let resolved = std::path::absolute(resolve_path(&link.path, base.as_deref()))
            .map_err(|e| invalid(format!("cannot resolve {}: {e}", link.path)))?;
        let text = std::fs::read_to_string(&resolved)
            .map_err(|e| invalid(format!("cannot read {}: {e}", link.path)))?;
        let (source, _) = crate::file::load_state_at(&text, &resolved)
            .map_err(|e| invalid(format!("{}: {e}", link.path)))?;
        Ok((text, source))
    }
}

/// How [`Document::set_link_envelope`] shows an occurrence.
#[derive(Debug, Clone, Copy)]
enum Plan {
    /// The component itself switched, every occurrence of it.
    Switch,
    /// The occurrence moved into another component of the same file shown
    /// so; `drop`: the component it leaves goes (it was its last one).
    Move { to: ComponentUid, drop: bool },
    /// The occurrence moved into a new component of the same file shown by
    /// its envelope.
    Twin,
    /// The component (shown by its envelope) shown in full, its other
    /// occurrences moved into a component of the file shown at its detail
    /// size: `to`, or a new one.
    Split { to: Option<ComponentUid> },
}

/// What the file gives a component: its bodies, mount frames and detail
/// size (None: in full), with the file's digest.
type LinkParts = ((Vec<BaseBody>, Vec<SourceMount>, Option<f64>), String);

/// The end of the name of a component made for occurrences shown by their
/// envelope (`Machine Envelope`).
const TWIN_SUFFIX: &str = "Envelope";

/// A component's name without what made it unique (` (2)`) and without
/// [`TWIN_SUFFIX`]: `Machine` of `Machine Envelope (1)`.
fn plain_name(name: &str) -> &str {
    let name = unnumbered(name);
    match name.strip_suffix(TWIN_SUFFIX) {
        Some(rest) if rest.ends_with(' ') && !rest.trim().is_empty() => rest.trim_end(),
        _ => name,
    }
}

/// A name without the number that made it unique (` (2)`).
fn unnumbered(name: &str) -> &str {
    let name = name.trim_end();
    match name.rfind(" (") {
        Some(open)
            if name.ends_with(')')
                && open + 3 < name.len()
                && name[open + 2..name.len() - 1]
                    .chars()
                    .all(|c| c.is_ascii_digit()) =>
        {
            &name[..open]
        }
        _ => name,
    }
}

impl DocState {
    /// Gives the first body of the feature `made` the appearance
    /// [`Document::made_appearance`] found, if any.
    pub(super) fn take_appearance(&mut self, made: FeatureUid, appearance: Option<&str>) {
        if let Some(appearance) = appearance {
            self.body_attributes
                .entry(BodyUid::new(made, 0))
                .or_default()
                .appearance = Some(appearance.to_owned());
        }
    }

    /// A linked component's base feature takes the bodies (the envelope or
    /// the design's), its mount frames follow the file (mitcad#525), and
    /// its link takes the digest, the detail size and the joints' frames.
    fn switch_link(
        &mut self,
        component: ComponentUid,
        link: &ExternalLink,
        parts: LinkParts,
        frames: Vec<EnvelopeFrame>,
    ) -> Result<(), ModelError> {
        let ((bodies, mounts, envelope), digest) = parts;
        super::mounts::update_linked_mounts(self, component, &mounts)?;
        let position = self.require(link.feature)?;
        let mut entry = (*self.features[position]).clone();
        if let FeatureDef::Base(base) = &mut entry.def {
            base.bodies = bodies;
        }
        self.features[position] = Arc::new(entry);
        if let Some(l) = self
            .assembly
            .component_mut(component)
            .and_then(|c| c.link.as_mut())
        {
            l.digest = digest;
            l.envelope = envelope;
            l.frames = frames;
        }
        Ok(())
    }

    /// A new component linking the same file as `of` (mitcad#1276), shown
    /// so (`parts`), for occurrences of `of` shown otherwise than `of`
    /// itself: named after it (`Machine Envelope`), with its policy, its
    /// base feature right after that of `of` in the timeline and its mount
    /// frames. Not placed anywhere yet.
    fn add_link_twin(
        &mut self,
        of: ComponentUid,
        link: &ExternalLink,
        parts: LinkParts,
    ) -> Result<ComponentUid, ModelError> {
        let ((bodies, mounts, envelope), digest) = parts;
        let name = self.free_component_name(&format!(
            "{} {TWIN_SUFFIX}",
            plain_name(&self.assembly.name(of))
        ));
        let made = self.assembly.create(Some(&name), None);
        let policy = self.assembly.policy(of);
        self.assembly.set_policy(made, policy);
        let base = BaseDef {
            source: Some(file_label(&link.path)),
            ..BaseDef::new(bodies)
        };
        let added = self.add_feature_in(&FeatureDef::Base(base), None, made)?;
        let from = self.require(added.uid)?;
        let after = self.require(link.feature)? + 1;
        if after < from {
            let entry = self.features.remove(from);
            self.features.insert(after, entry);
        }
        super::mounts::add_linked_mounts(self, made, &mounts)?;
        self.assembly.component_mut(made).expect("made").link = Some(ExternalLink {
            path: link.path.clone(),
            digest,
            feature: added.uid,
            envelope,
            frames: Vec::new(),
        });
        Ok(made)
    }

    /// Places the occurrence's component by `to` instead, keeping its
    /// number unless `to` has an occurrence of that number.
    fn move_occurrence_to(&mut self, occurrence: OccurrenceUid, to: ComponentUid) {
        let Some(number) = self.assembly.occurrence(occurrence).map(|o| o.number) else {
            return;
        };
        let taken: Vec<u32> = self.assembly.occurrences_of(to).map(|o| o.number).collect();
        let number = if taken.contains(&number) {
            taken.iter().max().copied().unwrap_or(0) + 1
        } else {
            number
        };
        if let Some(o) = self.assembly.occurrence_mut(occurrence) {
            o.component = to;
            o.number = number;
        }
    }
}
