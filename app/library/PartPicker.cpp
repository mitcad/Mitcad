// SPDX-License-Identifier: MIT
#include "PartPicker.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <utility>

#include <QAction>
#include <QApplication>
#include <QByteArray>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QEvent>
#include <QFontMetrics>
#include <QFrame>
#include <QGuiApplication>
#include <QHBoxLayout>
#include <QHelpEvent>
#include <QItemSelectionModel>
#include <QJsonArray>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QKeySequence>
#include <QLabel>
#include <QLineEdit>
#include <QListView>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QPushButton>
#include <QRegularExpression>
#include <QScreen>
#include <QSettings>
#include <QSignalBlocker>
#include <QSlider>
#include <QStyle>
#include <QStyledItemDelegate>
#include <QToolButton>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>
#include <QVBoxLayout>
#include <QtLogging>

#include "../build/BuildCatalog.hpp"
#include "../files/Libraries.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Json.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/WindowStyle.hpp"
#include "../platform/MacDialogs.hpp"
#include "OptionChooser.hpp"
#include "PartFavourites.hpp"
#include "PartOptions.hpp"

namespace mitcad {
namespace {

constexpr double kPi = 3.14159265358979323846;
constexpr int kMinTile = 48;
constexpr int kMaxTile = 192;
constexpr int kDefaultTile = 96;
constexpr int kHoverImage = 256;

const QString kTileSizeKey = QStringLiteral("partPicker/tileSize");
const QString kGeometryKey = QStringLiteral("partPicker/geometry");
// The groups the user folded (mitcad#527), by key: a JSON array, as the
// stars (PartFavourites.cpp). Every picker's tree reads it when it is made.
const QString kFoldedKey = QStringLiteral("partPicker/foldedGroups");

QSet<QString> foldedGroups() {
  QSet<QString> folded;
  for (const QJsonValue& value : QJsonDocument::fromJson(QSettings().value(kFoldedKey).toByteArray()).array()) {
    if (!value.toString().isEmpty()) {
      folded.insert(value.toString());
    }
  }
  return folded;
}

void setGroupFolded(const QString& key, bool folded) {
  QSet<QString> all = foldedGroups();
  if (all.contains(key) == folded) {
    return;
  }
  if (folded) {
    all.insert(key);
  } else {
    all.remove(key);
  }
  QStringList keys(all.begin(), all.end());
  keys.sort();
  QSettings().setValue(kFoldedKey, QJsonDocument(QJsonArray::fromStringList(keys)).toJson(QJsonDocument::Compact));
}

QColor starColour() { return QColor(232, 170, 0); }

// "<library>|<component>": how the design's parts know a part (a star
// knows a tile's row too, starredTile).
QString partId(const PickerPart& part) { return part.library + QLatin1Char('|') + part.component; }

QString partLabel(const PickerPart& part) { return part.library + QLatin1Char('/') + part.component; }

// Whether the tile is starred: a tile of a configuration row by its row
// (mitcad#491).
bool starredTile(const PickerPart& part) {
  return PartFavourites::instance().starred(part.library, part.component, part.config, part.defaultRow);
}

// The label with its row, as the tiles are logged: each row is a tile of
// its own (mitcad#484).
QString tileLabel(const PickerPart& part) {
  return part.config.isEmpty() ? partLabel(part) : QStringLiteral("%1 (%2)").arg(partLabel(part), part.config);
}

// The pictures read, by part and version, for every grid of the session; a
// row with a picture of its own (mitcad#484) by its row too.
QString imageKey(const PickerPart& part) {
  const QString row = part.rowPreview ? part.config : QString();
  return QStringList{part.library, part.component, part.rev, part.url, row}.join(QLatin1Char('|'));
}

QHash<QString, QImage>& imageCache() {
  static QHash<QString, QImage> cache;
  return cache;
}

bool hasImage(const PickerPart& part) { return part.preview && !part.libraryEntry && !part.component.isEmpty(); }

// The part's picture (library_preview), read once; null when it has none.
QImage readImage(const PickerPart& part) {
  QHash<QString, QImage>& cache = imageCache();
  const QString key = imageKey(part);
  const auto found = cache.constFind(key);
  if (found != cache.cend()) {
    return found.value();
  }
  QImage image;
  if (hasImage(part)) {
    QJsonObject request{{QStringLiteral("cmd"), QStringLiteral("library_preview")},
                        {QStringLiteral("component"), part.component}};
    if (part.rowPreview) {
      request.insert(QStringLiteral("config"), part.config);
    }
    if (!part.url.isEmpty()) {
      request.insert(QStringLiteral("url"), part.url);
    }
    if (!part.library.isEmpty()) {
      request.insert(QStringLiteral("id"), part.library);
    }
    if (!part.rev.isEmpty()) {
      request.insert(QStringLiteral("rev"), part.rev);
    }
    const QJsonValue png = libraryCommand(request).value(QStringLiteral("png"));
    if (png.isString()) {
      image = QImage::fromData(QByteArray::fromBase64(png.toString().toLatin1()), "PNG");
    }
    // A row's own picture with its row (mitcad#484).
    const QString size =
        image.isNull() ? QStringLiteral("none") : QStringLiteral("%1x%2").arg(image.width()).arg(image.height());
    qDebug().noquote() << QStringLiteral("Part picker picture of %1: %2")
                              .arg(part.rowPreview ? tileLabel(part) : partLabel(part), size);
  }
  cache.insert(key, image);
  return image;
}

// What the hover tells of a tile's part: its name, sizes and system,
// supplier, part number, library, licence, its uses in the design and why
// it does not fit. Rich text for the Qt hover; plain lines for the native
// grid's tooltip and hover card (mitcad#813).
QString partDetails(const PickerPart& shown, int uses, bool rich) {
  const auto line = [rich](const QString& label, const QString& value) {
    if (value.isEmpty()) {
      return QString();
    }
    return rich ? QStringLiteral("<br><b>%1</b> %2").arg(label, value.toHtmlEscaped())
                : QStringLiteral("\n%1 %2").arg(label, value);
  };
  QString text = rich ? QStringLiteral("<b>%1</b>").arg(shown.name.toHtmlEscaped()) : shown.name;
  QStringList about;
  for (const QString& one : {shown.sizes, shown.system}) {
    if (!one.isEmpty() && !about.contains(one)) {
      about << one;
    }
  }
  if (!about.isEmpty()) {
    const QString joined = about.join(QStringLiteral(", "));
    text += rich ? QStringLiteral("<br>") + joined.toHtmlEscaped() : QStringLiteral("\n") + joined;
  }
  text += line(PartGrid::tr("Supplier:"), shown.supplier);
  text += line(PartGrid::tr("Part number:"), shown.partNumber);
  // The suppliers of its rows (mitcad#949).
  QList<PartSupplier> others;
  for (const PartSupplier& one : shown.suppliers) {
    if (one.id != shown.supplier) {
      others.append(one);
    }
  }
  text += line(PartGrid::tr("Sold by:"), supplierNames(others));
  const QString library = shown.libraryName.isEmpty() || shown.libraryName == shown.library
                              ? shown.library
                              : QStringLiteral("%1 (%2)").arg(shown.libraryName, shown.library);
  text += line(PartGrid::tr("Library:"), QStringList{library, shown.version}.join(QLatin1Char(' ')).trimmed());
  text += line(PartGrid::tr("Licence:"), shown.license.isEmpty() ? PartGrid::tr("none") : shown.license);
  if (uses > 0) {
    text += line(PartGrid::tr("In this design:"), QString(QChar(0x00D7)) + QString::number(uses));
  }
  if (!shown.refused.isEmpty()) {
    const QString refused = PartGrid::tr("Does not fit: %1").arg(shown.refused);
    text += rich ? QStringLiteral("<br><i>%1</i>").arg(refused.toHtmlEscaped()) : QStringLiteral("\n") + refused;
  }
  return text;
}

// A five-pointed star in `box`.
QPainterPath starPath(const QRectF& box) {
  QPainterPath path;
  const QPointF centre = box.center();
  const double outer = std::min(box.width(), box.height()) / 2.0;
  const double inner = outer * 0.45;
  for (int i = 0; i < 10; ++i) {
    const double radius = i % 2 == 0 ? outer : inner;
    const double angle = -kPi / 2.0 + i * kPi / 5.0;
    const QPointF point(centre.x() + radius * std::cos(angle), centre.y() + radius * std::sin(angle));
    if (i == 0) {
      path.moveTo(point);
    } else {
      path.lineTo(point);
    }
  }
  path.closeSubpath();
  return path;
}

// A filled star when on, else its outline.
void paintStar(QPainter* painter, const QRectF& box, bool on, const QColor& outline) {
  painter->save();
  painter->setRenderHint(QPainter::Antialiasing);
  const QPainterPath path = starPath(box.adjusted(1.0, 1.0, -1.0, -1.0));
  if (on) {
    painter->fillPath(path, starColour());
    painter->setPen(QPen(starColour().darker(130), 1.0));
  } else {
    painter->setPen(QPen(outline, 1.2));
  }
  painter->setBrush(Qt::NoBrush);
  painter->drawPath(path);
  painter->restore();
}

QIcon starIcon(bool on, const QColor& outline) {
  QPixmap pixmap(16, 16);
  pixmap.fill(Qt::transparent);
  QPainter painter(&pixmap);
  paintStar(&painter, QRectF(0.0, 0.0, 16.0, 16.0), on, outline);
  painter.end();
  return QIcon(pixmap);
}

QIcon blankIcon() {
  QPixmap pixmap(16, 16);
  pixmap.fill(Qt::transparent);
  return QIcon(pixmap);
}

#ifdef Q_OS_MACOS
constexpr int kGroupStarRole = Qt::UserRole + 71;

// Keep the star's hit area in place without filling the whole sidebar
// with unchecked stars. The source-list row delegate retains its band.
class GroupDelegate final : public PanelRowDelegate {
public:
  using PanelRowDelegate::PanelRowDelegate;

protected:
  void initStyleOption(QStyleOptionViewItem* option, const QModelIndex& index) const override {
    QStyledItemDelegate::initStyleOption(option, index);
    const QVariant starred = index.data(kGroupStarRole);
    if (starred.isValid()) {
      option->icon = starred.toBool() || option->state.testFlag(QStyle::State_MouseOver)
                         ? starIcon(starred.toBool(), option->palette.color(QPalette::PlaceholderText))
                         : blankIcon();
    }
  }
};
#endif

// A tile: the picture at the top, the name and the sizes under it.
QSize tileBox(int size, int lineHeight) { return QSize(size + 24, size + 2 * lineHeight + 18); }

QRect imageRect(const QRect& tile, int size) {
  return QRect(tile.left() + (tile.width() - size) / 2, tile.top() + 6, size, size);
}

QRect starRect(const QRect& tile, int size) {
  const int side = std::clamp(size / 5, 14, 22);
  const QRect image = imageRect(tile, size);
  return QRect(image.right() - side + 1, image.top(), side, side);
}

class TileDelegate : public QStyledItemDelegate {
public:
  TileDelegate(const int* tile, QObject* parent) : QStyledItemDelegate(parent), m_tile(tile) {}

