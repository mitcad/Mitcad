// SPDX-License-Identifier: MIT
#pragma once

#include <QColor>
#include <QIcon>
#include <QString>

namespace mitcad {

// What an icon is drawn on, which decides whether its dark outline is
// lightened: the widgets' palette (Palette: dark outline on a light palette,
// light on a dark one), or a surface of its own, like the 3D view, whose
// background does not follow the palette.
enum class IconBackdrop { Palette, Light, Dark };

// An icon of app/icons (<name>.svg), rendered here so that no image plugin
// is needed; a null icon when there is no such file. The icon renders the
// drawing at the size and the device pixel ratio it is asked for (sharp on
// HiDPI screens) and when it is asked, so that it follows the palette.
QIcon themeIcon(const QString& name, IconBackdrop backdrop = IconBackdrop::Palette);

// A small square of a colour, for the selection inputs of command panels.
QIcon swatchIcon(const QColor& color);

} // namespace mitcad
