// SPDX-License-Identifier: MIT
#include "BuildLists.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <limits>
#include <utility>

#include <QApplication>
#include <QClipboard>
#include <QDate>
#include <QDialogButtonBox>
#include <QDir>
#include <QDoubleSpinBox>
#include <QFileInfo>
#include <QFont>
#include <QFontMetrics>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHash>
#include <QHeaderView>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QPageLayout>
#include <QPageSize>
#include <QPainter>
#include <QPdfWriter>
#include <QPushButton>
#include <QStandardPaths>
#include <QTabWidget>
#include <QTableWidget>
#include <QTimer>
#include <QVBoxLayout>
#include <QtLogging>

#include "../browser/DocumentHost.hpp"
#include "../framework/Command.hpp"
#include "../framework/DialogFolders.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Json.hpp"
#include "BuildModel.hpp"

#include "../platform/MacDialogs.hpp"

namespace mitcad::build {
namespace {

// The parts list's property columns as last typed (mitcad#496), for the
// next time the list opens in this session.
QString& lastPropertyColumns() {
  static QString columns;
  return columns;
}

constexpr double kPi = 3.14159265358979323846;
constexpr double kNaN = std::numeric_limits<double>::quiet_NaN();

QString text(const QJsonObject& object, const char* key) { return object.value(QLatin1String(key)).toString(); }

QString millimetresText(double value) {
  if (!std::isfinite(value)) {
    return QString();
  }
  return QString::number(std::round(value * 10.0) / 10.0, 'f', 1);
}

// A cut's angle (radians, as the replies give angles) in degrees: "45 deg",
// or "0".
QString angleText(const QJsonValue& angle) {
  const double degrees = angle.toDouble() * 180.0 / kPi;
  if (!angle.isDouble() || !std::isfinite(degrees) || std::abs(degrees) < 1e-6) {
    return QStringLiteral("0");
  }
  return QStringLiteral("%1 deg").arg(QString::number(std::round(degrees * 10.0) / 10.0, 'g', 6));
}

// An end of a cut part: "square", or "45 deg (x)".
QString endText(const QJsonValue& value) {
  const QJsonObject end = value.toObject();
  const QString shown = angleText(end.value(QStringLiteral("angle")));
  if (shown == QStringLiteral("0")) {
    return QObject::tr("square");
  }
  const QString tilt = text(end, "tilt");
  return tilt.isEmpty() ? shown : QStringLiteral("%1 (%2)").arg(shown, tilt);
}

// The holes machined into a part in millimetres: "D6.8 x 15 M8 at end_a".
// The cut list's are flat, {at, diameter, depth, thread, connector};
// build_edit's {end, hole: {...}}.
QString machiningText(const QJsonValue& value) {
  QStringList holes;
  for (const QJsonValue& entry : value.toArray()) {
    const QJsonObject item = entry.toObject();
    // A joinery cut (mitcad#478): {kind, size, with}, "half_lap 148 x 24 x 148".
    if (!text(item, "kind").isEmpty()) {
      QStringList size;
      for (const QJsonValue& side : item.value(QStringLiteral("size")).toArray()) {
        size << millimetresText(side.toDouble(kNaN));
      }
      holes << QStringLiteral("%1 %2").arg(text(item, "kind"), size.join(QStringLiteral(" x "))).trimmed();
      continue;
    }
    const QJsonObject hole = item.value(QStringLiteral("hole")).isObject() ? item.value(QStringLiteral("hole")).toObject() : item;
    QString shown = QStringLiteral("D%1").arg(millimetresText(hole.value(QStringLiteral("diameter")).toDouble(kNaN)));
    const double depth = hole.value(QStringLiteral("depth")).toDouble(kNaN);
    if (std::isfinite(depth)) {
      shown += QStringLiteral(" x %1").arg(millimetresText(depth));
    }
    if (!text(hole, "thread").isEmpty()) {
      shown += QLatin1Char(' ') + text(hole, "thread");
    }
    if (!text(item, "end").isEmpty()) {
      shown += QObject::tr(" at end %1").arg(text(item, "end"));
    } else if (!text(item, "at").isEmpty()) {
      shown += QObject::tr(" at %1").arg(text(item, "at"));
    }
    holes << shown;
  }
  return holes.join(QStringLiteral("; "));
}

// A mass in kilograms.
QString massText(const QJsonValue& mass) {
  return mass.isDouble() ? QString::number(mass.toDouble(), 'f', 3) : QString();
}

// Running metres, three decimals; a volume in m³, four.
QString metresText(const QJsonValue& metres) {
  return metres.isDouble() ? QString::number(metres.toDouble(), 'f', 3) : QString();
}
QString volumeText(const QJsonValue& volume) {
  return volume.isDouble() ? QString::number(volume.toDouble(), 'f', 4) : QString();
}

// A timber section as its rows name it, thickness first: {width: 148,
// height: 48} is "48x148".
QString sectionText(const QJsonValue& value) {
  const QJsonObject section = value.toObject();
  const double width = section.value(QStringLiteral("width")).toDouble(kNaN);
  const double height = section.value(QStringLiteral("height")).toDouble(kNaN);
  if (!std::isfinite(width) || !std::isfinite(height)) {
    return QString();
  }
  return QStringLiteral("%1x%2").arg(height, 0, 'g', 6).arg(width, 0, 'g', 6);
}

// A cut row's timber columns (mitcad#478; commands.md, "BUILD: timber
// construction", *Lists*): material, grade, finish, nominal and actual
// section, running metres, volume; empty for a row that is no timber.
QStringList timberCells(const QJsonObject& row) {
  if (text(row, "kind") != QStringLiteral("timber")) {
    return QStringList(7, QString());
  }
  const QJsonObject material = row.value(QStringLiteral("material")).toObject();
  return {text(material, "species"),
          text(material, "grade"),
          text(material, "finish"),
          sectionText(row.value(QStringLiteral("nominal"))),
          sectionText(row.value(QStringLiteral("section"))),
          metresText(row.value(QStringLiteral("running_metres"))),
          volumeText(row.value(QStringLiteral("volume")))};
}

// A covered area in m², three decimals (mitcad#477).
QString areaText(const QJsonValue& area) {
  return area.isDouble() ? QString::number(area.toDouble(), 'f', 3) : QString();
}

// A cut row's profiled board columns (mitcad#477; commands.md, "BUILD:
// timber construction", *Profiled boards*): the profile, the cover width
// and the area covered; empty for a row that is no profiled board.
QStringList boardCells(const QJsonObject& row) {
  if (text(row, "profile").isEmpty()) {
    return QStringList(3, QString());
  }
  return {text(row, "profile").replace(QLatin1Char('_'), QLatin1Char(' ')),
          millimetresText(row.value(QStringLiteral("cover")).toDouble(kNaN)), areaText(row.value(QStringLiteral("area")))};
}

QString countText(const QJsonValue& value) {
  if (value.isDouble()) {
    return QString::number(value.toInt());
  }
  if (value.isArray()) {
    return QString::number(value.toArray().size());
  }
  return value.toString();
}

QString csvField(const QString& cell) {
  if (!cell.contains(QLatin1Char(',')) && !cell.contains(QLatin1Char('"')) && !cell.contains(QLatin1Char('\n'))) {
    return cell;
  }
  return QStringLiteral("\"%1\"").arg(QString(cell).replace(QLatin1Char('"'), QStringLiteral("\"\"")));
}

void fill(QTableWidget* table, const ListTable& list) {
  table->clear();
  table->setColumnCount(static_cast<int>(list.columns.size()));
  table->setHorizontalHeaderLabels(list.columns);
  table->setRowCount(static_cast<int>(list.rows.size()));
  for (int r = 0; r < list.rows.size(); ++r) {
    const QStringList& cells = list.rows[r];
    for (int c = 0; c < cells.size() && c < list.columns.size(); ++c) {
      table->setItem(r, c, new QTableWidgetItem(cells[c]));
    }
  }
  table->resizeColumnsToContents();
}

QTableWidget* newTable(const QString& name) {
  auto* table = new QTableWidget;
  table->setObjectName(name);
  table->setEditTriggers(QAbstractItemView::NoEditTriggers);
  table->setSelectionBehavior(QAbstractItemView::SelectRows);
  table->horizontalHeader()->setStretchLastSection(true);
  table->verticalHeader()->setVisible(false);
  mac::styleListTable(table);
  return table;
}

} // namespace

QString csvOf(const QVector<ListTable>& tables) {
  QStringList lines;
  for (const ListTable& table : tables) {
    if (!lines.isEmpty()) {
      lines << QString();
    }
    if (tables.size() > 1 && !table.heading.isEmpty()) {
      lines << csvField(table.heading);
    }
    QStringList header;
    for (const QString& column : table.columns) {
      header << csvField(column);
    }
    lines << header.join(QLatin1Char(','));
    for (const QStringList& row : table.rows) {
      QStringList cells;
      for (const QString& cell : row) {
        cells << csvField(cell);
      }
      lines << cells.join(QLatin1Char(','));
    }
  }
  return lines.join(QLatin1Char('\n')) + QLatin1Char('\n');
}

QVector<ListTable> cutListTables(const QJsonObject& answer) {
  // Timber (mitcad#478): its columns only when the design has timber.
  const bool timber = answer.value(QStringLiteral("timber")).toBool();
  ListTable rows{QObject::tr("Cut list"),
                 {QObject::tr("Quantity"), QObject::tr("Part"), QObject::tr("Length (mm)"), QObject::tr("End A"),
                  QObject::tr("End B"), QObject::tr("Machining"), QObject::tr("Own features"), QObject::tr("Supplier"),
                  QObject::tr("Part number"), QObject::tr("Mass each (kg)")},
                 {}};
  if (timber) {
    rows.columns << QObject::tr("Material") << QObject::tr("Grade") << QObject::tr("Finish")
                 << QObject::tr("Nominal (mm)") << QObject::tr("Actual (mm)") << QObject::tr("Running metres (m)")
                 << QObject::tr("Volume (m³)");
  }
  // Profiled boards (mitcad#477): their columns only when the design has
  // them.
  const bool profiled = timber && answer.value(QStringLiteral("profiled")).toBool();
  if (profiled) {
    rows.columns << QObject::tr("Profile") << QObject::tr("Cover width (mm)") << QObject::tr("Area covered (m²)");
  }
  // The timber frames' members by frame (mitcad#522): the last column, only
  // when the design has frames.
  const QJsonArray answerRows = answer.value(QStringLiteral("rows")).toArray();
  const bool frames = std::any_of(answerRows.begin(), answerRows.end(), [](const QJsonValue& value) {
    return !value.toObject().value(QStringLiteral("frame")).toString().isEmpty();
  });
  if (frames) {
    rows.columns << QObject::tr("Frame");
  }
  for (const QJsonValue& value : answerRows) {
    const QJsonObject row = value.toObject();
    const QJsonObject ends = row.value(QStringLiteral("ends")).toObject();
    QStringList cells{countText(row.value(QStringLiteral("quantity"))), text(row, "designation"),
                      millimetresText(lengthOf(row.value(QStringLiteral("length")))),
                      endText(ends.value(QStringLiteral("a"))), endText(ends.value(QStringLiteral("b"))),
                      machiningText(row.value(QStringLiteral("machining"))),
                      countText(row.value(QStringLiteral("features"))), text(row, "supplier"),
                      text(row, "part_number"), massText(row.value(QStringLiteral("mass")))};
    if (timber) {
      cells << timberCells(row);
    }
    if (profiled) {
      cells << boardCells(row);
    }
    if (frames) {
      cells << text(row, "frame");
    }
    rows.rows.append(cells);
  }
  if (timber) {
    // The totals of the timber rows.
    const QJsonObject totals = answer.value(QStringLiteral("totals")).toObject();
    QStringList total(rows.columns.size(), QString());
    total[1] = QObject::tr("Total timber");
    total[9] = massText(totals.value(QStringLiteral("mass")));
    total[15] = metresText(totals.value(QStringLiteral("running_metres")));
    total[16] = volumeText(totals.value(QStringLiteral("volume")));
    if (profiled) {
      total[19] = areaText(totals.value(QStringLiteral("area")));
    }
    rows.rows.append(total);
  }
  ListTable stock{QObject::tr("Stock"),
                  {QObject::tr("Part"), QObject::tr("Stock length (mm)"),
                   QObject::tr("Pieces (mm, in cutting order; | between stock pieces)"), QObject::tr("Stock pieces"),
                   QObject::tr("Offcut (mm)"), QObject::tr("Too long for the stock (mm)")},
                  {}};
  if (timber) {
    // Stock per size, each piece's stock length (mitcad#478).
    stock.columns << QObject::tr("Size") << QObject::tr("Stock lengths (mm)");
  }
  for (const QJsonValue& value : answer.value(QStringLiteral("stock")).toArray()) {
    // {source, designation, stock_length, pieces, bars, needed, offcut, too_long}
    const QJsonObject entry = value.toObject();
    const QJsonArray pieces = entry.value(QStringLiteral("pieces")).toArray();
    // `bars`: where each stock piece's cuts end in `pieces`.
    QList<int> ends;
    for (const QJsonValue& end : entry.value(QStringLiteral("bars")).toArray()) {
      ends << end.toInt();
    }
    QStringList bars;
    QStringList bar;
    for (int i = 0; i < pieces.size(); ++i) {
      bar << millimetresText(lengthOf(pieces[i]));
      if (ends.contains(i + 1)) {
        bars << bar.join(QStringLiteral(", "));
        bar.clear();
      }
    }
    if (!bar.isEmpty()) {
      bars << bar.join(QStringLiteral(", "));
    }
    const QJsonValue tooLong = entry.value(QStringLiteral("too_long"));
    QStringList longer;
    for (const QJsonValue& piece : tooLong.toArray()) {
      longer << millimetresText(lengthOf(piece));
    }
    if (tooLong.isDouble() && tooLong.toDouble() > 0.0) {
      longer << QString::number(tooLong.toInt());
    } else if (tooLong.isBool() && tooLong.toBool()) {
      longer << QObject::tr("yes");
    }
    const QString part = text(entry, "designation").isEmpty() ? text(entry, "source") : text(entry, "designation");
    QStringList cells{part, millimetresText(lengthOf(entry.value(QStringLiteral("stock_length")))),
                      bars.join(QStringLiteral(" | ")), countText(entry.value(QStringLiteral("needed"))),
                      millimetresText(lengthOf(entry.value(QStringLiteral("offcut")))), longer.join(QStringLiteral(", "))};
    if (timber) {
      QStringList lengths;
      for (const QJsonValue& length : entry.value(QStringLiteral("stock_lengths")).toArray()) {
        lengths << millimetresText(lengthOf(length));
      }
      cells << text(entry, "config") << lengths.join(QStringLiteral(", "));
    }
    stock.rows.append(cells);
  }
  ListTable other{QObject::tr("Other parts"),
                  {QObject::tr("Quantity"), QObject::tr("Part"), QObject::tr("Supplier"), QObject::tr("Part number")},
                  {}};
  for (const QJsonValue& value : answer.value(QStringLiteral("other")).toArray()) {
    const QJsonObject row = value.toObject();
    other.rows.append({countText(row.value(QStringLiteral("quantity"))),
                       text(row, "designation").isEmpty() ? text(row, "name") : text(row, "designation"),
                       text(row, "supplier"), text(row, "part_number")});
  }
  return {rows, stock, other};
}

QStringList propertyKeys(const QString& typed) {
  QStringList keys;
  for (const QString& key : typed.split(QLatin1Char(','), Qt::SkipEmptyParts)) {
    if (!key.trimmed().isEmpty()) {
      keys << key.trimmed();
    }
  }
  return keys;
}

QVector<ListTable> partsListTables(const QJsonObject& answer, const QJsonObject& cut, const QStringList& keys) {
  ListTable parts{QObject::tr("Parts list"),
                  {QObject::tr("Quantity"), QObject::tr("Part"), QObject::tr("Supplier"), QObject::tr("Part number"),
                   QObject::tr("Supplier part numbers"), QObject::tr("Library"), QObject::tr("Version"),
                   QObject::tr("Licence")},
                  {}};
  // Timber (mitcad#478): the material and grade of the cut list's timber
  // rows, by the components they list; the columns only with timber.
  const bool timber = cut.value(QStringLiteral("timber")).toBool();
  QHash<QString, QJsonObject> material;
  if (timber) {
    parts.columns << QObject::tr("Material") << QObject::tr("Grade");
    for (const QJsonValue& value : cut.value(QStringLiteral("rows")).toArray()) {
      const QJsonObject row = value.toObject();
      if (text(row, "kind") != QStringLiteral("timber")) {
        continue;
      }
      for (const QJsonValue& component : row.value(QStringLiteral("components")).toArray()) {
        material.insert(component.toString(), row.value(QStringLiteral("material")).toObject());
      }
    }
  }
  parts.columns << keys;
  for (const QJsonValue& value : answer.value(QStringLiteral("rows")).toArray()) {
    const QJsonObject row = value.toObject();
    // The suppliers' articles of the part's row (mitcad#949): "Würth 0084 6
    // 8 (8.8 zinc-plated); ...".
    QStringList cells{countText(row.value(QStringLiteral("quantity"))),
                      text(row, "designation").isEmpty() ? text(row, "name") : text(row, "designation"),
                      text(row, "supplier"),
                      text(row, "part_number"),
                      text(row, "supplier_parts_text"),
                      text(row, "library"),
                      text(row, "version"),
                      text(row, "license")};
    if (timber) {
      const QJsonObject own = material.value(text(row, "component"));
      cells << text(own, "species") << text(own, "grade");
    }
    // The chosen property keys (mitcad#496): empty where the part has none.
    const QJsonObject properties = row.value(QStringLiteral("properties")).toObject();
    for (const QString& key : keys) {
      cells << properties.value(key).toString();
    }
    parts.rows.append(cells);
  }
  return {parts};
}

bool writeListPdf(const QString& path, const QString& title, const QString& subtitle,
                  const QVector<ListTable>& tables, QString& error) {
  QPdfWriter writer(path);
  writer.setPageSize(QPageSize(QPageSize::A4));
  writer.setPageOrientation(QPageLayout::Landscape);
  writer.setPageMargins(QMarginsF(12, 12, 12, 12), QPageLayout::Millimeter);
  writer.setResolution(300);
  writer.setTitle(title);
  writer.setCreator(QStringLiteral("Mitcad"));
  QPainter painter;
  if (!painter.begin(&writer)) {
    error = QObject::tr("%1 cannot be written").arg(QDir::toNativeSeparators(path));
    return false;
  }
  const int width = writer.width();
  const int height = writer.height();
  QFont heading(QStringLiteral("Helvetica"));
  heading.setPointSizeF(14);
  heading.setBold(true);
  QFont body(QStringLiteral("Helvetica"));
  body.setPointSizeF(8.5);
  QFont bold = body;
  bold.setBold(true);
  int y = 0;
  const auto lineOf = [&painter](const QFont& font) {
    painter.setFont(font);
    return painter.fontMetrics().height();
  };
  const auto room = [&](int needed) {
    if (y + needed > height) {
      writer.newPage();
      y = 0;
    }
  };
  // The title and the design's name with the date.
  int line = lineOf(heading);
  painter.drawText(QRect(0, y, width, line), Qt::AlignLeft | Qt::AlignVCenter, title);
  y += line;
  line = lineOf(body);
  painter.drawText(QRect(0, y, width, line), Qt::AlignLeft | Qt::AlignVCenter, subtitle);
  y += line * 2;
  for (const ListTable& table : tables) {
    if (table.rows.isEmpty()) {
      continue;
    }
    // Columns as wide as their texts need, in the page's width.
    QVector<double> widths(table.columns.size(), 0.0);
    painter.setFont(bold);
    for (int c = 0; c < table.columns.size(); ++c) {
      widths[c] = painter.fontMetrics().horizontalAdvance(table.columns[c]);
    }
    painter.setFont(body);
    for (const QStringList& row : table.rows) {
      for (int c = 0; c < row.size() && c < widths.size(); ++c) {
        widths[c] = std::max(widths[c], static_cast<double>(painter.fontMetrics().horizontalAdvance(row[c])));
      }
    }
    double total = 0.0;
    for (double& w : widths) {
      w += painter.fontMetrics().averageCharWidth() * 2.0;
      total += w;
    }
    const double scale = total > width ? width / total : 1.0;
    const auto drawRow = [&](const QStringList& cells, const QFont& font) {
      const int h = lineOf(font);
      room(h);
      double x = 0.0;
      for (int c = 0; c < cells.size() && c < widths.size(); ++c) {
        const int w = static_cast<int>(widths[c] * scale);
        const QString shown = painter.fontMetrics().elidedText(cells[c], Qt::ElideRight, w - 4);
        painter.drawText(QRect(static_cast<int>(x), y, w, h), Qt::AlignLeft | Qt::AlignVCenter, shown);
        x += widths[c] * scale;
      }
      y += h;
    };
    line = lineOf(bold);
    room(line * 3);
    painter.drawText(QRect(0, y, width, line), Qt::AlignLeft | Qt::AlignVCenter, table.heading);
    y += line;
    drawRow(table.columns, bold);
    painter.drawLine(0, y, width, y);
    for (const QStringList& row : table.rows) {
      drawRow(row, body);
    }
    y += line;
  }
  if (!painter.end()) {
    error = QObject::tr("%1 was not written").arg(QDir::toNativeSeparators(path));
    return false;
  }
  return true;
}

BuildListsDialog::BuildListsDialog(const CommandContext& model, const QString& design, Page page, QWidget* parent)
    : QDialog(parent), m_model(model), m_design(design) {
  setWindowTitle(page == Page::CutList ? tr("Cut List") : tr("Parts List"));
  resize(980, 600);
  auto* layout = new QVBoxLayout(this);
  m_tabs = new QTabWidget;
  layout->addWidget(m_tabs, 1);

  auto* cutPage = new QWidget;
  auto* cut = new QVBoxLayout(cutPage);
  auto* options = new QFormLayout;
  m_kerf = new QDoubleSpinBox;
  m_kerf->setObjectName(QStringLiteral("cutListKerf"));
  m_kerf->setRange(0.0, 20.0);
  m_kerf->setDecimals(1);
  m_kerf->setSingleStep(0.5);
  m_kerf->setSuffix(QStringLiteral(" mm"));
  m_kerf->setValue(3.0);
  m_kerf->setToolTip(tr("What each saw cut takes from the stock, for the stock pieces needed"));
  options->addRow(tr("&Kerf:"), m_kerf);
  cut->addLayout(options);
  const QStringList names{QStringLiteral("cutListRows"), QStringLiteral("cutListStock"), QStringLiteral("cutListOther")};
  const QStringList headings{tr("Cut parts"), tr("Stock"), tr("Other parts")};
  for (int i = 0; i < names.size(); ++i) {
    cut->addWidget(new QLabel(QStringLiteral("<b>%1</b>").arg(headings[i])));
    QTableWidget* table = newTable(names[i]);
    m_cutTables.append(table);
    cut->addWidget(table, i == 0 ? 3 : 1);
  }
  m_tabs->addTab(cutPage, tr("Cut List"));

  auto* partsPage = new QWidget;
  auto* parts = new QVBoxLayout(partsPage);
  // Columns of property values (mitcad#496): the keys typed here.
  auto* columns = new QFormLayout;
  m_columns = new QLineEdit(lastPropertyColumns());
  m_columns->setObjectName(QStringLiteral("buildPartsColumns"));
  m_columns->setPlaceholderText(tr("Property keys, separated by commas: Mark, Pset_WallCommon/FireRating, Mass"));
  m_columns->setToolTip(tr("A column per key with each part's value: its own, its library's or computed"));
  columns->addRow(tr("Property co&lumns:"), m_columns);
  parts->addLayout(columns);
  m_partsTable = newTable(QStringLiteral("buildPartsList"));
  parts->addWidget(m_partsTable, 1);
  m_tabs->addTab(partsPage, tr("Parts List"));
  m_tabs->setCurrentIndex(page == Page::CutList ? 0 : 1);

  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Close);
  auto* copy = buttons->addButton(tr("Copy as &CSV"), QDialogButtonBox::ActionRole);
  copy->setObjectName(QStringLiteral("listCopyCsv"));
  auto* pdf = buttons->addButton(tr("Save as &PDF…"), QDialogButtonBox::ActionRole);
  pdf->setObjectName(QStringLiteral("listSavePdf"));
  mac::styleDialogButtons(buttons);
  layout->addWidget(buttons);
  connect(copy, &QPushButton::clicked, this, &BuildListsDialog::copyCsv);
  connect(pdf, &QPushButton::clicked, this, &BuildListsDialog::savePdf);
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  connect(m_kerf, &QDoubleSpinBox::valueChanged, this, [this] { load(); });
  connect(m_columns, &QLineEdit::editingFinished, this, [this] {
    if (propertyKeys(m_columns->text()) != propertyKeys(lastPropertyColumns())) {
      lastPropertyColumns() = m_columns->text();
      load();
    }
  });
  connect(m_tabs, &QTabWidget::currentChanged, this, [this](int index) {
    qInfo().noquote() << QStringLiteral("Build lists tab: %1").arg(m_tabs->tabText(index).remove(QLatin1Char('&')));
    QTimer::singleShot(0, this, &BuildListsDialog::logLayout);
  });
  load();
  mac::styleDialog(this);
}

