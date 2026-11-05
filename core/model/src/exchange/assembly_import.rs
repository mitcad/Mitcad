// SPDX-License-Identifier: MIT
//! STEP assemblies as components (mitcad#1274): a file whose assembly
//! places parts comes in as a component per distinct part, holding the
//! part's bodies once in its own coordinates, with an occurrence per
//! instance, and a sub-assembly as a component whose occurrences are
//! placed in it. Repeated parts so share their B-rep in memory, in the
//! design file and in the display meshes, which the kernel's
//! [`Kernel::read_file_structure`] gives once per part. A file without
//! such an assembly, or the choice of one base feature, imports as
//! [`Document::import_file`] does.

use std::io::Read;

use serde::{Deserialize, Serialize};

use super::{
    BaseInput, BodyKind, FileRead, ImportAssembly, ImportBody, ImportPart, Imported, ReadOptions,
    check_unit, file_name, invalid, kernel_error,
};
use crate::document::{Document, ModelError};
use crate::ids::{ComponentUid, OccurrenceUid};
use crate::kernel::{Kernel, KernelError};
use crate::transform::Transform;

/// How [`Document::import_file_structured`] brings a file's assembly in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStructure {
    /// Components and occurrences when the file has an assembly structure,
    /// else one base feature (the default).
    #[default]
    Components,
    /// One base feature: every placed copy of a part a body of its own,
    /// where the file places it.
    Base,
}

/// A component an import made for a part or an assembly of the file.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedComponent {
    pub component: ComponentUid,
    pub name: String,
    /// Its occurrences, in the order they were placed.
    pub occurrences: Vec<OccurrenceUid>,
    /// The base feature holding a part's bodies (None for an assembly).
    pub feature: Option<Imported>,
}

/// The result of [`Document::import_file_structured`].
#[derive(Debug, Clone, PartialEq)]
pub enum ImportedFile {
    /// One base feature, as [`Document::import_file`] makes it.
    Base(Imported),
    /// The file's assembly as components.
    Components {
        components: Vec<ImportedComponent>,
        /// What the import left out (empty parts and bodies, placements
        /// that could not be made).
        warnings: Vec<String>,
    },
}

/// The entity of STEP files that places a part or an assembly in another.
const ASSEMBLY_ENTITY: &[u8] = b"NEXT_ASSEMBLY_USAGE_OCCURRENCE";

/// Whether the file at `path` is a STEP file whose product structure
/// places parts at least twice (two or more assembly usages), read as
/// text without the kernel: what lets an application offer the choice of
/// [`ImportStructure`] before the import. Other files: false.
pub fn file_has_assembly(path: &str) -> Result<bool, ModelError> {
    let step = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("step") || e.eq_ignore_ascii_case("stp"));
    if !step {
        return Ok(false);
    }
    let mut file =
        std::fs::File::open(path).map_err(|e| invalid(format!("cannot read {path}: {e}")))?;
    let keep = ASSEMBLY_ENTITY.len() - 1;
    let mut buffer = vec![0u8; 1 << 20];
    // The end of the block before, so that a name across blocks is found.
    let mut filled = 0;
    let mut found = 0;
    loop {
        let read = file
            .read(&mut buffer[filled..])
            .map_err(|e| invalid(format!("cannot read {path}: {e}")))?;
        if read == 0 {
            return Ok(false);
        }
        let end = filled + read;
        found += buffer[..end]
            .windows(ASSEMBLY_ENTITY.len())
            .filter(|w| *w == ASSEMBLY_ENTITY)
            .count();
        if found >= 2 {
            return Ok(true);
        }
        if end > keep {
            buffer.copy_within(end - keep..end, 0);
            filled = keep;
        } else {
            filled = end;
        }
    }
}

/// What placing the parts keeps track of.
struct Placing<S> {
    parts: Vec<ImportPart<S>>,
    /// Whether a part has bodies, or places a part that has.
    filled: Vec<bool>,
    /// The component made for each part, and the index of its entry in
    /// `made`.
    components: Vec<Option<(ComponentUid, usize)>>,
    made: Vec<ImportedComponent>,
    /// Each entry's part.
    made_parts: Vec<usize>,
    warnings: Vec<String>,
}

