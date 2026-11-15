// SPDX-License-Identifier: MIT
#include "BuildMembers.hpp"

#include <QJsonObject>
#include <QJsonValue>

namespace mitcad {

QHash<QString, QString> buildPartAdders(const QJsonArray& parts) {
  QHash<QString, QString> adders;
  for (const QJsonValue& value : parts) {
    const QJsonObject part = value.toObject();
    const QString component = part.value(QStringLiteral("component")).toString();
    if (!component.isEmpty()) {
      adders.insert(component, part.value(QStringLiteral("added_by")).toString());
    }
  }
  return adders;
}

namespace {

// Whether the part gets a build_remove of its own: not when its adder is
// among the components and gets one, which takes it. An adder left to its
// own adder takes nothing (build_remove takes only what the part it is
// given added), so the part then gets its own.
bool removedOnItsOwn(const QString& component, const QStringList& components,
                     const QHash<QString, QString>& adders) {
  QStringList chain{component}; // the part and its adders, up to one with a command
  for (;;) {
    const QString adder = adders.value(chain.last());
    if (chain.contains(adder)) {
      return true; // adders that name each other: not the model's, nothing lost
    }
    if (adder.isEmpty() || !components.contains(adder)) {
      break; // the last of the chain gets its own command
    }
    chain.append(adder);
  }
  // The chain alternates from its top: the top removes the one below, that
  // one is removed and so takes nothing, the next gets its own command, ...
  return (chain.size() - 1) % 2 == 0;
}

} // namespace

QJsonArray buildRemoveCommands(const QStringList& components, const QHash<QString, QString>& adders) {
  QJsonArray commands;
  QStringList taken;
  for (const QString& component : components) {
    if (component.isEmpty() || taken.contains(component)) {
      continue;
    }
    taken.append(component);
    if (!removedOnItsOwn(component, components, adders)) {
      continue; // its adder's build_remove takes it
    }
    commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("build_remove")},
                                {QStringLiteral("component"), component}});
  }
  return commands;
}

} // namespace mitcad
