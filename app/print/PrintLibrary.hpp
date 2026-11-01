// SPDX-License-Identifier: MIT
#pragma once

#include <QColor>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QVector>

class QHBoxLayout;
class QLabel;
class QLineEdit;
class QPoint;
class QPushButton;
class QTreeWidget;
class QTreeWidgetItem;
class QWidget;

namespace mitcad {

class CommandContext;
class PrintKeyTable;

// An item the user can choose (mitcad#655: a row of the `library_items`
// query): a material, printer or process, one row with the design's copy
// of it. The user sees two states only: built-in (Mitcad's, read-only)
// or their own, saved in a library (`user`: My library, `project`: the
// project's) or not yet (`design`). The model's place stays for commands
// and logs; the windows show no places.
struct LibraryItem {
  QString id;   // in its place
  QString name;
  QString kind;    // material, printer or process
  QString place;   // mitcad, user, project or design
  QString library; // what a command names: base, user or project; empty for the design's and the built-ins
  bool readOnly = false;
  QString inDesign; // the design's copy's id; empty without one
  bool changed = false; // its library item was changed elsewhere (another design)
  bool chosen = false; // the design's printer or process
  QString group;       // materials: metals, plastics, fdm_plastics (the filaments), resins, other
  QString materialClass; // materials: metal, plastic, fdm_plastic, resin, other (mitcad#655, mitcad#1102)
  bool classFixed = false; // materials: print settings make it an FDM plastic (mitcad#1102)
  QColor displayColor; // materials; invalid without one
  QJsonObject row;     // as the query gave it