void BuildListsDialog::load() {
  const QStringList keys = propertyKeys(m_columns->text());
  QJsonObject cut;
  QJsonObject parts;
  try {
    cut = m_model.queryObject({{QStringLiteral("query"), QStringLiteral("build_cut_list")},
                               {QStringLiteral("kerf"), m_kerf->value()}});
  } catch (const std::exception& error) {
    qWarning().noquote() << QStringLiteral("Cut list not read: %1").arg(errorText(error));
  }
  try {
    QJsonObject query{{QStringLiteral("query"), QStringLiteral("parts_list")}};
    if (!keys.isEmpty()) {
      query.insert(QStringLiteral("properties"), QJsonArray::fromStringList(keys));
    }
    parts = m_model.queryObject(query);
  } catch (const std::exception& error) {
    qWarning().noquote() << QStringLiteral("Parts list not read: %1").arg(errorText(error));
  }
  m_cut = cutListTables(cut);
  m_parts = partsListTables(parts, cut, keys);
  for (int i = 0; i < m_cut.size() && i < m_cutTables.size(); ++i) {
    fill(m_cutTables[i], m_cut[i]);
  }
  fill(m_partsTable, m_parts.first());
  // For UI tests: what the lists hold; a frame's members with their frame
  // (mitcad#522, the last column).
  const bool byFrame = !m_cut[0].columns.isEmpty() && m_cut[0].columns.last() == QObject::tr("Frame");
  for (const QStringList& row : std::as_const(m_cut[0].rows)) {
    if (row[0].isEmpty()) {
      continue; // the timber's totals (mitcad#478)
    }
    const QString frame = byFrame && !row.last().isEmpty() ? QStringLiteral(", frame ") + row.last() : QString();
    qInfo().noquote() << QStringLiteral("Cut list row: %1 x %2, %3 mm, A %4, B %5%6%7")
                             .arg(row[0], row[1], row[2], row[3], row[4],
                                  row[5].isEmpty() ? QString() : QStringLiteral(", machining ") + row[5], frame);
  }
  for (const QStringList& row : std::as_const(m_cut[1].rows)) {
    qInfo().noquote() << QStringLiteral("Cut list stock: %1, %2 mm stock x %3, pieces %4, offcut %5")
                             .arg(row[0], row[1], row[3], row[2], row[4]);
  }
  // Timber (mitcad#478): its columns, the totals and the stock per size.
  if (cut.value(QStringLiteral("timber")).toBool()) {
    for (const QStringList& row : std::as_const(m_cut[0].rows)) {
      if (row.size() > 16 && !row[10].isEmpty()) {
        qInfo().noquote() << QStringLiteral("Cut list timber row: %1: %2 %3 %4, nominal %5, actual %6, %7 m, %8 m3")
                                 .arg(row[1], row[10], row[11].isEmpty() ? QStringLiteral("(no grade)") : row[11], row[12],
                                      row[13], row[14], row[15], row[16]);
      }
    }
    const QJsonObject totals = cut.value(QStringLiteral("totals")).toObject();
    qInfo().noquote() << QStringLiteral("Cut list timber: %1 m, %2 m3, %3 kg; columns %4")
                             .arg(metresText(totals.value(QStringLiteral("running_metres"))),
                                  volumeText(totals.value(QStringLiteral("volume"))),
                                  massText(totals.value(QStringLiteral("mass"))), m_cut[0].columns.join(QStringLiteral(", ")));
    for (const QStringList& row : std::as_const(m_cut[1].rows)) {
      if (row.size() > 7) {
        qInfo().noquote() << QStringLiteral("Cut list timber stock: %1 %2: %3").arg(row[0], row[6], row[7]);
      }
    }
    // Profiled boards (mitcad#477): the profile, cover width and area.
    if (cut.value(QStringLiteral("profiled")).toBool()) {
      for (const QStringList& row : std::as_const(m_cut[0].rows)) {
        if (row.size() > 19 && !row[17].isEmpty()) {
          qInfo().noquote() << QStringLiteral("Cut list profiled row: %1: %2, cover %3 mm, %4 m2")
                                   .arg(row[1], row[17], row[18], row[19]);
        }
      }
      qInfo().noquote() << QStringLiteral("Cut list profiled boards: %1 m2")
                               .arg(areaText(totals.value(QStringLiteral("area"))));
    }
  } else {
    qInfo().noquote() << QStringLiteral("Cut list: no timber columns");
  }
  qInfo().noquote() << QStringLiteral("Cut list: %1 row(s), %2 stock, %3 other part(s), kerf %4 mm")
                           .arg(m_cut[0].rows.size())
                           .arg(m_cut[1].rows.size())
                           .arg(m_cut[2].rows.size())
                           .arg(m_kerf->value());
  qInfo().noquote() << QStringLiteral("Build parts list: %1 row(s)").arg(m_parts.first().rows.size());
  // The property columns (mitcad#496): the keys and each row's values.
  if (!keys.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Build parts list columns: %1").arg(keys.join(QStringLiteral(", ")));
    const int first = static_cast<int>(m_parts.first().columns.size() - keys.size());
    for (const QStringList& row : std::as_const(m_parts.first().rows)) {
      qInfo().noquote() << QStringLiteral("Build parts list row: %1: %2")
                               .arg(row.value(1), row.mid(first).join(QStringLiteral(" | ")));
    }
  }
  QTimer::singleShot(0, this, &BuildListsDialog::logLayout);
}