/// The deepest nesting of assemblies placed.
const MAX_DEPTH: usize = 64;

/// Whether part `index` has bodies or places one that has (memoised in
/// `filled`, `None` while unknown).
fn filled_part<S>(
    parts: &[ImportPart<S>],
    kept: &[usize],
    filled: &mut [Option<bool>],
    index: usize,
    depth: usize,
) -> bool {
    if let Some(known) = filled[index] {
        return known;
    }
    if depth > MAX_DEPTH {
        return false;
    }
    // A cycle reads as empty while it is followed.
    filled[index] = Some(false);
    let has = kept[index] > 0
        || parts[index]
            .instances
            .iter()
            .any(|i| i.part < parts.len() && filled_part(parts, kept, filled, i.part, depth + 1));
    filled[index] = Some(has);
    has
}

fn part_name<S>(part: &ImportPart<S>, index: usize) -> String {
    match part.name.as_deref().map(str::trim) {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ => format!("part {index}"),
    }
}

impl<K: Kernel> Document<K> {
    /// Imports a STEP, IGES, BRep, STL or OBJ file: with
    /// [`ImportStructure::Components`] a STEP file whose assembly places
    /// parts comes in as components and occurrences (one component per
    /// distinct part with its bodies once, an occurrence per instance,
    /// sub-assemblies as components placed in theirs; placed in the active
    /// component), every other file, and every file with
    /// [`ImportStructure::Base`], as one base feature
    /// ([`Document::import_file`]; `name` names it). One undo step
    /// (`Import <file>`).
    pub fn import_file_structured(
        &mut self,
        path: &str,
        options: &ReadOptions,
        name: Option<&str>,
        structure: ImportStructure,
    ) -> Result<ImportedFile, ModelError> {
        check_unit(options)?;
        let keep = structure == ImportStructure::Components;
        let read = match self.kernel().read_file_structure(path, options, keep) {
            Ok(read) => read,
            Err(KernelError::Unsupported(_)) => FileRead::Bodies(
                self.kernel()
                    .read_file(path, options)
                    .map_err(kernel_error)?,
            ),
            Err(e) => return Err(kernel_error(e)),
        };
        match read {
            FileRead::Bodies(bodies) => self
                .import_read_bodies(path, bodies, name)
                .map(ImportedFile::Base),
            FileRead::Assembly(assembly) => self.import_assembly(path, assembly),
        }
    }

    /// The parts of a file's assembly as components (see
    /// [`Document::import_file_structured`]); nothing stays of a failed
    /// import.
    fn import_assembly(
        &mut self,
        path: &str,
        assembly: ImportAssembly<K::Shape>,
    ) -> Result<ImportedFile, ModelError> {
        let ImportAssembly { mut parts, roots } = assembly;
        let count = parts.len();
        let mut warnings = Vec::new();
        // Empty bodies (a part without geometry) are left out, as in
        // `import_file`.
        let mut kept = Vec::with_capacity(count);
        for (p, part) in parts.iter_mut().enumerate() {
            let named = part_name(part, p);
            let mut bodies = Vec::with_capacity(part.bodies.len());
            for (i, body) in std::mem::take(&mut part.bodies).into_iter().enumerate() {
                let kind = self
                    .kernel()
                    .body_kind(&body.shape)
                    .map_err(|e| invalid(format!("{named}: body {i}: {e}")))?;
                if kind == BodyKind::Empty {
                    warnings.push(format!("{named}: body {i} is empty: left out"));
                } else {
                    bodies.push(body);
                }
            }
            kept.push(bodies.len());
            part.bodies = bodies;
        }
        let mut known = vec![None; count];
        for p in 0..count {
            filled_part(&parts, &kept, &mut known, p, 0);
        }
        let filled: Vec<bool> = known.into_iter().map(|k| k == Some(true)).collect();
        let placed: Vec<_> = roots
            .into_iter()
            .filter(|r| r.part < count && filled[r.part])
            .collect();
        if placed.is_empty() {
            return Err(invalid(format!(
                "{path} has only empty parts ({count}): nothing to import"
            )));
        }
        for (p, part) in parts.iter().enumerate() {
            if kept[p] == 0 && part.instances.is_empty() {
                warnings.push(format!("{} is empty: left out", part_name(part, p)));
            }
        }

        let source = file_name(path);
        let label = format!("Import {source}");
        let depth = self.undo_depth();
        let parent = self.active_component();
        let mut placing = Placing {
            parts,
            filled,
            components: vec![None; count],
            made: Vec::new(),
            made_parts: Vec::new(),
            warnings,
        };
        let result = self.place_parts(&mut placing, &placed, parent, &source, &label);
        if let Err(e) = result {
            while self.undo_depth() > depth {
                if self.undo().is_none() {
                    break;
                }
            }
            return Err(e);
        }
        self.merge_undo(depth, &label);
        Ok(ImportedFile::Components {
            components: placing.made,
            warnings: placing.warnings,
        })
    }

