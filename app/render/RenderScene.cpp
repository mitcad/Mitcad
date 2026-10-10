// SPDX-License-Identifier: MIT
#include "render/RenderScene.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <limits>
#include <vector>

#include <QByteArray>
#include <QCryptographicHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QString>

#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepMesh_IncrementalMesh.hxx>

// The reader, compiled here (cgltf is a single header).
#define CGLTF_IMPLEMENTATION
#include <cgltf.h>

#include "render/RenderMesh.hpp"

namespace mitcad::render {

namespace {

// glTF's numbers for component types and buffer targets.
constexpr int kFloat = 5126;
constexpr int kUnsignedInt = 5125;
constexpr int kArrayBuffer = 34962;
constexpr int kElementArrayBuffer = 34963;

void appendBytes(std::vector<char>& buffer, const void* data, std::size_t bytes) {
  const auto* begin = static_cast<const char*>(data);
  buffer.insert(buffer.end(), begin, begin + bytes);
  while (buffer.size() % 4 != 0) {
    buffer.push_back(0);
  }
}

void appendUint32(std::vector<char>& out, std::uint32_t value) {
  // glb is little-endian, as every platform Mitcad builds for.
  appendBytes(out, &value, sizeof value);
}

QJsonArray vec3(const float* v) { return QJsonArray{double(v[0]), double(v[1]), double(v[2])}; }

} // namespace

namespace {

bool writeGlb(const std::vector<const MeshData*>& meshList, const QJsonObject& extras, const std::string& path,
              std::string& error) {
  std::vector<char> binary;
  QJsonArray nodes;
  QJsonArray meshes;
  QJsonArray accessors;
  QJsonArray views;
  QJsonArray sceneNodes;
  const auto addView = [&](const void* data, std::size_t bytes, int target) {
    const auto offset = static_cast<qint64>(binary.size());
    appendBytes(binary, data, bytes);
    views.append(QJsonObject{{QStringLiteral("buffer"), 0},
                             {QStringLiteral("byteOffset"), offset},
                             {QStringLiteral("byteLength"), static_cast<qint64>(bytes)},
                             {QStringLiteral("target"), target}});
    return static_cast<int>(views.size() - 1);
  };
  for (const MeshData* entry : meshList) {
    const MeshData& mesh = *entry;
    if (mesh.indices.empty()) {
      continue;
    }
    const auto vertices = static_cast<qint64>(mesh.positions.size() / 3);
    float low[3] = {std::numeric_limits<float>::max(), std::numeric_limits<float>::max(),
                    std::numeric_limits<float>::max()};
    float high[3] = {-low[0], -low[1], -low[2]};
    for (std::size_t i = 0; i < mesh.positions.size(); ++i) {
      low[i % 3] = std::min(low[i % 3], mesh.positions[i]);
      high[i % 3] = std::max(high[i % 3], mesh.positions[i]);
    }
    const int positions = addView(mesh.positions.data(), mesh.positions.size() * sizeof(float), kArrayBuffer);
    const int normals = addView(mesh.normals.data(), mesh.normals.size() * sizeof(float), kArrayBuffer);
    const int indices =
        addView(mesh.indices.data(), mesh.indices.size() * sizeof(std::uint32_t), kElementArrayBuffer);
    const auto accessor = static_cast<int>(accessors.size());
    accessors.append(QJsonObject{{QStringLiteral("bufferView"), positions},
                                 {QStringLiteral("componentType"), kFloat},
                                 {QStringLiteral("count"), vertices},
                                 {QStringLiteral("type"), QStringLiteral("VEC3")},
                                 {QStringLiteral("min"), vec3(low)},
                                 {QStringLiteral("max"), vec3(high)}});
    accessors.append(QJsonObject{{QStringLiteral("bufferView"), normals},
                                 {QStringLiteral("componentType"), kFloat},
                                 {QStringLiteral("count"), vertices},
                                 {QStringLiteral("type"), QStringLiteral("VEC3")}});
    accessors.append(QJsonObject{{QStringLiteral("bufferView"), indices},
                                 {QStringLiteral("componentType"), kUnsignedInt},
                                 {QStringLiteral("count"), static_cast<qint64>(mesh.indices.size())},
                                 {QStringLiteral("type"), QStringLiteral("SCALAR")}});
    QJsonObject primitive{
        {QStringLiteral("attributes"),
         QJsonObject{{QStringLiteral("POSITION"), accessor}, {QStringLiteral("NORMAL"), accessor + 1}}},
        {QStringLiteral("indices"), accessor + 2}};
    if (!mesh.faceTriangles.empty()) {
      // The faces' triangles (mitcad#53), for materials of faces.
      QJsonArray faces;
      for (const std::uint32_t count : mesh.faceTriangles) {
        faces.append(static_cast<qint64>(count));
      }
      primitive.insert(QStringLiteral("extras"), QJsonObject{{QStringLiteral("face_triangles"), faces}});
    }
    const auto meshIndex = static_cast<int>(meshes.size());
    meshes.append(QJsonObject{{QStringLiteral("name"), QString::fromStdString(mesh.name)},
                              {QStringLiteral("primitives"), QJsonArray{primitive}}});
    sceneNodes.append(static_cast<int>(nodes.size()));
    nodes.append(QJsonObject{{QStringLiteral("name"), QString::fromStdString(mesh.name)},
                             {QStringLiteral("mesh"), meshIndex}});
  }
  QJsonObject root{
      {QStringLiteral("asset"),
       QJsonObject{{QStringLiteral("version"), QStringLiteral("2.0")}, {QStringLiteral("generator"), QStringLiteral("Mitcad")}}},
      {QStringLiteral("scene"), 0},
      {QStringLiteral("scenes"), QJsonArray{QJsonObject{{QStringLiteral("nodes"), sceneNodes}}}},
      {QStringLiteral("nodes"), nodes},
      {QStringLiteral("meshes"), meshes},
      {QStringLiteral("accessors"), accessors},
      {QStringLiteral("bufferViews"), views},
      {QStringLiteral("buffers"),
       QJsonArray{QJsonObject{{QStringLiteral("byteLength"), static_cast<qint64>(binary.size())}}}}};
  if (!extras.isEmpty()) {
    root.insert(QStringLiteral("extras"), extras);
  }
  if (meshes.isEmpty()) {
    // glTF needs no buffer without data.
    root.remove(QStringLiteral("buffers"));
    root.remove(QStringLiteral("bufferViews"));
    root.remove(QStringLiteral("accessors"));
  }
  QByteArray json = QJsonDocument(root).toJson(QJsonDocument::Compact);
  while (json.size() % 4 != 0) {
    json.append(' ');
  }
  std::vector<char> out;
  const bool hasBinary = !binary.empty();
  const auto total = static_cast<std::uint32_t>(12 + 8 + json.size() + (hasBinary ? 8 + binary.size() : 0));
  appendUint32(out, 0x46546c67); // "glTF"
  appendUint32(out, 2);
  appendUint32(out, total);
  appendUint32(out, static_cast<std::uint32_t>(json.size()));
  appendUint32(out, 0x4e4f534a); // "JSON"
  appendBytes(out, json.constData(), static_cast<std::size_t>(json.size()));
  if (hasBinary) {
    appendUint32(out, static_cast<std::uint32_t>(binary.size()));
    appendUint32(out, 0x004e4942); // "BIN"
    appendBytes(out, binary.data(), binary.size());
  }
  std::ofstream file(path, std::ios::binary | std::ios::trunc);
  file.write(out.data(), static_cast<std::streamsize>(out.size()));
  file.close();
  if (!file) {
    error = "cannot write " + path;
    return false;
  }
  return true;
}

bool readGlb(const std::string& path, std::vector<MeshData>& meshes, QJsonObject& extras, std::string& error) {
  cgltf_options options{};
  cgltf_data* data = nullptr;
  if (cgltf_parse_file(&options, path.c_str(), &data) != cgltf_result_success) {
    error = "not a glTF file: " + path;
    return false;
  }
  struct Free {
    cgltf_data* data;
    ~Free() { cgltf_free(data); }
  } free{data};
  if (cgltf_load_buffers(&options, data, path.c_str()) != cgltf_result_success ||
      cgltf_validate(data) != cgltf_result_success) {
    error = "invalid glTF data in " + path;
    return false;
  }
  meshes.clear();
  extras = QJsonObject();
  if (data->extras.data != nullptr) {
    extras = QJsonDocument::fromJson(QByteArray(data->extras.data)).object();
  }
  for (cgltf_size n = 0; n < data->nodes_count; ++n) {
    const cgltf_node& node = data->nodes[n];
    if (node.mesh == nullptr) {
      continue;
    }
    float world[16];
    cgltf_node_transform_world(&node, world);
    for (cgltf_size p = 0; p < node.mesh->primitives_count; ++p) {
      const cgltf_primitive& primitive = node.mesh->primitives[p];
      if (primitive.type != cgltf_primitive_type_triangles) {
        continue;
      }
      const cgltf_accessor* positions = nullptr;
      const cgltf_accessor* normals = nullptr;
      for (cgltf_size a = 0; a < primitive.attributes_count; ++a) {
        if (primitive.attributes[a].type == cgltf_attribute_type_position) {
          positions = primitive.attributes[a].data;
        } else if (primitive.attributes[a].type == cgltf_attribute_type_normal) {
          normals = primitive.attributes[a].data;
        }
      }
      if (positions == nullptr) {
        continue;
      }
      MeshData mesh;
      mesh.name = node.mesh->name != nullptr ? node.mesh->name : node.name != nullptr ? node.name : std::string();
      mesh.positions.resize(positions->count * 3);
      cgltf_accessor_unpack_floats(positions, mesh.positions.data(), mesh.positions.size());
      if (normals != nullptr && normals->count == positions->count) {
        mesh.normals.resize(normals->count * 3);
        cgltf_accessor_unpack_floats(normals, mesh.normals.data(), mesh.normals.size());
      }
      if (primitive.indices != nullptr) {
        mesh.indices.resize(primitive.indices->count);
        for (cgltf_size i = 0; i < primitive.indices->count; ++i) {
          mesh.indices[i] = static_cast<std::uint32_t>(cgltf_accessor_read_index(primitive.indices, i));
        }
      } else {
        mesh.indices.resize(positions->count);
        for (cgltf_size i = 0; i < positions->count; ++i) {
          mesh.indices[i] = static_cast<std::uint32_t>(i);
        }
      }
      // The node's placement (column-major 4 x 4) on the vertices; Mitcad's
      // own files place nothing.
      bool identity = true;
      for (int i = 0; i < 16; ++i) {
        identity = identity && world[i] == (i % 5 == 0 ? 1.0f : 0.0f);
      }
      // The faces' triangles (Mitcad's own meshes).
      if (primitive.extras.data != nullptr) {
        const QJsonArray faces = QJsonDocument::fromJson(QByteArray(primitive.extras.data))
                                     .object()
                                     .value(QStringLiteral("face_triangles"))
                                     .toArray();
        std::size_t total = 0;
        for (const QJsonValue& count : faces) {
          mesh.faceTriangles.push_back(static_cast<std::uint32_t>(count.toInteger()));
          total += mesh.faceTriangles.back();
        }
        if (total * 3 != mesh.indices.size()) {
          mesh.faceTriangles.clear();
        }
      }
      if (!identity) {
        for (std::size_t v = 0; v + 2 < mesh.positions.size(); v += 3) {
          const float position[3] = {mesh.positions[v], mesh.positions[v + 1], mesh.positions[v + 2]};
          for (int row = 0; row < 3; ++row) {
            mesh.positions[v + static_cast<std::size_t>(row)] =
                world[row] * position[0] + world[4 + row] * position[1] + world[8 + row] * position[2] + world[12 + row];
          }
        }
        for (std::size_t v = 0; v + 2 < mesh.normals.size(); v += 3) {
          const float normal[3] = {mesh.normals[v], mesh.normals[v + 1], mesh.normals[v + 2]};
          float out[3];
          for (int row = 0; row < 3; ++row) {
            out[row] = world[row] * normal[0] + world[4 + row] * normal[1] + world[8 + row] * normal[2];
          }
          const float length = std::sqrt(out[0] * out[0] + out[1] * out[1] + out[2] * out[2]);
          for (int row = 0; row < 3; ++row) {
            mesh.normals[v + static_cast<std::size_t>(row)] = length > 0.0f ? out[row] / length : 0.0f;
          }
        }
      }
      meshes.push_back(std::move(mesh));
    }
  }
  return true;
}

} // namespace

bool writeMeshFile(const std::vector<const MeshData*>& meshes, const std::string& path, std::string& error) {
  return writeGlb(meshes, QJsonObject(), path, error);
}

bool readMeshFile(const std::string& path, std::vector<MeshData>& meshes, std::string& error) {
  QJsonObject extras;
  return readGlb(path, meshes, extras, error);
}

std::string meshKey(const MeshData& mesh) {
  QCryptographicHash hash(QCryptographicHash::Blake2b_160);
  const auto add = [&hash](const void* data, std::size_t bytes) {
    hash.addData(QByteArrayView(static_cast<const char*>(data), static_cast<qsizetype>(bytes)));
  };
  const std::uint64_t sizes[4] = {mesh.positions.size(), mesh.normals.size(), mesh.indices.size(),
                                  mesh.faceTriangles.size()};
  add(sizes, sizeof sizes);
  add(mesh.positions.data(), mesh.positions.size() * sizeof(float));
  add(mesh.normals.data(), mesh.normals.size() * sizeof(float));
  add(mesh.indices.data(), mesh.indices.size() * sizeof(std::uint32_t));
  add(mesh.faceTriangles.data(), mesh.faceTriangles.size() * sizeof(std::uint32_t));
  return hash.result().toHex().toStdString();
}

QJsonObject materialJson(const MaterialData& m) {
  const auto color = [](const std::array<float, 3>& c) { return QJsonArray{double(c[0]), double(c[1]), double(c[2])}; };
  QJsonObject json{{QStringLiteral("base_color"), color(m.baseColor)},
          {QStringLiteral("metallic"), double(m.metallic)},
          {QStringLiteral("roughness"), double(m.roughness)},
          {QStringLiteral("specular"), double(m.specular)},
          {QStringLiteral("transmission"), double(m.transmission)},
          {QStringLiteral("ior"), double(m.ior)},
          {QStringLiteral("coat"), double(m.coat)},
          {QStringLiteral("coat_roughness"), double(m.coatRoughness)},
          {QStringLiteral("emission_color"), color(m.emissionColor)},
          {QStringLiteral("emission"), double(m.emission)},
          {QStringLiteral("opacity"), double(m.opacity)}};
  if (!m.texture.empty()) {
    json.insert(QStringLiteral("texture"),
                QJsonObject{{QStringLiteral("path"), QString::fromStdString(m.texture)},
                            {QStringLiteral("size"), QJsonArray{double(m.textureSize[0]), double(m.textureSize[1])}},
                            {QStringLiteral("rotation"), double(m.textureRotation)},
                            {QStringLiteral("projection"), m.texturePlanar ? QStringLiteral("planar") : QStringLiteral("box")}});
  }
  return json;
}

MaterialData materialOf(const QJsonObject& json) {
  MaterialData m;
  const auto color = [&json](const char* name, std::array<float, 3>& c) {
    const QJsonArray array = json.value(QLatin1String(name)).toArray();
    if (array.size() == 3) {
      for (int i = 0; i < 3; ++i) {
        c[static_cast<std::size_t>(i)] = static_cast<float>(array.at(i).toDouble());
      }
    }
  };
  const auto number = [&json](const char* name, float& value) {
    value = static_cast<float>(json.value(QLatin1String(name)).toDouble(value));
  };
  color("base_color", m.baseColor);
  number("metallic", m.metallic);
  number("roughness", m.roughness);
  number("specular", m.specular);
  number("transmission", m.transmission);
  number("ior", m.ior);
  number("coat", m.coat);
  number("coat_roughness", m.coatRoughness);
  color("emission_color", m.emissionColor);
  number("emission", m.emission);
  number("opacity", m.opacity);
  const QJsonObject texture = json.value(QStringLiteral("texture")).toObject();
  m.texture = texture.value(QStringLiteral("path")).toString().toStdString();
  const QJsonArray size = texture.value(QStringLiteral("size")).toArray();
  if (size.size() == 2 && size.at(0).toDouble() > 0.0 && size.at(1).toDouble() > 0.0) {
    m.textureSize = {static_cast<float>(size.at(0).toDouble()), static_cast<float>(size.at(1).toDouble())};
  }
  m.textureRotation = static_cast<float>(texture.value(QStringLiteral("rotation")).toDouble());
  m.texturePlanar = texture.value(QStringLiteral("projection")).toString() == QLatin1String("planar");
  return m;
}

QJsonObject sceneCommand(const SceneUpdate& update, const QString& meshFile) {
  QJsonArray bodies;
  for (const BodyData& body : update.bodies) {
    QJsonArray transform;
    for (const float value : body.transform) {
      transform.append(double(value));
    }
    QJsonObject json{{QStringLiteral("id"), QString::fromStdString(body.id)},
                     {QStringLiteral("mesh"), QString::fromStdString(body.mesh)},
                     {QStringLiteral("transform"), transform},
                     {QStringLiteral("material"), materialJson(body.material)}};
    if (!body.faces.empty()) {
      QJsonArray faces;
      for (const FaceMaterial& group : body.faces) {
        QJsonArray indices;
        for (const std::uint32_t face : group.faces) {
          indices.append(static_cast<qint64>(face));
        }
        faces.append(QJsonObject{{QStringLiteral("faces"), indices},
                                 {QStringLiteral("material"), materialJson(group.material)}});
      }
      json.insert(QStringLiteral("faces"), faces);
    }
    bodies.append(json);
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("scene")},
                      {QStringLiteral("bodies"), bodies},
                      {QStringLiteral("ground"), update.ground}};
  if (!meshFile.isEmpty()) {
    command.insert(QStringLiteral("meshes"), meshFile);
  }
  if (!update.released.empty()) {
    QJsonArray released;
    for (const std::string& key : update.released) {
      released.append(QString::fromStdString(key));
    }
    command.insert(QStringLiteral("release"), released);
  }
  return command;
}