  QSize sizeHint(const QStyleOptionViewItem& option, const QModelIndex&) const override {
    return tileBox(*m_tile, option.fontMetrics.height());
  }

  void paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    QStyleOptionViewItem panel = option;
    initStyleOption(&panel, index);
    panel.text.clear();
    panel.icon = QIcon();
    const QWidget* widget = option.widget;
    QStyle* style = widget != nullptr ? widget->style() : QApplication::style();
#ifndef Q_OS_MACOS
    style->drawPrimitive(QStyle::PE_PanelItemViewItem, &panel, painter, widget);
#else
    Q_UNUSED(style);
#endif

    const int size = *m_tile;
    const QRect tile = option.rect;
    const QRect image = imageRect(tile, size);
    const bool selected = option.state.testFlag(QStyle::State_Selected);
    const QPalette& palette = option.palette;
    const QColor text = palette.color(selected ? QPalette::HighlightedText : QPalette::Text);
    const QColor faint =
#ifdef Q_OS_MACOS
        palette.color(QPalette::PlaceholderText);
#else
        selected ? text : palette.color(QPalette::PlaceholderText);
#endif
    painter->save();
    if (!index.data(PartGridModel::kRefusedRole).toString().isEmpty()) {
      painter->setOpacity(0.38);
    }
#ifdef Q_OS_MACOS
    if (selected) {
      QColor fill = palette.color(QPalette::Highlight);
      fill.setAlpha(38);
      painter->setRenderHint(QPainter::Antialiasing);
      painter->setPen(QPen(palette.color(QPalette::Highlight), 1));
      painter->setBrush(fill);
      painter->drawRoundedRect(QRectF(image).adjusted(-4, -4, 4, 4), 8, 8);
    }
#endif
    const QPixmap picture = index.data(PartGridModel::kImageRole).value<QPixmap>();
    if (picture.isNull()) {
      painter->setRenderHint(QPainter::Antialiasing);
      painter->setPen(Qt::NoPen);
      painter->setBrush(QColor(128, 128, 128, 40));
      painter->drawRoundedRect(QRectF(image).adjusted(4.0, 4.0, -4.0, -4.0), 6.0, 6.0);
    } else {
      const QSize shown = picture.size() / picture.devicePixelRatio();
      painter->drawPixmap(image.left() + (size - shown.width()) / 2, image.top() + (size - shown.height()) / 2, picture);
    }
    const QFontMetrics metrics = option.fontMetrics;
    const int line = metrics.height();
    const QRect nameRect(tile.left() + 3, image.bottom() + 5, tile.width() - 6, line);
    const QString name = metrics.elidedText(index.data(Qt::DisplayRole).toString(), Qt::ElideRight, nameRect.width());
#ifdef Q_OS_MACOS
    if (selected) {
      const int captionWidth = std::min(nameRect.width(), metrics.horizontalAdvance(name) + 12);
      const QRect caption(nameRect.center().x() - captionWidth / 2, nameRect.top(), captionWidth, nameRect.height());
      painter->setPen(Qt::NoPen);
      painter->setBrush(palette.brush(QPalette::Highlight));
      painter->drawRoundedRect(QRectF(caption), 4, 4);
    }
#endif
    painter->setFont(option.font);
    painter->setPen(text);
    painter->drawText(nameRect, Qt::AlignHCenter | Qt::AlignVCenter, name);
    const QString sizes = index.data(PartGridModel::kSizesRole).toString();
    if (!sizes.isEmpty()) {
      QFont smaller = option.font;
      if (smaller.pointSizeF() > 0.0) {
        smaller.setPointSizeF(smaller.pointSizeF() * 0.9);
      }
      const QFontMetrics smallMetrics(smaller);
      const QRect sizesRect(tile.left() + 3, nameRect.bottom() + 1, tile.width() - 6, line);
      painter->setFont(smaller);
      painter->setPen(faint);
      painter->drawText(sizesRect, Qt::AlignHCenter | Qt::AlignVCenter,
                        smallMetrics.elidedText(sizes, Qt::ElideRight, sizesRect.width()));
    }
    // How many times the design uses it (In this design).
    const int count = index.data(PartGridModel::kCountRole).toInt();
    if (count > 0) {
      const QString times = QString(QChar(0x00D7)) + QString::number(count);
      painter->setFont(option.font);
      const QRect badge(image.left(), image.top(), metrics.horizontalAdvance(times) + 8, line + 2);
      painter->setRenderHint(QPainter::Antialiasing);
      painter->setPen(Qt::NoPen);
      QColor fill = palette.color(QPalette::Highlight);
      fill.setAlpha(210);
      painter->setBrush(fill);
      painter->drawRoundedRect(QRectF(badge), 4.0, 4.0);
      painter->setPen(palette.color(QPalette::HighlightedText));
      painter->drawText(badge, Qt::AlignCenter, times);
    }
    const QVariant starred = index.data(PartGridModel::kStarRole);
    if (starred.isValid()) {
      paintStar(painter, QRectF(starRect(tile, size)), starred.toBool(), faint);
    }
    painter->restore();
  }

private:
  const int* m_tile;
};

} // namespace

// ---------------------------------------------------------------------------
// Parts from the library commands

PickerPart pickerPart(const QJsonObject& component) {
  PickerPart part;
  part.library = str(component, "library");
  part.libraryName = str(component, "library_name");
  part.url = str(component, "url");
  part.rev = str(component, "rev");
  part.version = str(component, "version");
  part.component = str(component, "id");
  if (part.component.isEmpty()) {
    part.component = str(component, "component");
  }
  part.name = str(component, "name");
  part.category = str(component, "category");
  // Its categories and their groups (mitcad#527): {category, path, order}.
  for (const QJsonValue& value : component.value(QStringLiteral("categories")).toArray()) {
    if (!value.toString().isEmpty() && !part.categories.contains(value.toString())) {
      part.categories << value.toString();
    }
  }
  if (part.categories.isEmpty() && !part.category.isEmpty()) {
    part.categories << part.category;
  }
  for (const QJsonValue& value : component.value(QStringLiteral("groups")).toArray()) {
    const QJsonObject group = value.toObject();
    PartGroupPath path;
    path.category = str(group, "category");
    for (const QJsonValue& name : group.value(QStringLiteral("path")).toArray()) {
      path.names << name.toString();
    }
    for (const QJsonValue& at : group.value(QStringLiteral("order")).toArray()) {
      path.order << at.toInt();
    }
    if (!groupKey(path.names).isEmpty()) {
      part.groups.append(path);
    }
  }
  if (part.groups.isEmpty()) {
    // An answer without groups: each category a group at the top.
    for (const QString& category : part.categories) {
      part.groups.append(PartGroupPath{category, {category}, {0}});
    }
  }
  part.standard = str(component, "standard");
  part.description = str(component, "description");
  part.designation = str(component, "designation");
  for (const QJsonValue& keyword : component.value(QStringLiteral("keywords")).toArray()) {
    part.keywords << keyword.toString();
  }
  part.supplier = str(component, "supplier");
  part.partNumber = str(component, "part_number");
  part.suppliers = componentSuppliers(component);
  part.license = str(component, "license");
  const QJsonValue preview = component.value(QStringLiteral("preview"));
  part.preview = preview.isBool() ? preview.toBool() : true;
  part.table = component.value(QStringLiteral("configurations")).toObject();
  build::Part shape;
  shape.build = component.value(QStringLiteral("build")).toObject();
  part.system = shape.systemText();
  part.sizes = shape.sizeText();
  if (part.sizes.isEmpty()) {
    part.sizes = part.standard;
  }
  if (part.name.isEmpty()) {
    part.name = part.component;
  }
  part.key = partId(part);
  return part;
}

QList<PickerPart> pickerParts(const QJsonObject& answer) {
  QList<PickerPart> parts;
  QHash<QString, PickerPart> libraries;
  for (const QJsonValue& value : answer.value(QStringLiteral("components")).toArray()) {
    const PickerPart part = pickerPart(value.toObject());
    libraries.insert(part.library, part);
    parts.append(part);
  }
  for (const QJsonValue& value : answer.value(QStringLiteral("refused")).toArray()) {
    // {id, name, library, reason}
    const QJsonObject entry = value.toObject();
    PickerPart part;
    part.library = str(entry, "library");
    part.component = str(entry, "id");
    part.name = str(entry, "name").isEmpty() ? part.component : str(entry, "name");
    part.refused = str(entry, "reason").isEmpty() ? QObject::tr("does not fit") : str(entry, "reason");
    const auto known = libraries.constFind(part.library);
    if (known != libraries.cend()) {
      part.libraryName = known->libraryName;
      part.url = known->url;
      part.rev = known->rev;
      part.version = known->version;
    }
    part.key = partId(part);
    parts.append(part);
  }
  return parts;
}

PickerPart pickerPart(const build::Part& source) {
  PickerPart part;
  part.key = source.key;
  part.library = source.library;
  part.url = source.url;
  part.rev = source.rev;
  part.version = source.version;
  part.component = source.component;
  part.config = source.config;
  part.rowPreview = source.ownPreview;
  part.defaultRow = source.defaultRow;
  part.table = source.table;
  part.name = source.name;
  part.designation = source.designation;
  part.supplier = source.supplier;
  part.partNumber = source.partNumber;
  // A tile of one row: the suppliers of that row (mitcad#949).
  part.suppliers = rowPartSuppliers(source.table, source.config, source.supplier);
  part.system = source.systemText();
  QStringList sizes;
  if (!source.sizeText().isEmpty()) {
    sizes << source.sizeText();
  }
  if (!source.config.isEmpty() && !sizes.contains(source.config)) {
    sizes << source.config;
  }
  // Timber's less common sizes are marked (mitcad#478); the standard
  // ones come first in the catalog.
  if (source.lessCommon()) {
    sizes << QObject::tr("less common");
  }
  part.sizes = sizes.join(QStringLiteral(", "));
  return part;
}

