// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <utility>

#include <QCoreApplication>
#include <QDialog>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>

class QComboBox;
class QLineEdit;
class QPushButton;
class QTableWidget;

namespace mitcad {

// The Annotations window (mitcad#702, app/COMMANDS.md "Annotations"): the
// dimensions, notes, datum labels and feature control frames kept in the
// design as a table — name, kind, value, tolerance, own text, colour,
// group, light bulb — sortable, with a search field and filters by kind
// and by group. Several rows selected change together (Colour…, Group…,
// Decimals…, Tolerance…, Show, Hide, Delete); Add Frame… puts a feature
// control frame under one dimension; Text Height… sets the height of all
// the design's annotations' texts (mitcad#894); a group is shown or hidden at once;
// Edit… or a double-click opens one's editor; a broken one (a pick gone) is marked with the reason. It follows
// the model (setAnnotations after every refresh, its selection and where
// the list is scrolled kept, mitcad#759); what it changes goes
// through its actions, each one undo step.
//
// For UI tests it logs its rows when they change ("Annotations row <name>:
// <kind>, <value>, <tolerance>, <group>, <#colour>, shown|hidden|broken:
// <why>"; "Annotations shown rows: <names>", those the filters leave), the
// selection ("Annotations selected: <names>") and where its rows, fields
// and buttons are, in the main window's coordinates ("Annotations row
// <name> at x,y", "Annotations Colour at x,y").
class AnnotationsWindow : public QDialog {
  Q_DECLARE_TR_FUNCTIONS(mitcad::AnnotationsWindow)

public:
  struct Actions {
    std::function<void(const QStringList& names)> selected; // rows selected: highlighted in the view
    std::function<void(const QString& name)> edit;          // Edit… or a double-click
    // Several changed at once: the fields of edit_annotation.
    std::function<void(const QStringList& names, const QJsonObject& change)> change;
    std::function<void(const QStringList& names)> remove;
    // Add Frame… on one dimension: a feature control frame under it.
    std::function<void(const QString& dimension)> frame;
    // Text Height…: the height of the design's annotations' texts
    // (mitcad#894), asked for and set.
    std::function<void()> textHeight;
  };

  explicit AnnotationsWindow(QWidget* parent);
  void setActions(Actions actions) { m_actions = std::move(actions); }
  // The `annotations` query's entries and the document's length unit.
  void setAnnotations(const QJsonArray& annotations);
  // Selects these rows (a click on an annotation in the view).
  void selectNames(const QStringList& names);
  // The groups the annotations have, sorted.
  QStringList groups() const;

protected:
  void showEvent(QShowEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;
  void moveEvent(QMoveEvent* event) override;

private:
  void fill();
  QStringList selectedNames() const;
  QJsonObject annotation(const QString& name) const;
  // The selected annotations that are dimensions (decimals and
  // tolerances), or all selected.
  QStringList selectedDimensions() const;
  void followSelection();
  // Selects the rows of these names, all at once.
  void selectRows(const QStringList& names, bool scroll);
  void changeSelected(const QJsonObject& change);
  void chooseColour();
  void chooseGroup();
  void chooseDecimals();
  void chooseTolerance();
  void showGroup(bool shown);
  void logLayout();

  Actions m_actions;
  QJsonArray m_annotations;
  QLineEdit* m_search = nullptr;
  QComboBox* m_kind = nullptr;
  QComboBox* m_group = nullptr;
  QPushButton* m_showGroup = nullptr;
  QPushButton* m_hideGroup = nullptr;
  QTableWidget* m_table = nullptr;
  QPushButton* m_edit = nullptr;
  QPushButton* m_colour = nullptr;
  QPushButton* m_groupButton = nullptr;
  QPushButton* m_decimals = nullptr;
  QPushButton* m_tolerance = nullptr;
  QPushButton* m_addFrame = nullptr;
  QPushButton* m_show = nullptr;
  QPushButton* m_hide = nullptr;
  QPushButton* m_delete = nullptr;
  QPushButton* m_textHeight = nullptr;
  bool m_filling = false;
  QString m_loggedRows;
  QString m_loggedPlaces;
  QString m_loggedSelection;
  QString m_loggedShown; // the rows the search and the filters leave, as last logged
};

} // namespace mitcad
