// SPDX-License-Identifier: MIT
//! A title block's SVG images (mitcad#1627) as paths: the file read with
//! usvg, which resolves its CSS, `use`s, transforms, basic shapes and
//! units into one tree of paths, and that tree turned into [`Shape`]s of
//! lines and cubic Béziers with solid colours. The PDF writer draws them as
//! vector paths ([`super::pdf`]), the DXF and DWG writers as lines
//! flattened within [`DEFLECTION`] ([`super::dxf`]); the SVG writer keeps
//! the file as it is. The view draws the file with Qt's own renderer.
//!
//! What a PDF or DXF file gets of an SVG image here:
//! - fills and strokes of paths with their colours, a gradient as its
//!   mean colour and a pattern as the colour of its first painted path;
//! - opacities (the paths', their paints' and their groups') multiplied
//!   onto each path: as transparency in a PDF, mixed with the paper's
//!   white in a DXF file;
//! - strokes' widths, caps, joins, miter limits and dashes, scaled with the
//!   image (in a DXF file only the width, as its layer's line weight);
//! - clip paths in a PDF (a DXF file draws the paths whole);
//! - images embedded as SVG files, as their paths;
//! - text as the outlines of its glyphs (mitcad#1637), filled and stroked
//!   as its paths are: laid out by usvg in the fonts its `font-family`
//!   names where this computer has them ([`fonts`]: the system's fonts and
//!   the bundled [`BUNDLED_FONT`]), else in the bundled font, which is
//!   also every generic family (`sans-serif`, `serif`, ...) and the font of
//!   text that names none, so that such text is the same on every machine.
//!
//! What is drawn otherwise or not at all, [`SvgImage::left_out`] says, and
//! the export warns of it ([`super::pdf_image::export_warnings`]): text
//! whose fonts this computer has none of (drawn in the bundled font),
//! characters no font has, and pictures inside the image (PNG, JPEG and
//! others), which are not drawn. Masks and filters are left out without a
//! word: the paths under them are drawn as they are.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use usvg::fontdb;
use usvg::tiny_skia_path::{PathSegment, Transform};

use super::P2;

/// How far a line of a flattened curve may leave the curve, mm of paper:
/// the drawing's views' deflection.
pub const DEFLECTION: f64 = 0.01;

/// The font of an image's text that names no font or only fonts this
/// computer does not have, and of its generic families: Droid Sans, the
/// font sketch text bundles too (`third_party/fonts/droid-sans`,
/// Apache-2.0), embedded here so that the exports draw such text alike on
/// every machine.
pub const BUNDLED_FONT: &str = "Droid Sans";

const DROID_SANS: &[u8] = include_bytes!("../../../third_party/fonts/droid-sans/DroidSans.ttf");
const DROID_SANS_BOLD: &[u8] =
    include_bytes!("../../../third_party/fonts/droid-sans/DroidSans-Bold.ttf");

/// A piece of a subpath from the end of the one before.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    Line(P2),
    /// Two control points and the end.
    Cubic(P2, P2, P2),
}

/// A subpath from its start through its segments, closed back to its start
/// or open.
#[derive(Debug, Clone, PartialEq)]
pub struct Subpath {
    pub start: P2,
    pub segments: Vec<Segment>,
    pub closed: bool,
}

/// An sRGB colour (each part 0 to 1) and how opaque it is (0 to 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colour {
    pub rgb: [f64; 3],
    pub alpha: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fill {
    pub colour: Colour,
    /// The even-odd rule, else the non-zero rule.
    pub even_odd: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cap {
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub colour: Colour,
    pub width: f64,
    pub cap: Cap,
    pub join: Join,
    pub miter_limit: f64,
    /// Dash and gap lengths; empty for a continuous line.
    pub dashes: Vec<f64>,
    pub dash_offset: f64,
}

/// A clip path: what lies outside it is not drawn. Its subpaths together
/// (the PDF writer takes them as one path by its rule).
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub subpaths: Vec<Subpath>,
    pub even_odd: bool,
}

/// A path of the image with its paint.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub subpaths: Vec<Subpath>,
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
    /// The stroke painted before the fill (`paint-order: stroke`).
    pub stroke_first: bool,
    /// Clip paths, each one cutting what the ones before left.
    pub clips: Vec<Clip>,
}

/// What of an image the exports draw otherwise or leave out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LeftOut {
    /// The `font-family` lists of its text this computer has no font of,
    /// as the file names them (comma-separated): their text is drawn in
    /// [`BUNDLED_FONT`].
    pub fonts: Vec<String>,
    /// Characters of its text that no font has.
    pub characters: bool,
    /// Pictures inside it (PNG, JPEG and others), not drawn.
    pub pictures: bool,
}

/// An SVG image as shapes in its own coordinates: x to the right, y down,
/// the image from (0, 0) to `size` (its `width` and `height`, or its
/// `viewBox`'s size).
#[derive(Debug, Clone, PartialEq)]
pub struct SvgImage {
    pub size: P2,
    pub shapes: Vec<Shape>,
    pub left_out: LeftOut,
}

