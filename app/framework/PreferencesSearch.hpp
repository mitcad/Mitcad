// SPDX-License-Identifier: MIT
#pragma once

// The search of Preferences (mitcad#2074, GitHub mitcad/Mitcad#450): a field
// above the page list that finds settings by what the pages show, counts
// the matches of each page in the list, greys the pages without one and
// paints the matching settings with the theme's SearchMatch color.

#include <functional>
#include <vector>

#include <QColor>
#include <QCoreApplication>
#include <QLineEdit>
#include <QPalette>
#include <QPointer>
#include <QString>
#include <QStringList>
#include <QVariant>

class QLabel;
class QListWidget;
class QTabWidget;
class QTableWidget;
class QTableWidgetItem;
class QWidget;

namespace mitcad {

// The words of a search, split at white space.
QStringList preferencesSearchWords(const QString& query);
// Whether each word is somewhere in `text`, in any order, ignoring case.
// No words match nothing.
bool preferencesSearchMatches(const QString& text, const QStringList& words);
// The words of what was typed in the command search that the command's name
// does not hold ("temperature" of "pref temperature" for "Preferences:
// General"): what a page's command passes on to Preferences' search.
QString preferencesSearchQueryFor(const QString& typed, const QString& commandName);
// A widget's text as it is read: without mnemonics (&) and markup.
QString preferencesPlainText(const QString& text);

// The index of the pages and the looks of a search's matches. The index is
// made from the pages' widgets as they are at each search, never from a
// list kept by hand: form row labels with their fields (tooltips, combo box
// items), check boxes, radio buttons, labels of a buddy, group box titles,
// sub-tab names, table headers and rows; tooltips with each. A page's own
// words (its name, its keywords: alternate spellings, English words in a
// translated interface) match the page as a whole.
class PreferencesSearch {
public:
  // The dynamic property set on a match's label and control (true, false
  // once no longer a match), for the UI tests.
  static constexpr const char* kMatchProperty = "preferencesSearchMatch";

  void addPage(QWidget* page, const QString& name, const QStringList& keywords);
  int pageCount() const { return int(m_pages.size()); }

  // Looks for `query` and paints its matches, the earlier ones' looks put
  // back first: marks the sub-tabs with a match and opens one where the
  // tab shown has none. Returns the number of matches of each page: its
  // settings that match, or 1 when only the page's own words do.
  const std::vector<int>& apply(const QString& query);
  const std::vector<int>& counts() const { return m_counts; }
  int total() const;
  QString query() const { return m_query; }
  // The shown texts of the matches on `page`, in the page's order (logs).
  QStringList matchTexts(int page) const;
  // Scrolls `page`'s first match into view (on the tab shown) and returns
  // its control, or nullptr when the page has no setting that matches.
  QWidget* revealFirst(int page);

private:
  struct Entry {
    QString text;  // what the words are looked for in
    QString shown; // the setting's own text, for logs
    QPointer<QWidget> paint;   // the label or text painted
    QPointer<QWidget> control; // what Return focuses
    QPointer<QTabWidget> tabs; // the sub-tab the entry is on (or names)
    int tab = -1;
    QPointer<QTableWidget> table; // a table's row (row -1: its header)
    int row = -1;
    int order = 0; // its place in the page's widgets
  };
  struct Page {
    QPointer<QWidget> widget;
    QString name;
    QString words; // the name and the keywords
    std::vector<Entry> entries;
    std::vector<int> matched; // indexes into entries
  };
  struct PaintedWidget {
    QPointer<QWidget> widget;
    bool ownPalette = false;
    QPalette palette;
  };
  struct PaintedItem {
    QPointer<QTableWidget> table;
    int row = 0;
    int column = 0;
    QTableWidgetItem* item = nullptr;
    QVariant foreground;
  };
  struct MarkedTab {
    QPointer<QTabWidget> tabs;
    int index = 0;
    QColor color;
  };

  static std::vector<Entry> index(QWidget* page);
  void restore();
  void paint(QWidget* widget, const QColor& color);
  void mark(QWidget* widget);
  int firstShown(const Page& page) const;

  std::vector<Page> m_pages;
  std::vector<int> m_counts;
  QString m_query;
  std::vector<PaintedWidget> m_painted;
  std::vector<PaintedItem> m_paintedItems;
  std::vector<MarkedTab> m_marked;
  std::vector<QPointer<QWidget>> m_flagged;
};

// The search field above Preferences' page list: as it is typed in, each
// page in the list shows its number of matches ("Display (3)"), a page
// without one is greyed (it stays clickable), and the first page with a
// match is shown when the page shown has none. Return focuses the first
// match's control (it never takes OK while there is text); Esc clears the
// text, and in an empty field closes Preferences as before; Down moves
// into the page list. Logged: `Preferences search: "<text>": <n> matches
// on <pages>`.
class PreferencesSearchField : public QLineEdit {
  Q_DECLARE_TR_FUNCTIONS(mitcad::PreferencesSearchField)

public:
  // `list`: the page list, a row per page in addPage's order; `showPage`
  // shows a page (the list's row follows).
  PreferencesSearchField(QListWidget* list, std::function<void(int)> showPage, QWidget* parent = nullptr);

  void addPage(QWidget* page, const QString& name, const QStringList& keywords);
  PreferencesSearch& search() { return m_search; }
  static QString noMatchText();

protected:
  bool event(QEvent* event) override;
  void keyPressEvent(QKeyEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;

private:
  void searchChanged(const QString& typed);
  void focusFirstMatch();
  void placeNoMatch();

  QListWidget* m_list = nullptr;
  std::function<void(int)> m_showPage;
  PreferencesSearch m_search;
  QStringList m_names;
  QLabel* m_noMatch = nullptr;
};

} // namespace mitcad
