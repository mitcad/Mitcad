// SPDX-License-Identifier: MIT
#include "BuildLists.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <limits>
#include <memory>
#include <utility>

#include <QApplication>
#include <QClipboard>
#include <QCheckBox>
#include <QComboBox>
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
#include <QPair>
#include <QPrinter>
#include <QPushButton>
#include <QShortcut>
#include <QStandardPaths>
#include <QSettings>
#include <QSaveFile>
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
#include "../library/SupplierShops.hpp"
#include "../printing/PaperPrint.hpp"

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

void fill(QTableWidget* table, const ListTable& list) {
  table->clear();
  table->setColumnCount(static_cast<int>(list.columns.size()));
  table->setHorizontalHeaderLabels(list.columns);
  table->setRowCount(static_cast<int>(list.rows.size()));
  for (int r = 0; r < list.rows.size(); ++r) {
    const QStringList& cells = list.rows[r];
    for (int c = 0; c < cells.size() && c < list.columns.size(); ++c) {
      auto* item = new QTableWidgetItem(cells[c]);
      item->setToolTip(list.tooltips.value(r));
      table->setItem(r, c, item);
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

// The lines of a list's text (CRLF ends, mitcad#1641), blank ones left out,
// for the log.
QStringList logLines(const QString& text) {
  return text.split(QStringLiteral("\r\n"), Qt::SkipEmptyParts);
}

} // namespace

QVector<ListTable> cutListTables(const QJsonObject& answer) {
  // Timber (mitcad#478): its columns only when the design has timber.
  const bool timber = answer.value(QStringLiteral("timber")).toBool();
  ListTable rows{QObject::tr("Cut List"),
                 {QObject::tr("Quantity"), QObject::tr("Component"), QObject::tr("Length (mm)"), QObject::tr("End A"),
                  QObject::tr("End B"), QObject::tr("Machining"), QObject::tr("Own Features"), QObject::tr("Supplier"),
                  QObject::tr("Part Number"), QObject::tr("Mass Each (kg)")},
                 {}};
  rows.nameColumn = 1;
  if (timber) {
    rows.columns << QObject::tr("Material") << QObject::tr("Grade") << QObject::tr("Finish")
                 << QObject::tr("Nominal (mm)") << QObject::tr("Actual (mm)") << QObject::tr("Running Meters (m)")
                 << QObject::tr("Volume (m³)");
  }
  // Profiled boards (mitcad#477): their columns only when the design has
  // them.
  const bool profiled = timber && answer.value(QStringLiteral("profiled")).toBool();
  if (profiled) {
    rows.columns << QObject::tr("Profile") << QObject::tr("Cover Width (mm)") << QObject::tr("Area Covered (m²)");
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
                  {QObject::tr("Component"), QObject::tr("Stock Length (mm)"),
                   QObject::tr("Pieces (mm, in cutting order; | between stock pieces)"), QObject::tr("Stock Pieces"),
                   QObject::tr("Offcut (mm)"), QObject::tr("Too Long for the Stock (mm)")},
                  {}};
  stock.nameColumn = 0;
  if (timber) {
    // Stock per size, each piece's stock length (mitcad#478).
    stock.columns << QObject::tr("Size") << QObject::tr("Stock Lengths (mm)");
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
  ListTable other{QObject::tr("Other Components"),
                  {QObject::tr("Quantity"), QObject::tr("Component"), QObject::tr("Supplier"),
                   QObject::tr("Part Number")},
                  {}};
  other.nameColumn = 1;
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
  ListTable parts{QObject::tr("Bill of Materials"),
                  {QObject::tr("Quantity"), QObject::tr("Component"), QObject::tr("Supplier"),
                   QObject::tr("Part Number"),
                   QObject::tr("Supplier Part Numbers"), QObject::tr("Library"), QObject::tr("Version"),
                   QObject::tr("License")},
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
  // The product links in the region chosen in Preferences (mitcad#950),
  // after the suppliers' part numbers, when a row has any.
  const QJsonArray listed = answer.value(QStringLiteral("rows")).toArray();
  const bool linked = std::any_of(listed.cbegin(), listed.cend(),
                                  [](const QJsonValue& row) { return !productUrls(row.toObject()).isEmpty(); });
  if (linked) {
    parts.columns.insert(5, QObject::tr("Product Pages"));
  }
  // A structured or parts-only view (mitcad#1985): each row's item number
  // first and its mass of one after the component, indented by its level
  // in the structured view.
  const QString view = answer.value(QStringLiteral("view")).toString();
  const bool numbered = view == QStringLiteral("structured") || view == QStringLiteral("parts");
  if (numbered) {
    parts.columns.insert(2, QObject::tr("Mass (kg)"));
    parts.columns.prepend(QObject::tr("Item"));
  }
  parts.columns << keys;
  parts.nameColumn = numbered ? 2 : 1;
  for (const QJsonValue& value : listed) {
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
    if (linked) {
      cells.insert(5, productUrls(row).join(QLatin1Char(' ')));
    }
    if (timber) {
      const QJsonObject own = material.value(text(row, "component"));
      cells << text(own, "species") << text(own, "grade");
    }
    if (numbered) {
      const int level = std::max(1, row.value(QStringLiteral("level")).toInt(1));
      cells[1] = QString(2 * (level - 1), QLatin1Char(' ')) + cells[1];
      // A Reference counts for no quantity (mitcad#1985).
      if (text(row, "bom_structure") == QStringLiteral("Reference")) {
        cells[0] = QObject::tr("Ref");
      }
      cells.insert(2, massText(row.value(QStringLiteral("mass"))));
      cells.prepend(text(row, "item"));
    }
    // The chosen property keys (mitcad#496): empty where the part has none.
    const QJsonObject properties = row.value(QStringLiteral("properties")).toObject();
    for (const QString& key : keys) {
      cells << properties.value(key).toString();
    }
    parts.rows.append(cells);
    QStringList names;
    for (const QJsonValue& name : row.value(QStringLiteral("group_names")).toArray()) {
      names << name.toString();
    }
    parts.tooltips.append(names.join(QLatin1Char('\n')));
  }
  return {parts};
}

QVector<ListTable> sheetMetalTables(const QJsonObject& answer) {
  ListTable sheets{QObject::tr("Sheet Metal"),
                   {QObject::tr("Quantity"), QObject::tr("Component"), QObject::tr("Body"), QObject::tr("Thickness (mm)"),
                    QObject::tr("Material"), QObject::tr("Flat Width (mm)"), QObject::tr("Flat Height (mm)"),
                    QObject::tr("Area (mm²)"), QObject::tr("Bends"), QObject::tr("Flat Pattern")},
                   {}};
  sheets.nameColumn = 1;
  const auto number = [](const QJsonValue& value, int decimals) {
    return value.isDouble() ? QString::number(value.toDouble(), 'f', decimals) : QString();
  };
  for (const QJsonValue& value : answer.value(QStringLiteral("rows")).toArray()) {
    const QJsonObject row = value.toObject();
    sheets.rows.append({countText(row.value(QStringLiteral("quantity"))), text(row, "component_name"), text(row, "name"),
                        number(row.value(QStringLiteral("thickness")), 2), text(row, "material"),
                        number(row.value(QStringLiteral("width")), 1), number(row.value(QStringLiteral("height")), 1),
                        number(row.value(QStringLiteral("area")), 1),
                        row.value(QStringLiteral("bends")).isDouble() ? countText(row.value(QStringLiteral("bends")))
                                                                     : QString(),
                        text(row, "problem")});
  }
  return {sheets};
}

BuildListsDialog::BuildListsDialog(const CommandContext& model, const QString& design, Page page, QWidget* parent)
    : QDialog(parent), m_model(model), m_design(design) {
  setWindowTitle(page == Page::CutList ? tr("Cut List")
                 : page == Page::SheetMetal ? tr("Sheet Metal")
                                            : tr("Bill of Materials"));
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
  const QStringList headings{tr("Cut components"), tr("Stock"), tr("Other components")};
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
  m_columns->setToolTip(tr("A column per key with each component's value: its own, its library's or computed"));
  columns->addRow(tr("Property co&lumns:"), m_columns);
  parts->addLayout(columns);
  m_groupIdentical = new QCheckBox(tr("Group identical parts"));
  m_groupIdentical->setObjectName(QStringLiteral("buildPartsGroupIdentical"));
  m_groupIdentical->setToolTip(tr("Combine equal measured geometry and material in this report; the design is unchanged"));
  m_groupIdentical->setChecked(QSettings().value(QStringLiteral("build/groupIdenticalParts"), false).toBool());
  parts->addWidget(m_groupIdentical);
  // The view (mitcad#1985): every component as before, or as the assembly
  // nests them, or the parts only, both applying the BOM Structures; the
  // rows of one kind (mitcad#1980) and of one BOM Structure (the model's
  // `parts_list` filters them).
  auto* filters = new QHBoxLayout;
  m_view = new QComboBox;
  m_view->setObjectName(QStringLiteral("buildPartsView"));
  m_view->setToolTip(tr("Lists every component, the components as the assembly nests them, or only the parts"));
  m_view->addItem(tr("Every component"), QStringLiteral("list"));
  m_view->addItem(tr("Structured"), QStringLiteral("structured"));
  m_view->addItem(tr("Parts only"), QStringLiteral("parts"));
  auto* viewLabel = new QLabel(tr("&View:"));
  viewLabel->setBuddy(m_view);
  filters->addWidget(viewLabel);
  filters->addWidget(m_view);
  filters->addSpacing(12);
  m_kind = new QComboBox;
  m_kind->setObjectName(QStringLiteral("buildPartsKind"));
  m_kind->setToolTip(tr("Lists only the components of the kind chosen, such as sheet metal or library parts"));
  m_kind->addItem(tr("All kinds"), QString());
  // Shown in words, sent as the model names them.
  const QList<QPair<QString, QString>> kinds{
      {tr("Sheet metal"), QStringLiteral("Sheet metal")}, {tr("Build profile"), QStringLiteral("Build profile")},
      {tr("Library part"), QStringLiteral("Library part")}, {tr("Imported"), QStringLiteral("Imported")},
      {tr("Solid"), QStringLiteral("Solid")},             {tr("Mixed"), QStringLiteral("Mixed")},
      {tr("Assembly"), QStringLiteral("Assembly")}};
  for (const auto& kind : kinds) {
    m_kind->addItem(kind.first, kind.second);
  }
  auto* kindLabel = new QLabel(tr("Kin&d:"));
  kindLabel->setBuddy(m_kind);
  m_structure = new QComboBox;
  m_structure->setObjectName(QStringLiteral("buildPartsStructure"));
  m_structure->setToolTip(tr("Lists only the components of the BOM Structure chosen, such as Normal or Purchased"));
  m_structure->addItem(tr("All structures"), QString());
  const QList<QPair<QString, QString>> structures{
      {tr("Normal"), QStringLiteral("Normal")},       {tr("Purchased"), QStringLiteral("Purchased")},
      {tr("Phantom"), QStringLiteral("Phantom")},     {tr("Reference"), QStringLiteral("Reference")},
      {tr("Inseparable"), QStringLiteral("Inseparable")}};
  for (const auto& structure : structures) {
    m_structure->addItem(structure.first, structure.second);
  }
  auto* structureLabel = new QLabel(tr("BOM &Structure:"));
  structureLabel->setBuddy(m_structure);
  filters->addWidget(kindLabel);
  filters->addWidget(m_kind);
  filters->addSpacing(12);
  filters->addWidget(structureLabel);
  filters->addWidget(m_structure);
  filters->addStretch(1);
  parts->addLayout(filters);
  m_partsTable = newTable(QStringLiteral("buildPartsList"));
  parts->addWidget(m_partsTable, 1);
  m_tabs->addTab(partsPage, tr("Bill of Materials"));

  // The Sheet Metal list (mitcad#1980): each sheet part's developed blank.
  auto* sheetPage = new QWidget;
  auto* sheet = new QVBoxLayout(sheetPage);
  m_sheetTable = newTable(QStringLiteral("buildSheetMetalList"));
  sheet->addWidget(m_sheetTable, 1);
  auto* toSheet = new QHBoxLayout;
  m_cutSheet = new QComboBox;
  m_cutSheet->setObjectName(QStringLiteral("sheetMetalCutSheet"));
  m_cutSheet->setToolTip(tr("The cut sheet the Flat Patterns go onto: a new one or one of the design's"));
  auto* cutSheetLabel = new QLabel(tr("Cut s&heet:"));
  cutSheetLabel->setBuddy(m_cutSheet);
  auto* addButton = new QPushButton(tr("&Add All to Cut Sheet"));
  addButton->setObjectName(QStringLiteral("sheetMetalAddToCutSheet"));
  addButton->setToolTip(tr("Puts every sheet part's Flat Pattern on the cut sheet, as many as the design has"));
  m_sheetNote = new QLabel;
  m_sheetNote->setObjectName(QStringLiteral("sheetMetalNote"));
  toSheet->addWidget(cutSheetLabel);
  toSheet->addWidget(m_cutSheet);
  toSheet->addWidget(addButton);
  toSheet->addWidget(m_sheetNote, 1);
  sheet->addLayout(toSheet);
  m_tabs->addTab(sheetPage, tr("Sheet Metal"));
  m_tabs->setCurrentIndex(page == Page::CutList ? 0 : page == Page::SheetMetal ? 2 : 1);

  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Close);
  auto* copy = buttons->addButton(tr("Copy as &CSV"), QDialogButtonBox::ActionRole);
  copy->setObjectName(QStringLiteral("listCopyCsv"));
  auto* saveCsv = buttons->addButton(tr("Save as CSV…"), QDialogButtonBox::ActionRole);
  saveCsv->setObjectName(QStringLiteral("listSaveCsv"));
  // The tables as aligned plain text, for an email (mitcad#1641).
  auto* copyTextButton = buttons->addButton(tr("Copy as &Text"), QDialogButtonBox::ActionRole);
  copyTextButton->setObjectName(QStringLiteral("listCopyText"));
  copyTextButton->setToolTip(tr("Copies the tab shown as plain text with its columns aligned, for an email"));
  auto* saveTextButton = buttons->addButton(tr("Save as Text…"), QDialogButtonBox::ActionRole);
  saveTextButton->setObjectName(QStringLiteral("listSaveText"));
  saveTextButton->setToolTip(tr("Saves the tab shown as a text file with its columns aligned"));
  auto* pdf =buttons->addButton(tr("Save as &PDF…"), QDialogButtonBox::ActionRole);
  pdf->setObjectName(QStringLiteral("listSavePdf"));
  // File › Print for the list (mitcad#1089): the window is modal, so it has
  // the button and the key itself.
  auto* printButton = buttons->addButton(tr("Pr&int…"), QDialogButtonBox::ActionRole);
  printButton->setObjectName(QStringLiteral("listPrint"));
  printButton->setToolTip(tr("Prints the tab shown as its PDF lays it out"));
  auto* printKey = new QShortcut(QKeySequence::Print, this);
  connect(printKey, &QShortcut::activated, this, &BuildListsDialog::print);
  connect(printButton, &QPushButton::clicked, this, &BuildListsDialog::print);
  mac::styleDialogButtons(buttons);
  layout->addWidget(buttons);
  connect(copy, &QPushButton::clicked, this, &BuildListsDialog::copyCsv);
  connect(saveCsv, &QPushButton::clicked, this, &BuildListsDialog::saveCsv);
  connect(copyTextButton, &QPushButton::clicked, this, &BuildListsDialog::copyText);
  connect(saveTextButton, &QPushButton::clicked, this, &BuildListsDialog::saveText);
  connect(m_groupIdentical, &QCheckBox::toggled, this, [this](bool checked) {
    QSettings().setValue(QStringLiteral("build/groupIdenticalParts"), checked);
    load();
  });
  connect(pdf, &QPushButton::clicked, this, &BuildListsDialog::savePdf);
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  // A double click or Return on a row's product pages opens one (a menu
  // of them when there are several, mitcad#950).
  connect(m_partsTable, &QAbstractItemView::activated, this, [this](const QModelIndex& index) {
    const QTableWidgetItem* heading = m_partsTable->horizontalHeaderItem(index.column());
    if (heading == nullptr || heading->text() != QObject::tr("Product Pages")) {
      return;
    }
    const QRect cell = m_partsTable->visualRect(index);
    openProductLinks(this, m_partsTable->viewport()->mapToGlobal(cell.bottomLeft()),
                     index.data().toString().split(QLatin1Char(' '), Qt::SkipEmptyParts));
  });
  connect(m_kerf, &QDoubleSpinBox::valueChanged, this, [this] { load(); });
  connect(m_kind, &QComboBox::currentIndexChanged, this, [this] { load(); });
  connect(m_structure, &QComboBox::currentIndexChanged, this, [this] { load(); });
  connect(m_view, &QComboBox::currentIndexChanged, this, [this] { load(); });
  connect(addButton, &QPushButton::clicked, this, &BuildListsDialog::addToCutSheet);
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
  loadCutSheets();
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
    // Group identical parts combines equal parts in every view: the list,
    // the parts-only view across the design, the structured view among
    // siblings (mitcad#2215).
    const QString view = m_view->currentData().toString();
    QJsonObject query{{QStringLiteral("query"), QStringLiteral("parts_list")},
                      {QStringLiteral("group_identical"), m_groupIdentical->isChecked()},
                      {QStringLiteral("view"), view}};
    if (!keys.isEmpty()) {
      query.insert(QStringLiteral("properties"), QJsonArray::fromStringList(keys));
    }
    // The filters (mitcad#1980).
    const QString kind = m_kind->currentData().toString();
    if (!kind.isEmpty()) {
      QJsonArray kinds;
      kinds.append(kind);
      query.insert(QStringLiteral("kind"), kinds);
    }
    const QString structure = m_structure->currentData().toString();
    if (!structure.isEmpty()) {
      QJsonArray wanted;
      wanted.append(structure);
      query.insert(QStringLiteral("bom_structure"), wanted);
    }
    parts = m_model.queryObject(query);
    // The articles' product links (mitcad#950).
    parts.insert(QStringLiteral("rows"), rowsWithProductLinks(parts.value(QStringLiteral("rows")).toArray()));
  } catch (const std::exception& error) {
    qWarning().noquote() << QStringLiteral("Parts list not read: %1").arg(errorText(error));
  }
  QJsonObject sheets;
  try {
    sheets = m_model.queryObject({{QStringLiteral("query"), QStringLiteral("sheet_metal_list")}});
  } catch (const std::exception& error) {
    qWarning().noquote() << QStringLiteral("Sheet metal list not read: %1").arg(errorText(error));
  }
  m_sheetRows = sheets.value(QStringLiteral("rows")).toArray();
  m_cut = cutListTables(cut);
  m_parts = partsListTables(parts, cut, keys);
  m_sheets = sheetMetalTables(sheets);
  fill(m_sheetTable, m_sheets.first());
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
  // The filters (mitcad#1980).
  qInfo().noquote() << QStringLiteral("Build parts list filter: kind %1, structure %2")
                           .arg(m_kind->currentData().toString().isEmpty() ? QStringLiteral("all")
                                                                         : m_kind->currentData().toString(),
                                m_structure->currentData().toString().isEmpty()
                                    ? QStringLiteral("all")
                                    : m_structure->currentData().toString());
  // The view (mitcad#1985): its rows with their items, quantities and
  // structures, indented by level, and the design's totals.
  const bool numbered = m_view->currentData().toString() != QStringLiteral("list");
  const int at = numbered ? 1 : 0;
  qInfo().noquote() << QStringLiteral("Build parts list view: %1").arg(m_view->currentData().toString());
  if (numbered) {
    const QJsonArray viewRows = parts.value(QStringLiteral("rows")).toArray();
    for (const QJsonValue& value : viewRows) {
      const QJsonObject row = value.toObject();
      const int level = std::max(1, row.value(QStringLiteral("level")).toInt(1));
      const QString item = text(row, "item").isEmpty() ? QStringLiteral("-") : text(row, "item");
      qInfo().noquote() << QStringLiteral("Build BOM item: %1%2 %3 x %4 (%5)")
                               .arg(QString(2 * (level - 1), QLatin1Char(' ')), item,
                                    text(row, "designation").isEmpty() ? text(row, "name") : text(row, "designation"),
                                    countText(row.value(QStringLiteral("quantity"))), text(row, "bom_structure"));
    }
    const QJsonObject totals = parts.value(QStringLiteral("totals")).toObject();
    qInfo().noquote() << QStringLiteral("Build BOM totals: %1 kg").arg(massText(totals.value(QStringLiteral("mass"))));
  }
  // The Sheet Metal list (mitcad#1980).
  for (const QStringList& row : std::as_const(m_sheets.first().rows)) {
    const QString problem = row.value(9).isEmpty() ? QString() : QStringLiteral(": ") + row.value(9);
    qInfo().noquote() << QStringLiteral("Sheet metal row: %1 x %2 / %3: %4 mm %5, %6 x %7 mm, %8 mm2, %9 bend(s)")
                                 .arg(row.value(0), row.value(1), row.value(2), row.value(3), row.value(4), row.value(5),
                                      row.value(6), row.value(7), row.value(8)) +
                             problem;
  }
  qInfo().noquote() << QStringLiteral("Sheet metal list: %1 row(s)").arg(m_sheets.first().rows.size());
  const qsizetype pages = m_parts.first().columns.indexOf(QObject::tr("Product Pages"));
  if (pages >= 0) {
    for (const QStringList& row : std::as_const(m_parts.first().rows)) {
      if (!row.value(pages).isEmpty()) {
        qInfo().noquote() << QStringLiteral("Build BOM product pages: %1: %2")
                                 .arg(row.value(1 + at).trimmed(), row.value(pages));
      }
    }
  }
  for (const QStringList& row : std::as_const(m_parts.first().rows)) {
    qInfo().noquote() << QStringLiteral("Build BOM row: %1 x %2").arg(row.value(at), row.value(1 + at).trimmed());
  }
  // The property columns (mitcad#496): the keys and each row's values.
  if (!keys.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Build parts list columns: %1").arg(keys.join(QStringLiteral(", ")));
    const int first = static_cast<int>(m_parts.first().columns.size() - keys.size());
    for (const QStringList& row : std::as_const(m_parts.first().rows)) {
      qInfo().noquote() << QStringLiteral("Build parts list row: %1: %2")
                               .arg(row.value(1 + at).trimmed(), row.mid(first).join(QStringLiteral(" | ")));
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

QVector<ListTable> BuildListsDialog::shownTables() const {
  switch (m_tabs->currentIndex()) {
  case 0:
    return m_cut;
  case 2:
    return m_sheets;
  default:
    return m_parts;
  }
}

QString BuildListsDialog::shownTitle() const {
  switch (m_tabs->currentIndex()) {
  case 0:
    return tr("Cut List");
  case 2:
    return tr("Sheet Metal");
  default:
    return tr("Bill of Materials");
  }
}

void BuildListsDialog::loadCutSheets() {
  const QString chosen = m_cutSheet->currentData().toString();
  m_cutSheet->clear();
  m_cutSheet->addItem(tr("New Cut Sheet"), QString());
  QJsonArray drawings;
  try {
    drawings = m_model.queryArray(QStringLiteral("drawings"));
  } catch (const std::exception& error) {
    qWarning().noquote() << QStringLiteral("Cut sheets not read: %1").arg(errorText(error));
  }
  for (const QJsonValue& value : drawings) {
    const QJsonObject drawing = value.toObject();
    if (text(drawing, "kind") == QStringLiteral("cut_sheet")) {
      m_cutSheet->addItem(text(drawing, "name"), drawing.value(QStringLiteral("drawing")).toVariant().toString());
    }
  }
  const int at = m_cutSheet->findData(chosen);
  m_cutSheet->setCurrentIndex(at < 0 ? 0 : at);
}

void BuildListsDialog::addToCutSheet() {
  // Each part that can be developed, as many times as the design has it.
  QJsonArray bodies;
  int count = 0;
  for (const QJsonValue& value : std::as_const(m_sheetRows)) {
    const QJsonObject row = value.toObject();
    const int quantity = row.value(QStringLiteral("quantity")).toInt();
    if (!text(row, "problem").isEmpty() || quantity <= 0) {
      continue;
    }
    QJsonObject body;
    body.insert(QStringLiteral("body"), text(row, "body"));
    body.insert(QStringLiteral("count"), quantity);
    bodies.append(body);
    count += quantity;
  }
  if (bodies.isEmpty()) {
    m_sheetNote->setText(tr("No sheet part's Flat Pattern can be developed"));
    qInfo().noquote() << QStringLiteral("Sheet metal parts not added: none can be developed");
    return;
  }
  auto* host = dynamic_cast<DocumentHost*>(parentWidget());
  if (host == nullptr) {
    return;
  }
  const QString drawing = m_cutSheet->currentData().toString();
  QJsonObject command{{QStringLiteral("flat"), true}, {QStringLiteral("bodies"), bodies}};
  if (drawing.isEmpty()) {
    command.insert(QStringLiteral("cmd"), QStringLiteral("add_cut_sheet"));
  } else {
    command.insert(QStringLiteral("cmd"), QStringLiteral("add_cut_sheet_parts"));
    command.insert(QStringLiteral("drawing"), drawing);
  }
  QJsonObject result;
  if (!host->runModelCommand(command, &result)) {
    m_sheetNote->setText(host->lastError());
    qWarning().noquote() << QStringLiteral("Sheet metal parts not added: %1").arg(host->lastError());
    return;
  }
  const QString uid = result.value(QStringLiteral("drawing")).toVariant().toString();
  loadCutSheets();
  const int at = m_cutSheet->findData(uid);
  if (at >= 0) {
    m_cutSheet->setCurrentIndex(at);
  }
  const QString name = m_cutSheet->currentText();
  const int warnings = static_cast<int>(result.value(QStringLiteral("warnings")).toArray().size());
  m_sheetNote->setText(warnings == 0 ? tr("%n Flat Pattern(s) added to %1", nullptr, count).arg(name)
                                     : tr("%n Flat Pattern(s) added to %1, with warnings", nullptr, count).arg(name));
  qInfo().noquote() << QStringLiteral("Sheet metal parts added to %1: %2 part(s), %3 warning(s)")
                           .arg(name)
                           .arg(count)
                           .arg(warnings);
}

void BuildListsDialog::copyCsv() {
  const QString csv = csvOf(shownTables());
  QApplication::clipboard()->setText(csv);
  const QStringList lines = logLines(csv);
  for (const QString& line : lines) {
    qInfo().noquote() << QStringLiteral("Build list CSV: %1").arg(line);
  }
  qInfo().noquote() << QStringLiteral("Build list copied as CSV: %1 line(s)").arg(lines.size());
}

void BuildListsDialog::copyText() {
  const QString text = textTableOf(shownTables());
  QApplication::clipboard()->setText(text);
  const QStringList lines = logLines(text);
  for (const QString& line : lines) {
    qInfo().noquote() << QStringLiteral("Build list text: %1").arg(line);
  }
  qInfo().noquote() << QStringLiteral("Build list copied as text: %1 line(s)").arg(lines.size());
}

void BuildListsDialog::print() {
  const QString name = m_design.isEmpty() ? tr("Untitled") : m_design;
  const QString title = shownTitle();
  const QString subtitle = tr("%1, %2").arg(name, QDate::currentDate().toString(Qt::ISODate));
  // The PDF's paper and margins to start with; the dialog may change them.
  // Found without freezing the window (mitcad#1591).
  const std::unique_ptr<QPrinter> made = printing::newPrinter(this);
  QPrinter& printer = *made;
  printer.setDocName(QStringLiteral("%1 %2").arg(name, title));
  printer.setCreator(QStringLiteral("MitCad"));
  printer.setPageLayout(listPageLayout());
  if (!printing::askPrinter(printer, this, 0)) {
    return;
  }
  const int pages = paintListPages(printer, title, subtitle, shownTables());
  if (pages < 0) {
    const QString error = tr("Could not print to %1.").arg(printing::printerTarget(printer));
    qWarning().noquote() << QStringLiteral("Build list not printed: %1").arg(error);
    sheetWarning(this, tr("Print"), error);
    return;
  }
  qInfo().noquote() << QStringLiteral("Printed %1: %2 page(s) to %3").arg(title).arg(pages).arg(printing::printerTarget(printer));
}

void BuildListsDialog::saveCsv() {
  saveList(tr("Save List as CSV"), QStringLiteral(".csv"), tr("CSV files (*.csv)"), tr("Save as CSV"),
           QStringLiteral("CSV"), csvOf(shownTables()));
}

void BuildListsDialog::saveText() {
  saveList(tr("Save List as Text"), QStringLiteral(".txt"), tr("Text files (*.txt)"), tr("Save as Text"),
           QStringLiteral("text"), textTableOf(shownTables()));
}

bool BuildListsDialog::saveList(const QString& caption, const QString& extension, const QString& filter,
                                const QString& errorTitle, const QString& what, const QString& contents) {
  const QString name = m_design.isEmpty() ? tr("Untitled") : QFileInfo(m_design).completeBaseName();
  const auto* host = dynamic_cast<const DocumentHost*>(parentWidget());
  QString fallback = host != nullptr ? host->documentFolder() : QString();
  if (fallback.isEmpty()) {
    fallback = QStandardPaths::writableLocation(QStandardPaths::DocumentsLocation);
  }
  const QString folder = dialogStartFolder(DialogGroup::Export, fallback);
  const QString path = dialogSaveFileName(this, caption, QDir(folder).filePath(name + extension), filter);
  if (path.isEmpty()) {
    return false;
  }
  QSaveFile file(path);
  const QByteArray bytes = contents.toUtf8();
  if (!file.open(QIODevice::WriteOnly) || file.write(bytes) != bytes.size() || !file.commit()) {
    qWarning().noquote() << QStringLiteral("Build list %1 not written: %2").arg(what, file.errorString());
    sheetCritical(this, errorTitle, file.errorString());
    return false;
  }
  rememberDialogFolder(DialogGroup::Export, path);
  qInfo().noquote() << QStringLiteral("Build list saved as %1: %2").arg(what, QDir::toNativeSeparators(path));
  return true;
}

void BuildListsDialog::savePdf() {
  const int tab = m_tabs->currentIndex();
  const QString what = tab == 0 ? tr("cut list") : tab == 2 ? tr("sheet metal list") : tr("bill of materials");
  const QString name = m_design.isEmpty() ? tr("Untitled") : m_design;
  // The Export group's folder (mitcad#498), else the design's, else Documents.
  const auto* host = dynamic_cast<const DocumentHost*>(parentWidget());
  QString folder = host != nullptr ? host->documentFolder() : QString();
  if (folder.isEmpty()) {
    folder = QStandardPaths::writableLocation(QStandardPaths::DocumentsLocation);
  }
  const QString suggested = QDir(dialogStartFolder(DialogGroup::Export, folder))
                                .filePath(QStringLiteral("%1 %2.pdf").arg(QFileInfo(name).completeBaseName(), what));
  const QString caption = tab == 0   ? tr("Save Cut List as PDF")
                          : tab == 2 ? tr("Save Sheet Metal List as PDF")
                                     : tr("Save Bill of Materials as PDF");
  const QString path = dialogSaveFileName(this, caption, suggested, tr("PDF documents (*.pdf)"));
  if (path.isEmpty()) {
    return;
  }
  rememberDialogFolder(DialogGroup::Export, path);
  const QString title = shownTitle();
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
