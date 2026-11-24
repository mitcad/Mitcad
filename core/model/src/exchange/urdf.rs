// SPDX-License-Identifier: MIT
//! URDF export for ROS 2 (mitcad#2100): the design's mechanism as a robot
//! description in a ROS 2 package folder.
//!
//! - **Package:** `<folder>/<package>/` beside the path, the package named
//!   after the file (or `package`): `urdf/<name>.urdf`, `meshes/<link>_<body>.stl`
//!   (binary, metres, in the link's frame), and a minimal `package.xml` and
//!   `CMakeLists.txt` (ament_cmake) that install `urdf/` and `meshes/`.
//!   Mesh files are referred to as `package://<package>/meshes/…`.
//! - **Links:** one per unit of the root component that moves as one: its
//!   top-level occurrences joined by rigid joints (as-built ones too) and
//!   rigid groups; the grounded occurrences and the root's own bodies are
//!   the base link, whose frame is the design's origin. With neither, the
//!   first unit is the base, with a warning. Each other link's frame is
//!   its parent joint's frame at joint value 0 (where the joint's motion
//!   starts on the parent side), so that a link's meshes, colours and
//!   inertial are written in it.
//! - **Joints** from the root component's joints in effect at the marker:
//!   `rigid` joins its sides into one link; `revolute` is `revolute` with
//!   limits on `rz`, else `continuous`; `slider` is `prismatic` along its
//!   slide axis; `cylindrical`, `pin_slot`, `planar` and `ball` are chains
//!   of single-axis joints in the kind's motion order, through
//!   intermediate links of negligible mass. The URDF joint's value is the
//!   MitCad joint's value: its zero is the MitCad joint's zero, the
//!   joint's angle, offset and flip folded into the origins.
//! - **Direction:** the tree grows from the base. A joint whose `a` side is
//!   nearer the base is written reversed: parent and child swapped, the
//!   chain in reverse order, each axis negated (so the value and its
//!   limits stay the MitCad joint's).
//! - **Tree only:** a joint that would close a loop is left out with a
//!   warning; a unit that no joint leads to from the base is fixed to the
//!   base where it is, with a warning.
//! - Lengths in metres, angles in radians, masses in kilograms, inertia
//!   in kg·m². The pure writer ([`robot_xml`]) takes a [`Robot`]; the
//!   document side ([`Document::export_urdf`]) builds it.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use super::{BodyKind, MeshTolerance, Refinement, TriangleMesh, invalid, kernel_error, stl};
use crate::analysis::PhysicalProperties;
use crate::assembly::inverse;
use crate::document::{Document, InstanceView, ModelError};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::joints::{JointKind, Motion};
use crate::kernel::Kernel;
use crate::transform::Transform;

/// The `effort` written on revolute and prismatic joints when the export
/// names none: N·m for turns, N for slides.
pub const DEFAULT_EFFORT: f64 = 100.0;
/// The `velocity` written on revolute and prismatic joints when the export
/// names none: rad/s for turns, m/s for slides.
pub const DEFAULT_VELOCITY: f64 = 1.0;
/// The limits of a slide without its own (URDF requires them on
/// `prismatic` joints), metres either way.
pub const UNLIMITED_SLIDE: f64 = 1.0;
/// The mass of an intermediate link of a joint chain, and of a link
/// without bodies, kg.
pub const TINY_MASS: f64 = 1e-6;
/// The inertia (each principal moment) of such a link, kg·m².
pub const TINY_INERTIA: f64 = 1e-9;

/// What [`Document::export_urdf`] writes besides the path.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UrdfOptions {
    /// The package's name (lower-case letters, digits and underscores,
    /// starting with a letter); the file's name made one when None.
    pub package: Option<String>,
    pub refinement: Refinement,
    /// The `effort` of revolute and prismatic joints; [`DEFAULT_EFFORT`]
    /// when None.
    pub effort: Option<f64>,
    /// Their `velocity`; [`DEFAULT_VELOCITY`] when None.
    pub velocity: Option<f64>,
    /// Colours of bodies (sRGB in [0, 1]) over their appearances.
    pub colors: BTreeMap<BodyUid, [f64; 3]>,
}

/// What [`Document::export_urdf`] wrote.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UrdfExported {
    /// The package folder.
    pub folder: PathBuf,
    pub package: String,
    /// The URDF file.
    pub urdf: PathBuf,
    /// The links and the URDF joints, in the file's order.
    pub links: Vec<String>,
    pub joints: Vec<String>,
    /// The mesh files written, in `meshes/`.
    pub meshes: Vec<String>,
    /// The bodies written (once per body, in order).
    pub bodies: Vec<BodyUid>,
    /// Bodies left out and why (hidden, empty).
    pub skipped: Vec<(BodyUid, String)>,
    /// The joint features left out (loops, sides that move as one).
    pub left_out: Vec<FeatureUid>,
    /// In words, for a person: what was written otherwise than the design
    /// has it.
    pub warnings: Vec<String>,
}

// The robot description, as written.

/// A URDF robot: links and joints that form a tree from the first link.
#[derive(Debug, Clone, PartialEq)]
pub struct Robot {
    pub name: String,
    /// The package the mesh files are in (`package://<package>/meshes/…`).
    pub package: String,
    pub links: Vec<Link>,
    pub joints: Vec<Joint>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub name: String,
    pub inertial: Inertial,
    pub visuals: Vec<Visual>,
}

/// A link's mass (kg), centre of mass (m, in the link's frame) and inertia
/// about it along the link's axes (kg·m²).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Inertial {
    pub mass: f64,
    pub center: [f64; 3],
    pub inertia: [[f64; 3]; 3],
}

