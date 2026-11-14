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
  // One row per command whose feature changes the bodies it picks, or
  // places a part by them (mitcad#1752): the inputs that hold those picks.
  // A command left out keeps the model's refusal for another component's
  // body.
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
      // Parts placed by a pick, by Place On (mitcad#1752): a shaft of
      // another component goes in linked instead (linksShafts).
      {QStringLiteral("solid.gear"), {QStringLiteral("at")}},
      {QStringLiteral("solid.pulley"), {QStringLiteral("at")}},
      {QStringLiteral("solid.bearing"), {QStringLiteral("at")}},
      {QStringLiteral("solid.coil"), {QStringLiteral("at")}},
      {QStringLiteral("solid.box"), {QStringLiteral("at")}},
      {QStringLiteral("solid.cylinder"), {QStringLiteral("at")}},
      {QStringLiteral("solid.sphere"), {QStringLiteral("at")}},
      {QStringLiteral("solid.torus"), {QStringLiteral("at")}},
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

bool linksShafts(const QString& command) {
  // The parts of the table's last rows: each sits on a shaft as a part of
  // its own (a gear, a pulley, a bearing, a spring, a spacer).
  static const QStringList parts = {
      QStringLiteral("solid.gear"),     QStringLiteral("solid.pulley"),   QStringLiteral("solid.bearing"),
      QStringLiteral("solid.coil"),     QStringLiteral("solid.box"),      QStringLiteral("solid.cylinder"),
      QStringLiteral("solid.sphere"),   QStringLiteral("solid.torus"),
  };
  return parts.contains(command);
}

bool isShaftPick(const SelectionItem& item) {
  if (item.kind == SelectKind::Face) {
    return item.geometry == QStringLiteral("cylinder");
  }
  if (item.kind == SelectKind::Edge) {
    return item.geometry == QStringLiteral("circle") || item.geometry == QStringLiteral("arc");
  }
  return false;
}

bool pickActivates(const QString& command, const SelectionItem& item) {
  return !(isShaftPick(item) && linksShafts(command));
}

Selection activatingPicks(const QString& command, const Selection& picks) {
  Selection kept;
  for (const SelectionItem& item : picks) {
    if (pickActivates(command, item)) {
      kept.append(item);
    }
  }
  return kept;
}

QString linkedShaftNotice(const DocumentSnapshot& snapshot, const QString& command, const QString& name,
                          const SelectionItem& item) {
  if (pickActivates(command, item)) {
    return QString();
  }
  const QString shaft = bodyComponent(snapshot, item);
  const QString active = activeOf(snapshot);
  if (shaft.isEmpty() || shaft == active) {
    return QString();
  }
  return QObject::tr("%1 goes into %2 (active), linked to the shaft of %3: a part on a shaft stays out of the "
                     "shaft's component.")
      .arg(name, snapshot.componentName(active), snapshot.componentName(shaft));
}

QString keptNotice(const DocumentSnapshot& snapshot, const QString& name, const QString& shown) {
  return shown == activatedNotice(name, snapshot.componentName(activeOf(snapshot)), 0) ? shown : QString();
}

} // namespace mitcad
