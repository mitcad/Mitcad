// SPDX-License-Identifier: MIT
// Decals (mitcad#2244; app/COMMANDS.md, "Decals"): Insert › Decal, a panel
// that puts an image file on a planar, cylindrical or conical face and
// edits one put there (its face, centre, angle, flip, width, height, Lock
// Aspect, opacity) with the live result drawn on the face and handles that
// move, turn and scale it; the decals drawn in the view after each
// refresh; and the decal under a right click on its face, for the view's
// menu (core/model-api/src/commands.md, "Decals").
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

#include "OcctViewer.hpp"
#include "commands/CommandSupport.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/Dialogs.hpp"
#include "framework/Json.hpp"
#include "framework/ModelShapes.hpp"
#include "view/Decals.hpp"
#include "view/HeifDecoder.hpp"
#include "view/ImageFile.hpp"

namespace mitcad {
namespace {

using namespace cmd;

constexpr double kDecalPi = 3.14159265358979323846;

const QString kDecalId = QStringLiteral("insert.decal");

// The panel edits a decal put before (its entry kept as the state's object
// "decal"), else it inserts a new one.
bool editingDecal(const CommandState& state) { return !state.object(QStringLiteral("decal")).isEmpty(); }

QJsonArray xyzOf(const gp_Pnt& point) { return QJsonArray{point.X(), point.Y(), point.Z()}; }

// A number without trailing zeros, for the panel's texts.
QString plainNumber(double value) { return QString::number(value, 'g', 10); }

// The most a design embeds (the model's MAX_EMBEDDED_IMAGE).
constexpr qint64 kDecalImageLimit = qint64(32) << 20;

// The size of a pixel of a file that states none (96 to the inch), which
// the model gives a HEIF file.
constexpr double kDecalUnstatedPixelSize = 25.4 / 96.0;

// The faces a decal goes on, as the view names their surfaces.
bool takesDecal(const SelectionItem& face) {
  return face.geometry == QStringLiteral("plane") || face.geometry == QStringLiteral("cylinder") ||
         face.geometry == QStringLiteral("cone");
}

// A HEIF file as Insert › Decal embeds it (as Insert › Image Reference
// does, mitcad#1304): decoded by the decoder that is there and kept as a
// PNG file upright. Empty for any other file and for one larger than a
// design embeds. The last file's result is kept, by its path, size and
// time.
std::optional<HeifPng> decalHeif(const QString& path) {
  struct Kept {
    QString path;
    qint64 size = -1;
    QDateTime modified;
    std::optional<HeifPng> png;
  };
  static Kept kept;
  const QFileInfo info(path);
  if (kept.path == info.absoluteFilePath() && kept.size == info.size() && kept.modified == info.lastModified() &&
      !(kept.png && kept.png->result == HeifDecoded::Result::NoDecoder)) {
    return kept.png;
  }
  kept = Kept{info.absoluteFilePath(), info.size(), info.lastModified(), std::nullopt};
  QFile file(path);
  if (info.size() > kDecalImageLimit || !file.open(QIODevice::ReadOnly) || !isHeifFile(file.peek(4096))) {
    return kept.png;
  }
  kept.png = heifAsPng(file.readAll());
  return kept.png;
}

// A new decal's file as the model reads it (image_info: its pixels and the
// size of a pixel); none when it reads none. The last file's answer is
// kept, by its path, size and time.
std::optional<QJsonObject> decalFileInfo(const QString& path, const CommandContext& context) {
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

// The image the panel puts: its pixels across and down, and the width it
// has while no Width is typed (an edited decal's own, a new decal's from
// its file).
struct DecalSource {
  int columns = 0;
  int rows = 0;
  double width = 0.0;
};

std::optional<DecalSource> decalSourceOf(const CommandState& state, const CommandContext& context) {
  QJsonArray pixels;
  double width = 0.0;
  if (editingDecal(state)) {
    const QJsonObject decal = state.object(QStringLiteral("decal"));
    pixels = decal.value(QStringLiteral("pixels")).toArray();
    width = decal.value(QStringLiteral("width")).toDouble();
  } else {
    const std::optional<QJsonObject> info =
        decalFileInfo(state.text(QStringLiteral("file")).trimmed(), context);
    if (!info) {
      return std::nullopt;
    }
    pixels = info->value(QStringLiteral("pixels")).toArray();
    width = info->value(QStringLiteral("pixel_size")).toDouble() * pixels.at(0).toDouble();
  }
  const DecalSource source{pixels.at(0).toInt(), pixels.at(1).toInt(), width};
  if (source.columns <= 0 || source.rows <= 0 || !(width > 0.0)) {
    return std::nullopt;
  }
  return source;
}

// The face picked, as the model's reference with its occurrence.
void addFace(QJsonObject& request, const CommandState& state) {
  const Selection& face = state.items(QStringLiteral("face"));
  if (!face.isEmpty()) {
    request.insert(QStringLiteral("face"), face.first().reference());
    if (!face.first().occurrence.isEmpty()) {
      request.insert(QStringLiteral("occurrence"), face.first().occurrence);
    }
  }
}

// The size the panel gives: with Lock Aspect the Width typed (else the
// Height typed), without it each typed; nothing typed keeps the size.
void addSize(QJsonObject& request, const CommandState& state) {
  const bool locked = state.checked(QStringLiteral("lock"));
  request.insert(QStringLiteral("lock_aspect"), locked);
  const bool width = !state.text(QStringLiteral("width")).trimmed().isEmpty();
  const bool height = !state.text(QStringLiteral("height")).trimmed().isEmpty();
  if (width) {
    request.insert(QStringLiteral("width"), state.value(QStringLiteral("width")));
  }
  if (height && (!locked || !width)) {
    request.insert(QStringLiteral("height"), state.value(QStringLiteral("height")));
  }
}

// The panel's settings common to the insert, the edit and the outline.
void addDecalSettings(QJsonObject& request, const CommandState& state) {
  addFace(request, state);
  request.insert(QStringLiteral("angle"), state.value(QStringLiteral("angle")) * 180.0 / kDecalPi);
  request.insert(QStringLiteral("flip"), state.checked(QStringLiteral("flip")));
  addSize(request, state);
}

// The decal's centre, X and Y, when a face is picked: the face's middle in
// its coordinates (the model's decal_outline gives it). An edited decal
// keeps its own until another face is picked.
void centreOnFace(CommandState& state, const CommandContext& context) {
  const Selection& face = state.items(QStringLiteral("face"));
  if (face.isEmpty() || !takesDecal(face.first())) {
    return;
  }
  // Where the face was clicked (in the root, where its coordinates are the
  // design's), else its middle.
  std::optional<gp_Pnt> at;
  if (face.first().at && face.first().occurrence.isEmpty()) {
    const std::array<double, 3>& hit = *face.first().at;
    at = gp_Pnt(hit[0], hit[1], hit[2]);
  } else {
    at = centerOf(face, context);
  }
  if (!at) {
    return;
  }
  QJsonObject outline{{QStringLiteral("query"), QStringLiteral("decal_outline")},
                      {QStringLiteral("pixels"), QJsonArray{1, 1}},
                      {QStringLiteral("at"), xyzOf(*at)}};
  addFace(outline, state);
  try {
    const QJsonArray center = context.queryObject(outline).value(QStringLiteral("center")).toArray();
    for (int i = 0; i < 2; ++i) {
      const QString key = i == 0 ? QStringLiteral("x") : QStringLiteral("y");
      const double value = center.at(i).toDouble();
      state.setText(key, lengthText(value, context));
      state.setValue(key, value);
    }
  } catch (const std::exception&) {
    // Not a face that takes a decal at the marker: the panel says so on OK.
  }
}

// Where the panel's values put the decal (the model's decal_outline): its
// kind of face, size, corners (top left, top right, bottom right, bottom
// left) and grid; the face's directions at its centre (x and y of the
// face's coordinates there, unit) and its outward normal. None while a
// value is not a number, no face is picked or no image is known.
struct DecalPlaced {
  QJsonObject outline;
  DecalSource source;
  gp_Pnt centre;
  gp_Vec x;
  gp_Vec y;
  gp_Vec normal;
  double width = 0.0;
  double height = 0.0;
  std::array<gp_Pnt, 4> corners;
};

std::optional<DecalPlaced> decalPlaced(const CommandState& state, const CommandContext& context) {
  const double x = state.value(QStringLiteral("x"));
  const double y = state.value(QStringLiteral("y"));
  const double angle = state.value(QStringLiteral("angle"));
  const Selection& face = state.items(QStringLiteral("face"));
  if (!std::isfinite(x) || !std::isfinite(y) || !std::isfinite(angle) || face.isEmpty() ||
      !takesDecal(face.first())) {
    return std::nullopt;
  }
  const std::optional<DecalSource> source = decalSourceOf(state, context);
  if (!source) {
    return std::nullopt;
  }
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("decal_outline")},
                      {QStringLiteral("pixels"), QJsonArray{source->columns, source->rows}},
                      {QStringLiteral("center"), QJsonArray{x, y}},
                      {QStringLiteral("default_width"), source->width}};
  addDecalSettings(request, state);
  // The face's directions at the centre: a decal 2 by 2 micrometres there.
  QJsonObject local{{QStringLiteral("query"), QStringLiteral("decal_outline")},
                    {QStringLiteral("pixels"), QJsonArray{2, 2}},
                    {QStringLiteral("center"), QJsonArray{x, y}},
                    {QStringLiteral("width"), 0.002}};
  addFace(local, state);
  DecalPlaced placed;
  QJsonArray localCorners;
  try {
    placed.outline = context.queryObject(request);
    localCorners = context.queryObject(local).value(QStringLiteral("corners")).toArray();
  } catch (const std::exception&) {
    return std::nullopt; // a value the model refuses: the panel says so on OK
  }
  const QJsonArray corners = placed.outline.value(QStringLiteral("corners")).toArray();
  if (corners.size() != 4 || localCorners.size() != 4) {
    return std::nullopt;
  }
  for (int i = 0; i < 4; ++i) {
    placed.corners[static_cast<std::size_t>(i)] = pointOf(corners.at(i));
  }
  const gp_Pnt topLeft = pointOf(localCorners.at(0));
  const gp_Pnt topRight = pointOf(localCorners.at(1));
  const gp_Pnt bottomRight = pointOf(localCorners.at(2));
  const gp_Pnt bottomLeft = pointOf(localCorners.at(3));
  placed.centre = gp_Pnt((topLeft.XYZ() + bottomRight.XYZ()) / 2.0);
  placed.x = gp_Vec(topLeft, topRight);
  placed.y = gp_Vec(bottomLeft, topLeft);
  if (placed.x.Magnitude() < 1e-9 || placed.y.Magnitude() < 1e-9) {
    return std::nullopt;
  }
  placed.x.Normalize();
  placed.y.Normalize();
  placed.normal = placed.x.Crossed(placed.y);
  if (placed.normal.Magnitude() < 1e-9) {
    return std::nullopt;
  }
  placed.normal.Normalize();
  const QJsonArray size = placed.outline.value(QStringLiteral("size")).toArray();
  placed.width = size.at(0).toDouble();
  placed.height = size.at(1).toDouble();
  placed.source = *source;
  if (!(placed.width > 0.0 && placed.height > 0.0)) {
    return std::nullopt;
  }
  return placed;
}

// The smallest Width a corner's drag gives (mm).
constexpr double kSmallestDecalWidth = 0.001;

// X's or Y's arrow: along the face's x or y direction at the decal's
// centre, its knob at the centre (where the value puts it); its value box
// only while it is under the mouse or dragged.
std::optional<Manipulator> decalArrow(const CommandState& state, const CommandContext& context, bool alongX) {
  const std::optional<DecalPlaced> placed = decalPlaced(state, context);
  if (!placed) {
    return std::nullopt;
  }
  const gp_Vec& along = alongX ? placed->x : placed->y;
  const double value = state.value(alongX ? QStringLiteral("x") : QStringLiteral("y"));
  Manipulator handle = arrow(placed->centre.Translated(along * -value), gp_Dir(along));
  handle.valueBox = true;
  handle.quiet = true;
  handle.scale = placed->width;
  handle.valueScale = false;
  return handle;
}

// The border of the decal's grid, around it from its top left corner.
std::vector<gp_Pnt> decalBorder(const QJsonObject& outline) {
  const QJsonObject mesh = outline.value(QStringLiteral("mesh")).toObject();
  const int columns = mesh.value(QStringLiteral("columns")).toInt();
  const int rows = mesh.value(QStringLiteral("rows")).toInt();
  const QJsonArray points = mesh.value(QStringLiteral("points")).toArray();
  std::vector<gp_Pnt> border;
  // (The grid's points come first; the points where the face's edges cut
  // it follow them.)
  if (columns < 1 || rows < 1 || points.size() < static_cast<qsizetype>(columns + 1) * (rows + 1)) {
    return border;
  }
  const auto at = [&](int row, int column) { return pointOf(points.at(row * (columns + 1) + column)); };
  for (int column = 0; column <= columns; ++column) {
    border.push_back(at(0, column));
  }
  for (int row = 1; row <= rows; ++row) {
    border.push_back(at(row, columns));
  }
  for (int column = columns - 1; column >= 0; --column) {
    border.push_back(at(rows, column));
  }
  for (int row = rows - 1; row >= 0; --row) {
    border.push_back(at(row, 0));
  }
  return border;
}

} // namespace

