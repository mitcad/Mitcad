// SPDX-License-Identifier: MIT
// The bodies of the web page and of File › Export's glTF types
// (web/WebPage.hpp, pageBodies): the visible bodies as placed, each meshed
// once on a copy of its shape, with their appearances. Apart from the page
// itself so that `mitcad-cli export` (mitcad#2210) builds it with the glTF
// writer and without the page's viewer and result store.
#include "web/WebPage.hpp"

#include <array>
#include <cstddef>
#include <cstdint>
#include <exception>
#include <map>
#include <memory>
#include <utility>
#include <vector>

#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QPair>
#include <QSet>
#include <QVector>

#include <Standard_Failure.hxx>

#include "framework/Appearances.hpp"
#include "framework/Command.hpp"
#include "mitcad/io/mesh.hpp"
#include "render/RenderBatch.hpp"
#include "render/RenderMesh.hpp"
#include "web/WebGlb.hpp"

namespace mitcad::web {

namespace {

// A doubly curved face gets about this many triangles at most (as the
// export's Medium refinement, io::MeshOptions): a large dome meshes a
// little coarser rather than taking millions of triangles.
constexpr std::size_t kFaceTriangles = 20000;

// The `instances` query's path: an array of occurrence uids or a text.
QString pathText(const QJsonValue& value) {
  if (value.isString()) {
    return value.toString();
  }
  QStringList uids;
  for (const QJsonValue& uid : value.toArray()) {
    uids << uid.toString();
  }
  return uids.join(QLatin1Char('/'));
}

// A placement of the model's queries (3 or 4 rows of 4 numbers) as a
// row-major 3 x 4 matrix.
std::array<double, 12> transformOf(const QJsonArray& rows) {
  std::array<double, 12> m{1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0};
  if (rows.size() < 3) {
    return m;
  }
  for (int r = 0; r < 3; ++r) {
    const QJsonArray row = rows.at(r).toArray();
    for (int c = 0; c < 4; ++c) {
      m[static_cast<std::size_t>(r * 4 + c)] = row.at(c).toDouble(m[static_cast<std::size_t>(r * 4 + c)]);
    }
  }
  return m;
}

} // namespace

std::vector<PageBody> pageBodies(const CommandContext& context, const PageOptions& options, QStringList& warnings) {
  // The appearances with their textures' image files (as the view and the
  // final render find them).
  QHash<QString, Appearance> known = appearancesById(appearancesOf(context.queryArray(QStringLiteral("appearances"))));
  const QJsonArray bodiesListed = context.queryArray(QStringLiteral("bodies"));
  // Only of the appearances in use: the library's woods and panels have
  // textures too (mitcad#2186).
  QSet<QString> used;
  for (const QJsonValue& value : bodiesListed) {
    const QJsonObject body = value.toObject();
    used.insert(body.value(QStringLiteral("appearance")).toString());
    for (const QJsonValue& face : body.value(QStringLiteral("face_appearances")).toArray()) {
      used.insert(face.toObject().value(QStringLiteral("appearance")).toString());
    }
  }
  for (Appearance& appearance : known) {
    if (appearance.hasTexture && used.contains(appearance.id)) {
      const QString id = appearance.id;
      resolveTextureMaps(appearance.texture, options.designFolder, [&context, id](const QString& map) {
        try {
          return QByteArray::fromBase64(context
                                            .queryObject({{QStringLiteral("query"), QStringLiteral("appearance_image")},
                                                          {QStringLiteral("id"), id},
                                                          {QStringLiteral("map"), map}})
                                            .value(QStringLiteral("data"))
                                            .toString()
                                            .toLatin1());
        } catch (const std::exception&) {
          return QByteArray();
        }
      });
    }
  }
  QHash<QString, QString> lookOf;
  QHash<QString, QString> nameOf;
  QHash<QString, QVector<QPair<QStringList, QString>>> faceAppearances;
  for (const QJsonValue& value : bodiesListed) {
    const QJsonObject body = value.toObject();
    const QString uid = body.value(QStringLiteral("uid")).toString();
    nameOf.insert(uid, body.value(QStringLiteral("name")).toString(uid));
    lookOf.insert(uid, body.value(QStringLiteral("appearance")).toString());
    for (const QJsonValue& face : body.value(QStringLiteral("face_appearances")).toArray()) {
      QStringList names;
      for (const QJsonValue& name : face.toObject().value(QStringLiteral("faces")).toArray()) {
        names << name.toString();
      }
      faceAppearances[uid].append({names, face.toObject().value(QStringLiteral("appearance")).toString()});
    }
  }
  const auto materialOf = [&known](const QString& id) {
    // An unknown id (an imported one) or none: the default look.
    const auto appearance = known.constFind(id);
    render::MaterialData material =
        render::materialOf(appearance != known.cend() ? appearance.value() : Appearance());
    // Named in the files that keep names (glTF, mitcad#1826).
    if (appearance != known.cend()) {
      material.name = appearance.value().name.toStdString();
    }
    return material;
  };

  // The visible bodies as placed; each body meshed once, on a copy of its
  // shape (io::triangulate), its faces in the shape's order.
  const io::MeshOptions meshing{options.linearDeflection, options.angularDeflection, false, kFaceTriangles,
                                false, false};
  std::map<QString, std::shared_ptr<const render::MeshData>> meshes;
  std::map<QString, std::vector<render::FaceMaterial>> faceMaterials;
  std::vector<PageBody> bodies;
  for (const QJsonValue& value : context.queryArray(QStringLiteral("instances"))) {
    const QJsonObject instance = value.toObject();
    const QString uid = instance.value(QStringLiteral("body")).toString();
    auto mesh = meshes.find(uid);
    if (mesh == meshes.end()) {
      std::shared_ptr<const render::MeshData> made;
      std::vector<render::FaceMaterial> faces;
      if (const std::shared_ptr<geometry::Shape> shape = context.bodyShape(uid)) {
        // A body that cannot be meshed is left out, and the page says so.
        try {
          made = std::make_shared<const render::MeshData>(render::meshOf(io::triangulate(shape->occt(), meshing)));
        } catch (const Standard_Failure& failure) {
          warnings << QObject::tr("%1 is left out: it cannot be meshed (%2)")
                          .arg(nameOf.value(uid, uid), QString::fromUtf8(failure.what()));
        } catch (const std::exception& e) {
          warnings << QObject::tr("%1 is left out: it cannot be meshed (%2)")
                          .arg(nameOf.value(uid, uid), QString::fromUtf8(e.what()));
        }
        // The faces with appearances of their own by their names, grouped
        // by appearance (a face named twice takes the later one).
        std::map<int, QString> byFace;
        for (const auto& [names, id] : faceAppearances.value(uid)) {
          for (const QString& name : names) {
            for (const int face : shape->find_faces(name.toStdString())) {
              byFace[face] = id;
            }
          }
        }
        std::map<QString, std::vector<std::uint32_t>> byAppearance;
        for (const auto& [face, id] : byFace) {
          if (face >= 0) {
            byAppearance[id].push_back(static_cast<std::uint32_t>(face));
          }
        }
        for (const auto& [id, list] : byAppearance) {
          render::FaceMaterial group;
          group.faces = list;
          group.material = materialOf(id);
          faces.push_back(std::move(group));
        }
      }
      faceMaterials[uid] = std::move(faces);
      mesh = meshes.emplace(uid, made).first;
    }
    if (!mesh->second || mesh->second->indices.empty()) {
      continue;
    }
    PageBody body;
    body.uid = uid;
    body.name = instance.value(QStringLiteral("name")).toString(nameOf.value(uid, uid));
    body.occurrence = pathText(instance.value(QStringLiteral("path")));
    body.mesh = mesh->second;
    body.transform = transformOf(instance.value(QStringLiteral("transform")).toArray());
    body.material = materialOf(lookOf.value(uid));
    body.faces = faceMaterials[uid];
    bodies.push_back(std::move(body));
  }
  // Textures of the page's bodies whose images are missing (the view shows
  // their base colour).
  std::vector<Appearance> looks;
  for (const auto& [uid, made] : meshes) {
    if (!made) {
      continue;
    }
    QStringList ids{lookOf.value(uid)};
    for (const QPair<QStringList, QString>& group : faceAppearances.value(uid)) {
      ids << group.second;
    }
    for (const QString& id : std::as_const(ids)) {
      if (known.contains(id)) {
        looks.push_back(known.value(id));
      }
    }
  }
  std::vector<const Appearance*> lookPointers;
  for (const Appearance& look : looks) {
    lookPointers.push_back(&look);
  }
  warnings += render::missingTextures(lookPointers);
  return bodies;
}

} // namespace mitcad::web