QList<PickerPart> pickerParts(const QVector<build::Part>& parts) {
  // The fields a BUILD part does not keep: one search of every library.
  QHash<QString, PickerPart> known;
  const QJsonObject answer = libraryCommand({{QStringLiteral("cmd"), QStringLiteral("library_search")},
                                             {QStringLiteral("unlicensed"), true}});
  for (const QJsonValue& value : answer.value(QStringLiteral("components")).toArray()) {
    const PickerPart found = pickerPart(value.toObject());
    known.insert(partId(found), found);
  }
  QList<PickerPart> out;
  for (const build::Part& source : parts) {
    PickerPart part = pickerPart(source);
    const auto found = known.constFind(partId(part));
    if (found != known.cend()) {
      part.libraryName = found->libraryName;
      part.category = found->category;
      part.categories = found->categories;
      part.groups = found->groups;
      part.standard = found->standard;
      part.description = found->description;
      part.keywords = found->keywords;
      part.license = found->license;
      part.preview = found->preview;
      // A tile of the component (no row): its rows' suppliers.
      if (part.config.isEmpty()) {
        part.suppliers = supplierChoices({part.suppliers, found->suppliers});
      }
    }
    out.append(part);
  }
  return out;
}

QList<PickerPart> buildCandidates(const QJsonObject& fields) {
  QJsonObject request = fields;
  request.insert(QStringLiteral("cmd"), QStringLiteral("library_build_candidates"));
  const QJsonObject answer = libraryCommand(request);
  if (answer.contains(QStringLiteral("error"))) {
    qWarning().noquote() << QStringLiteral("Part picker: no candidates: %1")
                                .arg(answer.value(QStringLiteral("error"))
                                         .toObject()
                                         .value(QStringLiteral("message"))
                                         .toString());
  }
  return pickerParts(answer);
}

// ---------------------------------------------------------------------------
// PartGridModel

PartGridModel::PartGridModel(QObject* parent) : QAbstractListModel(parent) {
  // The pictures come a few at a time while the application is idle, so
  // that a grid of hundreds of parts never waits for them.
  m_loader.setInterval(0);
  connect(&m_loader, &QTimer::timeout, this, [this] { loadSome(); });
}

void PartGridModel::setShown(const QList<PickerPart>& shownParts, const QHash<QString, int>& counts) {
  beginResetModel();
  m_shown = shownParts;
  m_counts = counts;
  m_wanted.clear();
  m_waiting.clear();
  endResetModel();
}

void PartGridModel::refreshStars() {
  if (!m_shown.isEmpty()) {
    emit dataChanged(index(0), index(static_cast<int>(m_shown.size()) - 1), {kStarRole});
  }
}

const PickerPart* PartGridModel::part(int row) const {
  return row >= 0 && row < m_shown.size() ? &m_shown.at(row) : nullptr;
}

int PartGridModel::rowOf(const QString& key) const {
  if (key.isEmpty()) {
    return -1;
  }
  for (int row = 0; row < m_shown.size(); ++row) {
    if (m_shown.at(row).key == key) {
      return row;
    }
  }
  return -1;
}

int PartGridModel::rowCount(const QModelIndex& parent) const {
  return parent.isValid() ? 0 : static_cast<int>(m_shown.size());
}

QVariant PartGridModel::data(const QModelIndex& index, int role) const {
  const PickerPart* shown = part(index.row());
  if (shown == nullptr) {
    return {};
  }
  switch (role) {
  case Qt::DisplayRole:
    return shown->name;
  case Qt::AccessibleTextRole:
    return QStringList{shown->name, shown->sizes}.join(QLatin1Char(' ')).trimmed();
  case Qt::ToolTipRole:
    // The hover's details for the native grid; PartGrid takes the Qt
    // view's tooltips and shows its own hover.
    return partDetails(*shown, m_counts.value(shown->key), false);
  case kPreviewRole:
    return hasImage(*shown) ? QVariant::fromValue(readImage(*shown)) : QVariant();
  case kKeyRole:
    return shown->key;
  case kSizesRole:
    if (shown->libraryEntry) {
      return shown->license.isEmpty() ? tr("Library, no licence") : tr("Library, %1").arg(shown->license);
    }
    return shown->sizes.isEmpty() ? shown->system : shown->sizes;
  case kCountRole:
    return m_counts.value(shown->key);
  case kStarRole:
    if (shown->libraryEntry) {
      return {};
    }
    return starredTile(*shown);
  case kRefusedRole:
    return shown->refused;
  case kImageRole: {
    const QString key = imageKey(*shown);
    const auto scaled = m_scaled.constFind(key);
    if (scaled != m_scaled.cend()) {
      return QVariant::fromValue(scaled.value());
    }
    const QHash<QString, QImage>& cache = imageCache();
    const auto read = cache.constFind(key);
    if (read != cache.cend()) {
      QPixmap tile;
      if (!read->isNull()) {
        // At the screen's pixels, drawn at the tile's size (TileDelegate
        // divides by the ratio; the native grid sets its points): sharp on
        // a high-density screen.
        const qreal ratio = std::max<qreal>(1.0, qApp->devicePixelRatio());
        const int pixels = qRound(m_tileSize * ratio);
        tile = QPixmap::fromImage(read->scaled(pixels, pixels, Qt::KeepAspectRatio, Qt::SmoothTransformation));
        tile.setDevicePixelRatio(ratio);
      }
      m_scaled.insert(key, tile);
      return QVariant::fromValue(tile);
    }
    // Asked the first time a view paints the tile: only the tiles in sight.
    if (hasImage(*shown) && !m_waiting.contains(key)) {
      m_waiting.insert(key, *shown);
      m_wanted.append(key);
      if (!m_loader.isActive()) {
        m_loader.start();
      }
    }
    return {};
  }
  default:
    return {};
  }
}

bool PartGridModel::setData(const QModelIndex& index, const QVariant& value, int role) {
  const PickerPart* shown = part(index.row());
  if (role != kStarRole || shown == nullptr || shown->libraryEntry) {
    return false;
  }
  // Only a request: PartGrid stars the tile (PartFavourites) and the stars
  // are shown again from there (refreshStars).
  if (value.toBool() != starredTile(*shown)) {
    const QString key = shown->key;
    emit starRequested(key);
  }
  return true;
}

QImage PartGridModel::image(const PickerPart& shownPart) const { return readImage(shownPart); }

void PartGridModel::setTileSize(int size) {
  if (size == m_tileSize) {
    return;
  }
  m_tileSize = size;
  m_scaled.clear();
  if (!m_shown.isEmpty()) {
    emit dataChanged(index(0), index(static_cast<int>(m_shown.size()) - 1), {kImageRole});
  }
}

void PartGridModel::loadSome() {
  // The newest wishes first: the tiles in sight after a scroll.
  for (int budget = 4; budget > 0 && !m_wanted.isEmpty(); --budget) {
    const QString key = m_wanted.takeLast();
    const auto waiting = m_waiting.find(key);
    if (waiting == m_waiting.end()) {
      continue;
    }
    const PickerPart wanted = waiting.value();
    m_waiting.erase(waiting);
    readImage(wanted);
    for (int row = 0; row < m_shown.size(); ++row) {
      if (imageKey(m_shown.at(row)) == key) {
        const QModelIndex changed = index(row);
        emit dataChanged(changed, changed, {kImageRole});
      }
    }
  }
  if (m_wanted.isEmpty()) {
    m_loader.stop();
  }
}

// ---------------------------------------------------------------------------
// PartGrid

const QString PartGrid::kFavourites = QStringLiteral(":favourites");
const QString PartGrid::kInDesign = QStringLiteral(":design");
const QString PartGrid::kAll = QStringLiteral(":all");
const QString PartGrid::kLibraries = QStringLiteral(":libraries");
const QString PartGrid::kOther = QStringLiteral(":other");

PartGrid::PartGrid(QWidget* parent) : QWidget(parent), m_group(kAll) {
  m_tile = std::clamp(QSettings().value(kTileSizeKey, kDefaultTile).toInt(), kMinTile, kMaxTile);
  auto* layout = new QHBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);

  auto* left = new QVBoxLayout;
  m_groups = new QTreeWidget;
  m_groups->setObjectName(QStringLiteral("partPickerGroups"));
  m_groups->setHeaderHidden(true);
  m_groups->setColumnCount(1);
  m_groups->setRootIsDecorated(true);
  m_groups->setIndentation(14);
  m_groups->setUniformRowHeights(true);
  m_groups->setIconSize(QSize(16, 16));
  m_groups->setMinimumWidth(160);
  m_groups->setMaximumWidth(240);
  m_groups->setToolTip(tr("A group lists its subgroups' parts; its star stars them all and puts them first (Ctrl+Shift+D)"));
  mac::styleListTree(m_groups);
#ifdef Q_OS_MACOS
  m_groups->setItemDelegate(new GroupDelegate(m_groups));
  m_groups->setMouseTracking(true);
#endif
  auto* groupsLabel = new QLabel(tr("Gr&oups:"));
  groupsLabel->setBuddy(m_groups);
  left->addWidget(groupsLabel);
#ifdef Q_OS_MACOS
  groupsLabel->hide();
#endif
  left->addWidget(m_groups, 1);
  layout->addLayout(left);

  auto* right = new QVBoxLayout;
  auto* top = new QHBoxLayout;
  m_search = new QLineEdit;
  m_search->setObjectName(QStringLiteral("partPickerSearch"));
  m_search->setPlaceholderText(tr("Search: name, standard, size, supplier, part number"));
  m_search->setClearButtonEnabled(true);
  mac::styleSearchField(m_search, m_search->placeholderText());
  auto* searchLabel = new QLabel(tr("&Search:"));
  searchLabel->setBuddy(m_search);
  top->addWidget(searchLabel);
#ifdef Q_OS_MACOS
  searchLabel->hide();
