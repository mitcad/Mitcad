// SPDX-License-Identifier: MIT
#pragma once

#include <QDialog>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>

class QComboBox;
class QFormLayout;
class QHideEvent;
class QLabel;
class QLineEdit;
class QPushButton;

namespace mitcad {

// Filament Change (mitcad#1643, mitcad#1800): one event of Send to
// Slicer's Filament changes (or Plates…'), a change or a pause, edited
// beside the panel. It is not modal, so that a plane or a flat face can be
// picked in the view meanwhile. **At** chooses how the event is placed, as
// Extrude chooses its extent: **Distance from Bed**, a height typed in the
// design's length unit (a bare number) or with a unit (`12 mm`, `0.5 in`)
// in **Height above bed**; or **To Object**, a plane or flat face picked in
// the view into **Plane or face**, which shows its name with the height it
// gives (`Plane1: 15 mm above the bed`), or why it would be left out under
// it. While To Object is chosen its field takes the view's picks (the owner
// gives them to it on picksWanted, through the panel's input that the panel
// does not draw), drawn checked as a panel's active selection is; a click
// on it takes them again after a panel row took them. **Change to** lists
// the design's materials in their appearances, then Pause; **Message** is
// a pause's. OK hands the event in the schedule's form (`{height | at,
// kind, material, appearance, message}`) to its owner, which sets the
// schedule (one undo step) and closes the window, or shows why it was
// refused in the window.
//
// Logs `Filament change dialog: <new | row N>, <height H | to object
// <place> | to object none>, change to <choice>` when it opens,
// `Filament change dialog extent: <distance from bed | to object>` when At
// changes, `Filament change dialog at: <place>` when a pick fills the
// field, `Filament change dialog picks: <on | off>` when the field starts
// or stops taking the view's picks, and `Filament change dialog refused:
// <why>`.
class FilamentChangeDialog : public QDialog {
  Q_OBJECT

public:
  explicit FilamentChangeDialog(QWidget* parent = nullptr);

  // Edits `event` of row `row` (-1: a new one): at a plane or face (its
  // `at`) To Object, with `place` its words and the height it gives and
  // `warning` why it would be left out (empty: none); else Distance from
  // Bed. What a change can change to (the `filament_changes` query's
  // `choices`) and the one to start with for a new change (`preferred`, an
  // index into them); the design's length unit (`mm`, `in`) and its
  // millimetres.
  void edit(int row, const QJsonObject& event, const QString& place, const QString& warning,
            const QJsonArray& choices, int preferred, const QString& unit, double unitMillimetres);
  // A plane or face picked in the view: the event is at it (`place` and
  // `warning` as for edit).
  void setAt(const QJsonValue& at, const QString& place, const QString& warning);
  // Whether the Plane or face field takes the view's picks now.
  void setPicking(bool picking);
  // Whether the window wants the view's picks: shown, with To Object.
  bool wantsPicks() const;
  // Whether the event is a pause (Pause chosen in Change to).
  bool isPause() const;
  // Why the owner refused the event, under the fields.
  void showRefusal(const QString& why);
  int row() const { return m_row; }

signals:
  // OK with a valid event.
  void applied(int row, const QJsonObject& event);
  // To Object chosen or its field clicked while the window shows (true):
  // the owner gives the field the view's picks; Distance from Bed chosen
  // or the window closed (false): they go back to the panel.
  void picksWanted(bool wanted);

protected:
  void hideEvent(QHideEvent* event) override;

private:
  void apply();
  void kindChanged();
  void extentChanged();
  bool toObject() const;
  QString placeText() const;
  // A typed height in mm, or a negative number when it is none.
  double typedHeight() const;

  int m_row = -1;
  QJsonObject m_event;
  QJsonValue m_at;
  QString m_atPlace;
  QJsonArray m_choices;
  QString m_unit;
  double m_unitMillimetres = 1.0;
  bool m_picking = false;
  QFormLayout* m_form = nullptr;
  QComboBox* m_extent = nullptr;
  QLineEdit* m_place = nullptr;
  QPushButton* m_object = nullptr;
  QLabel* m_objectNote = nullptr;
  QComboBox* m_change = nullptr;
  QLineEdit* m_message = nullptr;
  QLabel* m_refusal = nullptr;
};

} // namespace mitcad