/// An SVG file as shapes, or why usvg cannot read it.
pub fn read(bytes: &[u8]) -> Result<SvgImage, String> {
    let fonts_not_found = Mutex::new(BTreeSet::new());
    let characters_missing = AtomicBool::new(false);
    let options = usvg::Options {
        // Only images inside the file itself: no files of the disk by a
        // relative or absolute path.
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_: &str, _: &usvg::Options| None),
        },
        font_family: BUNDLED_FONT.to_owned(),
        font_resolver: font_resolver(&fonts_not_found, &characters_missing),
        // The fonts only for a file that may have text: reading the
        // system's fonts the first time takes a moment.
        fontdb: if may_have_text(bytes) {
            fonts()
        } else {
            Arc::new(fontdb::Database::new())
        },
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_data(bytes, &options).map_err(|e| e.to_string())?;
    // Its font resolver borrows what it noted.
    drop(options);
    let size = [
        f64::from(tree.size().width()),
        f64::from(tree.size().height()),
    ];
    let mut reader = Reader::default();
    reader.group(tree.root(), Affine::IDENTITY, 1.0, &[]);
    reader.left_out.fonts = fonts_not_found
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .into_iter()
        .collect();
    reader.left_out.characters = characters_missing.load(Ordering::Relaxed);
    Ok(SvgImage {
        size,
        shapes: reader.shapes,
        left_out: reader.left_out,
    })
}

/// An image file's base64 as shapes ([`read`]).
pub fn of_base64(base64: &str) -> Result<SvgImage, String> {
    let bytes = mitcad_model::library::decode_base64(base64)?;
    read(&bytes)
}

/// Whether the file may have text: `<text>` elements, or SVG images
/// embedded in it, which may have theirs. A file that is no UTF-8 or no
/// XML is left to usvg to judge.
fn may_have_text(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return true;
    };
    let options = usvg::roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let Ok(doc) = usvg::roxmltree::Document::parse_with_options(text, options) else {
        return true;
    };
    doc.descendants().any(|n| {
        n.is_element()
            && n.tag_name().namespace() == Some("http://www.w3.org/2000/svg")
            && (n.tag_name().name() == "text"
                || (n.tag_name().name() == "image"
                    && n.attributes()
                        .any(|a| a.name() == "href" && a.value().contains("image/svg+xml"))))
    })
}

/// The fonts of images' text, read once: the bundled [`BUNDLED_FONT`]
/// (regular and bold; italic from its regular, slanted by usvg) and the
/// system's fonts (on Linux the folders fontconfig's files name, on
/// Windows and macOS the system's and the user's font folders), every
/// generic family the bundled font.
pub fn fonts() -> Arc<fontdb::Database> {
    static FONTS: OnceLock<Arc<fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = fontdb::Database::new();
            db.load_font_data(DROID_SANS.to_vec());
            db.load_font_data(DROID_SANS_BOLD.to_vec());
            db.load_system_fonts();
            db.set_serif_family(BUNDLED_FONT);
            db.set_sans_serif_family(BUNDLED_FONT);
            db.set_cursive_family(BUNDLED_FONT);
            db.set_fantasy_family(BUNDLED_FONT);
            db.set_monospace_family(BUNDLED_FONT);
            Arc::new(db)
        })
        .clone()
}

/// How usvg picks the fonts of an image's text: the first family of a
/// `font-family` list this computer has a font of (the closest of its
/// weights, widths and styles), else [`BUNDLED_FONT`], the list then noted
/// in `not_found`; for a character the font lacks, another font of the
/// computer that has it (usvg's own choice), or none, which
/// `characters_missing` notes.
fn font_resolver<'a>(
    not_found: &'a Mutex<BTreeSet<String>>,
    characters_missing: &'a AtomicBool,
) -> usvg::FontResolver<'a> {
    let fallback = usvg::FontResolver::default_fallback_selector();
    usvg::FontResolver {
        select_font: Box::new(move |font: &usvg::Font, db: &mut Arc<fontdb::Database>| {
            let query = |families: &[fontdb::Family]| {
                db.query(&fontdb::Query {
                    families,
                    weight: fontdb::Weight(font.weight()),
                    stretch: stretch(font.stretch()),
                    style: style(font.style()),
                })
            };
            let named: Vec<fontdb::Family> = font.families().iter().map(family).collect();
            query(&named).or_else(|| {
                let names: Vec<String> = font
                    .families()
                    .iter()
                    .map(|f| match f {
                        usvg::FontFamily::Named(name) => name.clone(),
                        generic => generic.to_string(),
                    })
                    .collect();
                if !names.is_empty()
                    && let Ok(mut set) = not_found.lock()
                {
                    set.insert(names.join(", "));
                }
                query(&[fontdb::Family::Name(BUNDLED_FONT)])
            })
        }),
        select_fallback: Box::new(
            move |c: char, used: &[fontdb::ID], db: &mut Arc<fontdb::Database>| {
                let found = fallback(c, used, db);
                if found.is_none() {
                    characters_missing.store(true, Ordering::Relaxed);
                }
                found
            },
        ),
    }
}

fn family(family: &usvg::FontFamily) -> fontdb::Family<'_> {
    match family {
        usvg::FontFamily::Serif => fontdb::Family::Serif,
        usvg::FontFamily::SansSerif => fontdb::Family::SansSerif,
        usvg::FontFamily::Cursive => fontdb::Family::Cursive,
        usvg::FontFamily::Fantasy => fontdb::Family::Fantasy,
        usvg::FontFamily::Monospace => fontdb::Family::Monospace,
        usvg::FontFamily::Named(name) => fontdb::Family::Name(name),
    }
}

