// SPDX-License-Identifier: MIT
#pragma once

// Annotations drawn in the view (mitcad#702, grown from the measurement
// objects of mitcad#649; app/COMMANDS.md "Annotations"): the shown entries
// of the model's `annotations` query, laid out in a picture's pixels
// through a projection of the design's points, and painted with QPainter.
//
// An entry with the model's `drawing` (every annotation now) is drawn in
// its plane: its extension and dimension lines, an angle's arc, the arrows
// and a leader's (a note, a datum label's triangle, a feature control
// frame's) projected from the design, and its text set in its plane
// (foreshortened as the plane turns away, turned round where it would read
// upside down or mirrored, fading as the plane turns edge-on), ISO 129
// style: the value, a tolerance's deviations stacked after it, the own
// text after them; a feature control frame's cells ISO 1101, a datum
// label's letter in its box ISO 5459. An entry without one (the measurement objects'
// first form) keeps the screen-facing layout: the dimension line above its
// ends, the label "<name> = <value>" on a card.
//
// The look of a technical drawing's dimensions (mitcad#894, mitcad#902; ISO
// 129-1, ISO 3098, ISO 1101, ISO 128): the bundled technical lettering
// (measurementFontFamily: an ISO 3098 type B style font, a file loaded at
// run time) of regular weight with no card or outline under it; as high
// in its plane as the model says (`text_height`, mm), but its capitals on
// screen never under a floor or over a ceiling; crisp thin lines on the
// pixel grid with no halo; Auto near-black or a light grey, lines piece by piece and text in one,
// as reads over what lies behind it (MeasurementBackdrop); closed, filled
// arrowheads of 30 degrees, a text height long, in the annotation's plane
// as its text is (a leader's dot on a face, a datum label's triangle
// likewise); extension lines half a text height off the measured point
// and 0.8 text heights past the dimension line;
// a frame's cells two text heights high and a datum label's square
// outlined thinly, not filled. A selected or hovered annotation is the
// same thin drawing in the selection's or the hover's colour: no band, no
// box. An annotation of the colour Auto is drawn in the neutral
// annotation colour of the background tone (measurementAutoColor). The
// view (MeasurementOverlay), its snapshots (OcctViewer::snapshot) and
// Render Image draw them the same way, the pictures in the tone of their
// own background (measurementPictureTone); pixel sizes grow with a
// picture's scale. Plain Qt Gui and OCCT's points: no model, no widgets.

#include <array>
#include <functional>
#include <optional>
#include <utility>
#include <vector>

#include <QColor>
#include <QFont>
#include <QIcon>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QLineF>
#include <QPointF>
#include <QPolygonF>
#include <QRectF>
#include <QString>
#include <QStringList>
#include <QTransform>

#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

class QPainter;

namespace mitcad {

// An annotation as the model measured it at the marker, in the design's
// coordinates (commands.md, "Measurement objects" and "Annotations").
struct MeasurementDisplay {
  QString name;
  // distance, angle, offset, axis_distance, center_distance, radius,
  // diameter; the leaders note, datum, frame, hole_callout
  QString kind;
  QColor color;       // invalid for Auto
  bool autoColor = false; // the model's "auto" (mitcad#894): the background tone's annotation colour
  // A panel's preview of the annotation its Add makes (mitcad#1097): drawn
  // dimmed, never picked.
  bool preview = false;
  QString group;
  QString label; // on one line: "Ø20 H7 (+0.021/0)"; "Distance1 = 40 mm" without the model's label
  // A length's two ends; a radius from the centre to the rim, a diameter
  // across the circle.
  std::vector<gp_Pnt> points;
  // A circle (radius, diameter): its centre, unit normal and radius.
  bool circle = false;
  gp_Pnt center;
  gp_Vec axis;
  double radius = 0.0;
  // An angle: its corner and the unit rays of its sector.
  bool arc = false;
  gp_Pnt corner;
  gp_Vec from;
  gp_Vec to;

