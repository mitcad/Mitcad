// SPDX-License-Identifier: MIT
#pragma once

// The dialogs of component libraries (mitcad#64, mitcad#63):
// - LibrariesDialog: the sources (libraries and community indexes, on or
//   off, and Auto-update: fetched in the background at start-up,
//   mitcad#576), added by URL or folder, fetched when asked.
// - LibraryBrowser: Browse Components (mitcad#1283; Insert from Library and
//   the community library before), one window with the views Installed,
//   Community and Favorites: search with a licence filter, the results as
//   the part picker's grid of previews (PartGrid, mitcad#418: groups,
//   favourites, In this design), details with the licence and the
//   attribution, the version and the size (with the Standard and Supplier
//   filters, mitcad#949), the row's designation and its suppliers' articles
//   behind a button that opens their table (mitcad#979,
//   SupplierPartsDialog.hpp), linked or a copy; libraries a community index
//   lists, and their components, are fetched from there (Get Library, the
//   primary button while the selected item's library is not here), those
//   an enabled source has under another id left out.
// - LibraryPartsDialog: the design's library parts, each kept at the
//   version chosen for it: another version or size, what changes, the
//   update; and the parts list with the designations (a bill of
//   materials).
// - PublishLibraryDialog: the design added to a library of one's own (a
//   folder that is a Mitcad project with a git repository), recorded,
//   pushed, and the entry for a community index.

#include <functional>
#include <optional>

#include <QDialog>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QList>
#include <QMap>
#include <QString>
#include <QStringList>

#include "../library/OptionChooser.hpp"
#include "../library/SupplierPartsDialog.hpp"

class QCheckBox;
class QComboBox;
class QFormLayout;
class QLabel;
class QLineEdit;
class QListWidget;
class QPlainTextEdit;
class QPushButton;
class QRadioButton;
class QTabBar;
class QTableWidget;

namespace mitcad {

class PartGrid;
struct UsedPart;

// The size of a library part: a closed table's cascaded choices of its rows
// (Size, then Length), or an open table's chooser per option filtered by
// the rules (mitcad#810; app/library/OptionChooser.hpp).
using SizeChooser = OptionChooser;

class LibrariesDialog : public QDialog {
  Q_OBJECT

public:
  explicit LibrariesDialog(QWidget* parent = nullptr);

private:
  void reload();
  void addUrl(const QString& url);
  void fetch(bool all);
  void save();
  // The row's Auto-update greyed out while its source is off (mitcad#576).
  void greyAutoUpdate(int row);

  QTableWidget* m_table = nullptr;
};

// A library part chosen in the browser.
struct LibraryChoice {
  QString library;
  QString url;
  QString rev;
  QString version; // for people: "v1.0.0 (3f9a2c1)"
  QString component;
  QString name;
  QString config;
  // An open table's options as constants ({"Color": "'Black'", "Length":
  // "800 mm"}), insert_component's `parameters` (mitcad#810); empty for a
  // closed table. `config` is then the combination's row, if any.
  QJsonObject parameters;
  QString license;
  // The standard the part is named by (mitcad#949: the Standard filter's,
  // insert_component's `standard`); empty: its row's first.
  QString standard;
  bool link = true;
  // The manifest's mount frames of the part (mitcad#525): with any, Insert
  // from Library places it by one (Place Part).
  QJsonArray mounts;
};

class LibraryBrowser : public QDialog {
  Q_OBJECT

public:
  // The browser's views (mitcad#1283), the tabs above the grid:
  // - Installed: the fetched libraries' components, then the libraries the
  //   fetched indexes name that are not fetched ("Libraries to get");
  // - Community: every library the fetched indexes name first, each not
  //   fetched one with its components (the index's list, tiles in
  //   "Libraries to get" whose primary button is Get Library), then the
  //   fetched components (the community library before);
  // - Favorites: the starred components of the fetched libraries.
  enum class View { Installed, Community, Favorites };

