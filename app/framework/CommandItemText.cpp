// SPDX-License-Identifier: MIT
// CommandContext::itemText (mitcad#1741): a picked item named in words for
// the texts a person sees, apart from Command.cpp so that targets without
// the selection's code (test_drag_proxy) link Command.cpp alone.
#include "framework/Command.hpp"
#include "framework/Selection.hpp"

#include <exception>

#include <QCoreApplication>
#include <QJsonArray>
#include <QJsonObject>
#include <QStringList>

namespace mitcad {

QString CommandContext::itemText(const SelectionItem& item) const {
  QString kind = SelectionItem::kindText(item.kind);
  if (!kind.isEmpty()) {
    kind[0] = kind[0].toUpper();
  }
  // "<kind> of <owner>" by the owner's name; the kind alone when the owner
  // has none (never its uid).
  const auto of = [&kind](const QString& name, const QString& uid) {
    return name.isEmpty() || name == uid ? kind
                                         : QCoreApplication::translate("Selection", "%1 of %2").arg(kind, name);
  };
  const auto named = [&kind](const QString& name, const QString& uid) {
    return name.isEmpty() || name == uid ? kind : name;
  };
  static const QStringList origin = {QStringLiteral("xy"), QStringLiteral("xz"), QStringLiteral("yz"),
                                     QStringLiteral("x"),  QStringLiteral("y"),  QStringLiteral("z")};
  try {
    // A BUILD joint's origin on a part's mount frame (mitcad#1885): by its
    // end, and the part's name; never the frame's reference.
    if (isMountFrame(item)) {
      const QString part = mountFramePart(item);
      const QString partName = part.isEmpty() ? QString() : featureName(part);
      const QString frame = mountFrameName(item);
      const bool hasName = !partName.isEmpty() && partName != part;
      if (frame == QStringLiteral("end_a")) {
        return hasName ? QCoreApplication::translate("Selection", "End A frame of %1").arg(partName)
                       : QCoreApplication::translate("Selection", "End A frame");
      }
      if (frame == QStringLiteral("end_b")) {
        return hasName ? QCoreApplication::translate("Selection", "End B frame of %1").arg(partName)
                       : QCoreApplication::translate("Selection", "End B frame");
      }
      return hasName ? QCoreApplication::translate("Selection", "Mount frame of %1").arg(partName)
                     : QCoreApplication::translate("Selection", "Mount frame");
    }
    switch (item.kind) {
    case SelectKind::Face:
    case SelectKind::Edge:
    case SelectKind::Vertex:
      return of(bodyName(item.owner), item.owner);
    case SelectKind::Body:
      return named(bodyName(item.owner), item.owner);
    case SelectKind::Profile:
    case SelectKind::SketchCurve:
    case SelectKind::SketchPoint:
    case SelectKind::SketchConstraint:
    case SelectKind::SketchDimension:
      return of(featureName(item.owner), item.owner);
    case SelectKind::Plane:
    case SelectKind::Axis:
    case SelectKind::Point:
      if (item.owner == QStringLiteral("origin")) {
        return QCoreApplication::translate("Selection", "Origin");
      }
      if (origin.contains(item.owner)) {
        return QCoreApplication::translate("Selection", "Origin %1").arg(item.owner.toUpper());
      }
      return named(featureName(item.owner), item.owner);
    case SelectKind::Sketch:
    case SelectKind::Feature:
      return named(featureName(item.owner), item.owner);
    default:
      return kind;
    }
  } catch (const std::exception&) {
    return kind; // the model could not be asked
  }
}

} // namespace mitcad
