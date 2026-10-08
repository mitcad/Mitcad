// SPDX-License-Identifier: MIT
// mitcad-cli render (mitcad#48, docs/rendering.md "Final render"): a
// design's visible bodies rendered to an image file without the user
// interface, for scripts. Built only with MITCAD_RENDER. The command line
// tool makes the scene and the job as File > Render Image does
// (app/render/RenderBatch) and runs the render worker mitcad-render in its
// batch mode, which writes the image; it shares the application's Qt Core
// and Gui for that, but not its windows.

#include "render.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <iostream>
#include <string>
#include <map>
#include <vector>

#include <QCoreApplication>
#include <QDir>
#include <QEventLoop>
#include <QFileInfo>
#include <QHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLoggingCategory>
#include <QTemporaryDir>

#include <BRepBndLib.hxx>
#include <Bnd_Box.hxx>
#include <TopLoc_Location.hxx>
#include <gp_Dir.hxx>

#include "framework/Appearances.hpp"
#include "mitcad_bridge/lib.h"
#include "render/RenderBatch.hpp"
#include "render/RenderDevice.hpp"
#include "render/RenderScene.hpp"
#include "rust/cxx.h"

namespace mitcad::cli {

namespace {

QJsonValue query(const mitcad::Document& document, const QString& name) {
  const std::string answer(document.query("{\"query\": \"" + name.toStdString() + "\"}"));
  const QJsonDocument parsed = QJsonDocument::fromJson(QByteArray::fromStdString(answer));
  return parsed.isArray() ? QJsonValue(parsed.array()) : QJsonValue(parsed.object());
}

// A placement of the model's queries: 3 or 4 rows of 4 numbers.
gp_Trsf trsfOf(const QJsonArray& rows) {
  const auto at = [&rows](int r, int c) { return rows[r].toArray()[c].toDouble(); };
  gp_Trsf trsf;
  if (rows.size() >= 3) {
    trsf.SetValues(at(0, 0), at(0, 1), at(0, 2), at(0, 3), at(1, 0), at(1, 1), at(1, 2), at(1, 3), at(2, 0),
                   at(2, 1), at(2, 2), at(2, 3));
  }
  return trsf;
}

// The standard views as the View menu has them: the direction from the eye
// to the design and up.
struct StandardView {
  const char* name;
  gp_Dir direction;
  gp_Dir up;
};
const StandardView kStandardViews[] = {
    {"front", gp_Dir(0, 1, 0), gp_Dir(0, 0, 1)},  {"back", gp_Dir(0, -1, 0), gp_Dir(0, 0, 1)},
    {"left", gp_Dir(1, 0, 0), gp_Dir(0, 0, 1)},   {"right", gp_Dir(-1, 0, 0), gp_Dir(0, 0, 1)},
    {"top", gp_Dir(0, 0, -1), gp_Dir(0, 1, 0)},   {"bottom", gp_Dir(0, 0, 1), gp_Dir(0, -1, 0)},
    {"iso", gp_Dir(-1, 1, -1), gp_Dir(0, 0, 1)},
};

// An orthographic camera along a standard view that fits the bodies (their
// bounding sphere) into an image of `aspect`.
render::Camera fittedCamera(const StandardView& view, const Bnd_Box& box, double aspect) {
  render::Camera camera;
  gp_Pnt center(0, 0, 0);
  double radius = 50.0;
  if (!box.IsVoid()) {
    center = gp_Pnt((box.CornerMin().XYZ() + box.CornerMax().XYZ()) / 2.0);
    radius = std::max(1e-3, std::sqrt(box.SquareExtent()) / 2.0);
  }
  const gp_XYZ eye = center.XYZ() - view.direction.XYZ() * (radius * 4.0);
  camera.eye = {eye.X(), eye.Y(), eye.Z()};
  camera.target = {center.X(), center.Y(), center.Z()};
  camera.up = {view.up.X(), view.up.Y(), view.up.Z()};
  camera.perspective = false;
  camera.aspect = aspect;
  // A little margin; a portrait image fits the width.
  camera.halfHeight = radius * 1.05 * (aspect < 1.0 ? 1.0 / aspect : 1.0);
  return camera;
}

const char* const kUsage = R"(usage: mitcad-cli render <file.mitcad|script.json> -o <image> [--view <name>]
         [--size WxH] [--samples N] [--time-limit SECONDS] [--format png|png16|jpeg|exr]
         [--quality 1-100] [--transparent] [--no-denoise] [--device auto|cpu|cuda|<id>]
         [--worker <mitcad-render>]
       mitcad-cli render --list-devices [--worker <mitcad-render>])";

int usage(const std::string& problem) {
  std::cerr << "mitcad-cli: " << problem << "\n" << kUsage << "\n";
  return 2;
}

} // namespace

