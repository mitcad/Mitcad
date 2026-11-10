// SPDX-License-Identifier: MIT
//! A [`Scene`] as the specs of one layer (prims, attributes and
//! relationships; see the crate documentation for the layout).

use std::collections::BTreeSet;

use crate::sdf::{ListOp, Path, Spec, SpecType, Specifier, Value, Variability};
use crate::{Animation, Error, Material, Mesh, Node, Placement, Scene, TexCoords};

/// A valid prim name: ASCII letters, digits and `_`, not starting with a
/// digit; a run of other characters becomes one `_`, none at the end
/// (`Pin (2)` -> `Pin_2`, `Arm:1` -> `Arm_1`).
pub fn identifier(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    if out.len() > 1 && out.ends_with('_') && !name.ends_with('_') {
        out.pop();
    }
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// `identifier(name)`, or with `_2`, `_3`, ... the first not taken.
fn unique(name: &str, taken: &mut BTreeSet<String>) -> String {
    let base = identifier(name);
    let name = std::iter::once(base.clone())
        .chain((2..).map(|n| format!("{base}_{n}")))
        .find(|n| !taken.contains(n))
        .expect("a free name exists");
    taken.insert(name.clone());
    name
}

/// A prim being built: its spec's fields, its properties' specs and its
/// children's names.
struct Prim {
    path: Path,
    fields: Vec<(String, Value)>,
    properties: Vec<Spec>,
    children: Vec<String>,
    taken: BTreeSet<String>,
}

impl Prim {
    fn new(path: Path, type_name: &str) -> Self {
        let mut fields = vec![("specifier".to_owned(), Value::Specifier(Specifier::Def))];
        if !type_name.is_empty() {
            fields.push(("typeName".to_owned(), Value::Token(type_name.to_owned())));
        }
        Self {
            path,
            fields,
            properties: Vec::new(),
            children: Vec::new(),
            taken: BTreeSet::new(),
        }
    }

    fn metadata(&mut self, name: &str, value: Value) {
        self.fields.push((name.to_owned(), value));
    }

    /// A child prim's unique name.
    fn child(&mut self, name: &str) -> Path {
        let name = unique(name, &mut self.taken);
        self.children.push(name.clone());
        self.path.child(&name)
    }

    fn attribute(
        &mut self,
        name: &str,
        type_name: &str,
        uniform: bool,
        value: Option<Value>,
    ) -> &mut Spec {
        let mut fields = vec![
            ("typeName".to_owned(), Value::Token(type_name.to_owned())),
            ("custom".to_owned(), Value::Bool(false)),
            (
                "variability".to_owned(),
                Value::Variability(if uniform {
                    Variability::Uniform
                } else {
                    Variability::Varying
                }),
            ),
        ];
        if let Some(value) = value {
            fields.push(("default".to_owned(), value));
        }
        self.properties.push(Spec {
            path: self.path.property(name),
            spec_type: SpecType::Attribute,
            fields,
        });
        self.properties.last_mut().expect("just added")
    }

    fn connection(&mut self, name: &str, type_name: &str, source: Path) {
        self.attribute(name, type_name, false, None).fields.push((
            "connectionPaths".to_owned(),
            Value::PathListOp(ListOp::explicit(vec![source])),
        ));
    }

    fn relationship(&mut self, name: &str, target: Path) {
        self.properties.push(Spec {
            path: self.path.property(name),
            spec_type: SpecType::Relationship,
            fields: vec![
                ("custom".to_owned(), Value::Bool(false)),
                (
                    "variability".to_owned(),
                    Value::Variability(Variability::Uniform),
                ),
                (
                    "targetPaths".to_owned(),
                    Value::PathListOp(ListOp::explicit(vec![target])),
                ),
            ],
        });
    }

    /// The prim's spec, then its properties'; its children come after
    /// (`out` holds the prim's place for the spec).
    fn finish(self, out: &mut Vec<Spec>, at: usize) {
        let mut fields = self.fields;
        if !self.children.is_empty() {
            fields.push(("primChildren".to_owned(), Value::TokenVector(self.children)));
        }
        if !self.properties.is_empty() {
            let names = self
                .properties
                .iter()
                .map(|p| p.path.property.clone().expect("a property path"))
                .collect();
            fields.push(("properties".to_owned(), Value::TokenVector(names)));
        }
        out[at] = Spec {
            path: self.path,
            spec_type: SpecType::Prim,
            fields,
        };
        out.splice(at + 1..at + 1, self.properties);
    }
}

/// Reserves a spec's place in `out` for a prim finished later.
fn reserve(out: &mut Vec<Spec>) -> usize {
    out.push(Spec {
        path: Path::root(),
        spec_type: SpecType::Prim,
        fields: Vec::new(),
    });
    out.len() - 1
}

fn f32s(v: [f64; 3]) -> [f32; 3] {
    v.map(|c| c as f32)
}

/// The layer's specs: the pseudo-root, then each prim followed by its
/// properties, depth first.
pub fn specs(scene: &Scene) -> Result<Vec<Spec>, Error> {
    check(scene)?;
    let root_name = identifier(&scene.root.name);
    let mut out = vec![Spec {
        path: Path::root(),
        spec_type: SpecType::PseudoRoot,
        fields: vec![
            ("defaultPrim".to_owned(), Value::Token(root_name.clone())),
            ("metersPerUnit".to_owned(), Value::Double(0.001)),
            ("upAxis".to_owned(), Value::Token("Y".to_owned())),
            (
                "documentation".to_owned(),
                Value::String(scene.documentation.clone()),
            ),
            (
                "primChildren".to_owned(),
                Value::TokenVector(vec![root_name.clone()]),
            ),
        ],
    }];
    // The time of an animation (mitcad#1597): one time code per frame.
    if let Some(animation) = &scene.animation {
        let first = *animation.times.first().expect("checked");
        let last = *animation.times.last().expect("checked");
        out[0].fields.extend([
            ("startTimeCode".to_owned(), Value::Double(first)),
            ("endTimeCode".to_owned(), Value::Double(last)),
            (
                "timeCodesPerSecond".to_owned(),
                Value::Double(animation.fps),
            ),
            ("framesPerSecond".to_owned(), Value::Double(animation.fps)),
        ]);
    }
    let root_at = reserve(&mut out);
    let mut root = Prim::new(Path::root().child(&root_name), "Xform");
    root.metadata("kind", Value::Token("component".to_owned()));
    // Z up to Y up: -90 degrees about X takes Z to Y and Y to -Z.
    root.attribute("xformOp:rotateX", "float", false, Some(Value::Float(-90.0)));
    root.attribute(
        "xformOpOrder",
        "token[]",
        true,
        Some(Value::TokenArray(vec!["xformOp:rotateX".to_owned()])),
    );
    // The materials first, so that meshes know their paths.
    let scope_path = root.child("Materials");
    let scope_at = reserve(&mut out);
    let mut scope = Prim::new(scope_path, "Scope");
    let mut materials = Vec::with_capacity(scene.materials.len());
    for material in &scene.materials {
        let path = scope.child(&material.name);
        materials.push(path.clone());
        material_specs(scene, material, path, &mut out);
    }
    scope.finish(&mut out, scope_at);
    let times = scene.animation.as_ref().map_or(&[][..], |a| &a.times[..]);
    node_specs(&scene.root, &mut root, &materials, times, &mut out);
    root.finish(&mut out, root_at);
    Ok(out)
}

/// An `xformOp:transform` of a placement and, when it moves, its samples
/// at the times; then the `xformOpOrder` naming it.
fn transform_op(prim: &mut Prim, placement: &Placement, samples: &[Placement], times: &[f64]) {
    let op = prim.attribute(
        "xformOp:transform",
        "matrix4d",
        false,
        Some(Value::Matrix4d(placement.to_usd())),
    );
    if !samples.is_empty() {
        op.fields.push((
            "timeSamples".to_owned(),
            Value::TimeSamples(
                times
                    .iter()
                    .zip(samples)
                    .map(|(t, p)| (*t, Value::Matrix4d(p.to_usd())))
                    .collect(),
            ),
        ));
    }
    prim.attribute(
        "xformOpOrder",
        "token[]",
        true,
        Some(Value::TokenArray(vec!["xformOp:transform".to_owned()])),
    );
}

/// Why an animation cannot be written, if it cannot (mitcad#1597).
fn check_animation(animation: &Animation) -> Result<(), Error> {
    if !(animation.fps.is_finite() && animation.fps > 0.0) {
        return Err(Error(format!(
            "the frame rate must be a positive number of frames per second, not {}",
            animation.fps
        )));
    }
    if animation.times.is_empty() {
        return Err(Error(
            "an animation needs at least one time code".to_owned(),
        ));
    }
    if animation.times.iter().any(|t| !t.is_finite()) {
        return Err(Error("the time codes must be finite numbers".to_owned()));
    }
    if animation.times.windows(2).any(|w| w[1] <= w[0]) {
        return Err(Error(
            "the time codes must increase from each sample to the next".to_owned(),
        ));
    }
    Ok(())
}

/// Why a node's or a mesh's samples cannot be written, if they cannot.
fn check_samples(
    name: &str,
    samples: &[Placement],
    animation: Option<&Animation>,
) -> Result<(), Error> {
    if samples.is_empty() {
        return Ok(());
    }
    let Some(animation) = animation else {
        return Err(Error(format!(
            "{name} has samples, but the scene has no animation"
        )));
    };
    if samples.len() != animation.times.len() {
        return Err(Error(format!(
            "{name} has {} samples for {} time codes",
            samples.len(),
            animation.times.len()
        )));
    }
    if let Some(i) = samples.iter().position(|p| !p.is_finite()) {
        return Err(Error(format!(
            "{name}: the placement at time code {} is not finite",
            animation.times[i]
        )));
    }
    Ok(())
}

fn check(scene: &Scene) -> Result<(), Error> {
    let fail = |m: String| Err(Error(m));
    fn meshes(node: &Node) -> Box<dyn Iterator<Item = &Mesh> + '_> {
        Box::new(
            node.meshes
                .iter()
                .chain(node.children.iter().flat_map(meshes)),
        )
    }
    fn nodes(node: &Node) -> Box<dyn Iterator<Item = &Node> + '_> {
        Box::new(std::iter::once(node).chain(node.children.iter().flat_map(nodes)))
    }
    if let Some(animation) = &scene.animation {
        check_animation(animation)?;
    }
    for node in nodes(&scene.root) {
        check_samples(&node.name, &node.samples, scene.animation.as_ref())?;
    }
    if !scene.root.samples.is_empty() {
        return fail(format!(
            "{}: the root does not move; move its meshes or child nodes",
            scene.root.name
        ));
    }
    for mesh in meshes(&scene.root) {
        check_samples(&mesh.name, &mesh.samples, scene.animation.as_ref())?;
        let n = mesh.points.len();
        if mesh.triangles.is_empty() {
            return fail(format!("{} has no triangles", mesh.name));
        }
        if mesh.normals.len() != n {
            return fail(format!(
                "{} has {} normals for {n} points",
                mesh.name,
                mesh.normals.len()
            ));
        }
        if n > i32::MAX as usize || mesh.triangles.len() > (i32::MAX / 3) as usize {
            return fail(format!("{} is too large", mesh.name));
        }
        if let Some(t) = mesh
            .triangles
            .iter()
            .find(|t| t.iter().any(|&i| i as usize >= n))
        {
            return fail(format!(
                "{}: triangle {t:?} beyond its {n} points",
                mesh.name
            ));
        }
        let material = |m: usize| {
            (m >= scene.materials.len())
                .then(|| Error(format!("{}: material {m} does not exist", mesh.name)))
        };
        if let Some(e) = material(mesh.material) {
            return Err(e);
        }
        for subset in &mesh.subsets {
            if let Some(e) = material(subset.material) {
                return Err(e);
            }
            if subset
                .triangles
                .iter()
                .any(|&t| t as usize >= mesh.triangles.len())
            {
                return fail(format!(
                    "{}: subset {} beyond its triangles",
                    mesh.name, subset.name
                ));
            }
        }
        match &mesh.tex_coords {
            Some(TexCoords::Vertex(st)) if st.len() != n => {
                return fail(format!(
                    "{}: {} texture coordinates for {n} points",
                    mesh.name,
                    st.len()
                ));
            }
            Some(TexCoords::FaceVarying(st)) if st.len() != mesh.triangles.len() * 3 => {
                return fail(format!(
                    "{}: {} texture coordinates for {} corners",
                    mesh.name,
                    st.len(),
                    mesh.triangles.len() * 3
                ));
            }
            _ => {}
        }
    }
    for material in &scene.materials {
        if material.texture.is_some_and(|t| t >= scene.images.len()) {
            return fail(format!("{}: its image does not exist", material.name));
        }
    }
    Ok(())
}

