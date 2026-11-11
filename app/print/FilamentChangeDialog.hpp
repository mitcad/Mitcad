// SPDX-License-Identifier: MIT
#pragma once

#include <QDialog>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>

class QComboBox;
class QLabel;
class QLineEdit;

namespace mitcad {

// Filament Change (mitcad#1643): one event of Send to Slicer's Filament
// changes, edited beside the panel (it is not modal, so that a plane or a
// flat face can be picked in the view meanwhile, by the panel's Change At
// row). **At** takes a height typed in the design's length unit (a bare
// number) or with a unit (`12 mm`, `0.5 in`); a plane or face picked shows
// its name there instead, and typing takes it away. **Change to** lists
// the design's materials in their appearances, then Pause; **Message** is
// a pause's. OK hands the event in the schedule's form (`{height | at,
// kind, material, appearance, message}`) to its owner, which sets the
// schedule (one undo step) and closes the window, or shows why it was
// refused in the window.
//
// Logs `Filament change dialog: <row or new>, at <text>, change to
// <choice>` when it opens, `Filament change dialog at: <text>` when a pick
// fills At, and `Filament change dialog refused: <why>`.
class FilamentChangeDialog : public QDialog {
  Q_OBJECT

public:
  explicit FilamentChangeDialog(QWidget* parent = nullptr);

  // Edits `event` of row `row` (-1: a new one): its place in words
  // (`place`, for an event at a plane or face), what a change can change to
  // (the `filament_changes` query's `choices`) and the one to start with
  // for a new change (`preferred`, an index into them), the design's
  // length unit (`mm`, `in`) and its millimetres.
  void edit(int row, const QJsonObject& event, const QString& place, const QJsonArray& choices, int preferred,
            const QString& unit, double unitMillimetres);
  // A plane or face picked in the view: the event is at it.
  void setAt(const QJsonValue& at, const QString& place);
  // Why the owner refused the event, under the fields.
  void showRefusal(const QString& why);
  int row() const { return m_row; }

signals:
  // OK with a valid event.
  void applied(int row, const QJsonObject& event);

private:
  void apply();
  void kindChanged();
  // A typed height in mm, or a negative number when it is none.
  double typedHeight() const;

  int m_row = -1;
  QJsonObject m_event;
  QJsonValue m_at;
  QJsonArray m_choices;
  QString m_unit;
  double m_unitMillimetres = 1.0;
  QLineEdit* m_place = nullptr;
  QComboBox* m_change = nullptr;
  QLineEdit* m_message = nullptr;
  QLabel* m_refusal = nullptr;
};

} // namespace mitcad
