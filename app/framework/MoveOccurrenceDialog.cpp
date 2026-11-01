// SPDX-License-Identifier: MIT
#include "MoveOccurrenceDialog.hpp"

#include <exception>
#include <utility>

#include <QDebug>
#include <QDialog>
#include <QDialogButtonBox>
#include <QHash>
#include <QHeaderView>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QLabel>
#include <QList>
#include <QPalette>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QStringList>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "Dialogs.hpp"

namespace mitcad {
namespace {

class MoveOccurrenceDialog final : public QDialog {
public:
  MoveOccurrenceDialog(QWidget* parent, const HybridModel& model, const QString& occurrence)
      : QDialog(parent), m_model(model), m_occurrence(occurrence) {
    setObjectName(QStringLiteral("moveOccurrenceDialog"));
    setWindowTitle(tr("Move to Component"));
    resize(620, 600);
    auto* layout = new QVBoxLayout(this);
    m_heading = new QLabel;
    m_heading->setWordWrap(true);
    layout->addWidget(m_heading);
    auto* targetLabel = new QLabel(tr("New &parent:"));
    layout->addWidget(targetLabel);
    m_targets = new QTreeWidget;
    m_targets->setObjectName(QStringLiteral("moveOccurrenceDestinations"));
    m_targets->setHeaderLabels({tr("Component"), tr("Location")});
    m_targets->setSelectionMode(QAbstractItemView::SingleSelection);
    m_targets->header()->setSectionResizeMode(QHeaderView::Interactive);
    m_targets->header()->setStretchLastSection(true);
    m_targets->setColumnWidth(0, 200);
    targetLabel->setBuddy(m_targets);
    layout->addWidget(m_targets, 2);
    layout->addWidget(new QLabel(tr("What will move:")));
    m_details = new QPlainTextEdit;
    m_details->setObjectName(QStringLiteral("moveOccurrenceDetails"));
    m_details->setReadOnly(true);
    m_details->setMinimumWidth(0);
    layout->addWidget(m_details, 2);
    m_buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
    m_buttons->button(QDialogButtonBox::Ok)->setText(tr("&Move"));
    layout->addWidget(m_buttons);

    try {
      m_info = m_model.query(query(QString(), false));
    } catch (const std::exception& e) {
      m_error = QString::fromUtf8(e.what());
    }
    const QString name = m_info.value(QStringLiteral("name")).toString();
    m_heading->setText(tr("Move %1 (%2) with everything in it to another parent. It stays where it is in "
                          "the design; only the place in the hierarchy changes.")
                           .arg(name.isEmpty() ? m_occurrence : name,
                                m_info.value(QStringLiteral("path_name")).toString()));
    // Resolve parents in a second pass: the model need not sort its rows.
    QHash<QString, QTreeWidgetItem*> items;
    QStringList logged;
    for (const QJsonValue& value : m_info.value(QStringLiteral("destinations")).toArray()) {
      const QJsonObject row = value.toObject();
      auto* item = new QTreeWidgetItem;
      const QString path = row.value(QStringLiteral("path")).toString();
      const bool current = row.value(QStringLiteral("current")).toBool();
      const bool eligible = row.value(QStringLiteral("eligible")).toBool();
      item->setText(0, row.value(QStringLiteral("name")).toString());
      item->setText(1, current ? tr("%1 (current parent)").arg(path) : path);
      // Unavailable rows stay selectable, so their reason can be read, and
      // so an unavailable parent never prevents choosing a child of it.
      const QString reason = row.value(QStringLiteral("reason")).toString();
      item->setToolTip(0, reason);
      item->setToolTip(1, reason);
      if (!eligible) {
        item->setForeground(0, palette().brush(QPalette::Disabled, QPalette::Text));
        item->setForeground(1, palette().brush(QPalette::Disabled, QPalette::Text));
      }
      item->setData(0, Qt::UserRole, row);
      items.insert(row.value(QStringLiteral("occurrence")).toString(), item);
      m_rows.append(item);
      logged << (eligible ? path : QStringLiteral("%1 (unavailable)").arg(path));
    }
    for (QTreeWidgetItem* item : std::as_const(m_rows)) {
      const QJsonObject row = item->data(0, Qt::UserRole).toJsonObject();
      const QString occurrencePath = row.value(QStringLiteral("occurrence")).toString();
      QTreeWidgetItem* parentItem =
          occurrencePath.isEmpty() ? nullptr : items.value(row.value(QStringLiteral("parent_occurrence")).toString());
      if (parentItem != nullptr && parentItem != item) {
        parentItem->addChild(item);
      } else {
        m_targets->addTopLevelItem(item);
      }
    }
    m_targets->expandAll();
    qDebug().noquote() << QStringLiteral("Move Component parents of %1: %2")
                              .arg(name, logged.join(QStringLiteral(" | ")));
    connect(m_targets, &QTreeWidget::currentItemChanged, this, [this] { refresh(); });
    connect(m_buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(m_buttons, &QDialogButtonBox::accepted, this, [this] { move(); });
    refresh();
    m_targets->setFocus();
  }

  // The occurrence's new path after a move.
  QString moved() const { return m_moved; }

private:
  QJsonObject query(const QString& parent, bool withParent) const {
    QJsonObject out{{QStringLiteral("query"), QStringLiteral("reparent_occurrence")},
                    {QStringLiteral("occurrence"), m_occurrence}};
    if (withParent) {
      out.insert(QStringLiteral("parent"), parent);
    }
    return out;
  }

  QJsonObject currentRow() const {
    const QTreeWidgetItem* item = m_targets->currentItem();
    return item != nullptr ? item->data(0, Qt::UserRole).toJsonObject() : QJsonObject();
  }

  void refresh() {
    QStringList problems;
    if (!m_error.isEmpty()) {
      problems << m_error;
    }
    for (const QJsonValue& problem : m_info.value(QStringLiteral("problems")).toArray()) {
      problems << problem.toString();
    }
    m_plan = {};
    const QJsonObject row = currentRow();
    if (row.isEmpty()) {
      problems << tr("Select the new parent.");
    } else if (!row.value(QStringLiteral("eligible")).toBool()) {
      const QString reason = row.value(QStringLiteral("reason")).toString();
      problems << (reason.isEmpty() ? tr("This parent is unavailable.") : reason);
    } else {
      // The model validates the move the command would make.
      try {
        m_plan = m_model.query(query(row.value(QStringLiteral("occurrence")).toString(), true))
                     .value(QStringLiteral("plan"))
                     .toObject();
      } catch (const std::exception& e) {
        problems << QString::fromUtf8(e.what());
      }
      for (const QJsonValue& problem : m_plan.value(QStringLiteral("problems")).toArray()) {
        problems << problem.toString();
      }
    }
    problems.removeDuplicates();
    QStringList lines;
    if (!problems.isEmpty()) {
      lines << problems << QString();
    }
    const QJsonObject contents = m_info.value(QStringLiteral("contents")).toObject();
    lines << tr("%1, an occurrence of %2, with:")
                 .arg(m_info.value(QStringLiteral("name")).toString(),
                      m_info.value(QStringLiteral("component_name")).toString());
    QStringList inside;
    for (const QJsonValue& value : contents.value(QStringLiteral("occurrences")).toArray()) {
      inside << QStringLiteral("  %1").arg(value.toString());
    }
    lines << tr("Components in it: %1").arg(inside.isEmpty() ? tr("none") : QString::number(inside.size()));
    lines << inside;
    lines << tr("Bodies: %1, features: %2")
                 .arg(contents.value(QStringLiteral("bodies")).toInt())
                 .arg(contents.value(QStringLiteral("features")).toInt());
    const bool ready = problems.isEmpty() && m_plan.value(QStringLiteral("eligible")).toBool();
    if (ready) {
      lines << QString()
            << tr("From %1 to %2")
                   .arg(m_plan.value(QStringLiteral("from_path")).toString(),
                        m_plan.value(QStringLiteral("parent_path")).toString());
      QStringList updates;
      for (const QJsonValue& value : m_plan.value(QStringLiteral("updates")).toArray()) {
        updates << QStringLiteral("  %1").arg(value.toString());
      }
      lines << tr("References that follow it:") << (updates.isEmpty() ? QStringList{tr("  none")} : updates);
      // The joints, constraints, rigid groups and moves that place it go
      // with it into the new parent (mitcad#301).
      QStringList carried;
      QStringList logged;
      for (const QJsonValue& value : m_plan.value(QStringLiteral("features")).toArray()) {
        const QJsonObject feature = value.toObject();
        const QString name = feature.value(QStringLiteral("name")).toString();
        const QString from = feature.value(QStringLiteral("from_name")).toString();
        const QString to = feature.value(QStringLiteral("to_name")).toString();
        const QString split = feature.value(QStringLiteral("split")).toString();
        carried << (split.isEmpty() ? tr("  %1 moves from %2 to %3").arg(name, from, to)
                                    : tr("  %1's part of %2 moves to %3, a new feature of %4")
                                          .arg(m_info.value(QStringLiteral("name")).toString(), name, split, to));
        logged << QStringLiteral("%1 -> %2").arg(name, split.isEmpty() ? to : QStringLiteral("%1 in %2").arg(split, to));
      }
      if (!carried.isEmpty()) {
        lines << tr("Features that place it and move with it:") << carried;
        qDebug().noquote() << QStringLiteral("Move Component features: %1").arg(logged.join(QStringLiteral(", ")));
      }
    }
    m_details->setPlainText(lines.join(QLatin1Char('\n')));
    m_buttons->button(QDialogButtonBox::Ok)->setEnabled(ready);
    if (!row.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Move Component parent: %1, %2")
                                .arg(row.value(QStringLiteral("path")).toString(),
                                     ready ? QStringLiteral("ok")
                                           : QStringLiteral("unavailable: %1").arg(problems.join(QStringLiteral("; "))));
    }
  }

  void move() {
    // Validated again before the single model command. A refused move
    // leaves this dialog open with its choice intact.
    refresh();
    if (!m_buttons->button(QDialogButtonBox::Ok)->isEnabled()) {
      return;
    }
    const QJsonObject row = currentRow();
    const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("reparent_occurrence")},
                              {QStringLiteral("occurrence"), m_occurrence},
                              {QStringLiteral("parent"), row.value(QStringLiteral("occurrence"))}};
    QJsonObject result;
    if (!m_model.run(command, &result)) {
      return;
    }
    m_moved = result.value(QStringLiteral("path")).toString();
    qDebug().noquote() << QStringLiteral("Moved %1 to %2")
                              .arg(m_info.value(QStringLiteral("name")).toString(),
                                   row.value(QStringLiteral("path")).toString());
    accept();
  }

  const HybridModel& m_model;
  QString m_occurrence;
  QJsonObject m_info;
  QJsonObject m_plan;
  QString m_error;
  QString m_moved;
  QList<QTreeWidgetItem*> m_rows;
  QLabel* m_heading = nullptr;
  QTreeWidget* m_targets = nullptr;
  QPlainTextEdit* m_details = nullptr;
  QDialogButtonBox* m_buttons = nullptr;
};

} // namespace

QString moveOccurrence(QWidget* parent, const HybridModel& model, const QString& occurrence) {
  MoveOccurrenceDialog dialog(parent, model, occurrence);
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    return QString();
  }
  return dialog.moved();
}

} // namespace mitcad
