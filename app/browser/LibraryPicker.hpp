// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QFrame>
#include <QImage>
#include <QPoint>
#include <QString>
#include <QVector>

class QEvent;
class QHideEvent;
class QLineEdit;
class QListWidget;
class QListWidgetItem;
class QShowEvent;

namespace mitcad {

// Shared with the native popover's row presentation.
inline constexpr int PickerTagRole = Qt::UserRole + 2;
inline constexpr int PickerLineRole = Qt::UserRole + 3;

// One entry of a picker: what choosing it gives, its name, the heading it
// is listed under ("This design", "Built-in", "My library") and its
// swatch. A choice's picker (mitcad#1108, InputDef::choicePicker) adds a
// small tag at the right (where it comes from), more text its search
// matches, a check on the current choice and a line above the entry
// (New printer… below the printers).
struct PickerEntry {
  QString value;
  QString name;
  QString section;
  QImage swatch;
  QString tooltip;
  // Default member initialisers: the brace inits of the five fields above
  // stay free of -Wmissing-field-initializers.
  QString tag{};
  QString search{};
  bool checked = false;
  bool lineBefore = false;
};

// The picker of the Material and Appearance tools (mitcad#1059, app/COMMANDS.md
// "Material and Appearance tools"): a popover under the pointer with a
// search field and the entries under their headings, each with its swatch.
// Typing narrows the list to the names that contain the text; Up and Down
// move in it, Enter or a click chooses (the picker closes and `chosen` gets
// the entry's value), Esc or a click outside closes it choosing nothing.
// A frameless window of its own with a title ("Material Picker"), not a
// Qt popup, which closes once another window takes the activation: the
// Windows UI tests' posted input reaches it as a dialog's (a popup closes
// at once in their session, tools/ui-windows-lib.ps1).
// Logged for UI tests as "<log> opened: N entries, current <name>", with
// headings "<log> sections: Acme 2 | Bench 1" (mitcad#1108), "<log>
// search at x,y", "<log> item <name> at x,y" (the entries in sight, in the
// main window's coordinates), "<log> chose: <name>" and "<log> closed".
class LibraryPicker : public QFrame {
  Q_OBJECT

public:
  // Opens a picker over `parent`'s window with its top left corner at `at`
  // (global; moved to stay on the screen), titled `title`; `log` heads its
  // log lines ("Material picker"). The AppKit frontend shows it as a
  // popover under the control named `anchor` (the tile's input_<key>),
  // else under the pointer (showNativePicker, mitcad#1066). It is as wide
  // as its entries need (mitcad#1108), at least its usual width and
  // `minimumWidth`, at most the screen's.
  static LibraryPicker* open(QWidget* parent, const QString& title, const QString& log,
                             const QVector<PickerEntry>& entries, const QString& current, const QPoint& at,
                             std::function<void(const QString&)> chosen, const QString& anchor = QString(),
                             int minimumWidth = 0);

protected:
  bool event(QEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;
  void showEvent(QShowEvent* event) override;
  void hideEvent(QHideEvent* event) override;

private:
  LibraryPicker(QWidget* parent, const QString& log, std::function<void(const QString&)> chosen);
  void fill(const QVector<PickerEntry>& entries, const QString& current);
  // The width its entries need: the longest name with its icon and tag.
  int fittingWidth() const;
  void filter(const QString& text);
  // The next shown entry after the current one (`by` 1) or before it (-1).
  void step(int by);
  void choose(QListWidgetItem* item);
  void scheduleLogPlaces();
  void logPlaces();

  QString m_log;
  std::function<void(const QString&)> m_chosen;
  QWidget* m_window = nullptr; // whose coordinates the places are logged in
  QLineEdit* m_search = nullptr;
  QListWidget* m_list = nullptr;
  bool m_done = false;
  bool m_activated = false; // it had the activation: losing it closes it
  QString m_logged;
};

} // namespace mitcad
