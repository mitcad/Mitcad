// SPDX-License-Identifier: MIT
#pragma once

// The part picker (mitcad#418; app/COMMANDS.md, "Part picker"): the
// library's parts as a grid of pictures, each tile with the part's
// preview (`library_preview`, read as tiles come into sight), its name
// and main sizes, a star for the user's favourites and, for the parts the
// open design uses, how many times. On the left the groups: Favourites,
// In this design (when it has parts), All parts, then the libraries'
// categories as a tree of groups (mitcad#527: a click on a group shows its
// subgroups' parts too; folding is remembered; starred groups first on
// each level; PartGroups.hpp); above the grid a search, the slot
// system (BUILD parts), the supplier (mitcad#949, PartSuppliers.hpp: a
// part none of whose rows it sells is hidden; kept in the settings) and the
// tiles' size. Hovering a tile shows a larger
// picture with the supplier, the part number, the library and the licence.
//
// - PartGrid: the groups, filters and grid as a widget; Insert from
//   Library's dialog (LibraryBrowser) has one in place of its list.
// - PartPicker: a floating, non-modal window with a PartGrid that the
//   part-choosing commands open (BUILD's Profile, Panel, Linear Part and
//   Connector): a click on a tile chooses the part in the running command;
//   the window closes with the command unless it is pinned, and a pinned
//   one follows the next command that chooses parts. Its place, size and
//   tile size are kept in the settings (group "partPicker"). A part of an
//   open configuration table (mitcad#810) has its options under the grid,
//   a chooser per option filtered by the rules (OptionChooser.hpp): a
//   click on its tile chooses it with them, and a change of an option
//   chooses it again with the new ones (an options key, BuildCatalog.hpp).
//   The native macOS frontend hosts it in an AppKit utility panel that
//   leaves the keyboard with the document (showDialog, mitcad#819).
//
// The favourites and the order are PartFavourites' (PartFavourites.hpp),
// the design's parts usedLibraryParts' (UsedParts.hpp).

#include <functional>

#include <QAbstractListModel>
#include <QDialog>
#include <QHash>
#include <QImage>
#include <QJsonObject>
#include <QList>
#include <QPixmap>
#include <QPointer>
#include <QRect>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QTimer>
#include <QVector>
#include <QWidget>

#include "PartGroups.hpp"
#include "PartSuppliers.hpp"
#include "UsedParts.hpp"

class QComboBox;
class QFrame;
class QLabel;
class QLineEdit;
class QListView;
class QTreeWidget;
class QTreeWidgetItem;
class QSlider;
class QToolButton;

