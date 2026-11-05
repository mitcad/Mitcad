// SPDX-License-Identifier: MIT
#pragma once

#include <QColor>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QVector>

class QCheckBox;
class QComboBox;
class QHBoxLayout;
class QPalette;
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
// of it (the pickers), or each where it lives (queryPlaceItems, the
// managers' groups, mitcad#1135): built-in (`mitcad`, read-only), My
// library (`user`), This project (`project`) or This design (`design`).
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
  // By place (queryPlaceItems, mitcad#1135): a library item a nearer
  // library has too; the design's copy of a library item and whether the
  // design uses it; a design's copy's library item, by place and name.
  bool shadowed = false;
  QString copy;
  bool copyUsed = false;
  QString sourcePlace;
  QString sourceName;
  // A base library item the user hid (mitcad#1270, hiddenItemIds).
  bool hidden = false;
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
// any): the rows in the query's order, less the base library items the
// user hid that the design does not use (mitcad#1270, withoutHidden): the
// pickers' list. Sets `error` when it was refused.
QVector<LibraryItem> queryLibraryItems(const CommandContext& model, const QString& kind, const QString& project,
                                       QString* error = nullptr);
// The same by place (`by_place`, mitcad#1135: the managers' list model):
// every item of the design, then each library's own items, the project's,
// the user's and the built-in ones; `places` (when given) gets the places
// in that order (design, project in a project, user, mitcad). Hidden base
// items are listed too, marked `hidden` (the managers' Show hidden).
QVector<LibraryItem> queryPlaceItems(const CommandContext& model, const QString& kind, const QString& project,
                                     QStringList* places = nullptr, QString* error = nullptr);
// By key; null when it is not there.
const LibraryItem* findItem(const QVector<LibraryItem>& items, const QString& key);
// The row of the design's copy `designId`; null when none.
const LibraryItem* findDesignCopy(const QVector<LibraryItem>& items, const QString& designId);

// A place as the UI names it, the same in the managers' groups, the
// pickers' tags and the menus that ask where (mitcad#1135): This design,
// This project, My library, Built-in.
QString placeName(const QString& place);
// A place's group heading in the managers with its count: "This design (2)".
QString placeHeading(const QString& place, int count);
// The places New, Duplicate, Copy to and Import from Slicer can put an
// item in, in the lists' order: This design (with a design), This project
// (in a project), My library.
QStringList writablePlaces(bool hasDesign, bool inProject);
// Where New, Duplicate and Import from Slicer put an item unless the user
// chooses another (mitcad#1135): This design with a design, else My
// library.
QString defaultPlace(bool hasDesign);
// Asks where among `places` (writablePlaces), a menu at `anchor` (or at
// `at`, global) with `current`, the default, first and the others in the
// lists' order, logged "<logName> menu: This design | This project | My
// library" and "<logName> chosen: My library"; one place is taken at
// once. Empty when the menu was closed without a choice.
QString choosePlaceAmong(QWidget* anchor, const QString& logName, const QStringList& places, const QString& current,
                         const QPoint* at = nullptr);
// The editor's one line above the settings (mitcad#1135): where the
// selected item lives and whom an edit reaches, "Editing: this project's
// library — every design of the project uses it".
QString editingLine(const LibraryItem& item);
// The managers' search: an item whose name holds `text` (any case);
// every item for an empty text.
bool matchesSearch(const LibraryItem& item, const QString& text);
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

// The header of the managers' editor (mitcad#655, mitcad#1135): the
// item's name to edit (bold, "Name" as its placeholder), then its states
// as quiet grey text, one form in the three managers: "● Used in this
// design" (`used`, the lists' mark in words), "Built-in" with a lock and a
// Duplicate button (`builtIn`); else Copy to ▾ (`copyTo`, a menu of the
// other places). Under it `editing`, the line editingLine gives.
QHBoxLayout* itemHeader(QLineEdit** name, QLabel** used, QWidget** builtIn, QPushButton** duplicate,
                        QPushButton** copyTo, QLabel** editing);
// The places Copy to offers for `item`: writablePlaces less its own; none
// for a built-in one.
QStringList copyTargets(const LibraryItem& item, bool hasDesign, bool inProject);
// The command that copies `item` to `place` (mitcad#1135), keeping a link
// to its source only as before: a library item into the design is
// `copy_from_print_library` (the design's copy of it), a design's item into
// a library `save_to_print_library` (the design's copy then names it), a
// library item into another library `create_material` or
// `create_print_profile` based on it, by its name.
QJsonObject copyCommand(const LibraryItem& item, const QString& place, const QString& project);
// Marks a list row as the design's (mitcad#655: the chosen printer or
// process, a material the bodies use): bold, a small check after the
// name, `tooltip` on both; unmarks it when not.
void markInDesign(QTreeWidgetItem* row, bool used, const QString& tooltip);
// The item lists of Materials and Printers and Processes (mitcad#655): the
// selected row as readable while the list has no keyboard as with it.
void styleItemList(QTreeWidget* list);
// Adds a heading of the items after it to a drop-down (mitcad#1216: a
// process picker's printer): small, bold, not a choice; its data
// choiceHeadingPrefix() and the title.
void addComboHeading(QComboBox* combo, const QString& title);

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

// Hidden built-ins (mitcad#1270): the base library's printers, processes
// and materials the user does not use, out of the managers' lists (unless
// Show hidden) and every picker. The ids are a user setting
// (QSettings `printLibrary/hidden_<kind>`), never in a design or a
// project; a design that uses a hidden item keeps it as its own.
//
// Whether `item` can be hidden: an item of the base library (not the
// built-in materials MitCad itself has, nor the user's or the project's,
// which keep Delete).
bool canHide(const LibraryItem& item);
// The hidden ids of a kind (material, printer or process).
QStringList hiddenItemIds(const QString& kind);
// Hides or shows `item` again (canHide's only), logged "<logName> hidden:
// <name>" or "<logName> unhidden: <name>".
void setItemHidden(const LibraryItem& item, bool hidden, const QString& logName);
// The pickers' list: `items` less the hidden ones the design neither has a
// copy of nor uses.
QVector<LibraryItem> withoutHidden(const QVector<LibraryItem>& items);
// Whether My library or This project has one of `items` (the pickers' or
// the managers' rows of a kind): the built-ins then go out of the way, the
// managers' Built-in group folded at the opening and the pickers' built-ins
// last under a Built-in heading.
bool hasOwnLibraryItems(const QVector<LibraryItem>& items);
// The managers' Hide button (`printManagerHide`) and Show hidden box
// (`printManagerShowHidden`), and the button for `item`: shown for an item
// canHide allows, Unhide for a hidden one.
QPushButton* makeHideButton();
QCheckBox* makeShowHiddenBox();
void showHideButton(QPushButton* hide, const LibraryItem* item);
// A hidden item's row while Show hidden lists it: grayed, its tooltip
// saying so.
void markHiddenRow(QTreeWidgetItem* row, const QPalette& palette);
// A row's context menu in the managers (mitcad#1270) at `global`:
// Duplicate, then Hide or Unhide for a base item, else Delete (disabled
// when read only); logged "Context menu: Duplicate | Hide". What was
// chosen.
enum class ItemMenuChoice { None, Duplicate, Hide, Delete };
ItemMenuChoice itemContextMenu(QWidget* parent, const LibraryItem& item, const QPoint& global);

} // namespace mitcad