int render(int argc, char* argv[], const OpenPart& open) {
  std::string input;
  std::string output;
  QString viewName;
  QJsonObject change; // of the output section
  QString worker;
  QString device = QStringLiteral("auto"); // the render device (mitcad#50)
  bool listDevices = false;
  for (int i = 2; i < argc; ++i) {
    const std::string arg = argv[i];
    const bool more = i + 1 < argc;
    if ((arg == "-o" || arg == "--output") && more) {
      output = argv[++i];
    } else if (arg == "--view" && more) {
      viewName = QString::fromLocal8Bit(argv[++i]);
    } else if (arg == "--size" && more) {
      const QStringList parts = QString::fromLatin1(argv[++i]).split(QLatin1Char('x'));
      bool widthOk = false;
      bool heightOk = false;
      const int width = parts.value(0).toInt(&widthOk);
      const int height = parts.value(1).toInt(&heightOk);
      if (parts.size() != 2 || !widthOk || !heightOk) {
        return usage("--size needs WIDTHxHEIGHT in pixels, such as 1920x1080");
      }
      change.insert(QStringLiteral("width"), width);
      change.insert(QStringLiteral("height"), height);
      change.insert(QStringLiteral("aspect"), QStringLiteral("fixed"));
    } else if (arg == "--samples" && more) {
      bool ok = false;
      change.insert(QStringLiteral("samples"), QString::fromLatin1(argv[++i]).toInt(&ok));
      if (!ok) {
        return usage("--samples needs a number");
      }
    } else if (arg == "--time-limit" && more) {
      bool ok = false;
      change.insert(QStringLiteral("time_limit"), QString::fromLatin1(argv[++i]).toDouble(&ok));
      if (!ok) {
        return usage("--time-limit needs seconds");
      }
    } else if (arg == "--quality" && more) {
      bool ok = false;
      change.insert(QStringLiteral("quality"), QString::fromLatin1(argv[++i]).toInt(&ok));
      if (!ok) {
        return usage("--quality needs a number");
      }
    } else if (arg == "--format" && more) {
      change.insert(QStringLiteral("format"), QString::fromLatin1(argv[++i]));
    } else if (arg == "--transparent") {
      change.insert(QStringLiteral("transparent"), true);
    } else if (arg == "--no-denoise") {
      change.insert(QStringLiteral("denoise"), false);
    } else if (arg == "--worker" && more) {
      worker = QString::fromLocal8Bit(argv[++i]);
    } else if (arg == "--device" && more) {
      device = QString::fromLocal8Bit(argv[++i]);
    } else if (arg == "--list-devices") {
      listDevices = true;
    } else if (input.empty() && !arg.empty() && arg[0] != '-') {
      input = arg;
    } else {
      return usage("unexpected argument '" + arg + "'");
    }
  }
  if (listDevices) {
    // The worker's devices (mitcad#50): what --device takes.
    QCoreApplication app(argc, argv);
    if (worker.isEmpty()) {
      worker = render::workerExecutable();
    }
    QString error;
    const QList<render::RenderDevice> devices = render::renderDevices(worker, error);
    if (devices.isEmpty()) {
      std::cerr << "mitcad-cli: no render devices: " << error.toStdString() << "\n";
      return 1;
    }
    for (const render::RenderDevice& entry : devices) {
      std::printf("%s\t%s\t%s%s\n", entry.type.toUtf8().constData(), entry.id.toUtf8().constData(),
                  entry.name.toUtf8().constData(), entry.denoisesOnDevice ? " (denoises on the device)" : "");
    }
    return 0;
  }
  if (input.empty() || output.empty()) {
    return usage(input.empty() ? "no design to render" : "no image file (-o)");
  }
  // The format from the file's extension unless given.
  const QString outputPath = QFileInfo(QString::fromLocal8Bit(output.c_str())).absoluteFilePath();
  const QString suffix = QFileInfo(outputPath).suffix().toLower();
  if (!change.contains(QStringLiteral("format"))) {
    if (suffix == QLatin1String("jpg") || suffix == QLatin1String("jpeg")) {
      change.insert(QStringLiteral("format"), QStringLiteral("jpeg"));
    } else if (suffix == QLatin1String("exr")) {
      change.insert(QStringLiteral("format"), QStringLiteral("exr"));
    } else if (suffix != QLatin1String("png")) {
      return usage("the image's format: --format, or a file ending in .png, .jpg or .exr");
    }
  }

  QCoreApplication app(argc, argv);
  // The worker's own messages (Cycles' log) are the application's debug
  // log; here only warnings and the progress lines.
  QLoggingCategory::setFilterRules(QStringLiteral("*.debug=false"));
  if (worker.isEmpty()) {
    worker = render::workerExecutable();
  }
  if (worker.isEmpty()) {
    std::cerr << "mitcad-cli: no render worker: mitcad-render is not next to mitcad-cli (or give --worker)\n";
    return 1;
  }

  rust::Box<mitcad::Document> document = open(input);
  // The options through the model, which checks them; the file is not
  // saved.
  if (!change.isEmpty()) {
    document->command(QJsonDocument(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("set_render_settings")},
                                                {QStringLiteral("output"), change}})
                          .toJson(QJsonDocument::Compact)
                          .toStdString());
  }
  // The format of a .png stays the settings' (8 or 16 bits).
  QJsonObject settings = query(*document, QStringLiteral("render_settings")).toObject();
  const QJsonObject outputSettings = settings.value(QStringLiteral("output")).toObject();
  const QString format = outputSettings.value(QStringLiteral("format")).toString();
  if (suffix == QLatin1String("png") && format != QLatin1String("png") && format != QLatin1String("png16")) {
    return usage("a .png file needs --format png or png16");
  }
  // Without a view, the image's aspect is width x height.
  const QSize size = render::outputSize(outputSettings, -1.0);
  const double aspect = static_cast<double>(size.width()) / size.height();

  // The visible bodies as placed, with their appearances and their faces'
  // (mitcad#53); textures with their image files.
  const QString folder = QFileInfo(QString::fromLocal8Bit(input.c_str())).absolutePath();
  QHash<QString, Appearance> known =
      appearancesById(appearancesOf(query(*document, QStringLiteral("appearances")).toArray()));
  for (Appearance& appearance : known) {
    if (appearance.hasTexture) {
      const QString id = appearance.id;
      resolveTexture(appearance.texture, folder, [&document, &id] {
        const QJsonObject request{{QStringLiteral("query"), QStringLiteral("appearance_image")},
                                  {QStringLiteral("id"), id}};
        const std::string answer(
            document->query(QJsonDocument(request).toJson(QJsonDocument::Compact).toStdString()));
        return QByteArray::fromBase64(QJsonDocument::fromJson(QByteArray::fromStdString(answer))
                                          .object()
                                          .value(QStringLiteral("data"))
                                          .toString()
                                          .toLatin1());
      });
    }
  }
  QHash<QString, Appearance> looks;
  QHash<QString, QVector<QPair<QStringList, QString>>> faceAppearances;
  for (const QJsonValue& value : query(*document, QStringLiteral("bodies")).toArray()) {
    const QJsonObject body = value.toObject();
    const QString uid = body.value(QStringLiteral("uid")).toString();
    const auto appearance = known.constFind(body.value(QStringLiteral("appearance")).toString());
    if (appearance != known.cend()) {
      looks.insert(uid, appearance.value());
    }
    for (const QJsonValue& face : body.value(QStringLiteral("face_appearances")).toArray()) {
      QStringList names;
      for (const QJsonValue& name : face.toObject().value(QStringLiteral("faces")).toArray()) {
        names << name.toString();
      }
      faceAppearances[uid].append({names, face.toObject().value(QStringLiteral("appearance")).toString()});
    }
  }
  std::vector<render::SceneBody> bodies;
  Bnd_Box box;
  for (const QJsonValue& value : query(*document, QStringLiteral("instances")).toArray()) {
    const QJsonObject instance = value.toObject();
    const QString uid = instance.value(QStringLiteral("body")).toString();
    const auto shape = document->body_shape(uid.toStdString());
    if (!shape) {
      continue;
    }
    render::SceneBody body;
    const QString occurrence = instance.value(QStringLiteral("occurrence")).toString();
    body.name = (occurrence.isEmpty() ? uid : uid + QLatin1Char('@') + occurrence).toStdString();
    body.shape = shape->occt();
    body.placement = trsfOf(instance.value(QStringLiteral("transform")).toArray());
    body.appearance = looks.value(uid);
    // The faces by their names, grouped by appearance (a face named twice
    // takes the later one).
    std::map<int, QString> byFace;
    for (const auto& [names, id] : faceAppearances.value(uid)) {
      for (const QString& name : names) {
        for (const int face : shape->find_faces(name.toStdString())) {
          byFace[face] = id;
        }
      }
    }
    std::map<QString, std::vector<int>> byAppearance;
    for (const auto& [face, id] : byFace) {
      byAppearance[id].push_back(face);
    }
    for (const auto& [id, faces] : byAppearance) {
      body.faces.push_back({faces, known.value(id)});
    }
    BRepBndLib::Add(body.shape.Moved(TopLoc_Location(body.placement)), box);
    bodies.push_back(std::move(body));
  }
  // Textures whose images are missing show their base colour.
  std::vector<const Appearance*> shownLooks;
  for (const render::SceneBody& body : bodies) {
    shownLooks.push_back(&body.appearance);
    for (const render::SceneFaces& group : body.faces) {
      shownLooks.push_back(&group.appearance);
    }
  }
  for (const QString& line : render::missingTextures(shownLooks)) {
    std::cerr << "mitcad-cli: a texture is not drawn, its base colour is shown instead: " << line.toStdString()
              << "\n";
  }

  // The camera: a named view, a standard view fitted to the bodies, or the
  // design's Home view, else the isometric one.
  render::Camera camera;
  bool found = false;
  const QJsonArray named = query(*document, QStringLiteral("named_views")).toArray();
  const QString wanted = viewName.isEmpty() ? QStringLiteral("Home") : viewName;
  for (const QJsonValue& view : named) {
    if (view.toObject().value(QStringLiteral("name")).toString() == wanted) {
      camera = render::namedViewCamera(view.toObject(), aspect);
      found = true;
    }
  }
  const QString standard = viewName.isEmpty() || viewName.compare(QLatin1String("home"), Qt::CaseInsensitive) == 0
                               ? QStringLiteral("iso")
                               : viewName.toLower();
  for (const StandardView& view : kStandardViews) {
    if (!found && (standard == QLatin1String(view.name) ||
                   (standard == QLatin1String("isometric") && QLatin1String(view.name) == QLatin1String("iso")))) {
      camera = fittedCamera(view, box, aspect);
      found = true;
    }
  }
  if (!found) {
    QStringList names;
    for (const QJsonValue& view : named) {
      names << view.toObject().value(QStringLiteral("name")).toString();
    }
    std::cerr << "mitcad-cli: no view '" << viewName.toStdString() << "': the design's named views are "
              << (names.isEmpty() ? QStringLiteral("none") : names.join(QStringLiteral(", "))).toStdString()
              << "; the standard views are front, back, left, right, top, bottom and iso\n";
    return 1;
  }

  QTemporaryDir dir;
  if (!dir.isValid()) {
    std::cerr << "mitcad-cli: no temporary folder: " << dir.errorString().toStdString() << "\n";
    return 1;
  }
  const QString scenePath = dir.filePath(QStringLiteral("scene.glb"));
  std::string error;
  if (!render::writeSceneFile(render::sceneOf(bodies), QDir::toNativeSeparators(scenePath).toStdString(), error)) {
    std::cerr << "mitcad-cli: the scene cannot be written: " << error << "\n";
    return 1;
  }
  // Behind the bodies: the settings' colour, or the 3D view's default
  // light gradient for the view's background.
  QColor top(247, 248, 250);
  QColor bottom(223, 227, 234);
  const QJsonObject background = settings.value(QStringLiteral("background")).toObject();
  if (background.value(QStringLiteral("mode")).toString() == QLatin1String("color")) {
    top = bottom = colorOf(background.value(QStringLiteral("color")));
  }
  QJsonObject job = render::finalJob(settings, folder, scenePath, render::finalView(camera, size), outputPath, top,
                                     bottom, QString());
  job.insert(QStringLiteral("device"), device);

  render::FinalRender final;
  QEventLoop loop;
  int status = 1;
  int lastSamples = -1;
  QObject::connect(&final, &render::FinalRender::progress, &loop,
                   [&lastSamples](int samples, int total, double seconds, double) {
                     if (samples != lastSamples) {
                       lastSamples = samples;
                       std::fprintf(stderr, "Rendering: %d of %d samples, %.1f s\n", samples, total, seconds);
                     }
                   });
  QObject::connect(&final, &render::FinalRender::warning, &loop, [](const QString& message) {
    std::cerr << "Warning: " << message.toStdString() << "\n";
  });
  QObject::connect(&final, &render::FinalRender::finished, &loop,
                   [&](const QString& path, const QSize& rendered, int samples, double seconds) {
                     std::printf("Rendered %s: %d x %d pixels, %d samples in %.2f s (%s, %zu bodies, on %s)\n",
                                 QDir::toNativeSeparators(path).toLocal8Bit().constData(), rendered.width(),
                                 rendered.height(), samples, seconds, format.toLatin1().constData(),
                                 bodies.size(), final.deviceLabel().toUtf8().constData());
                     status = 0;
                     loop.quit();
                   });
  QObject::connect(&final, &render::FinalRender::failed, &loop, [&](const QString& reason) {
    std::cerr << "mitcad-cli: the render failed: " << reason.toStdString() << "\n";
    status = 1;
    loop.quit();
  });
  // When it cannot start, failed() comes from the event loop too.
  final.start(worker, job, dir.filePath(QStringLiteral("job.json")));
  loop.exec();
  return status;
}

} // namespace mitcad::cli