impl Inertial {
    /// The negligible inertial of an intermediate link or a link without
    /// bodies.
    pub fn tiny() -> Self {
        Self {
            mass: TINY_MASS,
            center: [0.0; 3],
            inertia: std::array::from_fn(|i| {
                std::array::from_fn(|j| if i == j { TINY_INERTIA } else { 0.0 })
            }),
        }
    }
}

/// A body's mesh, already in the link's frame: written as a `visual` and a
/// `collision`.
#[derive(Debug, Clone, PartialEq)]
pub struct Visual {
    /// The file's name in `meshes/`.
    pub mesh: String,
    /// sRGB in [0, 1]; no material when None.
    pub color: Option<[f64; 3]>,
}

/// URDF joint types MitCad writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JointType {
    Fixed,
    Revolute,
    Continuous,
    Prismatic,
}

impl JointType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::Revolute => "revolute",
            Self::Continuous => "continuous",
            Self::Prismatic => "prismatic",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Joint {
    pub name: String,
    pub kind: JointType,
    pub parent: String,
    pub child: String,
    /// The joint's frame in the parent link's, metres.
    pub origin: Transform,
    /// A unit axis in the joint's frame (not written for `fixed`).
    pub axis: [f64; 3],
    /// Lower and upper limits (rad or m) of `revolute` and `prismatic`.
    pub limits: Option<[f64; 2]>,
    pub effort: f64,
    pub velocity: f64,
}

/// A number as URDF files carry it: 12 significant digits, no exponent,
/// `0` for anything below 1e-12 in size.
pub fn num(v: f64) -> String {
    if !v.is_finite() || v.abs() < 1e-12 {
        return "0".to_owned();
    }
    let rounded: f64 = format!("{v:.11e}").parse().unwrap_or(v);
    format!("{rounded}")
}

fn triple(v: [f64; 3]) -> String {
    format!("{} {} {}", num(v[0]), num(v[1]), num(v[2]))
}

/// Roll, pitch and yaw (fixed axes x, y, z: `R = Rz(yaw) · Ry(pitch) ·
/// Rx(roll)`) of a rotation.
pub fn rpy(r: &[[f64; 3]; 3]) -> [f64; 3] {
    let pitch = (-r[2][0]).clamp(-1.0, 1.0).asin();
    if r[2][0].abs() < 1.0 - 1e-12 {
        [r[2][1].atan2(r[2][2]), pitch, r[1][0].atan2(r[0][0])]
    } else {
        // Gimbal lock: the roll is taken by the yaw.
        [0.0, pitch, (-r[0][1]).atan2(r[1][1])]
    }
}

fn origin_xml(t: &Transform) -> String {
    format!(
        "<origin xyz=\"{}\" rpy=\"{}\"/>",
        triple(t.translation),
        triple(rpy(&t.linear))
    )
}

/// A colour's material name, `color_rrggbb`.
fn material_name(c: [f64; 3]) -> String {
    let [r, g, b] = c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    format!("color_{r:02x}{g:02x}{b:02x}")
}

/// The URDF file of a robot.
pub fn robot_xml(robot: &Robot) -> String {
    let mut x = String::new();
    let _ = writeln!(x, "<?xml version=\"1.0\"?>");
    let _ = writeln!(
        x,
        "<!-- Written by MitCad: lengths in metres, angles in radians, masses in kilograms. -->"
    );
    let _ = writeln!(x, "<robot name=\"{}\">", robot.name);
    // Each colour once, referred to by name.
    let mut colors: BTreeMap<String, [f64; 3]> = BTreeMap::new();
    for link in &robot.links {
        for visual in &link.visuals {
            if let Some(c) = visual.color {
                colors.entry(material_name(c)).or_insert(c);
            }
        }
    }
    for (name, c) in &colors {
        let _ = writeln!(x, "  <material name=\"{name}\">");
        let _ = writeln!(
            x,
            "    <color rgba=\"{} {} {} 1\"/>",
            num(c[0]),
            num(c[1]),
            num(c[2])
        );
        let _ = writeln!(x, "  </material>");
    }
    for link in &robot.links {
        let _ = writeln!(x, "  <link name=\"{}\">", link.name);
        let i = &link.inertial;
        let _ = writeln!(x, "    <inertial>");
        let _ = writeln!(
            x,
            "      <origin xyz=\"{}\" rpy=\"0 0 0\"/>",
            triple(i.center)
        );
        let _ = writeln!(x, "      <mass value=\"{}\"/>", num(i.mass));
        let m = &i.inertia;
        let _ = writeln!(
            x,
            "      <inertia ixx=\"{}\" ixy=\"{}\" ixz=\"{}\" iyy=\"{}\" iyz=\"{}\" izz=\"{}\"/>",
            num(m[0][0]),
            num(m[0][1]),
            num(m[0][2]),
            num(m[1][1]),
            num(m[1][2]),
            num(m[2][2])
        );
        let _ = writeln!(x, "    </inertial>");
        for visual in &link.visuals {
            let mesh = format!(
                "<mesh filename=\"package://{}/meshes/{}\"/>",
                robot.package, visual.mesh
            );
            let _ = writeln!(x, "    <visual>");
            let _ = writeln!(x, "      <geometry>");
            let _ = writeln!(x, "        {mesh}");
            let _ = writeln!(x, "      </geometry>");
            if let Some(c) = visual.color {
                let _ = writeln!(x, "      <material name=\"{}\"/>", material_name(c));
            }
            let _ = writeln!(x, "    </visual>");
            let _ = writeln!(x, "    <collision>");
            let _ = writeln!(x, "      <geometry>");
            let _ = writeln!(x, "        {mesh}");
            let _ = writeln!(x, "      </geometry>");
            let _ = writeln!(x, "    </collision>");
        }
        let _ = writeln!(x, "  </link>");
    }
    for joint in &robot.joints {
        let _ = writeln!(
            x,
            "  <joint name=\"{}\" type=\"{}\">",
            joint.name,
            joint.kind.as_str()
        );
        let _ = writeln!(x, "    <parent link=\"{}\"/>", joint.parent);
        let _ = writeln!(x, "    <child link=\"{}\"/>", joint.child);
        let _ = writeln!(x, "    {}", origin_xml(&joint.origin));
        if joint.kind != JointType::Fixed {
            let _ = writeln!(x, "    <axis xyz=\"{}\"/>", triple(joint.axis));
        }
        match (joint.kind, joint.limits) {
            (JointType::Fixed, _) => {}
            (_, Some([lower, upper])) => {
                let _ = writeln!(
                    x,
                    "    <limit lower=\"{}\" upper=\"{}\" effort=\"{}\" velocity=\"{}\"/>",
                    num(lower),
                    num(upper),
                    num(joint.effort),
                    num(joint.velocity)
                );
            }
            (_, None) => {
                let _ = writeln!(
                    x,
                    "    <limit effort=\"{}\" velocity=\"{}\"/>",
                    num(joint.effort),
                    num(joint.velocity)
                );
            }
        }
        let _ = writeln!(x, "  </joint>");
    }
    let _ = writeln!(x, "</robot>");
    x
}

