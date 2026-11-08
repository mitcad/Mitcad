// SPDX-License-Identifier: MIT
//! The camera of a preview image (mitcad#1486): a named view, angles or an
//! explicit camera, framed on a box. A pure function of its inputs, shared
//! by every quality of `model_render` (the fast rasterizer, and the ray
//! traced picture of mitcad#1489), so that both frame a design alike.
//!
//! Conventions, as the application's view (`app/OcctViewer.cpp`): Z is up;
//! `front` looks along +Y (the eye at -Y), `right` along -X, `top` along
//! -Z with +Y up; `home` (and `iso`) is the isometric view from the front
//! right, Z up, unless the design has a named view `Home`. Distances are
//! millimetres, angles in the arguments degrees.

use mitcad_vec3::{Vec3, add, cross, dot, norm, scale, sub, unit};
use serde_json::{Value, json};

/// The vertical field of view of a perspective camera when none is given,
/// degrees (the application's view's).
pub const DEFAULT_FOV: f64 = 45.0;
/// How much of the picture's shorter extent a framed box takes.
const FILL: f64 = 0.85;
/// The height of the view framed on nothing (an empty design): the
/// origin, 100 mm top to bottom.
const EMPTY_HEIGHT: f64 = 100.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Projection {
    Perspective,
    Orthographic,
}

impl Projection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Perspective => "perspective",
            Self::Orthographic => "orthographic",
        }
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "perspective" => Ok(Self::Perspective),
            "orthographic" => Ok(Self::Orthographic),
            other => Err(format!(
                "projection must be perspective or orthographic, not {other}"
            )),
        }
    }
}

/// The camera used: where it is, what it looks at, which way is up, and
/// how much it shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    /// A unit vector square to the line of sight.
    pub up: Vec3,
    pub projection: Projection,
    /// Millimetres of the model the picture shows at the target, top to
    /// bottom.
    pub height: f64,
    /// The vertical field of view, degrees (the perspective's; an
    /// orthographic camera keeps the one it would use).
    pub fov: f64,
}

impl Camera {
    /// The unit direction from the eye to the target.
    pub fn direction(&self) -> Vec3 {
        unit(sub(self.target, self.eye)).unwrap_or([0.0, 1.0, 0.0])
    }

    /// The camera as `model_render` answers it, and takes it back as
    /// `view`: `eye`, `target`, `up`, `projection`, `height` and, for a
    /// perspective, `fov`.
    pub fn to_json(&self) -> Value {
        let mut value = json!({
            "eye": self.eye,
            "target": self.target,
            "up": self.up,
            "projection": self.projection.as_str(),
            "height": self.height,
        });
        if self.projection == Projection::Perspective {
            value["fov"] = json!(self.fov);
        }
        value
    }
}

/// The view asked for.
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    /// `home`, `iso`, `front`, `back`, `left`, `right`, `top`, `bottom`,
    /// or the name of a view saved in the design.
    Named(String),
    /// Degrees: azimuth about Z from the front (0: from -Y, 90: from +X),
    /// elevation above the XY plane (90: from above).
    Angles { azimuth: f64, elevation: f64 },
    /// A camera as given; `height`, `fov` and `projection` optional.
    Explicit {
        eye: Vec3,
        target: Vec3,
        up: Vec3,
        height: Option<f64>,
        fov: Option<f64>,
        projection: Option<Projection>,
    },
}

impl Default for View {
    fn default() -> Self {
        Self::Named("home".to_owned())
    }
}

/// A view saved in the design (the model's named views).
#[derive(Debug, Clone, PartialEq)]
pub struct SavedView {
    pub name: String,
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub perspective: bool,
    pub height: f64,
}

/// An axis-aligned box in the design's coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    /// The box of points; None for none.
    pub fn of_points(points: impl IntoIterator<Item = Vec3>) -> Option<Self> {
        let mut found: Option<Self> = None;
        for p in points {
            if !p.iter().all(|c| c.is_finite()) {
                continue;
            }
            found = Some(match found {
                None => Self { min: p, max: p },
                Some(b) => b.with(p),
            });
        }
        found
    }

    pub fn with(self, p: Vec3) -> Self {
        Self {
            min: [0, 1, 2].map(|i| self.min[i].min(p[i])),
            max: [0, 1, 2].map(|i| self.max[i].max(p[i])),
        }
    }

    pub fn union(self, other: Self) -> Self {
        self.with(other.min).with(other.max)
    }

    pub fn center(&self) -> Vec3 {
        scale(add(self.min, self.max), 0.5)
    }

    pub fn corners(&self) -> [Vec3; 8] {
        let (a, b) = (self.min, self.max);
        [
            [a[0], a[1], a[2]],
            [b[0], a[1], a[2]],
            [a[0], b[1], a[2]],
            [b[0], b[1], a[2]],
            [a[0], a[1], b[2]],
            [b[0], a[1], b[2]],
            [a[0], b[1], b[2]],
            [b[0], b[1], b[2]],
        ]
    }
}

