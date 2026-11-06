// SPDX-License-Identifier: MIT
#pragma once

// The selected body on the Make tab (mitcad#1049; app/COMMANDS.md "The
// Make tab"): a section of the selection card under the selection's text
// with the body's 3D print settings and its material, as the Properties
// window shows them, from the widest choice to the body's own
// (mitcad#1239): the printer, the process profile, the material and every
// print value set on the body, its occurrences or its components rather
// than taken from a profile, each with the icon of the level that gives it
// (body, occurrence, component). The printer is the whole design's (its
// root); the process, the material and the values are the body's. Each is
// edited here; the window asks the model and sets them (MainWindowMake.cpp),
// each one undo step. The printer and the process are chosen in
// searchable pickers as Send to Slicer's (PickerComboBox,
// profilePickerRows): the design's and the libraries' profiles, a
// library's copied into the design as it is chosen; their last rows open
// the Printers and the Processes managers. Add Print Value… under the
// values asks for a setting to add to the body (PrintValuePicker,
// mitcad#1178); a value set on the body has a remove button, which clears
// it there. Send to Slicer, the last row (mitcad#1377), opens Send to
// Slicer's panel with the card's body alone in Bodies. The Properties
// window stays for full editing.

#include <QGroupBox>
#include <QHash>
#include <QPair>
#include <QString>
#include <QStringList>
#include <QVector>

#include "ProfilePickerRows.hpp"

class QComboBox;
class QGridLayout;
class QLabel;
class QLineEdit;

namespace mitcad {

class PickerComboBox;

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
  // when none is chosen), their pickers' keys ("<place>/<id>"; empty for
  // none), the level that chose each and its name.
  QString printer;
  QString printerId;
  QString printerLevel;
  QString printerOwner;
  QString process;
  QString processId;
  QString processLevel;
  QString processOwner;
  // The printers and processes it can choose, as Send to Slicer's pickers
  // list them (mitcad#1239, profilePickerRows): the design's and the
  // libraries'; the printers under their vendors, the processes under
  // their printers, the body's printer's first (mitcad#1216), Any printer
  // last; no headings when none has a group.
  QVector<ProfilePickerRow> printers;
  QVector<ProfilePickerRow> processes;
  QVector<MakeCardValue> values;

  bool operator==(const MakeCardData& other) const {
    return body == other.body && material == other.material && materialId == other.materialId &&
           materialLevel == other.materialLevel && materials == other.materials && printer == other.printer &&
           printerId == other.printerId && printerLevel == other.printerLevel && printerOwner == other.printerOwner &&
           process == other.process && processId == other.processId && processLevel == other.processLevel &&
           processOwner == other.processOwner && printers == other.printers && processes == other.processes &&
           values == other.values;
  }
  bool operator!=(const MakeCardData& other) const { return !(*this == other); }
};

// The card's one-line summary for the UI tests' log: "Body1: material PLA
// (component Root); printer Bench (component Root); process Standard
// (component Root); own: Walls = 5 (body Body1)".
QString makeCardSummary(const MakeCardData& data);

// A picker of the card for the UI tests' log (mitcad#1216, mitcad#1239):
// its rows under their headings, the processes "Bench Y: Shared, Strong |
// Bench X: Fine | Any printer: Draft", the printers "Acme Lab: Acme Lab
// X1 | Other: Bench"; the names alone, "Fine, Draft", without groups.
QString makeCardChoiceList(const QVector<ProfilePickerRow>& rows);

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
  // Whether Send to Slicer can start now (mitcad#1377): its button is
  // greyed where the command is.
  void setSendEnabled(bool enabled);
  // The places of its fields in the main window's coordinates, for the UI
  // tests: "Make card printer at x,y", "Make card process at x,y", "Make
  // card material at x,y", "Make card value perimeters at x,y", "Make
  // card remove perimeters at x,y" (a value set on the body), "Make card
  // add value at x,y", "Make card send to slicer at x,y" (mitcad#1377,
  // with " (disabled)" after it while greyed); each value's level icon, "Make card value
  // perimeters icon body"; and its rows' names from the top, "Make card
  // rows: Printer, Process, Material, Walls" (mitcad#1239).
  QStringList places() const;

signals:
  // Another material chosen (its id), or another printer or process
  // profile (its picker key, "<place>/<id>"): the printer for the whole
  // design, the process for the body.
  void materialChosen(const QString& id);
  void printerChosen(const QString& key);
  void processChosen(const QString& key);
  // A picker's last row, Printers… or Processes… (`kind` printer or
  // process): the manager on `key`, the one chosen now (empty: none).
  void managerRequested(const QString& kind, const QString& key);
  // A value typed and confirmed (Enter, or the field left changed).
  void valueEdited(const QString& set, const QString& key, const QString& text);
  // Add Print Value… clicked.
  void addValueRequested();
  // A body value's remove button clicked (not a pending one's: the card
  // drops that itself).
  void valueRemoved(const QString& set, const QString& key, const QString& name);
  // Send to Slicer clicked (mitcad#1377).
  void sendRequested();

private:
  void rebuild();
  QLabel* levelIcon(const QString& level, const QString& owner);
  // A printer or process picker (`kind`) of the rows, on `current`.
  PickerComboBox* profilePicker(const QString& kind, const QVector<ProfilePickerRow>& rows, const QString& current,
                                const QString& currentName);

  MakeCardData m_data;
  MakeCardValue m_pending; // key empty: none
  bool m_hasData = false;
  QGridLayout* m_grid = nullptr;
  PickerComboBox* m_printer = nullptr;
  PickerComboBox* m_process = nullptr;
  QComboBox* m_material = nullptr;
  QHash<QString, QLineEdit*> m_fields;    // the values' fields, by key
  QHash<QString, QWidget*> m_removes;     // the body values' remove buttons, by key
  QHash<QString, QString> m_icons;        // the values' level icons, by key
  QWidget* m_add = nullptr;               // Add Print Value…
  QWidget* m_send = nullptr;              // Send to Slicer (mitcad#1377)
  bool m_sendEnabled = true;
};

} // namespace mitcad
