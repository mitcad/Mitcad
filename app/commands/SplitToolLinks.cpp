// SPDX-License-Identifier: MIT
#include "SplitToolLinks.hpp"

namespace mitcad::cmd {

namespace {

bool onBody(const SelectionItem& item) { return item.kind == SelectKind::Face || item.kind == SelectKind::Body; }

} // namespace

QJsonObject splitToolLinks(const Selection& bodies, const Selection& tools, const OccurrenceComponent& componentOf) {
  const QString here = bodies.isEmpty() ? QString() : bodies.first().occurrence;
  QJsonObject links;
  QString own;
  bool ownKnown = false;
  for (const SelectionItem& tool : tools) {
    if (!onBody(tool) || tool.occurrence == here) {
      continue;
    }
    if (!ownKnown) {
      own = componentOf(here);
      ownKnown = true;
    }
    // Another occurrence of the split's own component: the tool is read in
    // that component, where it is the same in every occurrence.
    if (!own.isEmpty() && componentOf(tool.occurrence) == own) {
      continue;
    }
    // Empty paths (the root) are left out, as the model saves them.
    QJsonObject link;
    if (!tool.occurrence.isEmpty()) {
      link.insert(QStringLiteral("source"), tool.occurrence);
    }
    if (!here.isEmpty()) {
      link.insert(QStringLiteral("target"), here);
    }
    links.insert(tool.owner, link);
  }
  return links;
}

void linkSplitPicks(const QJsonObject& links, Selection& bodies, Selection& tools) {
  if (links.isEmpty()) {
    return;
  }
  for (SelectionItem& tool : tools) {
    if (onBody(tool) && links.contains(tool.owner)) {
      tool.occurrence = links.value(tool.owner).toObject().value(QStringLiteral("source")).toString();
    }
  }
  // Every link sees the split's component at the same place.
  const QString target = links.constBegin().value().toObject().value(QStringLiteral("target")).toString();
  for (SelectionItem& body : bodies) {
    body.occurrence = target;
  }
}

} // namespace mitcad::cmd
