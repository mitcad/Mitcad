// SPDX-License-Identifier: MIT
//! Sheet templates and revisions of drawings (mitcad#707 phase 4,
//! `mitcad_model_drawing::template`; commands.md, "Drawing templates and
//! revisions"): the template a command chooses, the title block's fields
//! filled in from the design whenever a sheet is drawn, the revision
//! commands, saving a sheet's template to the user's or the project's
//! library and the `drawing_templates` query.
//!
//! A field's names: `title` (the sheet's own title, else the component's
//! property `Title`, which for the root is the design's summary's
//! `Summary/Title` (mitcad#1088), else the component's or the design's
//! name), `name` (that name), `drawing` (the drawing's name),
//! `number` (the sheet's drawing number, else the part number),
//! `part_number`, `description` (the property `Description`, else
//! `Summary/Comments`, the component's, else the design's),
//! `material` (the component's property, else
//! the materials of the bodies the drawing shows), `mass` (the stored
//! property `Mass`, else worked out from the bodies the drawing's
//! component shows, in the design's mass unit),
//! `author` (the sheet's, else the property `Author`, else the design's
//! `Summary/Author`), `date` (the sheet's,
//! else today's), `scale`, `sheet` (`1/2`), `sheet_number`, `sheets`,
//! `size`, `units`, `projection`, `revision` (the latest one's letter, `-`
//! before the first); any other name is a property of the drawing's
//! component (`Key` or `Set/Key`), empty when it has none.

use std::path::Path;

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::prelude::*;
use mitcad_model::ids::ComponentUid;
use mitcad_model::internal::fingerprint::Fingerprint;
use mitcad_model::kernel::Kernel;
use mitcad_model::{Document, SUMMARY_SET};
use mitcad_model_drawing::compose::TitleInfo;
use mitcad_model_drawing::def::{Drawing, DrawingSheet};
use mitcad_model_drawing::template::{
    self, BlockLayout, Filled, Revision, SheetTemplate, next_revision_letter,
};
use mitcad_model_drawing::{Projection, Units, scale_text, today};

/// `add_drawing_revision` and `edit_drawing_revision`: what is given
/// changes (`revision`: the one edited, by its letter).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RevisionCommand {
    drawing: String,
    #[serde(default)]
    revision: Option<String>,
    #[serde(default)]
    letter: Option<String>,
    #[serde(default)]
    description: Option<String>,
    /// YYYY-MM-DD; today's for a new one when left out, and when an edit
    /// clears it.
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    author: Option<String>,
}

/// `save_drawing_template`: a sheet's template saved to the user's or the
/// project's library under a name (its id from the name when left out).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SaveTemplateCommand {
    drawing: String,
    #[serde(default)]
    sheet: usize,
    /// `user` or `project`.
    library: String,
    name: String,
    #[serde(default)]
    id: Option<String>,
    /// The project's folder, when the design is in one.
    #[serde(default)]
    project: Option<String>,
}

/// `edit_drawing_title_block` (mitcad#1625): a sheet's title block as the
/// title block editor leaves it, its lines, fields and images, stored in
/// the sheet's copy of its template.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TitleBlockCommand {
    drawing: String,
    #[serde(default)]
    sheet: usize,
    title_block: BlockLayout,
}

/// The `drawing_templates` query.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TemplatesQuery {
    /// The project's folder, when the design is in one.
    #[serde(default)]
    project: Option<String>,
}

/// A template a command chooses: a built-in one by its id (`iso`, `ansi`),
/// a template's data (as `drawing_templates` lists it: a user's or a
/// project's), or `""` for the simple title block.
pub(super) fn template_choice(value: &Value) -> Result<Option<SheetTemplate>, ApiError> {
    match value {
        Value::Null => Ok(None),
        Value::String(id) if id.trim().is_empty() => Ok(None),
        Value::String(id) => template::built_in()
            .into_iter()
            .find(|t| t.id == id.trim())
            .map(Some)
            .ok_or_else(|| {
                ApiError(format!(
                    "there is no built-in template {id}: iso or ansi (a user's or a \
                     project's is given as its data, as drawing_templates lists it)"
                ))
            }),
        Value::Object(_) => {
            let t: SheetTemplate = serde_json::from_value(value.clone())
                .map_err(|e| ApiError(format!("invalid template: {e}")))?;
            t.check().map_err(ApiError)?;
            Ok(Some(t.for_sheet()))
        }
        _ => Err(ApiError(
            "a template is a built-in one's id or a template's data".to_owned(),
        )),
    }
}