namespace {

// The "bodies" of a scene command into the update.
bool bodiesOf(const QJsonObject& command, SceneUpdate& update, std::string& error) {
  for (const QJsonValue& value : command.value(QStringLiteral("bodies")).toArray()) {
    const QJsonObject json = value.toObject();
    BodyData body;
    body.id = json.value(QStringLiteral("id")).toString().toStdString();
    body.mesh = json.value(QStringLiteral("mesh")).toString().toStdString();
    const QJsonArray transform = json.value(QStringLiteral("transform")).toArray();
    if (transform.size() == 12) {
      for (int i = 0; i < 12; ++i) {
        body.transform[static_cast<std::size_t>(i)] = static_cast<float>(transform.at(i).toDouble());
      }
    }
    const QJsonObject material = json.value(QStringLiteral("material")).toObject();
    body.material = materialOf(material);
    // The key from all the fields, so that the same material is one key.
    body.materialKey = QJsonDocument(materialJson(body.material)).toJson(QJsonDocument::Compact).toStdString();
    // Faces with materials of their own (mitcad#53).
    for (const QJsonValue& groupValue : json.value(QStringLiteral("faces")).toArray()) {
      const QJsonObject groupJson = groupValue.toObject();
      FaceMaterial group;
      for (const QJsonValue& face : groupJson.value(QStringLiteral("faces")).toArray()) {
        if (face.toInteger(-1) >= 0) {
          group.faces.push_back(static_cast<std::uint32_t>(face.toInteger()));
        }
      }
      group.material = materialOf(groupJson.value(QStringLiteral("material")).toObject());
      group.materialKey = QJsonDocument(materialJson(group.material)).toJson(QJsonDocument::Compact).toStdString();
      if (!group.faces.empty()) {
        body.faces.push_back(std::move(group));
      }
    }
    if (body.id.empty() || body.mesh.empty()) {
      error = "a body without an id or a mesh";
      return false;
    }
    update.bodies.push_back(std::move(body));
  }
  return true;
}

} // namespace