#endif
  top->addWidget(m_search, 1);
  m_filters = new QWidget;
  auto* filters = new QHBoxLayout(m_filters);
  filters->setContentsMargins(0, 0, 0, 0);
  m_filters->hide();
  top->addWidget(m_filters);
  right->addLayout(top);

  auto* second = new QHBoxLayout;
  m_system = new QComboBox;
  m_system->setObjectName(QStringLiteral("partPickerSystem"));
  m_system->setToolTip(tr("The slot system the parts are made for"));
  m_systemLabel = new QLabel(tr("S&ystem:"));
  m_systemLabel->setBuddy(m_system);
  second->addWidget(m_systemLabel);
  second->addWidget(m_system);
  // The supplier (mitcad#949): kept in the settings.
  m_supplier = new QComboBox;
  m_supplier->setObjectName(QStringLiteral("partPickerSupplier"));
  m_supplier->setToolTip(tr("Only the parts this supplier sells in some size"));
  m_supplierLabel = new QLabel(tr("Supplie&r:"));
  m_supplierLabel->setBuddy(m_supplier);
  m_wantedSupplier = savedSupplierFilter();
  second->addWidget(m_supplierLabel);
  second->addWidget(m_supplier);
  m_supplier->hide();
  m_supplierLabel->hide();
  second->addStretch(1);
  m_star = new QToolButton;
  m_star->setObjectName(QStringLiteral("partPickerStar"));
  m_star->setText(tr("Star"));
  m_star->setIcon(starIcon(true, palette().color(QPalette::PlaceholderText)));
  m_star->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
  m_star->setAutoRaise(true);
  m_star->setToolTip(tr("Star the chosen part or turn its star off (Ctrl+D): favourites come first"));
#ifdef Q_OS_MACOS
  m_star->setCheckable(true);
#endif
  mac::styleIconToggle(m_star, QStringLiteral("star"), QStringLiteral("star.fill"));
  second->addWidget(m_star);
  m_size = new QSlider(Qt::Horizontal);
  m_size->setObjectName(QStringLiteral("partPickerTileSize"));
  m_size->setRange(kMinTile, kMaxTile);
  m_size->setSingleStep(8);
  m_size->setPageStep(32);
  m_size->setValue(m_tile);
  m_size->setFixedWidth(120);
  m_size->setToolTip(tr("The size of the pictures"));
  auto* sizeLabel = new QLabel(tr("&Tiles:"));
  sizeLabel->setBuddy(m_size);
  second->addWidget(sizeLabel);
  second->addWidget(m_size);
  right->addLayout(second);

  m_model = new PartGridModel(this);
  m_model->setTileSize(m_tile);
  m_view = new QListView;
  m_view->setObjectName(QStringLiteral("partPickerGrid"));
  m_view->setModel(m_model);
  m_view->setItemDelegate(new TileDelegate(&m_tile, m_view));
  m_view->setViewMode(QListView::IconMode);
  m_view->setFlow(QListView::LeftToRight);
  m_view->setWrapping(true);
  m_view->setResizeMode(QListView::Adjust);
  m_view->setMovement(QListView::Static);
  m_view->setUniformItemSizes(true);
  m_view->setSelectionMode(QAbstractItemView::SingleSelection);
  m_view->setEditTriggers(QAbstractItemView::NoEditTriggers);
  m_view->setVerticalScrollMode(QAbstractItemView::ScrollPerPixel);
  m_view->setMouseTracking(true);
  m_view->setDragEnabled(false);
  m_view->setMinimumWidth(tileBox(kMaxTile, fontMetrics().height()).width() + 40);
#ifdef Q_OS_MACOS
  m_view->setFrameShape(QFrame::NoFrame);
  m_view->setAttribute(Qt::WA_MacShowFocusRect, false);
#endif
  right->addWidget(m_view, 1);
  layout->addLayout(right, 1);
  setTileSize(m_tile);
  // The native macOS frontend presents this widget as a catalogue: the
  // groups as a source list, these rows and an AppKit tile grid reading
  // the tiles' roles (docs/native-macos-ui.md, mitcad#813). Plain
  // properties: no effect on the Qt presentation.
  setProperty("nativeTileCatalogue", true);
  m_view->setProperty("nativeTileKeyRole", PartGridModel::kKeyRole);
  m_view->setProperty("nativeTileImageRole", PartGridModel::kImageRole);
  m_view->setProperty("nativeTilePreviewRole", PartGridModel::kPreviewRole);
  m_view->setProperty("nativeTileSubtitleRole", PartGridModel::kSizesRole);
  m_view->setProperty("nativeTileStarRole", PartGridModel::kStarRole);
  m_view->setProperty("nativeTileBadgeRole", PartGridModel::kCountRole);
  m_view->setProperty("nativeTileDisabledRole", PartGridModel::kRefusedRole);
  m_groups->setProperty("nativeGroupKeyRole", int(Qt::UserRole));
#ifdef Q_OS_MACOS
  m_groups->setProperty("nativeStarRole", kGroupStarRole);
  m_groups->setProperty("nativeStarMethod", QByteArray("toggleGroupStar"));
#endif

  // The larger picture with the part's supplier, part number, library and
  // licence while the mouse rests on a tile.
  m_hover = new QFrame(this, Qt::ToolTip);
  m_hover->setObjectName(QStringLiteral("partPickerHover"));
  m_hover->setFrameShape(QFrame::StyledPanel);
  m_hover->setAttribute(Qt::WA_TransparentForMouseEvents);
  m_hover->setAttribute(Qt::WA_ShowWithoutActivating);
  auto* hoverLayout = new QVBoxLayout(m_hover);
  m_hoverImage = new QLabel;
  m_hoverImage->setAlignment(Qt::AlignCenter);
  m_hoverText = new QLabel;
  m_hoverText->setTextFormat(Qt::RichText);
  m_hoverText->setWordWrap(true);
  m_hoverText->setMaximumWidth(320);
  hoverLayout->addWidget(m_hoverImage);
  hoverLayout->addWidget(m_hoverText);
  m_hover->hide();

  auto* starAction = new QAction(tr("Star"), this);
  starAction->setShortcut(QKeySequence(QStringLiteral("Ctrl+D")));
  starAction->setShortcutContext(Qt::WidgetWithChildrenShortcut);
  addAction(starAction);
  auto* groupStarAction = new QAction(tr("Star Group"), this);
  groupStarAction->setShortcut(QKeySequence(QStringLiteral("Ctrl+Shift+D")));
  groupStarAction->setShortcutContext(Qt::WidgetWithChildrenShortcut);
  addAction(groupStarAction);
  connect(starAction, &QAction::triggered, this, &PartGrid::toggleStar);
  connect(groupStarAction, &QAction::triggered, this, &PartGrid::toggleGroupStar);
  connect(m_star, &QToolButton::clicked, this, &PartGrid::toggleStar);
  // A tile's star from the native grid (PartGridModel::setData).
  connect(m_model, &PartGridModel::starRequested, this, [this](const QString& key) { star(key); });

  connect(m_groups, &QTreeWidget::currentItemChanged, this, [this](QTreeWidgetItem* current) {
    if (m_filling || current == nullptr) {
      return;
    }
    const QString chosen = current->data(0, Qt::UserRole).toString();
    if (chosen != m_group) {
      m_group = chosen;
      const PartGroupTree::Node* node = m_tree.node(chosen);
      m_groupPath = node != nullptr ? node->group : PartGroupPath();
      refill(true);
    }
  });
  connect(m_groups, &QTreeWidget::itemCollapsed, this, [this](QTreeWidgetItem* item) { groupFolded(item, true); });
  connect(m_groups, &QTreeWidget::itemExpanded, this, [this](QTreeWidgetItem* item) { groupFolded(item, false); });
  connect(m_search, &QLineEdit::textChanged, this, [this](const QString& text) {
    if (!m_externalSearch) {
      qInfo().noquote() << QStringLiteral("Part picker search '%1'").arg(text);
      refill(true);
    }
    emit searchChanged(text);
  });
  connect(m_system, &QComboBox::currentIndexChanged, this, [this] {
    if (m_filling) {
      return;
    }
    const QString shown = m_system->currentData().toString();
    qInfo().noquote() << QStringLiteral("Part picker system %1").arg(shown.isEmpty() ? QStringLiteral("all") : shown);
    refill(true);
  });
  // The supplier (mitcad#949), kept in the settings.
  connect(m_supplier, &QComboBox::currentIndexChanged, this, [this] {
    if (m_filling) {
      return;
    }
    const QString chosen = m_supplier->currentData().toString();
    m_wantedSupplier = chosen;
    saveSupplierFilter(chosen);
    qInfo().noquote() << QStringLiteral("Part picker supplier %1")
                             .arg(chosen.isEmpty() ? QStringLiteral("all") : chosen);
    refill(true);
    emit supplierChanged(chosen);
  });
  connect(m_size, &QSlider::valueChanged, this, [this](int value) {
    setTileSize(value);
    QSettings().setValue(kTileSizeKey, value);
  });
  connect(m_view->selectionModel(), &QItemSelectionModel::currentChanged, this, [this](const QModelIndex& index) {
    if (m_filling) {
      return;
    }
    const QString key = index.data(PartGridModel::kKeyRole).toString();
    if (key != m_current) {
      m_current = key;
      refreshStarButton();
      emit currentChanged(m_current);
    }
  });
  connect(m_view, &QListView::clicked, this, [this](const QModelIndex& index) {
    if (m_starHit) {
      m_starHit = false;
      return;
    }
    emit clicked(index.data(PartGridModel::kKeyRole).toString());
  });
  connect(m_view, &QListView::doubleClicked, this, [this](const QModelIndex& index) {
    emit activated(index.data(PartGridModel::kKeyRole).toString());
  });
  // A star changes only the star (mitcad#517): the tiles and the groups
  // stay where they are; the starred-first order applies when the list is
  // made again (the picker opens, the group, the search or the system
  // changes).
  connect(&PartFavourites::instance(), &PartFavourites::changed, this, [this] { refreshStars(); });
  m_search->installEventFilter(this);
  m_view->installEventFilter(this);
  m_view->viewport()->installEventFilter(this);
  m_groups->viewport()->installEventFilter(this);
  fillGroups();
  fillSystems();
  refreshStarButton();
}

void PartGrid::setParts(const QList<PickerPart>& parts) {
  m_parts = parts;
  fillGroups();
  fillSystems();
  fillSuppliers();
  refill(false);
}

void PartGrid::setOtherParts(const QList<PickerPart>& parts) {
  const auto inShown = [this](const PickerPart& part) {
    if (m_group == kInDesign) {
      return !part.libraryEntry && countOf(part) > 0;
    }
    if (m_group == kLibraries) {
      return part.libraryEntry;
    }
    if (m_group == kOther) {
      return !part.libraryEntry && part.groups.isEmpty();
    }
    return inShownGroup(part);
  };
  if (m_group != kAll && m_group != kFavourites && std::none_of(parts.begin(), parts.end(), inShown)) {
    qInfo().noquote() << QStringLiteral("Part picker group %1 has none of the parts: All parts")
                             .arg(groupName(m_group));
    m_group = kAll;
  }
  setParts(parts);
}