/// A sheet's title block fields as drawn, each `{"label", "text"}`: a
/// template's, or the simple block's.
pub(super) fn title_fields(info: &TitleInfo) -> Vec<Value> {
    match &info.template {
        Some(filled) => filled
            .template
            .title_block
            .fields
            .iter()
            .zip(&filled.values)
            .map(|(f, v)| json!({"label": f.label, "text": v}))
            .collect(),
        None => {
            let (label, number) = match &info.number {
                Some(n) => ("DRAWING NO.", n.clone()),
                None => ("BODY", info.bodies.join(", ")),
            };
            vec![
                json!({"label": "TITLE", "text": info.title}),
                json!({"label": label, "text": number}),
                json!({"label": "DATE", "text": info.date}),
                json!({"label": "DRAWN BY", "text": info.author.clone().unwrap_or_default()}),
            ]
        }
    }
}

/// The template whose title block the title block editor edits on a
/// sheet (mitcad#1625): the sheet's own copy; for the simple title block,
/// which has no layout, a copy of the ISO template (`custom`, "Custom"),
/// whose frame and block the edit starts from.
pub(super) fn editable_template(sheet: &DrawingSheet) -> SheetTemplate {
    match &sheet.template {
        Some(t) => t.clone(),
        None => {
            let mut t = template::built_in()
                .into_iter()
                .find(|t| t.id == "iso")
                .expect("the built-in ISO template");
            t.id = "custom".to_owned();
            t.name = "Custom".to_owned();
            t.library = None;
            t.size = None;
            t.projection = None;
            t.for_sheet()
        }
    }
}

/// A slug of a name for a template's id: its letters and digits in lower
/// case, other runs as `-`.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_owned();
    if out.is_empty() {
        "template".to_owned()
    } else {
        out
    }
}

/// What a sheet's fields are filled in from.
struct TitleContext<'a> {
    drawing: &'a Drawing,
    si: usize,
    title: Option<&'a str>,
    scale: f64,
    units: Units,
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait DrawingTemplatesOps<K: Kernel> {
    /// A sheet's template with its fields filled in from the design, if
    /// the sheet has a template.
    fn filled_template(
        &self,
        drawing: &Drawing,
        si: usize,
        title: Option<&str>,
        scale: f64,
        units: Units,
    ) -> Option<Filled>;

    fn add_drawing_revision(&mut self, c: RevisionCommand) -> Result<Value, ApiError>;

    fn edit_drawing_revision(&mut self, c: RevisionCommand) -> Result<Value, ApiError>;

    fn delete_drawing_revision(&mut self, drawing: &str, revision: &str)
    -> Result<Value, ApiError>;

    /// Writes a sheet's template, under a name of the user's, to the
    /// user's or the project's library; the design does not change.
    fn save_drawing_template(&mut self, c: SaveTemplateCommand) -> Result<Value, ApiError>;

    /// `drawing_templates`: the built-in templates, the user's and the
    /// project's, each with its data and its `choice`; and what could not
    /// be read.
    fn drawing_templates_json(&self, query: &TemplatesQuery) -> Value;

    /// Stores a sheet's title block as edited on the sheet (mitcad#1625):
    /// in its template's copy, or in a copy of the ISO template's frame
    /// for a sheet of the simple title block.
    fn edit_drawing_title_block(&mut self, c: TitleBlockCommand) -> Result<Value, ApiError>;
}

