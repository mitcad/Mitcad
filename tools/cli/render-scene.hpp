// SPDX-License-Identifier: MIT
#pragma once

// The scene of mitcad-cli's final renders (render.cpp): `mitcad-cli render`
// and the MCP server's ray-traced model_render (mitcad#1489,
// mcp-raytrace.cpp) draw the same bodies with the same looks.

#include <vector>

#include <QColor>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QStringList>

#include <Bnd_Box.hxx>

#include "mitcad_bridge/lib.h"
#include "render/RenderBatch.hpp"

namespace mitcad::cli {

// A query's answer without arguments.
QJsonValue renderQuery(const mitcad::Document& document, const QString& name);

// The bodies of `instances` (entries of the `instances` query) as placed,
// with their appearances and their faces' (mitcad#53), textures with their
// image files found in `folder` or the design (mitcad#314); their box; and
// the textures' images that are not drawn (render::missingTextures).
struct DocumentScene {
  std::vector<render::SceneBody> bodies;
  Bnd_Box box;
  QStringList missingTextures;
};
DocumentScene documentScene(const mitcad::Document& document, const QString& folder, const QJsonArray& instances);

// Behind the bodies: the render settings' colour, or the 3D view's default
// light gradient for the view's background.
void renderBackground(const QJsonObject& settings, QColor& top, QColor& bottom);

} // namespace mitcad::cli
