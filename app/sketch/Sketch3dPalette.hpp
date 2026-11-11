// SPDX-License-Identifier: MIT
#pragma once

// The 3D Sketch Palette (mitcad#1524): the command card's page while a 3D
// sketch is edited, in place of the Sketch Palette. Its title says `3D
// Sketch` and the sketch's name; under it the status (its freedoms, or why
// it does not solve), what is determined, redundant, in plane and linked,
// what is selected and what is under the pointer, that the 2D sketch tools
// do not apply, and Finish Sketch.
//
// While a drawing tool is active (mitcad#1525) it shows the tool's
// options: the temporary plane (XY, YZ, ZX), the placement (On Plane, Axis
// X, Y or Z, Normal), Keep Axis, Keep Plane and Construction, the input
// mode (Offsets or Axis and Length with its axis and sense), the fields
// (Delta X, Y, Z, Length, Radius, Arc Length: an untouched field shows the
// live measurement greyed, a typed one is driving), the snap under the
// pointer, why a click would be refused, and Release from Plane when that
// names On Sketch Plane. Tab moves between the fields; Enter checks them
// and commits; Esc takes the field's value back. The point tools also show
// the plane's free rotation (mitcad#1661): a Rotate field per manipulator
// axis (Enter turns the plane by the angle typed; a handle's drag shows
// its angle there), Reset, and Show Plane and Show Handles.
//
// With no tool (mitcad#1526) it shows the editing: the plane or axis a
// drag moves on, the relations and dimensions the selection takes (each
// button disabled with the reason when it takes none), Attach, the
// notice (a refusal, a pick's prompt, a drag's state, what a Delete takes
// with it and Delete with Dependents), every relation and dimension with
// its driving value or measurement (Enter sets it), Driven and Remove,
// and every linked reference with its source and status, Replace and
// Detach.

#include <QJsonArray>
#include <QMap>
#include <QString>
#include <QWidget>

class QCheckBox;
class QGridLayout;
class QLabel;
class QLineEdit;
class QToolButton;

namespace mitcad {

class CommandRegistry;

namespace sketch {

class Sketch3dController;

class Sketch3dPalette : public QWidget {
  Q_OBJECT

public:
  Sketch3dPalette(Sketch3dController& controller, CommandRegistry& registry, QWidget* parent = nullptr);

  // Shows the sketch as it is now.
  void refresh();
  // A sketch opened in the 3D editor (also one just converted in place):
  // the card from its top, its list not scrolled into sight as after an
  // edit, its places logged again.
  void opened();
  // The status line as text, for live MCP's ui_state.
  QString statusText() const;
  // The lists' rows for live MCP's ui_state (mitcad#1652): each relation,
  // dimension and linked reference with its `id`, the `text` the palette
  // shows and the agents' `detail` with the ids.
  QJsonArray listState() const;
  // Logs where Finish Sketch is, for UI tests.
  void logLayout() const;

signals:
  // The 3D Sketch box unchecked (mitcad#1550): turn the sketch back into a
  // 2D one (it all lies in its sketch plane).
  void convertTo2dRequested();

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  // The 3D Sketch box's state, at once when it changes, also when a tool
  // starts or ends (mitcad#1592): enabled when the sketch all lies in its
  // sketch plane and no tool is active; logged "3D Sketch Palette 3D Sketch
  // box: enabled yes|no: <why>" when it changes.
  void refreshKind();
  void buildTool();
  // The tool's options as the controller has them now.
  void refreshTool();
  // The plane and placement controls' tooltips with the manipulator's keys
  // as Keyboard Shortcuts has them (mitcad#1661); logged "3D sketch control
  // keys: planes Tab: the next plane, Shift+Tab: the previous plane; normal
  // Up: On Plane or Normal" when they change.
  void refreshKeyTips();
  // Where the tool's controls and fields are, for UI tests: "3D sketch
  // control Plane YZ at x,y", "3D sketch field dx at x,y".
  void logToolLayout();

