// SPDX-License-Identifier: MIT
#pragma once

// Preferences › Display › Origin planes (mitcad#1748, GitHub
// mitcad/Mitcad#273; app/COMMANDS.md "Origin planes and the size of
// construction geometry"): how the 3D view draws the design's origin.
//   - Size Screen (the default): the planes about 80 px on the screen at any
//     zoom, drawn with a zoom-persistent transform at the origin; Model: in
//     millimetres (default 100 mm), so that they zoom with the model.
//   - Style Corner (the default): each plane a square from the origin into
//     the quadrant of the positive model axes in it, the axes coloured lines
//     from the origin a little past the planes; Centred: each plane a
//     square centred on the origin and the axes through it, as before.
// Kept in the application's settings ("view/originSize": "screen" or
// "model", "view/originSizeMm": the millimetres, "view/originStyle":
// "corner" or "centred"); a change goes to every open window at once
// (OriginDisplayNotifier::changed, MainWindow::applyOriginDisplay).

#include <optional>

#include <QJsonObject>
#include <QObject>
#include <QString>

#include <TopoDS_Shape.hxx>
#include <gp_Dir.hxx>

class QSettings;

namespace mitcad {

enum class OriginSize { Screen, Model };
enum class OriginStyle { Corner, Centred };

// The planes' side on the screen with Size Screen, logical pixels.
constexpr double kOriginScreenPixels = 80.0;
// The planes' side in millimetres with Size Model: the default and the
// range the Preferences field takes.
constexpr double kOriginDefaultMillimetres = 100.0;
constexpr double kOriginMinMillimetres = 0.1;
constexpr double kOriginMaxMillimetres = 100000.0;
// In the Corner style an axis reaches this many times the planes' side
// from the origin, past the planes' edges.
constexpr double kCornerAxisReach = 1.25;

struct OriginDisplay {
  OriginSize size = OriginSize::Screen;
  double millimetres = kOriginDefaultMillimetres;
  OriginStyle style = OriginStyle::Corner;

  bool operator==(const OriginDisplay& other) const {
    return size == other.size && millimetres == other.millimetres && style == other.style;
  }
  bool operator!=(const OriginDisplay& other) const { return !(*this == other); }

  // The settings above; an unknown size or style is the default, a
  // millimetre value out of range or no number is 100 mm.
  static OriginDisplay load(const QSettings& settings);
  void save(QSettings& settings) const;
  // "screen size, corner style" or "model size 25 mm, centred style", for
  // the log.
  QString describe() const;
};

QString originSizeId(OriginSize size);
std::optional<OriginSize> originSizeFromId(const QString& id);
QString originStyleId(OriginStyle style);
std::optional<OriginStyle> originStyleFromId(const QString& id);

// The application's choice (QSettings of the application).
OriginDisplay savedOriginDisplay();
// Saves the choice and tells OriginDisplayNotifier; false when it was
// already so.
bool saveOriginDisplay(const OriginDisplay& display);

// Tells the windows that the choice changed (saveOriginDisplay).
class OriginDisplayNotifier : public QObject {
  Q_OBJECT

public:
  static OriginDisplayNotifier& instance();

signals:
  void changed();
};

// The way of a direction whose largest component is positive: +X, +Y or
// +Z for the model's axes, whichever way the datum names them.
gp_Dir positiveWay(const gp_Dir& direction);

// The shape an origin datum (the datums query's numbers: a plane's origin,
// normal and x_axis, an axis's origin and direction, a point) is drawn
// with, its plane's side `size`: Centred, a square centred on the plane's
// origin and an axis `size` long through its origin (as datumShape);
// Corner, the square from the origin along the positive ways of the
// plane's two axes, an axis from its origin along its positive way,
// kCornerAxisReach times `size` long. A point is a vertex either way. Null
// for a datum that is degenerate. In pixels with Size Screen (the view
// scales it, OcctViewer::setOriginOnScreen), in millimetres with Size
// Model.
TopoDS_Shape originDatumShape(const QJsonObject& datum, OriginStyle style, double size);

} // namespace mitcad
