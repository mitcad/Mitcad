// SPDX-License-Identifier: MIT
// Decals as the renderer's bodies (mitcad#2244, RenderDecals.hpp): the
// Render workspace's live view (RenderMode) and the final render
// (RenderImagePanel) draw what the 3D view draws.
#include "render/RenderDecals.hpp"

#include <array>
#include <cstddef>
#include <cstdint>
#include <utility>

#include <QDir>
#include <QFileInfo>
#include <QtLogging>

#include "render/RenderScene.hpp"

namespace mitcad::render {
namespace {

// How far a decal's surface lies off its face (mm): over the face's own
// triangles, which cut inside a curved face between their corners, and
// clear of the face for the renderer's rays.
constexpr double kRenderLift = 0.03;

// The decal's mesh: its points lifted along their normals, the model's
// triangles of the part on the face (mitcad#2266) counter-clockwise seen
// from outside the material, its texture coordinates with v up the image.
MeshData meshOfDecal(const DecalShown& d) {
  MeshData mesh;
  for (std::size_t i = 0; i < d.points.size(); ++i) {
    for (int k = 0; k < 3; ++k) {
      const std::size_t axis = static_cast<std::size_t>(k);
      mesh.positions.push_back(static_cast<float>(d.points[i][axis] + d.normals[i][axis] * kRenderLift));
      mesh.normals.push_back(static_cast<float>(d.normals[i][axis]));
    }
    mesh.uvs.push_back(static_cast<float>(d.uvs[i][0]));
    mesh.uvs.push_back(static_cast<float>(1.0 - d.uvs[i][1]));
  }
  for (const std::array<std::size_t, 3>& t : d.triangles) {
    for (const std::size_t index : t) {
      mesh.indices.push_back(static_cast<std::uint32_t>(index));
    }
  }
  mesh.faceTriangles = {static_cast<std::uint32_t>(mesh.indices.size() / 3)};
  return mesh;
}

} // namespace

std::vector<DecalBody> decalBodies(const std::vector<DecalShown>& decals, const QHash<QString, QImage>& images,
                                   const QString& folder) {
  std::vector<DecalBody> bodies;
  const QDir dir(folder);
  for (const DecalShown& decal : decals) {
    const QImage picture = images.value(decal.sha256);
    if (picture.isNull() || decal.triangles.empty() || folder.isEmpty()) {
      continue;
    }
    // The image once per digest (a panel's picture has a key of its own).
    QString stem = decal.sha256;
    stem.replace(QLatin1Char('/'), QLatin1Char('-')).replace(QLatin1Char(':'), QLatin1Char('-'));
    const QString path = dir.filePath(QStringLiteral("decal-%1.png").arg(stem.right(64)));
    if (!QFileInfo::exists(path) && !picture.save(path, "PNG")) {
      qWarning().noquote() << QStringLiteral("Render decal %1 left out: its image cannot be written to %2")
                                  .arg(decal.name, path);
      continue;
    }
    DecalBody entry;
    entry.mesh = meshOfDecal(decal);
    const std::string key = meshKey(entry.mesh);
    entry.mesh.name = key;
    entry.body.id = "decal:" + decal.name.toStdString();
    entry.body.mesh = key;
    MaterialData& material = entry.body.material;
    material.baseColor = {1.0f, 1.0f, 1.0f};
    material.roughness = 0.5f;
    material.texture = QDir::toNativeSeparators(path).toStdString();
    material.textureUv = true;
    material.textureAlpha = true;
    material.opacity = static_cast<float>(decal.opacity);
    bodies.push_back(std::move(entry));
  }
  return bodies;
}

} // namespace mitcad::render
