// SPDX-License-Identifier: MIT
#pragma once

#include <QJsonObject>
#include <QWidget>

#include "Selection.hpp"

class QLabel;
class QModelIndex;
class QPushButton;
class QTreeWidget;
class QTreeWidgetItem;

namespace mitcad {

// Pure scope edits: neither a changed preview nor an empty candidate list
// changes these persistent identities.
QJsonObject setCutTargetChecked(const QJsonObject& scope, const QJsonObject& target,
                                const QString& body, bool checked);
QJsonObject selectCutTargets(const QJsonObject& scope, bool all);
// A row an automatic scope leaves out only because it moves against the
// cut's component through joints (the report row's `joint_motion`,
// mitcad#1789), checked in the list: the scope turns explicit with every
// eligible row of `report` (the preview's `cut_targets`) checked now, their
// body exceptions kept, plus `target`, in one step. With `body`, only that
// body of the target: its other bodies go to `excluded_bodies`.
QJsonObject includeMovingCutTarget(const QJsonObject& scope, const QJsonObject& report,
                                   const QJsonObject& target, const QString& body);

// What the view hides when a new cut's panel opens (mitcad#475), from the
// model's `components` and `bodies` queries and the `document` query's
// display: {"hidden_occurrences": [paths whose light bulb is off],
// "hidden_bodies": [uids whose light bulb is off], "isolated": [the
// isolated items], "seen": []}.
QJsonObject cutTargetVisibility(const QJsonObject& components, const QJsonArray& bodies,
                                const QJsonObject& document);
// A preview's cut targets against that visibility (mitcad#475): each row
// seen for the first time in an automatic scope starts unchecked when it is
// hidden (its occurrence's light bulb or one above it off, or Isolate
// shows none of its bodies: `excluded`), or only its hidden bodies do
// (`excluded_bodies`); a row seen before keeps what the user chose. The
// rows and bodies of the report that are hidden get "hidden": true, for
// the list. True when the scope changed.
bool excludeHiddenCutTargets(QJsonObject& scope, QJsonObject& visibility, QJsonObject& report);

class CutTargetsPanel final : public QWidget {
  Q_OBJECT
public:
  explicit CutTargetsPanel(QWidget* parent = nullptr);
  void setScope(const QJsonObject& scope);
  void setPreview(const QJsonObject& report);

signals:
  void scopeChanged(const QJsonObject& scope);
  void highlightChanged(const mitcad::Selection& items);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  void hideEvent(QHideEvent* event) override;

private:
  void rebuild();
  void highlight(const QModelIndex& index);
  QJsonObject m_scope;
  QJsonObject m_report;
  bool m_current = false;
  bool m_refreshing = false;
  bool m_wraps = false; // a row shows a wrapped reason or note
  QTreeWidget* m_tree = nullptr;
  QLabel* m_status = nullptr;
  QPushButton* m_all = nullptr;
  QPushButton* m_none = nullptr;
};

} // namespace mitcad