/// A node's meshes and child nodes as prims under `prim`; `times` are the
/// animation's time codes (none for a still scene).
fn node_specs(
    node: &Node,
    prim: &mut Prim,
    materials: &[Path],
    times: &[f64],
    out: &mut Vec<Spec>,
) {
    for mesh in &node.meshes {
        let path = prim.child(&mesh.name);
        mesh_specs(mesh, path, materials, times, out);
    }
    for child in &node.children {
        let path = prim.child(&child.name);
        let at = reserve(out);
        let mut xform = Prim::new(path, "Xform");
        transform_op(&mut xform, &child.placement, &child.samples, times);
        node_specs(child, &mut xform, materials, times, out);
        xform.finish(out, at);
    }
}

fn mesh_specs(mesh: &Mesh, path: Path, materials: &[Path], times: &[f64], out: &mut Vec<Spec>) {
    let at = reserve(out);
    let mut prim = Prim::new(path, "Mesh");
    prim.metadata(
        "apiSchemas",
        Value::TokenListOp(ListOp::prepended(vec!["MaterialBindingAPI".to_owned()])),
    );
    // A mesh moved in its node (mitcad#1597): where it is by default, its
    // samples along time.
    if !mesh.samples.is_empty() {
        transform_op(&mut prim, &Placement::IDENTITY, &mesh.samples, times);
    }
    let points: Vec<[f32; 3]> = mesh.points.iter().map(|p| f32s(*p)).collect();
    let mut low = [f32::INFINITY; 3];
    let mut high = [f32::NEG_INFINITY; 3];
    for p in &points {
        for k in 0..3 {
            low[k] = low[k].min(p[k]);
            high[k] = high[k].max(p[k]);
        }
    }
    if mesh.double_sided {
        prim.attribute("doubleSided", "bool", true, Some(Value::Bool(true)));
    }
    prim.attribute(
        "extent",
        "float3[]",
        false,
        Some(Value::Vec3fArray(vec![low, high])),
    );
    prim.attribute(
        "faceVertexCounts",
        "int[]",
        false,
        Some(Value::IntArray(vec![3; mesh.triangles.len()])),
    );
    prim.attribute(
        "faceVertexIndices",
        "int[]",
        false,
        Some(Value::IntArray(
            mesh.triangles.iter().flatten().map(|&i| i as i32).collect(),
        )),
    );
    prim.attribute(
        "normals",
        "normal3f[]",
        false,
        Some(Value::Vec3fArray(
            mesh.normals.iter().map(|n| f32s(*n)).collect(),
        )),
    )
    .fields
    .push((
        "interpolation".to_owned(),
        Value::Token("vertex".to_owned()),
    ));
    prim.attribute(
        "points",
        "point3f[]",
        false,
        Some(Value::Vec3fArray(points)),
    );
    if let Some(st) = &mesh.tex_coords {
        let (values, interpolation) = match st {
            TexCoords::Vertex(st) => (st, "vertex"),
            TexCoords::FaceVarying(st) => (st, "faceVarying"),
        };
        prim.attribute(
            "primvars:st",
            "texCoord2f[]",
            false,
            Some(Value::Vec2fArray(values.clone())),
        )
        .fields
        .push((
            "interpolation".to_owned(),
            Value::Token(interpolation.to_owned()),
        ));
    }
    prim.attribute(
        "subdivisionScheme",
        "token",
        true,
        Some(Value::Token("none".to_owned())),
    );
    prim.relationship("material:binding", materials[mesh.material].clone());
    if !mesh.subsets.is_empty() {
        prim.attribute(
            "subsetFamily:materialBind:familyType",
            "token",
            true,
            Some(Value::Token("nonOverlapping".to_owned())),
        );
    }
    let mut subsets = Vec::new();
    for subset in &mesh.subsets {
        let path = prim.child(&subset.name);
        let mut s = Prim::new(path, "GeomSubset");
        s.metadata(
            "apiSchemas",
            Value::TokenListOp(ListOp::prepended(vec!["MaterialBindingAPI".to_owned()])),
        );
        s.attribute(
            "elementType",
            "token",
            true,
            Some(Value::Token("face".to_owned())),
        );
        s.attribute(
            "familyName",
            "token",
            true,
            Some(Value::Token("materialBind".to_owned())),
        );
        s.attribute(
            "indices",
            "int[]",
            false,
            Some(Value::IntArray(
                subset.triangles.iter().map(|&t| t as i32).collect(),
            )),
        );
        s.relationship("material:binding", materials[subset.material].clone());
        subsets.push(s);
    }
    prim.finish(out, at);
    for s in subsets {
        let at = reserve(out);
        s.finish(out, at);
    }
}