impl<K: Kernel> DrawingTemplatesOps<K> for Document<K> {
    fn filled_template(
        &self,
        drawing: &Drawing,
        si: usize,
        title: Option<&str>,
        scale: f64,
        units: Units,
    ) -> Option<Filled> {
        let sheet = &drawing.sheets[si];
        let own = sheet.template.as_ref()?;
        let context = TitleContext {
            drawing,
            si,
            title,
            scale,
            units,
        };
        // Each name worked out once (a mass measures the bodies).
        let mut known: Vec<(String, String)> = Vec::new();
        let values = own
            .title_block
            .fields
            .iter()
            .map(|field| {
                template::fill(&field.text, &mut |key| {
                    if let Some((_, v)) = known.iter().find(|(k, _)| k == key) {
                        return v.clone();
                    }
                    let v = self.title_value(&context, key);
                    known.push((key.to_owned(), v.clone()));
                    v
                })
            })
            .collect();
        Some(Filled {
            template: own.clone(),
            values,
            revisions: drawing.revisions.clone(),
            projection: sheet.projection,
        })
    }

    fn add_drawing_revision(&mut self, c: RevisionCommand) -> Result<Value, ApiError> {
        let old = self.find_drawing(&c.drawing)?.clone();
        if c.revision.is_some() {
            return Err(ApiError(
                "a new revision has no `revision`: give its `letter`".to_owned(),
            ));
        }
        let letter = match &c.letter {
            Some(l) if !l.trim().is_empty() => l.trim().to_owned(),
            _ => next_revision_letter(&old.revisions),
        };
        if old.revisions.iter().any(|r| r.letter == letter) {
            return Err(ApiError(format!(
                "{} has a revision {letter} already",
                old.name
            )));
        }
        let revision = Revision {
            letter: letter.clone(),
            description: c
                .description
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .to_owned(),
            date: Some(
                c.date
                    .as_deref()
                    .map(str::trim)
                    .filter(|d| !d.is_empty())
                    .map_or_else(today, str::to_owned),
            ),
            author: c
                .author
                .as_deref()
                .map(str::trim)
                .filter(|a| !a.is_empty())
                .map(str::to_owned),
        };
        revision.check().map_err(ApiError)?;
        let mut changed = old.clone();
        changed.revisions.push(revision);
        self.set_drawing(
            old.uid,
            format!("Add revision {letter} to {}", old.name),
            changed,
        )?;
        Ok(json!({"drawing": old.uid, "revision": letter}))
    }

    fn edit_drawing_revision(&mut self, c: RevisionCommand) -> Result<Value, ApiError> {
        let old = self.find_drawing(&c.drawing)?.clone();
        let which = c
            .revision
            .as_deref()
            .ok_or_else(|| ApiError("give the `revision` to edit, by its letter".to_owned()))?;
        let mut changed = old.clone();
        let i = changed
            .revisions
            .iter()
            .position(|r| r.letter == which.trim())
            .ok_or_else(|| ApiError(format!("{} has no revision {which}", old.name)))?;
        let revision = &mut changed.revisions[i];
        let field = |value: &Option<String>| value.as_deref().map(str::trim).map(str::to_owned);
        if let Some(letter) = field(&c.letter) {
            revision.letter = letter;
        }
        if let Some(description) = field(&c.description) {
            revision.description = description;
        }
        // A date cleared is today's, as for a new revision (mitcad#722:
        // the dialog and the guide say so).
        if let Some(date) = field(&c.date) {
            revision.date = Some(if date.is_empty() { today() } else { date });
        }
        if let Some(author) = field(&c.author) {
            revision.author = (!author.is_empty()).then_some(author);
        }
        let letter = revision.letter.clone();
        self.set_drawing(
            old.uid,
            format!("Edit revision {which} of {}", old.name),
            changed,
        )?;
        Ok(json!({"drawing": old.uid, "revision": letter}))
    }