namespace mitcad {

class OptionChooser;

namespace build {
struct Part;
}

// A part the picker shows.
struct PickerPart {
  QString key;          // the caller's value (BUILD: build::Part::key); unique in a list
  QString library;      // the library's id
  QString libraryName;  // for people
  QString url;          // where the library was fetched from (empty: the id is enough)
  QString rev;          // the library's version the part comes from (empty: the newest)
  QString version;      // "v1.0.0 (3f9a2c1)"
  QString component;    // the component's id in the library
  QString config;       // a configuration row, empty for none
  QString name;
  QString category;     // the library's category: its home group
  QStringList categories; // every category it is in, its home first (mitcad#527)
  QList<PartGroupPath> groups; // the groups of its categories: names from the top
  QString standard;
  QString description;
  QString designation;
  QStringList keywords;
  QString supplier;
  QString partNumber;
  // The suppliers of its rows and its own (mitcad#949): the grid's
  // Supplier filter hides a part none of whose rows the chosen one sells.
  QList<PartSupplier> suppliers;
  QString license;      // SPDX id, empty for none
  QString sizes;        // the main sizes under the name ("30x30", "M5x16")
  QString system;       // the slot system(s) of a BUILD part ("B8 30")
  bool preview = true;  // the library has a picture of it
  bool rowPreview = false; // `config` has a picture of its own (library_preview with it, mitcad#484)
  bool defaultRow = false; // `config` is its component's default row (an old star marks it, mitcad#491)
  // Its configuration table as the library gives it: an open one is chosen
  // by its options under the grid (mitcad#810).
  QJsonObject table;
  QString refused;      // not choosable: why (greyed, the reason in the tooltip)
  // An index's library that is not fetched (Insert from Library, Community
  // Library): a tile of its own in the group "Libraries to get".
  bool libraryEntry = false;
};

// A component of `library_search` or `library_build_candidates` (with
// `build`: its section and system as the sizes); key "<library>|<component>".
PickerPart pickerPart(const QJsonObject& component);
// An answer of `library_search` or `library_build_candidates`: the
// components, then the refused ones (`refused`: greyed with their reason).
QList<PickerPart> pickerParts(const QJsonObject& answer);
// A BUILD picker's part (one row of a component), keyed by its key.
PickerPart pickerPart(const build::Part& part);
// BUILD pickers' parts with their categories, licences and pictures from
// one `library_search`.
QList<PickerPart> pickerParts(const QVector<build::Part>& parts);
// `library_build_candidates` with these fields (`role` with `system`, or
// `junction`; commands.md, "Library commands"), as the picker's parts.
QList<PickerPart> buildCandidates(const QJsonObject& fields);

// The grid's tiles: the parts the group and filters leave, in order.
class PartGridModel : public QAbstractListModel {
  Q_OBJECT

public:
  static constexpr int kKeyRole = Qt::UserRole;
  static constexpr int kImageRole = Qt::UserRole + 1;
  static constexpr int kCountRole = Qt::UserRole + 2;
  static constexpr int kStarRole = Qt::UserRole + 3;
  static constexpr int kRefusedRole = Qt::UserRole + 4;
  static constexpr int kSizesRole = Qt::UserRole + 5;
  // The part's whole picture, read now (the native grid's hover card,
  // mitcad#813); Qt's hover reads it through image().
  static constexpr int kPreviewRole = Qt::UserRole + 6;

  explicit PartGridModel(QObject* parent = nullptr);

  // `counts`: the design's uses of each shown tile by its key (mitcad#518:
  // per size, a tile of one configuration row counts that row's uses).
  void setShown(const QList<PickerPart>& shownParts, const QHash<QString, int>& counts);
  // The stars changed: the tiles are painted again, in the same places.
  void refreshStars();
  const PickerPart* part(int row) const;
  int rowOf(const QString& key) const;
  int rowCount(const QModelIndex& parent = QModelIndex()) const override;
  QVariant data(const QModelIndex& index, int role) const override;
  // kStarRole set by a presenter other than the tile delegate (the native
  // grid, mitcad#813): starRequested when it differs; nothing else is set.
  bool setData(const QModelIndex& index, const QVariant& value, int role) override;
  // The part's picture, read now when it is not read yet (the hover's).
  QImage image(const PickerPart& shownPart) const;
  // The tiles' size: the pictures are scaled to it.
  void setTileSize(int size);

signals:
  // A tile's star asked on or off through setData: PartGrid toggles it.
  void starRequested(const QString& key);

private:
  void loadSome();

  QList<PickerPart> m_shown;
  QHash<QString, int> m_counts;  // tile key -> uses
  // The parts whose pictures are wanted, in the order the view asked.
  mutable QStringList m_wanted;
  mutable QHash<QString, PickerPart> m_waiting;
  mutable QHash<QString, QPixmap> m_scaled;
  int m_tileSize = 96;
  mutable QTimer m_loader;
};

class PartGrid : public QWidget {
  Q_OBJECT

public:
  // The groups that are not the library's categories.
  static const QString kFavourites;
  static const QString kInDesign;
  static const QString kAll;
  static const QString kLibraries;
  static const QString kOther;

  explicit PartGrid(QWidget* parent = nullptr);

