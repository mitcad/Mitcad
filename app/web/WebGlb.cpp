// SPDX-License-Identifier: MIT
#include "web/WebGlb.hpp"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <functional>
#include <initializer_list>
#include <limits>
#include <string>
#include <utility>

#include <QBuffer>
#include <QFile>
#include <QHash>
#include <QImage>
#include <QImageReader>
#include <QIODevice>
#include <QJsonDocument>
#include <QJsonObject>
#include <QObject>
#include <QSet>
#include <QSize>

namespace mitcad::web {

namespace {

// glTF's numbers for component types, buffer targets and samplers.
constexpr int kFloat = 5126;
constexpr int kUnsignedInt = 5125;
constexpr int kArrayBuffer = 34962;
constexpr int kElementArrayBuffer = 34963;
constexpr int kRepeat = 10497;
constexpr int kLinear = 9729;
constexpr int kLinearMipmapLinear = 9987;

// Texture images larger than this (pixels a side) are scaled down: a page
// has to load in a phone's browser.
constexpr int kMaxImageSide = 2048;

QJsonArray numbers(std::initializer_list<double> values) {
  QJsonArray array;
  for (const double value : values) {
    array.append(value);
  }
  return array;
}

QString text(const std::string& value) { return QString::fromStdString(value); }

struct EncodedImage {
  QByteArray bytes;
  QString mime;
};

QByteArray pngOf(const QImage& image) {
  QByteArray bytes;
  QBuffer buffer(&bytes);
  buffer.open(QIODevice::WriteOnly);
  image.save(&buffer, "PNG");
  return bytes;
}

// An image file, scaled down to kMaxImageSide when it is larger.
bool readImage(const QString& file, QImage& image, QString& why) {
  QImageReader reader(file);
  reader.setAutoTransform(true);
  const QSize size = reader.size();
  if (size.isValid() && std::max(size.width(), size.height()) > kMaxImageSide) {
    reader.setScaledSize(size.scaled(kMaxImageSide, kMaxImageSide, Qt::KeepAspectRatio));
  }
  image = reader.read();
  if (image.isNull()) {
    why = reader.errorString();
    return false;
  }
  return true;
}

// An image for the file: a PNG or JPEG file as it is when it is small
// enough, anything else Qt reads (or a larger image) as a PNG image.
bool encodedImage(const QString& file, EncodedImage& out, QString& why) {
  QFile source(file);
  if (!source.open(QIODevice::ReadOnly)) {
    why = source.errorString();
    return false;
  }
  const QByteArray bytes = source.readAll();
  source.close();
  const bool png = bytes.startsWith("\x89PNG");
  const bool jpeg = bytes.startsWith("\xFF\xD8\xFF");
  const QSize size = QImageReader(file).size();
  if ((png || jpeg) && size.isValid() && std::max(size.width(), size.height()) <= kMaxImageSide) {
    out.bytes = bytes;
    out.mime = png ? QStringLiteral("image/png") : QStringLiteral("image/jpeg");
    return true;
  }
  QImage image;
  if (!readImage(file, image, why)) {
    return false;
  }
  out.bytes = pngOf(image);
  out.mime = QStringLiteral("image/png");
  return !out.bytes.isEmpty();
}

// glTF's one image of roughness (green) and metalness (blue) from the
// texture set's grey maps; a missing one is white (its factor counts).
bool metallicRoughnessImage(const QString& roughnessFile, const QString& metalnessFile, EncodedImage& out,
                            QString& why) {
  QImage roughness;
  QImage metalness;
  if (!roughnessFile.isEmpty() && !readImage(roughnessFile, roughness, why)) {
    why = QStringLiteral("%1: %2").arg(roughnessFile, why);
    return false;
  }
  if (!metalnessFile.isEmpty() && !readImage(metalnessFile, metalness, why)) {
    why = QStringLiteral("%1: %2").arg(metalnessFile, why);
    return false;
  }
  const QSize size = !roughness.isNull() ? roughness.size() : metalness.size();
  if (size.isEmpty()) {
    why = QStringLiteral("empty image");
    return false;
  }
  const auto grey = [&size](const QImage& image) {
    if (image.isNull()) {
      return image;
    }
    const QImage scaled =
        image.size() == size ? image : image.scaled(size, Qt::IgnoreAspectRatio, Qt::SmoothTransformation);
    return scaled.convertToFormat(QImage::Format_Grayscale8);
  };
  const QImage g = grey(roughness);
  const QImage b = grey(metalness);
  QImage combined(size, QImage::Format_RGB32);
  for (int y = 0; y < size.height(); ++y) {
    const uchar* green = g.isNull() ? nullptr : g.constScanLine(y);
    const uchar* blue = b.isNull() ? nullptr : b.constScanLine(y);
    auto* row = reinterpret_cast<QRgb*>(combined.scanLine(y));
    for (int x = 0; x < size.width(); ++x) {
      row[x] = qRgb(255, green != nullptr ? green[x] : 255, blue != nullptr ? blue[x] : 255);
    }
  }
  out.bytes = pngOf(combined);
  out.mime = QStringLiteral("image/png");
  return !out.bytes.isEmpty();
}

// How a material's images lie on a body (render::MaterialData): planar
// along Z, else box, turned and scaled; as the rendered view projects them.
struct Projection {
  bool planar = false;
  double width = 100.0;
  double height = 100.0;
  double rotation = 0.0;
};

// Texture coordinates of a point (mm, the body's own coordinates) on a
// triangle facing `normal`: planar takes (x, y); box (y, z) where it faces
// X most, (x, z) where it faces Y, (x, y) where it faces Z; turned back by
// the rotation and divided by the size of one repeat. glTF's v grows down
// the image, so it is negated.
std::array<float, 2> project(const Projection& p, const float* point, const std::array<double, 3>& normal) {
  const double nx = std::abs(normal[0]);
  const double ny = std::abs(normal[1]);
  const double nz = std::abs(normal[2]);
  double a = point[0];
  double b = point[1];
  if (!p.planar) {
    if (nx >= std::max(ny, nz)) {
      a = point[1];
      b = point[2];
    } else if (ny >= nz) {
      a = point[0];
      b = point[2];
    }
  }
  const double sine = std::sin(p.rotation);
  const double cosine = std::cos(p.rotation);
  const double u = (a * cosine + b * sine) / p.width;
  const double v = (-a * sine + b * cosine) / p.height;
  return {static_cast<float>(u), static_cast<float>(-v)};
}

class GlbWriter {
public:
  explicit GlbWriter(double scale) : m_scale(scale) {}

