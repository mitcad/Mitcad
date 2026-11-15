// SPDX-License-Identifier: MIT
#pragma once

// Preferences › Display › Selection color and Selected line width
// (mitcad#1803, GitHub mitcad/Mitcad#309; app/COMMANDS.md "Selection color
// and line width"): the colour the 3D view and sketch mode draw the
// selection in, and how wide the selection's and the hover's lines are
// (edges, curves, wires, face boundaries).
//   - Selection color: the selection role's blue (ColorRole::Select) by
//     default; any opaque colour.
//   - Selected line width: Thin (about the widths before mitcad#1803),
//     Normal (the default), Thick. The widths are in logical pixels: the
//     view multiplies them by its device pixel ratio, so that a highlight
//     looks as wide on a 200 % display as on a 100 % one.
// Kept in the application's settings ("view/selectionColor": "#rrggbb",
// "view/selectedLineWidth": "thin", "normal" or "thick"); a change goes to
// every open window at once (SelectionLookNotifier::changed,
// MainWindow::applySelectionLook).

#include <optional>

#include <QColor>
#include <QObject>
#include <QString>

class QSettings;

namespace mitcad {

enum class SelectedLineWidth { Thin, Normal, Thick };

// The width of a plain body edge in the view (device pixels, as OCCT draws
// it): what a selected edge is wider than.
constexpr double kPlainEdgeWidth = 1.0;

// The selection role's blue, the default Selection color.
QColor defaultSelectionColor();

struct SelectionLook {
  QColor color = defaultSelectionColor();
  SelectedLineWidth width = SelectedLineWidth::Normal;

  bool operator==(const SelectionLook& other) const {
    return color.rgb() == other.color.rgb() && width == other.width;
  }
  bool operator!=(const SelectionLook& other) const { return !(*this == other); }

  // The settings above; a colour that is not one, or an unknown width,
  // is the default.
  static SelectionLook load(const QSettings& settings);
  void save(QSettings& settings) const;
  // "color #1f7aff, line width normal", for the log.
  QString describe() const;
};

QString selectedLineWidthId(SelectedLineWidth width);
std::optional<SelectedLineWidth> selectedLineWidthFromId(const QString& id);

// The widths of the selection's and the hover's lines in logical pixels:
// edges, curves and wires, and the boundaries of selected faces.
//   Thin 3 and 2, Normal 4 and 3, Thick 6 and 4.
double selectedEdgeWidth(SelectedLineWidth width);
double selectedBoundaryWidth(SelectedLineWidth width);

// The application's choice (QSettings of the application), read once and
// kept until saveSelectionLook changes it.
SelectionLook savedSelectionLook();
// Saves the choice and tells SelectionLookNotifier; false when it was
// already so.
bool saveSelectionLook(const SelectionLook& look);
// The colour the selection is drawn in: savedSelectionLook().color.
QColor selectionColor();

// Tells the windows that the choice changed (saveSelectionLook).
class SelectionLookNotifier : public QObject {
  Q_OBJECT

public:
  static SelectionLookNotifier& instance();

signals:
  void changed();
};

} // namespace mitcad