  // The parts to choose from (the group, the filters and the current part
  // are kept while they are still there). Signals nothing.
  void setParts(const QList<PickerPart>& parts);
  // The parts of another command (a pinned picker follows it): as setParts,
  // but a group none of them is in goes back to All (within one list, a
  // search or a filter that empties the group keeps it).
  void setOtherParts(const QList<PickerPart>& parts);
  const QList<PickerPart>& allParts() const { return m_parts; }
  // The design's library parts (In this design, the tiles' xN): a tile of
  // one configuration row counts the uses of that row (mitcad#518), a tile
  // without a row its component's.
  void setUsedParts(const QList<UsedPart>& used);
  // The tile of `key` current (empty: none); `notify`: currentChanged.
  void setCurrent(const QString& key, bool notify = false);
  // The tile of `key` current and scrolled into sight (mitcad#452): when
  // the group or the search leaves it out, All parts without the search.
  // Signals nothing; false when no tile shows it.
  bool reveal(const QString& key);
  // The first tile shown current (none when nothing is shown).
  void selectFirst(bool notify = false);
  QString currentKey() const { return m_current; }
  const PickerPart* currentPart() const;
  // The group shown (kFavourites, kInDesign, kAll, kLibraries, kOther or a
  // group's key, groupKey: "wood/dimensioned timber"); an unknown one
  // shows All parts.
  void showGroup(const QString& group);
  QString groupShown() const { return m_group; }
  void setSystem(const QString& system);
  // The Supplier filter (mitcad#949): a supplier's id, empty for All. Kept
  // in the settings; a supplier none of the parts has shows as All.
  QString supplierFilter() const;
  void setSupplierFilter(const QString& supplier);
  // External: the search text is the caller's to search with
  // (searchChanged), the grid does not filter by it; else the grid keeps
  // the parts whose fields have every word.
  void setExternalSearch(bool external) { m_externalSearch = external; }
  QLineEdit* searchField() const { return m_search; }
  // A widget beside the search (the browser's licence filter).
  void addFilter(QWidget* widget);
  QListView* view() const { return m_view; }
  // The keys of the tiles shown, in order.
  QStringList shownKeys() const;
  // Why no tile is shown although parts are listed (mitcad#1015): the
  // group shown has none of them, under the search and the filters while
  // they are on ("No parts in Wood with this search and these
  // filters."). Empty while tiles are shown or no part is listed.
  QString emptyReason() const;
  // Stars the current part, or turns its star off. Only the star changes
  // (mitcad#517): the tiles and the groups stay where they are, the
  // starred-first order applies the next time the list is made again.
  void toggleStar();
  // Stars the current group (a library's group) and its subgroups, or
  // turns their stars off; the groups stay where they are as well. The
  // native group list calls it by name (nativeStarMethod, mitcad#813).
  Q_INVOKABLE void toggleGroupStar();
  // Logs where the search, Star, the groups and the tiles in sight are,
  // for UI tests: "Part picker place search at <at(widget, point)>",
  // "Part picker group row <group> at ...", "Part picker place group fold
  // <group> at ..." (a group with subgroups), "Part picker tile
  // <library>/<component> at ..." (PartPicker gives `at`).
  void logPlaces(const std::function<QString(const QWidget*, const QPoint&)>& at) const;

signals:
  // The tiles were made again (a group, a search, the system, the parts).
  void tilesChanged();
  // A star was turned on or off: the tiles and the groups stay as they are
  // (mitcad#517).
  void starsChanged();
  // A group was folded or unfolded: the groups' rows moved (mitcad#527).
  void placesChanged();
  void searchChanged(const QString& text);
  // The Supplier filter changed (its id, empty for All).
  void supplierChanged(const QString& supplier);
  // Another tile became current (a click, the arrow keys).
  void currentChanged(const QString& key);
  // A tile clicked (not on its star).
  void clicked(const QString& key);
  // A tile double-clicked, or Return on it.
  void activated(const QString& key);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  void hideEvent(QHideEvent* event) override;

private:
  // The groups' tree from the parts, then its rows (showGroups); logs
  // "Part picker groups: ...".
  void fillGroups();
  // The groups' rows (mitcad#1015): each ends with its count under the
  // search and the filters ("Wood (12)"), a row with none greyed; the
  // empty ones hidden (hiddenGroups), the chosen one kept.
  void showGroups(const QHash<QString, int>& counts);
  // After a search, a filter or another group (refill): the rows again
  // when the rows to show changed (later when a group is being chosen,
  // so that the click finds its row), else only their counts; logs the
  // groups when they changed.
  void updateGroups();
  // The parts each row would show under the search and the filters, by the
  // row's key (kFavourites, ..., a group's key with its subgroups').
  QHash<QString, int> countGroups() const;
  // The keys of the rows shown with these counts.
  QSet<QString> shownGroups(const QHash<QString, int>& counts) const;
  // A row's name with its count, and the count for the log and the grey.
  void showCount(QTreeWidgetItem* item, const QHash<QString, int>& counts) const;
  // A search or a filter off its "all" choice: empty groups hide.
  bool filtering() const;
  void fillSystems();
  // The Supplier filter's choices from the parts (mitcad#949).
  void fillSuppliers();
  // The tiles again from the parts, the group and the filters (logged).
  // The current part stays when it is still shown; else, with `notify`,
  // the first tile becomes current (currentChanged), without it none.
  void refill(bool notify);
  // The stars changed (any grid's): the tiles' and the groups' stars are
  // painted again without moving anything (mitcad#517).
  void refreshStars();
  void refreshStarButton();
  // "Part picker tiles[ kept]: <tile> [xN], ..." for UI tests: the tiles in
  // their order (the first 24) with their use counts.
  void logTiles(bool kept) const;
  // The groups in the order shown, by name, for the log; with `counts`
  // each with its count (mitcad#1015: "Wood (2)"); a folded one with
  // " (folded)", its subgroups left out.
  QString groupsText(bool counts) const;
  void star(const QString& key);
  void starGroup(const QString& group);
  // A library's group starred: by its key, or (kept before mitcad#527) by
  // the id of a category that is the group.
  bool groupStarred(const QString& group) const;
  // The group's row (any level), null when it has none.
  QTreeWidgetItem* groupItem(const QString& group) const;
  // Whether the part is in the group shown (or one of its subgroups).
  bool inShownGroup(const PickerPart& part) const;
  // A group folded or unfolded by the user: kept in the settings.
  void groupFolded(QTreeWidgetItem* item, bool folded);
  void step(int by);
  void setTileSize(int size);
  void showHover(const QModelIndex& index, const QPoint& global);
  void hideHover();
  QString groupName(const QString& group) const;
  // The name on the group's row (without its count).
  QString rowName(const QString& group) const;
  bool passes(const PickerPart& part) const;
  // Whether the part is listed in the group (kFavourites, ..., or a group's
  // key with its subgroups), before the search and the filters.
  bool inGroupOf(const PickerPart& part, const QString& group) const;
  // Whether the part passes the Supplier and System filters and the search.
  bool passesFilters(const PickerPart& part) const;
  // The design's uses of the part: of its configuration row for a tile of
  // one (mitcad#518; the default row's include the uses placed without a
  // row), else of its component.
  int countOf(const PickerPart& part) const;