void BuildListsDialog::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  QTimer::singleShot(0, this, &BuildListsDialog::logLayout);
}

void BuildListsDialog::logLayout() {
  if (!isVisible() || parentWidget() == nullptr || !m_columns->isVisible()) {
    return;
  }
  // In the main window's coordinates, as the other logged places.
  const QPoint origin = parentWidget()->window()->mapToGlobal(QPoint(0, 0));
  const QPoint at = m_columns->mapToGlobal(m_columns->rect().center()) - origin;
  const QString line = QStringLiteral("Parts list columns at %1,%2").arg(at.x()).arg(at.y());
  if (line != m_logged) {
    m_logged = line;
    qInfo().noquote() << line;
  }
}

QVector<ListTable> BuildListsDialog::shownTables() const { return m_tabs->currentIndex() == 0 ? m_cut : m_parts; }

void BuildListsDialog::copyCsv() {
  const QString csv = csvOf(shownTables());
  QApplication::clipboard()->setText(csv);
  const QStringList lines = csv.split(QLatin1Char('\n'), Qt::SkipEmptyParts);
  for (const QString& line : lines) {
    qInfo().noquote() << QStringLiteral("Build list CSV: %1").arg(line);
  }
  qInfo().noquote() << QStringLiteral("Build list copied as CSV: %1 line(s)").arg(lines.size());
}

