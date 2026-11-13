// SPDX-License-Identifier: MIT
// Exercise the actual modal destination dialog with a model boundary that
// records every mutation. Geometry and transactional behavior have core tests.
#include "framework/HybridAssembly.hpp"

#include <cstdio>
#include <functional>
#include <stdexcept>

#include <QApplication>
#include <QCheckBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QLineEdit>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QRadioButton>
#include <QTimer>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>

namespace {

QJsonObject row(const QString& component, const QString& name, const QString& occurrence,
                const QString& parent, const QString& path, bool eligible = true) {
  return {{QStringLiteral("component"), component}, {QStringLiteral("name"), name},
          {QStringLiteral("occurrence"), occurrence}, {QStringLiteral("parent_occurrence"), parent},
          {QStringLiteral("path"), path}, {QStringLiteral("eligible"), eligible},
          {QStringLiteral("reason"), eligible ? QString() : QStringLiteral("Already owned by this component.")},
          {QStringLiteral("can_create"), true}, {QStringLiteral("create_reason"), QString()}};
}

QTreeWidgetItem* target(QTreeWidget* tree, const QString& occurrence) {
  for (QTreeWidgetItemIterator it(tree); *it != nullptr; ++it) {
    if ((*it)->data(0, Qt::UserRole).toJsonObject().value(QStringLiteral("occurrence")).toString() == occurrence) {
      return *it;
    }
  }
  return nullptr;
}

} // namespace