  QList<PickerPart> m_parts;
  UsedCounts m_used;
  QString m_group;
  // The shown group's names and order, so that it stays while a search
  // empties it (mitcad#527).
  PartGroupPath m_groupPath;
  PartGroupTree m_tree;
  // What the parts have besides the tree's groups: In this design, Other,
  // Libraries to get (their rows, mitcad#418).
  bool m_anyInDesign = false;
  bool m_anyOther = false;
  bool m_anyLibraries = false;
  // The keys of the groups' rows made, and the groups last logged.
  QSet<QString> m_groupRows;
  QString m_groupsLogged;
  // A group is being chosen (its row's click): the rows are made again
  // after it.
  bool m_choosingGroup = false;
  QString m_current;
  bool m_externalSearch = false;
  bool m_filling = false;
  // A click on a tile's star: not a click on the tile.
  bool m_starHit = false;
  int m_tile = 96;
  QTreeWidget* m_groups = nullptr;
  QLineEdit* m_search = nullptr;
  QComboBox* m_system = nullptr;
  QLabel* m_systemLabel = nullptr;
  QComboBox* m_supplier = nullptr;
  QLabel* m_supplierLabel = nullptr;
  QString m_wantedSupplier; // the settings' or the user's, also while no part has it
  QSlider* m_size = nullptr;
  QWidget* m_filters = nullptr;
  QListView* m_view = nullptr;
  PartGridModel* m_model = nullptr;
  QToolButton* m_star = nullptr;
  QFrame* m_hover = nullptr;
  QLabel* m_hoverImage = nullptr;
  QLabel* m_hoverText = nullptr;
  QString m_hoverKey;
};

// What a part-choosing command asks the picker for.
struct PickerRequest {
  QString command;           // the command's id ("build.profile"): the picker closes with it
  QString title;             // what is chosen, in the window's title ("Profile")
  QList<PickerPart> parts;   // what the command takes; refused ones greyed with their reason
  QString current;           // the key chosen now
  QString system;            // the system filter to start with (empty: all)
  // The command's input the choice goes into, when not its part (a Timber
  // frame's fastener, mitcad#497); empty for the part.
  QString input;
  // The BUILD role the parts are of ("profile", "timber", ...), when the
  // caller can list them again (mitcad#576: new library versions that came
  // in the background); empty when it cannot.
  QString role;
};

class PartPicker : public QDialog {
  Q_OBJECT

public:
  // The picker of a main window, made the first time it is asked for.
  static PartPicker* of(QWidget* window);
  explicit PartPicker(QWidget* parent = nullptr);
  ~PartPicker() override;

