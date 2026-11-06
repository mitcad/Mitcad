// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <vector>

#include <QJsonObject>
#include <QHash>
#include <QJsonValue>
#include <QPair>
#include <QPoint>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QVector>
#include <QWidget>

#include "PrintKeys.hpp"

class QCheckBox;
class QLabel;
class QLineEdit;
class QScrollArea;
class QTabBar;
class QToolButton;
class QVBoxLayout;

namespace mitcad {

class SettingField;

// The settings form (mitcad#655), one widget for a set of print settings
// wherever they are edited: a material's print settings (the filament
// set) in Materials, a printer's or a process's in Printers and
// Processes.
//
// - Search settings (all keys, also advanced ones) and the Advanced switch
//   (remembered per user, `print/advancedSettings`) on top.
// - The categories as tabs, their sub-groups as headed sections in the key
//   table's order (`print_keys`' `layout`); each key a row of its plain
//   name and its SettingField. The search finds the plain names.
// - Advanced off hides the advanced keys unless the item sets them:
//   nothing that changes the print is hidden. A sub-group or a tab with
//   nothing to show is hidden, and one tab alone shows no tab bar (a
//   printer's one category, a metal's General).
//
// The form changes nothing itself: valueChanged gives a field's value
// (null: clear it) to the window, which runs the model command and shows
// the values the model then has (setValues).
class SettingsForm : public QWidget {
  Q_OBJECT

public:
  explicit SettingsForm(QWidget* parent = nullptr);
  ~SettingsForm() override;

  // The keys of `set` (printer, process, filament), built once per set and
  // slicer: `dialect` the design's (designDialectOf), whose percentages a
  // number-or-percentage field offers (mitcad#655; empty: all).
  void setKeys(const PrintKeyTable& table, const QString& set, const QString& dialect = QString());
  QString set() const { return m_set; }
  // Keys never shown (the material's density stands for filament_density).
  void setHiddenKeys(const QStringList& keys);
  // Keys kept per slicer that the form edits, a row per slicer that has
  // the key, named by the slicer, its field that slicer's (SettingField's
  // `slicer`; mitcad#1062: a material's filament preset); shown with
  // Advanced off too, found by the key's name or the slicer's. Other keys
  // kept per slicer (a printer's preset) are never shown. Before setKeys.
  void setSlicerKeys(const QStringList& keys);
  // The names a slicer's row offers for a key when its list opens (the
  // installed slicer's presets); none: a typed name only.
  std::function<QStringList(const QString& key, const QString& slicer)> slicerChoices;
  // The item's values by key. Its slicer-specific keys are not shown
  // (mitcad#655: an import's Details… lists them).
  void setValues(const QJsonObject& values);
  // The window's own fields (mitcad#655: Materials' Name, Density,
  // Appearance and Class) as a first tab `title`, the print settings' tabs
  // beside it; `page` is shown in the tab's place.
  void setGeneralPage(QWidget* page, const QString& title);
  // The names of the general page's fields, which Search finds too
  // (mitcad#655): a match shows the page with the settings found.
  void setGeneralNames(const QStringList& names);
  // Called with Search's text when it finds the general page's fields,
  // empty otherwise (mitcad#1228): the window shows only the fields found
  // there, so the general page stays short and the settings found below
  // it in sight. While searching the page takes only its height.
  std::function<void(const QString& search)> filterGeneral;
  // Shows the general page's tab (its fields take the keyboard).
  void showGeneral();
  // A page of the window's own after the print settings' tabs, its tab
  // `title` (mitcad#1054: a material's Variants), shown with the print
  // settings (setPrintShown) and not while searching.
  void setEndPage(QWidget* page, const QString& title);
  // The object names of the search, the tabs and the fields start with
  // `prefix` ("variant": variantSettingsSearch, variantSettingsTabs,
  // variantSetting_<key>; mitcad#1054) for a second form in a window;
  // before setKeys. Empty: settingsSearch, settingsTabs,
  // printSetting_<key>.
  void setNamePrefix(const QString& prefix);
  // Whether the print settings are shown at all (mitcad#655: a material
  // that cannot have them, a metal, shows the general page alone).
  void setPrintShown(bool shown);
  bool printShown() const { return m_printShown; }
  // What unset keys stand for (SettingField::setPlaceholderValue).
  // `words`: what names them, by key (a colour: "From appearance").
  void setPlaceholderValues(const QJsonObject& values, const QHash<QString, QString>& words = {});
  // The own values that owners below the root set, by key: (owner's
  // name, value) (mitcad#655); each such field shows them as a grey hint,
  // "4 on Body1, 3 on Arm".
  void setOwnValues(const QHash<QString, QVector<QPair<QString, QJsonValue>>>& owners);
  // The values Update would bring, as hints after the fields of `values`
  // keys ("Newer: 245 °C"), `from` the place that has them (My library);
  // each tab with differences shows their count in its title ("Cooling
  // (2)"), General `general` of its own. The number of fields hinted.
  int setHints(const QJsonObject& values, const QString& from, int general = 0);
  // A refusal under a key's field; empty hides it.
  void setError(const QString& key, const QString& message);
  void setReadOnly(bool readOnly);
  // Shows a key (its tab, also when advanced) and gives its field the
  // keyboard.
  void focusKey(const QString& key);
  // The places of what is in sight, as "<prefix> field <key> at x,y"
  // (`<key>.<slicer>` for a slicer's row, `clear <key>` for ↺, `tab
  // <category>`, `search`, `advanced`); `at`
  // maps a point of a widget into the logged coordinates.
  QStringList places(const QString& prefix, const std::function<QString(QWidget*, const QPoint&)>& at) const;
  // The values set, in the form's order: "<key> = <text>" (a slicer's row
  // "<key>.<slicer> = <name>").
  QStringList rows() const;
  // Whether a key's row is shown now, and the tabs shown (category ids):
  // for the unit tests, as the filter leaves them.
  bool keyShown(const QString& key) const;
  QStringList shownTabs() const;

signals:
  void valueChanged(const QString& key, const QJsonValue& value);
  // A typed text that is not a value of its key: the one-line reason.
  void refused(const QString& key, const QString& reason);
  // The rows in sight changed (a tab, the search, Advanced, a scroll), or
  // their places did once the layouts settled (the list moved or resized).
  void layoutChanged();

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  struct Row {
    PrintKey key;
    QString slicer; // a slicer's row of a key kept per slicer (setSlicerKeys)
    QString title;  // the row's name: the key's, or the slicer's
    QString category;
    QString group;
    QWidget* line = nullptr;
    SettingField* field = nullptr;
  };
  struct Group {
    QString category;
    QString categoryTitle;
    QString id;
    QString title;
    QLabel* heading = nullptr;
    std::vector<int> rows;
  };