  // Its key in the windows and the panel: "<place>/<id>".
  QString key() const { return place + QLatin1Char('/') + id; }
  // The id the design names it by: its copy's, or its own when it is the
  // design's or a built-in; empty when the design has none of it.
  QString designId() const;
  // The user's own item saved in My library or the project's library.
  bool saved() const;
  // The user's own item not saved in a library yet: Save to offers it.
  bool unsaved() const;
};

// The `library_items` query of a kind (`project` the design's project, if
// any): the rows in the query's order. Sets `error` when it was refused.
QVector<LibraryItem> queryLibraryItems(const CommandContext& model, const QString& kind, const QString& project,
                                       QString* error = nullptr);
// By key; null when it is not there.
const LibraryItem* findItem(const QVector<LibraryItem>& items, const QString& key);
// The row of the design's copy `designId`; null when none.
const LibraryItem* findDesignCopy(const QVector<LibraryItem>& items, const QString& designId);

// A library as the UI names it where one is chosen or named (Save to's
// menu, Delete's question): My library, Project; Built-in, This design.
QString placeName(const QString& place);
// A material group's title (Metals, General Plastics, FDM Plastics, Resins,
// Other), a class's (Metal, General Plastic, FDM Plastic, Resin, Other) and
// the classes and the groups in their order (mitcad#655, mitcad#1102).
QString materialGroupTitle(const QString& group);
QString materialClassTitle(const QString& materialClass);
const QStringList& materialClasses();
const QStringList& materialGroups();

// Names the item in a command: `field` (`id`, `material`, `value`) and,
// for an item of a library, `library` and the project.
void nameItem(QJsonObject& command, const LibraryItem& item, const QString& project,
              const QString& field = QStringLiteral("id"));
// The item's fields for its editor: the row's (the design's copy's when it
// has one), else its library's (`print_library`'s `items`), else the
// design's (`materials` or `print_profiles`).
QJsonObject itemFields(const LibraryItem& item, const QJsonArray& designItems, const QJsonObject& library);
// The fields the design prints with (mitcad#655): the design's copy when
// it has one, else as itemFields.
QJsonObject designFields(const LibraryItem& item, const QJsonArray& designItems, const QJsonObject& library);

// The header of Materials' and Printers and Processes' editor (mitcad#655):
// the item's name to edit (bold, "Name" as its placeholder), then its
// states as quiet grey text, one form in both windows: "● Used in this
// design" (`used`, the lists' mark in words), "In My library" or "In this
// project's library" (`saved`, showSavedState), "Built-in" with a lock and
// a Duplicate button (`builtIn`); else Save to.
QHBoxLayout* itemHeader(QLineEdit** name, QLabel** used, QLabel** saved, QWidget** builtIn, QPushButton** duplicate,
                        QPushButton** saveTo);
// Shows the saved state of `item` (none: hidden).
void showSavedState(QLabel* saved, const LibraryItem* item);
// Save to's text: "Save to My library", in a project "Save to ▾" (a menu
// of My library and Project).
QString saveToText(bool inProject);
// Marks a list row as the design's (mitcad#655: the chosen printer or
// process, a material the bodies use): bold, a small check after the
// name, `tooltip` on both; unmarks it when not.
void markInDesign(QTreeWidgetItem* row, bool used, const QString& tooltip);
// The item lists of Materials and Printers and Processes (mitcad#655): the
// selected row as readable while the list has no keyboard as with it.
void styleItemList(QTreeWidget* list);

// Asks before Delete (mitcad#655), the same in every window, saying what
// follows and who it concerns, never a rule. An item saved in a library:
// "Delete Shop PETG from My library? This design keeps using it." (the
// second sentence only when the design uses it). The design's printer or
// process: "Delete Fine from this design? This design then uses Slicer
// default." The design's material: "Delete Shop PETG? Body1 and Body2
// then use Default (Steel)." (`users`: the bodies' names, "3 bodies" for
// more than two; `fallback`: what they then use). Logged "<logName>
// delete: <question>". True to go on.
bool confirmDelete(QWidget* parent, const LibraryItem& item, const QString& logName,
                   const QStringList& users = {}, const QString& fallback = QString());

// A saved item changed in another design (`changed`, mitcad#655): asks
// once "Shop PETG was changed in another design. Use the changes or keep
// this design's settings?" with Use the changes and Keep this design's settings (the
// default, also Escape), when the design uses the item or it is edited,
// and gives the command of
// the answer: `update_from_print_library` or
// `decline_print_library_update` (the design remembers it: not asked
// again until the library changes again). Logged "<logName> changed
// elsewhere: <name>" and "<logName> changed elsewhere: use | keep". The
// item is not asked again in this session either way (changeAsked).
QJsonObject askChangedElsewhere(QWidget* parent, const LibraryItem& item, const QString& logName,
                                const QString& project);
bool changeAsked(const LibraryItem& item);
// Whether the design uses the item: bodies or components (a material),
// the design or an owner (a printer or process).
bool usedByDesign(const LibraryItem& item);

// Where Save to puts an item: My library (`user`) or the project's
// (`project`). Without a project My library at once; in a project a menu
// at `anchor` that starts on the place chosen last (My library the first
// time), logged as "<logName> menu: My library | Project" (at `at`,
// global, when given). Empty when the menu was closed without a choice.
QString choosePlace(QWidget* anchor, const QString& logName, bool inProject, const QPoint* at = nullptr);
// The slicer whose print settings the design's printer came from
// (mitcad#655): the slicer its `printer_settings_id` names, else the first of
// its `raw_slicers`, else Bambu Studio (`bambu_studio`): what a 3MF without
// a slicer of a known dialect is written in, and what the editors offer.
QString designDialectOf(const CommandContext& model);
// A model refusal in plain words with a next step (mitcad#655): a write
// failure ("Could not save to My library. Check that the folder can be
// written.", the place from `command`'s `library`), a missing item, a
// built-in item, no project; else the key table's plain names without
// paths, or "Could not change Shop PETG. Check its values and try again."
// (`name`) when that is long. No
// ids.
QString plainModelError(const QString& error, const PrintKeyTable& keys, const QJsonObject& command,
                        const QString& name = QString());

} // namespace mitcad