int moveToComponentTests() {
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) {
      std::fprintf(stderr, "move to component: %s\n", message);
      ++failures;
    }
  };
  const QJsonObject group{
      {QStringLiteral("features"), QJsonArray{
           QJsonObject{{QStringLiteral("uid"), QStringLiteral("F1")}, {QStringLiteral("name"), QStringLiteral("Sketch1")}},
           QJsonObject{{QStringLiteral("uid"), QStringLiteral("F2")}, {QStringLiteral("name"), QStringLiteral("Extrude1")}}}},
      {QStringLiteral("bodies"), QJsonArray{
           QJsonObject{{QStringLiteral("uid"), QStringLiteral("F2.b0")}, {QStringLiteral("name"), QStringLiteral("Body1")}}}}};
  QJsonObject plan{{QStringLiteral("groups"), QJsonArray{group}},
                   {QStringLiteral("together"), QJsonArray{group}},
                   // Deliberately child-first and with duplicate display names.
                   {QStringLiteral("destinations"), QJsonArray{
                        row(QStringLiteral("C2"), QStringLiteral("Part"), QStringLiteral("O1/O2"),
                            QStringLiteral("O1"), QStringLiteral("Root/Assembly/Part")),
                        row(QStringLiteral("C0"), QStringLiteral("Root"), QString(), QString(), QStringLiteral("Root"), false),
                        row(QStringLiteral("C1"), QStringLiteral("Assembly"), QStringLiteral("O1"),
                            QString(), QStringLiteral("Root/Assembly")),
                        row(QStringLiteral("C3"), QStringLiteral("Part"), QStringLiteral("O3"),
                            QString(), QStringLiteral("Root/Part"))}}};
  // A body transfer's plan (mitcad#465): the root is the bodies' own.
  const QJsonObject transferPlan{
      {QStringLiteral("component"), QStringLiteral("C0")}, {QStringLiteral("name"), QStringLiteral("Root")},
      {QStringLiteral("bodies"), QJsonArray{QJsonObject{{QStringLiteral("uid"), QStringLiteral("F2.b0")},
                                                        {QStringLiteral("name"), QStringLiteral("Body1")}}}},
      {QStringLiteral("destinations"), QJsonArray{
           row(QStringLiteral("C0"), QStringLiteral("Root"), QString(), QString(), QStringLiteral("Root"), false),
           row(QStringLiteral("C1"), QStringLiteral("Assembly"), QStringLiteral("O1"), QString(),
               QStringLiteral("Root/Assembly")),
           row(QStringLiteral("C2"), QStringLiteral("Part"), QStringLiteral("O1/O2"), QStringLiteral("O1"),
               QStringLiteral("Root/Assembly/Part")),
           row(QStringLiteral("C3"), QStringLiteral("Part"), QStringLiteral("O3"), QString(),
               QStringLiteral("Root/Part"))}}};
  QJsonObject selection{{QStringLiteral("component"), QStringLiteral("C0")},
                              {QStringLiteral("source_occurrence"), QString()},
                              {QStringLiteral("features"), QJsonArray{QStringLiteral("F2")}}};
  QList<QJsonObject> commands;
  QList<QJsonObject> queries;
  bool runSucceeds = true;
  bool queryThrows = false;
  const mitcad::HybridModel model{
      [&](const QJsonObject& query) {
        queries.append(query);
        if (queryThrows) { throw std::runtime_error("The source occurrence is ambiguous."); }
        if (query.value(QStringLiteral("query")) == QStringLiteral("transfer_bodies")) { return transferPlan; }
        return plan;
      },
      [&](const QJsonObject& command, QJsonObject* result) {
        commands.append(command);
        if (result != nullptr) { *result = {}; }
        return runSucceeds;
      }};
  const auto exercise = [&](const std::function<void(QDialog*, QTreeWidget*, QPushButton*)>& action) {
    commands.clear();
    queries.clear();
    QTimer watchdog;
    watchdog.setSingleShot(true);
    QObject::connect(&watchdog, &QTimer::timeout, [&] {
      check(false, "dialog interaction must finish");
      if (auto* dialog = qobject_cast<QDialog*>(QApplication::activeModalWidget())) { dialog->reject(); }
    });
    watchdog.start(3000);
    QTimer::singleShot(0, [&] {
      auto* dialog = qobject_cast<QDialog*>(QApplication::activeModalWidget());
      check(dialog != nullptr && dialog->objectName() == QStringLiteral("moveToComponentDialog"),
            "always opens the destination dialog");
      if (dialog == nullptr) { return; }
      auto* tree = dialog->findChild<QTreeWidget*>(QStringLiteral("moveDestinations"));
      auto* buttons = dialog->findChild<QDialogButtonBox*>();
      check(tree != nullptr && buttons != nullptr, "destination controls exist");
      if (tree == nullptr || buttons == nullptr) { dialog->reject(); return; }
      check(commands.isEmpty(), "opening and planning do not mutate the model");
      action(dialog, tree, buttons->button(QDialogButtonBox::Ok));
    });
    const bool moved = mitcad::moveToComponent(nullptr, model, selection);
    watchdog.stop();
    return moved;
  };

  check(!exercise([&](QDialog* dialog, QTreeWidget*, QPushButton* move) {
    check(!move->isEnabled(), "must explicitly select a destination");
    dialog->reject();
  }), "cancel reports no move");
  check(commands.isEmpty(), "cancel makes no command, including no component creation");

  check(exercise([&](QDialog* dialog, QTreeWidget* tree, QPushButton* move) {
    QTreeWidgetItem* nested = target(tree, QStringLiteral("O1/O2"));
    check(tree->topLevelItemCount() == 1, "root contains all destinations");
    check(nested != nullptr && nested->parent() == target(tree, QStringLiteral("O1")), "nested destination has its parent");
    check(nested != nullptr && nested->text(1) == QStringLiteral("Root/Assembly/Part"),
          "duplicate names have their full path, without the occurrence's ids (mitcad#1741)");
    tree->setCurrentItem(nested);
    const QString details = dialog->findChild<QPlainTextEdit*>(QStringLiteral("moveDetails"))->toPlainText();
    check(details.contains(QStringLiteral("Sketch1")) && details.contains(QStringLiteral("Extrude1")) &&
              details.contains(QStringLiteral("Body1")), "dependency expansion is visible before moving");
    check(move->isEnabled(), "eligible existing destination enables Move");
    move->click();
  }), "existing destination succeeds");
  check(commands.size() == 1, "existing move is one command");
  if (!commands.isEmpty()) {
    const QJsonObject command = commands.first();
    const QJsonObject destination = command.value(QStringLiteral("destination")).toObject();
    check(command.value(QStringLiteral("cmd")) == QStringLiteral("move_to_component") &&
              command.contains(QStringLiteral("source_occurrence")) &&
              command.value(QStringLiteral("features")) == selection.value(QStringLiteral("features")),
          "source selection and root occurrence are preserved");
    check(destination.value(QStringLiteral("kind")) == QStringLiteral("existing") &&
              destination.value(QStringLiteral("component")) == QStringLiteral("C2") &&
              destination.value(QStringLiteral("occurrence")) == QStringLiteral("O1/O2"),
          "existing move explicitly identifies the chosen occurrence");
  }

  check(exercise([&](QDialog* dialog, QTreeWidget* tree, QPushButton* move) {
    dialog->findChild<QRadioButton*>(QStringLiteral("moveCreateNew"))->click();
    tree->setCurrentItem(target(tree, QStringLiteral("O1")));
    dialog->findChild<QLineEdit*>(QStringLiteral("moveNewName"))->setText(QStringLiteral("Moved part"));
    check(move->isEnabled(), "Create New uses parent eligibility");
    check(commands.isEmpty(), "choosing Create New does not yet create anything");
    move->click();
  }), "Create New succeeds");
  check(commands.size() == 1, "create and move are one atomic model command");
  if (!commands.isEmpty()) {
    const QJsonObject destination = commands.first().value(QStringLiteral("destination")).toObject();
    check(destination.value(QStringLiteral("kind")) == QStringLiteral("new") &&
              destination.value(QStringLiteral("name")) == QStringLiteral("Moved part") &&
              destination.value(QStringLiteral("parent")) == QStringLiteral("C1") &&
              destination.value(QStringLiteral("parent_occurrence")) == QStringLiteral("O1"),
          "Create New carries the visible name and parent occurrence");
  }

  check(!exercise([&](QDialog* dialog, QTreeWidget* tree, QPushButton* move) {
    dialog->findChild<QRadioButton*>(QStringLiteral("moveCreateNew"))->click();
    tree->setCurrentItem(target(tree, QString()));
    dialog->findChild<QLineEdit*>(QStringLiteral("moveNewName"))->setText(QStringLiteral("Cancelled"));
    check(move->isEnabled(), "current owner may be a new component's parent");
    dialog->reject();
  }), "Create New can be cancelled");
  check(commands.isEmpty(), "Create New cancellation leaves model untouched");

  check(!exercise([&](QDialog* dialog, QTreeWidget* tree, QPushButton* move) {
    tree->setCurrentItem(target(tree, QString()));
    check(!move->isEnabled(), "current owner is an invalid existing destination");
    check(dialog->findChild<QPlainTextEdit*>(QStringLiteral("moveDetails"))->toPlainText().contains(
              QStringLiteral("Already owned")), "invalid destination gives its reason");
    move->click();
    check(commands.isEmpty(), "invalid existing destination never falls back to Create New");
    dialog->reject();
  }), "invalid destination cannot move");

  plan.insert(QStringLiteral("problems"), QJsonArray{QStringLiteral("Target is read-only.")});
  check(!exercise([&](QDialog* dialog, QTreeWidget* tree, QPushButton* move) {
    tree->setCurrentItem(target(tree, QStringLiteral("O3")));
    check(!move->isEnabled() && dialog->findChild<QPlainTextEdit*>(QStringLiteral("moveDetails"))->toPlainText().contains(
                                  QStringLiteral("Target is read-only.")), "target validation problems disable Move");
    const QJsonObject destination = queries.last().value(QStringLiteral("destination")).toObject();
    check(destination.value(QStringLiteral("component")) == QStringLiteral("C3") &&
              destination.value(QStringLiteral("occurrence")) == QStringLiteral("O3"),
          "validation query receives the explicit destination");
    dialog->reject();
  }), "model validation can reject an apparently eligible destination");
  check(commands.isEmpty(), "model query problems cannot trigger a fallback command");
  plan.remove(QStringLiteral("problems"));

  plan.insert(QStringLiteral("conflicts"), QJsonArray{
      QJsonObject{{QStringLiteral("message"), QStringLiteral("Body1 also depends on Body2.")}}});
  QJsonObject expanded = group;
  QJsonArray bodies = expanded.value(QStringLiteral("bodies")).toArray();
  bodies.append(QJsonObject{{QStringLiteral("uid"), QStringLiteral("F3.b0")}, {QStringLiteral("name"), QStringLiteral("Body2")}});
  expanded.insert(QStringLiteral("bodies"), bodies);
  plan.insert(QStringLiteral("together"), QJsonArray{expanded});
  check(exercise([&](QDialog* dialog, QTreeWidget* tree, QPushButton* move) {
    tree->setCurrentItem(target(tree, QStringLiteral("O3")));
    auto* together = dialog->findChild<QCheckBox*>(QStringLiteral("moveTogether"));
    check(together->isVisible() && !move->isEnabled(), "conflicts require explicit Keep Them Together");
    together->click();
    check(move->isEnabled() && dialog->findChild<QPlainTextEdit*>(QStringLiteral("moveDetails"))->toPlainText().contains(
                                  QStringLiteral("Body2")) &&
                              !dialog->findChild<QPlainTextEdit*>(QStringLiteral("moveDetails"))->toPlainText().contains(
                                  QStringLiteral("F3.b0")),
          "expanded connected bodies are shown by name");
    check(queries.last().value(QStringLiteral("together")).toBool(), "query and command share together consent");
    move->click();
  }), "confirmed dependency group moves");
  check(commands.size() == 1 && commands.first().value(QStringLiteral("together")).toBool(), "command keeps connected bodies together");
  plan.remove(QStringLiteral("conflicts"));

  runSucceeds = false;
  check(!exercise([&](QDialog* dialog, QTreeWidget* tree, QPushButton* move) {
    tree->setCurrentItem(target(tree, QStringLiteral("O3")));
    move->click();
    check(dialog->isVisible() && commands.size() == 1, "failed model command keeps dialog open");
    check(dialog->findChild<QRadioButton*>(QStringLiteral("moveExisting"))->isChecked(),
          "failed move preserves existing destination mode");
    dialog->reject();
  }), "failed move can be cancelled");
  check(commands.size() == 1, "failed existing move does not try creating a component");

  queryThrows = true;
  check(!exercise([&](QDialog* dialog, QTreeWidget*, QPushButton* move) {
    check(!move->isEnabled() && dialog->findChild<QPlainTextEdit*>(QStringLiteral("moveDetails"))->toPlainText().contains(
                                  QStringLiteral("source occurrence is ambiguous")), "query failure stays visible in destination dialog");
    dialog->reject();
  }), "ambiguous source cannot move");
  check(commands.isEmpty(), "query failure cannot mutate the model");

  // A body's row: the bodies alone by a body transfer (mitcad#465).
  queryThrows = false;
  runSucceeds = true;
  check(!exercise([&](QDialog* dialog, QTreeWidget*, QPushButton*) {
    check(dialog->findChild<QCheckBox*>(QStringLiteral("moveBodiesOnly"))->isHidden(),
          "a feature selection offers no body transfer");
    dialog->reject();
  }), "feature selection cancelled");
  selection = {{QStringLiteral("component"), QStringLiteral("C0")},
               {QStringLiteral("source_occurrence"), QString()},
               {QStringLiteral("bodies"), QJsonArray{QStringLiteral("F2.b0")}}};
  check(exercise([&](QDialog* dialog, QTreeWidget* tree, QPushButton* move) {
    auto* bodiesOnly = dialog->findChild<QCheckBox*>(QStringLiteral("moveBodiesOnly"));
    check(!bodiesOnly->isHidden() && !bodiesOnly->isChecked(), "a body's row offers the body transfer, off");
    dialog->findChild<QRadioButton*>(QStringLiteral("moveCreateNew"))->click();
    bodiesOnly->click();
    check(dialog->findChild<QRadioButton*>(QStringLiteral("moveExisting"))->isChecked() &&
              !dialog->findChild<QRadioButton*>(QStringLiteral("moveCreateNew"))->isEnabled(),
          "a body transfer goes into an existing component");
    tree->setCurrentItem(target(tree, QString()));
    check(!move->isEnabled(), "the bodies' own component is no destination");
    tree->setCurrentItem(target(tree, QStringLiteral("O1/O2")));
    const QString details = dialog->findChild<QPlainTextEdit*>(QStringLiteral("moveDetails"))->toPlainText();
    check(details.contains(QStringLiteral("Body Transfer")) && details.contains(QStringLiteral("Body1")) &&
              !details.contains(QStringLiteral("F2.b0")),
          "the plan names the transfer and the bodies");
    check(queries.last().value(QStringLiteral("query")) == QStringLiteral("transfer_bodies"),
          "the transfer's own query plans it");
    check(move->isEnabled(), "an eligible destination enables Move");
    move->click();
  }), "body transfer succeeds");
  check(commands.size() == 1, "a body transfer is one command");
  if (!commands.isEmpty()) {
    const QJsonObject command = commands.first();
    const QJsonObject destination = command.value(QStringLiteral("destination")).toObject();
    check(command.value(QStringLiteral("cmd")) == QStringLiteral("transfer_bodies") &&
              command.value(QStringLiteral("bodies")) == selection.value(QStringLiteral("bodies")) &&
              command.contains(QStringLiteral("source_occurrence")) &&
              destination.value(QStringLiteral("component")) == QStringLiteral("C2") &&
              destination.value(QStringLiteral("occurrence")) == QStringLiteral("O1/O2"),
          "the transfer names the bodies, their occurrence and the chosen destination");
  }
  return failures;
}
