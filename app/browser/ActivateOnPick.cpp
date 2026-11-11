// SPDX-License-Identifier: MIT
#include "ActivateOnPick.hpp"

#include <QObject>

namespace mitcad {

namespace {

const QString kRootComponent = QStringLiteral("C0");

QString activeOf(const DocumentSnapshot& snapshot) {
  return snapshot.activeComponent.isEmpty() ? kRootComponent : snapshot.activeComponent;
}

PickActivation activationOf(const DocumentSnapshot& snapshot, const QString& component, const SelectionItem& item) {
  PickActivation activation;
  activation.component = component;
  activation.name = snapshot.componentName(component);
  activation.picked = item;
  return activation;
}

} // namespace

const QVector<ActivatingCommand>& activatingCommands() {
  // One row per command whose feature changes the bodies it picks: the
  // inputs that hold those picks. A command left out keeps the model's
  // refusal for another component's body.
  static const QVector<ActivatingCommand> table = {
      {QStringLiteral("solid.fillet"), {QStringLiteral("edges"), QStringLiteral("body")}},
      {QStringLiteral("solid.chamfer"), {QStringLiteral("edges"), QStringLiteral("body")}},
      {QStringLiteral("solid.shell"), {QStringLiteral("faces")}},
      {QStringLiteral("solid.draft"), {QStringLiteral("faces")}},
      {QStringLiteral("solid.offset_face"), {QStringLiteral("faces")}},
      {QStringLiteral("solid.replace_face"), {QStringLiteral("faces")}},
      {QStringLiteral("solid.split_face"), {QStringLiteral("faces")}},
      {QStringLiteral("solid.split_body"), {QStringLiteral("bodies")}},
      {QStringLiteral("solid.delete_face"), {QStringLiteral("faces")}},
      {QStringLiteral("solid.move_face"), {QStringLiteral("faces")}},
      {QStringLiteral("solid.press_pull"), {QStringLiteral("target")}},
      {QStringLiteral("solid.thread"), {QStringLiteral("faces")}},
      {QStringLiteral("solid.hole"), {QStringLiteral("at")}},
      {QStringLiteral("solid.dogbone"), {QStringLiteral("picks")}},
      {QStringLiteral("solid.combine"), {QStringLiteral("target")}},
  };
  return table;
}

QStringList activatingInputs(const QString& command) {
  for (const ActivatingCommand& row : activatingCommands()) {
    if (row.command == command) {
      return row.inputs;
    }
  }
  return {};
}

QString bodyComponent(const DocumentSnapshot& snapshot, const SelectionItem& item) {
  switch (item.kind) {
  case SelectKind::Face:
  case SelectKind::Edge:
  case SelectKind::Vertex:
  case SelectKind::Body: {
    const auto body = snapshot.bodies.constFind(item.owner);
    if (body != snapshot.bodies.cend() && !body->component.isEmpty()) {
      return body->component;
    }
    // Else by the occurrence it was picked at (as ActivateQuestion does):
    // none for a preview's body at no occurrence of the document.
    return snapshot.occurrenceComponent(item.occurrence);
  }
  default:
    return QString();
  }
}

PickActivation activationForPick(const DocumentSnapshot& snapshot, const SelectionItem& item,
                                 const Selection& picked) {
  const QString component = bodyComponent(snapshot, item);
  if (component.isEmpty() || component == activeOf(snapshot)) {
    return {};
  }
  for (const SelectionItem& before : picked) {
    const QString of = bodyComponent(snapshot, before);
    if (!of.isEmpty() && of != component) {
      return {}; // the active component's, or a third one's: the refusal
    }
  }
  return activationOf(snapshot, component, item);
}

PickActivation activationForPicks(const DocumentSnapshot& snapshot, const Selection& picks) {
  const QString active = activeOf(snapshot);
  PickActivation offer;
  for (const SelectionItem& item : picks) {
    const QString of = bodyComponent(snapshot, item);
    if (!of.isEmpty() && of != active) {
      offer = activationOf(snapshot, of, item); // the last one wins
    }
  }
  if (offer.isEmpty()) {
    return {};
  }
  for (const SelectionItem& item : picks) {
    const QString of = bodyComponent(snapshot, item);
    if (!of.isEmpty() && of != offer.component) {
      ++offer.dropped;
    }
  }
  return offer;
}

QString activatedNotice(const QString& command, const QString& component, int dropped) {
  QString notice = QObject::tr("%1 goes into %2 (now active).").arg(command, component);
  if (dropped == 1) {
    notice += QLatin1Char('\n') + QObject::tr("1 pick of another component dropped.");
  } else if (dropped > 1) {
    notice += QLatin1Char('\n') + QObject::tr("%1 picks of other components dropped.").arg(dropped);
  }
  return notice;
}

} // namespace mitcad
