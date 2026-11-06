// SPDX-License-Identifier: MIT
#pragma once

#include <QHash>
#include <QStringList>
#include <QWidget>

class QAbstractButton;
class QCheckBox;
class QComboBox;
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
  // Finish Sketch out of the palette's own rows, for a page that keeps it
  // in sight under the scrolling palette (MainWindow's wrapPalette,
  // mitcad#1027); null without the command. Its places are logged with
  // the palette's wherever it is.
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

protected:
  void changeEvent(QEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  void refresh();
  void refreshTitle();
  // The Auto constraint section (mitcad#189): shown once the drawing tool
  // has drawn an entity, with the choices of its session.
  void refreshAuto();
  void logAutoLayout();
  // Logs where the Clean Up Stale Projections button is, when shown.
  void logCleanUp() const;
  // A click on a warning: what it is about selected (mitcad#264).
  void selectWarning(int index);
  // A section's command button: the action's icon, tooltip, shortcut and
  // checked state.
  QToolButton* commandButton(const QString& id, const QString& name) const;
  // Floating: a command button's flat look with its checked state.
  void styleButton(QToolButton* button) const;
  // Logs the Create and Constraints buttons' places, their rows and
  // whether any is clipped (mitcad#1027).
  void logButtons() const;

  SketchController& m_c;
  CommandRegistry& m_registry;
  QLabel* m_title = nullptr;
  QLabel* m_status = nullptr;
  // The solve's warnings and information under the status, each with its
  // level's icon (mitcad#1040: redundant constraints, unmet inactive ones).
  QLabel* m_solve = nullptr;
  QString m_loggedSolve; // its line last logged (UI tests)
  PaletteWarnings* m_warnings = nullptr; // under the status (mitcad#264)
  // The warnings' texts and the redundant constraints' ids last logged (UI
  // tests; mitcad#1171: the warnings group the redundant constraints).
  QString m_loggedWarnings;
  QString m_loggedRedundant;
  QLabel* m_tool = nullptr;
  QCheckBox* m_construction = nullptr;
  QCheckBox* m_signed = nullptr; // Signed distances for Sketch Dimension (mitcad#409)
  QCheckBox* m_grid = nullptr;
  QCheckBox* m_dimensions = nullptr;
  QCheckBox* m_constraints = nullptr;
  QCheckBox* m_profiles = nullptr;
  QCheckBox* m_hideAbove = nullptr;
  QComboBox* m_dimensionText = nullptr;
  QWidget* m_autoSection = nullptr;
  QRadioButton* m_autoFirst = nullptr;
  QRadioButton* m_autoLast = nullptr;
  QHash<QString, QCheckBox*> m_autoChecks; // by constraint type
  QString m_autoLogged;                    // the section as last logged
  // Clean Up Stale Projections under a warning about linked projections
  // that cannot follow (mitcad#252).
  QAbstractButton* m_cleanUp = nullptr;
  bool m_cleanUpOffered = false;
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
};

} // namespace sketch
} // namespace mitcad
