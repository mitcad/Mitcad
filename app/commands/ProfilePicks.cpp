// SPDX-License-Identifier: MIT
#include "ProfilePicks.hpp"

#include <algorithm>
#include <exception>

#include <QDebug>
#include <QJsonObject>
#include <QJsonValue>
#include <QObject>
#include <QStringList>

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

namespace {

// The state's text naming the sketch whose closed curves cross (crossingSketch).
const char* const kCrossing = "profiles_crossing";
// The state's text of what else Profiles held then (othersOf): the note
// stays while that is so.
const char* const kCrossingOthers = "profiles_crossing_others";

// The entries of `profiles` other than regions of `sketch`, as one text.
QString othersOf(const Selection& profiles, const QString& sketch) {
  QStringList others;
  for (const SelectionItem& item : profiles) {
    if (item.kind != SelectKind::Profile || item.owner != sketch) {
      others << SelectionItem::kindName(item.kind) + QLatin1Char('|') + item.owner + QLatin1Char('|') + item.name;
    }
  }
  others.sort();
  return others.join(QLatin1Char('\n'));
}

// Marks `sketch` as the one whose closed curves cross, with what else
// Profiles hold now; an empty `sketch` takes the mark away.
void markCrossing(CommandState& state, const QString& sketch) {
  state.setText(QString::fromLatin1(kCrossing), sketch);
  state.setText(QString::fromLatin1(kCrossingOthers),
                sketch.isEmpty() ? QString() : othersOf(state.items(QStringLiteral("profiles")), sketch));
}

// A QR code's free dark regions in `profiles` when `chosen` is a region of
// the code (mitcad#473), else nothing changed; whether it was. `model` is
// the planar `sketch` query's answer for its sketch, `all` the `profiles`
// query.
bool codeRegions(Selection& profiles, const SelectionItem& chosen, const QJsonObject& model, const QJsonArray& all);

// newestProfile's start, of the outline set first for Extrude and Revolve
// (`outlines`, mitcad#2194).
void startProfiles(CommandState& state, const CommandContext& context, bool outlines) {
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
  // A region of a QR code brings the code's other dark regions: the code
  // extrudes whole (mitcad#473). Only a planar sketch holds a code: a 3D
  // sketch's region is taken alone, on purpose, since the `sketch3d`
  // query gives plain region keys with no link to a text.
  const QString sketch = chosen->owner;
  const QJsonObject model = planarSketchOf(sketch, context);
  if (!model.isEmpty() && codeRegions(profiles, *chosen, model, all)) {
    return;
  }
  // The sketch's outline set (mitcad#2194): a plate less its holes, every
  // separate outline. When none of it is unused (the plate extruded
  // already), the region alone, so that the next Extrude takes a hole's
  // disc as a boss; when its closed curves cross, the region alone too,
  // and the panel says why no more (crossingNote).
  if (outlines) {
    const OutlinePick pick = sketchOutline(all, sketch);
    if (pick.crossing) {
      markCrossing(state, sketch);
    } else if (!pick.items.isEmpty()) {
      profiles = pick.items;
      return;
    }
  }
  profiles.append(*chosen);
}

bool codeRegions(Selection& profiles, const SelectionItem& chosen, const QJsonObject& model, const QJsonArray& all) {
  const QString sketch = chosen.owner;
  const QJsonArray regions = model.value(QStringLiteral("regions")).toArray();
  QString text;
  for (const QJsonValue& value : regions) {
    const QJsonObject region = value.toObject();
    if (region.value(QStringLiteral("key")).toString() == chosen.name) {
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
    return false;
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
    profiles.append(chosen);
  }
  return true;
}

bool sameProfile(const SelectionItem& a, const SelectionItem& b) {
  return a.kind == b.kind && a.owner == b.owner && a.name == b.name;
}

} // namespace

void newestProfile(CommandState& state, const CommandContext& context) { startProfiles(state, context, false); }

OutlinePick sketchOutline(const QJsonArray& profiles, const QString& sketch, const QString& occurrence) {
  OutlinePick pick;
  for (const QJsonValue& value : profiles) {
    const QJsonObject row = value.toObject();
    if (row.value(QStringLiteral("sketch")).toString() != sketch) {
      continue;
    }
    if (row.value(QStringLiteral("curves_cross")).toBool()) {
      pick.crossing = true;
    } else if (row.value(QStringLiteral("outline")).toBool() && !row.value(QStringLiteral("consumed")).toBool()) {
      pick.items.append({SelectKind::Profile, sketch, row.value(QStringLiteral("region")).toString(),
                         QStringLiteral("plane"), occurrence});
    }
  }
  if (pick.crossing) {
    pick.items.clear();
  }
  return pick;
}

QString crossingSketch(const CommandState& state) { return state.text(QString::fromLatin1(kCrossing)); }

void expandWholeSketches(CommandState& state, const CommandContext& context) {
  Selection& profiles = state.items(QStringLiteral("profiles"));
  Selection sketches;
  Selection kept;
  for (const SelectionItem& item : profiles) {
    (item.kind == SelectKind::Sketch ? sketches : kept).append(item);
  }
  if (sketches.isEmpty()) {
    return;
  }
  const QJsonArray all = context.queryArray(QStringLiteral("profiles"));
  // The note follows the sketch picked last.
  QString crossing;
  for (const SelectionItem& sketch : sketches) {
    const OutlinePick pick = sketchOutline(all, sketch.owner, sketch.occurrence);
    crossing = pick.crossing ? sketch.owner : QString();
    int added = 0;
    for (const SelectionItem& item : pick.items) {
      const bool held = std::any_of(kept.begin(), kept.end(),
                                    [&item](const SelectionItem& entry) { return sameProfile(entry, item); });
      if (!held) {
        kept.append(item);
        ++added;
      }
    }
    qDebug().noquote() << QStringLiteral("Profiles: the whole sketch %1 taken as %2 of its outline set%3")
                              .arg(context.featureName(sketch.owner))
                              .arg(added)
                              .arg(pick.crossing ? QStringLiteral(": none, its closed curves cross") : QString());
  }
  profiles = kept;
  markCrossing(state, crossing);
}

void newestOutline(CommandState& state, const CommandContext& context) {
  const Selection& held = state.items(QStringLiteral("profiles"));
  const bool whole = std::any_of(held.begin(), held.end(),
                                 [](const SelectionItem& item) { return item.kind == SelectKind::Sketch; });
  expandWholeSketches(state, context);
  // A whole sketch selected before is what the user meant: when it gives
  // nothing (its closed curves cross, or its areas are used), Profiles
  // wait for clicks rather than take another sketch's.
  if (whole) {
    return;
  }
  startProfiles(state, context, true);
}

QString crossingNote(const CommandState& state, const CommandContext& context) {
  const QString sketch = crossingSketch(state);
  // While Profiles hold what they held besides the sketch's own regions
  // when it was marked (the regions clicked since are the sketch's).
  if (sketch.isEmpty() ||
      othersOf(state.items(QStringLiteral("profiles")), sketch) != state.text(QString::fromLatin1(kCrossingOthers))) {
    return QString();
  }
  return QObject::tr("The closed curves of %1 cross each other, so which areas are meant is not clear: click "
                     "the areas you want in the view.")
      .arg(context.featureName(sketch));
}

bool unfilledProfile(const QJsonObject& row) {
  return !row.value(QStringLiteral("curves_cross")).toBool() && row.contains(QStringLiteral("outline")) &&
         !row.value(QStringLiteral("outline")).toBool();
}

} // namespace mitcad::cmd
