// SPDX-License-Identifier: MIT
#pragma once

#include <QHash>
#include <QStringList>
#include <QWidget>

class QAbstractButton;
class QCheckBox;
class QComboBox;
class QDialogButtonBox;
class QGridLayout;
class QLabel;
class QRadioButton;
class QToolButton;

namespace mitcad {

class CommandRegistry;
struct CommandDef;

namespace sketch {

class PaletteWarnings;
class SketchController;

// The sketch palette, in the command panel's place while sketching:
// the sketch's degrees of freedom, display and snap options, the user's
// most used sketch tools (Create, mitcad#1027), the constraints, the
// drawing tool's auto constraints and Finish Sketch.
class SketchPalette : public QWidget {
  Q_OBJECT

public:
  // `constraints` are the ids of the constraint commands, in palette order.
  SketchPalette(SketchController& controller, CommandRegistry& registry, const QStringList& constraints,
                QWidget* parent = nullptr);

  // Logs where the options are, for UI tests.
  void logLayout() const;
  // Logs where the options and the Auto constraint section, when shown,
  // are: again when the palette scrolls (mitcad#412).
  void logPlaces();
  // The status line as text ("2 degrees of freedom") and whether Clean Up
  // Stale Projections is offered, for live MCP's ui_state (mitcad#265).
  QString statusText() const;
  bool cleanUpOffered() const { return m_cleanUpOffered; }
  // Whether Project Again is offered beside it (mitcad#1303).
  bool projectAgainOffered() const { return m_projectAgainOffered; }

