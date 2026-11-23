// SPDX-License-Identifier: MIT
#pragma once

#include <string>
#include <vector>

#include <QHash>
#include <QImage>
#include <QString>

#include "render/Renderer.hpp"
#include "view/Decals.hpp"

namespace mitcad::render {

// A decal as the renderer draws it (mitcad#2244): a body of its own whose
// mesh is the decal's grid on its face and whose material is its image.
struct DecalBody {
  BodyData body;
  MeshData mesh; // named by its key (meshKey), as the worker holds it
};

// The decals the view draws (OcctViewer::drawnDecals, their pictures by
// digest) as renderer bodies: each mesh the decal's grid in design
// coordinates, lifted a little off the face along its normals so that it
// lies over the face's own triangles, only the model's triangles of the
// part on the face (cut at its edges, mitcad#2266) facing out of the
// material, the image's coordinates as the mesh's texture coordinates;
// each material white with the image as its base colour by those
// coordinates (MaterialData::textureUv), the image's alpha times the
// decal's opacity as the surface's (MaterialData::textureAlpha), the image
// written once into `folder` as a PNG file named by its digest. A decal
// whose picture is missing, wholly off its face, or whose file cannot be
// written, is left out.
std::vector<DecalBody> decalBodies(const std::vector<DecalShown>& decals, const QHash<QString, QImage>& images,
                                   const QString& folder);

} // namespace mitcad::render
