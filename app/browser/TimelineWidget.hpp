// SPDX-License-Identifier: MIT
#pragma once

#include <QHash>
#include <QPair>
#include <QPoint>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QVector>
#include <QWidget>

#include "DocumentSnapshot.hpp"

class QLineEdit;
class QScrollArea;
class QToolButton;

namespace mitcad {

class TimelineStrip;

// The timeline, docked at the bottom: the features as icons in
// timeline order (coloured by component), the history marker, and the
// playback buttons that step it. Hovering shows a feature's name, a failed
// feature is red and one with a warning yellow, with the message as its
// tooltip. Dragging the marker rolls the model back and forward; dragging
// a feature moves it in the timeline, and a place it cannot go shows red
// while dragging. Timeline groups (P9) have a band over their features,
// which a click folds into one folder cell (and a double-click on that
// opens again). Shift+click selects a run of features. What
// the actions do is the BrowserController's.
class TimelineWidget : public QWidget {
  Q_OBJECT

public:
  explicit TimelineWidget(const CommandContext& model, QWidget* parent = nullptr);

  void rebuild(const DocumentSnapshot& snapshot);
  // Selects a feature and scrolls to it (Find in Timeline).
  void selectFeature(const QString& uid);
  QString selectedFeature() const;
  // The selected run of features (Shift+click), in timeline order.
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

signals:
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

protected:
  void changeEvent(QEvent* event) override;

private:
  friend class TimelineStrip;

  // The playback buttons' icons and the marker in the colours of the
  // palette, light or dark (mitcad#14); logs them when they change.
  void applyPalette();

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
};

} // namespace mitcad