  // The Create section (mitcad#1027): the tools chosen again from the
  // Sketch tab's pins and the counts of the tools' starts
  // (PaletteCreateTools), as the palette opens for a sketch; the buttons
  // stay as they are until it opens again. Logged as "Palette create
  // tools: line, rectangle, … (counts|defaults)".
  void chooseCreateTools(const QStringList& pinned);
  const QStringList& createTools() const { return m_createIds; }
  // Finish Sketch, with Cancel Sketch beside it (mitcad#2225), out of the
  // palette's own rows, for a page that keeps them in sight under the
  // scrolling palette (MainWindow's wrapPalette, mitcad#1027): their
  // button box, null without the command. Their places are logged with
  // the palette's wherever they are.
  QWidget* takeFinish();
  // Whether a command can be in the Create section and is counted when it
  // starts in a sketch: a tool or a command with a panel of the Sketch
  // tab's Create and Modify groups (not an action such as Delete or Copy).
  static bool isCreateTool(const CommandDef& def);
  // The Create section's candidates in the ribbon's order: Create, then
  // Modify, a submenu's commands together where its first one is.
  static QStringList createCandidates(const CommandRegistry& registry);

signals:
  // The Auto constraint section was laid out shown: a scroll area around
  // the palette brings it into view (mitcad#412).
  void autoSectionShown(QWidget* section);
  // The `3D Sketch` box was checked (mitcad#1544): the main window turns
  // the open sketch into a 3D sketch in place.
  void convertTo3dRequested();

protected:
  void changeEvent(QEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  void refresh();
  void refreshTitle();
  // The Auto constraint section (mitcad#189): shown once the drawing tool
  // has drawn an entity, with the choices of its session.
  void refreshAuto();
  // The 3D Sketch box: unchecked, disabled while a tool has pending input
  // (mitcad#1544).
  void refreshSpatial();
  void logAutoLayout();
  // Logs where the Clean Up Stale Projections button is, when shown.
  void logCleanUp() const;
  // A click on a warning: what it is about selected (mitcad#264).
  void selectWarning(int index);
  // A click on the status "Conflicting constraints": the conflicting
  // constraints and dimensions selected (mitcad#1566); and where that link
  // is, logged once it shows.
  void selectConflicts();
  void logConflictLink();
  // The Construction (X) box (mitcad#2040): the selected curves' state
  // while X would convert them, else the mode for new geometry; logged
  // when it changes.
  void refreshConstruction();
  // A section's command button: the action's icon, tooltip, shortcut and
  // checked state.
  QToolButton* commandButton(const QString& id, const QString& name) const;
  // Floating: a command button's flat look with its checked state.
  void styleButton(QToolButton* button) const;
  // Logs the Create and Constraints buttons' places, their rows and
  // whether any is clipped (mitcad#1027).
  void logButtons() const;
  // Logs whether every row is whole within the panel (mitcad#1825: at a
  // large Text size and Icon size the Docked panel cut the labels).
  void logRows() const;

  SketchController& m_c;
  CommandRegistry& m_registry;
  QLabel* m_title = nullptr;
  QLabel* m_status = nullptr;
  // The solve's warnings and information under the status, each with its
  // level's icon (mitcad#1040: redundant constraints, unmet inactive ones).
  QLabel* m_solve = nullptr;
  QString m_loggedSolve; // its line last logged (UI tests)
  // The constraint whose notice the pointer is over (mitcad#1992).
  QString m_hoveredNotice;
  PaletteWarnings* m_warnings = nullptr; // under the status (mitcad#264)
  // The warnings' texts and the redundant constraints' ids last logged (UI
  // tests; mitcad#1171: the warnings group the redundant constraints).
  QString m_loggedWarnings;
  QString m_loggedRedundant;
  QLabel* m_tool = nullptr;
  QCheckBox* m_construction = nullptr;
  QString m_loggedConstruction; // its state last logged (UI tests)
  QCheckBox* m_signed = nullptr; // Signed distances for Sketch Dimension (mitcad#409)
  QCheckBox* m_grid = nullptr;
  QCheckBox* m_extensions = nullptr; // Snap to line extensions (mitcad#2035)
  QLabel* m_snapLabel = nullptr;     // "Snap to" before Grid and Line extensions
  QCheckBox* m_dimensions = nullptr;
  QCheckBox* m_constraints = nullptr;
  QCheckBox* m_points = nullptr; // Show points (mitcad#1705)
  QCheckBox* m_profiles = nullptr;
  QCheckBox* m_hideAbove = nullptr;
  QCheckBox* m_spatial = nullptr; // 3D Sketch: converts the sketch (mitcad#1544)
  QComboBox* m_dimensionText = nullptr;
  QLabel* m_dimensionLabel = nullptr; // "Dimension text" before it
  QWidget* m_autoSection = nullptr;
  QRadioButton* m_autoFirst = nullptr;
  QRadioButton* m_autoLast = nullptr;
  QHash<QString, QCheckBox*> m_autoChecks; // by constraint type
  QString m_autoLogged;                    // the section as last logged
  // Clean Up Stale Projections under a warning about linked projections
  // that cannot follow (mitcad#252).
  QAbstractButton* m_cleanUp = nullptr;
  bool m_cleanUpOffered = false;
  // The status's conflict link has been logged since it showed.
  bool m_conflictLinkShown = false;
  // Project Again beside it, for those whose sources are there
  // (mitcad#1303).
  QAbstractButton* m_projectAgain = nullptr;
  bool m_projectAgainOffered = false;
  // The Create section (mitcad#1027) and the constraints' buttons.
  QGridLayout* m_createGrid = nullptr;
  QStringList m_createIds;
  QList<QToolButton*> m_createButtons;
  QList<QToolButton*> m_constraintButtons;
  QToolButton* m_finish = nullptr; // Finish Sketch
  QAbstractButton* m_lookAt = nullptr; // Look At, logged for UI tests (mitcad#1060)
  // Show Unconstrained beside the status (mitcad#2026).
  QAbstractButton* m_showUnconstrained = nullptr;
  // Cancel Sketch beside Finish Sketch, both in m_finishButtons
  // (mitcad#2225, finishButtons).
  QToolButton* m_cancel = nullptr;
  QDialogButtonBox* m_finishButtons = nullptr;
};

// Finish Sketch and Cancel Sketch for a sketch palette (mitcad#2225), in
// the platform's OK/Cancel order as a command panel's OK and Cancel
// (QDialogButtonBox): capsules in the Floating layout, Finish Sketch the
// prominent one, tool buttons with their icons in the Docked one; each
// follows its command's action. The buttons are named `<prefix>Finish` and
// `<prefix>Cancel`, the box `<prefix>FinishButtons`. Null (and both
// buttons null) without the Finish Sketch command; `cancel` stays null
// without Cancel Sketch's.
QDialogButtonBox* finishButtons(CommandRegistry& registry, const QString& prefix, QToolButton*& finish,
                                QToolButton*& cancel);

} // namespace sketch
} // namespace mitcad
