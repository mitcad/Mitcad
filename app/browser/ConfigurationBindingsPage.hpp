// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <utility>

#include <QHash>
#include <QString>
#include <QVector>

#include "BindingList.hpp"
#include "ConfigurationsDialog.hpp"

class QComboBox;
class QLabel;
class QLineEdit;
class QPushButton;
class QTableWidget;

namespace mitcad {

class DocumentHost;

// What the Bindings tab's buttons ask of the rest of the window
// (BrowserController): Edit… opens the same editor as the fx card's Edit…
// (the Appearance or Materials window for that target, Show When… or
// Suppress When…), Select in Browser selects the item and shows its row.
struct BindingActions {
  std::function<void(const BindingEntry&)> edit;
  std::function<void(const BindingEntry&)> select;
};

// Configurations › Bindings (mitcad#862): every binding of the design as a
// table (Item, Property, Follows, Now), filtered by Option (mitcad#968; All
// options or one option: the parameters the bindings use) and by an item
// search.
// A binding whose results name no appearance or material, or that the
// model warns about, is a warning row (`no appearance “paint_purple”`).
// Edit… (or a double-click), Select in Browser, and Remove Binding (the
// command with an empty expression: it keeps the value it gave, one undo
// step). The status line counts them (`7 bindings`, `2 of 7 bindings`,
// `, 1 warning`).
//
// Logged for UI tests: `Configurations binding <item> <property>: <follows>;
// now <now>[; warning <w>]` per row shown, `Configurations bindings status:
// <status>` when either changes, `Configurations bindings driven by
// <option|All options>, search '<text>': <status>`, `Configurations binding
// removed: <item> <property>` (or `… refused: <reason>`), and the places
// `Configurations binding <item> <property> at x,y`, `Configurations
// bindings driven by|search|edit|select|remove at x,y` in the main
// window's coordinates (`driven by` is the Option filter's key, kept for
// the tests), and `Configurations bindings filter label: Option:`.
class ConfigurationBindingsPage : public ConfigurationPage {
  Q_OBJECT

public:
  ConfigurationBindingsPage(DocumentHost& host, QWidget* parent = nullptr);

  void refresh() override;
  void setActions(BindingActions actions) { m_actions = std::move(actions); }
  // Shows the bindings an option drives (empty: all options).
  void showOption(const QString& option);
  // How many of the design's bindings use each parameter.
  QHash<QString, int> usage() const { return m_usage; }
  // The bindings that use a parameter, as `Body1 Appearance` lines.
  QStringList usersOf(const QString& parameter) const;

private:
  void fill();
  int selectedIndex() const;
  void editSelected();
  void selectSelected();
  void removeSelected();
  void updateButtons();
  void logPlaces();

  DocumentHost& m_host;
  BindingActions m_actions;
  QVector<BindingEntry> m_entries;
  QVector<ReadableBinding> m_readable; // the words of each entry
  QHash<QString, int> m_usage;
  QComboBox* m_driven = nullptr;
  QLineEdit* m_search = nullptr;
  QTableWidget* m_table = nullptr;
  QPushButton* m_edit = nullptr;
  QPushButton* m_select = nullptr;
  QPushButton* m_remove = nullptr;
  QLabel* m_status = nullptr;
  QLabel* m_message = nullptr;
  bool m_building = false;
  QString m_logged;
  QString m_loggedFilter;
  QString m_loggedPlaces;
  QString m_counts;        // the status line's counts
  QString m_loggedProblem; // the selected row's warning, as logged
};

} // namespace mitcad