fn material_specs(scene: &Scene, material: &Material, path: Path, out: &mut Vec<Spec>) {
    let at = reserve(out);
    let mut prim = Prim::new(path.clone(), "Material");
    let surface = path.child("Surface");
    prim.connection(
        "outputs:surface",
        "token",
        surface.property("outputs:surface"),
    );
    prim.children.push("Surface".to_owned());
    prim.taken.insert("Surface".to_owned());
    let mut shader = Prim::new(surface, "Shader");
    shader.attribute(
        "info:id",
        "token",
        true,
        Some(Value::Token("UsdPreviewSurface".to_owned())),
    );
    let color = |c: [f64; 3]| Value::Vec3f(f32s(c));
    let float = |v: f64| Value::Float(v as f32);
    let mut texture = None;
    if let Some(image) = material.texture {
        let texture_path = prim.child("Texture");
        let st_path = prim.child("TexCoords");
        shader.connection(
            "inputs:diffuseColor",
            "color3f",
            texture_path.property("outputs:rgb"),
        );
        let mut t = Prim::new(texture_path, "Shader");
        t.attribute(
            "info:id",
            "token",
            true,
            Some(Value::Token("UsdUVTexture".to_owned())),
        );
        t.attribute(
            "inputs:file",
            "asset",
            false,
            Some(Value::Asset(scene.images[image].path.clone())),
        );
        t.attribute(
            "inputs:sourceColorSpace",
            "token",
            false,
            Some(Value::Token("sRGB".to_owned())),
        );
        t.connection("inputs:st", "float2", st_path.property("outputs:result"));
        t.attribute(
            "inputs:wrapS",
            "token",
            false,
            Some(Value::Token("repeat".to_owned())),
        );
        t.attribute(
            "inputs:wrapT",
            "token",
            false,
            Some(Value::Token("repeat".to_owned())),
        );
        t.attribute("outputs:rgb", "float3", false, None);
        let mut st = Prim::new(st_path, "Shader");
        st.attribute(
            "info:id",
            "token",
            true,
            Some(Value::Token("UsdPrimvarReader_float2".to_owned())),
        );
        st.attribute(
            "inputs:varname",
            "string",
            false,
            Some(Value::String("st".to_owned())),
        );
        st.attribute("outputs:result", "float2", false, None);
        texture = Some((t, st));
    } else {
        shader.attribute(
            "inputs:diffuseColor",
            "color3f",
            false,
            Some(color(material.diffuse_color)),
        );
    }
    shader.attribute(
        "inputs:metallic",
        "float",
        false,
        Some(float(material.metallic)),
    );
    shader.attribute(
        "inputs:roughness",
        "float",
        false,
        Some(float(material.roughness)),
    );
    shader.attribute(
        "inputs:opacity",
        "float",
        false,
        Some(float(material.opacity)),
    );
    shader.attribute("inputs:ior", "float", false, Some(float(material.ior)));
    shader.attribute(
        "inputs:clearcoat",
        "float",
        false,
        Some(float(material.clearcoat)),
    );
    shader.attribute(
        "inputs:clearcoatRoughness",
        "float",
        false,
        Some(float(material.clearcoat_roughness)),
    );
    shader.attribute(
        "inputs:emissiveColor",
        "color3f",
        false,
        Some(color(material.emissive_color)),
    );
    shader.attribute("outputs:surface", "token", false, None);
    prim.finish(out, at);
    let at = reserve(out);
    shader.finish(out, at);
    if let Some((t, st)) = texture {
        let at = reserve(out);
        t.finish(out, at);
        let at = reserve(out);
        st.finish(out, at);
    }
}