  // Editing (mitcad#1526): the drag plane or axis, the relations and
  // dimensions the selection takes, the notice with Delete with
  // Dependents, the list of relations and dimensions (a dimension's value,
  // Driven and Remove), the linked references (Replace, Detach).
  void buildEdit();
  void refreshEdit();
  // Queue a rebuild of the lists from the event loop (never inside an
  // input event of one of their rows); build* does it when they changed.
  void refreshItems();
  void refreshReferences();
  void buildItems();
  void buildReferences();
  // A list row's words (mitcad#1652): no ids; the pointer over them makes
  // the view paint what the row names (`id`) and its source, and their
  // place is logged "3D sketch item k3 text at x,y" ("reference c10 text").
  QLabel* rowText(const QString& words, const QString& id);
  // Where they are, for UI tests: "3D sketch edit control coincident at
  // x,y", "3D sketch item k3 field at x,y", "3D sketch reference k2
  // replace at x,y".
  void logEditLayout();
  void scheduleEditLayout();
  bool itemFieldEvent(QLineEdit* edit, QEvent* event);
  // A Rotate field's Enter (the plane turned by its angle) and Esc
  // (mitcad#1661).
  bool rotateFieldEvent(QLineEdit* edit, const QString& axis, QEvent* event);

  Sketch3dController& m_c;
  QLabel* m_title = nullptr;
  // Checked; unchecking it turns the sketch back into 2D (mitcad#1544,
  // mitcad#1550).
  QCheckBox* m_kind = nullptr;
  QString m_loggedKind; // its state last logged
  QLabel* m_status = nullptr;
  QLabel* m_details = nullptr;
  QLabel* m_selection = nullptr;
  QLabel* m_hover = nullptr;
  QLabel* m_tools = nullptr;
  QToolButton* m_finish = nullptr;
  QString m_logged; // the palette's text last logged

  // The tool's section.
  QWidget* m_toolBox = nullptr;
  QLabel* m_toolTitle = nullptr;
  QLabel* m_toolHint = nullptr;
  QMap<QString, QToolButton*> m_controls; // by the name the logs give
  QCheckBox* m_keepAxis = nullptr;
  QCheckBox* m_keepPlane = nullptr;
  QCheckBox* m_construction = nullptr;
  QWidget* m_modeRow = nullptr;
  QWidget* m_axisRow = nullptr;
  QWidget* m_normalRow = nullptr; // a Center Point Arc's normal
  QLabel* m_normalText = nullptr;
  QWidget* m_planeRow = nullptr; // the temporary plane and the placement:
  QWidget* m_placeRow = nullptr; // not Fillet's, which places no point
  // The plane's free rotation (mitcad#1661): the Rotate fields by their
  // axis ("x", "y", "z"), Reset, Show Plane and Show Handles.
  QWidget* m_rotateRow = nullptr;
  QWidget* m_showRow = nullptr;
  QMap<QString, QLineEdit*> m_rotateFields;
  QCheckBox* m_showPlane = nullptr;
  QCheckBox* m_showHandles = nullptr;
  QMap<QString, QLineEdit*> m_fields;
  QMap<QString, QWidget*> m_fieldRows;
  QLabel* m_snap = nullptr;
  QLabel* m_problem = nullptr;
  QToolButton* m_release = nullptr;
  bool m_toolShown = false;
  QString m_loggedLayout;
  QString m_loggedKeyTips; // mitcad#1661

  // Editing's section (mitcad#1526).
  QWidget* m_editBox = nullptr;
  QLabel* m_notice = nullptr;
  QMap<QString, QToolButton*> m_editControls; // by the name the logs give
  QWidget* m_itemsBox = nullptr;
  QGridLayout* m_itemsGrid = nullptr;
  QString m_itemsShown;
  QMap<QString, QLineEdit*> m_itemFields; // a dimension's value, by its id
  QWidget* m_refsBox = nullptr;
  QGridLayout* m_refsGrid = nullptr;
  QString m_refsShown;
  QMap<QString, QWidget*> m_editPlaces; // what the layout log names
  QString m_loggedEditLayout;
  QString m_loggedItems;
  bool m_editLayoutPending = false;
  bool m_scrollWatched = false; // the card's scrolling logs the places again
  bool m_itemsQueued = false; // a rebuild of the list waits for the event loop
  bool m_revealItems = false; // the rebuilt list is scrolled into the card's sight
  QString m_itemsSketch;      // the sketch the list was built for (an edit's rebuild reveals it)
  bool m_refsQueued = false;
  // A new selection's description is scrolled into the card's sight
  // (mitcad#1592: below the lists of a long card it was out of it).
  QString m_selectionShown;
  bool m_revealSelection = false;
};

} // namespace sketch
} // namespace mitcad
