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
// (MainWindowMake.cpp), each one undo step. The Properties window stays
// for full editing.

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
  QVector<MakeCardValue> values;

  bool operator==(const MakeCardData& other) const {
    return body == other.body && material == other.material && materialId == other.materialId &&
           materialLevel == other.materialLevel && materials == other.materials && printer == other.printer &&
           printerLevel == other.printerLevel && printerOwner == other.printerOwner && process == other.process &&
           processId == other.processId && processLevel == other.processLevel &&
           processOwner == other.processOwner && processes == other.processes && values == other.values;
  }
  bool operator!=(const MakeCardData& other) const { return !(*this == other); }
};

// The card's one-line summary for the UI tests' log: "Body1: material PLA
// (component Root); printer Bench (component Root); process Standard
// (component Root); own: Walls = 5 (body Body1)".
QString makeCardSummary(const MakeCardData& data);

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
  // The places of its fields in the main window's coordinates, for the UI
  // tests: "Make card material at x,y", "Make card process at x,y", "Make
  // card value perimeters at x,y".
  QStringList places() const;

signals:
  // Another material, or another process profile, chosen (its id).
  void materialChosen(const QString& id);
  void processChosen(const QString& id);
  // A value typed and confirmed (Enter, or the field left changed).
  void valueEdited(const QString& set, const QString& key, const QString& text);

private:
  void rebuild();
  QLabel* levelIcon(const QString& level, const QString& owner);

  MakeCardData m_data;
  bool m_hasData = false;
  QGridLayout* m_grid = nullptr;
  QComboBox* m_material = nullptr;
  QComboBox* m_process = nullptr;
  QHash<QString, QLineEdit*> m_fields; // the values' fields, by key
};

} // namespace mitcad