void PartGrid::setUsedParts(const QList<UsedPart>& used) {
  m_used.set(used);
  QStringList listed;
  for (const UsedPart& usedPart : used) {
    if (usedPart.count <= 0) {
      continue;
    }
    QStringList rows;
    for (const UsedPart::Row& row : usedPart.rows) {
      rows << QStringLiteral("%1 x%2").arg(row.config).arg(row.count);
    }
    QString entry = QStringLiteral("%1/%2 x%3").arg(usedPart.library, usedPart.component).arg(usedPart.count);
    if (!rows.isEmpty()) {
      entry += QStringLiteral(" [%1]").arg(rows.join(QStringLiteral(", ")));
    }
    listed << entry;
  }
  qInfo().noquote() << QStringLiteral("Part picker in this design: %1 parts%2")
                           .arg(listed.size())
                           .arg(listed.isEmpty() ? QString() : QStringLiteral(": ") + listed.join(QStringLiteral(", ")));
  fillGroups();
  refill(false);
}

void PartGrid::setCurrent(const QString& key, bool notify) {
  const int row = m_model->rowOf(key);
  m_filling = true;
  if (row >= 0) {
    const QModelIndex index = m_model->index(row);
    m_view->setCurrentIndex(index);
    m_view->scrollTo(index);
    m_current = key;
  } else {
    m_view->selectionModel()->clear();
    m_current.clear();
  }
  m_filling = false;
  refreshStarButton();
  if (notify) {
    emit currentChanged(m_current);
  }
}

bool PartGrid::reveal(const QString& key) {
  setCurrent(key);
  if (key.isEmpty() || m_current == key) {
    return !key.isEmpty();
  }
  if (m_group != kAll) {
    qInfo().noquote() << QStringLiteral("Part picker group %1 has not the part chosen: All parts").arg(groupName(m_group));
    m_group = kAll;
    fillGroups();
    refill(false);
    setCurrent(key);
  }
  if (m_current != key && !m_externalSearch && !m_search->text().isEmpty()) {
    qInfo().noquote() << QStringLiteral("Part picker search cleared: it left out the part chosen");
    {
      const QSignalBlocker quiet(m_search);
      m_search->clear();
    }
    refill(false);
    setCurrent(key);
  }
  return m_current == key;
}

void PartGrid::selectFirst(bool notify) {
  const PickerPart* first = m_model->part(0);
  setCurrent(first != nullptr ? first->key : QString(), notify);
}

const PickerPart* PartGrid::currentPart() const { return m_model->part(m_model->rowOf(m_current)); }

void PartGrid::showGroup(const QString& group) {
  QTreeWidgetItem* item = groupItem(group);
  if (item == nullptr) {
    item = groupItem(kAll);
  }
  if (item != nullptr) {
    m_groups->setCurrentItem(item);
  }
}

QTreeWidgetItem* PartGrid::groupItem(const QString& group) const {
  for (QTreeWidgetItemIterator it(m_groups); *it != nullptr; ++it) {
    if ((*it)->data(0, Qt::UserRole).toString() == group) {
      return *it;
    }
  }
  return nullptr;
}

bool PartGrid::inShownGroup(const PickerPart& part) const {
  return !part.libraryEntry && inGroup(part.groups, m_group);
}

void PartGrid::groupFolded(QTreeWidgetItem* item, bool folded) {
  if (m_filling || item == nullptr) {
    return;
  }
  const QString key = item->data(0, Qt::UserRole).toString();
  if (key.isEmpty() || key.startsWith(QLatin1Char(':'))) {
    return;
  }
  setGroupFolded(key, folded);
  qInfo().noquote() << QStringLiteral("Part picker group %1 %2")
                           .arg(groupName(key), folded ? QStringLiteral("folded") : QStringLiteral("unfolded"));
  emit placesChanged();
}

void PartGrid::setSystem(const QString& system) {
  const int found = m_system->findData(system);
  if (found < 0 || found == m_system->currentIndex()) {
    return;
  }
  m_filling = true;
  m_system->setCurrentIndex(found);
  m_filling = false;
  refill(false);
}

void PartGrid::addFilter(QWidget* widget) {
  m_filters->layout()->addWidget(widget);
  m_filters->show();
}

QStringList PartGrid::shownKeys() const {
  QStringList keys;
  for (int row = 0; row < m_model->rowCount(); ++row) {
    keys << m_model->part(row)->key;
  }
  return keys;
}

void PartGrid::toggleStar() {
  if (!m_current.isEmpty()) {
    star(m_current);
  }
}

void PartGrid::toggleGroupStar() { starGroup(m_group); }

void PartGrid::star(const QString& key) {
  for (const PickerPart& listed : m_parts) {
    if (listed.key != key || listed.libraryEntry) {
      continue;
    }
    // Only this tile: a row is starred by itself (mitcad#491).
    const bool on = !starredTile(listed);
    const QString which =
        listed.config.isEmpty() ? listed.component : QStringLiteral("%1 (%2)").arg(listed.component, listed.config);
    qInfo().noquote() << QStringLiteral("Part picker star %1 %2").arg(which, on ? QStringLiteral("on") : QStringLiteral("off"));
    PartFavourites::instance().setStarred(listed.library, listed.component, listed.config, listed.defaultRow, on);
    return;
  }
}

void PartGrid::starGroup(const QString& group) {
  // Only the libraries' groups; Favourites, In this design and All parts
  // keep their places.
  if (group.isEmpty() || group.startsWith(QLatin1Char(':'))) {
    return;
  }
  // The group and its subgroups (mitcad#527); turned off, also the stars
  // kept by a category's id before the groups were a tree.
  const bool on = !groupStarred(group);
  QStringList keys = m_tree.subtree(group);
  if (keys.isEmpty()) {
    keys << group;
  }
  if (!on) {
    for (const QString& key : QStringList(keys)) {
      if (const PartGroupTree::Node* node = m_tree.node(key)) {
        keys << node->categories;
      }
    }
  }
  qInfo().noquote() << QStringLiteral("Part picker group star %1 %2")
                           .arg(group, on ? QStringLiteral("on") : QStringLiteral("off"));
  PartFavourites::instance().setGroupsStarred(keys, on);
}

bool PartGrid::groupStarred(const QString& group) const {
  const PartFavourites& favourites = PartFavourites::instance();
  if (favourites.groupStarred(group)) {
    return true;
  }
  const PartGroupTree::Node* node = m_tree.node(group);
  return node != nullptr && std::any_of(node->categories.begin(), node->categories.end(),
                                        [&favourites](const QString& id) { return favourites.groupStarred(id); });
}

QString PartGrid::groupName(const QString& group) const {
  if (group == kFavourites) {
    return tr("Favourites");
  }
  if (group == kInDesign) {
    return tr("In this design");
  }
  if (group == kAll) {
    return tr("All parts");
  }
  if (group == kLibraries) {
    return tr("Libraries to get");
  }
  if (group == kOther) {
    return tr("Other");
  }
  if (const PartGroupTree::Node* node = m_tree.node(group)) {
    return node->path;
  }
  if (groupKey(m_groupPath.names) == group) {
    QStringList labels;
    for (const QString& name : m_groupPath.names) {
      labels << groupLabel(name);
    }
    return labels.join(QLatin1Char('/'));
  }
  return groupLabel(group);
}

int PartGrid::countOf(const PickerPart& part) const {
  return m_used.of(part.library, part.component, part.config, part.defaultRow);
}

void PartGrid::fillGroups() {
  m_tree.clear();
  bool libraries = false;
  bool other = false;
  bool inDesign = false;
  for (const PickerPart& listed : m_parts) {
    if (listed.libraryEntry) {
      libraries = true;
      continue;
    }
    inDesign = inDesign || countOf(listed) > 0;
    if (listed.groups.isEmpty()) {
      other = true;
    }
    for (const PartGroupPath& group : listed.groups) {
      m_tree.add(group);
    }
  }
  // The chosen group stays while a search or a filter empties it, so that
  // the search goes on inside it (mitcad#418).
  const bool special = m_group.startsWith(QLatin1Char(':'));
  if (m_group == kInDesign) {
    inDesign = true;
  } else if (m_group == kOther) {
    other = true;
  } else if (m_group == kLibraries) {
    libraries = true;
  } else if (!special && !m_tree.contains(m_group) && groupKey(m_groupPath.names) == m_group) {
    m_tree.add(m_groupPath);
  }
  m_tree.finish();
  const bool known = m_group == kFavourites || m_group == kAll || m_group == kInDesign || m_group == kOther ||
                     m_group == kLibraries || m_tree.contains(m_group);
  if (!known) {
    m_group = kAll;
  }
  if (const PartGroupTree::Node* node = m_tree.node(m_group)) {
    m_groupPath = node->group;
  }
  const QColor outline = palette().color(QPalette::PlaceholderText);
  // Starred groups first on each level, each class in the tree's order.
  const auto ordered = [this](const QStringList& keys) {
    QStringList starred;
    QStringList rest;
    for (const QString& key : keys) {
      (groupStarred(key) ? starred : rest).append(key);
    }
    return starred + rest;
  };
  const std::function<QTreeWidgetItem*(const QString&)> made = [&](const QString& key) {
    const PartGroupTree::Node* node = m_tree.node(key);
    auto* item = new QTreeWidgetItem(QStringList{node != nullptr ? node->name : groupLabel(key)});
    item->setData(0, Qt::UserRole, key);
    item->setIcon(0, starIcon(groupStarred(key), outline));
#ifdef Q_OS_MACOS
    item->setData(0, kGroupStarRole, groupStarred(key));
#endif
    if (node != nullptr) {
      for (const QString& child : ordered(node->children)) {
        item->addChild(made(child));
      }
    }
    return item;
  };
  const auto addSpecial = [this](const QString& id, const QIcon& icon) {
    auto* item = new QTreeWidgetItem(QStringList{groupName(id)});
    item->setData(0, Qt::UserRole, id);
    item->setIcon(0, icon);
    m_groups->addTopLevelItem(item);
  };
  m_filling = true;
  m_groups->clear();
  addSpecial(kFavourites, starIcon(true, outline));
  if (inDesign) {
    addSpecial(kInDesign, blankIcon());
  }
  addSpecial(kAll, blankIcon());
  for (const QString& key : ordered(m_tree.top())) {
    m_groups->addTopLevelItem(made(key));
  }
  if (other) {
    addSpecial(kOther, blankIcon());
  }
  if (libraries) {
    addSpecial(kLibraries, blankIcon());
  }
  // Folded as the user left them (mitcad#527); unfolded by default.
  const QSet<QString> folded = foldedGroups();
  for (QTreeWidgetItemIterator it(m_groups); *it != nullptr; ++it) {
    if ((*it)->childCount() > 0) {
      (*it)->setExpanded(!folded.contains((*it)->data(0, Qt::UserRole).toString()));
    }
  }
  if (QTreeWidgetItem* current = groupItem(m_group)) {
    // A group inside a folded one: shown, the fold kept in the settings.
    for (QTreeWidgetItem* up = current->parent(); up != nullptr; up = up->parent()) {
      up->setExpanded(true);
    }
    m_groups->setCurrentItem(current);
  }
  m_filling = false;
  qInfo().noquote() << QStringLiteral("Part picker groups: %1").arg(groupsText());
}