    /// Places the root instances and what they place, then adds the
    /// parts' bodies as one base feature per component.
    fn place_parts(
        &mut self,
        placing: &mut Placing<K::Shape>,
        roots: &[super::ImportInstance],
        parent: ComponentUid,
        source: &str,
        label: &str,
    ) -> Result<(), ModelError> {
        for root in roots {
            self.place_part(placing, root.part, parent, root.transform, 0)?;
        }
        let mut inputs = Vec::new();
        let mut owners = Vec::new();
        for (entry, &part) in placing.made_parts.iter().enumerate() {
            let bodies: Vec<ImportBody<K::Shape>> = std::mem::take(&mut placing.parts[part].bodies);
            if bodies.is_empty() {
                continue;
            }
            inputs.push(BaseInput {
                source: Some(source.to_owned()),
                component: Some(placing.made[entry].component),
                ..BaseInput::new(bodies)
            });
            owners.push(entry);
        }
        if inputs.is_empty() {
            return Ok(());
        }
        let added = self.add_base_features(label, inputs)?;
        for (entry, imported) in owners.into_iter().zip(added) {
            placing.made[entry].feature = Some(imported);
        }
        Ok(())
    }

    /// Places part `index` in `parent`: a new component the first time
    /// (with what it places), an occurrence of that one after.
    fn place_part(
        &mut self,
        placing: &mut Placing<K::Shape>,
        index: usize,
        parent: ComponentUid,
        transform: Transform,
        depth: usize,
    ) -> Result<(), ModelError> {
        let name = part_name(&placing.parts[index], index);
        if depth > MAX_DEPTH {
            placing
                .warnings
                .push(format!("{name}: assemblies nested too deeply: left out"));
            return Ok(());
        }
        if let Some((component, entry)) = placing.components[index] {
            match self.add_occurrence(component, parent, transform) {
                Ok(occurrence) => placing.made[entry].occurrences.push(occurrence),
                // An assembly that contains itself.
                Err(e) => placing.warnings.push(format!("{name}: {e}")),
            }
            return Ok(());
        }
        let file_name = placing.parts[index].name.clone();
        let (component, occurrence) =
            self.add_component(file_name.as_deref(), parent, transform)?;
        let entry = placing.made.len();
        placing.components[index] = Some((component, entry));
        placing.made.push(ImportedComponent {
            component,
            name: self.assembly().name(component),
            occurrences: vec![occurrence],
            feature: None,
        });
        placing.made_parts.push(index);
        let instances = placing.parts[index].instances.clone();
        for instance in instances {
            if instance.part < placing.parts.len() && placing.filled[instance.part] {
                self.place_part(
                    placing,
                    instance.part,
                    component,
                    instance.transform,
                    depth + 1,
                )?;
            }
        }
        Ok(())
    }
}
