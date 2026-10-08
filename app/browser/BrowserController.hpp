// SPDX-License-Identifier: MIT
#pragma once

#include <QObject>
#include <QPoint>
#include <QPointer>
#include <QString>
#include <QStringList>

#include "BrowserPanel.hpp"
#include "DocumentSnapshot.hpp"

class QToolButton;
class QWidget;

namespace mitcad {

class AppearanceDialog;
class DocumentHost;
class ParametersDialog;
class TimelineWidget;

// What the browser, the timeline, the parameters dialog and the status
// bar's list of failed features do (U3): their menus and the model commands
// behind them, the history marker's drags as one undo step, and keeping
// them in step with the model (refresh) and the selection (showSelection).
class BrowserController : public QObject {
  Q_OBJECT

public:
  BrowserController(DocumentHost& host, QWidget* window);

  BrowserPanel* browser() const { return m_browser; }
  TimelineWidget* timeline() const { return m_timeline; }
  // The status bar's summary of failed features; a click goes to them.
  QToolButton* failures() const { return m_failures; }

  // After the model or the origin's visibility changed.
  void refresh(const DocumentSnapshot& snapshot);
  // The selection outside commands changed (in the view or here).
  void showSelection(const Selection& items);
  // Closes an open rename editor; false when none was open.
  bool cancelEditing();
  // Change Parameters.
  void openParameters();
  // Edit Appearances (mitcad#46); Assign gives the targets (bodies, and
  // faces: mitcad#53) the selected appearance.
  void openAppearances(const Selection& targets);
  // Find in Browser for an item picked in the view (a face finds its body).
  void findInBrowser(const SelectionItem& item);

private:
  // Connects a signal of the browser or the timeline to a slot, queued:
  // the slot may change the model, which rebuilds the widget that emitted
  // it. A call that comes while a job computes the model (P7: posted before
  // the job began, or the end of a drag that the progress dialog cut short)
  // waits until the job is done; a menu is not shown then (dropWhenBusy).
  template <typename Sender, typename... Args>
  void follow(Sender* sender, void (Sender::*signal)(Args...), void (BrowserController::*slot)(Args...),
              bool dropWhenBusy = false);
  void pickItems(const Selection& items);
  void editFeature(const QString& uid);
  void markerDragged(int position);
  void browserMenu(const BrowserNode& node, const QPoint& at);
  void timelineMenu(const QString& uid, const QPoint& at);
  // Timeline groups (P9): a group's menu, and grouping features.
  void groupMenu(const QString& name, const QPoint& at);
  void groupFeatures(const QStringList& uids);
  void browserDoubleClicked(const BrowserNode& node);
  void toggleVisibility(const BrowserNode& node);
  void activate(const BrowserNode& node);
  void rename(const BrowserNode& node, const QString& name);
  void renameFeature(const QString& uid, const QString& name);
  void deleteNode(const BrowserNode& node);
  void deleteFeature(const QString& uid);
  void featuresClicked(const QStringList& uids);
  void reorder(const QString& uid, int index);
  void setMarker(int position);
  void markerDragFinished(int position);
  void newComponent(const BrowserNode& node);
  void paste(bool asNew);
  void changeUnits();
  void reveal(const QString& uid);
  void updateFailures();
  bool canChange(bool visibilityOnly = false);
  const DocumentSnapshot::Occurrence* occurrence(const QString& path) const;
  int undoDepth() const;

  DocumentHost& m_host;
  QWidget* m_window;
  BrowserPanel* m_browser = nullptr;
  TimelineWidget* m_timeline = nullptr;
  QToolButton* m_failures = nullptr;
  QPointer<ParametersDialog> m_parameters;
  QPointer<AppearanceDialog> m_appearances;
  DocumentSnapshot m_snapshot;
  bool m_shown = false;      // something was shown
  bool m_originShown = false; // as the browser shows it
  QString m_clipboard;        // a copied occurrence's path
  QString m_clipboardName;
  int m_markerDepth = -1; // undo depth when a marker drag began
  int m_markerStart = 0;
  QString m_loggedFailures;
  QString m_loggedWarnings;
  QString m_loggedDof;
  QString m_loggedJoints;
};

} // namespace mitcad