fn stretch(stretch: usvg::FontStretch) -> fontdb::Stretch {
    match stretch {
        usvg::FontStretch::UltraCondensed => fontdb::Stretch::UltraCondensed,
        usvg::FontStretch::ExtraCondensed => fontdb::Stretch::ExtraCondensed,
        usvg::FontStretch::Condensed => fontdb::Stretch::Condensed,
        usvg::FontStretch::SemiCondensed => fontdb::Stretch::SemiCondensed,
        usvg::FontStretch::Normal => fontdb::Stretch::Normal,
        usvg::FontStretch::SemiExpanded => fontdb::Stretch::SemiExpanded,
        usvg::FontStretch::Expanded => fontdb::Stretch::Expanded,
        usvg::FontStretch::ExtraExpanded => fontdb::Stretch::ExtraExpanded,
        usvg::FontStretch::UltraExpanded => fontdb::Stretch::UltraExpanded,
    }
}

fn style(style: usvg::FontStyle) -> fontdb::Style {
    match style {
        usvg::FontStyle::Normal => fontdb::Style::Normal,
        usvg::FontStyle::Italic => fontdb::Style::Italic,
        usvg::FontStyle::Oblique => fontdb::Style::Oblique,
    }
}

/// An affine map `[a, b, c, d, e, f]`: (x, y) to (a x + c y + e,
/// b x + d y + f).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine(pub [f64; 6]);

impl Affine {
    pub const IDENTITY: Affine = Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    fn of(t: Transform) -> Affine {
        Affine([
            f64::from(t.sx),
            f64::from(t.ky),
            f64::from(t.kx),
            f64::from(t.sy),
            f64::from(t.tx),
            f64::from(t.ty),
        ])
    }

    /// This map after `inner`: `inner` first.
    pub fn then_inner(self, inner: Affine) -> Affine {
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = inner.0;
        Affine([
            a * a2 + c * b2,
            b * a2 + d * b2,
            a * c2 + c * d2,
            b * c2 + d * d2,
            a * e2 + c * f2 + e,
            b * e2 + d * f2 + f,
        ])
    }

    pub fn apply(self, p: P2) -> P2 {
        let [a, b, c, d, e, f] = self.0;
        [a * p[0] + c * p[1] + e, b * p[0] + d * p[1] + f]
    }

    /// How much it scales lengths: the square root of its area's scale.
    pub fn scale(self) -> f64 {
        let [a, b, c, d, ..] = self.0;
        (a * d - b * c).abs().sqrt()
    }
}

#[derive(Default)]
struct Reader {
    shapes: Vec<Shape>,
    left_out: LeftOut,
}

impl Reader {
    /// A group's children under the map `t` to the image's coordinates,
    /// the opacity of the groups around them and their clip paths.
    fn group(&mut self, group: &usvg::Group, t: Affine, opacity: f64, clips: &[Clip]) {
        for child in group.children() {
            match child {
                usvg::Node::Group(g) => {
                    let t = t.then_inner(Affine::of(g.transform()));
                    let mut clips = clips.to_vec();
                    if let Some(clip) = g.clip_path() {
                        push_clip(clip, t, &mut clips);
                    }
                    self.group(g, t, opacity * f64::from(g.opacity().get()), &clips);
                }
                usvg::Node::Path(path) => {
                    if path.is_visible() {
                        self.path(path, t, opacity, clips);
                    }
                }
                usvg::Node::Image(image) => match image.kind() {
                    usvg::ImageKind::SVG(tree) => {
                        if image.is_visible() {
                            self.group(tree.root(), t, opacity, clips);
                        }
                    }
                    _ => self.left_out.pictures = true,
                },
                // Text as the outlines of its glyphs, laid out by usvg in
                // the fonts `read` gave it: paths with the text's paint, in
                // the coordinates of the text's parent (as usvg's renderer
                // draws them).
                usvg::Node::Text(text) => {
                    let glyphs = text.flattened();
                    let t = t.then_inner(Affine::of(glyphs.transform()));
                    self.group(
                        glyphs,
                        t,
                        opacity * f64::from(glyphs.opacity().get()),
                        clips,
                    );
                }
            }
        }
    }

    fn path(&mut self, path: &usvg::Path, t: Affine, opacity: f64, clips: &[Clip]) {
        let subpaths = subpaths(path.data(), t);
        if subpaths.is_empty() {
            return;
        }
        let fill = path.fill().and_then(|f| {
            let colour = paint_colour(f.paint(), opacity * f64::from(f.opacity().get()))?;
            Some(Fill {
                colour,
                even_odd: f.rule() == usvg::FillRule::EvenOdd,
            })
        });
        let scale = t.scale();
        let stroke = path.stroke().and_then(|s| {
            let colour = paint_colour(s.paint(), opacity * f64::from(s.opacity().get()))?;
            Some(Stroke {
                colour,
                width: f64::from(s.width().get()) * scale,
                cap: match s.linecap() {
                    usvg::LineCap::Butt => Cap::Butt,
                    usvg::LineCap::Round => Cap::Round,
                    usvg::LineCap::Square => Cap::Square,
                },
                join: match s.linejoin() {
                    usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => Join::Miter,
                    usvg::LineJoin::Round => Join::Round,
                    usvg::LineJoin::Bevel => Join::Bevel,
                },
                miter_limit: f64::from(s.miterlimit().get()),
                dashes: s
                    .dasharray()
                    .map(|d| d.iter().map(|v| f64::from(*v) * scale).collect())
                    .unwrap_or_default(),
                dash_offset: f64::from(s.dashoffset()) * scale,
            })
        });
        if fill.is_none() && stroke.is_none() {
            return;
        }
        self.shapes.push(Shape {
            subpaths,
            fill,
            stroke,
            stroke_first: path.paint_order() == usvg::PaintOrder::StrokeAndFill,
            clips: clips.to_vec(),
        });
    }
}

