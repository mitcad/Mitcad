// SPDX-License-Identifier: MIT
#pragma once

// The pattern panels' Direction Type and Distance Type (mitcad#545,
// app/COMMANDS.md "Patterns"): how the choices map onto a definition's
// `symmetric` and `distance_type`, the length row's label, and the summary
// of the options the panels log when they add a pattern.

#include <QJsonObject>
#include <QString>

namespace mitcad {
class CommandState;
}

namespace mitcad::cmd {

// The Direction Type values: copies along the direction, or on both sides
// of the original.
QString oneDirection();       // "one"
QString symmetricDirection(); // "symmetric"

// The Direction Type a direction of a rectangular pattern's definition (or
// a circular or path pattern's definition) holds: symmetric when its
// `symmetric` is true, else one direction.
QString directionTypeOf(const QJsonObject& holder);

// Sets `symmetric` in a direction (or a definition) for the Direction
// Type: true for Symmetric, left out for One Direction (the default).
void setDirectionType(QJsonObject& holder, const QString& type);

// Whether a direction's Flip negates the distance it builds: not while the
// direction is symmetric (the panel hides Flip then).
bool flipApplies(const QString& directionType, bool flip);

// The length row's label for a Distance Type ("spacing" or "extent"),
// with the direction's suffix: "Spacing", "Extent 2".
QString lengthLabel(const QString& distanceType, const QString& suffix = QString());

// A rectangular, circular or path pattern's options in words, from its
// definition: "direction 1 symmetric, quantity 3, extent 40 mm; direction 2
// one direction, quantity 3, extent 100 mm", "symmetric, quantity 3, angle
// 90 deg", "one direction, quantity 4, spacing 15 mm, flipped". Empty for
// other definitions.
QString patternOptionsText(const QJsonObject& def);

// The Pattern Type (choice `object_type`) of Rectangular Pattern, Circular
// Pattern, Pattern on Path and Mirror once the selection made before the
// command went to their inputs (mitcad#1290): Features when the Features
// input (`features`) took something, a timeline feature or a face a
// feature made (as that feature), and the Bodies input (`bodies`)
// nothing. Otherwise the type stays as it is (Bodies, the default); Faces
// is never chosen from the selection.
void choosePreselectedPatternType(CommandState& state);

} // namespace mitcad::cmd