bool sceneUpdateOf(const QJsonObject& command, SceneUpdate& update, std::string& error) {
  update = SceneUpdate();
  update.ground = command.value(QStringLiteral("ground")).toBool(true);
  const QString meshFile = command.value(QStringLiteral("meshes")).toString();
  if (!meshFile.isEmpty()) {
    std::vector<MeshData> meshes;
    if (!readMeshFile(meshFile.toStdString(), meshes, error)) {
      return false;
    }
    for (MeshData& mesh : meshes) {
      std::string key = mesh.name;
      update.meshes.emplace_back(std::move(key), std::move(mesh));
    }
  }
  for (const QJsonValue& key : command.value(QStringLiteral("release")).toArray()) {
    update.released.push_back(key.toString().toStdString());
  }
  return bodiesOf(command, update, error);
}

bool writeSceneFile(const SceneUpdate& scene, const std::string& path, std::string& error) {
  std::vector<const MeshData*> meshes;
  for (const auto& [key, mesh] : scene.meshes) {
    meshes.push_back(&mesh);
  }
  QJsonObject bodies = sceneCommand(scene, QString());
  bodies.remove(QStringLiteral("cmd"));
  return writeGlb(meshes, QJsonObject{{QStringLiteral("mitcad_scene"), bodies}}, path, error);
}

