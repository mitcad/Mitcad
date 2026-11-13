// SPDX-License-Identifier: MIT
// Image references (mitcad#782; app/COMMANDS.md, "Image references"):
// Insert › Image Reference, a panel that places an image file on a plane
// or a planar face and edits one placed (its plane, centre, angle, flip,
// width, opacity, Through Bodies), the images drawn in the view after each
// refresh, and Calibrate: two points clicked on the image and the distance
// typed between them (core/model-api/src/commands.md, "Image
// references").
#include "MainWindow.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <exception>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include <QByteArray>
#include <QDateTime>
#include <QFile>
#include <QFileInfo>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QtLogging>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>
#include <gp_XYZ.hxx>

#include "commands/CommandSupport.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/Dialogs.hpp"
#include "framework/Json.hpp"
#include "framework/ModelShapes.hpp"
#include "view/HeifDecoder.hpp"
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

// The size of a pixel of a file that states none (96 to the inch), which
// the model gives a HEIF file.
constexpr double kUnstatedPixelSize = 25.4 / 96.0;

// A HEIF file as Insert › Image Reference embeds it (mitcad#1304): decoded
// by the decoder that is there (view/HeifDecoder.hpp) and kept as a PNG
// file upright, so that the design opens anywhere. Empty (nullopt) for any
// other file, and for one larger than a design embeds (the model refuses
// it with its own message). The panel builds its command on each change:
// the last file's result is kept, by its path, size and time, except that
// a file no decoder read is tried again (the decoder installed meanwhile),
// and logged only the first time.
std::optional<HeifPng> heifInsert(const QString& path) {
  struct Kept {
    QString path;
    qint64 size = -1;
    QDateTime modified;
    std::optional<HeifPng> png;
  };
  static Kept kept;
  const QFileInfo info(path);
  const bool same =
      kept.path == info.absoluteFilePath() && kept.size == info.size() && kept.modified == info.lastModified();
  const bool missingBefore = same && kept.png && kept.png->result == HeifDecoded::Result::NoDecoder;
  if (same && !missingBefore) {
    return kept.png;
  }
  kept = Kept{info.absoluteFilePath(), info.size(), info.lastModified(), std::nullopt};
  QFile file(path);
  if (info.size() > kEmbeddedImageLimit || !file.open(QIODevice::ReadOnly) || !isHeifFile(file.peek(4096))) {
    return kept.png;
  }
  kept.png = heifAsPng(file.readAll());
  if (missingBefore && kept.png->result == HeifDecoded::Result::NoDecoder) {
    return kept.png;
  }
  if (kept.png->result == HeifDecoded::Result::Decoded) {
    qDebug().noquote() << QStringLiteral("Image Reference HEIF file %1 decoded: %2 x %3 pixels, %4 bytes as PNG")
                              .arg(info.fileName())
                              .arg(kept.png->width)
                              .arg(kept.png->height)
                              .arg(kept.png->png.size());
  } else {
    qDebug().noquote() << QStringLiteral("Image Reference HEIF file %1 not decoded: %2")
                              .arg(info.fileName(), kept.png->error);
  }
  return kept.png;
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

// --- Where the panel puts the image (mitcad#1747): its handles and the
// image drawn there while the panel is open.

// A new image's file as the model reads it (image_info: its pixels and the
// size of a pixel); none when it reads none. The handles and the image
// drawn ask for it on each refresh: the last file's answer is kept, by its
// path, size and time.
std::optional<QJsonObject> fileInfo(const QString& path, const CommandContext& context) {
  struct Kept {
    QString path;
    qint64 size = -1;
    QDateTime modified;
    std::optional<QJsonObject> info;
  };
  static Kept kept;
  const QFileInfo file(path);
  if (path.isEmpty() || !file.isFile()) {
    return std::nullopt;
  }
  if (kept.path == file.absoluteFilePath() && kept.size == file.size() && kept.modified == file.lastModified()) {
    return kept.info;
  }
  kept = Kept{file.absoluteFilePath(), file.size(), file.lastModified(), std::nullopt};
  try {
    kept.info = context.queryObject({{QStringLiteral("query"), QStringLiteral("image_info")},
                                     {QStringLiteral("path"), file.absoluteFilePath()}});
  } catch (const std::exception&) {
    // No image the model reads: the panel says so on OK.
  }
  return kept.info;
}

// The image the panel places: its pixels across and down, and the size of
// a pixel while no Width is typed (an edited image's own, a new image's
// file's).
struct ImageSource {
  double columns = 0.0;
  double rows = 0.0;
  double scale = 0.0;
};

std::optional<ImageSource> sourceOf(const CommandState& state, const CommandContext& context) {
  QJsonArray pixels;
  double scale = 0.0;
  if (editing(state)) {
    const QJsonObject image = state.object(QStringLiteral("image"));
    pixels = image.value(QStringLiteral("pixels")).toArray();
    scale = image.value(QStringLiteral("scale")).toDouble();
  } else {
    const std::optional<QJsonObject> info = fileInfo(state.text(QStringLiteral("file")).trimmed(), context);
    if (!info) {
      return std::nullopt;
    }
    pixels = info->value(QStringLiteral("pixels")).toArray();
    scale = info->value(QStringLiteral("pixel_size")).toDouble();
  }
  const ImageSource source{pixels.at(0).toDouble(), pixels.at(1).toDouble(), scale};
  if (!(source.columns > 0.0 && source.rows > 0.0)) {
    return std::nullopt;
  }
  return source;
}

// The picked plane's frame as the model places images on it: its origin,
// its x and y axes (unit) and its normal, from the corners of an image of
// 2 x 2 pixels, 2 mm wide, at its origin (image_reference_outline).
struct ImagePlane {
  gp_Pnt origin;
  gp_Vec x;
  gp_Vec y;
  gp_Vec normal;
};

std::optional<ImagePlane> imagePlane(const CommandState& state, const CommandContext& context) {
  const Selection& plane = state.items(QStringLiteral("plane"));
  if (plane.isEmpty()) {
    return std::nullopt;
  }
  QJsonObject outline{{QStringLiteral("query"), QStringLiteral("image_reference_outline")},
                      {QStringLiteral("plane"), plane.first().reference()},
                      {QStringLiteral("pixels"), QJsonArray{2, 2}},
                      {QStringLiteral("center"), QJsonArray{0.0, 0.0}},
                      {QStringLiteral("angle"), 0.0},
                      {QStringLiteral("flip"), false},
                      {QStringLiteral("width"), 2.0}};
  if (!plane.first().occurrence.isEmpty()) {
    outline.insert(QStringLiteral("occurrence"), plane.first().occurrence);
  }
  QJsonArray corners;
  try {
    corners = context.queryObject(outline).value(QStringLiteral("corners")).toArray();
  } catch (const std::exception&) {
    return std::nullopt; // not a plane at the marker: the panel says so on OK
  }
  if (corners.size() != 4) {
    return std::nullopt;
  }
  // Top left at -x +y, top right at +x +y, bottom right at +x -y.
  const gp_Pnt topLeft = pointOf(corners.at(0));
  const gp_Pnt topRight = pointOf(corners.at(1));
  const gp_Pnt bottomRight = pointOf(corners.at(2));
  const gp_Pnt bottomLeft = pointOf(corners.at(3));
  ImagePlane frame;
  frame.origin = gp_Pnt((topLeft.XYZ() + bottomRight.XYZ()) / 2.0);
  frame.x = gp_Vec(topLeft, topRight) / 2.0;
  frame.y = gp_Vec(bottomLeft, topLeft) / 2.0;
  frame.normal = frame.x.Crossed(frame.y);
  if (frame.normal.Magnitude() < 1e-9) {
    return std::nullopt;
  }
  return frame;
}

// Where the panel's values put the image (the model's
// document::image_references placement): its plane, its centre, the way
// its columns grow (mirrored when flipped) and the way up its rows (unit),
// its size (mm) and its corners as the model gives them (top left, top
// right, bottom right, bottom left). None while a value is not a number,
// no plane is picked or no image is known.
struct ImagePlaced {
  ImagePlane plane;
  ImageSource source;
  gp_Pnt centre;
  gp_Vec right;
  gp_Vec up;
  double width = 0.0;
  double height = 0.0;
  std::array<gp_Pnt, 4> corners;
};

std::optional<ImagePlaced> imagePlaced(const CommandState& state, const CommandContext& context) {
  const double x = state.value(QStringLiteral("x"));
  const double y = state.value(QStringLiteral("y"));
  const double angle = state.value(QStringLiteral("angle"));
  if (!std::isfinite(x) || !std::isfinite(y) || !std::isfinite(angle)) {
    return std::nullopt;
  }
  const std::optional<ImageSource> source = sourceOf(state, context);
  if (!source) {
    return std::nullopt;
  }
  // Width empty: the file's size (a new image), the image's own (edited).
  double width = source->scale * source->columns;
  if (!state.text(QStringLiteral("width")).trimmed().isEmpty()) {
    width = state.value(QStringLiteral("width"));
  }
  if (!(std::isfinite(width) && width > 0.0)) {
    return std::nullopt;
  }
  const std::optional<ImagePlane> plane = imagePlane(state, context);
  if (!plane) {
    return std::nullopt;
  }
  ImagePlaced placed;
  placed.plane = *plane;
  placed.source = *source;
  placed.width = width;
  placed.height = width * source->rows / source->columns;
  const double sign = state.checked(QStringLiteral("flip")) ? -1.0 : 1.0;
  placed.right = (plane->x * std::cos(angle) + plane->y * std::sin(angle)) * sign;
  placed.up = plane->x * -std::sin(angle) + plane->y * std::cos(angle);
  placed.centre = plane->origin.Translated(plane->x * x + plane->y * y);
  const gp_Vec across = placed.right * (placed.width / 2.0);
  const gp_Vec upward = placed.up * (placed.height / 2.0);
  placed.corners = {placed.centre.Translated(upward - across), placed.centre.Translated(upward + across),
                    placed.centre.Translated(across - upward), placed.centre.Translated(-across - upward)};
  return placed;
}

// The smallest Width a corner's drag gives (mm).
constexpr double kSmallestWidth = 0.001;

// X's or Y's arrow (mitcad#1747): along the plane's x or y axis, from its
// other axis (where the value is 0) to the image's centre; its value box
// only while it is under the mouse or dragged.
std::optional<Manipulator> centreArrow(const CommandState& state, const CommandContext& context, bool alongX) {
  const std::optional<ImagePlaced> placed = imagePlaced(state, context);
  if (!placed) {
    return std::nullopt;
  }
  const gp_Vec& along = alongX ? placed->plane.x : placed->plane.y;
  const gp_Vec& other = alongX ? placed->plane.y : placed->plane.x;
  const double across = state.value(alongX ? QStringLiteral("y") : QStringLiteral("x"));
  Manipulator handle = arrow(placed->plane.origin.Translated(other * across), gp_Dir(along));
  handle.valueBox = true;
  handle.quiet = true;
  // A drag steps by a hundredth of the image's width, or the zoom's step.
  handle.scale = placed->width;
  handle.valueScale = false;
  return handle;
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
            // HEIF on every platform (mitcad#1304): a decoder is looked for
            // only when such a file is chosen.
            const QString filter = imageFileFilter();
            qDebug().noquote() << QStringLiteral("Image Reference file dialog: %1").arg(filter);
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
      // Handles in the view (mitcad#1747): arrows along the plane's x and y
      // at the image's centre, a ring round the image, the four corners.
      valueInput(QStringLiteral("x"), tr("X"), ValueKind::Length, QStringLiteral("0 mm"))
          .withManipulator([](const CommandState& state, const CommandContext& context) {
            return centreArrow(state, context, true);
          }),
      valueInput(QStringLiteral("y"), tr("Y"), ValueKind::Length, QStringLiteral("0 mm"))
          .withManipulator([](const CommandState& state, const CommandContext& context) {
            return centreArrow(state, context, false);
          }),
      valueInput(QStringLiteral("angle"), tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withManipulator([](const CommandState& state, const CommandContext& context) -> std::optional<Manipulator> {
            const std::optional<ImagePlaced> placed = imagePlaced(state, context);
            if (!placed) {
              return std::nullopt;
            }
            // About the plane's normal, 0 along its x axis (counter-clockwise
            // seen from the normal's side, as the model turns it).
            Manipulator handle = ring(placed->centre, gp_Dir(placed->plane.normal));
            handle.reference = gp_Dir(placed->plane.x);
            handle.radius = std::hypot(placed->width, placed->height) / 2.0;
            handle.valueBox = true;
            handle.quiet = true;
            return handle;
          }),
      flipInput(QStringLiteral("flip"), tr("Flip")).withTooltip(tr("Mirror the image left to right")),
      // Empty: the size from the file for a new image, the size it has for
      // one edited.
      valueInput(QStringLiteral("width"), tr("Width"), ValueKind::Length, QString())
          .withAuto([](const CommandState& state, const CommandContext&) {
            return editing(state) ? tr("Unchanged") : tr("From the image");
          })
          .withTooltip(tr("The image's width; calibrate it by two points afterwards"))
          // A handle at each corner (element: top left, top right, bottom
          // right, bottom left): an arrow from the opposite corner along the
          // diagonal, its knob at the corner, so that a drag scales the image
          // with its aspect ratio kept.
          .withManipulators([](const CommandState& state, const CommandContext& context) {
            std::vector<Manipulator> handles;
            const std::optional<ImagePlaced> placed = imagePlaced(state, context);
            if (!placed) {
              return handles;
            }
            // Millimetres along the diagonal per millimetre of width.
            const double factor = std::hypot(placed->width, placed->height) / placed->width;
            for (int corner = 0; corner < 4; ++corner) {
              const gp_Pnt& opposite = placed->corners[static_cast<std::size_t>((corner + 2) % 4)];
              const gp_Vec diagonal(opposite, placed->corners[static_cast<std::size_t>(corner)]);
              if (diagonal.Magnitude() < 1e-9) {
                continue;
              }
              Manipulator handle = arrow(opposite, gp_Dir(diagonal));
              handle.element = corner;
              handle.factor = factor;
              handle.minimum = kSmallestWidth;
              handle.autoValue = placed->width; // Width empty: the image's size now
              handle.scale = placed->width;
              handle.valueScale = false;
              handle.valueBox = true;
              handle.boxAtTip = true;
              handle.quiet = true;
              handles.push_back(handle);
            }
            return handles;
          })
          // The opposite corner stays where it was when the drag began: the
          // centre moves half as far as the corner dragged.
          .withHandleDrag([](int corner, const CommandState& atStart, CommandState& state,
                             const CommandContext& context) {
            const double width = state.value(QStringLiteral("width"));
            const std::optional<ImagePlaced> before = imagePlaced(atStart, context);
            if (corner < 0 || corner > 3 || !before || !std::isfinite(width) || width <= 0.0) {
              return;
            }
            const gp_Vec diagonal(before->corners[static_cast<std::size_t>((corner + 2) % 4)],
                                  before->corners[static_cast<std::size_t>(corner)]);
            const gp_Vec shift = diagonal * ((width - before->width) / (2.0 * before->width));
            const double x = atStart.value(QStringLiteral("x")) + shift.Dot(before->plane.x);
            const double y = atStart.value(QStringLiteral("y")) + shift.Dot(before->plane.y);
            state.setText(QStringLiteral("x"), lengthText(x, context));
            state.setValue(QStringLiteral("x"), x);
            state.setText(QStringLiteral("y"), lengthText(y, context));
            state.setValue(QStringLiteral("y"), y);
          }),
      valueInput(QStringLiteral("opacity"), tr("Opacity %"), ValueKind::Unitless, QStringLiteral("50")),
      checkInput(QStringLiteral("through"), tr("Through Bodies"))
          .withTooltip(tr("Draw the image over the bodies instead of hidden behind them")),
  };
  // Whether a panel is open: the image it places is drawn only then
  // (mitcad#1747).
  auto open = std::make_shared<bool>(false);
  // A plane or face selected before the command: centred on it.
  def.init = [open](CommandState& state, const CommandContext& context) {
    *open = true;
    centreOnPlane(state, context);
  };
  def.ended = [this, open](bool) {
    *open = false;
    setImageReferencePanel(std::nullopt);
  };
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
    command.insert(QStringLiteral("cmd"), QStringLiteral("insert_image_reference"));
    const std::optional<HeifPng> heif = heifInsert(file);
    if (!heif) {
      command.insert(QStringLiteral("path"), QFileInfo(file).absoluteFilePath());
      return Built::of({{QStringLiteral("commands"), QJsonArray{command}}});
    }
    // A HEIF file: the one message only when no decoder here reads it.
    if (heif->result == HeifDecoded::Result::NoDecoder) {
      return Built::failure(heifDecoderMissingMessage(), QStringLiteral("file"));
    }
    if (heif->result != HeifDecoded::Result::Decoded) {
      return Built::failure(tr("The image cannot be read: %1.").arg(heif->error), QStringLiteral("file"));
    }
    // Embedded as the PNG file, its own name kept; the size a HEIF file has
    // (96 pixels to the inch) unless a width is typed.
    command.insert(QStringLiteral("data"), QString::fromLatin1(heif->png.toBase64()));
    command.insert(QStringLiteral("file_name"), QFileInfo(file).fileName());
    if (!command.contains(QStringLiteral("width"))) {
      command.insert(QStringLiteral("scale"), kUnstatedPixelSize);
    }
    return Built::of({{QStringLiteral("commands"), QJsonArray{command}}});
  };
  // Where the image goes while the panel is open (mitcad#1747): the image
  // itself drawn there with its opacity, an edited one in place of where it
  // was, and its outline (as the model's image_reference_outline puts it;
  // a new image's size from its file). Asked again with the handles, after
  // each change of the values.
  def.viewLines = [this, open](const CommandState& state, const CommandContext& context) {
    std::vector<std::vector<gp_Pnt>> lines;
    const std::optional<ImagePlaced> placed = imagePlaced(state, context);
    if (*open) {
      std::optional<ImageReferencePanel> panel;
      if (placed) {
        panel.emplace();
        panel->editing = editing(state);
        const QJsonObject image = state.object(QStringLiteral("image"));
        if (!panel->editing) {
          panel->file = QFileInfo(state.text(QStringLiteral("file")).trimmed()).absoluteFilePath();
        }
        ImageReferenceShown& shown = panel->image;
        shown.name =
            panel->editing ? image.value(QStringLiteral("name")).toString() : QFileInfo(panel->file).fileName();
        shown.sha256 = image.value(QStringLiteral("sha256")).toString();
        for (std::size_t i = 0; i < 4; ++i) {
          shown.corners[i] = {placed->corners[i].X(), placed->corners[i].Y(), placed->corners[i].Z()};
        }
        // An opacity that is no percentage yet: the panel's default.
        const double opacity = state.value(QStringLiteral("opacity"));
        shown.opacity = std::isfinite(opacity) ? std::clamp(opacity / 100.0, 0.0, 1.0) : 0.5;
        shown.through = state.checked(QStringLiteral("through"));
        shown.width = static_cast<int>(placed->source.columns);
        shown.height = static_cast<int>(placed->source.rows);
      }
      setImageReferencePanel(panel);
    }
    if (placed) {
      std::vector<gp_Pnt> loop;
      for (int i = 0; i <= 4; ++i) {
        loop.push_back(placed->corners[static_cast<std::size_t>(i % 4)]);
      }
      lines.push_back(loop);
    }
    return lines;
  };
  def.load = [open](const QJsonObject& image, CommandState& state, const CommandContext& context) {
    *open = true;
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
      picture = imageReferencePicture(image.name, image.sha256);
    }
    images.insert(image.sha256, picture);
  }
  m_imageReferenceImages = images;
  drawImageReferences();
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