/// The `package.xml` of the package (format 3, ament_cmake).
pub fn package_xml(package: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?>
<package format=\"3\">
  <name>{package}</name>
  <version>0.0.0</version>
  <description>Robot description of {package}, exported by MitCad.</description>
  <maintainer email=\"user@example.com\">MitCad user</maintainer>
  <license>TODO</license>
  <buildtool_depend>ament_cmake</buildtool_depend>
  <export>
    <build_type>ament_cmake</build_type>
  </export>
</package>
"
    )
}

/// The package's `CMakeLists.txt`: installs `urdf/` and `meshes/`.
pub fn package_cmake(package: &str) -> String {
    format!(
        "cmake_minimum_required(VERSION 3.8)
project({package})
find_package(ament_cmake REQUIRED)
install(DIRECTORY urdf meshes DESTINATION share/${{PROJECT_NAME}})
ament_package()
"
    )
}

/// A name URDF and file systems take: letters, digits, `_`, `-` and `.`,
/// anything else `_`; `fallback` when nothing is left.
pub fn xml_name(name: &str, fallback: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches('_').to_owned();
    if cleaned.is_empty() || cleaned.starts_with('.') {
        fallback.to_owned()
    } else {
        cleaned
    }
}

/// A ROS package name made of a file's name (REP 144): lower-case
/// letters, digits and single underscores, starting with a letter.
pub fn package_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let c = c.to_ascii_lowercase();
        let c = if c.is_ascii_lowercase() || c.is_ascii_digit() {
            c
        } else {
            '_'
        };
        if c == '_' && (out.is_empty() || out.ends_with('_')) {
            continue;
        }
        out.push(c);
    }
    let out = out.trim_end_matches('_').to_owned();
    match out.chars().next() {
        None => "robot".to_owned(),
        Some(first) if !first.is_ascii_lowercase() => format!("robot_{out}"),
        Some(_) => out,
    }
}

/// Whether `name` is a valid ROS package name.
pub fn is_package_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !name.contains("__")
        && !name.ends_with('_')
}

/// `name`, or `name_2`, `name_3`, … when taken; taken then.
fn unique(name: &str, taken: &mut BTreeSet<String>) -> String {
    let mut candidate = name.to_owned();
    let mut n = 2;
    while taken.contains(&candidate) {
        candidate = format!("{name}_{n}");
        n += 1;
    }
    taken.insert(candidate.clone());
    candidate
}

/// Physical properties moved by a rigid transform: the centre moved, the
/// inertia turned (`R I Rᵀ`).
fn placed(p: &PhysicalProperties, t: &Transform) -> PhysicalProperties {
    let l = &t.linear;
    let mut moved = *p;
    moved.center_of_mass = t.apply_point(p.center_of_mass);
    moved.inertia = std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            (0..3)
                .map(|a| {
                    (0..3)
                        .map(|b| l[i][a] * p.inertia[a][b] * l[j][b])
                        .sum::<f64>()
                })
                .sum()
        })
    });
    moved
}

/// One motion of a chain: its URDF axis in the joint's frame (negated on a
/// reversed joint) and its MitCad limits.
#[derive(Debug, Clone, Copy)]
struct Step {
    motion: Motion,
    sign: f64,
    min: Option<f64>,
    max: Option<f64>,
    /// Its value at the marker (0 when the joint does not hold).
    value: f64,
}

impl Step {
    fn axis(&self) -> [f64; 3] {
        let a = match self.motion {
            Motion::Tx | Motion::Rx => [1.0, 0.0, 0.0],
            Motion::Ty | Motion::Ry => [0.0, 1.0, 0.0],
            Motion::Tz | Motion::Rz => [0.0, 0.0, 1.0],
        };
        a.map(|v| v * self.sign)
    }

    /// This motion alone at its value (mm, radians).
    fn transform(&self) -> Transform {
        let axis = self.axis();
        if self.motion.is_rotation() {
            Transform::rotation([0.0; 3], axis, self.value).unwrap_or(Transform::IDENTITY)
        } else {
            Transform::translation(axis.map(|a| a * self.value))
        }
    }
}

