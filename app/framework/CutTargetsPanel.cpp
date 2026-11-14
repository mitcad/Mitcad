// SPDX-License-Identifier: MIT
#include "CutTargetsPanel.hpp"

#include <algorithm>

#include <QEvent>
#include <QHeaderView>
#include <QHideEvent>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QLabel>
#include <QPushButton>
#include <QResizeEvent>
#include <QScrollBar>
#include <QSet>
#include <QSignalBlocker>
#include <QStyle>
#include <QStyledItemDelegate>
#include <QTimer>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "../platform/NativeDialogs.hpp"
#include "ModelMessages.hpp"
#include "PanelTree.hpp"

namespace mitcad {
namespace {
constexpr int kTargetRole = Qt::UserRole;
constexpr int kBodyRole = Qt::UserRole + 1;
constexpr int kBodiesRole = Qt::UserRole + 2;
// A row left out only because it moves through joints (mitcad#1789):
// checking it, or one of its bodies, turns the scope explicit with it.
constexpr int kMovesRole = Qt::UserRole + 3;
// A row whose reason or note is under its name, wrapped to the list's width.
constexpr int kWrapRole = Qt::UserRole + 4;

// Wraps the rows marked kWrapRole, and gives them the height of their
// wrapped lines (mitcad#1789): a reason cut off at the list's width hid the
// way out.
class WrappingDelegate final : public QStyledItemDelegate {
public:
  explicit WrappingDelegate(QTreeWidget* tree) : QStyledItemDelegate(tree), m_tree(tree) {}