void PartGrid::fillSystems() {
  QStringList systems;
  for (const PickerPart& listed : m_parts) {
    for (const QString& one : listed.system.split(QStringLiteral(", "), Qt::SkipEmptyParts)) {
      if (!systems.contains(one.trimmed())) {
        systems << one.trimmed();
      }
    }
  }
  systems.sort();
  const QString kept = m_system->currentData().toString();
  m_filling = true;
  m_system->clear();
  m_system->addItem(tr("All systems"), QString());
  for (const QString& one : systems) {
    m_system->addItem(one, one);
  }
  m_system->setCurrentIndex(std::max(0, m_system->findData(kept)));
  m_filling = false;
  m_system->setVisible(!systems.isEmpty());
  m_systemLabel->setVisible(!systems.isEmpty());
}

void PartGrid::fillSuppliers() {
  QList<QList<PartSupplier>> lists;
  for (const PickerPart& listed : m_parts) {
    lists.append(listed.suppliers);
  }
  const QList<PartSupplier> suppliers = supplierChoices(lists);
  m_filling = true;
  m_supplier->clear();
  m_supplier->addItem(tr("All suppliers"), QString());
  for (const PartSupplier& one : suppliers) {
    m_supplier->addItem(one.name, one.id);
  }
  m_supplier->setCurrentIndex(std::max(0, m_supplier->findData(m_wantedSupplier)));
  m_filling = false;
  m_supplier->setVisible(!suppliers.isEmpty());
  m_supplierLabel->setVisible(!suppliers.isEmpty());
}

QString PartGrid::supplierFilter() const { return m_supplier->currentData().toString(); }

void PartGrid::setSupplierFilter(const QString& supplier) {
  m_wantedSupplier = supplier;
  const int at = std::max(0, m_supplier->findData(supplier));
  if (at != m_supplier->currentIndex()) {
    m_supplier->setCurrentIndex(at); // refills
  }
}

bool PartGrid::passes(const PickerPart& part) const {
  if (m_group == kFavourites) {
    if (part.libraryEntry || !starredTile(part)) {
      return false;
    }
  } else if (m_group == kInDesign) {
    if (part.libraryEntry || countOf(part) <= 0) {
      return false;
    }
  } else if (m_group == kLibraries) {
    if (!part.libraryEntry) {
      return false;
    }
  } else if (m_group == kOther) {
    if (part.libraryEntry || !part.groups.isEmpty()) {
      return false;
    }
  } else if (m_group != kAll) {
    // The group's parts and its subgroups' (mitcad#527).
    if (!inShownGroup(part)) {
      return false;
    }
  }
  // The supplier (mitcad#949): a library's entry has none to compare.
  if (!part.libraryEntry && !soldBy(part.suppliers, m_supplier->currentData().toString())) {
    return false;
  }
  const QString system = m_system->currentData().toString();
  if (!system.isEmpty()) {
    // A part made for no system fits any (as library_build_candidates
    // says): a linear part's carriage beside its rail of a system.
    const QStringList systems = part.system.split(QStringLiteral(", "), Qt::SkipEmptyParts);
    if (!systems.isEmpty() && !systems.contains(system)) {
      return false;
    }
  }
  if (!m_externalSearch) {
    // The fields library_search matches.
    const QString haystack =
        QStringList{part.name, part.component, part.standard, part.description, part.categories.join(QLatin1Char(' ')),
                    part.keywords.join(QLatin1Char(' ')), part.supplier, part.partNumber, part.library,
                    part.libraryName, part.designation, part.sizes, part.system, part.config}
            .join(QLatin1Char(' '))
            .toLower();
    static const QRegularExpression spaces(QStringLiteral("\\s+"));
    for (const QString& word : m_search->text().toLower().split(spaces, Qt::SkipEmptyParts)) {
      if (!haystack.contains(word)) {
        return false;
      }
    }
  }
  return true;
}

void PartGrid::refill(bool notify) {
  hideHover();
  QList<PickerPart> kept;
  QList<OrderedPart> order;
  QList<QStringList> groups;
  for (const PickerPart& listed : m_parts) {
    if (passes(listed)) {
      kept.append(listed);
      // Its groups by key, and its categories' ids (stars kept before
      // mitcad#527).
      QStringList keys;
      for (const PartGroupPath& group : listed.groups) {
        keys << groupKey(group.names);
      }
      keys << listed.categories;
      order.append({listed.library, listed.component, keys.value(0), listed.config, listed.defaultRow});
      groups.append(keys);
    }
  }
  // Starred first, then the rest, each in the given order; in Favourites
  // the starred groups' parts first.
  QList<PickerPart> shown;
  for (const int at : pickerOrder(order, m_group == kFavourites, groups)) {
    if (at >= 0 && at < kept.size()) {
      shown.append(kept.at(at));
    }
  }
  const QString before = m_current;
  QHash<QString, int> uses;
  for (const PickerPart& tile : shown) {
    if (!tile.libraryEntry) {
      uses.insert(tile.key, countOf(tile));
    }
  }
  m_filling = true;
  m_model->setShown(shown, uses);
  m_filling = false;
  qInfo().noquote() << QStringLiteral("Part picker group %1: %2 parts, first %3")
                           .arg(groupName(m_group))
                           .arg(shown.size())
                           .arg(shown.isEmpty() ? QStringLiteral("-") : tileLabel(shown.first()));
  logTiles(false);
  if (m_model->rowOf(before) >= 0) {
    setCurrent(before, false);
  } else if (notify) {
    selectFirst(true);
  } else {
    setCurrent(QString(), false);
  }
  emit tilesChanged();
}

void PartGrid::refreshStars() {
  // Only the stars: the groups keep their order and their rows, the tiles
  // their places and the current one its selection (mitcad#517).
  const QColor outline = palette().color(QPalette::PlaceholderText);
  for (QTreeWidgetItemIterator it(m_groups); *it != nullptr; ++it) {
    const QString id = (*it)->data(0, Qt::UserRole).toString();
    if (!id.startsWith(QLatin1Char(':'))) {
      (*it)->setIcon(0, starIcon(groupStarred(id), outline));
#ifdef Q_OS_MACOS
      (*it)->setData(0, kGroupStarRole, groupStarred(id));
#endif
    }
  }
  m_model->refreshStars();
  refreshStarButton();
  logTiles(true);
  qInfo().noquote() << QStringLiteral("Part picker groups kept: %1").arg(groupsText());
  emit starsChanged();
}

void PartGrid::refreshStarButton() {
  const PickerPart* part = currentPart();
  const bool enabled = part != nullptr && !part->libraryEntry;
  m_star->setEnabled(enabled);
#ifdef Q_OS_MACOS
  const bool starred = enabled && starredTile(*part);
  const QSignalBlocker quiet(m_star);
  m_star->setChecked(starred);
  m_star->setIcon(mac::symbolIcon(starred ? QStringLiteral("star.fill") : QStringLiteral("star"),
                                starIcon(starred, palette().color(QPalette::PlaceholderText))));
#endif
}

QString PartGrid::groupsText() const {
  QStringList names;
  for (QTreeWidgetItemIterator it(m_groups); *it != nullptr; ++it) {
    const QTreeWidgetItem* item = *it;
    bool shown = true;
    for (const QTreeWidgetItem* up = item->parent(); up != nullptr; up = up->parent()) {
      shown = shown && up->isExpanded();
    }
    if (!shown) {
      continue;
    }
    QString name = groupName(item->data(0, Qt::UserRole).toString());
    if (item->childCount() > 0 && !item->isExpanded()) {
      name += QStringLiteral(" (folded)");
    }
    names << name;
  }
  return names.join(QStringLiteral(", "));
}

void PartGrid::logTiles(bool kept) const {
  QStringList tiles;
  for (int row = 0; row < m_model->rowCount() && row < 24; ++row) {
    QString tile = tileLabel(*m_model->part(row));
    const int uses = m_model->index(row).data(PartGridModel::kCountRole).toInt();
    if (uses > 0) {
      tile += QStringLiteral(" x%1").arg(uses);
    }
    tiles << tile;
  }
  if (m_model->rowCount() > 24) {
    tiles << QStringLiteral("...");
  }
  if (kept) {
    const PickerPart* current = currentPart();
    qInfo().noquote() << QStringLiteral("Part picker tiles kept: %1; current %2")
                             .arg(tiles.join(QStringLiteral(", ")),
                                  current != nullptr ? tileLabel(*current) : QStringLiteral("none"));
  } else {
    qInfo().noquote() << QStringLiteral("Part picker tiles: %1").arg(tiles.join(QStringLiteral(", ")));
  }
}

