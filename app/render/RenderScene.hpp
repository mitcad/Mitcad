// SPDX-License-Identifier: MIT
#pragma once

#include <string>
#include <vector>

#include <QJsonObject>
#include <QString>

#include "render/Renderer.hpp"

namespace mitcad::render {

// The scene as the application hands it to the render worker
// (docs/rendering.md, "Scene updates"): a "scene" command lists every body
// (id, mesh key, placement, material) and names a binary glTF 2.0 file
// (.glb) with only the meshes the worker does not hold yet.

// The meshes as a .glb file: one node with one mesh per MeshData, named by
// its name (the key), positions and normals in mm, 32-bit indices, the
// faces' triangle counts in the primitive's extras ("face_triangles"), no
// materials. Written by the application, read in the worker (cgltf); any
// glTF file with triangle meshes can be read (node placements are applied
// to the vertices).
bool writeMeshFile(const std::vector<const MeshData*>& meshes, const std::string& path, std::string& error);
bool readMeshFile(const std::string& path, std::vector<MeshData>& meshes, std::string& error);

// A mesh's content hash (positions, normals, indices and the faces'
// triangle counts; hex): the key under which the worker holds it. Bodies
// with the same triangles share it.
std::string meshKey(const MeshData& mesh);

// The "scene" command for an update whose new meshes are in `meshFile`
// (empty when there are none), and the update a command describes, with
// the meshes read from its file. A body lists its faces with materials of
// their own as "faces": [{"faces": [indices], "material": {...}}].
QJsonObject sceneCommand(const SceneUpdate& update, const QString& meshFile);
bool sceneUpdateOf(const QJsonObject& command, SceneUpdate& update, std::string& error);

// A material as JSON (MaterialData's fields in snake case: "base_color",
// "metallic", "roughness", "specular", "transmission", "ior", "coat",
// "coat_roughness", "emission_color", "emission", "opacity"; linear
// colours; "texture": {"path", "size", "rotation", "projection"} when it
// has one) and back; missing members keep MaterialData's defaults.
QJsonObject materialJson(const MaterialData& material);
MaterialData materialOf(const QJsonObject& json);

// A whole scene in one .glb file (the final render's job, mitcad#48): its
// meshes as writeMeshFile writes them, and its bodies (as the "scene"
// command lists them) in the file's extras ("mitcad_scene"). Any other
// glTF file with triangle meshes reads as a body per mesh in the default
// material.
bool writeSceneFile(const SceneUpdate& scene, const std::string& path, std::string& error);
bool readSceneFile(const std::string& path, SceneUpdate& scene, std::string& error);

// The test scene: a 60 x 40 x 20 mm block with 6 mm fillets on its upright
// edges, standing on the ground at the origin.
SceneUpdate testScene();

} // namespace mitcad::render
