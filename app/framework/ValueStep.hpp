// SPDX-License-Identifier: MIT
#pragma once

// Stepping a value field's number with Up and Down and the mouse wheel
// (mitcad#1254, app/COMMANDS.md "Stepping a value"): the command panel's
// value fields and the sketch's value fields (framework/ParameterField.hpp
// sends the keys and the wheel here). Text rules without widgets, so that
// they are unit tested (app/tests/value_step.cpp).
//
// A step is 1 in the unit the field shows (1 mm, 1 in, 1°, or 1 for a
// plain number), Shift 10 of it, the fine modifier 0.1 of it. Only a plain
// value is stepped: a number with an optional sign and at most one unit of
// the field's kind ("4", "10 mm", "0.5 in", "360 deg", "45°", "12,5 mm"),
// also after `name = ` (the field names its own parameter, mitcad#231). An
// expression or a parameter name ("d1*2", "d1", "10 mm + 2") is left as it
// is.

#include <limits>
#include <optional>

#include <QString>
#include <QtCore/qnamespace.h>

#include "ParameterSuggest.hpp"

namespace mitcad {

// The smallest value a step goes to, in mm (a length), degrees (an angle)
// or as the number itself (a count): a pattern's quantity is at least 1, a
// radius above 0. None by default. A step down stops at an inclusive
// minimum (a count of 3 stepped down by 10 is 1) and does not take a value
// to or below an exclusive one (a radius of 0.5 mm stays 0.5 mm on Down;
// Ctrl+Down makes it 0.4 mm). A step up is always taken, also from below
// the minimum.
struct ValueStepLimit {
  double minimum = -std::numeric_limits<double>::infinity();
  // The minimum itself is not a value: above it (a positive length).
  bool above = false;
};

// The step for the modifiers held with Up, Down or the wheel: 1 without
// modifiers, 10 with Shift, 0.1 with the fine modifier; 0 for any other
// combination (not a step: the key is the field's or a shortcut's as
// before). The fine modifier is Ctrl on Windows and Linux. On macOS (where
// Qt reports the Command key as Ctrl) it is Option (Alt): Cmd+Up and
// Cmd+Down move the caret to the start and end of a text field, and the
// Control key with Up and Down is the system's Mission Control and
// application windows. The keypad flag of an arrow key on the keypad is
// ignored.
double valueStepFactor(Qt::KeyboardModifiers modifiers, bool macKeys);

// The line a stepping field's tooltip gets: "Up/Down steps the value;
// Shift ×10, Ctrl ×0.1" ("Option ×0.1" on macOS).
QString valueStepHint(bool macKeys);

// `text` stepped by `step` (signed, in the unit the text shows), kept in
// that unit with the decimals the text or the step has ("10 mm" + 10 is
// "20 mm", "0.5 in" + 1 is "1.5 in", "360 deg" + 0.1 is "360.1 deg", "4"
// + 1 is "5"; a decimal comma comes back as a point). The limit is applied
// in the shown unit: `bareUnit` is the size in mm of the unit a bare
// number of a length is read in (the design's length unit; a bare angle is
// in degrees). The text itself when the limit stops the step; nothing when
// the text is not a plain value of the kind (an expression, a parameter's
// name, a unit of another kind, a text field).
std::optional<QString> steppedValue(const QString& text, ParameterKind kind, double step,
                                    const ValueStepLimit& limit = {}, double bareUnit = 1.0);

// The limit of a sketch dimension's value editor, by the dimension's type
// as the sketch query gives it and the range the sketch solver checks
// (core/sketch/src/lib.rs, DimensionKind::range): a signed horizontal or
// vertical distance and an angle have none; the distances between points,
// lines and curves (horizontal_distance, vertical_distance,
// point_line_distance, line_distance, tangent_distance,
// concentric_distance, tangent_circle_distance, tangent_point_distance)
// stop at 0; every other one (a distance between two
// points, a length, a radius, a diameter, an arc length) stays above 0.
ValueStepLimit sketchDimensionStepLimit(const QString& type, bool signedOffset);

} // namespace mitcad
