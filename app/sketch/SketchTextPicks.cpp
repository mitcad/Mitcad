// SPDX-License-Identifier: MIT
// The text a pick in the sketch stands for (mitcad#1369): its outline, or a
// region its contours bound. Zoomed in, a click inside a letter (its stroke,
// or a counter such as the hole of an 'o') picks a profile, not the
// outline; the context menu's and a double-click's Edit Text (Edit QR Code
// for a code) take that region for its text, by the `sketch` query's
// regions with a `text` (core/model-api/src/commands.md). Only the sketch
// model's data: the app's unit tests take this file alone.
#include "SketchModel.hpp"

#include "../framework/Selection.hpp"

namespace mitcad::sketch {

const TextData* SketchModel::textOfPick(const SelectionItem& item) const {
  if (item.owner != uid) {
    return nullptr;
  }
  const QString id = item.kind == SelectKind::Profile ? regionTexts.value(item.name) : item.name;
  if (id.isEmpty()) {
    return nullptr;
  }
  for (const TextData& t : texts) {
    if (t.id == id) {
      return &t;
    }
  }
  return nullptr;
}

SelectionItem textSelectionItem(const SketchModel& model, const SelectionItem& item) {
  if (item.kind != SelectKind::Profile) {
    return item;
  }
  const TextData* text = model.textOfPick(item);
  if (text == nullptr) {
    return item;
  }
  SelectionItem curve{SelectKind::SketchCurve, item.owner, text->id, QString(), item.occurrence};
  curve.at = item.at;
  return curve;
}

} // namespace mitcad::sketch
