// SPDX-License-Identifier: MIT
// Move to Component... on an occurrence (mitcad#219; Move to... before
// mitcad#735): the actual modal parent picker with a model boundary that
// records every mutation. The move itself, its geometry, references and
// undo have core tests.
#include "framework/MoveOccurrenceDialog.hpp"

#include <cstdio>
#include <functional>

#include <QApplication>
#include <QCheckBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QKeyEvent>
#include <QList>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QStringList>
#include <QTimer>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>

namespace {

QJsonObject parentRow(const QString& component, const QString& name, const QString& occurrence,
                      const QString& parent, const QString& path, const QString& reason = QString(),
                      bool current = false) {
  return {{QStringLiteral("component"), component}, {QStringLiteral("name"), name},
          {QStringLiteral("occurrence"), occurrence}, {QStringLiteral("parent_occurrence"), parent},
          {QStringLiteral("path"), path}, {QStringLiteral("current"), current},
          {QStringLiteral("eligible"), reason.isEmpty()}, {QStringLiteral("reason"), reason}};
}

QTreeWidgetItem* parentItem(QTreeWidget* tree, const QString& occurrence) {
  for (QTreeWidgetItemIterator it(tree); *it != nullptr; ++it) {
    if ((*it)->data(0, Qt::UserRole).toJsonObject().value(QStringLiteral("occurrence")).toString() == occurrence) {
      return *it;
    }
  }
  return nullptr;
}

} // namespace

