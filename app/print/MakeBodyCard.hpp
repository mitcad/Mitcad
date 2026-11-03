// SPDX-License-Identifier: MIT
#pragma once

// The selected body on the Make tab (mitcad#1049; app/COMMANDS.md "The
// Make tab"): a section of the selection card under the selection's text
// with the body's material and its 3D print settings, as the Properties
// window shows them: the printer and the process profile and every print
// value set on the body, its occurrences or its components rather than
// taken from a profile, each with the icon of the level that gives it
// (body, occurrence, component). The material, the process and the values
// are edited here; the window asks the model and sets them on the body
// (MainWindowMake.cpp), each one undo step. Add Print Value… under the
// values asks for a setting to add to the body (PrintValuePicker,
// mitcad#1178); a value set on the body has a remove button, which clears
// it there. The Properties window stays for full editing.

#include <QGroupBox>
#include <QHash>
#include <QPair>
#include <QString>
#include <QStringList>
#include <QVector>

class QComboBox;
class QGridLayout;
class QLabel;
class QLineEdit;

namespace mitcad {

// A print value set on a level, as the card shows it.
struct MakeCardValue {
  QString set;   // "printer" or "process"
  QString key;   // the key map's ("perimeters")
  QString name;  // its plain name ("Walls")
  QString text;  // its value as shown ("5", "0.2 mm")
  QString level; // the level that sets it: body, occurrence or component
  QString owner; // that level's name ("Body1", "Root")
  bool operator==(const MakeCardValue& other) const {
    return set == other.set && key == other.key && name == other.name && text == other.text &&
           level == other.level && owner == other.owner;
  }
};

// What the card shows of one body.
struct MakeCardData {
  QString body; // the body's name
  // Its material: name, id and where it comes from (body, component or
  // default), and the materials it can have (id, name).
  QString material;
  QString materialId;
  QString materialLevel;
  QVector<QPair<QString, QString>> materials;
  // The printer and process profiles it prints with ("Slicer default"
  // when none is chosen), the level that chose each and its name; the
  // design's process profiles (id, name) it can choose.
  QString printer;
  QString printerLevel;
  QString printerOwner;
  QString process;
  QString processId;
  QString processLevel;
  QString processOwner;
  QVector<QPair<QString, QString>> processes;
  // The printer each process is listed under (mitcad#1216: the body's
  // printer's processes first, then the others under their printers, Any
  // printer last; profilePickerRows), as many as `processes`; empty when
  // no process names a printer: then no headings.
  QStringList processGroups;
  QVector<MakeCardValue> values;

  bool operator==(const MakeCardData& other) const {
    return body == other.body && material == other.material && materialId == other.materialId &&
           materialLevel == other.materialLevel && materials == other.materials && printer == other.printer &&
           printerLevel == other.printerLevel && printerOwner == other.printerOwner && process == other.process &&
           processId == other.processId && processLevel == other.processLevel &&
           processOwner == other.processOwner && processes == other.processes &&
           processGroups == other.processGroups && values == other.values;
  }
  bool operator!=(const MakeCardData& other) const { return !(*this == other); }
};

// The card's one-line summary for the UI tests' log: "Body1: material PLA
// (component Root); printer Bench (component Root); process Standard
// (component Root); own: Walls = 5 (body Body1)".
QString makeCardSummary(const MakeCardData& data);

// The card's process drop-down for the UI tests' log (mitcad#1216): its
// processes under their printers, "Bench Y: Shared, Strong | Bench X:
// Fine | Any printer: Draft"; the names alone, "Fine, Draft", without
// groups.
QString makeCardProcessList(const MakeCardData& data);

// The icon of a level in app/icons, as the Properties window's Source
// column has it: body, insert-component (an occurrence), component;
// settings for a value without a level (a default).
QString makeCardLevelIcon(const QString& level);

class MakeBodyCard : public QGroupBox {
  Q_OBJECT

public:
  explicit MakeBodyCard(QWidget* parent = nullptr);

  // Shows the body's data; the rows are made again only when it changed,
  // so that a field being edited keeps its place. True when it changed.
  bool setData(const MakeCardData& data);
  const MakeCardData& data() const { return m_data; }
  // A setting added without a value to start from (the slicer's default:
  // mitcad#1178): a body row with an empty field, shown until a value
  // typed there is set (the data then has it), another body is shown or
  // its remove button drops it. `value.level` is "body".
  void addPending(const MakeCardValue& value);
  const MakeCardValue& pending() const { return m_pending; }
  // Gives a value's field the keyboard, its text selected; false when the
  // card has no such field.
  bool focusValue(const QString& key);
  // The places of its fields in the main window's coordinates, for the UI
  // tests: "Make card material at x,y", "Make card process at x,y", "Make
  // card value perimeters at x,y", "Make card remove perimeters at x,y"
  // (a value set on the body), "Make card add value at x,y"; and each
  // value's level icon, "Make card value perimeters icon body".
  QStringList places() const;

signals:
  // Another material, or another process profile, chosen (its id).
  void materialChosen(const QString& id);
  void processChosen(const QString& id);
  // A value typed and confirmed (Enter, or the field left changed).
  void valueEdited(const QString& set, const QString& key, const QString& text);
  // Add Print Value… clicked.
  void addValueRequested();
  // A body value's remove button clicked (not a pending one's: the card
  // drops that itself).
  void valueRemoved(const QString& set, const QString& key, const QString& name);

private:
  void rebuild();
  QLabel* levelIcon(const QString& level, const QString& owner);

  MakeCardData m_data;
  MakeCardValue m_pending; // key empty: none
  bool m_hasData = false;
  QGridLayout* m_grid = nullptr;
  QComboBox* m_material = nullptr;
  QComboBox* m_process = nullptr;
  QHash<QString, QLineEdit*> m_fields;    // the values' fields, by key
  QHash<QString, QWidget*> m_removes;     // the body values' remove buttons, by key
  QHash<QString, QString> m_icons;        // the values' level icons, by key
  QWidget* m_add = nullptr;               // Add Print Value…
};

} // namespace mitcad