  QSize sizeHint(const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    QSize size = QStyledItemDelegate::sizeHint(option, index);
    if (!index.data(kWrapRole).toBool()) {
      return size;
    }
    int depth = 1; // the root's decoration
    for (QModelIndex parent = index.parent(); parent.isValid(); parent = parent.parent()) { ++depth; }
    QStyle* style = m_tree->style();
    const int margin = style->pixelMetric(QStyle::PM_FocusFrameHMargin, nullptr, m_tree) + 1;
    const int width = m_tree->viewport()->width() - depth * m_tree->indentation() -
                      style->pixelMetric(QStyle::PM_IndicatorWidth, nullptr, m_tree) - 4 * margin;
    if (width <= 0) {
      return size;
    }
    const QRect lines = option.fontMetrics.boundingRect(QRect(0, 0, width, 1 << 20), Qt::TextWordWrap,
                                                        index.data(Qt::DisplayRole).toString());
    size.setHeight(std::max(size.height(), lines.height() + 2 * margin));
    return size;
  }

protected:
  void initStyleOption(QStyleOptionViewItem* option, const QModelIndex& index) const override {
    QStyledItemDelegate::initStyleOption(option, index);
    if (index.data(kWrapRole).toBool()) {
      option->features |= QStyleOptionViewItem::WrapText;
      option->textElideMode = Qt::ElideNone;
    }
  }

private:
  QTreeWidget* m_tree;
};

QJsonObject identity(const QJsonObject& row) {
  return {{QStringLiteral("component"), row.value(QStringLiteral("component"))},
          {QStringLiteral("occurrence"), row.value(QStringLiteral("occurrence"))}};
}

QJsonArray membership(QJsonArray values, const QJsonObject& item, bool contains) {
  QJsonArray result;
  for (const QJsonValue& value : values) {
    if (value.toObject() != item) { result.append(value); }
  }
  if (contains) { result.append(item); }
  return result;
}
} // namespace

QJsonObject setCutTargetChecked(const QJsonObject& scope, const QJsonObject& target,
                                const QString& body, bool checked) {
  QJsonObject result = scope;
  QJsonObject targets = result.value(QStringLiteral("targets")).toObject();
  const QJsonObject component = identity(target);
  if (body.isEmpty()) {
    const bool automatic = targets.value(QStringLiteral("type")).toString() == QStringLiteral("automatic");
    const QString key = automatic ? QStringLiteral("excluded") : QStringLiteral("included");
    targets.insert(key, membership(targets.value(key).toArray(), component, automatic ? !checked : checked));
    if (checked) {
      QJsonArray exceptions;
      for (const QJsonValue& value : targets.value(QStringLiteral("excluded_bodies")).toArray()) {
        if (identity(value.toObject()) != component) { exceptions.append(value); }
      }
      targets.insert(QStringLiteral("excluded_bodies"), exceptions);
    }
  } else {
    QJsonObject item = component;
    item.insert(QStringLiteral("body"), body);
    targets.insert(QStringLiteral("excluded_bodies"),
                   membership(targets.value(QStringLiteral("excluded_bodies")).toArray(), item, !checked));
  }
  result.insert(QStringLiteral("targets"), targets);
  return result;
}

QJsonObject selectCutTargets(const QJsonObject& scope, bool all) {
  QJsonObject result = scope;
  result.insert(QStringLiteral("targets"),
                QJsonObject{{QStringLiteral("type"), all ? QStringLiteral("automatic") : QStringLiteral("explicit")},
                            {all ? QStringLiteral("excluded") : QStringLiteral("included"), QJsonArray()},
                            {QStringLiteral("excluded_bodies"), QJsonArray()}});
  return result;
}

QJsonObject includeMovingCutTarget(const QJsonObject& scope, const QJsonObject& report,
                                   const QJsonObject& target, const QString& body) {
  const QJsonObject component = identity(target);
  const QJsonArray before = scope.value(QStringLiteral("targets")).toObject().value(QStringLiteral("excluded_bodies")).toArray();
  const QJsonArray rows = report.value(QStringLiteral("rows")).toArray();
  QJsonArray included;
  QJsonArray excludedBodies;
  for (const QJsonValue& value : rows) {
    const QJsonObject row = value.toObject();
    const QJsonObject id = identity(row);
    const bool checked = row.value(QStringLiteral("checked")).toBool() || row.value(QStringLiteral("partial")).toBool();
    if (id == component || !row.value(QStringLiteral("eligible")).toBool() || !checked || included.contains(id)) {
      continue;
    }
    included.append(id);
    for (const QJsonValue& exception : before) {
      if (identity(exception.toObject()) == id) { excludedBodies.append(exception); }
    }
  }
  included.append(component);
  if (!body.isEmpty()) {
    for (const QJsonValue& value : rows) {
      const QJsonObject row = value.toObject();
      if (identity(row) != component) { continue; }
      for (const QJsonValue& other : row.value(QStringLiteral("bodies")).toArray()) {
        const QString uid = other.toObject().value(QStringLiteral("body")).toString();
        if (uid == body) { continue; }
        QJsonObject item = component;
        item.insert(QStringLiteral("body"), uid);
        if (!excludedBodies.contains(item)) { excludedBodies.append(item); }
      }
    }
  }
  QJsonObject result = scope;
  result.insert(QStringLiteral("targets"), QJsonObject{{QStringLiteral("type"), QStringLiteral("explicit")},
                                                       {QStringLiteral("included"), included},
                                                       {QStringLiteral("excluded_bodies"), excludedBodies}});
  return result;
}

namespace {

void hiddenOccurrences(const QJsonArray& occurrences, const QString& parent, QJsonArray& hidden) {
  for (const QJsonValue& value : occurrences) {
    const QJsonObject occurrence = value.toObject();
    const QString uid = occurrence.value(QStringLiteral("uid")).toString();
    const QString path = parent.isEmpty() ? uid : parent + QLatin1Char('/') + uid;
    if (!occurrence.value(QStringLiteral("visible")).toBool(true)) {
      hidden.append(path); // what is in it is hidden with it
      continue;
    }
    hiddenOccurrences(occurrence.value(QStringLiteral("children")).toArray(), path, hidden);
  }
}

bool within(const QString& path, const QString& above) {
  return path == above || path.startsWith(above + QLatin1Char('/'));
}

// Whether Isolate shows a body where an occurrence places it (as the view
// decides it, MainWindow::isIsolated); everything when nothing is isolated.
bool isolatedBody(const QJsonArray& isolated, const QString& body, const QString& occurrence) {
  if (isolated.isEmpty()) {
    return true;
  }
  for (const QJsonValue& value : isolated) {
    const QJsonObject item = value.toObject();
    const QString at = item.value(QStringLiteral("occurrence")).toString();
    if (item.contains(QStringLiteral("body")) ? item.value(QStringLiteral("body")).toString() == body && at == occurrence
                                              : within(occurrence, at)) {
      return true;
    }
  }
  return false;
}

} // namespace

QJsonObject cutTargetVisibility(const QJsonObject& components, const QJsonArray& bodies,
                                const QJsonObject& document) {
  QJsonArray occurrences;
  hiddenOccurrences(components.value(QStringLiteral("occurrences")).toArray(), QString(), occurrences);
  QJsonArray hiddenBodies;
  for (const QJsonValue& value : bodies) {
    const QJsonObject body = value.toObject();
    if (!body.value(QStringLiteral("visible")).toBool(true)) {
      hiddenBodies.append(body.value(QStringLiteral("uid")));
    }
  }
  return {{QStringLiteral("hidden_occurrences"), occurrences},
          {QStringLiteral("hidden_bodies"), hiddenBodies},
          {QStringLiteral("isolated"),
           document.value(QStringLiteral("display")).toObject().value(QStringLiteral("isolated")).toArray()},
          {QStringLiteral("seen"), QJsonArray()}};
}

bool excludeHiddenCutTargets(QJsonObject& scope, QJsonObject& visibility, QJsonObject& report) {
  QJsonObject targets = scope.value(QStringLiteral("targets")).toObject();
  const bool automatic = targets.value(QStringLiteral("type")).toString() == QStringLiteral("automatic");
  QJsonArray excluded = targets.value(QStringLiteral("excluded")).toArray();
  QJsonArray excludedBodies = targets.value(QStringLiteral("excluded_bodies")).toArray();
  QJsonArray seen = visibility.value(QStringLiteral("seen")).toArray();
  const QJsonArray hiddenPaths = visibility.value(QStringLiteral("hidden_occurrences")).toArray();
  const QJsonArray hiddenBodies = visibility.value(QStringLiteral("hidden_bodies")).toArray();
  const QJsonArray isolated = visibility.value(QStringLiteral("isolated")).toArray();
  QJsonObject cut = report.value(QStringLiteral("cut_targets")).toObject();
  QJsonArray rows = cut.value(QStringLiteral("rows")).toArray();
  bool changed = false;
  for (int i = 0; i < rows.size(); ++i) {
    QJsonObject row = rows[i].toObject();
    const QString path = row.value(QStringLiteral("occurrence")).toString();
    const QJsonObject target = identity(row);
    QJsonArray bodies = row.value(QStringLiteral("bodies")).toArray();
    bool rowHidden = std::any_of(hiddenPaths.begin(), hiddenPaths.end(),
                                 [&path](const QJsonValue& hidden) { return within(path, hidden.toString()); });
    // Isolate showing none of its bodies hides the row as a whole.
    if (!isolated.isEmpty() && !bodies.isEmpty() &&
        std::none_of(bodies.begin(), bodies.end(), [&isolated, &path](const QJsonValue& body) {
          return isolatedBody(isolated, body.toObject().value(QStringLiteral("body")).toString(), path);
        })) {
      rowHidden = true;
    }
    QStringList hidden;
    for (int b = 0; b < bodies.size(); ++b) {
      QJsonObject body = bodies[b].toObject();
      const QString uid = body.value(QStringLiteral("body")).toString();
      const bool bodyHidden = rowHidden || hiddenBodies.contains(QJsonValue(uid)) || !isolatedBody(isolated, uid, path);
      if (bodyHidden) {
        hidden << uid;
        body.insert(QStringLiteral("hidden"), true);
        bodies[b] = body;
      }
    }
    row.insert(QStringLiteral("bodies"), bodies);
    if (rowHidden || (!bodies.isEmpty() && hidden.size() == bodies.size())) {
      row.insert(QStringLiteral("hidden"), true);
    }
    rows[i] = row;
    const QString key = target.value(QStringLiteral("component")).toString() + QLatin1Char('@') + path;
    if (seen.contains(QJsonValue(key))) {
      continue; // the user's choice stays
    }
    seen.append(key);
    if (!automatic) {
      continue;
    }
    if (rowHidden) {
      if (!excluded.contains(target)) {
        excluded.append(target);
        changed = true;
      }
      continue;
    }
    for (const QString& uid : hidden) {
      QJsonObject item = target;
      item.insert(QStringLiteral("body"), uid);
      if (!excludedBodies.contains(item)) {
        excludedBodies.append(item);
        changed = true;
      }
    }
  }
  cut.insert(QStringLiteral("rows"), rows);
  report.insert(QStringLiteral("cut_targets"), cut);
  visibility.insert(QStringLiteral("seen"), seen);
  if (changed) {
    targets.insert(QStringLiteral("excluded"), excluded);
    targets.insert(QStringLiteral("excluded_bodies"), excludedBodies);
    scope.insert(QStringLiteral("targets"), targets);
  }
  return changed;
}

CutTargetsPanel::CutTargetsPanel(QWidget* parent) : QWidget(parent) {
  auto* layout = new QVBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);
  m_status = new QLabel(tr("Select profiles to find affected components."));
  m_status->setObjectName(QStringLiteral("cut_targets_status"));
  m_status->setWordWrap(true);
  layout->addWidget(m_status);
  m_tree = new QTreeWidget;
  panel::setCompactIndentation(m_tree, panel::Indent::Tree);
  m_tree->setObjectName(QStringLiteral("cut_targets_list"));
  m_tree->setProperty("nativeModelHoverEvents", true);
  m_tree->setHeaderHidden(true);
  m_tree->setColumnCount(1);
  m_tree->setMinimumHeight(120);
  m_tree->setMaximumHeight(180);
  m_tree->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Fixed);
  m_tree->setMouseTracking(true);
  m_tree->viewport()->installEventFilter(this);
  m_tree->installEventFilter(this);
  m_tree->setTextElideMode(Qt::ElideMiddle);
  m_tree->setSelectionMode(QAbstractItemView::SingleSelection);
  m_tree->setEditTriggers(QAbstractItemView::NoEditTriggers);
  m_tree->header()->setStretchLastSection(true);
  m_tree->setItemDelegate(new WrappingDelegate(m_tree));
  layout->addWidget(m_tree);
  auto* buttons = new QHBoxLayout;
  m_all = new QPushButton(tr("Select All"));
  m_all->setObjectName(QStringLiteral("cut_targets_all"));
  m_none = new QPushButton(tr("Select none"));
  m_none->setObjectName(QStringLiteral("cut_targets_none"));
  buttons->addWidget(m_all);
  buttons->addWidget(m_none);
  layout->addLayout(buttons);
  connect(m_all, &QPushButton::clicked, this, [this] { emit scopeChanged(selectCutTargets(m_scope, true)); });
  connect(m_none, &QPushButton::clicked, this, [this] { emit scopeChanged(selectCutTargets(m_scope, false)); });
  connect(m_tree, &QTreeWidget::itemChanged, this, [this](QTreeWidgetItem* item, int) {
    if (m_refreshing || !m_current) { return; }
    const QJsonObject target = item->data(0, kTargetRole).toJsonObject();
    const QString body = item->data(0, kBodyRole).toString();
    const bool checked = item->checkState(0) != Qt::Unchecked;
    if (checked && item->data(0, kMovesRole).toBool()) {
      // Left out of the automatic scope for its joints (mitcad#1789): the
      // scope turns explicit with what is checked now and this one.
      qDebug().noquote() << QStringLiteral("Cut target %1@%2 checked: explicit scope with it")
                                .arg(target.value(QStringLiteral("component")).toString(),
                                     target.value(QStringLiteral("occurrence")).toString());
      emit scopeChanged(includeMovingCutTarget(m_scope, m_report, target, body));
      return;
    }
    emit scopeChanged(setCutTargetChecked(m_scope, target, body, checked));
  });
  connect(m_tree, &QAbstractItemView::entered, this, &CutTargetsPanel::highlight);
  connect(m_tree->selectionModel(), &QItemSelectionModel::currentChanged, this,
          [this](const QModelIndex& index, const QModelIndex&) { highlight(index); });
  rebuild();
}