  explicit LibraryBrowser(View view, QWidget* parent = nullptr);
  std::optional<LibraryChoice> choice() const { return m_choice; }
  // The open design's library parts: the grid's In this design (mitcad#418).
  void setUsedParts(const QList<UsedPart>& used);
  View view() const { return m_view; }
  // The view's name as its tab shows it, without the underline.
  static QString viewName(View view);

private:
  // The item of m_items the grid's current tile is, or -1.
  int currentIndex() const;
  // Another view (its tab): the search, the filters and the selected item
  // kept while it is listed there.
  void setView(View view);
  void search();
  void select();
  // The primary button follows the selected item (mitcad#1283): Get Library
  // (`get`) while its library is not here, else Insert; `enabled`
  // whether it can now. Logs "Library browser button: Get Library" or
  // "... Insert", with " (off)" while disabled, when that changes.
  void setPrimary(bool get, bool enabled);
  // The primary button pressed (or a tile activated): Insert or Get Library.
  void primary();
  // After Get Library on a component of an index's library (mitcad#1283):
  // that component of the fetched library selected; false when it is not
  // listed.
  bool selectFetched(const QJsonObject& wanted);
  void showVersion();
  // The chosen row's designation (following the Standard filter) under the
  // size (mitcad#949), and the button of its suppliers' articles
  // (mitcad#979).
  void showRowDetails();
  // The size's or the options' fields in the tab order after the version.
  void orderFields();
  void getLibrary();
  // Update: fetches every enabled source, reports the failures in one
  // warning and refreshes the results (mitcad#317).
  void updateLibraries();
  // The search again, the selected item kept when it is still listed.
  void refresh();
  void insert();
  // An item of the results the same after a new search.
  static QString itemKey(const QJsonObject& item);
  // Whether any enabled source has been fetched (the empty list's text).
  static bool anySourceFetched();

  View m_view = View::Installed;
  // The views as tabs (libraryView), in View's order.
  QTabBar* m_views = nullptr;
  QLineEdit* m_search = nullptr;
  QComboBox* m_license = nullptr;
  QCheckBox* m_unlicensed = nullptr;
  // The results as the part picker's grid (mitcad#418), keyed by itemKey.
  PartGrid* m_grid = nullptr;
  QStringList m_keys;
  QLabel* m_details = nullptr;
  QComboBox* m_version = nullptr;
  SizeChooser* m_size = nullptr;
  QLabel* m_rowDetails = nullptr;
  // Supplier Articles (n)… and what its table shows (mitcad#979).
  QPushButton* m_supplierButton = nullptr;
  SupplierPartsRequest m_supplierRequest;
  QRadioButton* m_link = nullptr;
  QRadioButton* m_copy = nullptr;
  // Insert or Get Library (setPrimary), the dialog's default button.
  QPushButton* m_primary = nullptr;
  bool m_primaryGets = false;
  QString m_primaryLogged;
  QJsonArray m_items;
  QJsonObject m_shown; // library_show of the selected item's version
  std::optional<LibraryChoice> m_choice;
};

// A change of a part the dialog asks for.
struct LibraryPartChange {
  QString component; // uid
  QString rev;       // empty: unchanged
  QString config;    // empty: unchanged
};

class LibraryPartsDialog : public QDialog {
  Q_OBJECT

public:
  // `parts`: the library_parts query's "parts"; `list`: the parts_list
  // query's "rows".
  LibraryPartsDialog(const QJsonArray& parts, const QJsonArray& list, QWidget* parent = nullptr);
  // The changes when accepted with Update.
  QList<LibraryPartChange> changes() const;

private:
  struct Part {
    QJsonObject part;   // from the query
    QJsonObject shown;  // library_show of the library (versions)
    QString rev;        // the chosen version (full id)
    QString config;     // the chosen row
  };
  void load(int index);
  void selectPart();
  void versionChosen();
  void showChanges();
  void checkNewer();
  void getMissing();

  QList<Part> m_parts;
  QTableWidget* m_table = nullptr;
  QComboBox* m_version = nullptr;
  SizeChooser* m_size = nullptr;
  // The chosen part's suppliers' articles behind a button (mitcad#949,
  // mitcad#979) and what its table shows.
  QPushButton* m_supplierButton = nullptr;
  SupplierPartsRequest m_supplierRequest;
  QPlainTextEdit* m_changes = nullptr;
  QPushButton* m_update = nullptr;
  QPushButton* m_get = nullptr;
  bool m_loading = false;
};

class PublishLibraryDialog : public QDialog {
  Q_OBJECT

public:
  // `design`: the open design as a single-file design; `preview`: an
  // image of the view; `author`: "Name <email>" for the version, or "".
  PublishLibraryDialog(std::function<QString()> design, std::function<QImage()> preview, QString author,
                       QWidget* parent = nullptr);

private:
  void folderChanged();
  void add();
  void push();
  void indexEntry();
  void say(const QString& text);

  std::function<QString()> m_design;
  std::function<QImage()> m_preview;
  QString m_author;
  QLineEdit* m_folder = nullptr;
  QLabel* m_status = nullptr;
  QLineEdit* m_libraryId = nullptr;
  QLineEdit* m_libraryName = nullptr;
  QComboBox* m_libraryLicense = nullptr;
  QLineEdit* m_libraryAuthor = nullptr;
  QLineEdit* m_id = nullptr;
  QLineEdit* m_name = nullptr;
  QLineEdit* m_category = nullptr;
  QLineEdit* m_standard = nullptr;
  QLineEdit* m_keywords = nullptr;
  QCheckBox* m_withPreview = nullptr;
  QLineEdit* m_remote = nullptr;
  QPlainTextEdit* m_log = nullptr;
};

} // namespace mitcad