bool readSceneFile(const std::string& path, SceneUpdate& scene, std::string& error) {
  scene = SceneUpdate();
  std::vector<MeshData> meshes;
  QJsonObject extras;
  if (!readGlb(path, meshes, extras, error)) {
    return false;
  }
  const QJsonObject bodies = extras.value(QStringLiteral("mitcad_scene")).toObject();
  if (!bodies.isEmpty()) {
    scene.ground = bodies.value(QStringLiteral("ground")).toBool(true);
    if (!bodiesOf(bodies, scene, error)) {
      return false;
    }
  }
  int index = 0;
  for (MeshData& mesh : meshes) {
    std::string key = mesh.name;
    if (bodies.isEmpty()) {
      // Another glTF file: a body per mesh, placed already.
      key = std::to_string(index++) + ":" + key;
      BodyData body;
      body.id = key;
      body.mesh = key;
      body.materialKey = QJsonDocument(materialJson(body.material)).toJson(QJsonDocument::Compact).toStdString();
      scene.bodies.push_back(std::move(body));
    }
    scene.meshes.emplace_back(std::move(key), std::move(mesh));
  }
  return true;
}

SceneUpdate testScene() {
  BRepPrimAPI_MakeBox box(gp_Pnt(-30, -20, 0), 60, 40, 20);
  BRepFilletAPI_MakeFillet fillet(box.Shape());
  for (TopExp_Explorer edges(box.Shape(), TopAbs_EDGE); edges.More(); edges.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(edges.Current());
    BRepAdaptor_Curve curve(edge);
    const gp_Vec along(curve.Value(curve.FirstParameter()), curve.Value(curve.LastParameter()));
    if (std::abs(along.Z()) > 1.0) { // upright
      fillet.Add(6.0, edge);
    }
  }
  const TopoDS_Shape shape = fillet.Shape();
  BRepMesh_IncrementalMesh(shape, 0.05, false, 0.2);
  SceneUpdate scene;
  MeshData mesh = meshOf(shape);
  const std::string key = meshKey(mesh);
  mesh.name = key;
  BodyData body;
  body.id = "test block";
  body.mesh = key;
  body.material.baseColor = {0.13f, 0.30f, 0.62f};
  body.material.roughness = 0.35f;
  body.materialKey = QJsonDocument(materialJson(body.material)).toJson(QJsonDocument::Compact).toStdString();
  scene.meshes.emplace_back(key, std::move(mesh));
  scene.bodies.push_back(std::move(body));
  scene.ground = true;
  return scene;
}

} // namespace mitcad::render