  // The model's drawing in the annotation's plane (mitcad#702).
  struct Segment {
    gp_Pnt from;
    gp_Pnt to;
  };
  struct Arc {
    gp_Pnt center;
    double radius = 0.0;
    gp_Vec from; // unit rays it runs between
    gp_Vec to;
    gp_Vec normal;
  };
  struct Arrow {
    gp_Pnt tip;
    gp_Vec direction; // the way it points
  };
  bool planar = false;
  gp_Vec planeX; // the axes text_offset is along (unit)
  gp_Vec planeY;
  gp_Vec planeNormal;
  std::vector<Segment> extensionLines; // from a measured point to the dimension line
  std::vector<Segment> dimensionLines; // and a leader
  std::vector<Arc> arcs;
  std::vector<Arrow> arrows;
  std::optional<gp_Pnt> centerMark;
  gp_Pnt textAt;
  gp_Vec textDirection;
  gp_Vec textUp;
  double lineOffset = 0.0;                 // in use: a length's line, an angle's arc radius
  std::array<double, 2> textOffset{0, 0}; // in use, along planeX and planeY
  // The label in parts (ISO 129): the value with its prefix and own text
  // before it; deviations stacked after it (a fit's in brackets); the own
  // text after them. A note's lines.
  QString main;
  QString upper;
  QString lower;
  bool brackets = false;
  QString after;
  QStringList lines;
  // A feature control frame's cells (ISO 1101): the characteristic's
  // symbol, the tolerance, the datums referred to.
  QStringList frame;
  // Its characteristic (`perpendicularity`) and material condition (M,
  // L), whose symbols are drawn as lines, not taken from the font.
  QString characteristic;
  QString modifier;
  // A datum label's letter (ISO 5459); its leader ends in a filled
  // triangle on the feature.
  QString datum;
  bool datumTerminator = false;
  // A note's or a frame's leader on a face ends in a dot (mitcad#902).
  bool dotTerminator = false;
  // Which end of a leader's text its leader meets, as the model says
  // ("start", "end"; empty: worked out from the arrow).
  QString align;
  // A frame under a hole callout: the callout's lines it stands below.
  int underLines = 0;
  // A frame under a leader's text: that text's lines (a callout's), whose
  // left edge it is set flush with (mitcad#902).
  QStringList overLines;
  // A frame under this leader's text: its cells, characteristic and
  // material condition, so that the lines take the same left edge.
  struct FrameUnder {
    QStringList cells;
    QString characteristic;
    QString modifier;
  };
  std::vector<FrameUnder> framesUnder;
  // A frame under a dimension: that dimension's name; the frame is set
  // just below its dimension line, at its text.
  QString attached;
  // The height of its text in its plane, mm (the model's `text_height`,
  // mitcad#894); 0 when the model gives none (the pixel floor then).
  double textHeight = 0.0;
};

// A value of a kind in words: "40 mm", "90°".
using MeasurementValueText = std::function<QString(const QString& kind, double value)>;

// Millimetres per document length unit (mm, cm, m, in, ft; 1 for others).
double lengthUnitMillimetres(const QString& unit);
// A value of a kind (mm or radians) in words: a length in `unit` (of
// `unitMillimetres` mm), up to three decimals, "40 mm"; an angle in
// degrees, "90°".
QString measurementValueText(const QString& kind, double value, const QString& unit, double unitMillimetres);
// A small swatch of an annotation's colour, a row's icon.
QIcon measurementSwatch(const QColor& color);

// The annotations of the `annotations` query to draw: those shown that the
// model laid out (hidden and broken ones left out), each with its label
// (the model's, else "<name> = <value>").
std::vector<MeasurementDisplay> measurementDisplays(const QJsonArray& measurements,
                                                    const MeasurementValueText& valueText);

// A point of the design in a picture's pixels; none where it cannot be
// drawn (behind a perspective camera).
using MeasurementProjection = std::function<std::optional<QPointF>(const gp_Pnt&)>;

// An annotation laid out in a picture's pixels.
struct MeasurementShape {
  QString name;
  QColor color;
  bool autoColor = false; // drawn in measurementAutoColor of the halo
  bool preview = false;   // a preview (mitcad#1097): painted dimmed
  // The height of its text's capitals on the picture: the model's height
  // in its plane, between the text floor and ceiling (times the scale;
  // mitcad#902); a screen-facing label's the floor. Arrowheads, gaps and
  // frames' cells are sized by it.
  double textPixels = 0.0;
  std::vector<QLineF> lines;                       // extension and dimension lines, rays, leaders
  std::vector<QPolygonF> arcs;                     // an angle's arc
  std::vector<std::pair<QPointF, QPointF>> arrows; // the tip and the direction it points along
  QPointF labelAt;                                 // the label's middle
  QString label;
  bool valid = false;