/// Mesh files by name in `meshes/`, with their bytes.
type MeshFiles = Vec<(String, Vec<u8>)>;

/// A union-find over the root (0) and the top-level occurrences.
struct Units {
    parent: Vec<usize>,
}

impl Units {
    fn find(&mut self, mut i: usize) -> usize {
        while self.parent[i] != i {
            self.parent[i] = self.parent[self.parent[i]];
            i = self.parent[i];
        }
        i
    }

    /// Joins two sets, the smaller index the representative.
    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            let (low, high) = if a < b { (a, b) } else { (b, a) };
            self.parent[high] = low;
        }
    }
}

/// A joint of the root component between two units.
struct Candidate {
    uid: FeatureUid,
    name: String,
    a: usize,
    b: usize,
}

impl<K: Kernel> Document<K> {
    /// Writes the design's mechanism as a URDF robot description in a ROS 2
    /// package folder beside `path` (mitcad#2100; see the module
    /// documentation): the folder is named after the package, the URDF
    /// file after `path`.
    pub fn export_urdf(
        &self,
        path: &str,
        options: &UrdfOptions,
    ) -> Result<UrdfExported, ModelError> {
        let stem = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("robot");
        let package = match options.package.as_deref() {
            Some(given) if is_package_name(given) => given.to_owned(),
            Some(given) => {
                return Err(invalid(format!(
                    "{given} is no ROS package name: lower-case letters, digits and single \
                     underscores, starting with a letter"
                )));
            }
            None => package_name(stem),
        };
        let effort = options.effort.unwrap_or(DEFAULT_EFFORT);
        let velocity = options.velocity.unwrap_or(DEFAULT_VELOCITY);
        for (what, v) in [("effort", effort), ("velocity", velocity)] {
            if !(v > 0.0 && v.is_finite()) {
                return Err(invalid(format!(
                    "the joints' {what} must be greater than zero, not {v}"
                )));
            }
        }
        let tolerance = options.refinement.tolerance();
        if !(tolerance.deviation > 0.0 && tolerance.angle > 0.0) {
            return Err(invalid(
                "the mesh deviation and angle must be greater than zero",
            ));
        }
        let robot_name = xml_name(stem, "robot");
        let (robot, meshes, mut exported) = self.urdf_robot(
            &robot_name,
            &package,
            &tolerance,
            &options.colors,
            effort,
            velocity,
        )?;
        let parent = Path::new(path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let folder = parent.join(&package);
        let write = |file: &Path, data: &[u8]| {
            std::fs::write(file, data)
                .map_err(|e| invalid(format!("cannot write {}: {e}", file.display())))
        };
        for sub in ["urdf", "meshes"] {
            let dir = folder.join(sub);
            std::fs::create_dir_all(&dir)
                .map_err(|e| invalid(format!("cannot make the folder {}: {e}", dir.display())))?;
        }
        let urdf = folder.join("urdf").join(format!("{robot_name}.urdf"));
        write(&urdf, robot_xml(&robot).as_bytes())?;
        for (name, data) in &meshes {
            write(&folder.join("meshes").join(name), data)?;
        }
        write(
            &folder.join("package.xml"),
            package_xml(&package).as_bytes(),
        )?;
        write(
            &folder.join("CMakeLists.txt"),
            package_cmake(&package).as_bytes(),
        )?;
        exported.folder = folder;
        exported.package = package;
        exported.urdf = urdf;
        exported.links = robot.links.iter().map(|l| l.name.clone()).collect();
        exported.joints = robot.joints.iter().map(|j| j.name.clone()).collect();
        exported.meshes = meshes.into_iter().map(|(name, _)| name).collect();
        Ok(exported)
    }

    /// The robot of the design, its mesh files (name, bytes) and what was
    /// written ([`Document::export_urdf`]).
    #[allow(clippy::too_many_lines)]
    fn urdf_robot(
        &self,
        robot_name: &str,
        package: &str,
        tolerance: &MeshTolerance,
        colors: &BTreeMap<BodyUid, [f64; 3]>,
        effort: f64,
        velocity: f64,
    ) -> Result<(Robot, MeshFiles, UrdfExported), ModelError> {
        let assembly = self.assembly();
        let root = ComponentUid::ROOT;
        let mut exported = UrdfExported::default();
        // Node 0 the design itself, then the top-level occurrences.
        let top: Vec<OccurrenceUid> = assembly.children(root).map(|o| o.uid).collect();
        let node_of: BTreeMap<OccurrenceUid, usize> =
            top.iter().enumerate().map(|(i, o)| (*o, i + 1)).collect();
        let node = |path: &[OccurrenceUid]| match path.first() {
            None => Some(0),
            Some(first) => node_of.get(first).copied(),
        };
        let mut units = Units {
            parent: (0..=top.len()).collect(),
        };
        // Everything fixed in the design is the base: the root's own
        // geometry and the grounded occurrences.
        let mut grounded = false;
        for (i, o) in top.iter().enumerate() {
            if assembly.occurrence(*o).is_some_and(|o| o.grounded) {
                units.join(0, i + 1);
                grounded = true;
            }
        }
        // Rigid groups of the root component.
        for (_, component, members) in self.rigid_groups() {
            if *component != root {
                continue;
            }
            let nodes: Vec<usize> = members.iter().filter_map(|o| node(&[*o])).collect();
            for pair in nodes.windows(2) {
                units.join(pair[0], pair[1]);
            }
        }
        // The root component's joints in effect, in timeline order; rigid
        // ones join their sides.
        let state = self.joints_state();
        let mut candidates: Vec<Candidate> = Vec::new();
        let mut inner_joints = false;
        let mut constraint_sets: Vec<String> = Vec::new();
        let feature_name = |uid: FeatureUid| {
            self.feature(uid)
                .map_or_else(|| uid.to_string(), |e| e.name.clone())
        };
        for joint in &state.active {
            if joint.component != root || !joint.under.is_empty() {
                inner_joints = true;
                continue;
            }
            if joint.row.is_some() {
                let name = feature_name(joint.uid);
                if !constraint_sets.contains(&name) {
                    constraint_sets.push(name);
                }
                continue;
            }
            let (Some(a), Some(b)) = (node(&joint.a), node(&joint.b)) else {
                continue;
            };
            if joint.kind == JointKind::Rigid {
                units.join(a, b);
                continue;
            }
            candidates.push(Candidate {
                uid: joint.uid,
                name: feature_name(joint.uid),
                a,
                b,
            });
        }
        if inner_joints {
            exported.warnings.push(
                "Joints inside components are not written: each top-level occurrence moves as \
                 one link"
                    .to_owned(),
            );
        }
        for name in &constraint_sets {
            exported.warnings.push(format!(
                "{name} is a constraint set, which URDF cannot hold: left out"
            ));
        }
        // The bodies by unit, where the design places them.
        let views = self.bodies();
        let view_of = |uid: BodyUid| views.iter().find(|v| v.uid == uid);
        let mut placed_bodies: BTreeMap<usize, Vec<InstanceView>> = BTreeMap::new();
        let mut skipped: BTreeSet<BodyUid> = BTreeSet::new();
        let mut written: Vec<BodyUid> = Vec::new();
        for instance in self.instances() {
            let Some(view) = view_of(instance.body) else {
                continue;
            };
            if !instance.visible {
                if skipped.insert(instance.body) {
                    exported.skipped.push((instance.body, "hidden".to_owned()));
                }
                continue;
            }
            let kind = self.kernel().body_kind(view.shape).map_err(kernel_error)?;
            if kind == BodyKind::Empty {
                if skipped.insert(instance.body) {
                    exported
                        .skipped
                        .push((instance.body, "an empty body".to_owned()));
                }
                continue;
            }
            let Some(n) = node(&instance.path) else {
                continue;
            };
            if !written.contains(&instance.body) {
                written.push(instance.body);
            }
            let unit = units.find(n);
            placed_bodies.entry(unit).or_default().push(instance);
        }
        // The units written: those with bodies or joints.
        let mut present: BTreeSet<usize> = placed_bodies.keys().copied().collect();
        for c in &candidates {
            present.insert(units.find(c.a));
            present.insert(units.find(c.b));
        }
        let base = if grounded || placed_bodies.contains_key(&0) || present.contains(&0) {
            0
        } else {
            match present.iter().next() {
                Some(first) => *first,
                None => return Err(invalid("there are no bodies to export")),
            }
        };
        present.insert(base);
        // Link names: the base's `base_link` when it is the design's own,
        // else each unit's first occurrence's.
        let mut link_names: BTreeSet<String> = BTreeSet::new();
        let mut names: BTreeMap<usize, String> = BTreeMap::new();
        for unit in &present {
            let wanted = if *unit == 0 {
                "base_link".to_owned()
            } else {
                xml_name(&assembly.occurrence_name(top[unit - 1]), "link")
            };
            names.insert(*unit, unique(&wanted, &mut link_names));
        }
        if base != 0 {
            exported.warnings.push(format!(
                "Nothing is grounded and the design has no bodies of its own: {} is the base link",
                names[&base]
            ));
        }
        // The tree from the base, breadth first; each link's frame in the
        // design (mm) as the design places it.
        let mut frames: BTreeMap<usize, Transform> = BTreeMap::new();
        frames.insert(base, Transform::IDENTITY);
        let mut joints: Vec<Joint> = Vec::new();
        let mut extra_links: Vec<Link> = Vec::new();
        let mut joint_names: BTreeSet<String> = BTreeSet::new();
        let mut used = vec![false; candidates.len()];
        let mut order: Vec<usize> = vec![base];
        let mut queue: VecDeque<usize> = VecDeque::from([base]);
        loop {
            while let Some(u) = queue.pop_front() {
                for (k, c) in candidates.iter().enumerate() {
                    if used[k] {
                        continue;
                    }
                    let (ua, ub) = (units.find(c.a), units.find(c.b));
                    if ua != u && ub != u {
                        continue;
                    }
                    used[k] = true;
                    if ua == ub {
                        exported.left_out.push(c.uid);
                        exported
                            .warnings
                            .push(format!("{} joins parts that move as one: left out", c.name));
                        continue;
                    }
                    let other = if ua == u { ub } else { ua };
                    if frames.contains_key(&other) {
                        exported.left_out.push(c.uid);
                        exported.warnings.push(format!(
                            "{} closes a loop, which URDF cannot hold (it holds a tree): left out",
                            c.name
                        ));
                        continue;
                    }
                    // Reversed when its a side is the parent.
                    let reversed = ua == u;
                    let written = self.urdf_joint(
                        c,
                        reversed,
                        (&names[&u], &frames[&u]),
                        &names[&other],
                        &mut link_names,
                        &mut joint_names,
                        (effort, velocity),
                        &mut exported.warnings,
                    );
                    match written {
                        Some((child_frame, mut chain, links)) => {
                            frames.insert(other, child_frame);
                            joints.append(&mut chain);
                            extra_links.extend(links);
                            order.push(other);
                            queue.push_back(other);
                        }
                        None => exported.left_out.push(c.uid),
                    }
                }
            }
            // A unit no joint leads to from the base: fixed to it.
            let Some(loose) = present.iter().find(|u| !frames.contains_key(u)).copied() else {
                break;
            };
            exported.warnings.push(format!(
                "{} is not joined to the base: written fixed to {} where it is",
                names[&loose], names[&base]
            ));
            joints.push(Joint {
                name: unique(&names[&loose], &mut joint_names),
                kind: JointType::Fixed,
                parent: names[&base].clone(),
                child: names[&loose].clone(),
                origin: Transform::IDENTITY,
                axis: [0.0, 0.0, 1.0],
                limits: None,
                effort,
                velocity,
            });
            frames.insert(loose, Transform::IDENTITY);
            order.push(loose);
            queue.push_back(loose);
        }
        // The links: meshes and inertials in their frames.
        let mut meshes: Vec<(String, Vec<u8>)> = Vec::new();
        let mut mesh_names: BTreeSet<String> = BTreeSet::new();
        let mut body_meshes: BTreeMap<BodyUid, TriangleMesh> = BTreeMap::new();
        let mut properties: BTreeMap<(BodyUid, u64), PhysicalProperties> = BTreeMap::new();
        let mut without_material = 0usize;
        let mut default_density: Option<(String, f64)> = None;
        let mut links: Vec<Link> = Vec::new();
        for unit in &order {
            let name = names[unit].clone();
            let into_link = inverse(&frames[unit]);
            let mut visuals = Vec::new();
            let mut parts: Vec<PhysicalProperties> = Vec::new();
            for instance in placed_bodies.get(unit).map_or(&[][..], Vec::as_slice) {
                let Some(view) = view_of(instance.body) else {
                    continue;
                };
                let to_link = into_link.after(&instance.transform);
                // Its mesh in the link's frame, metres.
                let mesh = match body_meshes.entry(instance.body) {
                    std::collections::btree_map::Entry::Occupied(found) => found.into_mut(),
                    std::collections::btree_map::Entry::Vacant(slot) => {
                        slot.insert(self.export_mesh(view, tolerance)?)
                    }
                };
                let mirrored = to_link.determinant() < 0.0;
                let moved = TriangleMesh {
                    vertices: mesh
                        .vertices
                        .iter()
                        .map(|v| to_link.apply_point(*v).map(|x| x / 1000.0))
                        .collect(),
                    triangles: mesh
                        .triangles
                        .iter()
                        .map(|[a, b, c]| if mirrored { [*a, *c, *b] } else { [*a, *b, *c] })
                        .collect(),
                };
                let file = unique(
                    &format!("{name}_{}", xml_name(&view.name, "body")),
                    &mut mesh_names,
                );
                let file = format!("{file}.stl");
                meshes.push((file.clone(), stl::write(&moved, &view.name, false)));
                let color = colors
                    .get(&instance.body)
                    .copied()
                    .or_else(|| {
                        self.body_attributes(instance.body)
                            .appearance
                            .as_deref()
                            .and_then(|id| self.appearance(id))
                            .map(|a| a.base_color)
                    })
                    .or_else(|| self.imported_color(instance.body));
                visuals.push(Visual { mesh: file, color });
                // Its mass where the link has it.
                let at = (!instance.path.is_empty()).then_some(instance.path.as_slice());
                let (material, source) = self.body_material(instance.body, at);
                if source == crate::MaterialSource::Default {
                    without_material += 1;
                    default_density = Some((material.name.clone(), material.density));
                }
                let key = (instance.body, material.density.to_bits());
                let local = match properties.get(&key) {
                    Some(found) => *found,
                    None => {
                        let found = self
                            .kernel()
                            .physical_properties(view.shape, material.density)
                            .map_err(|e| invalid(super::body_failure(&view.name, view.uid, &e)))?;
                        properties.insert(key, found);
                        found
                    }
                };
                parts.push(placed(&local, &to_link));
            }
            let total = crate::analysis::combine(&parts);
            let weighed = total.mass > 0.0;
            let inertial = if parts.is_empty() || !weighed {
                Inertial::tiny()
            } else {
                Inertial {
                    mass: total.mass,
                    center: total.center_of_mass.map(|v| v / 1000.0),
                    inertia: total.inertia.map(|row| row.map(|v| v / 1e6)),
                }
            };
            links.push(Link {
                name,
                inertial,
                visuals,
            });
        }
        links.extend(extra_links);
        if without_material > 0
            && let Some((material, density)) = default_density
        {
            exported.warnings.push(if without_material == 1 {
                format!(
                    "1 body has no material: its mass is from the default density ({material}, \
                     {density} g/cm³)"
                )
            } else {
                format!(
                    "{without_material} bodies have no material: their masses are from the \
                     default density ({material}, {density} g/cm³)"
                )
            });
        }
        exported.bodies = written;
        let robot = Robot {
            name: robot_name.to_owned(),
            package: package.to_owned(),
            links,
            joints,
        };
        Ok((robot, meshes, exported))
    }

    /// The URDF joints of a MitCad joint from its parent link (name and
    /// frame in the design) to `child`, reversed when its `a` side is the
    /// parent: the child link's frame in the design (mm), the joints in
    /// metres and the intermediate links of a chain. None when the joint
    /// has no result at the marker.
    #[allow(clippy::too_many_arguments)]
    fn urdf_joint(
        &self,
        c: &Candidate,
        reversed: bool,
        (parent, parent_frame): (&str, &Transform),
        child: &str,
        link_names: &mut BTreeSet<String>,
        joint_names: &mut BTreeSet<String>,
        (effort, velocity): (f64, f64),
        warnings: &mut Vec<String>,
    ) -> Option<(Transform, Vec<Joint>, Vec<Link>)> {
        let result = self.joint_result(c.uid)?;
        let active = self
            .joints_state()
            .active
            .iter()
            .find(|j| j.uid == c.uid && j.row.is_none())?;
        let holds = result.values.is_some();
        if !holds {
            warnings.push(format!(
                "{} does not hold where the parts are: written from where its sides' frames are",
                c.name
            ));
        }
        let value = |m: Motion| {
            result
                .values
                .as_ref()
                .and_then(|v| v.get(&m))
                .copied()
                .unwrap_or(0.0)
        };
        let limit = |m: Motion| {
            active
                .motions
                .iter()
                .find(|s| s.motion == m)
                .map_or((None, None), |s| (s.min, s.max))
        };
        let mut steps: Vec<Step> = result
            .kind
            .motions(result.slide)
            .into_iter()
            .map(|motion| {
                let (min, max) = limit(motion);
                Step {
                    motion,
                    sign: 1.0,
                    min,
                    max,
                    value: value(motion),
                }
            })
            .collect();
        // frame_a = frame_b · motion · alignment: forward, the motion starts
        // at b's frame and the child (a) is at frame_a · alignment⁻¹;
        // reversed, it starts at frame_a · alignment⁻¹ and runs backwards.
        let unaligned_a = result.frame_a.after(&inverse(&active.alignment));
        let (start, child_frame) = if reversed {
            steps.reverse();
            for s in &mut steps {
                s.sign = -1.0;
            }
            (unaligned_a, result.frame_b)
        } else {
            (result.frame_b, unaligned_a)
        };
        let to_m = |t: Transform| Transform {
            linear: t.linear,
            translation: t.translation.map(|v| v / 1000.0),
        };
        let mut joints = Vec::new();
        let mut links = Vec::new();
        let base_name = xml_name(&c.name, "joint");
        let count = steps.len();
        let mut from = parent.to_owned();
        let mut origin = to_m(inverse(parent_frame).after(&start));
        for (i, step) in steps.iter().enumerate() {
            let last = i + 1 == count;
            let to = if last {
                child.to_owned()
            } else {
                let name = unique(&format!("{base_name}_{}", i + 1), link_names);
                links.push(Link {
                    name: name.clone(),
                    inertial: Inertial::tiny(),
                    visuals: Vec::new(),
                });
                name
            };
            let joint_name = if count == 1 {
                unique(&base_name, joint_names)
            } else {
                unique(
                    &format!("{base_name}_{}", step.motion.as_str()),
                    joint_names,
                )
            };
            let rotation = step.motion.is_rotation();
            let (kind, limits) = match (step.min, step.max) {
                (None, None) if rotation => (JointType::Continuous, None),
                (None, None) => {
                    warnings.push(format!(
                        "{} slides without limits: written with limits of ±{} m, which URDF \
                         requires",
                        c.name, UNLIMITED_SLIDE
                    ));
                    (
                        JointType::Prismatic,
                        Some([-UNLIMITED_SLIDE, UNLIMITED_SLIDE]),
                    )
                }
                (min, max) if rotation => {
                    if min.is_none() || max.is_none() {
                        warnings.push(format!(
                            "{} turns with one limit only: the other is written as half a turn",
                            c.name
                        ));
                    }
                    let pi = std::f64::consts::PI;
                    (
                        JointType::Revolute,
                        Some([min.unwrap_or(-pi), max.unwrap_or(pi)]),
                    )
                }
                (min, max) => {
                    if min.is_none() || max.is_none() {
                        warnings.push(format!(
                            "{} slides with one limit only: the other is written {} m away",
                            c.name, UNLIMITED_SLIDE
                        ));
                    }
                    let lower = min.map_or(-UNLIMITED_SLIDE, |v| v / 1000.0);
                    let upper = max.map_or(UNLIMITED_SLIDE, |v| v / 1000.0);
                    (JointType::Prismatic, Some([lower, upper]))
                }
            };
            joints.push(Joint {
                name: joint_name,
                kind,
                parent: from.clone(),
                child: to.clone(),
                origin,
                axis: step.axis(),
                limits,
                effort,
                velocity,
            });
            // The next joint of the chain starts where this one moved.
            origin = Transform::IDENTITY;
            from = to;
        }
        // The child's frame: where the chain's motions at their values
        // put it (the side's own frame when the joint does not hold).
        let moved = steps.iter().fold(start, |t, s| t.after(&s.transform()));
        Some((if holds { moved } else { child_frame }, joints, links))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn_z(angle: f64) -> Transform {
        Transform::rotation([0.0; 3], [0.0, 0.0, 1.0], angle).unwrap()
    }

    /// A synthetic two-link arm: a base and an arm on a revolute joint
    /// 0.1 m up, turned a quarter, limited to ±90°.
    fn arm() -> Robot {
        let mut origin = turn_z(std::f64::consts::FRAC_PI_2);
        origin.translation = [0.0, 0.0, 0.1];
        Robot {
            name: "arm".to_owned(),
            package: "arm".to_owned(),
            links: vec![
                Link {
                    name: "base_link".to_owned(),
                    inertial: Inertial {
                        mass: 2.0,
                        center: [0.0, 0.0, 0.025],
                        inertia: [[0.01, 0.0, 0.0], [0.0, 0.02, 0.0], [0.0, 0.0, 0.03]],
                    },
                    visuals: vec![Visual {
                        mesh: "base_link_Base.stl".to_owned(),
                        color: Some([1.0, 0.0, 0.0]),
                    }],
                },
                Link {
                    name: "Arm_1".to_owned(),
                    inertial: Inertial::tiny(),
                    visuals: vec![Visual {
                        mesh: "Arm_1_Arm.stl".to_owned(),
                        color: None,
                    }],
                },
            ],
            joints: vec![Joint {
                name: "Joint1".to_owned(),
                kind: JointType::Revolute,
                parent: "base_link".to_owned(),
                child: "Arm_1".to_owned(),
                origin,
                axis: [0.0, 0.0, 1.0],
                limits: Some([-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2]),
                effort: DEFAULT_EFFORT,
                velocity: DEFAULT_VELOCITY,
            }],
        }
    }

    #[test]
    fn a_two_link_arm_is_written_exactly() {
        let expected = r#"<?xml version="1.0"?>
<!-- Written by MitCad: lengths in metres, angles in radians, masses in kilograms. -->
<robot name="arm">
  <material name="color_ff0000">
    <color rgba="1 0 0 1"/>
  </material>
  <link name="base_link">
    <inertial>
      <origin xyz="0 0 0.025" rpy="0 0 0"/>
      <mass value="2"/>
      <inertia ixx="0.01" ixy="0" ixz="0" iyy="0.02" iyz="0" izz="0.03"/>
    </inertial>
    <visual>
      <geometry>
        <mesh filename="package://arm/meshes/base_link_Base.stl"/>
      </geometry>
      <material name="color_ff0000"/>
    </visual>
    <collision>
      <geometry>
        <mesh filename="package://arm/meshes/base_link_Base.stl"/>
      </geometry>
    </collision>
  </link>
  <link name="Arm_1">
    <inertial>
      <origin xyz="0 0 0" rpy="0 0 0"/>
      <mass value="0.000001"/>
      <inertia ixx="0.000000001" ixy="0" ixz="0" iyy="0.000000001" iyz="0" izz="0.000000001"/>
    </inertial>
    <visual>
      <geometry>
        <mesh filename="package://arm/meshes/Arm_1_Arm.stl"/>
      </geometry>
    </visual>
    <collision>
      <geometry>
        <mesh filename="package://arm/meshes/Arm_1_Arm.stl"/>
      </geometry>
    </collision>
  </link>
  <joint name="Joint1" type="revolute">
    <parent link="base_link"/>
    <child link="Arm_1"/>
    <origin xyz="0 0 0.1" rpy="0 0 1.57079632679"/>
    <axis xyz="0 0 1"/>
    <limit lower="-1.57079632679" upper="1.57079632679" effort="100" velocity="1"/>
  </joint>
</robot>
"#;
        let written = robot_xml(&arm());
        assert_eq!(written, expected);
        // It parses as XML.
        let parsed = roxmltree::Document::parse(&written).unwrap();
        assert_eq!(parsed.root_element().tag_name().name(), "robot");
    }

    #[test]
    fn continuous_and_fixed_joints_carry_what_urdf_wants() {
        let mut robot = arm();
        robot.joints[0].kind = JointType::Continuous;
        robot.joints[0].limits = None;
        let text = robot_xml(&robot);
        assert!(text.contains("<joint name=\"Joint1\" type=\"continuous\">"));
        assert!(text.contains("<limit effort=\"100\" velocity=\"1\"/>"));
        robot.joints[0].kind = JointType::Fixed;
        let text = robot_xml(&robot);
        assert!(!text.contains("<axis"));
        assert!(!text.contains("<limit"));
    }

    #[test]
    fn roll_pitch_yaw_give_the_rotation_back() {
        for (roll, pitch, yaw) in [(0.3, -0.4, 1.2), (0.0, 0.0, -2.5), (1.0, 0.2, 0.0)] {
            let r = Transform::rotation([0.0; 3], [0.0, 0.0, 1.0], yaw)
                .unwrap()
                .after(&Transform::rotation([0.0; 3], [0.0, 1.0, 0.0], pitch).unwrap())
                .after(&Transform::rotation([0.0; 3], [1.0, 0.0, 0.0], roll).unwrap());
            let [x, y, z] = rpy(&r.linear);
            assert!(
                (x - roll).abs() < 1e-12 && (y - pitch).abs() < 1e-12 && (z - yaw).abs() < 1e-12
            );
        }
    }

    #[test]
    fn names_are_made_safe() {
        assert_eq!(xml_name("Arm:1", "link"), "Arm_1");
        assert_eq!(xml_name("<&>", "link"), "link");
        assert_eq!(package_name("My Robot-Arm 2"), "my_robot_arm_2");
        assert_eq!(package_name("2arm"), "robot_2arm");
        assert_eq!(package_name("__"), "robot");
        assert!(is_package_name("my_arm2"));
        assert!(!is_package_name("My_arm"));
        assert!(!is_package_name("arm__2"));
        assert!(!is_package_name("_arm"));
        assert_eq!(num(-0.0), "0");
        assert_eq!(num(1e-13), "0");
        assert_eq!(num(0.1 + 0.2), "0.3");
    }

    #[test]
    fn the_package_files_install_the_description() {
        let cmake = package_cmake("arm");
        assert!(cmake.contains("project(arm)"));
        assert!(cmake.contains("install(DIRECTORY urdf meshes DESTINATION share/${PROJECT_NAME})"));
        let xml = package_xml("arm");
        let parsed = roxmltree::Document::parse(&xml).unwrap();
        let name = parsed
            .descendants()
            .find(|n| n.has_tag_name("name"))
            .and_then(|n| n.text());
        assert_eq!(name, Some("arm"));
    }
}
