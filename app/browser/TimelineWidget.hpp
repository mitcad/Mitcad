// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <utility>

#include <QHash>
#include <QPair>
#include <QPoint>
#include <QPointer>
#include <QRect>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QVector>
#include <QWidget>

#include "DocumentSnapshot.hpp"

class QLabel;
class QLineEdit;
class QScrollArea;
class QToolButton;
class QTreeWidget;
class QWheelEvent;

namespace mitcad {

class TimelineStrip;

// The timeline, docked at the bottom: the features as icons in
// timeline order (coloured by component), the history marker, and the
// playback buttons that step it. Hovering shows a feature's name, a failed
// feature is red and one with a warning yellow, with a status badge that
// stays whatever the cell's state (mitcad#961, FeatureStatus.hpp) and the
// message as its tooltip; a placeholder of an item an import skipped (mitcad#170) is
// suppressed with a yellow outline, the file's state in its tooltip. Dragging the marker rolls the model back and forward; dragging
// a feature moves it in the timeline, and a place it cannot go shows red
// while dragging. Timeline groups (P9) have a band over their features,
// which a click folds into one folder cell (and a double-click on that
// opens again). Shift+click selects a run of features. A filter (mitcad#98)
// shows only some components' features: the others take no room, a mark
// stands for each run of them, and the marker and the playback buttons step
// between the features shown. The search (mitcad#179) finds features by
// name, type, component and status: a field beside the filter button
// (Ctrl+F, Cmd+F on macOS), the matches outlined in the strip and listed
// with their components over it, stepped through with Enter and the arrow
// keys. The mouse wheel over the strip scrolls it sideways (mitcad#826).
// What the actions do is the BrowserController's.
class TimelineWidget : public QWidget {
  Q_OBJECT

public:
  explicit TimelineWidget(const CommandContext& model, QWidget* parent = nullptr);

