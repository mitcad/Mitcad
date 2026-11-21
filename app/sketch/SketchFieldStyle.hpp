// SPDX-License-Identifier: MIT
#pragma once

// The style sheet of the sketch's value fields, which float over the view
// (a dimension's value, a tool's length or angle). The view's background
// does not follow the palette, so the fields are always light: their own
// light background, the role table's light text and frame, and a value
// that is not valid with the error's frame and fill (mitcad#1041). Selected
// text is the palette's pair in its light values, the accent (`Action`)
// with `AccentText` on it, as in every other field (mitcad#2086,
// docs/ui-conventions.md "States"); never `HoverFill`, the hovered row's.
// Plain Qt: the app's unit tests take this file alone with Theme.cpp.

#include <QString>

namespace mitcad::sketch {

QString valueFieldStyle(bool invalid);

} // namespace mitcad::sketch