  void clearRows();
  void applyFilter();
  // The bar's tabs: the ones of `wanted` (ids), the current one kept.
  void syncTabs(const QStringList& wanted);
  QString tabTitle(const QString& id) const;
  bool rowShown(const Row& row, bool searching, const QString& search, const QString& category) const;

  QString m_set;
  QString m_dialect;
  QLineEdit* m_search = nullptr;
  QCheckBox* m_advanced = nullptr;
  QTabBar* m_tabs = nullptr;
  QScrollArea* m_scroll = nullptr;
  QWidget* m_page = nullptr;
  QVBoxLayout* m_rowsLayout = nullptr;
  std::vector<Row> m_rows;
  std::vector<Group> m_groups;
  QStringList m_categories; // the tabs' categories, in order
  QSet<QString> m_hidden;
  QStringList m_slicerKeys; // setSlicerKeys'
  QString m_forced; // a key focusKey shows whatever the filter says
  QWidget* m_general = nullptr; // the window's own page, if any
  QString m_generalTitle;
  QWidget* m_end = nullptr; // the window's page after the print tabs, if any
  QString m_endTitle;
  QString m_fieldPrefix = QStringLiteral("printSetting_");
  QStringList m_generalNames;
  QHash<QString, QString> m_tabTitles; // the print tabs' titles by category
  QHash<QString, int> m_hintCounts;     // setHints' marked tabs by id
  QString m_currentTab;                 // the tab shown, by id
  bool m_printShown = true;
  bool m_readOnly = false;
};

} // namespace mitcad