/// A clip path (and the clip paths it is clipped by) on the clips of an
/// element whose coordinates `t` maps to the image's.
fn push_clip(clip: &usvg::ClipPath, t: Affine, clips: &mut Vec<Clip>) {
    if let Some(inner) = clip.clip_path() {
        push_clip(inner, t, clips);
    }
    let t = t.then_inner(Affine::of(clip.transform()));
    let mut subpaths_of_clip = Vec::new();
    let mut even_odd = false;
    clip_children(clip.root(), t, &mut subpaths_of_clip, &mut even_odd);
    clips.push(Clip {
        subpaths: subpaths_of_clip,
        even_odd,
    });
}

/// The paths of a clip path's group, under `t`; the rule of its last path.
fn clip_children(group: &usvg::Group, t: Affine, out: &mut Vec<Subpath>, even_odd: &mut bool) {
    for child in group.children() {
        match child {
            usvg::Node::Group(g) => {
                clip_children(g, t.then_inner(Affine::of(g.transform())), out, even_odd);
            }
            usvg::Node::Path(path) if path.is_visible() => {
                out.extend(subpaths(path.data(), t));
                if let Some(fill) = path.fill() {
                    *even_odd = fill.rule() == usvg::FillRule::EvenOdd;
                }
            }
            // Text in a clip path clips by its glyphs' outlines.
            usvg::Node::Text(text) => {
                let glyphs = text.flattened();
                let t = t.then_inner(Affine::of(glyphs.transform()));
                clip_children(glyphs, t, out, even_odd);
            }
            _ => {}
        }
    }
}

/// A paint as one solid colour with this opacity; none when it paints
/// nothing.
fn paint_colour(paint: &usvg::Paint, opacity: f64) -> Option<Colour> {
    let (rgb, alpha) = match paint {
        usvg::Paint::Color(c) => (rgb(*c), 1.0),
        usvg::Paint::LinearGradient(g) => mean_of_stops(g.stops()),
        usvg::Paint::RadialGradient(g) => mean_of_stops(g.stops()),
        usvg::Paint::Pattern(p) => (first_colour(p.root()).unwrap_or([0.5; 3]), 1.0),
    };
    let alpha = (alpha * opacity).clamp(0.0, 1.0);
    (alpha > 0.0).then_some(Colour { rgb, alpha })
}

fn rgb(c: usvg::Color) -> [f64; 3] {
    [
        f64::from(c.red) / 255.0,
        f64::from(c.green) / 255.0,
        f64::from(c.blue) / 255.0,
    ]
}

/// A gradient's mean colour and opacity along it: its stops joined by
/// straight lines, the first and last held to its ends.
fn mean_of_stops(stops: &[usvg::Stop]) -> ([f64; 3], f64) {
    let value = |s: &usvg::Stop| {
        let [r, g, b] = rgb(s.color());
        [r, g, b, f64::from(s.opacity().get())]
    };
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
        return ([0.0; 3], 0.0);
    };
    let at = |s: &usvg::Stop| f64::from(s.offset().get());
    let mut sum = [0.0; 4];
    let mut add = |v: [f64; 4], w: f64| {
        for (s, x) in sum.iter_mut().zip(v) {
            *s += x * w;
        }
    };
    add(value(first), at(first));
    for pair in stops.windows(2) {
        let (a, b) = (value(&pair[0]), value(&pair[1]));
        let w = (at(&pair[1]) - at(&pair[0])).max(0.0);
        add(
            [
                (a[0] + b[0]) / 2.0,
                (a[1] + b[1]) / 2.0,
                (a[2] + b[2]) / 2.0,
                (a[3] + b[3]) / 2.0,
            ],
            w,
        );
    }
    add(value(last), 1.0 - at(last));
    ([sum[0], sum[1], sum[2]], sum[3])
}

/// The solid colour of the first path a group paints.
fn first_colour(group: &usvg::Group) -> Option<[f64; 3]> {
    group.children().iter().find_map(|child| match child {
        usvg::Node::Group(g) => first_colour(g),
        usvg::Node::Path(p) => {
            let paint = p
                .fill()
                .map(usvg::Fill::paint)
                .or_else(|| p.stroke().map(usvg::Stroke::paint))?;
            match paint {
                usvg::Paint::Color(c) => Some(rgb(*c)),
                _ => None,
            }
        }
        _ => None,
    })
}