  // A material's index (equal materials share one).
  int material(const render::MaterialData& m);
  // The glTF mesh of a body's mesh with its body's material and its faces'.
  int mesh(const render::MeshData& data, int bodyMaterial,
           const std::vector<std::pair<int, const render::FaceMaterial*>>& faces);
  void node(const QJsonObject& entry) {
    m_sceneNodes.append(static_cast<int>(m_nodes.size()));
    m_nodes.append(entry);
  }
  QByteArray finish();

  QStringList warnings;

private:
  int view(const void* data, std::size_t bytes, int target);
  int vec3(const std::vector<float>& values, bool bounds);
  int vec2(const std::vector<float>& values);
  int indices(const std::vector<std::uint32_t>& values);
  // A texture of an image made once per key; -1 (and a warning) when it
  // cannot be made.
  int texture(const QString& key, const QString& what, const std::function<bool(EncodedImage&, QString&)>& make);

  double m_scale;
  std::vector<char> m_binary;
  QJsonArray m_views;
  QJsonArray m_accessors;
  QJsonArray m_meshes;
  QJsonArray m_materials;
  QJsonArray m_images;
  QJsonArray m_textures;
  QJsonArray m_nodes;
  QJsonArray m_sceneNodes;
  QHash<QString, int> m_materialIndex;
  QHash<QString, int> m_textureIndex;
  std::vector<bool> m_textured;          // per material: it has an image
  std::vector<Projection> m_projections; // per material
};

int GlbWriter::view(const void* data, std::size_t bytes, int target) {
  const auto offset = static_cast<qint64>(m_binary.size());
  const auto* begin = static_cast<const char*>(data);
  m_binary.insert(m_binary.end(), begin, begin + bytes);
  while (m_binary.size() % 4 != 0) {
    m_binary.push_back(0);
  }
  QJsonObject entry{{QStringLiteral("buffer"), 0},
                    {QStringLiteral("byteOffset"), offset},
                    {QStringLiteral("byteLength"), static_cast<qint64>(bytes)}};
  if (target != 0) {
    entry.insert(QStringLiteral("target"), target);
  }
  m_views.append(entry);
  return static_cast<int>(m_views.size() - 1);
}

int GlbWriter::vec3(const std::vector<float>& values, bool bounds) {
  const int at = view(values.data(), values.size() * sizeof(float), kArrayBuffer);
  QJsonObject accessor{{QStringLiteral("bufferView"), at},
                       {QStringLiteral("componentType"), kFloat},
                       {QStringLiteral("count"), static_cast<qint64>(values.size() / 3)},
                       {QStringLiteral("type"), QStringLiteral("VEC3")}};
  if (bounds) {
    std::array<float, 3> low{std::numeric_limits<float>::max(), std::numeric_limits<float>::max(),
                             std::numeric_limits<float>::max()};
    std::array<float, 3> high{-low[0], -low[1], -low[2]};
    for (std::size_t i = 0; i < values.size(); ++i) {
      low[i % 3] = std::min(low[i % 3], values[i]);
      high[i % 3] = std::max(high[i % 3], values[i]);
    }
    accessor.insert(QStringLiteral("min"), numbers({low[0], low[1], low[2]}));
    accessor.insert(QStringLiteral("max"), numbers({high[0], high[1], high[2]}));
  }
  m_accessors.append(accessor);
  return static_cast<int>(m_accessors.size() - 1);
}

int GlbWriter::vec2(const std::vector<float>& values) {
  const int at = view(values.data(), values.size() * sizeof(float), kArrayBuffer);
  m_accessors.append(QJsonObject{{QStringLiteral("bufferView"), at},
                                 {QStringLiteral("componentType"), kFloat},
                                 {QStringLiteral("count"), static_cast<qint64>(values.size() / 2)},
                                 {QStringLiteral("type"), QStringLiteral("VEC2")}});
  return static_cast<int>(m_accessors.size() - 1);
}

int GlbWriter::indices(const std::vector<std::uint32_t>& values) {
  const int at = view(values.data(), values.size() * sizeof(std::uint32_t), kElementArrayBuffer);
  m_accessors.append(QJsonObject{{QStringLiteral("bufferView"), at},
                                 {QStringLiteral("componentType"), kUnsignedInt},
                                 {QStringLiteral("count"), static_cast<qint64>(values.size())},
                                 {QStringLiteral("type"), QStringLiteral("SCALAR")}});
  return static_cast<int>(m_accessors.size() - 1);
}

int GlbWriter::texture(const QString& key, const QString& what,
                       const std::function<bool(EncodedImage&, QString&)>& make) {
  const auto known = m_textureIndex.constFind(key);
  if (known != m_textureIndex.cend()) {
    return known.value();
  }
  EncodedImage image;
  QString why;
  int index = -1;
  if (make(image, why) && !image.bytes.isEmpty()) {
    const int at = view(image.bytes.constData(), static_cast<std::size_t>(image.bytes.size()), 0);
    m_images.append(QJsonObject{{QStringLiteral("bufferView"), at}, {QStringLiteral("mimeType"), image.mime}});
    m_textures.append(QJsonObject{{QStringLiteral("sampler"), 0},
                                  {QStringLiteral("source"), static_cast<int>(m_images.size() - 1)}});
    index = static_cast<int>(m_textures.size() - 1);
  } else {
    warnings << QObject::tr("the texture image %1 cannot be read (%2); it is left out").arg(what, why);
  }
  m_textureIndex.insert(key, index);
  return index;
}

int GlbWriter::material(const render::MaterialData& m) {
  QJsonObject pbr;
  bool textured = false;
  // Glass stays faintly visible: the materials have no transmission.
  const double alpha =
      std::clamp(static_cast<double>(m.opacity) * (1.0 - 0.9 * static_cast<double>(m.transmission)), 0.0, 1.0);
  std::array<double, 3> base{m.baseColor[0], m.baseColor[1], m.baseColor[2]};
  if (!m.texture.empty()) {
    const QString file = text(m.texture);
    const int color = texture(QStringLiteral("base|") + file, file,
                              [&file](EncodedImage& image, QString& why) { return encodedImage(file, image, why); });
    if (color >= 0) {
      // The image gives the base colour.
      base = {1.0, 1.0, 1.0};
      pbr.insert(QStringLiteral("baseColorTexture"), QJsonObject{{QStringLiteral("index"), color}});
      textured = true;
    }
  }
  pbr.insert(QStringLiteral("baseColorFactor"), numbers({base[0], base[1], base[2], alpha}));
  double metallic = m.metallic;
  double roughness = m.roughness;
  if (!m.roughnessMap.empty() || !m.metalnessMap.empty()) {
    const QString rough = text(m.roughnessMap);
    const QString metal = text(m.metalnessMap);
    const int combined = texture(
        QStringLiteral("metallic-roughness|%1|%2").arg(rough, metal), rough.isEmpty() ? metal : rough,
        [&rough, &metal](EncodedImage& image, QString& why) { return metallicRoughnessImage(rough, metal, image, why); });
    if (combined >= 0) {
      // The maps replace the values.
      roughness = rough.isEmpty() ? roughness : 1.0;
      metallic = metal.isEmpty() ? metallic : 1.0;
      pbr.insert(QStringLiteral("metallicRoughnessTexture"), QJsonObject{{QStringLiteral("index"), combined}});
      textured = true;
    }
  }
  pbr.insert(QStringLiteral("metallicFactor"), std::clamp(metallic, 0.0, 1.0));
  pbr.insert(QStringLiteral("roughnessFactor"), std::clamp(roughness, 0.0, 1.0));
  QJsonObject entry{{QStringLiteral("pbrMetallicRoughness"), pbr}, {QStringLiteral("doubleSided"), true}};
  const auto map = [&](const std::string& path, const char* kind, const char* member) {
    if (path.empty()) {
      return;
    }
    const QString file = text(path);
    const int index = texture(QString::fromLatin1(kind) + QLatin1Char('|') + file, file,
                              [&file](EncodedImage& image, QString& why) { return encodedImage(file, image, why); });
    if (index >= 0) {
      entry.insert(QLatin1String(member), QJsonObject{{QStringLiteral("index"), index}});
      textured = true;
    }
  };
  map(m.normalMap, "normal", "normalTexture");
  map(m.occlusionMap, "occlusion", "occlusionTexture");
  if (m.emission > 0.0f) {
    const double strength = std::min(1.0, static_cast<double>(m.emission));
    entry.insert(QStringLiteral("emissiveFactor"),
                 numbers({std::clamp(m.emissionColor[0] * strength, 0.0, 1.0),
                          std::clamp(m.emissionColor[1] * strength, 0.0, 1.0),
                          std::clamp(m.emissionColor[2] * strength, 0.0, 1.0)}));
  }
  if (alpha < 1.0) {
    entry.insert(QStringLiteral("alphaMode"), QStringLiteral("BLEND"));
  }
  Projection projection;
  if (textured) {
    projection.planar = m.texturePlanar;
    projection.width = m.textureSize[0] > 0.0f ? static_cast<double>(m.textureSize[0]) : 100.0;
    projection.height = m.textureSize[1] > 0.0f ? static_cast<double>(m.textureSize[1]) : 100.0;
    projection.rotation = m.textureRotation;
  }
  const QString key = QString::fromUtf8(QJsonDocument(entry).toJson(QJsonDocument::Compact)) +
                      (textured ? QStringLiteral("|%1|%2|%3|%4")
                                      .arg(projection.planar ? 1 : 0)
                                      .arg(projection.width)
                                      .arg(projection.height)
                                      .arg(projection.rotation)
                                : QString());
  const auto known = m_materialIndex.constFind(key);
  if (known != m_materialIndex.cend()) {
    return known.value();
  }
  m_materials.append(entry);
  m_textured.push_back(textured);
  m_projections.push_back(projection);
  const auto index = static_cast<int>(m_materials.size() - 1);
  m_materialIndex.insert(key, index);
  return index;
}

int GlbWriter::mesh(const render::MeshData& data, int bodyMaterial,
                    const std::vector<std::pair<int, const render::FaceMaterial*>>& faces) {
  const std::size_t triangles = data.indices.size() / 3;
  const std::size_t vertices = data.positions.size() / 3;
  // Each triangle's material: the body's, or its face's.
  std::vector<int> materialOf(triangles, bodyMaterial);
  if (!data.faceTriangles.empty()) {
    std::vector<std::size_t> start(data.faceTriangles.size() + 1, 0);
    for (std::size_t f = 0; f < data.faceTriangles.size(); ++f) {
      start[f + 1] = start[f] + data.faceTriangles[f];
    }
    for (const auto& [material, group] : faces) {
      for (const std::uint32_t face : group->faces) {
        if (face >= data.faceTriangles.size()) {
          continue;
        }
        for (std::size_t t = start[face]; t < start[face + 1] && t < triangles; ++t) {
          materialOf[t] = material;
        }
      }
    }
  }
  std::vector<int> used;
  for (const int material : materialOf) {
    if (std::find(used.begin(), used.end(), material) == used.end()) {
      used.push_back(material);
    }
  }
  const bool hasNormals = data.normals.size() == data.positions.size();
  int positions = -1;
  int normals = -1;
  QJsonArray primitives;
  for (const int material : used) {
    QJsonObject attributes;
    QJsonObject primitive;
    if (!m_textured[static_cast<std::size_t>(material)]) {
      // The mesh's vertices, shared by its primitives without images.
      if (positions < 0) {
        std::vector<float> scaled(data.positions);
        for (float& value : scaled) {
          value = static_cast<float>(value * m_scale);
        }
        positions = vec3(scaled, true);
        normals = hasNormals ? vec3(data.normals, false) : -1;
      }
      std::vector<std::uint32_t> chosen;
      for (std::size_t t = 0; t < triangles; ++t) {
        if (materialOf[t] == material) {
          chosen.insert(chosen.end(), data.indices.begin() + static_cast<std::ptrdiff_t>(t * 3),
                        data.indices.begin() + static_cast<std::ptrdiff_t>(t * 3 + 3));
        }
      }
      attributes.insert(QStringLiteral("POSITION"), positions);
      if (normals >= 0) {
        attributes.insert(QStringLiteral("NORMAL"), normals);
      }
      primitive.insert(QStringLiteral("indices"), indices(chosen));
    } else {
      // With an image: each triangle's own corners, with texture
      // coordinates by the triangle's facing (a box projection changes
      // between neighbouring triangles).
      const Projection& projection = m_projections[static_cast<std::size_t>(material)];
      std::vector<float> points;
      std::vector<float> directions;
      std::vector<float> uv;
      for (std::size_t t = 0; t < triangles; ++t) {
        if (materialOf[t] != material) {
          continue;
        }
        const std::uint32_t* corner = &data.indices[t * 3];
        if (corner[0] >= vertices || corner[1] >= vertices || corner[2] >= vertices) {
          continue;
        }
        const float* a = &data.positions[static_cast<std::size_t>(corner[0]) * 3];
        const float* b = &data.positions[static_cast<std::size_t>(corner[1]) * 3];
        const float* c = &data.positions[static_cast<std::size_t>(corner[2]) * 3];
        const std::array<double, 3> ab{double(b[0]) - a[0], double(b[1]) - a[1], double(b[2]) - a[2]};
        const std::array<double, 3> ac{double(c[0]) - a[0], double(c[1]) - a[1], double(c[2]) - a[2]};
        const std::array<double, 3> facing{ab[1] * ac[2] - ab[2] * ac[1], ab[2] * ac[0] - ab[0] * ac[2],
                                           ab[0] * ac[1] - ab[1] * ac[0]};
        for (int k = 0; k < 3; ++k) {
          const std::size_t at = static_cast<std::size_t>(corner[k]) * 3;
          const float* p = &data.positions[at];
          points.insert(points.end(), {static_cast<float>(p[0] * m_scale), static_cast<float>(p[1] * m_scale),
                                       static_cast<float>(p[2] * m_scale)});
          if (hasNormals) {
            directions.insert(directions.end(), {data.normals[at], data.normals[at + 1], data.normals[at + 2]});
          }
          const std::array<float, 2> st = project(projection, p, facing);
          uv.insert(uv.end(), {st[0], st[1]});
        }
      }
      if (points.empty()) {
        continue;
      }
      attributes.insert(QStringLiteral("POSITION"), vec3(points, true));
      if (hasNormals) {
        attributes.insert(QStringLiteral("NORMAL"), vec3(directions, false));
      }
      attributes.insert(QStringLiteral("TEXCOORD_0"), vec2(uv));
    }
    primitive.insert(QStringLiteral("attributes"), attributes);
    primitive.insert(QStringLiteral("material"), material);
    primitives.append(primitive);
  }
  if (primitives.isEmpty()) {
    return -1;
  }
  m_meshes.append(QJsonObject{{QStringLiteral("primitives"), primitives}});
  return static_cast<int>(m_meshes.size() - 1);
}

void appendUint32(QByteArray& out, std::uint32_t value) {
  // glb is little-endian, as every platform MitCad builds for.
  char bytes[4];
  std::memcpy(bytes, &value, sizeof value);
  out.append(bytes, 4);
}

QByteArray GlbWriter::finish() {
  QJsonObject scene;
  if (!m_sceneNodes.isEmpty()) {
    scene.insert(QStringLiteral("nodes"), m_sceneNodes);
  }
  QJsonObject root{
      {QStringLiteral("asset"),
       QJsonObject{{QStringLiteral("version"), QStringLiteral("2.0")}, {QStringLiteral("generator"), QStringLiteral("MitCad")}}},
      {QStringLiteral("scene"), 0},
      {QStringLiteral("scenes"), QJsonArray{scene}}};
  // glTF's arrays have at least one item when they are there.
  const auto add = [&root](const char* name, const QJsonArray& array) {
    if (!array.isEmpty()) {
      root.insert(QLatin1String(name), array);
    }
  };
  add("nodes", m_nodes);
  add("meshes", m_meshes);
  add("materials", m_materials);
  add("accessors", m_accessors);
  add("bufferViews", m_views);
  add("images", m_images);
  add("textures", m_textures);
  if (!m_textures.isEmpty()) {
    root.insert(QStringLiteral("samplers"), QJsonArray{QJsonObject{{QStringLiteral("magFilter"), kLinear},
                                                                    {QStringLiteral("minFilter"), kLinearMipmapLinear},
                                                                    {QStringLiteral("wrapS"), kRepeat},
                                                                    {QStringLiteral("wrapT"), kRepeat}}});
  }
  if (!m_binary.empty()) {
    root.insert(QStringLiteral("buffers"),
                QJsonArray{QJsonObject{{QStringLiteral("byteLength"), static_cast<qint64>(m_binary.size())}}});
  }
  QByteArray json = QJsonDocument(root).toJson(QJsonDocument::Compact);
  while (json.size() % 4 != 0) {
    json.append(' ');
  }
  const std::size_t total = 12 + 8 + static_cast<std::size_t>(json.size()) + (m_binary.empty() ? 0 : 8 + m_binary.size());
  QByteArray out;
  out.reserve(static_cast<qsizetype>(total));
  appendUint32(out, 0x46546c67); // "glTF"
  appendUint32(out, 2);
  appendUint32(out, static_cast<std::uint32_t>(total));
  appendUint32(out, static_cast<std::uint32_t>(json.size()));
  appendUint32(out, 0x4e4f534a); // "JSON"
  out.append(json);
  if (!m_binary.empty()) {
    appendUint32(out, static_cast<std::uint32_t>(m_binary.size()));
    appendUint32(out, 0x004e4942); // "BIN"
    out.append(m_binary.data(), static_cast<qsizetype>(m_binary.size()));
  }
  return out;
}

bool isIdentity(const std::array<double, 12>& m) {
  static const std::array<double, 12> identity{1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0};
  for (std::size_t i = 0; i < m.size(); ++i) {
    if (std::abs(m[i] - identity[i]) > 1e-12) {
      return false;
    }
  }
  return true;
}

// The occurrences' items with their nodes (pageTree); `placed` collects
// the paths that have an item.
QJsonArray occurrenceItems(const QJsonArray& occurrences, const QString& prefix,
                           const QHash<QString, QStringList>& nodesOf, QSet<QString>& placed, int depth) {
  QJsonArray items;
  if (depth > 64) {
    return items; // a guard against cycles in loaded data
  }
  for (const QJsonValue& value : occurrences) {
    const QJsonObject occurrence = value.toObject();
    const QString uid = occurrence.value(QStringLiteral("uid")).toString();
    const QString path = prefix.isEmpty() ? uid : prefix + QLatin1Char('/') + uid;
    const QJsonArray children = occurrenceItems(occurrence.value(QStringLiteral("children")).toArray(), path, nodesOf,
                                                placed, depth + 1);
    const QStringList own = nodesOf.value(path);
    if (own.isEmpty() && children.isEmpty()) {
      continue;
    }
    placed.insert(path);
    items.append(QJsonObject{{QStringLiteral("id"), path},
                             {QStringLiteral("name"), occurrence.value(QStringLiteral("name")).toString()},
                             {QStringLiteral("nodes"), QJsonArray::fromStringList(own)},
                             {QStringLiteral("children"), children}});
  }
  return items;
}

} // namespace

PageGlb pageGlb(const std::vector<PageBody>& bodies, double scale) {
  PageGlb result;
  GlbWriter writer(scale);
  QHash<QString, int> meshes; // by mesh and look
  int count = 0;
  for (const PageBody& body : bodies) {
    if (!body.mesh || body.mesh->indices.empty()) {
      result.nodes << QString();
      continue;
    }
    const int material = writer.material(body.material);
    std::vector<std::pair<int, const render::FaceMaterial*>> faces;
    QString key = QStringLiteral("%1|%2").arg(reinterpret_cast<quintptr>(body.mesh.get())).arg(material);
    for (const render::FaceMaterial& group : body.faces) {
      const int own = writer.material(group.material);
      faces.emplace_back(own, &group);
      key += QStringLiteral("|%1:").arg(own);
      for (const std::uint32_t face : group.faces) {
        key += QString::number(face) + QLatin1Char(',');
      }
    }
    int mesh = meshes.value(key, -2);
    if (mesh == -2) {
      mesh = writer.mesh(*body.mesh, material, faces);
      meshes.insert(key, mesh);
    }
    if (mesh < 0) {
      result.nodes << QString();
      continue;
    }
    const QString name = QStringLiteral("n%1").arg(++count);
    QJsonObject node{{QStringLiteral("name"), name},
                     {QStringLiteral("mesh"), mesh},
                     {QStringLiteral("extras"), QJsonObject{{QStringLiteral("body"), body.uid},
                                                            {QStringLiteral("body_name"), body.name},
                                                            {QStringLiteral("occurrence"), body.occurrence}}}};
    if (!isIdentity(body.transform)) {
      // Column-major, the translation in model units.
      const std::array<double, 12>& m = body.transform;
      node.insert(QStringLiteral("matrix"),
                  numbers({m[0], m[4], m[8], 0.0, m[1], m[5], m[9], 0.0, m[2], m[6], m[10], 0.0, m[3] * scale,
                           m[7] * scale, m[11] * scale, 1.0}));
    }
    writer.node(node);
    result.nodes << name;
  }
  result.glb = writer.finish();
  result.warnings = writer.warnings;
  return result;
}

QJsonArray pageTree(const QString& rootName, const QJsonArray& occurrences, const std::vector<PageBody>& bodies,
                    const QStringList& nodes) {
  QHash<QString, QStringList> nodesOf;
  for (std::size_t i = 0; i < bodies.size() && i < static_cast<std::size_t>(nodes.size()); ++i) {
    const QString& node = nodes.at(static_cast<qsizetype>(i));
    if (!node.isEmpty()) {
      nodesOf[bodies[i].occurrence] << node;
    }
  }
  QSet<QString> placed;
  const QJsonArray children = occurrenceItems(occurrences, QString(), nodesOf, placed, 0);
  // The root's own bodies, and any whose occurrence the tree does not have.
  QStringList own = nodesOf.value(QString());
  for (auto it = nodesOf.cbegin(); it != nodesOf.cend(); ++it) {
    if (!it.key().isEmpty() && !placed.contains(it.key())) {
      own += it.value();
    }
  }
  return QJsonArray{QJsonObject{{QStringLiteral("id"), QStringLiteral("root")},
                                {QStringLiteral("name"), rootName},
                                {QStringLiteral("nodes"), QJsonArray::fromStringList(own)},
                                {QStringLiteral("children"), children}}};
}

} // namespace mitcad::web