CommandDef MainWindow::decalCommand() {
  // The file chosen with Choose Image…, taken into the File input when the
  // panel shows its inputs again (modelChanged).
  auto chosen = std::make_shared<QString>();
  CommandDef def;
  def.id = kDecalId;
  def.name = tr("Decal");
  def.icon = QStringLiteral("decal");
  def.tooltip = tr("Puts an image on a face: a label, a logo or a sign");
  def.keywords = {QStringLiteral("label"), QStringLiteral("logo"), QStringLiteral("sticker"),
                  QStringLiteral("image"), QStringLiteral("picture"), QStringLiteral("sign")};
  def.modelCommand = true;
  // An edit logs "Editing decal Decal1 with Decal".
  def.editedKind = QStringLiteral("decal");
  def.inputs = {
      textInput(QStringLiteral("file"), tr("Image")).withVisible([](const CommandState& state) {
        return !editingDecal(state);
      }),
      noteInput(
          QStringLiteral("choose"),
          [](const CommandState&) { return InputNote{QString(), {tr("Choose Image…")}, {}}; },
          [this, chosen](int, const CommandState&, const CommandContext&) {
            const QString filter = imageFileFilter();
            const QString path =
                dialogOpenFileName(this, tr("Insert Decal"), dialogFolder(DialogGroup::Import), filter);
            if (!path.isEmpty()) {
              rememberDialogFolder(DialogGroup::Import, path);
              *chosen = path;
              qDebug().noquote() << QStringLiteral("Decal file chosen: %1").arg(QFileInfo(path).fileName());
            }
          })
          .withFieldColumn()
          .withVisible([](const CommandState& state) { return !editingDecal(state); }),
      // A new decal starts centred on the face picked (X and Y follow the
      // pick); X and Y then move it on the face.
      selectionInput(QStringLiteral("face"), tr("Face"), SelectKind::Face, 0, 1).withOnChange(centreOnFace),
      // Handles in the view: arrows along the face at the decal's centre,
      // a ring round it, an arrow to each corner.
      valueInput(QStringLiteral("x"), tr("X"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(tr("Across the face; around the axis on a curved face"))
          .withManipulator([](const CommandState& state, const CommandContext& context) {
            return decalArrow(state, context, true);
          }),
      valueInput(QStringLiteral("y"), tr("Y"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(tr("Up the face; along the axis on a curved face"))
          .withManipulator([](const CommandState& state, const CommandContext& context) {
            return decalArrow(state, context, false);
          }),
      valueInput(QStringLiteral("width"), tr("Width"), ValueKind::Length, QString())
          .withAuto([](const CommandState& state, const CommandContext&) {
            return editingDecal(state) ? tr("Unchanged") : tr("From the image");
          })
          .withTooltip(tr("The decal's width along the face"))
          // A handle at each corner (element: top left, top right, bottom
          // right, bottom left): an arrow from the centre to the corner, so
          // that a drag scales the decal about its centre.
          .withManipulators([](const CommandState& state, const CommandContext& context) {
            std::vector<Manipulator> handles;
            const std::optional<DecalPlaced> placed = decalPlaced(state, context);
            if (!placed) {
              return handles;
            }
            // Millimetres from the centre to a corner per millimetre of width.
            const double factor = std::hypot(placed->width, placed->height) / (2.0 * placed->width);
            for (int corner = 0; corner < 4; ++corner) {
              const gp_Vec diagonal(placed->centre, placed->corners[static_cast<std::size_t>(corner)]);
              if (diagonal.Magnitude() < 1e-9) {
                continue;
              }
              Manipulator handle = arrow(placed->centre, gp_Dir(diagonal));
              handle.element = corner;
              handle.factor = factor;
              handle.minimum = kSmallestDecalWidth;
              handle.autoValue = placed->width; // Width empty: the decal's size now
              handle.scale = placed->width;
              handle.valueScale = false;
              handle.valueBox = true;
              handle.boxAtTip = true;
              handle.quiet = true;
              handles.push_back(handle);
            }
            return handles;
          }),
      valueInput(QStringLiteral("height"), tr("Height"), ValueKind::Length, QString())
          .withAuto([](const CommandState& state, const CommandContext&) {
            return state.checked(QStringLiteral("lock")) ? tr("In proportion") : tr("Unchanged");
          })
          .withTooltip(tr("The decal's height along the face")),
      checkInput(QStringLiteral("lock"), tr("Lock Aspect"), true)
          .withTooltip(tr("Keep the image's proportions: the width sets the height")),
      valueInput(QStringLiteral("angle"), tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withManipulator([](const CommandState& state, const CommandContext& context) -> std::optional<Manipulator> {
            const std::optional<DecalPlaced> placed = decalPlaced(state, context);
            if (!placed) {
              return std::nullopt;
            }
            // About the face's normal, 0 along its x direction
            // (counter-clockwise seen from outside, as the model turns it).
            Manipulator handle = ring(placed->centre, gp_Dir(placed->normal));
            handle.reference = gp_Dir(placed->x);
            handle.radius = std::hypot(placed->width, placed->height) / 2.0;
            handle.valueBox = true;
            handle.quiet = true;
            return handle;
          }),
      flipInput(QStringLiteral("flip"), tr("Flip")).withTooltip(tr("Mirror the image left to right")),
      valueInput(QStringLiteral("opacity"), tr("Opacity %"), ValueKind::Unitless, QStringLiteral("100")),
  };
  // Whether a panel is open: the decal it puts is drawn only then.
  auto open = std::make_shared<bool>(false);
  // A face selected before the command: centred on it.
  def.init = [open](CommandState& state, const CommandContext& context) {
    *open = true;
    centreOnFace(state, context);
  };
  def.ended = [this, open](bool) {
    *open = false;
    setDecalPanel(std::nullopt);
  };
  def.modelChanged = [chosen](CommandState& state, const CommandContext&) {
    if (!chosen->isEmpty()) {
      state.setText(QStringLiteral("file"), *chosen);
      chosen->clear();
    }
  };
  def.build = [](const CommandState& state, const CommandContext&) -> Built {
    const Selection& face = state.items(QStringLiteral("face"));
    const double opacity = state.value(QStringLiteral("opacity"));
    if (!std::isfinite(opacity) || opacity < 0.0 || opacity > 100.0) {
      return Built::failure(tr("Opacity: a percentage from 0 to 100."), QStringLiteral("opacity"));
    }
    if (!std::isfinite(state.value(QStringLiteral("angle")))) {
      return Built::failure(tr("Angle: check the value."), QStringLiteral("angle"));
    }
    for (const char* key : {"width", "height"}) {
      const QString id = QString::fromLatin1(key);
      const double value = state.value(id);
      if (!state.text(id).trimmed().isEmpty() && !(std::isfinite(value) && value > 0.0)) {
        return Built::failure(id == QLatin1String("width") ? tr("Width: a length greater than zero.")
                                                           : tr("Height: a length greater than zero."),
                              id);
      }
    }
    const double x = state.value(QStringLiteral("x"));
    const double y = state.value(QStringLiteral("y"));
    if (!std::isfinite(x) || !std::isfinite(y)) {
      return Built::failure(tr("X and Y: check the values."), std::isfinite(x) ? QStringLiteral("y") : QStringLiteral("x"));
    }
    if (!face.isEmpty() && !takesDecal(face.first())) {
      return Built::failure(tr("A decal goes on a planar, cylindrical or conical face."), QStringLiteral("face"));
    }
    QJsonObject command;
    addDecalSettings(command, state);
    command.insert(QStringLiteral("opacity"), opacity / 100.0);
    command.insert(QStringLiteral("center"), QJsonArray{x, y});
    // One command: its undo step is the model's ("Insert Decal1", "Edit
    // Decal1"), no label of the panel's.
    if (editingDecal(state)) {
      command.insert(QStringLiteral("cmd"), QStringLiteral("edit_decal"));
      command.insert(QStringLiteral("name"), state.object(QStringLiteral("decal")).value(QStringLiteral("name")));
      return Built::of({{QStringLiteral("commands"), QJsonArray{command}}});
    }
    if (face.isEmpty()) {
      return Built::failure(tr("Select a face."), QStringLiteral("face"));
    }
    const QString file = state.text(QStringLiteral("file")).trimmed();
    if (file.isEmpty()) {
      return Built::failure(tr("Choose an image file."), QStringLiteral("file"));
    }
    const QFileInfo info(file);
    if (!info.isFile()) {
      return Built::failure(tr("There is no file %1.").arg(file), QStringLiteral("file"));
    }
    if (info.size() > kDecalImageLimit) {
      return Built::failure(tr("The image is %1 MB, more than the %2 MB a design embeds.")
                                .arg(info.size() >> 20)
                                .arg(kDecalImageLimit >> 20),
                            QStringLiteral("file"));
    }
    command.insert(QStringLiteral("cmd"), QStringLiteral("insert_decal"));
    const std::optional<HeifPng> heif = decalHeif(file);
    if (!heif) {
      command.insert(QStringLiteral("path"), info.absoluteFilePath());
      return Built::of({{QStringLiteral("commands"), QJsonArray{command}}});
    }
    if (heif->result == HeifDecoded::Result::NoDecoder) {
      return Built::failure(heifDecoderMissingMessage(), QStringLiteral("file"));
    }
    if (heif->result != HeifDecoded::Result::Decoded) {
      return Built::failure(tr("The image cannot be read: %1.").arg(heif->error), QStringLiteral("file"));
    }
    // Embedded as the PNG file, its own name kept; the size a HEIF file has
    // (96 pixels to the inch) unless a size is typed.
    command.insert(QStringLiteral("data"), QString::fromLatin1(heif->png.toBase64()));
    command.insert(QStringLiteral("file_name"), info.fileName());
    if (!command.contains(QStringLiteral("width")) && !command.contains(QStringLiteral("height"))) {
      command.insert(QStringLiteral("width"), kDecalUnstatedPixelSize * heif->width);
    }
    return Built::of({{QStringLiteral("commands"), QJsonArray{command}}});
  };
  // The live result while the panel is open: the decal drawn on the face
  // where the values put it, an edited one in place of where it was, and
  // its border (as the model's decal_outline puts it). Asked again with the
  // handles, after each change of the values.
  def.viewLines = [this, open](const CommandState& state, const CommandContext& context) {
    std::vector<std::vector<gp_Pnt>> lines;
    const std::optional<DecalPlaced> placed = decalPlaced(state, context);
    if (*open) {
      std::optional<DecalPanel> panel;
      if (placed) {
        QJsonObject entry = placed->outline;
        const QJsonObject decal = state.object(QStringLiteral("decal"));
        const bool editing = editingDecal(state);
        const QString file = editing ? QString() : QFileInfo(state.text(QStringLiteral("file")).trimmed()).absoluteFilePath();
        entry.insert(QStringLiteral("name"),
                     editing ? decal.value(QStringLiteral("name")).toString() : QFileInfo(file).fileName());
        entry.insert(QStringLiteral("sha256"), decal.value(QStringLiteral("sha256")).toString());
        entry.insert(QStringLiteral("width"), placed->width);
        entry.insert(QStringLiteral("height"), placed->height);
        entry.insert(QStringLiteral("angle"), state.value(QStringLiteral("angle")) * 180.0 / kDecalPi);
        entry.insert(QStringLiteral("flip"), state.checked(QStringLiteral("flip")));
        // An opacity that is no percentage yet: opaque.
        const double opacity = state.value(QStringLiteral("opacity"));
        entry.insert(QStringLiteral("opacity"), std::isfinite(opacity) ? std::clamp(opacity / 100.0, 0.0, 1.0) : 1.0);
        entry.insert(QStringLiteral("pixels"), QJsonArray{placed->source.columns, placed->source.rows});
        if (const std::optional<DecalShown> shown = decalOf(entry)) {
          panel.emplace();
          panel->decal = *shown;
          panel->editing = editing;
          panel->file = file;
        }
      }
      setDecalPanel(panel);
    }
    if (placed) {
      std::vector<gp_Pnt> border = decalBorder(placed->outline);
      if (!border.empty()) {
        lines.push_back(std::move(border));
      }
    }
    return lines;
  };
  def.load = [open](const QJsonObject& decal, CommandState& state, const CommandContext& context) {
    *open = true;
    state.setObject(QStringLiteral("decal"), decal);
    // A face that is gone (`error`) is left empty: the decal keeps it until
    // another face is picked.
    SelectionItem face = itemOf(decal.value(QStringLiteral("face")), context);
    if (face.isValid() && !decal.contains(QStringLiteral("error"))) {
      face.occurrence = decal.value(QStringLiteral("occurrence")).toString();
      face.geometry = decal.value(QStringLiteral("surface")).toString();
      state.items(QStringLiteral("face")) = {face};
    }
    const QJsonArray center = decal.value(QStringLiteral("center")).toArray();
    state.setText(QStringLiteral("x"), lengthText(center.at(0).toDouble(), context));
    state.setText(QStringLiteral("y"), lengthText(center.at(1).toDouble(), context));
    state.setText(QStringLiteral("angle"), plainNumber(decal.value(QStringLiteral("angle")).toDouble()) + QStringLiteral(" deg"));
    state.setChecked(QStringLiteral("flip"), decal.value(QStringLiteral("flip")).toBool());
    const bool locked = decal.value(QStringLiteral("lock_aspect")).toBool(true);
    state.setChecked(QStringLiteral("lock"), locked);
    state.setText(QStringLiteral("width"), lengthText(decal.value(QStringLiteral("width")).toDouble(), context));
    state.setText(QStringLiteral("height"),
                  locked ? QString() : lengthText(decal.value(QStringLiteral("height")).toDouble(), context));
    state.setText(QStringLiteral("opacity"),
                  plainNumber(std::round(decal.value(QStringLiteral("opacity")).toDouble(1.0) * 1000.0) / 10.0));
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    if (editingDecal(state)) {
      return QStringLiteral("Edited decal %1")
          .arg(state.object(QStringLiteral("decal")).value(QStringLiteral("name")).toString());
    }
    return QStringLiteral("Inserted decal from %1").arg(QFileInfo(state.text(QStringLiteral("file")).trimmed()).fileName());
  };
  return def;
}

void MainWindow::registerDecalCommands() { m_registry->add(decalCommand()); }

void MainWindow::followDecals() {
  const std::vector<DecalShown> shown = shownDecals(m_snapshot.decals);
  QHash<QString, QImage> images;
  for (const DecalShown& decal : shown) {
    if (images.contains(decal.sha256)) {
      continue;
    }
    QImage picture = m_decalImages.value(decal.sha256);
    if (picture.isNull()) {
      picture = decalPicture(decal.name, decal.sha256);
    }
    images.insert(decal.sha256, picture);
  }
  m_decalImages = images;
  drawDecals();
}

QImage MainWindow::decalPicture(const QString& name, const QString& sha256) {
  QImage picture;
  QString why;
  try {
    const QJsonObject file = queryObject({{QStringLiteral("query"), QStringLiteral("decal_image")},
                                          {QStringLiteral("name"), name}});
    picture = decodeImageFile(QByteArray::fromBase64(file.value(QStringLiteral("data")).toString().toLatin1()), &why);
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Decal %1 not read: %2").arg(name, errorText(e));
  }
  if (picture.isNull()) {
    qWarning().noquote() << QStringLiteral("Decal %1 cannot be drawn: %2").arg(name, why);
  } else {
    qDebug().noquote() << QStringLiteral("Decal %1 decoded: %2 x %3 pixels, %4")
                              .arg(name)
                              .arg(picture.width())
                              .arg(picture.height())
                              .arg(sha256.left(12));
  }
  return picture;
}

void MainWindow::setDecalPanel(const std::optional<DecalPanel>& panel) {
  if (!panel) {
    if (!m_decalPanel && m_decalPanelSource.isEmpty()) {
      return;
    }
    m_decalPanel.reset();
    m_decalPanelSource.clear();
    m_decalPanelPicture = QImage();
    drawDecals();
    return;
  }
  DecalPanel next = *panel;
  next.decal.panel = true; // logged as the panel's
  // Its picture: an edited decal's from the design, a new one's from its
  // file (as it is now), decoded once.
  QString source;
  if (next.editing) {
    source = QStringLiteral("decal/") + next.decal.sha256;
  } else {
    const QFileInfo info(next.file);
    source = QStringLiteral("file/%1/%2/%3")
                 .arg(info.absoluteFilePath())
                 .arg(info.size())
                 .arg(info.lastModified().toMSecsSinceEpoch());
  }
  if (source != m_decalPanelSource) {
    m_decalPanelSource = source;
    if (next.editing) {
      m_decalPanelPicture = m_decalImages.value(next.decal.sha256);
      if (m_decalPanelPicture.isNull()) {
        m_decalPanelPicture = decalPicture(next.decal.name, next.decal.sha256);
      }
    } else {
      // A file larger than a design embeds is not read: the panel refuses it.
      QFile file(next.file);
      QString why;
      m_decalPanelPicture = QImage();
      if (file.size() <= kDecalImageLimit && file.open(QIODevice::ReadOnly)) {
        m_decalPanelPicture = decodeImageFile(file.readAll(), &why);
      }
      if (m_decalPanelPicture.isNull()) {
        qDebug().noquote() << QStringLiteral("Decal panel image %1 not drawn: %2")
                                  .arg(next.decal.name, why.isEmpty() ? QStringLiteral("not read") : why);
      }
    }
  }
  if (!next.editing) {
    next.decal.sha256 = QStringLiteral("panel/") + source; // its picture's and its texture's key
  }
  // Drawn again only when it moved or looks otherwise.
  if (m_decalPanel) {
    const DecalShown& was = m_decalPanel->decal;
    const DecalShown& now = next.decal;
    if (was.name == now.name && was.sha256 == now.sha256 && was.points == now.points &&
        was.opacity == now.opacity && m_decalPanel->editing == next.editing) {
      return;
    }
  }
  m_decalPanel = next;
  // For UI tests: where it is drawn, its centre in the design.
  // (The grid's first and last corners: points where the face's edges cut
  // it follow the grid's.)
  const std::vector<DecalPoint>& points = next.decal.points;
  const DecalPoint& first = points.front();
  const DecalPoint& last = points[next.decal.at(next.decal.rows, next.decal.columns)];
  const auto middle = [&first, &last](std::size_t axis) {
    return QString::number((first[axis] + last[axis]) / 2.0, 'f', 6);
  };
  qDebug().noquote() << QStringLiteral("Decal panel decal %1 on a %2 centre %3,%4,%5%6")
                            .arg(next.decal.name, next.decal.surface, middle(0), middle(1), middle(2),
                                 next.editing ? QStringLiteral(", in place of the decal") : QString());
  drawDecals();
}

void MainWindow::drawDecals() {
  std::vector<DecalShown> shown = shownDecals(m_snapshot.decals);
  QHash<QString, QImage> images = m_decalImages;
  if (m_decalPanel) {
    // The panel's decal in place of the one it edits, drawn where the
    // panel's values put it.
    const DecalPanel& panel = *m_decalPanel;
    if (panel.editing) {
      shown.erase(std::remove_if(shown.begin(), shown.end(),
                                 [&panel](const DecalShown& decal) { return decal.name == panel.decal.name; }),
                  shown.end());
    }
    if (!m_decalPanelPicture.isNull()) {
      images.insert(panel.decal.sha256, m_decalPanelPicture);
      shown.push_back(panel.decal);
    }
  }
  m_viewer->setDecals(shown, images);
}

void MainWindow::editDecal(const QString& name) {
  if (m_mode != Mode::Idle) {
    return;
  }
  const QJsonObject decal = m_snapshot.decal(name);
  const CommandDef* def = m_registry->find(kDecalId);
  if (decal.isEmpty() || def == nullptr) {
    return;
  }
  startFeatureCommand(*def, QString(), decal);
}

QString MainWindow::decalUnder(const SelectionItem& picked, const QPoint& globalPosition) const {
  if (picked.kind != SelectKind::Face) {
    return QString();
  }
  const std::vector<DecalShown> shown = shownDecals(m_snapshot.decals);
  if (shown.empty()) {
    return QString();
  }
  // The decal under the click, by the model point there; without one, the
  // face's only decal.
  if (const std::optional<gp_Pnt> at = m_viewer->contextMenuPoint(globalPosition)) {
    const std::optional<QString> found = decalAt(shown, {at->X(), at->Y(), at->Z()}, 0.2);
    return found.value_or(QString());
  }
  QString only;
  for (const DecalShown& decal : shown) {
    if (decal.body == picked.owner && decal.face == picked.name && decal.occurrence == picked.occurrence) {
      if (!only.isEmpty()) {
        return QString();
      }
      only = decal.name;
    }
  }
  return only;
}

} // namespace mitcad
