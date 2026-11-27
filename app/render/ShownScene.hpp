// SPDX-License-Identifier: MIT
#pragma once

// The scene of a final render as the 3D view shows the design: the shown
// bodies with their appearances (see-through ones as the rendered view
// shows them) and the decals drawn on their faces. Render Image
// (RenderImagePanel) and Export Turntable's rendered frames (mitcad#2523,
// view/TurntableExport.cpp) write the same scene file.

#include <cstddef>
#include <string>

#include <QString>
#include <QStringList>

namespace mitcad {
class OcctViewer;
}

namespace mitcad::render {

struct ShownScene {
  std::size_t bodies = 0;     // the shown bodies written
  QStringList missingTextures; // render::missingTextures of their appearances
};

// Writes the bodies the view shows and its decals as the worker's scene
// file at `scenePath` (glTF binary), the decals' images beside it in
// `folder`. Reads the shapes' triangulations: only while no model job
// computes them. False, with `error`, when the file cannot be written.
bool writeShownScene(const OcctViewer& viewer, const QString& folder, const QString& scenePath, ShownScene& written,
                     std::string& error);

} // namespace mitcad::render