  void rebuild(const DocumentSnapshot& snapshot);
  void setFeatureDetailsProvider(std::function<QString(const QString&)> provider) { m_featureDetails = std::move(provider); }
  QStringList hoveredFeatures() const { return m_hoveredFeatures; }
  // Shows only some components' features (mitcad#98): all (an empty
  // filter), the active component's (kActiveFilter) or a component's (its
  // uid), with those of the components placed in it when `subcomponents`.
  // Joints and rigid groups that place occurrences of a shown component
  // are shown too. This window's view, not saved.
  void setFilter(const QString& filter, bool subcomponents);
  QString filter() const { return m_filter; }
  bool filterSubcomponents() const { return m_subcomponents; }
  static const QString kActiveFilter;
  // Shared presentation metadata and menu for native timeline adapters.
  QSet<QString> filteredFeatures() const;
  QString filterLabel() const;
  void showFilterMenu(const QPoint& globalPosition);
  // Selects a feature and scrolls to it (Find in Timeline).
  void selectFeature(const QString& uid);
  // Mirrors an external selection exactly (the browser's rows, mitcad#177),
  // without emitting a selection signal; logs it and scrolls to it when it
  // changed. Missing features are ignored; the selection stays in timeline
  // order.
  void setSelectedFeatures(const QStringList& uids);
  QString selectedFeature() const;
  // The selected features (a Shift+click run or an external set), in timeline order.
  QStringList selectedFeatures() const;
  // Folds a timeline group into one cell or opens it (this window's view
  // of it, P9); a renamed group keeps how it was shown.
  void setGroupCollapsed(const QString& name, bool collapsed);
  bool isGroupCollapsed(const QString& name) const { return m_collapsed.contains(name); }
  void groupRenamed(const QString& name, const QString& newName);
  // Renames a feature in place.
  void startRename(const QString& uid);
  // Closes an open rename editor without renaming; false when none is open.
  bool cancelEditing();
  // Logs where the features and the marker are, for UI tests (when changed).
  void logLayout();
  // The search (mitcad#179): opens the field with the keys in it (Ctrl+F in
  // the timeline or the browser), or closes it, which clears it. Clearing
  // the search brings back the scroll position and the selection it began
  // with, unless something else was selected since.
  void openSearch();
  void closeSearch();
  bool isSearchOpen() const;
  // Other toolkit presentations use this same search and restoration state.
  void beginExternalSearch();
  void setSearchText(const QString& text);
  QString searchText() const;
  QString searchCountText() const;
  QStringList searchMatches() const { return m_matches; }
  int currentSearchMatch() const { return m_match; }
  void stepSearch(int delta);
  void goToMatch(int index);
  void showSearchMenu(const QPoint& globalPosition);

signals:
  void filterChanged();
  // Preview only: leaves the selection and history marker untouched.
  void featuresHovered(const QStringList& uids);
  // A click selected a feature, a Shift+click a run of them (in timeline
  // order).
  void featuresClicked(const QStringList& uids);
  void editRequested(const QString& uid);
  void contextMenuRequested(const QString& uid, const QPoint& globalPosition);
  void deleteRequested(const QString& uid);
  void renamed(const QString& uid, const QString& name);
  // The marker: a drag starts, crosses features, ends; the playback
  // buttons ask for a position.
  void markerDragStarted();
  void markerDragged(int position);
  void markerDragFinished(int position);
  void markerRequested(int position);
  void reorderRequested(const QString& uid, int index);
  // A right click on a group's band or folded cell.
  void groupMenuRequested(const QString& name, const QPoint& globalPosition);
  // The fx badge of a feature suppressed by a condition (mitcad#862): the
  // pointer rests on it (`pinned` false) or it was clicked; `anchor` is
  // the badge in global coordinates.
  void bindingsRequested(const QString& uid, const QRect& anchor, bool pinned);
  // A search began (its text is no longer empty): what is selected now
  // comes back when it is cleared. A step to a match selects it as a click
  // does (featuresClicked).
  void searchStarted();
  void searchChanged();
  // The search was cleared after it selected matches, with nothing else
  // selected since: the timeline shows its selection from before again;
  // the rest of the window is to follow.
  void searchRestored();

protected:
  void changeEvent(QEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;

private:
  friend class TimelineStrip;

  // The playback buttons' icons and the marker in the colours of the
  // palette, light or dark (mitcad#14); logs them when they change.
  void applyPalette();
  // The filter (mitcad#98): the components whose features it shows (empty:
  // all), whether a feature is shown, its menu and its button's state.
  QSet<QString> filteredComponents() const;
  bool passesFilter(const DocumentSnapshot::Feature& feature, const QSet<QString>& components,
                    const QHash<QString, QString>& occurrenceComponents) const;
  void showFilterMenu();
  void updateFilterButton();
  // The search (mitcad#179): the field's text changed; the matches found
  // again (after a change of the text or the timeline); a step to the
  // previous or next match, or to one; the list of matches over the field;
  // the field's menu of filters.
  void searchTextChanged();
  void endSearch();
  void updateMatches();
  void updateSearchCount();
  void fillSearchList();
  void showSearchList();
  void hideSearchList();
  void placeSearchList();
  void showSearchMenu();
  void logSearchList();
  // A wheel over the strip or its scroll bar (mitcad#826): the larger of
  // its two deltas scrolls the strip sideways; nothing goes on to the view.
  void wheelStrip(QWheelEvent* event);

  std::function<QString(const QString&)> m_featureDetails;
  QStringList m_hoveredFeatures;
  const CommandContext& m_model;
  DocumentSnapshot m_snapshot; // the last rebuild's
  QSet<QString> m_collapsed;   // folded groups
  QVector<QPair<QString, QToolButton*>> m_buttons; // the playback buttons
  QVector<int> m_buttonIcons; // their QStyle::StandardPixmap
  QStringList m_buttonSymbols; // their SF Symbols in a floating card; empty: the style's
  QString m_loggedColours;
  TimelineStrip* m_strip = nullptr;
  QScrollArea* m_scroll = nullptr;
  QLineEdit* m_editor = nullptr;
  QString m_editing; // the feature being renamed
  QString m_logged;
  QString m_filter; // empty: all components; kActiveFilter or a component's uid
  bool m_subcomponents = false;
  QToolButton* m_filterButton = nullptr;
  QString m_loggedFilter;
  // The search (mitcad#179).
  QToolButton* m_searchButton = nullptr;
  QWidget* m_searchBar = nullptr; // the field, the count and the step buttons
  QLineEdit* m_searchField = nullptr;
  QLabel* m_searchCount = nullptr;
  QToolButton* m_searchPrevious = nullptr;
  QToolButton* m_searchNext = nullptr;
  QPointer<QTreeWidget> m_searchList; // over the window (its child), made when first shown
  QStringList m_matches;     // in timeline order
  int m_match = -1;          // the match stepped to
  bool m_externalSearch = false; // Native presentation owns focus, results and scrolling.
  bool m_searching = false;  // the field has a text
  int m_savedScroll = 0;     // when the search began
  double m_wheelRemainder = 0; // part of a wheel step not scrolled yet, in pixels
  QStringList m_savedSelection;
  QString m_searchSelected;  // the match last selected by a step
  QString m_loggedSearch;
  QString m_loggedResults;
};

} // namespace mitcad
