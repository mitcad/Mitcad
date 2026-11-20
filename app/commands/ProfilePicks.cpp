// SPDX-License-Identifier: MIT
#include "ProfilePicks.hpp"

#include <algorithm>
#include <exception>

#include <QJsonObject>
#include <QJsonValue>
#include <QObject>

#include "../framework/Command.hpp"

namespace mitcad::cmd {
namespace {

bool sameFace(const SelectionItem& a, const SelectionItem& b) {
  // The view's pick names the occurrence that an edited definition's item
  // may leave empty (the root's, or the profiles' own).
  return a.kind == SelectKind::Face && b.kind == SelectKind::Face && a.owner == b.owner && a.name == b.name &&
         (a.occurrence.isEmpty() || b.occurrence.isEmpty() || a.occurrence == b.occurrence);
}

bool isFace(const SelectionItem& item) { return item.kind == SelectKind::Face; }

// The planar `sketch` query's answer for the sketch of an automatic
// profile; empty for a 3D sketch, which that query refuses (mitcad#2087:
// Helix after Convert to 3D Sketch threw out of the command's start), and
// for a sketch the model cannot answer for. The kind is read from the
// `feature` query first, as sketchCurveType does with isSpatialSketch.
QJsonObject planarSketchOf(const QString& uid, const CommandContext& context) {
  try {
    if (context.queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), uid}})
            .value(QStringLiteral("type"))
            .toString() == QStringLiteral("spatial_sketch")) {
      return QJsonObject();
    }
    return context.queryObject({{QStringLiteral("query"), QStringLiteral("sketch")}, {QStringLiteral("uid"), uid}});
  } catch (const std::exception&) {
    return QJsonObject(); // not a feature, or one just taken back
  }
}

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

void newestProfile(CommandState& state, const CommandContext& context) {
  Selection& profiles = state.items(QStringLiteral("profiles"));
  if (!profiles.isEmpty()) {
    return;
  }
  // A profile the sketch's own curves bound, before the leftover region
  // of the face it lies on (mitcad#1226).
  const QJsonArray all = context.queryArray(QStringLiteral("profiles"));
  const std::optional<SelectionItem> chosen = automaticProfile(all);
  if (!chosen) {
    return;
  }
  profiles.append(*chosen);
  // A region of a QR code brings the code's other dark regions: the code
  // extrudes whole (mitcad#473). Only a planar sketch holds a code: a 3D
  // sketch's region is taken alone, on purpose, since the `sketch3d`
  // query gives plain region keys with no link to a text.
  const QString sketch = chosen->owner;
  const QJsonObject model = planarSketchOf(sketch, context);
  if (model.isEmpty()) {
    return;
  }
  const QJsonArray regions = model.value(QStringLiteral("regions")).toArray();
  QString text;
  for (const QJsonValue& value : regions) {
    const QJsonObject region = value.toObject();
    if (region.value(QStringLiteral("key")).toString() == chosen->name) {
      text = region.value(QStringLiteral("text")).toString();
    }
  }
  bool code = false;
  for (const QJsonValue& value : model.value(QStringLiteral("texts")).toArray()) {
    const QJsonObject t = value.toObject();
    code = code || (!text.isEmpty() && t.value(QStringLiteral("id")).toString() == text &&
                    t.value(QStringLiteral("code")).isObject());
  }
  if (!code) {
    return;
  }
  profiles.clear();
  for (const QJsonValue& value : regions) {
    const QJsonObject region = value.toObject();
    const QString key = region.value(QStringLiteral("key")).toString();
    const bool free = std::any_of(all.begin(), all.end(), [&](const QJsonValue& entry) {
      const QJsonObject profile = entry.toObject();
      return !profile.value(QStringLiteral("consumed")).toBool() &&
             profile.value(QStringLiteral("sketch")).toString() == sketch &&
             profile.value(QStringLiteral("region")).toString() == key;
    });
    if (free && region.value(QStringLiteral("text")).toString() == text &&
        region.value(QStringLiteral("letter")).toBool()) {
      profiles.append({SelectKind::Profile, sketch, key, QStringLiteral("plane")});
    }
  }
  if (profiles.isEmpty()) {
    profiles.append(*chosen);
  }
}

} // namespace mitcad::cmd