void BuildListsDialog::savePdf() {
  const bool cutList = m_tabs->currentIndex() == 0;
  const QString what = cutList ? tr("cut list") : tr("parts list");
  const QString name = m_design.isEmpty() ? tr("Untitled") : m_design;
  // The Export group's folder (mitcad#498), else the design's, else Documents.
  const auto* host = dynamic_cast<const DocumentHost*>(parentWidget());
  QString folder = host != nullptr ? host->documentFolder() : QString();
  if (folder.isEmpty()) {
    folder = QStandardPaths::writableLocation(QStandardPaths::DocumentsLocation);
  }
  const QString suggested = QDir(dialogStartFolder(DialogGroup::Export, folder))
                                .filePath(QStringLiteral("%1 %2.pdf").arg(QFileInfo(name).completeBaseName(), what));
  const QString path = dialogSaveFileName(this, cutList ? tr("Save Cut List as PDF") : tr("Save Parts List as PDF"),
                                          suggested, tr("PDF documents (*.pdf)"));
  if (path.isEmpty()) {
    return;
  }
  rememberDialogFolder(DialogGroup::Export, path);
  const QString title = cutList ? tr("Cut list") : tr("Parts list");
  const QString subtitle = tr("%1, %2").arg(name, QDate::currentDate().toString(Qt::ISODate));
  QString error;
  if (!writeListPdf(path, title, subtitle, shownTables(), error)) {
    qWarning().noquote() << QStringLiteral("Build list PDF not written: %1").arg(error);
    sheetWarning(this, tr("Save as PDF"), error);
    return;
  }
  qInfo().noquote() << QStringLiteral("Build list PDF written: %1 (%2 bytes; %3 of %4, %5)")
                           .arg(path)
                           .arg(QFileInfo(path).size())
                           .arg(title, name, QDate::currentDate().toString(Qt::ISODate));
}

} // namespace mitcad::build
