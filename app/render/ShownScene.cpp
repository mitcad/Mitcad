// SPDX-License-Identifier: MIT
#include "render/ShownScene.hpp"

#include <algorithm>
#include <utility>
#include <vector>

#include <QDir>

#include "OcctViewer.hpp"
#include "render/RenderBatch.hpp"
#include "render/RenderDecals.hpp"
#include "render/RenderScene.hpp"

namespace mitcad::render {

bool writeShownScene(const OcctViewer& viewer, const QString& folder, const QString& scenePath, ShownScene& written,
                     std::string& error) {
  std::vector<SceneBody> bodies;
  const std::vector<BodyDisplay> shown = viewer.shownBodies();
  std::vector<const Appearance*> looks;
  for (const BodyDisplay& body : shown) {
    SceneBody scene;
    scene.name = body.occurrence.empty() ? body.uid : body.uid + "@" + body.occurrence;
    scene.shape = body.shape->occt();
    scene.placement = body.placement.Transformation();
    scene.appearance = body.appearance;
    scene.transparent = body.transparent; // mitcad#1093
    looks.push_back(&body.appearance);
    for (const BodyDisplay::FaceLook& look : body.faceLooks) {
      scene.faces.push_back({look.faces, look.appearance});
      looks.push_back(&look.appearance);
    }
    bodies.push_back(std::move(scene));
  }
  // Textures whose images are missing show their base colour (mitcad#53);
  // missing maps are left out (mitcad#314).
  written.missingTextures = missingTextures(looks);
  written.bodies = bodies.size();
  SceneUpdate scene = sceneOf(bodies);
  // Decals (mitcad#2244): what the view draws on the faces, as bodies of
  // their own, their images beside the scene's file.
  for (DecalBody& decal : decalBodies(viewer.drawnDecals(), viewer.drawnDecalImages(), folder)) {
    const bool held = std::any_of(scene.meshes.begin(), scene.meshes.end(),
                                  [&decal](const auto& known) { return known.first == decal.body.mesh; });
    if (!held) {
      scene.meshes.emplace_back(decal.body.mesh, std::move(decal.mesh));
    }
    scene.bodies.push_back(std::move(decal.body));
  }
  return writeSceneFile(scene, QDir::toNativeSeparators(scenePath).toStdString(), error);
}

} // namespace mitcad::render