    fn delete_drawing_revision(
        &mut self,
        drawing: &str,
        revision: &str,
    ) -> Result<Value, ApiError> {
        let old = self.find_drawing(drawing)?.clone();
        let mut changed = old.clone();
        let before = changed.revisions.len();
        changed.revisions.retain(|r| r.letter != revision.trim());
        if changed.revisions.len() == before {
            return Err(ApiError(format!("{} has no revision {revision}", old.name)));
        }
        self.set_drawing(
            old.uid,
            format!("Delete revision {} of {}", revision.trim(), old.name),
            changed,
        )?;
        Ok(json!({"drawing": old.uid}))
    }

    fn save_drawing_template(&mut self, c: SaveTemplateCommand) -> Result<Value, ApiError> {
        let drawing = self.find_drawing(&c.drawing)?;
        let sheet = drawing
            .sheets
            .get(c.sheet)
            .ok_or_else(|| ApiError(format!("{} has no sheet {}", drawing.name, c.sheet)))?;
        let mut t = sheet.template.clone().ok_or_else(|| {
            ApiError(format!(
                "{} has the simple title block: give it a template first",
                sheet.name
            ))
        })?;
        if c.name.trim().is_empty() {
            return Err(ApiError("a template needs a name".to_owned()));
        }
        t.name = c.name.trim().to_owned();
        t.id =
            c.id.as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map_or_else(|| slug(&c.name), str::to_owned);
        t.size = Some(sheet.size);
        t.projection = Some(sheet.projection);
        t.library = None;
        let project = c.project.as_deref().map(Path::new);
        let path = template::save(&t, c.library.trim(), project).map_err(ApiError)?;
        t.library = Some(c.library.trim().to_owned());
        Ok(json!({
            "path": path.to_string_lossy(),
            "id": t.id,
            "choice": t.choice(),
        }))
    }

    fn drawing_templates_json(&self, query: &TemplatesQuery) -> Value {
        let (templates, problems) = template::list(query.project.as_deref().map(Path::new));
        let list: Vec<Value> = templates
            .iter()
            .map(|t| {
                let mut value = serde_json::to_value(t).unwrap_or(Value::Null);
                value["choice"] = json!(t.choice());
                value
            })
            .collect();
        json!({"templates": list, "problems": problems})
    }

    fn edit_drawing_title_block(&mut self, c: TitleBlockCommand) -> Result<Value, ApiError> {
        let old = self.find_drawing(&c.drawing)?.clone();
        let mut changed = old.clone();
        let sheet = changed
            .sheets
            .get_mut(c.sheet)
            .ok_or_else(|| ApiError(format!("{} has no sheet {}", old.name, c.sheet)))?;
        let mut t = editable_template(sheet);
        t.title_block = c.title_block;
        t.check().map_err(ApiError)?;
        let (width, height) = sheet.dimensions();
        t.fits(width, height).map_err(ApiError)?;
        let label = format!("Edit title block of {} of {}", sheet.name, old.name);
        sheet.template = Some(t);
        self.set_drawing(old.uid, label, changed)?;
        Ok(json!({"drawing": old.uid, "sheet": c.sheet}))
    }
}

