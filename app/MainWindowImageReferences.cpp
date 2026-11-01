// SPDX-License-Identifier: MIT
// Image references (mitcad#782; app/COMMANDS.md, "Image references"):
// Insert › Image Reference, a panel that places an image file on a plane
// or a planar face and edits one placed (its plane, centre, angle, flip,
// width, opacity, Through Bodies), the images drawn in the view after each
// refresh, and Calibrate: two points clicked on the image and the distance
// typed between them (core/model-api/src/commands.md, "Image
// references").
#include "MainWindow.hpp"

#include <cmath>
#include <exception>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include <QByteArray>
#include <QFile>
#include <QFileInfo>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QtLogging>

#include "commands/CommandSupport.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/Dialogs.hpp"
#include "framework/Json.hpp"
#include "framework/ModelShapes.hpp"
#include "view/ImageCalibration.hpp"
#include "view/ImageFile.hpp"
#include "view/ImageReferences.hpp"

namespace mitcad {
namespace {

using namespace cmd;

constexpr double kPi = 3.14159265358979323846;

const QString kInsertId = QStringLiteral("insert.image_reference");

// The panel edits an image reference placed before (its entry kept as the
// state's object "image"), else it inserts a new one.
bool editing(const CommandState& state) { return !state.object(QStringLiteral("image")).isEmpty(); }

QJsonArray xyz(const gp_Pnt& point) { return QJsonArray{point.X(), point.Y(), point.Z()}; }

QJsonArray xyz(const ImagePoint& point) { return QJsonArray{point[0], point[1], point[2]}; }

// A number without trailing zeros, for the panel's texts.
QString plain(double value) { return QString::number(value, 'g', 10); }

// The most a design embeds (the model's MAX_EMBEDDED_IMAGE): a larger file
// is the model's to refuse, not decoded here first.
constexpr qint64 kEmbeddedImageLimit = qint64(32) << 20;

// Why a HEIF file cannot be drawn, if it cannot: MitCad decodes HEIF itself
// (view/ImageFile.hpp), and its decoder may refuse a file whose header the
// model takes, or the build may have none. Other formats are Qt's, as
// before; empty when the file is drawable or not HEIF.
QString heifProblem(const QString& path) {
  QFile file(path);
  if (QFileInfo(path).size() > kEmbeddedImageLimit || !file.open(QIODevice::ReadOnly) ||
      !isHeifFile(file.peek(4096))) {
    return {};
  }
  QString why;
  if (decodeImageFile(file.readAll(), &why).isNull()) {
    return why;
  }
  return {};
}

// The panel's settings common to the insert, the edit and the outline:
// angle (degrees), flip, width (when one is typed), the plane picked.
void addSettings(QJsonObject& request, const CommandState& state) {
  const Selection& plane = state.items(QStringLiteral("plane"));
  if (!plane.isEmpty()) {
    request.insert(QStringLiteral("plane"), plane.first().reference());
    if (!plane.first().occurrence.isEmpty()) {
      request.insert(QStringLiteral("occurrence"), plane.first().occurrence);
    }
  }
  request.insert(QStringLiteral("angle"), state.value(QStringLiteral("angle")) * 180.0 / kPi);
  request.insert(QStringLiteral("flip"), state.checked(QStringLiteral("flip")));
  const double width = state.value(QStringLiteral("width"));
  if (std::isfinite(width)) {
    request.insert(QStringLiteral("width"), width);
  }
}

// The image's centre, X and Y, when a plane or face is picked: its middle in
// the plane's frame (the model's image_reference_outline gives it). An
// edited image keeps its own until another plane is picked.
void centreOnPlane(CommandState& state, const CommandContext& context) {
  const Selection& plane = state.items(QStringLiteral("plane"));
  if (plane.isEmpty()) {
    return;
  }
  const std::optional<gp_Pnt> at = centerOf(plane, context);
  if (!at) {
    return;
  }
  QJsonObject outline{{QStringLiteral("query"), QStringLiteral("image_reference_outline")},
                      {QStringLiteral("plane"), plane.first().reference()},
                      {QStringLiteral("pixels"), QJsonArray{1, 1}},
                      {QStringLiteral("at"), xyz(*at)}};
  if (!plane.first().occurrence.isEmpty()) {
    outline.insert(QStringLiteral("occurrence"), plane.first().occurrence);
  }
  try {
    const QJsonArray center = context.queryObject(outline).value(QStringLiteral("center")).toArray();
    for (int i = 0; i < 2; ++i) {
      const QString key = i == 0 ? QStringLiteral("x") : QStringLiteral("y");
      const double value = center.at(i).toDouble();
      state.setText(key, lengthText(value, context));
      state.setValue(key, value);
    }
  } catch (const std::exception&) {
    // Not a plane at the marker: the panel says so on OK.
  }
}

} // namespace

CommandDef MainWindow::imageReferenceCommand() {
  // The file chosen with Choose Image…, taken into the File input when the
  // panel shows its inputs again (modelChanged).
  auto chosen = std::make_shared<QString>();
  CommandDef def;
  def.id = kInsertId;
  def.name = tr("Image Reference");
  def.icon = QStringLiteral("image-reference");
  def.tooltip = tr("Places an image on a plane or a face to model from");
  def.tab = QStringLiteral("SOLID");
  def.group = QStringLiteral("INSERT");
  def.keywords = {QStringLiteral("canvas"), QStringLiteral("picture"), QStringLiteral("photo"),
                  QStringLiteral("image"), QStringLiteral("trace"), QStringLiteral("insert image")};
  def.modelCommand = true;
  // An edit logs "Editing image reference Image1 with Image Reference".
  def.editedKind = QStringLiteral("image reference");
  def.inputs = {
      textInput(QStringLiteral("file"), tr("Image")).withVisible([](const CommandState& state) {
        return !editing(state);
      }),
      noteInput(
          QStringLiteral("choose"),
          [](const CommandState&) { return InputNote{QString(), {tr("Choose Image…")}, {}}; },
          [this, chosen](int, const CommandState&, const CommandContext&) {
            // HEIF in builds with its decoder (MITCAD_HEIF, mitcad#850).
            const QString filter = heifDecoderBuilt()
                                       ? tr("Images (*.png *.jpg *.jpeg *.bmp *.webp *.heic *.heif);;All files (*)")
                                       : tr("Images (*.png *.jpg *.jpeg *.bmp *.webp);;All files (*)");
            const QString path =
                dialogOpenFileName(this, tr("Insert Image Reference"), dialogFolder(DialogGroup::Import), filter);
            if (!path.isEmpty()) {
              rememberDialogFolder(DialogGroup::Import, path);
              *chosen = path;
              qDebug().noquote() << QStringLiteral("Image Reference file chosen: %1").arg(QFileInfo(path).fileName());
            }
          })
          .withFieldColumn()
          .withVisible([](const CommandState& state) { return !editing(state); }),
      // A new image starts centred on the plane or face picked (X and Y
      // follow the pick); X and Y then move it on the plane.
      selectionInput(QStringLiteral("plane"), tr("Plane"), kPlaneKinds, 0, 1)
          .withAccepts(isPlanar)
          .withOnChange(centreOnPlane),
      valueInput(QStringLiteral("x"), tr("X"), ValueKind::Length, QStringLiteral("0 mm")),
      valueInput(QStringLiteral("y"), tr("Y"), ValueKind::Length, QStringLiteral("0 mm")),
      valueInput(QStringLiteral("angle"), tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg")),
      flipInput(QStringLiteral("flip"), tr("Flip")).withTooltip(tr("Mirror the image left to right")),
      // Empty: the size from the file for a new image, the size it has for
      // one edited.
      valueInput(QStringLiteral("width"), tr("Width"), ValueKind::Length, QString())
          .withAuto([](const CommandState& state, const CommandContext&) {
            return editing(state) ? tr("Unchanged") : tr("From the image");
          })
          .withTooltip(tr("The image's width; calibrate it by two points afterwards")),
      valueInput(QStringLiteral("opacity"), tr("Opacity %"), ValueKind::Unitless, QStringLiteral("50")),
      checkInput(QStringLiteral("through"), tr("Through Bodies"))
          .withTooltip(tr("Draw the image over the bodies instead of hidden behind them")),
  };
  // A plane or face selected before the command: centred on it.
  def.init = centreOnPlane;
  def.modelChanged = [chosen](CommandState& state, const CommandContext&) {
    if (!chosen->isEmpty()) {
      state.setText(QStringLiteral("file"), *chosen);
      chosen->clear();
    }
  };
  def.build = [](const CommandState& state, const CommandContext&) -> Built {
    const Selection& plane = state.items(QStringLiteral("plane"));
    const double opacity = state.value(QStringLiteral("opacity"));
    if (!std::isfinite(opacity) || opacity < 0.0 || opacity > 100.0) {
      return Built::failure(tr("Opacity: a percentage from 0 to 100."), QStringLiteral("opacity"));
    }
    if (!std::isfinite(state.value(QStringLiteral("angle")))) {
      return Built::failure(tr("Angle: check the value."), QStringLiteral("angle"));
    }
    const double width = state.value(QStringLiteral("width"));
    if (!state.text(QStringLiteral("width")).trimmed().isEmpty() && !(std::isfinite(width) && width > 0.0)) {
      return Built::failure(tr("Width: a length greater than zero."), QStringLiteral("width"));
    }
    QJsonObject command;
    addSettings(command, state);
    command.insert(QStringLiteral("opacity"), opacity / 100.0);
    command.insert(QStringLiteral("through"), state.checked(QStringLiteral("through")));
    const double x = state.value(QStringLiteral("x"));
    const double y = state.value(QStringLiteral("y"));
    if (!std::isfinite(x) || !std::isfinite(y)) {
      return Built::failure(tr("X and Y: check the values."), std::isfinite(x) ? QStringLiteral("y") : QStringLiteral("x"));
    }
    command.insert(QStringLiteral("center"), QJsonArray{x, y});
    // One command: its undo step is the model's ("Insert Image1", "Edit
    // Image1"), no label of the panel's.
    if (editing(state)) {
      command.insert(QStringLiteral("cmd"), QStringLiteral("edit_image_reference"));
      command.insert(QStringLiteral("name"), state.object(QStringLiteral("image")).value(QStringLiteral("name")));
      return Built::of({{QStringLiteral("commands"), QJsonArray{command}}});
    }
    if (plane.isEmpty()) {
      return Built::failure(tr("Select a plane or a planar face."), QStringLiteral("plane"));
    }
    const QString file = state.text(QStringLiteral("file")).trimmed();
    if (file.isEmpty()) {
      return Built::failure(tr("Choose an image file."), QStringLiteral("file"));
    }
    if (!QFileInfo(file).isFile()) {
      return Built::failure(tr("There is no file %1.").arg(file), QStringLiteral("file"));
    }
    const QString problem = heifProblem(file);
    if (!problem.isEmpty()) {
      return Built::failure(tr("The image cannot be read: %1.").arg(problem), QStringLiteral("file"));
    }
    command.insert(QStringLiteral("cmd"), QStringLiteral("insert_image_reference"));
    command.insert(QStringLiteral("path"), QFileInfo(file).absoluteFilePath());
    return Built::of({{QStringLiteral("commands"), QJsonArray{command}}});
  };
  // Where the image goes, outlined while the panel is open (the model's
  // image_reference_outline; a new image's size from its file).
  def.viewLines = [](const CommandState& state, const CommandContext& context) {
    std::vector<std::vector<gp_Pnt>> lines;
    const Selection& plane = state.items(QStringLiteral("plane"));
    if (plane.isEmpty() || !std::isfinite(state.value(QStringLiteral("angle"))) ||
        !std::isfinite(state.value(QStringLiteral("x"))) || !std::isfinite(state.value(QStringLiteral("y")))) {
      return lines;
    }
    QJsonObject outline{{QStringLiteral("query"), QStringLiteral("image_reference_outline")}};
    addSettings(outline, state);
    outline.insert(QStringLiteral("center"),
                   QJsonArray{state.value(QStringLiteral("x")), state.value(QStringLiteral("y"))});
    try {
      if (editing(state)) {
        const QJsonObject image = state.object(QStringLiteral("image"));
        outline.insert(QStringLiteral("pixels"), image.value(QStringLiteral("pixels")));
        if (!outline.contains(QStringLiteral("width"))) {
          outline.insert(QStringLiteral("scale"), image.value(QStringLiteral("scale")));
        }
      } else {
        const QString file = state.text(QStringLiteral("file")).trimmed();
        if (file.isEmpty() || !QFileInfo(file).isFile()) {
          return lines;
        }
        const QJsonObject info = context.queryObject({{QStringLiteral("query"), QStringLiteral("image_info")},
                                                      {QStringLiteral("path"), QFileInfo(file).absoluteFilePath()}});
        outline.insert(QStringLiteral("pixels"), info.value(QStringLiteral("pixels")));
        if (!outline.contains(QStringLiteral("width"))) {
          outline.insert(QStringLiteral("scale"), info.value(QStringLiteral("pixel_size")));
        }
      }
      const QJsonArray corners = context.queryObject(outline).value(QStringLiteral("corners")).toArray();
      if (corners.size() == 4) {
        std::vector<gp_Pnt> loop;
        for (int i = 0; i <= 4; ++i) {
          loop.push_back(pointOf(corners.at(i % 4)));
        }
        lines.push_back(loop);
      }
    } catch (const std::exception&) {
      // No outline: the panel says what is wrong when it is OK'd.
    }
    return lines;
  };
  def.load = [](const QJsonObject& image, CommandState& state, const CommandContext& context) {
    state.setObject(QStringLiteral("image"), image);
    // A plane that is gone (`error`) is left empty: the image keeps it
    // until another plane is picked.
    SelectionItem plane = itemOf(image.value(QStringLiteral("plane")), context);
    if (plane.isValid() && !image.contains(QStringLiteral("error"))) {
      plane.occurrence = image.value(QStringLiteral("occurrence")).toString();
      state.items(QStringLiteral("plane")) = {plane};
    }
    const QJsonArray center = image.value(QStringLiteral("center")).toArray();
    state.setText(QStringLiteral("x"), lengthText(center.at(0).toDouble(), context));
    state.setText(QStringLiteral("y"), lengthText(center.at(1).toDouble(), context));
    state.setText(QStringLiteral("angle"), plain(image.value(QStringLiteral("angle")).toDouble()) + QStringLiteral(" deg"));
    state.setChecked(QStringLiteral("flip"), image.value(QStringLiteral("flip")).toBool());
    state.setText(QStringLiteral("width"),
                  lengthText(image.value(QStringLiteral("size")).toArray().at(0).toDouble(), context));
    state.setText(QStringLiteral("opacity"),
                  plain(std::round(image.value(QStringLiteral("opacity")).toDouble() * 1000.0) / 10.0));
    state.setChecked(QStringLiteral("through"), image.value(QStringLiteral("through")).toBool());
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    if (editing(state)) {
      return QStringLiteral("Edited image reference %1")
          .arg(state.object(QStringLiteral("image")).value(QStringLiteral("name")).toString());
    }
    return QStringLiteral("Inserted image reference from %1")
        .arg(QFileInfo(state.text(QStringLiteral("file")).trimmed()).fileName());
  };
  return def;
}

void MainWindow::registerImageReferenceCommands() {
  m_registry->add(imageReferenceCommand());
  CommandDef calibrate;
  calibrate.id = QStringLiteral("insert.calibrate_image_reference");
  calibrate.name = tr("Calibrate Image Reference");
  calibrate.icon = QStringLiteral("image-calibrate");
  calibrate.tooltip = tr("Scales an image by two points on it and the distance between them");
  calibrate.kind = CommandDef::Kind::Action;
  calibrate.tab = QStringLiteral("SOLID");
  calibrate.group = QStringLiteral("INSERT");
  calibrate.keywords = {QStringLiteral("canvas"), QStringLiteral("scale"), QStringLiteral("image"),
                        QStringLiteral("picture")};
  calibrate.run = [this] { calibrateImageReference(QString()); };
  calibrate.enabled = [this] { return !m_snapshot.imageReferences.isEmpty(); };
  m_registry->add(calibrate);
}

void MainWindow::followImageReferences() {
  const std::vector<ImageReferenceShown> shown = shownImageReferences(m_snapshot.imageReferences);
  QHash<QString, QImage> images;
  for (const ImageReferenceShown& image : shown) {
    if (images.contains(image.sha256)) {
      continue;
    }
    QImage picture = m_imageReferenceImages.value(image.sha256);
    if (picture.isNull()) {
      QString why;
      try {
        const QJsonObject file = queryObject({{QStringLiteral("query"), QStringLiteral("image_reference_image")},
                                              {QStringLiteral("name"), image.name}});
        // HEIF through MitCad's decoder, the rest through Qt's readers
        // (view/ImageFile.hpp).
        picture = decodeImageFile(QByteArray::fromBase64(file.value(QStringLiteral("data")).toString().toLatin1()),
                                  &why);
      } catch (const std::exception& e) {
        qWarning().noquote() << QStringLiteral("Image reference %1 not read: %2").arg(image.name, errorText(e));
      }
      if (picture.isNull()) {
        qWarning().noquote() << QStringLiteral("Image reference %1 cannot be drawn: %2").arg(image.name, why);
      } else {
        qDebug().noquote() << QStringLiteral("Image reference %1 decoded: %2 x %3 pixels")
                                  .arg(image.name)
                                  .arg(picture.width())
                                  .arg(picture.height());
      }
    }
    images.insert(image.sha256, picture);
  }
  m_imageReferenceImages = images;
  m_viewer->setImageReferences(shown, images);
  // A calibration ends when one of its images is gone or hidden.
  if (m_imageCalibration != nullptr) {
    for (const QString& name : m_imageCalibration->names()) {
      const std::optional<ImageReferenceShown> image = imageReferenceOf(m_snapshot.imageReference(name));
      if (!image || !image->visible) {
        endImageCalibration(true);
        break;
      }
    }
  }
}

void MainWindow::editImageReference(const QString& name) {
  if (m_mode != Mode::Idle || m_imageCalibration != nullptr) {
    return;
  }
  const QJsonObject image = m_snapshot.imageReference(name);
  const CommandDef* def = m_registry->find(kInsertId);
  if (image.isEmpty() || def == nullptr) {
    return;
  }
  startFeatureCommand(*def, QString(), image);
}

void MainWindow::calibrateImageReference(const QString& name) {
  if (m_mode != Mode::Idle || m_imageCalibration != nullptr || !canChangeModel()) {
    return;
  }
  // The image asked for (its browser row), which must be shown; else the
  // shown images, of which the first click takes the one it lands on.
  std::vector<ImageReferenceShown> images;
  if (!name.isEmpty()) {
    const std::optional<ImageReferenceShown> image = imageReferenceOf(m_snapshot.imageReference(name));
    if (!image) {
      showStatus(tr("%1 cannot be calibrated: its plane is not there.").arg(name), NoticeLevel::Error);
      return;
    }
    if (!image->visible) {
      showStatus(tr("Show %1 first.").arg(name), NoticeLevel::Error);
      return;
    }
    images.push_back(*image);
  } else {
    images = shownImageReferences(m_snapshot.imageReferences);
    if (images.empty()) {
      showStatus(m_snapshot.imageReferences.isEmpty() ? tr("Insert an image reference first.")
                                                      : tr("Show an image reference first."),
                 NoticeLevel::Error);
      return;
    }
  }
  ImageCalibration::Callbacks callbacks;
  callbacks.first = [this](const QString& image, const ImagePoint& first) {
    qDebug().noquote() << QStringLiteral("Calibrate %1 first point (%2, %3, %4)")
                              .arg(image, plain(first[0]), plain(first[1]), plain(first[2]));
    showHint(tr("Calibrate %1: click the second point on the image. Esc cancels.").arg(image));
  };
  callbacks.picked = [this](const QString& image, const ImagePoint& first, const ImagePoint& second) {
    endImageCalibration(false);
    finishImageCalibration(image, first, second);
  };
  callbacks.cancelled = [this] { endImageCalibration(true); };
  const QString target = images.size() == 1 ? images.front().name : QString();
  m_imageCalibration = new ImageCalibration(m_viewer, std::move(images), std::move(callbacks));
  connect(m_viewer, &OcctViewer::cameraRested, m_imageCalibration, [this] { logImageCalibrationPlaces(); });
  if (!target.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Calibrate %1 started").arg(target);
    showHint(tr("Calibrate %1: click the first point on the image. Esc cancels.").arg(target));
  } else {
    qDebug().noquote() << QStringLiteral("Calibrate started on %1").arg(m_imageCalibration->names().join(QStringLiteral(", ")));
    showHint(tr("Calibrate: click the first point on an image. Esc cancels."));
  }
  logImageCalibrationPlaces();
}

void MainWindow::logImageCalibrationPlaces() {
  if (m_imageCalibration == nullptr) {
    return;
  }
  const struct {
    const char* name;
    double across;
    double down;
  } places[] = {{"left", 0.1, 0.5}, {"right", 0.9, 0.5}, {"top", 0.5, 0.1}, {"bottom", 0.5, 0.9}};
  for (const ImageReferenceShown& image : m_imageCalibration->images()) {
    for (const auto& place : places) {
      const ImagePoint p = imagePointAt(image, place.across, place.down);
      const QPoint at = m_viewer->mapTo(this, m_viewer->toWidget(gp_Pnt(p[0], p[1], p[2])));
      qDebug().noquote() << QStringLiteral("Calibrate %1 %2 at %3,%4")
                                .arg(image.name, QString::fromLatin1(place.name))
                                .arg(at.x())
                                .arg(at.y());
    }
  }
}

void MainWindow::endImageCalibration(bool cancelled) {
  if (m_imageCalibration == nullptr) {
    return;
  }
  const QString name = m_imageCalibration->names().join(QStringLiteral(", "));
  m_imageCalibration->deleteLater();
  m_imageCalibration = nullptr;
  showHint(QString());
  if (cancelled) {
    qDebug().noquote() << QStringLiteral("Calibrate %1 cancelled").arg(name);
  }
}

void MainWindow::finishImageCalibration(const QString& name, const ImagePoint& first, const ImagePoint& second) {
  const double apart = std::hypot(second[0] - first[0], second[1] - first[1], second[2] - first[2]);
  qDebug().noquote() << QStringLiteral("Calibrate %1 second point (%2, %3, %4), %5 mm from the first")
                            .arg(name, plain(second[0]), plain(second[1]), plain(second[2]), plain(apart));
  // The distance as the image is now, in the design's unit, to type over.
  bool accepted = false;
  const QString measured = QString::number(apart / unitMillimetres(), 'g', 6);
  const QString typed = dialogText(this, tr("Calibrate %1").arg(name),
                                   tr("Distance between the two points (%1):").arg(m_snapshot.lengthUnit), measured,
                                   &accepted);
  if (!accepted || typed.trimmed().isEmpty()) {
    qDebug().noquote() << QStringLiteral("Calibrate %1 cancelled").arg(name);
    return;
  }
  double distance = 0.0;
  try {
    distance = queryObject({{QStringLiteral("query"), QStringLiteral("evaluate")},
                            {QStringLiteral("expression"), calibrationExpression(typed, m_snapshot.lengthUnit)},
                            {QStringLiteral("kind"), QStringLiteral("length")}})
                   .value(QStringLiteral("value"))
                   .toDouble();
  } catch (const std::exception& e) {
    showError(tr("The distance %1 is not a length: %2").arg(typed, errorText(e)));
    return;
  }
  QJsonObject result;
  const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("calibrate_image_reference")},
                            {QStringLiteral("name"), name},
                            {QStringLiteral("points"), QJsonArray{xyz(first), xyz(second)}},
                            {QStringLiteral("distance"), distance}};
  if (runModelCommand(command, &result)) {
    qDebug().noquote() << QStringLiteral("Calibrated %1: %2 mm is %3 mm, %4 mm per pixel")
                              .arg(name, plain(apart), plain(distance),
                                   plain(result.value(QStringLiteral("scale")).toDouble()));
  }
}

} // namespace mitcad