/// A path's data as subpaths under `t`, its quadratic pieces as cubic
/// ones.
fn subpaths(path: &usvg::tiny_skia_path::Path, t: Affine) -> Vec<Subpath> {
    let mut out: Vec<Subpath> = Vec::new();
    let mut current: Option<Subpath> = None;
    let mut last = [0.0; 2];
    let p = |q: usvg::tiny_skia_path::Point| t.apply([f64::from(q.x), f64::from(q.y)]);
    for segment in path.segments() {
        // A piece after a close starts again at the closed one's start.
        if !matches!(segment, PathSegment::MoveTo(_)) && current.as_ref().is_none_or(|s| s.closed) {
            let start = current.as_ref().map_or(last, |s| s.start);
            if let Some(done) = current.take() {
                out.push(done);
            }
            current = Some(Subpath {
                start,
                segments: Vec::new(),
                closed: false,
            });
            last = start;
        }
        match segment {
            PathSegment::MoveTo(to) => {
                if let Some(done) = current.take() {
                    out.push(done);
                }
                last = p(to);
                current = Some(Subpath {
                    start: last,
                    segments: Vec::new(),
                    closed: false,
                });
            }
            PathSegment::LineTo(to) => {
                last = p(to);
                if let Some(s) = current.as_mut() {
                    s.segments.push(Segment::Line(last));
                }
            }
            PathSegment::QuadTo(control, to) => {
                let (q, end) = (p(control), p(to));
                let c1 = [
                    last[0] + 2.0 / 3.0 * (q[0] - last[0]),
                    last[1] + 2.0 / 3.0 * (q[1] - last[1]),
                ];
                let c2 = [
                    end[0] + 2.0 / 3.0 * (q[0] - end[0]),
                    end[1] + 2.0 / 3.0 * (q[1] - end[1]),
                ];
                last = end;
                if let Some(s) = current.as_mut() {
                    s.segments.push(Segment::Cubic(c1, c2, end));
                }
            }
            PathSegment::CubicTo(c1, c2, to) => {
                last = p(to);
                if let Some(s) = current.as_mut() {
                    s.segments.push(Segment::Cubic(p(c1), p(c2), last));
                }
            }
            PathSegment::Close => {
                if let Some(s) = current.as_mut() {
                    s.closed = true;
                    last = s.start;
                }
            }
        }
    }
    if let Some(done) = current.take() {
        out.push(done);
    }
    out.retain(|s| !s.segments.is_empty());
    out
}

impl Subpath {
    /// The subpath under a map.
    pub fn mapped(&self, t: Affine) -> Subpath {
        Subpath {
            start: t.apply(self.start),
            segments: self
                .segments
                .iter()
                .map(|s| match *s {
                    Segment::Line(p) => Segment::Line(t.apply(p)),
                    Segment::Cubic(a, b, c) => Segment::Cubic(t.apply(a), t.apply(b), t.apply(c)),
                })
                .collect(),
            closed: self.closed,
        }
    }

    /// Its points with its curves as lines leaving them by at most
    /// `deflection`; a closed one without its start again at the end.
    pub fn flattened(&self, deflection: f64) -> Vec<P2> {
        let mut out = vec![self.start];
        let mut last = self.start;
        for segment in &self.segments {
            match *segment {
                Segment::Line(p) => out.push(p),
                Segment::Cubic(a, b, c) => flatten_cubic([last, a, b, c], deflection, 16, &mut out),
            }
            last = match *segment {
                Segment::Line(p) | Segment::Cubic(_, _, p) => p,
            };
        }
        if self.closed && out.len() > 1 && out.last() == Some(&self.start) {
            out.pop();
        }
        out
    }
}

/// A cubic Bézier's points after its start: halved until its control points
/// lie within `deflection` of its chord (the curve lies in their hull).
fn flatten_cubic(c: [P2; 4], deflection: f64, depth: u32, out: &mut Vec<P2>) {
    let off = |p: P2| {
        let (dx, dy) = (c[3][0] - c[0][0], c[3][1] - c[0][1]);
        let length = dx.hypot(dy);
        if length < 1e-12 {
            (p[0] - c[0][0]).hypot(p[1] - c[0][1])
        } else {
            ((p[0] - c[0][0]) * dy - (p[1] - c[0][1]) * dx).abs() / length
        }
    };
    if depth == 0 || (off(c[1]) <= deflection && off(c[2]) <= deflection) {
        out.push(c[3]);
        return;
    }
    let mid = |p: P2, q: P2| [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0];
    let (ab, bc, cd) = (mid(c[0], c[1]), mid(c[1], c[2]), mid(c[2], c[3]));
    let (abc, bcd) = (mid(ab, bc), mid(bc, cd));
    let m = mid(abc, bcd);
    flatten_cubic([c[0], ab, abc, m], deflection, depth - 1, out);
    flatten_cubic([m, bcd, cd, c[3]], deflection, depth - 1, out);
}

