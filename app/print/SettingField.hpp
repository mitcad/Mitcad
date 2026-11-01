// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QJsonValue>
#include <QString>
#include <QStringList>
#include <QWidget>

#include "PrintKeys.hpp"

class QEvent;
class QComboBox;
class QLabel;
class QLineEdit;
class QPushButton;
class QToolButton;

namespace mitcad {

// One print setting's field (mitcad#655), the same for a kind of value in
// the material editor, the profile editor and Property Sets:
//
// | Kind              | Field                                      |
// |-------------------|--------------------------------------------|
// | Boolean           | a drop-down Yes | No                        |
// | Integer           | a number field with its unit               |
// | Number            | a number field with its unit               |
// | Percent           | a number field with `%`                    |
// | Number or percent | a number field with a [unit \| %] box      |
// | Choice            | a drop-down of plain names                 |
// | Colour            | a field-wide button: its swatch, the picker  |
// | Text              | a text field                               |
// | Outline           | `256 × 256 mm` and Edit…                   |
// | Per slicer        | `slicer: name`, read only                  |
// | One slicer's      | an editable drop-down of preset names      |
//
// One slicer's field (`slicer`, mitcad#1062: a material's filament preset
// per slicer) edits that slicer's entry of a value kept per slicer: it
// shows the entry, offers the names `setChoicesSource` gives when its list
// opens, takes a typed name, and commits the whole value with that entry
// set or, emptied or ↺, taken out (null when none is left). Unset, it
// shows the name another slicer of the Bambu Studio family gives
// (slicerPresetOf), which the export uses then, else None.
//
// Values are in the key's own unit and range (printRange). Every field is
// a real input of the same width; an unset one is empty with the grey
// placeholder "Slicer default" (a drop-down too) and stays empty when it
// is opened: Enter on an empty field sets nothing. A value is committed
// when Enter is pressed or the field is left after a change (a choice and
// a colour at once); ↺ beside
// the field clears a set value; a refusal shows under the field. Drop-downs
// and spin boxes take the mouse wheel only while they have the keyboard. The field
// commits nothing itself: `committed` gives the value (null: clear) to the
// window, which runs the model command and shows the value it then has.
class SettingField : public QWidget {
  Q_OBJECT

public:
  // `takesPercent`: a number-or-percentage key offers [unit | %] (false:
  // the design's slicer takes no percentage for it, mitcad#655: the unit as
  // a label).
  // `slicer`: a key kept per slicer edited for that slicer only (above);
  // empty: the whole value, read only.
  explicit SettingField(const PrintKey& key, QWidget* parent = nullptr, bool takesPercent = true,
                        const QString& slicer = QString());

  const PrintKey& key() const { return m_key; }
  QString slicer() const { return m_slicer; }
  // The names one slicer's field offers, asked each time its list opens
  // (the installed slicer's presets).
  void setChoicesSource(std::function<QStringList()> source) { m_choices = std::move(source); }
  // The design's slicer (designDialectOf; empty: none): typed numbers are
  // checked against its range of the key where it takes less than the
  // key's own (mitcad#746: Bambu Studio's -1 for "automatic").
  void setDialect(const QString& dialect) { m_dialect = dialect; }
  // The value shown (null or undefined: unset). Ignored while the user has
  // typed into the field without committing.
  void setValue(const QJsonValue& value);
  QJsonValue value() const { return m_value; }
  bool isSet() const { return m_set; }
  // Read only: the field disabled, no ↺.
  void setReadOnly(bool readOnly);
  bool isReadOnly() const { return m_readOnly; }
  // What an unset field stands for, shown as its placeholder instead of
  // "Slicer default" (mitcad#655: a filament colour, the material's
  // appearance colour, which is what is exported); null: none.
  // `words`, when given, name it instead (a colour: "From appearance").
  void setPlaceholderValue(const QJsonValue& value, const QString& words = QString());
  // The own values that bodies, occurrences or components set of the key
  // (mitcad#655: the process editor on the design's process), as a grey
  // hint after the field: "4 on Body1", one entry each. Empty hides it.
  void setOwnValues(const QStringList& values);
  // A small grey hint after the field (mitcad#655: the value Update would
  // bring), its tooltip saying where it comes from; empty hides it.
  void setHint(const QString& hint, const QString& tooltip = QString());
  // Whether a set value shows ↺ (Property Sets: only a value of the level
  // shown).
  void setClearable(bool clearable);
  // A refusal shown under the field (the model's, or typed text that is
  // not a value of the key); empty hides it.
  void setError(const QString& message);
  // Gives the field the keyboard (an unset drop-down opens its list).
  void startEditing();
  // Where a click edits the field, and ↺ (null while hidden): for the
  // places logged.
  QWidget* placeWidget() const;
  QWidget* clearWidget() const;

protected:
  // Enter in a text field commits what it shows (nothing when empty).
  bool eventFilter(QObject* watched, QEvent* event) override;

signals:
  // A value to set (null: clear the value).
  void committed(const QString& key, const QJsonValue& value);
  // Typed text that is not a value of the key: the one-line reason.
  void refused(const QString& key, const QString& reason);

private:
  void build();
  void showValue();
  // The [unit | %] box once there is a value, else the unit as text.
  void showUnit();
  void updateState();
  // The field's text and the unit box as a value; commits it when it is
  // one, else reports why not.
  void commitTyped();
  void commit(const QJsonValue& value);
  // One slicer's field: the value with this slicer's entry `name` (empty:
  // taken out), committed when it changes.
  void commitPreset(const QString& name);
  // One slicer's field: its own entry of the value (empty: none).
  QString ownPreset() const;
  void editOutline();
  void pickColor();
  QString typedText() const;

  PrintKey m_key;
  QString m_slicer;  // one slicer's field: the slicer (mitcad#1062)
  std::function<QStringList()> m_choices; // ... the names it offers
  QString m_dialect; // the design's slicer, for the range
  QJsonValue m_value;
  QJsonValue m_placeholder; // what an unset value stands for
  QString m_placeholderText; // its words, when they name it
  bool m_set = false;
  bool m_dirty = false;   // typed since shown
  bool m_readOnly = false;
  bool m_clearable = true;
  QWidget* m_editor = nullptr;
  QToolButton* m_clear = nullptr;
  QLineEdit* m_line = nullptr;
  QComboBox* m_combo = nullptr;    // a choice, yes or no
  QComboBox* m_unitBox = nullptr;  // number or percent
  QLabel* m_unitLabel = nullptr;    // number or percent without a percentage
  QLabel* m_unitPlain = nullptr;    // the unit, unset, until a value is typed
  bool m_takesPercent = true;
  QPushButton* m_swatch = nullptr; // colour
  QLabel* m_text = nullptr;        // per slicer
  QLineEdit* m_outline = nullptr;  // an outline, read only (Edit… changes it)
  QPushButton* m_edit = nullptr;   // outline's Edit…
  QLabel* m_message = nullptr;     // a refusal
  QLabel* m_hint = nullptr;        // the value Update would bring
  QLabel* m_overrideMark = nullptr; // own values of bodies or components below
};

} // namespace mitcad
