// SPDX-License-Identifier: MIT
#include "ProfilePicks.hpp"

#include <algorithm>

#include <QJsonObject>
#include <QJsonValue>
#include <QObject>

namespace mitcad::cmd {
namespace {

bool sameFace(const SelectionItem& a, const SelectionItem& b) {
  // The view's pick names the occurrence that an edited definition's item
  // may leave empty (the root's, or the profiles' own).
  return a.kind == SelectKind::Face && b.kind == SelectKind::Face && a.owner == b.owner && a.name == b.name &&
         (a.occurrence.isEmpty() || b.occurrence.isEmpty() || a.occurrence == b.occurrence);
}

bool isFace(const SelectionItem& item) { return item.kind == SelectKind::Face; }

} // namespace

bool profilesTake(const SelectionItem& item, const Selection& held) {
  if (isFace(item)) {
    return std::all_of(held.begin(), held.end(), [&item](const SelectionItem& kept) { return sameFace(kept, item); });
  }
  return std::none_of(held.begin(), held.end(), isFace);
}

QString profilesRefused(const SelectionItem& item, const Selection& held, bool planar, ProfilesCommand command,
                        const QString& start) {
  if (profilesTake(item, held) || (isFace(item) && !planar)) {
    return QString();
  }
  const bool extrude = command == ProfilesCommand::Extrude;
  if (!isFace(item)) {
    return extrude ? QObject::tr("Profiles holds a face, which is extruded alone: click the face to remove it "
                                 "first, or clear Profiles.")
                   : QObject::tr("Profiles holds a face, which is revolved alone: click the face to remove it "
                                 "first, or clear Profiles.");
  }
  if (std::all_of(held.begin(), held.end(), isFace)) {
    return QObject::tr("Profiles takes one planar face: click the selected face to remove it first.");
  }
  if (!extrude) {
    return QObject::tr("Profiles holds sketch profiles, which a face does not join: to revolve the face, clear "
                       "Profiles first.");
  }
  return start == QStringLiteral("object")
             ? QObject::tr("Profiles holds sketch profiles, which a face does not join. To start at this face, "
                           "click Start Object, then the face; to extrude the face, clear Profiles first.")
             : QObject::tr("Profiles holds sketch profiles, which a face does not join. To start at this face, "
                           "set Start to Object; to extrude the face, clear Profiles first.");
}

std::optional<SelectionItem> automaticProfile(const QJsonArray& profiles) {
  const auto unused = [](const QJsonValue& value) {
    return !value.toObject().value(QStringLiteral("consumed")).toBool();
  };
  QString sketch;
  for (const QJsonValue& value : profiles) {
    if (unused(value)) {
      sketch = value.toObject().value(QStringLiteral("sketch")).toString();
    }
  }
  if (sketch.isEmpty()) {
    return std::nullopt;
  }
  std::optional<SelectionItem> drawn;
  std::optional<SelectionItem> leftover;
  for (const QJsonValue& value : profiles) {
    const QJsonObject profile = value.toObject();
    if (!unused(value) || profile.value(QStringLiteral("sketch")).toString() != sketch) {
      continue;
    }
    const SelectionItem item{SelectKind::Profile, sketch, profile.value(QStringLiteral("region")).toString(),
                             QStringLiteral("plane")};
    if (profile.value(QStringLiteral("face_bounded")).toBool()) {
      leftover = item;
    } else {
      drawn = item;
    }
  }
  return drawn ? drawn : leftover;
}

} // namespace mitcad::cmd
