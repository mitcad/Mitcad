// SPDX-License-Identifier: MIT
//! What a sheet holds: lines, arcs, polylines, filled arrowheads and texts
//! in millimetres of paper from the sheet's lower left corner, in groups
//! (a view, a dimension, a row of the hole table) that the SVG writer keeps
//! as `<g class="…">`. The writers turn the pens into line widths and
//! dashes after ISO 128 and the texts into Helvetica (ISO 3098 is a sans
//! serif: the standard fonts of PDF readers and browsers stand in for it).

use super::P2;

/// The kinds of line (ISO 128-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pen {
    /// Visible edges: continuous wide.
    Thick,
    /// Lines drawn by hand to stand out: continuous extra wide, twice as
    /// wide as the wide line (mitcad#1402).
    ExtraThick,
    /// Dimension, extension and leader lines, tangent edges, a thread's
    /// roots, the title block's cells: continuous narrow.
    Thin,
    /// Hidden edges: dashed narrow.
    Hidden,
    /// Hatching of cut faces.
    Hatch,
    /// A section's cutting plane: long dash dotted narrow.
    Chain,
    /// The sheet's frame.
    Frame,
}

impl Pen {
    /// Every pen, in the order of the table the drawing query gives.
    pub const ALL: [Pen; 7] = [
        Self::Thick,
        Self::ExtraThick,
        Self::Thin,
        Self::Hidden,
        Self::Hatch,
        Self::Chain,
        Self::Frame,
    ];

    /// The line width, mm: ISO 128's 0.35 series (mitcad#1112), wide
    /// lines twice as wide as narrow ones and extra wide ones twice as wide
    /// as wide ones (narrow, wide, extra wide at 1:2:4; the extra wide line
    /// mitcad#1402). The one table of widths: the PDF and SVG writers take
    /// them from here and the application's sheet from the drawing query
    /// (`pens`).
    pub fn width(self) -> f64 {
        match self {
            Self::Thick => 0.35,
            Self::ExtraThick => 0.7,
            // ISO 128 draws hidden outlines with the narrow line.
            Self::Thin | Self::Chain | Self::Hidden => 0.18,
            Self::Hatch => 0.13,
            Self::Frame => 0.5,
        }
    }

    /// Its name in the drawing query's items.
    pub fn name(self) -> &'static str {
        match self {
            Self::Thick => "thick",
            Self::ExtraThick => "extra_thick",
            Self::Thin => "thin",
            Self::Hidden => "hidden",
            Self::Hatch => "hatch",
            Self::Chain => "chain",
            Self::Frame => "frame",
        }
    }

    /// Dash and gap lengths, mm; empty for a continuous line.
    pub fn dashes(self) -> &'static [f64] {
        match self {
            Self::Hidden => &[3.0, 1.0],
            Self::Chain => &[12.0, 1.5, 0.5, 1.5],
            _ => &[],
        }
    }

    /// The layer of a DXF file.
    pub fn layer(self) -> &'static str {
        match self {
            Self::Thick => "VISIBLE",
            Self::ExtraThick => "EXTRA_THICK",
            Self::Thin => "THIN",
            Self::Hidden => "HIDDEN",
            Self::Hatch => "HATCH",
            Self::Chain => "SECTION_LINE",
            Self::Frame => "FRAME",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Line {
        pen: Pen,
        a: P2,
        b: P2,
    },
    /// Counter-clockwise from `start` to `end`, radians.
    Arc {
        pen: Pen,
        center: P2,
        radius: f64,
        start: f64,
        end: f64,
    },
    Polyline {
        pen: Pen,
        points: Vec<P2>,
        closed: bool,
    },
    /// A filled polygon (an arrowhead).
    Fill {
        points: Vec<P2>,
    },
    /// A polygon filled grey (0 black, 1 white) without an outline: a
    /// shaded view's face (mitcad#707).
    Shade {
        points: Vec<P2>,
        grey: f64,
    },
    /// `height` is the height of capitals (ISO 3098's h), `angle` the
    /// baseline's direction (radians), `at` the anchor on the baseline.
    Text {
        at: P2,
        height: f64,
        angle: f64,
        anchor: Anchor,
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// `view`, `dimension`, `callout`, `hole-tag`, `hole-table`,
    /// `hole-row`, `section-line`, `title-block` or `frame`.
    pub class: &'static str,
    pub id: Option<String>,
    pub items: Vec<Item>,
}

impl Group {
    pub fn new(class: &'static str) -> Self {
        Self {
            class,
            id: None,
            items: Vec::new(),
        }
    }

    pub fn line(&mut self, pen: Pen, a: P2, b: P2) {
        self.items.push(Item::Line { pen, a, b });
    }

    pub fn text(
        &mut self,
        at: P2,
        height: f64,
        angle: f64,
        anchor: Anchor,
        text: impl Into<String>,
    ) {
        self.items.push(Item::Text {
            at,
            height,
            angle,
            anchor,
            text: text.into(),
        });
    }

    pub fn rect(&mut self, pen: Pen, min: P2, max: P2) {
        self.items.push(Item::Polyline {
            pen,
            points: vec![min, [max[0], min[1]], max, [min[0], max[1]]],
            closed: true,
        });
    }
}

/// One sheet of a drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    /// mm.
    pub width: f64,
    pub height: f64,
    pub title: String,
    pub groups: Vec<Group>,
}

impl Sheet {
    pub fn groups_of<'a>(&'a self, class: &'a str) -> impl Iterator<Item = &'a Group> + 'a {
        self.groups.iter().filter(move |g| g.class == class)
    }
}

/// Helvetica's cap height per em.
pub const CAP_HEIGHT: f64 = 0.718;

/// The font size (em, mm) of text whose capitals are `height` tall.
pub fn font_size(height: f64) -> f64 {
    height / CAP_HEIGHT
}

/// Helvetica's advance widths (per 1000 em) of the printable ASCII
/// characters from the space on.
const WIDTHS: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278,
    278, // ' '..'/'
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // '0'..'9'
    278, 278, 584, 584, 584, 556, 1015, // ':'..'@'
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, // 'A'..'M'
    722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, // 'N'..'Z'
    278, 278, 278, 469, 556, 333, // '['..'`'
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, // 'a'..'m'
    556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, // 'n'..'z'
    334, 260, 334, 584, // '{'..'~'
];

/// The advance width of a character, per 1000 em.
fn char_width(c: char) -> u16 {
    match c {
        ' '..='~' => WIDTHS[c as usize - 32],
        'Ø' | 'ø' => 778,
        '°' => 400,
        '±' | '×' => 584,
        _ => 556,
    }
}

/// How wide a text is in the drawing's font, mm.
pub fn text_width(text: &str, height: f64) -> f64 {
    let em: f64 = text.chars().map(|c| f64::from(char_width(c))).sum::<f64>() / 1000.0;
    em * font_size(height)
}