void PartGrid::logPlaces(const std::function<QString(const QWidget*, const QPoint&)>& at) const {
  qDebug().noquote() << QStringLiteral("Part picker place search at %1").arg(at(m_search, m_search->rect().center()));
  qDebug().noquote() << QStringLiteral("Part picker place star at %1").arg(at(m_star, m_star->rect().center()));
  const QRect groups = m_groups->viewport()->rect();
  for (QTreeWidgetItemIterator it(m_groups); *it != nullptr; ++it) {
    const QTreeWidgetItem* item = *it;
    // The middle of the row: its star is at its left end. A row inside a
    // folded group has no place.
    const QRect box = m_groups->visualItemRect(item);
    if (item->isHidden() || box.isEmpty() || !groups.contains(box.center())) {
      continue;
    }
    const QString key = item->data(0, Qt::UserRole).toString();
    qDebug().noquote() << QStringLiteral("Part picker group row %1 at %2")
                              .arg(groupName(key), at(m_groups->viewport(), box.center()));
    // A library group's star, at the left end of its row (mitcad#517).
    if (!key.startsWith(QLatin1Char(':'))) {
      const QPoint star(box.left() + m_groups->iconSize().width() / 2 + 4, box.center().y());
      qDebug().noquote() << QStringLiteral("Part picker place group star %1 at %2")
                                .arg(groupName(key), at(m_groups->viewport(), star));
    }
    // The fold's arrow, left of the row (mitcad#527).
    if (item->childCount() > 0) {
      const QPoint fold(box.left() - m_groups->indentation() / 2, box.center().y());
      qDebug().noquote() << QStringLiteral("Part picker place group fold %1 at %2")
                                .arg(groupName(key), at(m_groups->viewport(), fold));
    }
  }
  const QRect sight = m_view->viewport()->rect();
  for (int row = 0; row < m_model->rowCount() && row < 24; ++row) {
    const PickerPart* shown = m_model->part(row);
    const QRect tile = m_view->visualRect(m_model->index(row));
    if (shown == nullptr || !sight.contains(tile.center())) {
      continue;
    }
    // The middle of the tile: its star is in a corner.
    qDebug().noquote() << QStringLiteral("Part picker tile %1 at %2").arg(tileLabel(*shown), at(m_view->viewport(), tile.center()));
  }
}

void PartGrid::step(int by) {
  const int count = m_model->rowCount();
  if (count == 0) {
    return;
  }
  const int row = m_model->rowOf(m_current);
  const int next = row < 0 ? 0 : std::clamp(row + by, 0, count - 1);
  if (next != row) {
    setCurrent(m_model->part(next)->key, true);
  }
}

void PartGrid::setTileSize(int size) {
  m_tile = std::clamp(size, kMinTile, kMaxTile);
  m_model->setTileSize(m_tile);
  m_view->setIconSize(QSize(m_tile, m_tile));
  m_view->setGridSize(tileBox(m_tile, m_view->fontMetrics().height()) + QSize(6, 6));
}

void PartGrid::showHover(const QModelIndex& index, const QPoint& global) {
  const PickerPart* shown = m_model->part(index.row());
  if (shown == nullptr) {
    hideHover();
    return;
  }
  const QImage picture = m_model->image(*shown);
  if (picture.isNull()) {
    m_hoverImage->hide();
  } else {
    m_hoverImage->setPixmap(QPixmap::fromImage(
        picture.width() > kHoverImage || picture.height() > kHoverImage
            ? picture.scaled(kHoverImage, kHoverImage, Qt::KeepAspectRatio, Qt::SmoothTransformation)
            : picture));
    m_hoverImage->show();
  }
  m_hoverText->setText(partDetails(*shown, countOf(*shown), true));
  m_hover->adjustSize();
  QPoint at = global + QPoint(16, 20);
  if (const QScreen* screen = QGuiApplication::screenAt(global)) {
    const QRect room = screen->availableGeometry();
    at.setX(std::min(at.x(), room.right() - m_hover->width()));
    if (at.y() + m_hover->height() > room.bottom()) {
      at.setY(global.y() - m_hover->height() - 8);
    }
    at.setX(std::max(at.x(), room.left()));
    at.setY(std::max(at.y(), room.top()));
  }
  m_hover->move(at);
  m_hover->show();
  m_hoverKey = shown->key;
  qDebug().noquote() << QStringLiteral("Part picker hover %1").arg(tileLabel(*shown));
}

void PartGrid::hideHover() {
  if (m_hover != nullptr && m_hover->isVisible()) {
    m_hover->hide();
  }
  m_hoverKey.clear();
}

void PartGrid::hideEvent(QHideEvent* event) {
  hideHover();
  QWidget::hideEvent(event);
}

bool PartGrid::eventFilter(QObject* watched, QEvent* event) {
  if (watched == m_search && event->type() == QEvent::KeyPress) {
    // Up and Down in the search move through the tiles.
    const int key = static_cast<QKeyEvent*>(event)->key();
    if ((key == Qt::Key_Down || key == Qt::Key_Up) && m_model->rowCount() > 0) {
      step(key == Qt::Key_Down ? 1 : -1);
      return true;
    }
  } else if (watched == m_view && event->type() == QEvent::KeyPress) {
    const int key = static_cast<QKeyEvent*>(event)->key();
    if ((key == Qt::Key_Return || key == Qt::Key_Enter) && !m_current.isEmpty()) {
      emit activated(m_current);
      return true;
    }
  } else if (watched == m_view->viewport()) {
    switch (event->type()) {
    case QEvent::ToolTip: {
      const auto* help = static_cast<QHelpEvent*>(event);
      const QModelIndex index = m_view->indexAt(help->pos());
      if (index.isValid()) {
        showHover(index, help->globalPos());
      } else {
        hideHover();
      }
      return true;
    }
    case QEvent::MouseMove: {
      if (!m_hoverKey.isEmpty()) {
        const QModelIndex index = m_view->indexAt(static_cast<QMouseEvent*>(event)->position().toPoint());
        if (!index.isValid() || index.data(PartGridModel::kKeyRole).toString() != m_hoverKey) {
          hideHover();
        }
      }
      break;
    }
    case QEvent::Leave:
    case QEvent::Wheel:
      hideHover();
      break;
    case QEvent::MouseButtonPress:
      m_starHit = false;
      hideHover();
      break;
    case QEvent::MouseButtonRelease: {
      const auto* mouse = static_cast<QMouseEvent*>(event);
      const QPoint at = mouse->position().toPoint();
      const QModelIndex index = m_view->indexAt(at);
      if (mouse->button() == Qt::LeftButton && index.isValid() &&
          starRect(m_view->visualRect(index), m_tile).contains(at)) {
        // The star, not the tile: toggled once the view is done with the
        // click (the tiles are made again).
        m_starHit = true;
        const QString key = index.data(PartGridModel::kKeyRole).toString();
        QTimer::singleShot(0, this, [this, key] { star(key); });
      }
      break;
    }
    default:
      break;
    }
  } else if (watched == m_groups->viewport() && event->type() == QEvent::MouseButtonRelease) {
    const auto* mouse = static_cast<QMouseEvent*>(event);
    const QPoint at = mouse->position().toPoint();
    if (QTreeWidgetItem* item = m_groups->itemAt(at)) {
      // The star at the row's left end; left of the row is the fold's arrow.
      const QRect box = m_groups->visualItemRect(item);
      if (mouse->button() == Qt::LeftButton && at.x() >= box.left() &&
          at.x() < box.left() + m_groups->iconSize().width() + 8) {
        const QString group = item->data(0, Qt::UserRole).toString();
        QTimer::singleShot(0, this, [this, group] { starGroup(group); });
      }
    }
  }
  return QWidget::eventFilter(watched, event);
}

// ---------------------------------------------------------------------------
// PartPicker

PartPicker* PartPicker::of(QWidget* window) {
  QWidget* top = window != nullptr ? window->window() : nullptr;
  if (top != nullptr) {
    if (auto* found = top->findChild<PartPicker*>(QString(), Qt::FindDirectChildrenOnly)) {
      return found;
    }
  }
  return new PartPicker(top);
}

PartPicker::PartPicker(QWidget* parent) : QDialog(parent) {
  setObjectName(QStringLiteral("partPicker"));
  setWindowTitle(tr("Part Picker"));
  setModal(false);
  // The view keeps the keyboard: the picker is there to be clicked.
  setAttribute(Qt::WA_ShowWithoutActivating);
  // The same in the native frontend: an AppKit utility panel that does not
  // take the keyboard from the document, its frame kept under this key
  // (showDialog, mitcad#819).
  setProperty("nativeUtilityWindow", true);
  setProperty("nativeFrameKey", QStringLiteral("partPicker/nativeFrame"));
  resize(780, 560);
  auto* layout = new QVBoxLayout(this);
  m_grid = new PartGrid;
  layout->addWidget(m_grid, 1);
  // An open table's options (mitcad#810), under the grid.
  m_options = new OptionChooser;
  m_options->setObjectName(QStringLiteral("partPickerOptions"));
  m_options->setLogName(QStringLiteral("Part picker"));
  m_options->hide();
  layout->addWidget(m_options);
  auto* buttons = new QDialogButtonBox;
  m_pin = new QToolButton;
  m_pin->setObjectName(QStringLiteral("partPickerPin"));
  m_pin->setText(tr("&Pin"));
  m_pin->setCheckable(true);
  m_pin->setToolTip(tr("Keep the picker open when the command ends; it follows the next command that chooses parts"));
  mac::styleIconToggle(m_pin, QStringLiteral("pin"), QStringLiteral("pin.fill"));
  auto* close = new QPushButton(tr("Close"));
  auto* footer = new QHBoxLayout;
  footer->addWidget(m_pin);
  footer->addStretch(1);
  buttons->addButton(close, QDialogButtonBox::RejectRole);
  mac::styleDialogButtons(buttons, close);
  footer->addWidget(buttons);
  layout->addLayout(footer);
  mac::styleDialog(this);
  m_saveTimer.setSingleShot(true);
  m_saveTimer.setInterval(400);
  connect(&m_saveTimer, &QTimer::timeout, this, [this] {
    if (m_restored && !testAttribute(Qt::WA_DontShowOnScreen)) {
      QSettings().setValue(kGeometryKey, saveGeometry());
    }
  });
  connect(close, &QPushButton::clicked, this, &QWidget::hide);
  connect(m_pin, &QToolButton::toggled, this, [](bool on) {
    qInfo().noquote() << QStringLiteral("Part picker pinned %1").arg(on ? QStringLiteral("on") : QStringLiteral("off"));
  });
  connect(m_grid, &PartGrid::clicked, this, &PartPicker::choose);
  connect(m_grid, &PartGrid::activated, this, &PartPicker::choose);
  connect(m_grid, &PartGrid::currentChanged, this, [this](const QString& key) {
    if (key != m_optionsFor) {
      // The tile chosen keeps the options it was chosen with.
      showOptions(key, key == m_chosenTile ? build::keyOptions(m_request.current) : QJsonObject());
    }
  });
  // An option changed: the part chosen with the options follows them.
  connect(m_options, &OptionChooser::optionsChanged, this, [this] {
    if (!m_showingOptions && !m_chosenTile.isEmpty() && m_chosenTile == m_optionsFor) {
      choose(m_chosenTile);
    }
  });
  m_placesTimer.setSingleShot(true);
  m_placesTimer.setInterval(150);
  connect(&m_placesTimer, &QTimer::timeout, this, [this] { logPlaces(); });
  TestSync::watch(&m_placesTimer);
  // The window moved or was resized (mitcad#528): its places again once it
  // settles, when they are not where they were logged last; the UI tests'
  // sync waits for it.
  m_movedTimer.setSingleShot(true);
  m_movedTimer.setInterval(50);
  connect(&m_movedTimer, &QTimer::timeout, this, [this] {
    if (isVisible() && m_loggedGeometry.isValid() && placesGeometry() != m_loggedGeometry) {
      logPlaces();
    }
  });
  TestSync::watch(&m_movedTimer);
  connect(m_grid, &PartGrid::tilesChanged, this, &PartPicker::logPlacesLater);
  connect(m_grid, &PartGrid::placesChanged, this, &PartPicker::logPlacesLater);
}