QImage MainWindow::imageReferencePicture(const QString& name, const QString& sha256) {
  // A HEIF image that no decoder here reads (one a design embedded as it
  // was, before mitcad#1304) is not tried again on each refresh.
  if (m_imageReferenceNoDecoder.contains(sha256)) {
    return QImage();
  }
  QImage picture;
  QString why;
  bool noDecoder = false;
  try {
    const QJsonObject file = queryObject({{QStringLiteral("query"), QStringLiteral("image_reference_image")},
                                          {QStringLiteral("name"), name}});
    // HEIF through the decoder that is there, the rest through Qt's
    // readers (view/ImageFile.hpp).
    picture = decodeImageFile(QByteArray::fromBase64(file.value(QStringLiteral("data")).toString().toLatin1()),
                              &why, &noDecoder);
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Image reference %1 not read: %2").arg(name, errorText(e));
  }
  if (noDecoder) {
    // Said once, where the user sees it: what to install.
    m_imageReferenceNoDecoder.insert(sha256, true);
    showStatus(tr("%1 is a HEIF image: %2").arg(name, heifDecoderMissingMessage()), NoticeLevel::Warning);
  }
  if (picture.isNull()) {
    qWarning().noquote() << QStringLiteral("Image reference %1 cannot be drawn: %2").arg(name, why);
  } else {
    qDebug().noquote() << QStringLiteral("Image reference %1 decoded: %2 x %3 pixels")
                              .arg(name)
                              .arg(picture.width())
                              .arg(picture.height());
  }
  return picture;
}

