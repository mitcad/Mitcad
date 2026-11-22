// SPDX-License-Identifier: MIT
#include "SelectionSets.hpp"

#include <QJsonValue>
#include <QRegularExpression>

namespace mitcad {
namespace {

// A feature's uid (F5): construction geometry of the timeline, not the
// origin's (xy, x, origin) or a fixed plane's.
bool isFeatureUid(const QString& uid) {
  static const QRegularExpression pattern(QStringLiteral("^F[0-9]+$"));
  return pattern.match(uid).hasMatch();
}

QJsonObject placed(QJsonObject member, const QString& occurrence) {
  if (!occurrence.isEmpty()) {
    member.insert(QStringLiteral("occurrence"), occurrence);
  }
  return member;
}

void appendOnce(QJsonArray& members, const QJsonObject& member) {
  if (!member.isEmpty() && !members.contains(member)) {
    members.append(member);
  }
}

} // namespace

QJsonObject setMemberOf(const SelectionItem& item) {
  switch (item.kind) {
  case SelectKind::Body:
    return item.owner.isEmpty() ? QJsonObject()
                                : placed({{QStringLiteral("body"), item.owner}}, item.occurrence);
  case SelectKind::Component:
    return item.occurrence.isEmpty() ? QJsonObject() : QJsonObject{{QStringLiteral("occurrence"), item.occurrence}};
  case SelectKind::Sketch:
  case SelectKind::Plane:
  case SelectKind::Axis:
  case SelectKind::Point:
  case SelectKind::Feature:
    return isFeatureUid(item.owner) ? placed({{QStringLiteral("feature"), item.owner}}, item.occurrence)
                                    : QJsonObject();
  default:
    return QJsonObject();
  }
}

QJsonObject setMemberOf(const BrowserNode& node) {
  switch (node.type) {
  case BrowserNode::Type::Body:
    return placed({{QStringLiteral("body"), node.uid}}, node.occurrence);
  case BrowserNode::Type::Component:
    return node.occurrence.isEmpty() ? QJsonObject() : QJsonObject{{QStringLiteral("occurrence"), node.occurrence}};
  case BrowserNode::Type::Sketch:
  case BrowserNode::Type::Datum:
  case BrowserNode::Type::Joint:
  case BrowserNode::Type::Feature:
    return isFeatureUid(node.uid) ? placed({{QStringLiteral("feature"), node.uid}}, node.occurrence)
                                  : QJsonObject();
  default:
    return QJsonObject();
  }
}

QJsonArray setMembersOf(const Selection& items) {
  QJsonArray members;
  for (const SelectionItem& item : items) {
    const QJsonObject member = setMemberOf(item);
    if (member.isEmpty()) {
      return QJsonArray();
    }
    appendOnce(members, member);
  }
  return members;
}

QJsonArray setMembersOf(const QVector<BrowserNode>& nodes) {
  // A feature's row selected selects the bodies it made too (their rows
  // show the feature in the view): the set takes the feature alone.
  QStringList features;
  for (const BrowserNode& node : nodes) {
    if (node.type == BrowserNode::Type::Feature) {
      features << node.uid;
    }
  }
  QJsonArray members;
  for (const BrowserNode& node : nodes) {
    if (node.type == BrowserNode::Type::Body && features.contains(node.uid.section(QLatin1Char('.'), 0, 0))) {
      continue;
    }
    appendOnce(members, setMemberOf(node));
  }
  return members;
}

QString setMemberText(const QJsonObject& member) {
  QString text = member.contains(QStringLiteral("body"))
                     ? QStringLiteral("body ") + member.value(QStringLiteral("body")).toString()
                 : member.contains(QStringLiteral("feature"))
                     ? QStringLiteral("feature ") + member.value(QStringLiteral("feature")).toString()
                     : QStringLiteral("occurrence ") + member.value(QStringLiteral("occurrence")).toString();
  if ((member.contains(QStringLiteral("body")) || member.contains(QStringLiteral("feature"))) &&
      member.contains(QStringLiteral("occurrence"))) {
    text += QStringLiteral(" in ") + member.value(QStringLiteral("occurrence")).toString();
  }
  return text;
}

} // namespace mitcad