void CutTargetsPanel::setScope(const QJsonObject& scope) { m_scope = scope; }

void CutTargetsPanel::setPreview(const QJsonObject& report) {
  m_current = !report.isEmpty();
  emit highlightChanged({});
  m_tree->setEnabled(m_current);
  if (!m_current) {
    // A checkbox callback may still be on the model's stack. Retain its
    // items until the next asynchronous result; stale rows cannot be used.
    m_all->setEnabled(false);
    m_none->setEnabled(false);
    m_status->setText(tr("Updating affected components…"));
    return;
  }
  m_report = report;
  rebuild();
}

void CutTargetsPanel::rebuild() {
  m_refreshing = true;
  const QSignalBlocker blocker(m_tree);
  const int scroll = m_tree->verticalScrollBar()->value();
  QSet<QString> expanded;
  for (int i = 0; i < m_tree->topLevelItemCount(); ++i) {
    auto* item = m_tree->topLevelItem(i);
    if (item->isExpanded()) { expanded.insert(item->data(0, NativeControlIdentifierRole).toString()); }
  }
  m_tree->clear();
  m_wraps = false;
  const QJsonArray rows = m_report.value(QStringLiteral("rows")).toArray();
  for (const QJsonValue& value : rows) {
    const QJsonObject row = value.toObject();
    const QString path = row.value(QStringLiteral("occurrence")).toString();
    const QString component = row.value(QStringLiteral("component")).toString();
    const QString name = row.value(QStringLiteral("path_name")).toString(row.value(QStringLiteral("name")).toString(component));
    const QString reason = row.value(QStringLiteral("reason")).toString();
    // The extrusion's own component with several occurrences (mitcad#769):
    // cut in its definition, so every occurrence changes.
    const QString note = row.value(QStringLiteral("note")).toString();
    const bool eligible = row.value(QStringLiteral("eligible")).toBool();
    const bool intersects = row.value(QStringLiteral("intersects")).toBool();
    const bool hidden = row.value(QStringLiteral("hidden")).toBool(); // mitcad#475
    const QJsonObject scope = m_scope.value(QStringLiteral("targets")).toObject();
    const bool automatic = scope.value(QStringLiteral("type")) == QStringLiteral("automatic");
    // Left out only because it moves through joints (mitcad#1789): enabled,
    // and checking it turns the scope explicit with it. Rows unavailable
    // for other reasons (linked, placed more than once) stay disabled.
    const bool moves = !eligible && automatic && row.value(QStringLiteral("joint_motion")).toBool();
    QString text = name;
    if (hidden) { text += tr(" (hidden)"); }
    if (!intersects) { text += tr(" — outside current cut"); }
    // The reason and the note in full under the name, wrapped (mitcad#1789).
    if (!reason.isEmpty()) { text += QLatin1Char('\n') + modelMessage(reason); }
    if (!note.isEmpty()) { text += QLatin1Char('\n') + modelMessage(note); }
    m_wraps = m_wraps || !reason.isEmpty() || !note.isEmpty();
    auto* item = new QTreeWidgetItem(m_tree, {text});
    // In words (mitcad#1741): the component's and the occurrences' ids are
    // the row's data.
    item->setToolTip(0, text);
    item->setData(0, kTargetRole, identity(row));
    item->setData(0, kBodiesRole, row.value(QStringLiteral("bodies")).toArray());
    item->setData(0, kMovesRole, moves);
    item->setData(0, kWrapRole, !reason.isEmpty() || !note.isEmpty());
    item->setData(0, NativeControlIdentifierRole, QStringLiteral("cut_target_") + component + QLatin1Char('@') + path);
    item->setFlags(Qt::ItemIsSelectable | Qt::ItemIsUserCheckable |
                   (m_current && (eligible || moves) ? Qt::ItemIsEnabled : Qt::NoItemFlags));
    item->setCheckState(0, row.value(QStringLiteral("partial")).toBool() ? Qt::PartiallyChecked
                         : row.value(QStringLiteral("checked")).toBool() ? Qt::Checked : Qt::Unchecked);
    for (const QJsonValue& bodyValue : row.value(QStringLiteral("bodies")).toArray()) {
      const QJsonObject body = bodyValue.toObject();
      const QString uid = body.value(QStringLiteral("body")).toString();
      const QString bodyName = body.value(QStringLiteral("name")).toString();
      const QString shown = bodyName.isEmpty() ? tr("Body") : bodyName;
      auto* child = new QTreeWidgetItem(
          item, {shown + (body.value(QStringLiteral("hidden")).toBool() ? tr(" (hidden)") : QString())});
      child->setData(0, kTargetRole, identity(row));
      child->setData(0, kBodyRole, uid);
      child->setData(0, kBodiesRole, QJsonArray{body});
      child->setData(0, kMovesRole, moves);
      child->setData(0, NativeControlIdentifierRole, QStringLiteral("cut_body_") + uid + QLatin1Char('@') + path);
      const QJsonArray membership = scope.value(automatic ? QStringLiteral("excluded") : QStringLiteral("included")).toArray();
      const bool included = automatic ? !membership.contains(identity(row)) : membership.contains(identity(row));
      child->setFlags(Qt::ItemIsSelectable | Qt::ItemIsUserCheckable |
                      (m_current && ((eligible && included) || moves) ? Qt::ItemIsEnabled : Qt::NoItemFlags));
      child->setCheckState(0, body.value(QStringLiteral("checked")).toBool() ? Qt::Checked : Qt::Unchecked);
      // The body and its component in words (mitcad#1789, no ids in what a
      // person reads): the uid is the item's data and in the log.
      child->setToolTip(0, tr("%1 in %2").arg(shown, name));
      qDebug().noquote() << QStringLiteral("Cut body %1@%2 tooltip: %3").arg(uid, path, child->toolTip(0));
    }
    item->setExpanded(expanded.contains(item->data(0, NativeControlIdentifierRole).toString()));
    qDebug().noquote() << QStringLiteral("Cut target %1@%2: %3, %4, %5%6%7")
        .arg(component, path, eligible ? QStringLiteral("eligible") : moves ? QStringLiteral("moves through joints")
                                                                            : QStringLiteral("unavailable"),
             item->checkState(0) == Qt::Checked ? QStringLiteral("checked") : item->checkState(0) == Qt::PartiallyChecked
                 ? QStringLiteral("partial") : QStringLiteral("unchecked"), intersects ? QStringLiteral("intersects") : QStringLiteral("outside"),
             hidden ? QStringLiteral(", hidden") : QString(), note.isEmpty() ? QString() : QStringLiteral(", own definition"));
  }
  m_tree->verticalScrollBar()->setValue(scroll);
  m_all->setEnabled(m_current);
  m_none->setEnabled(m_current);
  const int selected = m_report.value(QStringLiteral("selected_components")).toInt();
  const int selectedBodies = m_report.value(QStringLiteral("selected_bodies")).toInt();
  // One whole sentence per count (mitcad#1675): English has no catalogue, so
  // a %n text would read "1 component(s)"; each sentence is translated.
  const QString selection = selected == 1
      ? (selectedBodies == 1 ? tr("1 component selected (1 body)")
                             : tr("1 component selected (%1 bodies)").arg(selectedBodies))
      : (selectedBodies == 1 ? tr("%1 components selected (1 body)").arg(selected)
                             : tr("%1 components selected (%2 bodies)").arg(selected).arg(selectedBodies));
  m_status->setText(!m_current ? tr("Update the inputs to preview affected components.")
      : selected > 0 ? selection
      // A cut drawn away from the body (mitcad#1676): the model found that
      // the other direction would cut.
      : m_report.value(QStringLiteral("flip_would_cut")).toBool()
          ? tr("No eligible components intersect the cut. Try Flip: the other direction cuts a component.")
      : rows.isEmpty() ? tr("No eligible components intersect the cut.") : tr("No components selected."));
  m_refreshing = false;
  // Where each row is in the window once laid out, for UI tests: "Cut
  // target row C2@O2 at x,y" (a click there selects it, Space checks it).
  QTimer::singleShot(0, this, [this] {
    if (!m_tree->isVisible()) {
      return;
    }
    for (int i = 0; i < m_tree->topLevelItemCount(); ++i) {
      QTreeWidgetItem* item = m_tree->topLevelItem(i);
      const QRect rect = m_tree->visualItemRect(item);
      if (!rect.isValid() || !m_tree->viewport()->rect().contains(rect.center())) {
        continue;
      }
      const QPoint at = m_tree->viewport()->mapTo(window(), rect.center());
      const QJsonObject target = item->data(0, kTargetRole).toJsonObject();
      qDebug().noquote() << QStringLiteral("Cut target row %1@%2 at %3,%4")
                                .arg(target.value(QStringLiteral("component")).toString(),
                                     target.value(QStringLiteral("occurrence")).toString())
                                .arg(at.x())
                                .arg(at.y());
    }
  });
}