void MainWindow::setImageReferencePanel(const std::optional<ImageReferencePanel>& panel) {
  if (!panel) {
    if (!m_imageReferencePanel && m_imageReferencePanelSource.isEmpty()) {
      return;
    }
    m_imageReferencePanel.reset();
    m_imageReferencePanelSource.clear();
    m_imageReferencePanelPicture = QImage();
    drawImageReferences();
    return;
  }
  ImageReferencePanel next = *panel;
  // Its picture: an edited image's from the design, a new one's from its
  // file (as it is now), decoded once.
  QString source;
  if (next.editing) {
    source = QStringLiteral("image/") + next.image.sha256;
  } else {
    const QFileInfo info(next.file);
    source = QStringLiteral("file/%1/%2/%3")
                 .arg(info.absoluteFilePath())
                 .arg(info.size())
                 .arg(info.lastModified().toMSecsSinceEpoch());
  }
  if (source != m_imageReferencePanelSource) {
    m_imageReferencePanelSource = source;
    if (next.editing) {
      m_imageReferencePanelPicture = m_imageReferenceImages.value(next.image.sha256);
      if (m_imageReferencePanelPicture.isNull()) {
        m_imageReferencePanelPicture = imageReferencePicture(next.image.name, next.image.sha256);
      }
    } else {
      // A file larger than a design embeds is not read: the model refuses it.
      QFile file(next.file);
      QString why;
      m_imageReferencePanelPicture = QImage();
      if (file.size() <= kEmbeddedImageLimit && file.open(QIODevice::ReadOnly)) {
        m_imageReferencePanelPicture = decodeImageFile(file.readAll(), &why);
      }
      if (m_imageReferencePanelPicture.isNull()) {
        qDebug().noquote() << QStringLiteral("Image Reference panel image %1 not drawn: %2")
                                  .arg(next.image.name, why.isEmpty() ? QStringLiteral("not read") : why);
      } else {
        qDebug().noquote() << QStringLiteral("Image Reference panel image %1 decoded: %2 x %3 pixels")
                                  .arg(next.image.name)
                                  .arg(m_imageReferencePanelPicture.width())
                                  .arg(m_imageReferencePanelPicture.height());
      }
    }
  }
  if (!next.editing) {
    next.image.sha256 = QStringLiteral("panel/") + source; // its picture's and its texture's key
  }
  // Drawn again only when it moved or looks otherwise.
  if (m_imageReferencePanel) {
    const ImageReferenceShown& was = m_imageReferencePanel->image;
    const ImageReferenceShown& now = next.image;
    if (was.name == now.name && was.sha256 == now.sha256 && was.corners == now.corners &&
        was.opacity == now.opacity && was.through == now.through && m_imageReferencePanel->editing == next.editing) {
      return;
    }
  }
  m_imageReferencePanel = next;
  // For UI tests: where it is drawn, its centre in the design.
  const std::array<ImagePoint, 4>& corners = next.image.corners;
  const auto middle = [&corners](std::size_t axis) {
    return QString::number((corners[0][axis] + corners[2][axis]) / 2.0, 'f', 6);
  };
  qDebug().noquote() << QStringLiteral("Image Reference panel image %1 centre %2,%3,%4%5")
                            .arg(next.image.name, middle(0), middle(1), middle(2),
                                 next.editing ? QStringLiteral(", in place of the image") : QString());
  drawImageReferences();
}

void MainWindow::drawImageReferences() {
  std::vector<ImageReferenceShown> shown = shownImageReferences(m_snapshot.imageReferences);
  QHash<QString, QImage> images = m_imageReferenceImages;
  if (m_imageReferencePanel) {
    // The panel's image (mitcad#1747) in place of the one it edits, drawn
    // where the panel's values put it.
    const ImageReferencePanel& panel = *m_imageReferencePanel;
    if (panel.editing) {
      shown.erase(std::remove_if(shown.begin(), shown.end(),
                                 [&panel](const ImageReferenceShown& image) { return image.name == panel.image.name; }),
                  shown.end());
    }
    if (!m_imageReferencePanelPicture.isNull()) {
      images.insert(panel.image.sha256, m_imageReferencePanelPicture);
      shown.push_back(panel.image);
    }
  }
  m_viewer->setImageReferences(shown, images);
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
      const QPoint at = m_viewer->mapTo(window(), m_viewer->toWidget(gp_Pnt(p[0], p[1], p[2])));
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