/// What the camera frames: the box of what `fit` names (None: nothing,
/// e.g. an empty design), and whether `fit` was given explicitly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub bounds: Option<Aabb>,
    pub explicit: bool,
}

/// The camera of a view: `projection` (the argument; None: the explicit
/// view's own, else the saved view's, else perspective), framed on
/// `frame` for a picture of `aspect` (width / height), with the design's
/// `saved` views; and warnings.
///
/// - A standard view or angles: the direction and up of the view, framed
///   on the box (centred, the box taking 85 % of the shorter extent).
/// - A saved view or an explicit camera: as it is, unless `fit` was given
///   explicitly, which keeps its direction and up and frames the box. An
///   orthographic camera without a height frames the box around its
///   target; a perspective one shows what its distance and field of view
///   show (`fov` default 45 degrees).
pub fn resolve_camera(
    view: &View,
    projection: Option<Projection>,
    frame: Frame,
    aspect: f64,
    saved: &[SavedView],
) -> Result<(Camera, Vec<String>), String> {
    let aspect = if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        1.0
    };
    let mut warnings = Vec::new();
    let saved_view = |name: &str| {
        saved
            .iter()
            .find(|v| v.name == name)
            .or_else(|| saved.iter().find(|v| v.name.eq_ignore_ascii_case(name)))
    };
    // (direction, up) of a standard view, or a camera as given.
    enum Base {
        Standard(Vec3, Vec3),
        Given {
            eye: Vec3,
            target: Vec3,
            up: Vec3,
            height: Option<f64>,
            fov: Option<f64>,
            projection: Option<Projection>,
        },
    }
    let from_saved = |v: &SavedView| Base::Given {
        eye: v.eye,
        target: v.target,
        up: v.up,
        height: Some(v.height),
        fov: None,
        projection: Some(if v.perspective {
            Projection::Perspective
        } else {
            Projection::Orthographic
        }),
    };
    let base = match view {
        View::Named(name) => {
            let key = name.trim().to_ascii_lowercase();
            match key.as_str() {
                "home" => match saved_view("Home") {
                    Some(v) => from_saved(v),
                    None => Base::Standard([-1.0, 1.0, -1.0], [0.0, 0.0, 1.0]),
                },
                "iso" | "isometric" => Base::Standard([-1.0, 1.0, -1.0], [0.0, 0.0, 1.0]),
                "front" => Base::Standard([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
                "back" => Base::Standard([0.0, -1.0, 0.0], [0.0, 0.0, 1.0]),
                "right" => Base::Standard([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
                "left" => Base::Standard([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
                "top" => Base::Standard([0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
                "bottom" => Base::Standard([0.0, 0.0, 1.0], [0.0, -1.0, 0.0]),
                _ => match saved_view(name.trim()) {
                    Some(v) => from_saved(v),
                    None => {
                        let mut names: Vec<&str> = vec![
                            "home", "iso", "front", "back", "left", "right", "top", "bottom",
                        ];
                        names.extend(saved.iter().map(|v| v.name.as_str()));
                        return Err(format!(
                            "no view {name}: the views are {}",
                            names.join(", ")
                        ));
                    }
                },
            }
        }
        View::Angles { azimuth, elevation } => {
            if !(azimuth.is_finite() && elevation.is_finite()) {
                return Err("view azimuth and elevation must be numbers".to_owned());
            }
            if elevation.abs() > 90.0 {
                return Err(format!(
                    "view elevation must be between -90 and 90 degrees, not {elevation}"
                ));
            }
            let (sa, ca) = azimuth.to_radians().sin_cos();
            let (se, ce) = elevation.to_radians().sin_cos();
            // From the target towards the eye, and up along the meridian.
            let toward_eye = [ce * sa, -ce * ca, se];
            let up = [-se * sa, se * ca, ce];
            Base::Standard(scale(toward_eye, -1.0), up)
        }
        View::Explicit {
            eye,
            target,
            up,
            height,
            fov,
            projection,
        } => Base::Given {
            eye: *eye,
            target: *target,
            up: *up,
            height: *height,
            fov: *fov,
            projection: *projection,
        },
    };
    let (direction, up, given) = match &base {
        Base::Standard(direction, up) => (*direction, *up, None),
        Base::Given {
            eye,
            target,
            up,
            height,
            fov,
            projection,
        } => {
            if !eye.iter().chain(target).chain(up).all(|c| c.is_finite()) {
                return Err("the view's eye, target and up must be numbers".to_owned());
            }
            (
                sub(*target, *eye),
                *up,
                Some((*eye, *target, *height, *fov, *projection)),
            )
        }
    };
    if norm(direction) <= 1e-12 {
        return Err("the view's eye must differ from its target".to_owned());
    }
    let direction = unit(direction).ok_or("the view's eye must differ from its target")?;
    let across = sub(up, scale(direction, dot(up, direction)));
    if norm(across) <= 1e-9 * norm(up) || norm(up) <= 1e-12 {
        return Err("the view's up must not be zero or along its line of sight".to_owned());
    }
    // A unit up square to the sight (a camera this function answered) is
    // kept to the bit, so that the answer given back gives it again.
    let up = if (norm(up) - 1.0).abs() < 1e-12 && dot(up, direction).abs() < 1e-12 {
        up
    } else {
        unit(across).ok_or("the view's up must not be along its line of sight")?
    };
    let fov = given.and_then(|g| g.3).unwrap_or(DEFAULT_FOV);
    if !(fov.is_finite() && (1.0..=170.0).contains(&fov)) {
        return Err(format!(
            "the view's fov must be between 1 and 170 degrees, not {fov}"
        ));
    }
    let projection = projection
        .or(given.and_then(|g| g.4))
        .unwrap_or(Projection::Perspective);
    let reframe = given.is_none() || frame.explicit;
    let bounds = match frame.bounds {
        Some(b) => Some(b),
        None if reframe => {
            warnings.push("there is nothing to frame: the origin is shown".to_owned());
            None
        }
        None => None,
    };
    let camera = if reframe {
        framed(direction, up, projection, fov, aspect, bounds)
    } else {
        let (eye, target, height, _, _) = given.expect("a given camera");
        let distance = norm(sub(target, eye));
        let half = (fov.to_radians() / 2.0).tan();
        let height = match projection {
            Projection::Perspective => 2.0 * distance * half,
            Projection::Orthographic => match height {
                Some(h) if h.is_finite() && h > 0.0 => h,
                Some(h) => return Err(format!("the view's height must be positive, not {h}")),
                None => {
                    // Framed around the target, which stays.
                    let fitted = framed(direction, up, projection, fov, aspect, bounds);
                    match bounds {
                        Some(b) => {
                            let (r, u) = (cross(direction, up), up);
                            let mut need: f64 = 0.0;
                            for c in b.corners() {
                                let q = sub(c, target);
                                need = need.max(dot(q, u).abs()).max(dot(q, r).abs() / aspect);
                            }
                            if need > 0.0 {
                                2.0 * need / FILL
                            } else {
                                fitted.height
                            }
                        }
                        None => fitted.height,
                    }
                }
            },
        };
        Camera {
            eye,
            target,
            up,
            projection,
            height,
            fov,
        }
    };
    Ok((camera, warnings))
}

/// A camera looking along `direction` with `up`, framed on `bounds` (the
/// origin at [`EMPTY_HEIGHT`] when None).
fn framed(
    direction: Vec3,
    up: Vec3,
    projection: Projection,
    fov: f64,
    aspect: f64,
    bounds: Option<Aabb>,
) -> Camera {
    let right = cross(direction, up);
    let half = (fov.to_radians() / 2.0).tan();
    let Some(bounds) = bounds else {
        let distance = EMPTY_HEIGHT / 2.0 / half;
        let eye = scale(direction, -distance);
        return Camera {
            eye,
            target: [0.0; 3],
            up,
            projection,
            height: match projection {
                Projection::Perspective => 2.0 * norm(eye) * half,
                Projection::Orthographic => EMPTY_HEIGHT,
            },
            fov,
        };
    };
    let center = bounds.center();
    // The box's extent across the picture, about its centre.
    let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for c in bounds.corners() {
        let q = sub(c, center);
        let (x, y) = (dot(q, right), dot(q, up));
        x0 = x0.min(x);
        x1 = x1.max(x);
        y0 = y0.min(y);
        y1 = y1.max(y);
    }
    let target = add(
        center,
        add(scale(right, (x0 + x1) / 2.0), scale(up, (y0 + y1) / 2.0)),
    );
    let radius = norm(sub(bounds.max, bounds.min)) / 2.0;
    match projection {
        Projection::Orthographic => {
            let need = ((y1 - y0) / 2.0).max((x1 - x0) / 2.0 / aspect);
            let height = if need > 1e-9 {
                2.0 * need / FILL
            } else {
                EMPTY_HEIGHT
            };
            let distance = 2.0 * radius + height;
            Camera {
                eye: sub(target, scale(direction, distance)),
                target,
                up,
                projection,
                height,
                fov,
            }
        }
        Projection::Perspective => {
            // The nearest distance at which every corner is in the
            // frustum's framed part: |y| <= (d + z) * t, |x| <= (d + z) * t * aspect.
            let t = half * FILL;
            let mut distance: f64 = 0.0;
            let mut nearest: f64 = 0.0;
            for c in bounds.corners() {
                let q = sub(c, target);
                let (x, y, z) = (dot(q, right), dot(q, up), dot(q, direction));
                distance = distance
                    .max(y.abs() / t - z)
                    .max(x.abs() / (t * aspect) - z);
                nearest = nearest.max(-z);
            }
            // The eye outside the box, also for a flat one seen edge on.
            distance = distance.max(nearest * 1.05 + 1e-6);
            if distance <= 1e-9 {
                distance = EMPTY_HEIGHT / 2.0 / half;
            }
            let eye = sub(target, scale(direction, distance));
            // The height as a given camera's is measured, from its eye.
            Camera {
                eye,
                target,
                up,
                projection,
                height: 2.0 * norm(sub(target, eye)) * half,
                fov,
            }
        }
    }
}
