// SPDX-License-Identifier: MIT
#pragma once

// The dialogs of component libraries (mitcad#64, mitcad#63):
// - LibrariesDialog: the sources (libraries and community indexes, on or
//   off), added by URL or folder, fetched only when asked.
// - LibraryBrowser: Insert from Library and the community library: search
//   with a licence filter, previews, details with the licence and the
//   attribution, the version and the size, linked or a copy; libraries a
//   community index lists are fetched from there.
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

class QCheckBox;
class QComboBox;
class QFormLayout;
class QLabel;
class QLineEdit;
class QListWidget;
class QPlainTextEdit;
class QPushButton;
class QRadioButton;
class QTableWidget;

namespace mitcad {

// Cascaded choices of a configuration table's rows (Size, then Length):
// each selector a combo box of the values the rows with the earlier
// choices have.
class SizeChooser : public QWidget {
  Q_OBJECT

public:
  explicit SizeChooser(QWidget* parent = nullptr);
  // The table as library_show gives it ({"selectors", "rows", "default"});
  // `row`: the row to show first (else the default).
  void setTable(const QJsonObject& configurations, const QString& row = QString());
  // The chosen row's name; empty without a table.
  QString row() const { return m_row; }
  QList<QComboBox*> combos() const { return m_combos; }

signals:
  void rowChanged(const QString& row);

private:
  void fill(int from);
  void choose();

  QFormLayout* m_form = nullptr;
  QList<QComboBox*> m_combos;
  QStringList m_selectors;
  QJsonArray m_rows;
  QString m_row;
  bool m_filling = false;
};

class LibrariesDialog : public QDialog {
  Q_OBJECT

public:
  explicit LibrariesDialog(QWidget* parent = nullptr);

private:
  void reload();
  void addUrl(const QString& url);
  void fetch(bool all);
  void save();

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
  QString license;
  bool link = true;
};

class LibraryBrowser : public QDialog {
  Q_OBJECT

public:
  // `community`: titled Community Library, listing the libraries the
  // fetched indexes name first.
  explicit LibraryBrowser(bool community, QWidget* parent = nullptr);
  std::optional<LibraryChoice> choice() const { return m_choice; }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  void search();
  void select();
  void showVersion();
  void getLibrary();
  void insert();

  bool m_community = false;
  QLineEdit* m_search = nullptr;
  QComboBox* m_license = nullptr;
  QCheckBox* m_unlicensed = nullptr;
  QListWidget* m_results = nullptr;
  QLabel* m_details = nullptr;
  QComboBox* m_version = nullptr;
  SizeChooser* m_size = nullptr;
  QRadioButton* m_link = nullptr;
  QRadioButton* m_copy = nullptr;
  QPushButton* m_insert = nullptr;
  QPushButton* m_get = nullptr;
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
  QPlainTextEdit* m_changes = nullptr;
  QPushButton* m_update = nullptr;
  QPushButton* m_get = nullptr;
  bool m_loading = false;
};

class PublishLibraryDialog : public QDialog {
  Q_OBJECT

public:
  // `design`: the open design as a single project file; `preview`: an
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