/// The document's methods used only in this module, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
trait DrawingTemplatesLocal<K: Kernel> {
    /// The value of one of a title block's names.
    fn title_value(&self, c: &TitleContext<'_>, key: &str) -> String;

    /// The materials of the bodies a drawing's component shows, each once.
    fn shown_materials(&self, drawing: &Drawing) -> String;

    /// The mass of the bodies a drawing's component shows, each where an
    /// occurrence places it: their volumes (kept by shape version) times
    /// their materials' densities; empty when none can be measured.
    fn shown_mass(&self, drawing: &Drawing) -> String;
}

impl<K: Kernel> DrawingTemplatesLocal<K> for Document<K> {
    fn title_value(&self, c: &TitleContext<'_>, key: &str) -> String {
        let sheet = &c.drawing.sheets[c.si];
        let block = &sheet.title_block;
        let component = c.drawing.component;
        let property = |key: &str| {
            self.component_property_of(component, None, key)
                .filter(|v| !v.trim().is_empty())
        };
        // The design's summary (mitcad#1088): the root's set Summary.
        let summary = |key: &str| {
            self.component_stored_property(ComponentUid::ROOT, &format!("{SUMMARY_SET}/{key}"))
                .filter(|v| !v.trim().is_empty())
        };
        match key {
            // A component's own Title (the root's: the summary's), else
            // its name: the design's summary names the design, not each
            // of its components.
            "title" => block
                .title
                .clone()
                .or_else(|| property("Title"))
                .unwrap_or_else(|| self.drawing_title(c.drawing, c.title)),
            "name" => self.drawing_title(c.drawing, c.title),
            "drawing" => c.drawing.name.clone(),
            "number" => block
                .number
                .clone()
                .or_else(|| property("Part number"))
                .unwrap_or_default(),
            "part_number" => property("Part number").unwrap_or_default(),
            "description" => property("Description")
                .or_else(|| property(&format!("{SUMMARY_SET}/Comments")))
                .or_else(|| summary("Comments"))
                .unwrap_or_default(),
            "author" => block
                .author
                .clone()
                .or_else(|| property("Author"))
                .or_else(|| summary("Author"))
                .unwrap_or_default(),
            "date" => block.date.clone().unwrap_or_else(today),
            "scale" => scale_text(c.scale),
            "sheet" => format!("{}/{}", c.si + 1, c.drawing.sheets.len()),
            "sheet_number" => (c.si + 1).to_string(),
            "sheets" => c.drawing.sheets.len().to_string(),
            "size" => sheet.size.name().replace('_', " "),
            "units" => c.units.symbol.to_owned(),
            "projection" => match sheet.projection {
                Projection::ThirdAngle => "THIRD ANGLE".to_owned(),
                Projection::FirstAngle => "FIRST ANGLE".to_owned(),
            },
            "revision" => c
                .drawing
                .revisions
                .last()
                .map_or_else(|| "-".to_owned(), |r| r.letter.clone()),
            "material" => property("Material").unwrap_or_else(|| self.shown_materials(c.drawing)),
            // A stored `Mass` as a stored `Material` (mitcad#722), else
            // worked out from the bodies shown.
            "mass" => self
                .component_stored_property(component, "Mass")
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| self.shown_mass(c.drawing)),
            other => property(other).unwrap_or_default(),
        }
    }

    fn shown_materials(&self, drawing: &Drawing) -> String {
        // What its views show (mitcad#1293), not what the 3D view shows.
        let shown = self.drawing_shown(drawing);
        let mut names: Vec<String> = Vec::new();
        for s in &shown {
            let name = self
                .body_material_under(s.body, drawing.component, &s.path)
                .0
                .name
                .clone();
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names.join(", ")
    }

    fn shown_mass(&self, drawing: &Drawing) -> String {
        let shown = self.drawing_shown(drawing);
        let mut kg = 0.0;
        let mut measured = 0;
        for s in &shown {
            let mut fp = Fingerprint::new("drawing-body-volume-v1");
            fp.str(&s.body.to_string())
                .u128(self.body_version(s.body).unwrap_or(0));
            let key = fp.finish();
            let volume = match self.drawing_cache().volume(key) {
                Some(volume) => volume,
                None => {
                    let Some(shape) = self.body_shape(s.body) else {
                        continue;
                    };
                    let Ok(mass) = self.kernel().mass_properties(shape) else {
                        continue;
                    };
                    self.drawing_cache().keep_volume(key, mass.volume);
                    mass.volume
                }
            };
            let density = self
                .body_material_under(s.body, drawing.component, &s.path)
                .0
                .density;
            kg += volume * density * 1e-6;
            measured += 1;
        }
        if measured == 0 {
            String::new()
        } else {
            self.mass_text(kg)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn a_name_gives_an_id() {
        assert_eq!(slug("Company A3"), "company-a3");
        assert_eq!(slug("  ISO (A2) / Ours "), "iso-a2-ours");
        assert_eq!(slug("ÄÖ"), "template");
    }
}