  // Drawn in its plane (mitcad#702).
  bool planar = false;
  bool note = false; // a note or a hole callout: lines of text
  // A note, datum label, feature control frame or hole callout: its text
  // at the end of its leader.
  bool leader = false;
  // The text's own coordinates (x along it, y down; 0 at its anchor) to
  // the picture's.
  QTransform textTransform;
  // A note's text ends at its anchor (its leader comes from the right)
  // rather than starting there.
  bool rightAligned = false;
  // 1 face on, 0 edge on: everything is drawn this opaque.
  double opacity = 1.0;
  QString main;
  QString upper;
  QString lower;
  bool brackets = false;
  QString after;
  QStringList noteLines; // a note's text
  QStringList frameCells; // a feature control frame's cells
  QString characteristic; // its symbol drawn (the first cell)
  QString modifier;       // M or L: circled after the tolerance
  QString datumLetter;    // a datum label's letter
  // The leader ends in a filled triangle (a datum label), not an arrow.
  bool datumTerminator = false;
  // The leader ends in a dot (a note or a frame on a face, mitcad#902).
  bool dotTerminator = false;
  // The terminators (arrowheads, a datum's triangle, a dot) as laid out
  // in the annotation's plane, filled when painted; the same as `arrows`
  // in order. Empty: drawn screen-facing from `arrows`.
  std::vector<QPolygonF> heads;
  // A frame under a hole callout: below that many of its lines, the
  // callout's lines (its left edge flush with theirs, mitcad#902).
  int underLines = 0;
  QStringList overLines;
  // A leader's text with frames under it: their widths share its left
  // edge.
  std::vector<MeasurementDisplay::FrameUnder> framesUnder;
  // A datum label's square centred on its stem's end (the model's align
  // "middle", mitcad#902).
  bool centred = false;
  // A frame set below a dimension's line: drawn there, not dragged.
  bool attached = false;
  // Where the text is on the picture, for clicks.
  QPolygonF textArea;
  // The lines a drag of the dimension line takes (a length's line, an
  // angle's arc); none for a circle or a note.
  std::vector<QPolygonF> grips;
  // The plane's axes (planeX, planeY) per millimetre on the picture at the
  // text: a drag in pixels into millimetres.
  QPointF xPerMillimetre;
  QPointF yPerMillimetre;
  // Drawn in the selection's colour (a row selected in the Annotations
  // window, a click in the view), the same thin lines and text.
  bool highlighted = false;
  // Drawn in the hover's colour (the mouse over it).
  bool hovered = false;
};

// Lays out an annotation through `project`. With the model's drawing: its
// lines, arcs and arrows projected (arrows of a length too short for them
// put outside, pointing in), its text in its plane. Without: a length's
// dimension line parallel to its ends a little off them (above them on the
// picture) with extension lines from the ends, a radius from the centre to
// the point of the circle farthest from it on the picture, a diameter
// across the circle there, an angle's arc about its corner between its
// rays. `scale` is the picture's size against the view's (gaps, arrows,
// the arc's radius and the text grow with it). Not valid when its points
// cannot be drawn.
MeasurementShape layoutMeasurement(const MeasurementDisplay& measurement, const MeasurementProjection& project,
                                   double scale = 1.0);

// The family of the bundled technical lettering (mitcad#902; an ISO 3098
// type B style font, third_party/fonts/iso-lettering): loaded once from its file
// beside the executable (fonts/), in an installed tree's share/mitcad/fonts
// or a macOS bundle's Resources/fonts; empty when it is not there (the
// annotations then take the UI's font). Logs "Annotation lettering: ...".
QString measurementFontFamily();
// The least and the most a text's capitals are high on screen at `scale`
// (pixels): zoomed out a text stays readable, zoomed in it stays in
// proportion to the reference drawing.
double measurementTextFloor(double scale = 1.0);
double measurementTextCeiling(double scale = 1.0);
// The screen-facing labels' font at `scale`: the technical lettering
// (else `base`'s family), regular, its capitals at the text floor.
QFont measurementFont(const QFont& base, double scale = 1.0);
// A laid out annotation's text font: the technical lettering (else
// `base`'s family), regular, its capitals `shape.textPixels` high.
QFont measurementTextFont(const QFont& base, const MeasurementShape& shape, double scale = 1.0);
// A thin line of `width` pixels on the pixel grid: its ends on whole-pixel
// centres for an odd width (x.5), on pixel corners for an even one, so
// that a horizontal or vertical one is crisp.
QPointF measurementSnapped(const QPointF& point, double width);
QLineF measurementSnapped(const QLineF& line, double width);
// The lines' width at `scale`: a pixel (ISO 128's thin line), whole
// pixels at a larger scale.
double measurementLineWidth(double scale = 1.0);
// The colours of a selected and a hovered annotation.
QColor measurementSelectionColor();
QColor measurementHoverColor();
// The tone of a picture's own background (mitcad#902): the mean of its
// opaque border pixels; invalid when it has none (a transparent one).
QColor measurementPictureTone(const QImage& image);
// Where a laid out annotation's label is drawn, in `font`'s family: on the
// picture for a screen-facing one, in the text's own coordinates for one
// in its plane (at its own height).
QRectF measurementLabelRect(const MeasurementShape& shape, const QFont& font, double scale = 1.0);
// The neutral annotation colour of Auto on a background of the tone of
// `halo` (mitcad#894): near-black on a light one, a light grey on a dark
// one (mitcad#902).
QColor measurementAutoColor(const QColor& halo);
// A closed arrowhead with its tip at `tip`, pointing along `along`: 30
// degrees included (ISO 129-1), 0.8 times `textPixels` long (at least 5
// pixels times `scale`).
QPolygonF measurementArrowHead(const QPointF& tip, const QPointF& along, double textPixels, double scale = 1.0);

// Paints laid out annotations, each in its colour (Auto: of the
// background tone `halo`; a highlighted one in the selection's, a hovered
// one in the hover's): its crisp thin lines, filled arrowheads and its
// text in the technical lettering, with no card, band, box or halo; Auto
// in the ink that reads over the `backdrop` (lines piece by piece, text in one).
// What lies behind the annotations (mitcad#902): a picture of the view or
// the picture painted over, `pixels` of it per painter pixel (a screen's
// device pixel ratio). Auto takes its ink against it.
struct MeasurementBackdrop {
  QImage image;
  double pixels = 1.0;
};
// Auto's ink at a point: near-black over what is light behind it, a light
// grey over what is dark (a line's pieces, a terminator); for an
// annotation's text and frames, one ink by what lies behind most of them
// (mitcad#902; without text, most of its lines); without a backdrop, that
// of the background tone (measurementAutoColor).
QColor measurementAutoInk(const MeasurementBackdrop* backdrop, const QPointF& at, const QColor& halo);
QColor measurementAutoInk(const MeasurementBackdrop* backdrop, const MeasurementShape& shape, const QColor& halo);
void paintMeasurements(QPainter& painter, const std::vector<MeasurementShape>& shapes, const QColor& halo,
                       const QFont& font, double scale = 1.0, const MeasurementBackdrop* backdrop = nullptr);

// A drag of an annotation's text (`line` false) or dimension line by
// `delta` pixels on the picture, in millimetres along the plane's axes
// (planeX, planeY); none where the plane is seen edge on.
std::optional<std::array<double, 2>> measurementDragMillimetres(const MeasurementShape& shape, const QPointF& delta);
// The shape as it shows while its text or its line is dragged by `delta`
// pixels: the text moved, or the dimension line, its arrows and the text
// moved across it.
MeasurementShape draggedMeasurement(const MeasurementShape& shape, bool line, const QPointF& delta);
// What of the annotation is at a point of the picture: 1 its text, 2 its
// dimension line, 0 nothing (within `reach` pixels of a line).
int measurementHit(const MeasurementShape& shape, const QPointF& at, double reach = 5.0);

// The projection of a final render's camera (render::finalView's "view":
// projection, eye, target, up, half_height, size), for Render Image's
// annotations: half the image's height spans `half_height` at the target.
MeasurementProjection renderViewProjection(const QJsonObject& view);

} // namespace mitcad