  // Shows the request's parts, without taking the focus from the view. A
  // pinned picker that is open follows the new command.
  void ask(const PickerRequest& request);
  // New parts for the same command (after a placement, another system).
  void setParts(const QList<PickerPart>& parts, const QString& current = QString());
  // The same parts read again (a library was updated): the part chosen,
  // the group and the filters stay.
  void refreshParts(const QList<PickerPart>& parts);
  // The BUILD role of the parts asked for (PickerRequest::role).
  QString role() const { return m_request.role; }
  // The part chosen in the command changed elsewhere (its panel).
  void setCurrent(const QString& key);
  // The command ended: the picker closes unless it is pinned.
  void commandEnded(const QString& command);
  QString command() const { return m_request.command; }
  // The input the choice goes into (PickerRequest::input; empty: the part).
  QString input() const { return m_request.input; }
  bool isPinned() const;
  void setPinned(bool on);
  // How In this design is read: when the picker shows and on
  // designChanged() while it is open.
  void setDesignParts(std::function<QList<UsedPart>()> read);
  // The design changed (a part placed or removed): In this design again.
  void designChanged();
  PartGrid* grid() const { return m_grid; }

signals:
  // A part chosen with a click (or Return) for `command`.
  void chosen(const PickerPart& part, const QString& command);

protected:
  void showEvent(QShowEvent* event) override;
  void hideEvent(QHideEvent* event) override;
  void moveEvent(QMoveEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;

private:
  void choose(const QString& key);
  // The options under the grid for the tile `key` (mitcad#810): shown for
  // a part of an open table, else hidden; `parameters` to start with.
  void showOptions(const QString& key, const QJsonObject& parameters = QJsonObject());
  // The tile of a key: an options key's tile of its row, else of its
  // component (BuildCatalog.hpp, optionsKey); a tile's key as it is.
  QString tileOf(const QString& key) const;
  void readDesign();
  void saveGeometryLater();
  // Logs where the pin and the grid's places are once laid out, for UI
  // tests (mitcad#418): in the main window's coordinates, as the panel's
  // places, and in the picker's own after "; in the picker".
  void logPlacesLater();
  void logPlaces() const;
  // The window's place and size in the main window, where its places are
  // logged (mitcad#528: logged again when it moved).
  QRect placesGeometry() const;

  PartGrid* m_grid = nullptr;
  QToolButton* m_pin = nullptr;
  // An open table's options (mitcad#810), for the tile m_optionsFor; the
  // tile chosen with them last, which follows a change of an option.
  OptionChooser* m_options = nullptr;
  QString m_optionsFor;
  QString m_chosenTile;
  bool m_showingOptions = false; // the options of another tile come in: nothing chosen
  PickerRequest m_request;
  std::function<QList<UsedPart>()> m_readDesign;
  bool m_designStale = true;
  bool m_restored = false;
  QTimer m_saveTimer;
  QTimer m_placesTimer;
  // The check after a move or a resize, and where the places were logged.
  QTimer m_movedTimer;
  mutable QRect m_loggedGeometry;
};

} // namespace mitcad