QRect PartPicker::placesGeometry() const {
  // Where the window is in the main window (its places are logged there).
  const QWidget* host = parentWidget() != nullptr ? parentWidget()->window() : nullptr;
  const QPoint corner = mapToGlobal(QPoint(0, 0));
  return QRect(host != nullptr ? host->mapFromGlobal(corner) : corner, size());
}

void PartPicker::logPlacesLater() {
  if (isVisible()) {
    m_placesTimer.start();
  }
}

void PartPicker::logPlaces() const {
  if (!isVisible()) {
    return;
  }
  // As the panel's places (GlassCard::mapToHost): in the main window.
  const QWidget* host = parentWidget() != nullptr ? parentWidget()->window() : nullptr;
  const auto at = [this, host](const QWidget* widget, const QPoint& point) {
    const QPoint global = widget->mapToGlobal(point);
    const QPoint inHost = host != nullptr ? host->mapFromGlobal(global) : global;
    const QPoint own = mapFromGlobal(global);
    return QStringLiteral("%1,%2; in the picker %3,%4").arg(inHost.x()).arg(inHost.y()).arg(own.x()).arg(own.y());
  };
  qDebug().noquote() << QStringLiteral("Part picker place pin at %1").arg(at(m_pin, m_pin->rect().center()));
  m_grid->logPlaces(at);
  m_loggedGeometry = placesGeometry();
}

PartPicker::~PartPicker() {
  m_placesTimer.stop();
  m_movedTimer.stop();
  m_saveTimer.stop();
  if (m_restored && isVisible() && !testAttribute(Qt::WA_DontShowOnScreen)) {
    QSettings().setValue(kGeometryKey, saveGeometry());
  }
}

void PartPicker::ask(const PickerRequest& request) {
  const bool following = isVisible() && isPinned() && request.command != m_request.command;
  const bool another = request.command != m_request.command || request.input != m_request.input;
  m_request = request;
  setWindowTitle(request.title.isEmpty() ? tr("Part Picker") : tr("Part Picker - %1").arg(request.title));
  if (m_designStale) {
    readDesign();
  }
  if (another) {
    m_grid->setOtherParts(request.parts);
  } else {
    m_grid->setParts(request.parts);
  }
  m_grid->setSystem(request.system);
  // The part chosen in sight, highlighted (mitcad#452); with its options
  // (mitcad#810).
  m_chosenTile = tileOf(request.current);
  showOptions(m_chosenTile, build::keyOptions(request.current));
  if (m_grid->reveal(m_chosenTile)) {
    if (const PickerPart* current = m_grid->currentPart()) {
      qInfo().noquote() << QStringLiteral("Part picker shows the part chosen: %1").arg(tileLabel(*current));
    }
  }
  qInfo().noquote() << (following ? QStringLiteral("Part picker follows %2: %1 parts")
                                  : QStringLiteral("Part picker opened: %1 parts for %2"))
                           .arg(request.parts.size())
                           .arg(request.command);
  if (!m_restored) {
    restoreGeometry(QSettings().value(kGeometryKey).toByteArray());
    m_restored = true;
  }
  // Shown through the shared dispatch (mitcad#819): Qt shows its window
  // once; the native frontend hosts the picker in an AppKit utility panel
  // and brings that panel forward again while it is open, without taking
  // the keyboard. A native panel's presenter is never on the screen.
  showDialog(this);
  if (!testAttribute(Qt::WA_DontShowOnScreen)) {
    raise();
  }
  logPlacesLater();
}

void PartPicker::setParts(const QList<PickerPart>& parts, const QString& current) {
  m_request.parts = parts;
  m_grid->setParts(parts);
  if (!current.isEmpty()) {
    m_grid->setCurrent(current);
  }
}

void PartPicker::refreshParts(const QList<PickerPart>& parts) {
  setParts(parts, m_request.current);
  qInfo().noquote() << QStringLiteral("Part picker refreshed: %1 parts for %2")
                           .arg(parts.size())
                           .arg(m_request.command);
}

void PartPicker::setCurrent(const QString& key) {
  m_request.current = key;
  const QString tile = tileOf(key);
  m_grid->setCurrent(tile);
  if (tile != m_optionsFor) {
    showOptions(tile, build::keyOptions(key));
  }
}

QString PartPicker::tileOf(const QString& key) const {
  if (!key.contains(QLatin1Char('?'))) {
    return key;
  }
  const QString base = build::baseKey(key);
  const QString prefix = build::keyPrefix(base);
  QString found;
  for (const PickerPart& listed : m_grid->allParts()) {
    if (listed.key == base) {
      return listed.key;
    }
    if (found.isEmpty() && build::keyPrefix(listed.key) == prefix) {
      found = listed.key;
    }
  }
  return found;
}

void PartPicker::showOptions(const QString& key, const QJsonObject& parameters) {
  const PickerPart* shown = nullptr;
  for (const PickerPart& listed : m_grid->allParts()) {
    if (listed.key == key) {
      shown = &listed;
      break;
    }
  }
  m_optionsFor = key;
  if (shown == nullptr || !isOpenTable(shown->table) || !shown->refused.isEmpty()) {
    m_options->hide();
    return;
  }
  m_showingOptions = true;
  m_options->setOptionsQuery(libraryOptionsQuery(libraryCommand, shown->url, shown->library, shown->rev,
                                                 shown->component, [this] { return m_options->standard(); }));
  // The Standard and Supplier filters above the options (mitcad#949): the
  // grid's supplier to start with.
  m_options->setSupplierFilter(m_grid->supplierFilter(), false);
  m_options->setTable(shown->table, shown->config, parameters);
  m_showingOptions = false;
  m_options->setVisible(m_options->isOpen());
  qInfo().noquote() << QStringLiteral("Part picker options of %1: %2").arg(tileLabel(*shown), m_options->summary());
}

void PartPicker::commandEnded(const QString& ended) {
  if (!ended.isEmpty() && ended != m_request.command) {
    return;
  }
  if (isPinned()) {
    if (isVisible()) {
      qInfo().noquote() << QStringLiteral("Part picker stays pinned after %1").arg(ended);
    }
    return;
  }
  if (isVisible()) {
    qInfo().noquote() << QStringLiteral("Part picker closed with %1").arg(ended);
    hide();
  }
}

bool PartPicker::isPinned() const { return m_pin->isChecked(); }

void PartPicker::setPinned(bool on) { m_pin->setChecked(on); }

void PartPicker::setDesignParts(std::function<QList<UsedPart>()> read) {
  m_readDesign = std::move(read);
  m_designStale = true;
  if (isVisible()) {
    readDesign();
  }
}

void PartPicker::designChanged() {
  if (isVisible()) {
    readDesign();
  } else {
    m_designStale = true;
  }
}

void PartPicker::readDesign() {
  m_designStale = false;
  if (!m_readDesign) {
    return;
  }
  try {
    m_grid->setUsedParts(m_readDesign());
  } catch (const std::exception& error) {
    qWarning().noquote() << QStringLiteral("Part picker: the design's parts not read: %1")
                                .arg(QString::fromUtf8(error.what()));
  }
}

void PartPicker::choose(const QString& key) {
  for (const PickerPart& listed : m_grid->allParts()) {
    if (listed.key != key) {
      continue;
    }
    if (!listed.refused.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Part picker refused %1: %2").arg(partLabel(listed), listed.refused);
      return;
    }
    if (listed.libraryEntry) {
      return;
    }
    // A part of an open table: with the options under the grid (mitcad#810).
    if (isOpenTable(listed.table) && m_optionsFor != key) {
      showOptions(key);
    }
    m_chosenTile = key;
    if (m_optionsFor == key && m_options->isOpen()) {
      PickerPart withOptions = listed;
      withOptions.config = m_options->row();
      withOptions.key = build::optionsKey(key, withOptions.config, m_options->parameters());
      m_request.current = withOptions.key;
      qInfo().noquote() << QStringLiteral("Part picker chose %1 with options %2")
                               .arg(partLabel(listed), m_options->summary());
      emit chosen(withOptions, m_request.command);
      return;
    }
    m_request.current = key;
    qInfo().noquote() << QStringLiteral("Part picker chose %1%2")
                             .arg(partLabel(listed), listed.config.isEmpty()
                                                         ? QString()
                                                         : QStringLiteral(" (%1)").arg(listed.config));
    emit chosen(listed, m_request.command);
    return;
  }
}

// A natively hosted picker's presenter is off the screen: its AppKit panel
// keeps its own frame (nativeFrameKey), so the Qt geometry is left as it was.
void PartPicker::saveGeometryLater() {
  if (m_restored && isVisible() && !testAttribute(Qt::WA_DontShowOnScreen)) {
    m_saveTimer.start();
  }
}

void PartPicker::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  if (m_designStale) {
    readDesign();
  }
  logPlacesLater();
}

void PartPicker::hideEvent(QHideEvent* event) {
  if (m_restored && !testAttribute(Qt::WA_DontShowOnScreen)) {
    m_saveTimer.stop();
    QSettings().setValue(kGeometryKey, saveGeometry());
  }
  m_designStale = true;
  QDialog::hideEvent(event);
}

void PartPicker::moveEvent(QMoveEvent* event) {
  QDialog::moveEvent(event);
  saveGeometryLater();
  if (isVisible()) {
    m_movedTimer.start(); // mitcad#528
  }
}

void PartPicker::resizeEvent(QResizeEvent* event) {
  QDialog::resizeEvent(event);
  saveGeometryLater();
  // A new size lays the tiles out again: its size differs from the one
  // logged, so the check logs every place anew (mitcad#528).
  if (isVisible()) {
    m_movedTimer.start();
  }
}

} // namespace mitcad
