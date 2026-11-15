// SPDX-License-Identifier: MIT
#include "MoveOccurrenceDialog.hpp"
#include "ModelMessages.hpp"

#include <algorithm>
#include <exception>
#include <utility>

#include <QCheckBox>
#include <QCoreApplication>
#include <QDebug>
#include <QDialog>
#include <QDialogButtonBox>
#include <QFont>
#include <QFontMetrics>
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
#include <QRect>
#include <QSize>
#include <QStringList>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "Dialogs.hpp"
#include "PanelTree.hpp"

namespace mitcad {
namespace {

// A row under an unavailable parent that says why (mitcad#1847).
constexpr int kReasonRole = Qt::UserRole + 1;

class MoveOccurrenceDialog final : public QDialog {
  Q_DECLARE_TR_FUNCTIONS(mitcad::MoveOccurrenceDialog)

public:
  MoveOccurrenceDialog(QWidget* parent, const HybridModel& model, const QStringList& occurrences)
      : QDialog(parent), m_model(model), m_occurrences(occurrences) {
    setObjectName(QStringLiteral("moveOccurrenceDialog"));
    setWindowTitle(tr("Move to Component"));
    resize(680, 640);
    auto* layout = new QVBoxLayout(this);
    m_heading = new QLabel;
    m_heading->setWordWrap(true);
    layout->addWidget(m_heading);
    // A rigid group's other members, offered to move with it (mitcad#1847).
    m_wholeGroup = new QCheckBox;
    m_wholeGroup->setObjectName(QStringLiteral("moveOccurrenceRigidGroup"));
    m_wholeGroup->setVisible(false);
    layout->addWidget(m_wholeGroup);
    auto* targetLabel = new QLabel(tr("New &parent:"));
    layout->addWidget(targetLabel);
    m_targets = new QTreeWidget;
    panel::setCompactIndentation(m_targets, panel::Indent::Tree);
    m_targets->setObjectName(QStringLiteral("moveOccurrenceDestinations"));
    m_targets->setHeaderLabels({tr("Component"), tr("Location")});
    m_targets->setSelectionMode(QAbstractItemView::SingleSelection);
    m_targets->setWordWrap(true);
    m_targets->header()->setSectionResizeMode(QHeaderView::Interactive);
    m_targets->header()->setStretchLastSection(true);
    m_targets->setColumnWidth(0, 200);
    targetLabel->setBuddy(m_targets);
    layout->addWidget(m_targets, 3);
    layout->addWidget(new QLabel(tr("What will move:")));
    m_details = new QPlainTextEdit;
    m_details->setObjectName(QStringLiteral("moveOccurrenceDetails"));
    m_details->setReadOnly(true);
    m_details->setMinimumWidth(0);
    layout->addWidget(m_details, 2);
    m_buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
    m_buttons->button(QDialogButtonBox::Ok)->setText(tr("&Move"));
    layout->addWidget(m_buttons);

    fetch(m_occurrences);
    // The rest of its rigid group, from the first answer: moved with it
    // unless unchecked.
    for (const QJsonValue& value : m_info.value(QStringLiteral("rigid_group")).toArray()) {
      const QJsonObject member = value.toObject();
      m_group << member.value(QStringLiteral("path")).toString();
      m_groupNames << member.value(QStringLiteral("name")).toString();
    }
    if (!m_group.isEmpty()) {
      m_wholeGroup->setText(
          tr("Move the &rest of its rigid group with it: %1").arg(m_groupNames.join(QStringLiteral(", "))));
      m_wholeGroup->setChecked(true);
      m_wholeGroup->setVisible(true);
      qDebug().noquote() << QStringLiteral("Move Component rigid group offered: %1")
                                .arg(m_groupNames.join(QStringLiteral(", ")));
      fetch(moving());
      connect(m_wholeGroup, &QCheckBox::toggled, this, [this](bool checked) {
        qDebug().noquote() << QStringLiteral("Move Component rigid group: %1")
                                  .arg(checked ? QStringLiteral("with it") : QStringLiteral("left out"));
        fetch(moving());
        build();
        refresh();
      });
    }
    build();
    connect(m_targets, &QTreeWidget::currentItemChanged, this, [this] { refresh(); });
    connect(m_targets->header(), &QHeaderView::sectionResized, this, [this] { fitReasons(); });
    connect(m_buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(m_buttons, &QDialogButtonBox::accepted, this, [this] { move(); });
    refresh();
    m_targets->setFocus();
  }

  // The occurrence's new path after a move.
  QString moved() const { return m_moved; }

private:
  // The occurrences that move: the given ones, and the rest of their rigid
  // group when it goes with them.
  QStringList moving() const {
    QStringList out = m_occurrences;
    if (m_wholeGroup->isChecked()) {
      out << m_group;
    }
    return out;
  }

  QString movingNames() const {
    QStringList names;
    for (const QJsonValue& value : m_info.value(QStringLiteral("occurrences")).toArray()) {
      names << value.toObject().value(QStringLiteral("name")).toString();
    }
    if (names.isEmpty()) {
      const QString name = m_info.value(QStringLiteral("name")).toString();
      names << (name.isEmpty() ? m_occurrences.value(0) : name);
    }
    return names.join(QStringLiteral(", "));
  }

  QJsonObject query(const QStringList& occurrences, const QString& parent, bool withParent) const {
    QJsonObject out{{QStringLiteral("query"), QStringLiteral("reparent_occurrence")},
                    {QStringLiteral("occurrence"), occurrences.value(0)}};
    if (occurrences.size() > 1) {
      out.insert(QStringLiteral("with"), QJsonArray::fromStringList(occurrences.mid(1)));
    }
    if (withParent) {
      out.insert(QStringLiteral("parent"), parent);
    }
    return out;
  }

  // Asks the model where `occurrences` can go.
  void fetch(const QStringList& occurrences) {
    m_error.clear();
    m_info = {};
    try {
      m_info = m_model.query(query(occurrences, QString(), false));
    } catch (const std::exception& e) {
      m_error = mitcad::modelMessage(e.what());
    }
  }

  // Lists the parents of the last answer, each unavailable one with its
  // reasons under it.
  void build() {
    const QString names = movingNames();
    const bool several = m_info.value(QStringLiteral("occurrences")).toArray().size() > 1;
    m_heading->setText(several ? tr("Move %1 with everything in them to another parent. They stay where they are "
                                    "in the design; only their place in the hierarchy changes.")
                                     .arg(names)
                               : tr("Move %1 (%2) with everything in it to another parent. It stays where it is in "
                                    "the design; only the place in the hierarchy changes.")
                                     .arg(names, m_info.value(QStringLiteral("path_name")).toString()));
    const QString chosen = currentRow().value(QStringLiteral("occurrence")).toString();
    const bool hadChoice = m_targets->currentItem() != nullptr;
    m_targets->clear();
    m_rows.clear();
    m_reasons.clear();
    // Resolve parents in a second pass: the model need not sort its rows.
    QHash<QString, QTreeWidgetItem*> items;
    QStringList logged;
    QStringList explained;
    QFont reasonFont = m_targets->font();
    reasonFont.setItalic(true);
    for (const QJsonValue& value : m_info.value(QStringLiteral("destinations")).toArray()) {
      const QJsonObject row = value.toObject();
      auto* item = new QTreeWidgetItem;
      const QString path = row.value(QStringLiteral("path")).toString();
      const bool current = row.value(QStringLiteral("current")).toBool();
      const bool eligible = row.value(QStringLiteral("eligible")).toBool();
      item->setText(0, row.value(QStringLiteral("name")).toString());
      item->setText(1, current ? tr("%1 (current parent)").arg(path) : path);
      // Unavailable rows stay selectable, so an unavailable parent never
      // prevents choosing a child of it; their reasons are listed under
      // them in words (mitcad#1847).
      const QString reason = row.value(QStringLiteral("reason")).toString();
      item->setToolTip(0, reason);
      item->setToolTip(1, reason);
      if (!eligible) {
        item->setForeground(0, palette().brush(QPalette::Disabled, QPalette::Text));
        item->setForeground(1, palette().brush(QPalette::Disabled, QPalette::Text));
        QStringList reasons;
        for (const QJsonValue& why : row.value(QStringLiteral("reasons")).toArray()) {
          reasons << why.toString();
        }
        if (reasons.isEmpty()) {
          reasons << (reason.isEmpty() ? tr("This parent is unavailable.") : reason);
        }
        for (const QString& why : std::as_const(reasons)) {
          // Not enabled: the arrow keys and clicks pass over it, so the
          // parents keep their places in the keyboard order.
          auto* line = new QTreeWidgetItem(item);
          line->setText(1, why);
          line->setToolTip(1, why);
          line->setFlags(Qt::ItemNeverHasChildren);
          line->setData(0, kReasonRole, true);
          line->setFont(1, reasonFont);
          line->setForeground(1, palette().brush(QPalette::Disabled, QPalette::Text));
          m_reasons.append(line);
          explained << QStringLiteral("%1: %2").arg(path, why);
        }
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
    fitReasons();
    if (hadChoice) {
      m_targets->setCurrentItem(items.value(chosen));
    }
    qDebug().noquote() << QStringLiteral("Move Component parents of %1: %2")
                              .arg(names, logged.join(QStringLiteral(" | ")));
    for (const QString& line : std::as_const(explained)) {
      qDebug().noquote() << QStringLiteral("Move Component reason shown under %1").arg(line);
    }
  }

  // The reason rows as tall as their wrapped text in the Location column:
  // a tree view sizes its rows by one line otherwise.
  void fitReasons() {
    if (m_reasons.isEmpty()) {
      return;
    }
    const int wrapWidth = std::max(120, m_targets->header()->sectionSize(1) - 12);
    for (QTreeWidgetItem* line : std::as_const(m_reasons)) {
      const QFontMetrics metrics(line->font(1));
      const QRect bounds =
          metrics.boundingRect(QRect(0, 0, wrapWidth, 100000), Qt::TextWordWrap, line->text(1));
      line->setSizeHint(1, QSize(wrapWidth, bounds.height() + 6));
    }
  }

  QJsonObject currentRow() const {
    const QTreeWidgetItem* item = m_targets->currentItem();
    // A reason row stands for the parent it explains.
    if (item != nullptr && item->data(0, kReasonRole).toBool()) {
      item = item->parent();
    }
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
        m_plan = m_model.query(query(moving(), row.value(QStringLiteral("occurrence")).toString(), true))
                     .value(QStringLiteral("plan"))
                     .toObject();
      } catch (const std::exception& e) {
        problems << mitcad::modelMessage(e.what());
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
    const QString names = movingNames();
    if (m_info.value(QStringLiteral("occurrences")).toArray().size() > 1) {
      lines << tr("%1, with:").arg(names);
    } else {
      lines << tr("%1, an occurrence of %2, with:").arg(names, m_info.value(QStringLiteral("component_name")).toString());
    }
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
                                    : tr("  %1's part of %2 moves to %3, a new feature of %4").arg(names, name, split, to));
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
    const QStringList occurrences = moving();
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("reparent_occurrence")},
                        {QStringLiteral("occurrence"), occurrences.value(0)},
                        {QStringLiteral("parent"), row.value(QStringLiteral("occurrence"))}};
    if (occurrences.size() > 1) {
      command.insert(QStringLiteral("with"), QJsonArray::fromStringList(occurrences.mid(1)));
    }
    QJsonObject result;
    if (!m_model.run(command, &result)) {
      return;
    }
    m_moved = result.value(QStringLiteral("path")).toString();
    qDebug().noquote() << QStringLiteral("Moved %1 to %2").arg(movingNames(), row.value(QStringLiteral("path")).toString());
    accept();
  }

  const HybridModel& m_model;
  QStringList m_occurrences;
  QStringList m_group;
  QStringList m_groupNames;
  QJsonObject m_info;
  QJsonObject m_plan;
  QString m_error;
  QString m_moved;
  QList<QTreeWidgetItem*> m_rows;
  QList<QTreeWidgetItem*> m_reasons;
  QLabel* m_heading = nullptr;
  QCheckBox* m_wholeGroup = nullptr;
  QTreeWidget* m_targets = nullptr;
  QPlainTextEdit* m_details = nullptr;
  QDialogButtonBox* m_buttons = nullptr;
};

} // namespace

QString moveOccurrence(QWidget* parent, const HybridModel& model, const QString& occurrence) {
  return moveOccurrence(parent, model, QStringList{occurrence});
}

QString moveOccurrence(QWidget* parent, const HybridModel& model, const QStringList& occurrences) {
  if (occurrences.isEmpty()) {
    return QString();
  }
  MoveOccurrenceDialog dialog(parent, model, occurrences);
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    return QString();
  }
  return dialog.moved();
}

} // namespace mitcad