int moveOccurrenceTests() {
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) {
      std::fprintf(stderr, "move occurrence: %s\n", message);
      ++failures;
    }
  };
  // Part:1 in Assembly:1, with Pin:1 in it; Bracket:1 beside Assembly.
  // Child rows come first: the dialog must place them by their parents.
  QJsonObject info{
      {QStringLiteral("name"), QStringLiteral("Part:1")},
      {QStringLiteral("path_name"), QStringLiteral("Root/Assembly:1/Part:1")},
      {QStringLiteral("component_name"), QStringLiteral("Part")},
      {QStringLiteral("contents"),
       QJsonObject{{QStringLiteral("occurrences"), QJsonArray{QStringLiteral("Pin:1")}},
                   {QStringLiteral("components"), 2},
                   {QStringLiteral("bodies"), 2},
                   {QStringLiteral("features"), 4}}},
      {QStringLiteral("problems"), QJsonArray{}},
      {QStringLiteral("destinations"),
       QJsonArray{parentRow(QStringLiteral("C2"), QStringLiteral("Part"), QStringLiteral("O1/O2"), QStringLiteral("O1"),
                            QStringLiteral("Root/Assembly:1/Part:1"),
                            QStringLiteral("Part:1 cannot go into Part, which is its own component")),
                  parentRow(QStringLiteral("C0"), QStringLiteral("Root"), QString(), QString(), QStringLiteral("Root")),
                  parentRow(QStringLiteral("C1"), QStringLiteral("Assembly"), QStringLiteral("O1"), QString(),
                            QStringLiteral("Root/Assembly:1"), QStringLiteral("Part:1 is already in Assembly"), true),
                  parentRow(QStringLiteral("C3"), QStringLiteral("Bracket"), QStringLiteral("O4"), QString(),
                            QStringLiteral("Root/Bracket:1"))}}};
  QString planProblem;
  QJsonArray planFeatures;
  QList<QJsonObject> commands;
  QList<QJsonObject> queries;
  bool runSucceeds = true;
  const mitcad::HybridModel model{
      [&](const QJsonObject& query) {
        queries.append(query);
        QJsonObject answer = info;
        if (query.contains(QStringLiteral("parent"))) {
          const QString parent = query.value(QStringLiteral("parent")).toString();
          QJsonObject plan{{QStringLiteral("eligible"), planProblem.isEmpty()},
                           {QStringLiteral("problems"),
                            planProblem.isEmpty() ? QJsonArray{} : QJsonArray{planProblem}},
                           {QStringLiteral("updates"), QJsonArray{QStringLiteral("Sketch2"), QStringLiteral("Isolate")}},
                           {QStringLiteral("features"), planFeatures},
                           {QStringLiteral("from_path"), QStringLiteral("Root/Assembly:1")},
                           {QStringLiteral("parent_path"),
                            parent.isEmpty() ? QStringLiteral("Root") : QStringLiteral("Root/Bracket:1")}};
          answer.insert(QStringLiteral("plan"), plan);
        }
        return answer;
      },
      [&](const QJsonObject& command, QJsonObject* result) {
        commands.append(command);
        if (result != nullptr) {
          *result = {{QStringLiteral("path"), QStringLiteral("O2")}};
        }
        return runSucceeds;
      }};
  const auto exercise = [&](const std::function<void(QDialog*, QTreeWidget*, QPushButton*)>& action) {
    commands.clear();
    queries.clear();
    QTimer watchdog;
    watchdog.setSingleShot(true);
    QObject::connect(&watchdog, &QTimer::timeout, [&] {
      check(false, "dialog interaction must finish");
      if (auto* dialog = qobject_cast<QDialog*>(QApplication::activeModalWidget())) {
        dialog->reject();
      }
    });
    watchdog.start(3000);
    QTimer::singleShot(0, [&] {
      auto* dialog = qobject_cast<QDialog*>(QApplication::activeModalWidget());
      check(dialog != nullptr && dialog->objectName() == QStringLiteral("moveOccurrenceDialog"),
            "opens the parent dialog");
      if (dialog == nullptr) {
        return;
      }
      auto* tree = dialog->findChild<QTreeWidget*>(QStringLiteral("moveOccurrenceDestinations"));
      auto* buttons = dialog->findChild<QDialogButtonBox*>();
      check(tree != nullptr && buttons != nullptr, "parent controls exist");
      if (tree == nullptr || buttons == nullptr) {
        dialog->reject();
        return;
      }
      check(commands.isEmpty(), "opening and previewing do not change the model");
      action(dialog, tree, buttons->button(QDialogButtonBox::Ok));
    });
    const QString moved = mitcad::moveOccurrence(nullptr, model, QStringLiteral("O1/O2"));
    watchdog.stop();
    return moved;
  };
  const auto details = [](QDialog* dialog) {
    return dialog->findChild<QPlainTextEdit*>(QStringLiteral("moveOccurrenceDetails"))->toPlainText();
  };

  check(exercise([&](QDialog* dialog, QTreeWidget* tree, QPushButton* move) {
          check(tree->topLevelItemCount() == 1, "the root holds every parent");
          QTreeWidgetItem* own = parentItem(tree, QStringLiteral("O1/O2"));
          check(own != nullptr && own->parent() == parentItem(tree, QStringLiteral("O1")),
                "a nested parent is under its own parent");
          QTreeWidgetItem* current = parentItem(tree, QStringLiteral("O1"));
          check(current != nullptr && current->text(1).contains(QStringLiteral("(current parent)")),
                "the current parent is marked");
          check(!move->isEnabled() || tree->currentItem() != nullptr, "a parent must be chosen first");
          dialog->reject();
        }).isEmpty(),
        "cancel moves nothing");
  check(commands.isEmpty(), "cancel sends no command");

  check(exercise([&](QDialog* dialog, QTreeWidget* tree, QPushButton* move) {
          tree->setCurrentItem(parentItem(tree, QStringLiteral("O1")));
          check(!move->isEnabled(), "the current parent cannot be chosen");
          check(details(dialog).contains(QStringLiteral("already in Assembly")), "the reason is shown");
          tree->setCurrentItem(parentItem(tree, QStringLiteral("O1/O2")));
          check(!move->isEnabled() && details(dialog).contains(QStringLiteral("its own component")),
                "its own component is refused with the reason");
          move->click();
          check(commands.isEmpty(), "an unavailable parent sends no command");
          dialog->reject();
        }).isEmpty(),
        "unavailable parents move nothing");

  check(exercise([&](QDialog* dialog, QTreeWidget* tree, QPushButton* move) {
          tree->setCurrentItem(parentItem(tree, QString()));
          check(move->isEnabled(), "the root can be chosen");
          const QString text = details(dialog);
          check(text.contains(QStringLiteral("Pin:1")) && text.contains(QStringLiteral("Sketch2")) &&
                    text.contains(QStringLiteral("Root/Assembly:1")),
                "what moves and what follows are shown before moving");
          check(queries.last().value(QStringLiteral("parent")).toString().isEmpty() &&
                    queries.last().contains(QStringLiteral("parent")),
                "the preview asks the model about the chosen parent");
          move->click();
        }) == QStringLiteral("O2"),
        "a move to the root returns the new path");
  check(commands.size() == 1, "one model command moves it");
  if (!commands.isEmpty()) {
    const QJsonObject command = commands.first();
    check(command.value(QStringLiteral("cmd")).toString() == QStringLiteral("reparent_occurrence") &&
              command.value(QStringLiteral("occurrence")).toString() == QStringLiteral("O1/O2") &&
              command.contains(QStringLiteral("parent")) && command.value(QStringLiteral("parent")).toString().isEmpty(),
          "the command names the occurrence's path and the root");
  }

  check(!exercise([&](QDialog*, QTreeWidget* tree, QPushButton* move) {
           tree->setCurrentItem(parentItem(tree, QStringLiteral("O4")));
           move->click();
         }).isEmpty(),
        "a move into another component succeeds");
  check(commands.size() == 1 && commands.first().value(QStringLiteral("parent")).toString() == QStringLiteral("O4"),
        "the command names the chosen component's occurrence");

  planProblem = QStringLiteral("Joint1: Part:1 would no longer be in Assembly:1, the occurrence it moves");
  check(exercise([&](QDialog* dialog, QTreeWidget* tree, QPushButton* move) {
          tree->setCurrentItem(parentItem(tree, QStringLiteral("O4")));
          check(!move->isEnabled() && details(dialog).contains(QStringLiteral("Joint1")),
                "a blocker of the model disables Move and is shown");
          move->click();
          dialog->reject();
        }).isEmpty(),
        "a blocked move moves nothing");
  check(commands.isEmpty(), "a blocked move sends no command");
  planProblem.clear();

  // A joint and a split Move that place Part:1 go with it (mitcad#301):
  // the parent is available and the details name them.
  planFeatures = QJsonArray{
      QJsonObject{{QStringLiteral("name"), QStringLiteral("Joint1")},
                  {QStringLiteral("from_name"), QStringLiteral("Assembly")},
                  {QStringLiteral("to_name"), QStringLiteral("Root")},
                  {QStringLiteral("split"), QJsonValue()}},
      QJsonObject{{QStringLiteral("name"), QStringLiteral("Move1")},
                  {QStringLiteral("from_name"), QStringLiteral("Assembly")},
                  {QStringLiteral("to_name"), QStringLiteral("Root")},
                  {QStringLiteral("split"), QStringLiteral("Move2")}}};
  check(exercise([&](QDialog* dialog, QTreeWidget* tree, QPushButton* move) {
          tree->setCurrentItem(parentItem(tree, QString()));
          check(move->isEnabled(), "a parent whose placing features go along can be chosen");
          const QString text = details(dialog);
          check(text.contains(QStringLiteral("Joint1 moves from Assembly to Root")),
                "the joint that moves along is listed");
          check(text.contains(QStringLiteral("Part:1's part of Move1 moves to Move2, a new feature of Root")),
                "a split feature is listed with its new feature");
          move->click();
        }) == QStringLiteral("O2"),
        "a move with its placing features returns the new path");
  check(commands.size() == 1, "one model command moves the occurrence and its features");
  planFeatures = QJsonArray{};

  runSucceeds = false;
  check(exercise([&](QDialog* dialog, QTreeWidget* tree, QPushButton* move) {
          tree->setCurrentItem(parentItem(tree, QString()));
          move->click();
          check(dialog->isVisible() && commands.size() == 1, "a refused command keeps the dialog open");
          dialog->reject();
        }).isEmpty(),
        "a refused move can be cancelled");
  runSucceeds = true;

  // Every unavailable parent lists its reasons under it, each in a row of
  // its own that the arrow keys pass over (mitcad#1847).
  const QJsonObject plain = info;
  QJsonArray destinations = info.value(QStringLiteral("destinations")).toArray();
  QJsonObject bracket = destinations.at(3).toObject();
  bracket.insert(QStringLiteral("eligible"), false);
  bracket.insert(QStringLiteral("reason"), QStringLiteral("Joint1: it joins Part:1 to Shaft:1; RigidGroup1: it holds"));
  bracket.insert(QStringLiteral("reasons"),
                 QJsonArray{QStringLiteral("Joint1: it joins Part:1 to Shaft:1, which is not in Bracket"),
                            QStringLiteral("RigidGroup1: it holds Part:1 rigidly to Roller:1")});
  destinations.replace(3, bracket);
  info.insert(QStringLiteral("destinations"), destinations);
  const auto reasonsUnder = [](QTreeWidgetItem* item) {
    QStringList out;
    for (int i = 0; item != nullptr && i < item->childCount(); ++i) {
      if (item->child(i)->data(0, Qt::UserRole + 1).toBool()) {
        out << item->child(i)->text(1);
      }
    }
    return out;
  };
  check(exercise([&](QDialog* dialog, QTreeWidget* tree, QPushButton*) {
          const QStringList shown = reasonsUnder(parentItem(tree, QStringLiteral("O4")));
          check(shown.size() == 2 && shown.first().contains(QStringLiteral("not in Bracket")) &&
                    shown.last().contains(QStringLiteral("rigidly")),
                "each reason of an unavailable parent is shown under it");
          check(reasonsUnder(parentItem(tree, QStringLiteral("O1"))) ==
                    QStringList{QStringLiteral("Part:1 is already in Assembly")},
                "the current parent says why under it, from its joined reason");
          check(reasonsUnder(parentItem(tree, QString())).isEmpty(), "an available parent has no reason row");
          // Assembly:1's reason row lies between it and Part:1.
          tree->setCurrentItem(parentItem(tree, QStringLiteral("O1")));
          QKeyEvent down(QEvent::KeyPress, Qt::Key_Down, Qt::NoModifier);
          QApplication::sendEvent(tree, &down);
          check(tree->currentItem() == parentItem(tree, QStringLiteral("O1/O2")), "Down passes over the reason row");
          dialog->reject();
        }).isEmpty(),
        "reasons under the parents move nothing");

  // One occurrence of a rigid group: the rest of the group is offered,
  // checked, and goes with it in the queries and the one command.
  info.insert(QStringLiteral("rigid_group"),
              QJsonArray{QJsonObject{{QStringLiteral("occurrence"), QStringLiteral("O5")},
                                     {QStringLiteral("name"), QStringLiteral("Roller:1")},
                                     {QStringLiteral("path"), QStringLiteral("O1/O5")},
                                     {QStringLiteral("path_name"), QStringLiteral("Root/Assembly:1/Roller:1")}}});
  check(exercise([&](QDialog* dialog, QTreeWidget* tree, QPushButton* move) {
          auto* group = dialog->findChild<QCheckBox*>(QStringLiteral("moveOccurrenceRigidGroup"));
          check(group != nullptr && group->isVisibleTo(dialog) && group->isChecked() &&
                    group->text().contains(QStringLiteral("Roller:1")),
                "the rest of the rigid group is offered, checked");
          check(queries.last().value(QStringLiteral("with")).toArray() == QJsonArray{QStringLiteral("O1/O5")},
                "the parents are asked for with the group");
          if (group != nullptr) {
            group->setChecked(false);
            check(!queries.last().contains(QStringLiteral("with")), "unchecked, the occurrence is asked for alone");
            group->setChecked(true);
          }
          tree->setCurrentItem(parentItem(tree, QString()));
          check(queries.last().value(QStringLiteral("with")).toArray() == QJsonArray{QStringLiteral("O1/O5")},
                "the plan is asked for with the group");
          move->click();
        }) == QStringLiteral("O2"),
        "a rigid group moves as one");
  check(commands.size() == 1 &&
            commands.first().value(QStringLiteral("with")).toArray() == QJsonArray{QStringLiteral("O1/O5")},
        "one command moves the occurrence with the rest of its group");
  info = plain;

  info.insert(QStringLiteral("problems"), QJsonArray{QStringLiteral("Part:1 is placed by the joints")});
  check(exercise([&](QDialog* dialog, QTreeWidget* tree, QPushButton* move) {
          tree->setCurrentItem(parentItem(tree, QString()));
          check(!move->isEnabled() && details(dialog).contains(QStringLiteral("placed by the joints")),
                "a problem of the occurrence itself disables every parent");
          dialog->reject();
        }).isEmpty(),
        "an occurrence that cannot move moves nothing");
  check(commands.isEmpty(), "no command for an occurrence that cannot move");
  return failures;
}