void CutTargetsPanel::highlight(const QModelIndex& index) {
  Selection items;
  if (m_current && !m_refreshing && index.isValid()) {
    const QString occurrence = index.data(kTargetRole).toJsonObject().value(QStringLiteral("occurrence")).toString();
    for (const QJsonValue& value : index.data(kBodiesRole).toJsonArray()) {
      items.append({SelectKind::Body, value.toObject().value(QStringLiteral("body")).toString(), QString(), QString(), occurrence});
    }
    if (items.isEmpty() && index.data(kBodyRole).toString().isEmpty()) {
      items.append({SelectKind::Component, index.data(kTargetRole).toJsonObject().value(QStringLiteral("component")).toString(),
                    QString(), QString(), occurrence});
    }
  }
  qDebug().noquote() << QStringLiteral("Cut target highlight: %1 item(s)").arg(items.size());
  emit highlightChanged(items);
}

bool CutTargetsPanel::eventFilter(QObject* watched, QEvent* event) {
  if (event->type() == QEvent::Leave || event->type() == QEvent::FocusOut) { emit highlightChanged({}); }
  // Wrapped reasons take the height of the new width (mitcad#1789).
  if (m_wraps && watched == m_tree->viewport() && event->type() == QEvent::Resize &&
      static_cast<QResizeEvent*>(event)->size().width() != static_cast<QResizeEvent*>(event)->oldSize().width()) {
    QAbstractItemView* view = m_tree;
    QMetaObject::invokeMethod(view, [view] { view->doItemsLayout(); }, Qt::QueuedConnection);
  }
  return QWidget::eventFilter(watched, event);
}

void CutTargetsPanel::hideEvent(QHideEvent* event) {
  emit highlightChanged({});
  QWidget::hideEvent(event);
}
} // namespace mitcad
