// SPDX-License-Identifier: MIT
#include "RemoteDialogs.hpp"

#include <algorithm>

#include <QButtonGroup>
#include <QDateTime>
#include <QDialog>
#include <QDialogButtonBox>
#include <QFileInfo>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QLabel>
#include <QPushButton>
#include <QRadioButton>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "VersionDialogs.hpp"

namespace mitcad {
namespace {

QLabel* note(const QString& text) {
  auto* label = new QLabel(text);
  label->setWordWrap(true);
  return label;
}

// "yyyy-MM-dd HH:mm" of a version's time (seconds since 1970).
QString when(const QJsonValue& seconds) {
  return QDateTime::fromSecsSinceEpoch(seconds.toInteger()).toLocalTime().toString(QStringLiteral("yyyy-MM-dd HH:mm"));
}

// A side of a conflict: its latest version's summary, who and when.
QString sideText(const QJsonObject& side, bool withAuthor) {
  const QJsonObject version = side.value(QStringLiteral("version")).toObject();
  QString text;
  if (side.value(QStringLiteral("blob")).isNull()) {
    text = QObject::tr("deleted");
  }
  if (!version.isEmpty()) {
    const QString summary = version.value(QStringLiteral("summary")).toString();
    const QString who = version.value(QStringLiteral("author")).toObject().value(QStringLiteral("name")).toString();
    const QString at = when(version.value(QStringLiteral("time")));
    const QString detail = withAuthor ? QObject::tr("%1 (%2, %3)").arg(summary, who, at)
                                      : QObject::tr("%1 (%2)").arg(summary, at);
    text = text.isEmpty() ? detail : QObject::tr("%1: %2").arg(text, detail);
  }
  return text;
}

QString kindText(const QString& kind) {
  if (kind == QLatin1String("deleted_theirs")) {
    return QObject::tr("deleted on the remote, changed here");
  }
  if (kind == QLatin1String("deleted_mine")) {
    return QObject::tr("deleted here, changed on the remote");
  }
  if (kind == QLatin1String("added_both")) {
    return QObject::tr("added on both sides");
  }
  return QObject::tr("changed on both sides");
}

QString choiceText(const QString& choice) {
  if (choice == QLatin1String("mine")) {
    return QObject::tr("mine");
  }
  if (choice == QLatin1String("theirs")) {
    return QObject::tr("theirs");
  }
  return QObject::tr("mine as a copy");
}

} // namespace

std::optional<QJsonObject> askResolveConflicts(QWidget* parent, const QJsonArray& conflicts,
                                               const std::function<QString(const QJsonObject& conflict)>& compare) {
  enum Column { kFile, kChange, kRemote, kHere, kKeep, kColumns };
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Resolve Sync Conflicts"));
  auto* layout = new QVBoxLayout(&dialog);
  layout->addWidget(note(
      QObject::tr("%n file(s) changed both in this project and on the remote. Choose what to keep of each; "
                  "nothing changes before Apply. Saving yours as a copy keeps both designs; the versions left "
                  "out stay in a backup of the project's history.",
                  nullptr, static_cast<int>(conflicts.size()))));
  auto* list = new QTreeWidget;
  list->setColumnCount(kColumns);
  list->setHeaderLabels({QObject::tr("File"), QObject::tr("Change"), QObject::tr("On the remote"),
                         QObject::tr("Here"), QObject::tr("Keep")});
  list->setRootIsDecorated(false);
  list->setAllColumnsShowFocus(true);
  list->header()->setStretchLastSection(false);
  layout->addWidget(list, 1);

  // The choices, by row.
  QStringList choices;
  QStringList logged;
  for (const QJsonValue& value : conflicts) {
    const QJsonObject conflict = value.toObject();
    const QString path = conflict.value(QStringLiteral("path")).toString();
    const QJsonObject mine = conflict.value(QStringLiteral("mine")).toObject();
    const QJsonObject theirs = conflict.value(QStringLiteral("theirs")).toObject();
    const bool copies = conflict.value(QStringLiteral("choices")).toArray().contains(QStringLiteral("copy"));
    // Nothing is lost: a copy where there can be one, else mine (theirs
    // stays in the history after it).
    choices << (copies ? QStringLiteral("copy") : QStringLiteral("mine"));
    auto* item = new QTreeWidgetItem(list);
    item->setText(kFile, path);
    item->setText(kChange, kindText(conflict.value(QStringLiteral("kind")).toString()));
    item->setText(kRemote, sideText(theirs, true));
    const int unsent = mine.value(QStringLiteral("versions")).toInt();
    item->setText(kHere, QObject::tr("%1; %n version(s) not sent", nullptr, unsent).arg(sideText(mine, false)));
    item->setText(kKeep, choiceText(choices.constLast()));
    logged << QStringLiteral("%1 (%2; remote %3 by %4; here %5 version(s))")
                  .arg(path, conflict.value(QStringLiteral("kind")).toString(),
                       theirs.value(QStringLiteral("version")).toObject().value(QStringLiteral("short_id")).toString(),
                       theirs.value(QStringLiteral("version"))
                           .toObject()
                           .value(QStringLiteral("author"))
                           .toObject()
                           .value(QStringLiteral("name"))
                           .toString())
                  .arg(unsent);
  }
  for (int column = 0; column < kColumns; ++column) {
    list->resizeColumnToContents(column);
  }
  // The versions' descriptions in their tooltips when long: the choice
  // stays in sight.
  for (const int column : {kRemote, kHere}) {
    list->setColumnWidth(column, std::min(list->columnWidth(column), 260));
    for (int row = 0; row < list->topLevelItemCount(); ++row) {
      QTreeWidgetItem* item = list->topLevelItem(row);
      item->setToolTip(column, item->text(column));
    }
  }

  auto* chosenBox = new QHBoxLayout;
  auto* forFile = new QLabel;
  chosenBox->addWidget(forFile);
  auto* keepMine = new QRadioButton(QObject::tr("&Keep mine"));
  keepMine->setToolTip(QObject::tr("Your versions of the file come after the remote's: the file is yours"));
  auto* takeTheirs = new QRadioButton(QObject::tr("Take &theirs"));
  takeTheirs->setToolTip(QObject::tr("The remote's file; your versions of it stay only in the backup"));
  auto* copy = new QRadioButton(QObject::tr("Save mine as a &copy"));
  auto* group = new QButtonGroup(&dialog);
  group->addButton(keepMine);
  group->addButton(takeTheirs);
  group->addButton(copy);
  chosenBox->addWidget(keepMine);
  chosenBox->addWidget(takeTheirs);
  chosenBox->addWidget(copy);
  chosenBox->addStretch(1);
  auto* compareButton = new QPushButton(QObject::tr("Co&mpare..."));
  compareButton->setToolTip(QObject::tr("What differs between the remote's design and yours"));
  chosenBox->addWidget(compareButton);
  layout->addLayout(chosenBox);

  auto* allRow = new QHBoxLayout;
  allRow->addWidget(new QLabel(QObject::tr("For all:")));
  auto* allMine = new QPushButton(QObject::tr("Keep All Mine"));
  auto* allTheirs = new QPushButton(QObject::tr("Take All Theirs"));
  auto* allCopies = new QPushButton(QObject::tr("Copy All Mine"));
  allRow->addWidget(allMine);
  allRow->addWidget(allTheirs);
  allRow->addWidget(allCopies);
  allRow->addStretch(1);
  layout->addLayout(allRow);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  buttons->button(QDialogButtonBox::Ok)->setText(QObject::tr("Apply"));
  layout->addWidget(buttons);
  for (QPushButton* button : {compareButton, allMine, allTheirs, allCopies}) {
    button->setAutoDefault(false); // Enter applies
  }
  buttons->button(QDialogButtonBox::Ok)->setDefault(true);

  const auto row = [list] { return list->indexOfTopLevelItem(list->currentItem()); };
  const auto conflictAt = [&conflicts](int index) { return conflicts.at(index).toObject(); };
  const auto set = [&](int index, const QString& choice) {
    if (index < 0 || index >= choices.size() ||
        !conflictAt(index).value(QStringLiteral("choices")).toArray().contains(choice)) {
      return;
    }
    choices[index] = choice;
    list->topLevelItem(index)->setText(kKeep, choiceText(choice));
  };
  const auto selected = [&] {
    const int index = row();
    const bool there = index >= 0;
    for (QAbstractButton* button : group->buttons()) {
      button->setEnabled(there);
    }
    compareButton->setEnabled(false);
    if (!there) {
      forFile->clear();
      return;
    }
    const QJsonObject conflict = conflictAt(index);
    const QJsonArray allowed = conflict.value(QStringLiteral("choices")).toArray();
    forFile->setText(QObject::tr("%1:").arg(QFileInfo(conflict.value(QStringLiteral("path")).toString()).fileName()));
    copy->setEnabled(allowed.contains(QStringLiteral("copy")));
    const QString copyPath = conflict.value(QStringLiteral("copy")).toString();
    copy->setToolTip(copyPath.isEmpty() ? QObject::tr("Files of the project's own settings have no copy")
                                        : QObject::tr("The remote's file, and yours next to it as %1").arg(copyPath));
    const QString choice = choices.at(index);
    QRadioButton* button = choice == QLatin1String("mine")     ? keepMine
                           : choice == QLatin1String("theirs") ? takeTheirs
                                                               : copy;
    const QSignalBlocker blocker(group);
    button->setChecked(true);
    const bool bothThere = !conflict.value(QStringLiteral("mine")).toObject().value(QStringLiteral("blob")).isNull() &&
                           !conflict.value(QStringLiteral("theirs")).toObject().value(QStringLiteral("blob")).isNull();
    compareButton->setEnabled(bothThere && static_cast<bool>(compare));
  };
  QObject::connect(list, &QTreeWidget::currentItemChanged, &dialog, selected);
  QObject::connect(keepMine, &QRadioButton::toggled, &dialog, [&](bool on) {
    if (on) {
      set(row(), QStringLiteral("mine"));
    }
  });
  QObject::connect(takeTheirs, &QRadioButton::toggled, &dialog, [&](bool on) {
    if (on) {
      set(row(), QStringLiteral("theirs"));
    }
  });
  QObject::connect(copy, &QRadioButton::toggled, &dialog, [&](bool on) {
    if (on) {
      set(row(), QStringLiteral("copy"));
    }
  });
  const auto all = [&](const QString& choice) {
    for (int index = 0; index < choices.size(); ++index) {
      set(index, choice);
    }
    selected();
  };
  QObject::connect(allMine, &QPushButton::clicked, &dialog, [&] { all(QStringLiteral("mine")); });
  QObject::connect(allTheirs, &QPushButton::clicked, &dialog, [&] { all(QStringLiteral("theirs")); });
  QObject::connect(allCopies, &QPushButton::clicked, &dialog, [&] { all(QStringLiteral("copy")); });
  QObject::connect(compareButton, &QPushButton::clicked, &dialog, [&] {
    const int index = row();
    if (index < 0 || !compare) {
      return;
    }
    const QJsonObject conflict = conflictAt(index);
    const QString path = conflict.value(QStringLiteral("path")).toString();
    const QString text = compare(conflict);
    qInfo().noquote() << QStringLiteral("Resolve Sync Conflicts compare %1: %2")
                             .arg(path, text.split(QLatin1Char('\n'), Qt::SkipEmptyParts).join(QStringLiteral(" | ")));
    showComparison(&dialog, QObject::tr("Compare %1: the remote's and yours").arg(path), text);
  });
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  if (list->topLevelItemCount() > 0) {
    list->setCurrentItem(list->topLevelItem(0));
  }
  selected();
  dialog.resize(900, 420);
  qInfo().noquote() << QStringLiteral("Resolve Sync Conflicts dialog: %1").arg(logged.join(QStringLiteral(" | ")));
  if (dialog.exec() != QDialog::Accepted) {
    qInfo().noquote() << QStringLiteral("Resolve Sync Conflicts cancelled");
    return std::nullopt;
  }
  QJsonObject chosen;
  for (int index = 0; index < choices.size(); ++index) {
    chosen.insert(conflictAt(index).value(QStringLiteral("path")).toString(), choices.at(index));
  }
  return chosen;
}

} // namespace mitcad