impl SvgImage {
    /// The map from the image's coordinates to the sheet's (mm, y up) that
    /// puts it in an image item's box as the view does: the box has the
    /// proportions of the file's `viewBox` (`template::fitted_box`), and the
    /// part of the image of those proportions in its middle (where its
    /// `viewBox` lies by SVG's default `preserveAspectRatio`) fills the box.
    pub fn placement(&self, at: P2, size: P2) -> Affine {
        let [w, h] = self.size;
        let aspect = if size[1] > 0.0 {
            size[0] / size[1]
        } else {
            0.0
        };
        let known = aspect.is_finite() && aspect > 0.0 && w > 0.0 && h > 0.0;
        let (x0, y0, inner_w) = if !known {
            (0.0, 0.0, w)
        } else if w / h > aspect {
            ((w - h * aspect) / 2.0, 0.0, h * aspect)
        } else {
            (0.0, (h - w / aspect) / 2.0, w)
        };
        let s = if inner_w > 0.0 {
            size[0] / inner_w
        } else {
            1.0
        };
        Affine([s, 0.0, 0.0, -s, at[0] - x0 * s, at[1] + size[1] + y0 * s])
    }

    /// Its shapes in an image item's box on the sheet ([`Self::placement`]),
    /// their strokes' widths and dashes scaled with them.
    pub fn placed(&self, at: P2, size: P2) -> Vec<Shape> {
        let t = self.placement(at, size);
        let scale = t.scale();
        self.shapes
            .iter()
            .map(|shape| Shape {
                subpaths: shape.subpaths.iter().map(|s| s.mapped(t)).collect(),
                fill: shape.fill,
                stroke: shape.stroke.as_ref().map(|s| Stroke {
                    width: s.width * scale,
                    dashes: s.dashes.iter().map(|d| d * scale).collect(),
                    dash_offset: s.dash_offset * scale,
                    ..s.clone()
                }),
                stroke_first: shape.stroke_first,
                clips: shape
                    .clips
                    .iter()
                    .map(|c| Clip {
                        subpaths: c.subpaths.iter().map(|s| s.mapped(t)).collect(),
                        even_odd: c.even_odd,
                    })
                    .collect(),
            })
            .collect()
    }
}

impl Colour {
    /// The colour as it looks on white paper.
    pub fn on_white(self) -> [f64; 3] {
        self.rgb.map(|c| c * self.alpha + (1.0 - self.alpha))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: P2, b: P2) -> bool {
        (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4
    }

    #[test]
    fn a_rect_and_a_circle_become_closed_subpaths_with_their_paint() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50">
            <rect x="10" y="5" width="20" height="10" fill="#ff0000"/>
            <circle cx="70" cy="25" r="10" fill="none" stroke="blue" stroke-width="2"/>
        </svg>"##;
        let image = read(svg).unwrap();
        assert_eq!(image.size, [100.0, 50.0]);
        assert_eq!(image.left_out, LeftOut::default());
        assert_eq!(image.shapes.len(), 2);
        let rect = &image.shapes[0];
        assert_eq!(
            rect.fill,
            Some(Fill {
                colour: Colour {
                    rgb: [1.0, 0.0, 0.0],
                    alpha: 1.0
                },
                even_odd: false
            })
        );
        assert!(rect.stroke.is_none());
        assert_eq!(rect.subpaths.len(), 1);
        let corners = rect.subpaths[0].flattened(DEFLECTION);
        assert!(rect.subpaths[0].closed);
        assert_eq!(corners.len(), 4, "{corners:?}");
        for corner in [[10.0, 5.0], [30.0, 5.0], [30.0, 15.0], [10.0, 15.0]] {
            assert!(corners.iter().any(|c| close(*c, corner)), "{corners:?}");
        }
        let circle = &image.shapes[1];
        assert!(circle.fill.is_none());
        let stroke = circle.stroke.as_ref().unwrap();
        assert_eq!(stroke.colour.rgb, [0.0, 0.0, 1.0]);
        assert!((stroke.width - 2.0).abs() < 1e-9);
        // Curves, every flattened point on the circle within the
        // deflection.
        assert!(
            circle.subpaths[0]
                .segments
                .iter()
                .any(|s| matches!(s, Segment::Cubic(..)))
        );
        let points = circle.subpaths[0].flattened(DEFLECTION);
        assert!(points.len() > 16, "{}", points.len());
        for p in points {
            let r = (p[0] - 70.0).hypot(p[1] - 25.0);
            assert!((r - 10.0).abs() < 2.0 * DEFLECTION, "{p:?} at {r}");
        }
    }

    #[test]
    fn transforms_view_boxes_and_group_opacity_are_applied() {
        // A viewBox twice the size: the user units halved into the
        // image's 100 x 50; a group moved and half transparent.
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50" viewBox="0 0 200 100">
            <g transform="translate(20 10)" opacity="0.5">
              <path d="M0 0 L40 0 L40 20 Z" fill="black" stroke="black" stroke-width="4" stroke-dasharray="6 2"/>
            </g>
        </svg>"#;
        let image = read(svg).unwrap();
        let shape = &image.shapes[0];
        let path = &shape.subpaths[0];
        assert!(close(path.start, [10.0, 5.0]), "{path:?}");
        assert_eq!(
            path.segments,
            vec![Segment::Line([30.0, 5.0]), Segment::Line([30.0, 15.0])]
        );
        assert!(path.closed);
        assert!((shape.fill.unwrap().colour.alpha - 0.5).abs() < 1e-6);
        let stroke = shape.stroke.as_ref().unwrap();
        assert!((stroke.width - 2.0).abs() < 1e-6);
        assert_eq!(stroke.dashes, vec![3.0, 1.0]);
        assert!((stroke.colour.alpha - 0.5).abs() < 1e-6);
        // Half transparent black looks grey on white paper.
        assert_eq!(shape.fill.unwrap().colour.on_white(), [0.5, 0.5, 0.5]);
    }

    #[test]
    fn gradients_are_their_mean_colour_and_quadratics_become_cubics() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <defs><linearGradient id="g"><stop offset="0" stop-color="#000000"/>
              <stop offset="1" stop-color="#ffffff"/></linearGradient></defs>
            <path d="M0 0 Q5 10 10 0 Z" fill="url(#g)"/>
        </svg>"##;
        let image = read(svg).unwrap();
        let fill = image.shapes[0].fill.unwrap();
        for c in fill.colour.rgb {
            assert!((c - 0.5).abs() < 1e-6, "{fill:?}");
        }
        let segments = &image.shapes[0].subpaths[0].segments;
        // The quadratic's control (5, 10) at two thirds: (10/3, 20/3) and
        // (20/3, 20/3).
        let Segment::Cubic(c1, c2, end) = segments[0] else {
            panic!("{segments:?}");
        };
        assert!(close(c1, [10.0 / 3.0, 20.0 / 3.0]), "{c1:?}");
        assert!(close(c2, [20.0 / 3.0, 20.0 / 3.0]), "{c2:?}");
        assert!(close(end, [10.0, 0.0]));
    }

    #[test]
    fn clip_paths_text_and_pictures_are_known() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <clipPath id="c"><rect width="5" height="5"/></clipPath>
            <rect width="10" height="10" clip-path="url(#c)"/>
            <text x="1" y="9">Logo</text>
            <image width="2" height="2" href="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="/>
        </svg>"#;
        let image = read(svg).unwrap();
        // The text, which names no font, in the bundled one: drawn, not
        // warned of; the picture left out.
        assert_eq!(
            image.left_out,
            LeftOut {
                fonts: Vec::new(),
                characters: false,
                pictures: true
            }
        );
        assert_eq!(image.shapes.len(), 2, "the rectangle and the text");
        let clipped = &image.shapes[0];
        assert_eq!(clipped.clips.len(), 1);
        let clip = clipped.clips[0].subpaths[0].flattened(DEFLECTION);
        assert!(clip.iter().any(|p| close(*p, [5.0, 5.0])), "{clip:?}");
    }

    /// The points of an image's shapes, their curves flattened.
    fn points(image: &SvgImage) -> Vec<P2> {
        image
            .shapes
            .iter()
            .flat_map(|s| &s.subpaths)
            .flat_map(|s| s.flattened(DEFLECTION))
            .collect()
    }

    #[test]
    fn text_is_the_outlines_of_its_glyphs_in_the_bundled_font() {
        // mitcad#1637: a line of 20-unit text from (10, 40), black, in the
        // bundled font named.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="60">
            <text x="10" y="40" font-family="Droid Sans" font-size="20" fill="#000000">MitCad</text>
        </svg>"##;
        let image = read(svg).unwrap();
        assert_eq!(image.left_out, LeftOut::default());
        assert!(!image.shapes.is_empty());
        for shape in &image.shapes {
            let fill = shape.fill.unwrap();
            assert_eq!(fill.colour.rgb, [0.0, 0.0, 0.0]);
            assert!(shape.stroke.is_none());
        }
        // The glyphs' outlines: closed, many of them (M, i's two parts, t,
        // C, a's two loops, d's two), their curves kept as curves.
        let subpaths: Vec<&Subpath> = image.shapes.iter().flat_map(|s| &s.subpaths).collect();
        assert!(subpaths.len() >= 8, "{} outlines", subpaths.len());
        assert!(subpaths.iter().all(|s| s.closed));
        assert!(
            subpaths
                .iter()
                .flat_map(|s| &s.segments)
                .any(|s| matches!(s, Segment::Cubic(..)))
        );
        // On the baseline at 40, the capitals' tops about 14 units above
        // it, from x = 10 on, six letters of at most a font size each.
        let points = points(&image);
        let (mut x_min, mut x_max, mut y_min, mut y_max) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for p in &points {
            x_min = x_min.min(p[0]);
            x_max = x_max.max(p[0]);
            y_min = y_min.min(p[1]);
            y_max = y_max.max(p[1]);
        }
        assert!((9.0..14.0).contains(&x_min), "{x_min}");
        assert!((40.0..130.0).contains(&x_max), "{x_max}");
        assert!((22.0..28.0).contains(&y_min), "{y_min}");
        assert!((39.5..40.5).contains(&y_max), "{y_max}");
    }

    #[test]
    fn a_font_this_computer_lacks_is_drawn_in_the_bundled_font_and_noted() {
        let svg = |family: &str| {
            format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="60">
                <text x="10" y="40" font-family="{family}" font-size="20">MitCad</text>
                </svg>"#
            )
        };
        let bundled = read(svg("Droid Sans").as_bytes()).unwrap();
        // A font no computer has: the bundled font's outlines, the font
        // named.
        let missing = read(svg("No Such Font Of MitCad Tests").as_bytes()).unwrap();
        assert_eq!(missing.left_out.fonts, vec!["No Such Font Of MitCad Tests"]);
        assert!(!missing.left_out.characters);
        assert_eq!(points(&missing), points(&bundled));
        // A generic family is the bundled font, and a list's later family
        // stands in for a missing first one: no note.
        for family in [
            "sans-serif",
            "serif",
            "monospace",
            "No Such Font Of MitCad Tests, sans-serif",
        ] {
            let generic = read(svg(family).as_bytes()).unwrap();
            assert_eq!(generic.left_out, LeftOut::default(), "{family}");
            assert_eq!(points(&generic), points(&bundled), "{family}");
        }
        // Bold: the bundled bold, wider.
        let bold = read(
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="60">
                <text x="10" y="40" font-family="Droid Sans" font-size="20" font-weight="bold">MitCad</text>
                </svg>"#,
        )
        .unwrap();
        let right = |image: &SvgImage| points(image).iter().map(|p| p[0]).fold(f64::MIN, f64::max);
        assert!(right(&bold) > right(&bundled) + 1.0);
    }

    #[test]
    fn a_character_no_font_has_is_noted() {
        // A database of the bundled regular font alone, which the text
        // already used: no other font for a character it lacks.
        let mut db = fontdb::Database::new();
        db.load_font_data(DROID_SANS.to_vec());
        let id = db.faces().next().unwrap().id;
        let mut db = Arc::new(db);
        let not_found = Mutex::new(BTreeSet::new());
        let characters_missing = AtomicBool::new(false);
        let resolver = font_resolver(&not_found, &characters_missing);
        assert_eq!((resolver.select_fallback)('\u{378}', &[id], &mut db), None);
        assert!(characters_missing.load(Ordering::Relaxed));
        assert!(not_found.lock().unwrap().is_empty());
    }

    #[test]
    fn only_files_that_may_have_text_read_the_fonts() {
        assert!(!may_have_text(
            br#"<svg xmlns="http://www.w3.org/2000/svg"><rect width="1" height="1"/></svg>"#
        ));
        assert!(may_have_text(
            br#"<svg xmlns="http://www.w3.org/2000/svg"><text>A</text></svg>"#
        ));
        assert!(may_have_text(
            br#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><image xlink:href="data:image/svg+xml;base64,AAAA"/></svg>"#
        ));
        // The bundled font is in the fonts, its regular and its bold.
        let db = fonts();
        let bundled: Vec<u16> = db
            .faces()
            .filter(|f| f.families.iter().any(|(name, _)| name == BUNDLED_FONT))
            .map(|f| f.weight.0)
            .collect();
        assert!(
            bundled.contains(&400) && bundled.contains(&700),
            "{bundled:?}"
        );
    }

    #[test]
    fn files_of_the_disk_are_not_read_and_broken_files_are_refused() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <image width="2" height="2" href="logo.svg"/>
        </svg>"#;
        let image = read(svg).unwrap();
        assert!(image.shapes.is_empty());
        assert!(read(b"<svg xmlns=\"http://www.w3.org/2000/svg\"><rect").is_err());
        assert!(
            read(b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"0\" height=\"0\"/>").is_err()
        );
    }

    #[test]
    fn an_image_fills_its_box_as_the_view_draws_it() {
        let image = SvgImage {
            size: [100.0, 50.0],
            shapes: Vec::new(),
            left_out: LeftOut::default(),
        };
        // Its own proportions: the whole image in the box, y up.
        let t = image.placement([10.0, 20.0], [30.0, 15.0]);
        assert!(close(t.apply([0.0, 0.0]), [10.0, 35.0]));
        assert!(close(t.apply([100.0, 50.0]), [40.0, 20.0]));
        assert!((t.scale() - 0.3).abs() < 1e-12);
        // A box of a square viewBox in a 100 x 50 image: its middle square.
        let t = image.placement([0.0, 0.0], [10.0, 10.0]);
        assert!(close(t.apply([25.0, 0.0]), [0.0, 10.0]));
        assert!(close(t.apply([75.0, 50.0]), [10.0, 0.0]));
    }

    #[test]
    fn placed_shapes_scale_their_strokes() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
            <line x1="0" y1="5" x2="20" y2="5" stroke="black" stroke-width="2" stroke-dasharray="4"/>
        </svg>"#;
        let image = read(svg).unwrap();
        let placed = image.placed([0.0, 0.0], [40.0, 20.0]);
        let stroke = placed[0].stroke.as_ref().unwrap();
        assert!((stroke.width - 4.0).abs() < 1e-9);
        assert_eq!(stroke.dashes, vec![8.0, 8.0]);
        let points = placed[0].subpaths[0].flattened(DEFLECTION);
        assert!(close(points[0], [0.0, 10.0]));
        assert!(close(points[1], [40.0, 10.0]));
    }

    #[test]
    fn a_cubic_is_flattened_within_the_deflection() {
        let s = Subpath {
            start: [0.0, 0.0],
            segments: vec![Segment::Cubic([0.0, 10.0], [10.0, 10.0], [10.0, 0.0])],
            closed: false,
        };
        let points = s.flattened(0.01);
        assert!(points.len() > 8);
        assert_eq!(points.first(), Some(&[0.0, 0.0]));
        assert_eq!(points.last(), Some(&[10.0, 0.0]));
        // Its top at t = 1/2: (5, 7.5).
        let top = points.iter().map(|p| p[1]).fold(0.0, f64::max);
        assert!(top <= 7.5 + 1e-9 && top > 7.5 - 0.01, "{top}");
    }
}
