// SPDX-License-Identifier: MIT
// Machines and Tools… (mitcad#1013; CamMachinesDialog.hpp).
#include "CamMachinesDialog.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <exception>
#include <optional>
#include <utility>

#include <QAction>
#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonDocument>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QLocale>
#include <QMenu>
#include <QPushButton>
#include <QSignalBlocker>
#include <QSpinBox>
#include <QStringList>
#include <QTabBar>
#include <QTabWidget>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/ModelMessages.hpp"
#include "../framework/PanelTree.hpp"
#include "../framework/UserText.hpp"

namespace mitcad {
namespace {

QString text(const QJsonObject& object, const char* key) { return object.value(QLatin1String(key)).toString(); }

double numberOf(const QJsonObject& object, const char* key, double fallback = 0.0) {
  return object.value(QLatin1String(key)).toDouble(fallback);
}

// A number without trailing zeros, as typed back into a field.
QString plain(double value) {
  QString shown = QString::number(value, 'f', 4);
  while (shown.contains(QLatin1Char('.')) && (shown.endsWith(QLatin1Char('0')) || shown.endsWith(QLatin1Char('.')))) {
    shown.chop(1);
  }
  return shown == QStringLiteral("-0") ? QStringLiteral("0") : shown;
}

// The tool shapes by the model's kinds, with their names.
const QStringList& shapeKinds() {
  static const QStringList kinds{QStringLiteral("flat"),  QStringLiteral("ball"),    QStringLiteral("bull"),
                                 QStringLiteral("drill"), QStringLiteral("chamfer"), QStringLiteral("engraver")};
  return kinds;
}

// A lathe's tool kinds (mitcad#2230), the model's lathe tools.
const QStringList& latheKinds() {
  static const QStringList kinds{QStringLiteral("turning"), QStringLiteral("boring"), QStringLiteral("grooving"),
                                 QStringLiteral("parting"), QStringLiteral("threading")};
  return kinds;
}

QString shapeName(const QString& kind) {
  if (kind == QStringLiteral("flat")) return CamMachinesDialog::tr("Flat end mill");
  if (kind == QStringLiteral("ball")) return CamMachinesDialog::tr("Ball end mill");
  if (kind == QStringLiteral("bull")) return CamMachinesDialog::tr("Bull nose end mill");
  if (kind == QStringLiteral("drill")) return CamMachinesDialog::tr("Drill");
  if (kind == QStringLiteral("chamfer")) return CamMachinesDialog::tr("Chamfer mill");
  if (kind == QStringLiteral("engraver")) return CamMachinesDialog::tr("Engraving cutter");
  if (kind == QStringLiteral("turning")) return CamMachinesDialog::tr("Turning tool");
  if (kind == QStringLiteral("boring")) return CamMachinesDialog::tr("Boring bar");
  if (kind == QStringLiteral("grooving")) return CamMachinesDialog::tr("Grooving tool");
  if (kind == QStringLiteral("parting")) return CamMachinesDialog::tr("Parting tool");
  if (kind == QStringLiteral("threading")) return CamMachinesDialog::tr("Threading tool");
  return kind;
}

// Whether a machine (`cam_machines`' `machine`) is a lathe.
bool isLathe(const QJsonObject& body) { return body.value(QStringLiteral("lathe")).isObject(); }

// Whether a lathe has a C axis and live tools (mitcad#2067): milling tools
// and drills on its turret are its live tools.
bool latheMills(const QJsonObject& body) {
  const QJsonObject axes =
      body.value(QStringLiteral("lathe")).toObject().value(QStringLiteral("axes")).toObject();
  return axes.value(QStringLiteral("c")).toBool() && axes.value(QStringLiteral("live_tools")).toBool();
}

QString zoneAxis(int axis, bool lathe) {
  return lathe && axis == 0 ? CamMachinesDialog::tr("X (radius)") : QString(QLatin1Char("XYZ"[axis]));
}

QLineEdit* field(const QString& objectName, const QString& shown, const QString& tip = QString()) {
  auto* edit = new QLineEdit(shown);
  edit->setObjectName(objectName);
  if (!tip.isEmpty()) {
    edit->setToolTip(tip);
  }
  return edit;
}

// The fields of a tool (cam_tool_put's `tool`, its values as typed: a
// length with its unit, "6 mm", "0.25 in"; angles in degrees), from a
// tool as the design keeps it (millimetres and degrees); none when
// cancelled. `taken`: the numbers of the machine's other tools. On a
// `lathe` the lathe's tool kinds with their own fields (mitcad#2230): the
// number is the turret station, the diameter the holder's size (a boring
// bar's diameter), the flute length the depth of cut, the stick-out the
// overhang; the milling fields are hidden there and keep their values. On
// a lathe that `mills` (a C axis and live tools, mitcad#2067) the milling
// shapes are its live tools too, with their milling fields and the holder
// they sit in on the turret: axial (the face) or radial (the diameter).
std::optional<QJsonObject> askTool(QWidget* parent, const QJsonObject& tool, int toolNumber, const QList<int>& taken,
                                   bool lathe, bool mills) {
  QDialog dialog(parent);
  dialog.setObjectName(QStringLiteral("camToolDialog"));
  dialog.setWindowTitle(tool.isEmpty() ? CamMachinesDialog::tr("Add Tool") : CamMachinesDialog::tr("Edit Tool"));
  auto* form = new QFormLayout;
  auto* numberBox = new QSpinBox;
  numberBox->setObjectName(QStringLiteral("camToolNumber"));
  numberBox->setRange(1, lathe ? 99 : 999);
  numberBox->setValue(toolNumber);
  numberBox->setToolTip(lathe ? CamMachinesDialog::tr("The turret station the tool sits in, also its offset number")
                              : CamMachinesDialog::tr("The tool number, also its place in a tool changer"));
  form->addRow(lathe ? CamMachinesDialog::tr("Turret Station") : CamMachinesDialog::tr("Number"), numberBox);
  QLineEdit* name = field(QStringLiteral("camToolName"), text(tool, "name"),
                          lathe ? CamMachinesDialog::tr("Empty: the holder size and the kind")
                                : CamMachinesDialog::tr("Empty: the diameter and the shape"));
  form->addRow(CamMachinesDialog::tr("Name"), name);
  auto* shape = new QComboBox;
  shape->setObjectName(QStringLiteral("camToolShape"));
  QStringList kinds = lathe ? latheKinds() : shapeKinds();
  if (lathe && mills) {
    kinds += shapeKinds();
  }
  for (const QString& kind : std::as_const(kinds)) {
    shape->addItem(shapeName(kind), kind);
  }
  const QJsonObject shapeJson = tool.value(QStringLiteral("shape")).toObject();
  const QString kind = text(shapeJson, "kind").isEmpty() ? kinds.first() : text(shapeJson, "kind");
  shape->setCurrentIndex(std::max(0, static_cast<int>(kinds.indexOf(kind))));
  form->addRow(lathe ? CamMachinesDialog::tr("Kind") : CamMachinesDialog::tr("Shape"), shape);
  // A live tool's holder on the turret (mitcad#2067).
  auto* mount = new QComboBox;
  mount->setObjectName(QStringLiteral("camToolMount"));
  mount->addItem(CamMachinesDialog::tr("Axial: cuts the face"), QStringLiteral("axial"));
  mount->addItem(CamMachinesDialog::tr("Radial: cuts the diameter"), QStringLiteral("radial"));
  // The tilting milling head (mitcad#2319).
  mount->addItem(CamMachinesDialog::tr("B head: tilts to any plane"), QStringLiteral("head"));
  mount->setCurrentIndex(std::max(0, mount->findData(text(tool, "mount"))));
  mount->setToolTip(CamMachinesDialog::tr("How the live tool's holder sits on the turret: along the spindle or across it"));
  form->addRow(CamMachinesDialog::tr("Live Holder"), mount);
  qInfo().noquote() << QStringLiteral("CAM tool dialog: %1 kinds: %2")
                           .arg(lathe ? QStringLiteral("lathe") : QStringLiteral("milling"),
                                kinds.join(QStringLiteral(", ")));
  const auto mm = [&tool](const char* key, const QString& fallback) {
    return tool.contains(QLatin1String(key)) ? plain(numberOf(tool, key)) + QStringLiteral(" mm") : fallback;
  };
  const auto shapeMm = [&shapeJson](const char* key, const QString& fallback) {
    return shapeJson.contains(QLatin1String(key)) ? plain(numberOf(shapeJson, key)) + QStringLiteral(" mm") : fallback;
  };
  // The lathe's insert: nose radius, width, thread angle, outside or in a
  // bore.
  QLineEdit* noseRadius = field(QStringLiteral("camToolNoseRadius"), shapeMm("nose_radius", QStringLiteral("0.4 mm")),
                                CamMachinesDialog::tr("The insert's corner radius; 0 for a sharp corner"));
  form->addRow(CamMachinesDialog::tr("Nose Radius"), noseRadius);
  QLineEdit* width = field(QStringLiteral("camToolWidth"), shapeMm("width", QStringLiteral("3 mm")),
                           CamMachinesDialog::tr("The insert's or the blade's width"));
  form->addRow(CamMachinesDialog::tr("Width"), width);
  QLineEdit* threadAngle = field(QStringLiteral("camToolThreadAngle"),
                                 plain(numberOf(shapeJson, "angle", 60.0)) + QStringLiteral(" deg"),
                                 CamMachinesDialog::tr("The thread's included angle: 60 degrees for metric threads"));
  form->addRow(CamMachinesDialog::tr("Thread Angle"), threadAngle);
  auto* inside = new QCheckBox(CamMachinesDialog::tr("Cuts in a bore"));
  inside->setObjectName(QStringLiteral("camToolInside"));
  inside->setChecked(shapeJson.value(QStringLiteral("inside")).toBool());
  inside->setToolTip(CamMachinesDialog::tr("An insert for the inside of a bore rather than the outside"));
  form->addRow(QString(), inside);
  auto* diameterLabel = new QLabel(CamMachinesDialog::tr("Diameter"));
  QLineEdit* diameter = field(QStringLiteral("camToolDiameter"),
                              mm("diameter", lathe ? QStringLiteral("20 mm") : QStringLiteral("6 mm")),
                              CamMachinesDialog::tr("The cutting diameter; a length with its unit"));
  form->addRow(diameterLabel, diameter);
  QLineEdit* corner = field(QStringLiteral("camToolCorner"),
                            shapeJson.contains(QStringLiteral("corner_radius"))
                                ? plain(numberOf(shapeJson, "corner_radius")) + QStringLiteral(" mm")
                                : QStringLiteral("1 mm"));
  form->addRow(CamMachinesDialog::tr("Corner Radius"), corner);
  QLineEdit* point = field(QStringLiteral("camToolPointAngle"),
                           plain(numberOf(shapeJson, "point_angle", 118.0)) + QStringLiteral(" deg"));
  form->addRow(CamMachinesDialog::tr("Point Angle"), point);
  QLineEdit* angle = field(QStringLiteral("camToolAngle"),
                           plain(numberOf(shapeJson, "angle", kind == QStringLiteral("engraver") ? 30.0 : 90.0)) +
                               QStringLiteral(" deg"),
                           CamMachinesDialog::tr("The included angle of the cutting end"));
  form->addRow(CamMachinesDialog::tr("Angle"), angle);
  QLineEdit* tip = field(QStringLiteral("camToolTip"), plain(numberOf(shapeJson, "tip_diameter")) + QStringLiteral(" mm"));
  form->addRow(CamMachinesDialog::tr("Tip Diameter"), tip);
  QLineEdit* shank = field(QStringLiteral("camToolShank"), mm("shank_diameter", QString()),
                           CamMachinesDialog::tr("Empty: the same as the cutting diameter"));
  form->addRow(CamMachinesDialog::tr("Shank Diameter"), shank);
  QLineEdit* flutes =
      field(QStringLiteral("camToolFluteLength"), mm("flute_length", QString()),
            lathe ? CamMachinesDialog::tr("The deepest cut in one pass; empty: 4 mm, grooving 5 mm, parting 25 mm")
                  : CamMachinesDialog::tr("The deepest cut in one pass; empty: three diameters"));
  form->addRow(lathe ? CamMachinesDialog::tr("Depth of Cut") : CamMachinesDialog::tr("Flute Length"), flutes);
  QLineEdit* length =
      field(QStringLiteral("camToolStickOut"), mm("length", QString()),
            lathe ? CamMachinesDialog::tr("How far the tool reaches out of the turret; empty: the depth of cut and 5 mm")
                  : CamMachinesDialog::tr("From the holder to the tip; empty: the flutes and 5 mm"));
  form->addRow(lathe ? CamMachinesDialog::tr("Overhang") : CamMachinesDialog::tr("Stick-Out"), length);
  auto* count = new QSpinBox;
  count->setObjectName(QStringLiteral("camToolFlutes"));
  count->setRange(1, 16);
  count->setValue(tool.value(QStringLiteral("flutes")).toInt(2));
  form->addRow(CamMachinesDialog::tr("Flutes"), count);
  QLineEdit* holder = field(QStringLiteral("camToolHolder"), text(tool, "holder"),
                            CamMachinesDialog::tr("The holder of the spindle the tool sits in, by its name"));
  form->addRow(CamMachinesDialog::tr("Holder"), holder);
  const QJsonObject cutting = tool.value(QStringLiteral("cutting")).toObject();
  QLineEdit* rpm = field(QStringLiteral("camToolRpm"),
                         cutting.isEmpty() ? QString() : plain(numberOf(cutting, "spindle_rpm")),
                         CamMachinesDialog::tr("Revolutions per minute; empty: no defaults for the operations"));
  form->addRow(CamMachinesDialog::tr("Spindle Speed (rpm)"), rpm);
  QLineEdit* feed = field(QStringLiteral("camToolFeed"),
                          cutting.isEmpty() ? QString() : plain(numberOf(cutting, "feed")) + QStringLiteral(" mm"),
                          CamMachinesDialog::tr("A length per minute: 1200 mm or 40 in"));
  form->addRow(CamMachinesDialog::tr("Feed per Minute"), feed);
  QLineEdit* plunge = field(QStringLiteral("camToolPlunge"),
                            cutting.isEmpty() ? QString() : plain(numberOf(cutting, "plunge_feed")) + QStringLiteral(" mm"),
                            CamMachinesDialog::tr("A length per minute, for plunges and ramps"));
  form->addRow(CamMachinesDialog::tr("Plunge Feed per Minute"), plunge);
  auto* message = new QLabel;
  message->setObjectName(QStringLiteral("camToolMessage"));
  message->setWordWrap(true);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  auto* layout = new QVBoxLayout(&dialog);
  layout->addLayout(form);
  layout->addWidget(message);
  layout->addWidget(buttons);
  // A lathe's tools have no shank, flutes, spindle holder or speeds and
  // feeds per minute here (the turning operations set their own); a value
  // a tool already has is kept.
  // A live tool on a lathe (mitcad#2067) has a milling tool's shank,
  // flutes and speeds and feeds, and its holder on the turret.
  form->setRowVisible(holder, !lathe);
  const auto shapeChanged = [form, shape, corner, point, angle, tip, noseRadius, width, threadAngle, inside,
                             diameterLabel, diameter, lathe, mount, shank, count, rpm, feed, plunge] {
    const QString now = shape->currentData().toString();
    const bool milling = !lathe || shapeKinds().contains(now);
    for (QWidget* row : {static_cast<QWidget*>(shank), static_cast<QWidget*>(count), static_cast<QWidget*>(rpm),
                         static_cast<QWidget*>(feed), static_cast<QWidget*>(plunge)}) {
      form->setRowVisible(row, milling);
    }
    form->setRowVisible(mount, lathe && milling);
    form->setRowVisible(corner, milling && now == QStringLiteral("bull"));
    form->setRowVisible(point, milling && now == QStringLiteral("drill"));
    const bool angled = milling && (now == QStringLiteral("chamfer") || now == QStringLiteral("engraver"));
    form->setRowVisible(angle, angled);
    form->setRowVisible(tip, angled);
    const bool grooving = now == QStringLiteral("grooving");
    const bool threading = now == QStringLiteral("threading");
    const bool boring = now == QStringLiteral("boring");
    form->setRowVisible(noseRadius, lathe && (now == QStringLiteral("turning") || boring || grooving));
    form->setRowVisible(width, lathe && (grooving || now == QStringLiteral("parting")));
    form->setRowVisible(threadAngle, lathe && threading);
    form->setRowVisible(inside, lathe && (grooving || threading));
    if (milling) {
      diameterLabel->setText(CamMachinesDialog::tr("Diameter"));
      diameter->setToolTip(CamMachinesDialog::tr("The cutting diameter; a length with its unit"));
    } else if (boring) {
      diameterLabel->setText(CamMachinesDialog::tr("Bar Diameter"));
      diameter->setToolTip(CamMachinesDialog::tr("The bar's diameter: it must fit the stock's bore"));
    } else {
      diameterLabel->setText(CamMachinesDialog::tr("Holder Size"));
      diameter->setToolTip(CamMachinesDialog::tr("The holder's size, for the tool's default name"));
    }
  };
  QObject::connect(shape, &QComboBox::currentIndexChanged, &dialog, shapeChanged);
  shapeChanged();
  QJsonObject result;
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, [&] {
    if (taken.contains(numberBox->value())) {
      message->setText(CamMachinesDialog::tr("Another tool has the number %1.").arg(numberBox->value()));
      return;
    }
    const QString chosen = shape->currentData().toString();
    // A milling tool or a drill on a lathe: a live tool (mitcad#2067).
    const bool liveTool = lathe && shapeKinds().contains(chosen);
    if (diameter->text().trimmed().isEmpty()) {
      message->setText(!lathe || liveTool                  ? CamMachinesDialog::tr("Type the tool's diameter.")
                       : chosen == QStringLiteral("boring") ? CamMachinesDialog::tr("Type the bar's diameter.")
                                                            : CamMachinesDialog::tr("Type the holder's size."));
      return;
    }
    if (lathe && (chosen == QStringLiteral("grooving") || chosen == QStringLiteral("parting")) &&
        width->text().trimmed().isEmpty()) {
      message->setText(CamMachinesDialog::tr("Type the insert's width."));
      return;
    }
    const bool anyCutting = !rpm->text().trimmed().isEmpty() || !feed->text().trimmed().isEmpty() ||
                            !plunge->text().trimmed().isEmpty();
    const bool allCutting = !rpm->text().trimmed().isEmpty() && !feed->text().trimmed().isEmpty() &&
                            !plunge->text().trimmed().isEmpty();
    if (anyCutting && !allCutting) {
      message->setText(CamMachinesDialog::tr("Type the spindle speed, the feed and the plunge feed, or none of them."));
      return;
    }
    QJsonObject shapeOut{{QStringLiteral("kind"), chosen}};
    const auto typed = [&shapeOut](const char* key, const QLineEdit* edit) {
      if (!edit->text().trimmed().isEmpty()) {
        shapeOut.insert(QLatin1String(key), edit->text().trimmed());
      }
    };
    if (lathe && !liveTool) {
      if (chosen == QStringLiteral("turning") || chosen == QStringLiteral("boring")) {
        typed("nose_radius", noseRadius);
      } else if (chosen == QStringLiteral("grooving")) {
        typed("width", width);
        typed("nose_radius", noseRadius);
        shapeOut.insert(QStringLiteral("inside"), inside->isChecked());
      } else if (chosen == QStringLiteral("parting")) {
        typed("width", width);
      } else if (chosen == QStringLiteral("threading")) {
        typed("angle", threadAngle);
        shapeOut.insert(QStringLiteral("inside"), inside->isChecked());
      }
    } else if (chosen == QStringLiteral("bull")) {
      shapeOut.insert(QStringLiteral("corner_radius"), corner->text().trimmed());
    } else if (chosen == QStringLiteral("drill")) {
      shapeOut.insert(QStringLiteral("point_angle"), point->text().trimmed());
    } else if (chosen == QStringLiteral("chamfer") || chosen == QStringLiteral("engraver")) {
      shapeOut.insert(QStringLiteral("angle"), angle->text().trimmed());
      shapeOut.insert(QStringLiteral("tip_diameter"), tip->text().trimmed());
    }
    result = QJsonObject{{QStringLiteral("number"), numberBox->value()},
                         {QStringLiteral("shape"), shapeOut},
                         {QStringLiteral("diameter"), diameter->text().trimmed()},
                         {QStringLiteral("flutes"), count->value()}};
    if (!name->text().trimmed().isEmpty()) {
      result.insert(QStringLiteral("name"), name->text().trimmed());
    }
    for (const auto& [key, edit] : {std::pair<const char*, QLineEdit*>{"shank_diameter", shank},
                                    std::pair<const char*, QLineEdit*>{"flute_length", flutes},
                                    std::pair<const char*, QLineEdit*>{"length", length}}) {
      if (!edit->text().trimmed().isEmpty()) {
        result.insert(QLatin1String(key), edit->text().trimmed());
      }
    }
    if (liveTool) {
      result.insert(QStringLiteral("mount"), mount->currentData().toString());
    }
    if (!holder->text().trimmed().isEmpty()) {
      result.insert(QStringLiteral("holder"), holder->text().trimmed());
    }
    if (allCutting) {
      bool ok = false;
      const double speed = rpm->text().trimmed().toDouble(&ok);
      result.insert(QStringLiteral("cutting"),
                    QJsonObject{{QStringLiteral("spindle_rpm"), ok ? QJsonValue(speed) : QJsonValue(rpm->text().trimmed())},
                                {QStringLiteral("feed"), feed->text().trimmed()},
                                {QStringLiteral("plunge_feed"), plunge->text().trimmed()}});
    }
    dialog.accept();
  });
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    return std::nullopt;
  }
  return result;
}

} // namespace

CamMachinesDialog::CamMachinesDialog(Host host, QWidget* parent) : QDialog(parent), m_host(std::move(host)) {
  setObjectName(QStringLiteral("camMachinesDialog"));
  setWindowTitle(tr("Machines and Tools"));
  auto* layout = new QHBoxLayout(this);

  auto* left = new QVBoxLayout;
  left->addWidget(new QLabel(tr("Machines of this design")));
  m_list = new QListWidget;
  m_list->setObjectName(QStringLiteral("camMachineList"));
  left->addWidget(m_list, 1);
  const auto button = [this](const char* objectName, const QString& label, const QString& tip, auto slot) {
    auto* b = new QPushButton(label);
    b->setObjectName(QString::fromLatin1(objectName));
    b->setToolTip(tip);
    b->setAutoDefault(false);
    connect(b, &QPushButton::clicked, this, slot);
    return b;
  };
  left->addWidget(button("camNewMachine", tr("New Machine…"), tr("Adds a machine to the design"),
                         [this] { newMachine(); }));
  m_editMachine = button("camEditMachine", tr("Edit Machine…"), tr("Changes machine settings and keep-out zones"),
                         [this] { editMachine(); });
  left->addWidget(m_editMachine);
  m_deleteMachine = button("camDeleteMachine", tr("Delete Machine"), tr("Takes the machine out of the design"),
                           [this] { deleteMachine(); });
  left->addWidget(m_deleteMachine);
  left->addWidget(button("camImportMachine", tr("Import from Library…"),
                         tr("Copies a machine with its tools and zones from your or the project's library"),
                         [this] { importMachine(); }));
  m_saveMachine = new QPushButton(tr("Save to Library"));
  m_saveMachine->setObjectName(QStringLiteral("camSaveMachine"));
  m_saveMachine->setToolTip(tr("Writes the machine with its tools and zones to your or the project's library"));
  m_saveMachine->setAutoDefault(false);
  auto* saveMenu = new QMenu(m_saveMachine);
  connect(saveMenu->addAction(tr("My library")), &QAction::triggered, this, [this] { saveMachine(QStringLiteral("user")); });
  QAction* project = saveMenu->addAction(tr("This project"));
  project->setEnabled(!m_host.projectFolder.isEmpty());
  connect(project, &QAction::triggered, this, [this] { saveMachine(QStringLiteral("project")); });
  m_saveMachine->setMenu(saveMenu);
  left->addWidget(m_saveMachine);
  layout->addLayout(left, 1);

  auto* right = new QVBoxLayout;
  m_summary = new QLabel;
  m_summary->setObjectName(QStringLiteral("camMachineSummary"));
  m_summary->setWordWrap(true);
  right->addWidget(m_summary);
  m_tools = new QTreeWidget;
  m_tools->setObjectName(QStringLiteral("camToolTable"));
  m_tools->setRootIsDecorated(false);
  // A table of tools: rows without children.
  panel::setCompactIndentation(m_tools, panel::Indent::Flat);
  m_tools->setHeaderLabels({tr("Number"), tr("Name"), tr("Shape"), tr("Diameter"), tr("Shank"), tr("Flute Length"),
                            tr("Stick-Out"), tr("Holder"), tr("Speeds and Feeds")});
  m_tools->header()->setStretchLastSection(true);
  right->addWidget(m_tools, 1);
  auto* toolButtons = new QHBoxLayout;
  m_addTool = button("camAddTool", tr("Add Tool…"), tr("Adds a tool to the machine's library"), [this] { addTool(); });
  m_editTool = button("camEditTool", tr("Edit Tool…"), tr("Changes the selected tool"), [this] { editTool(); });
  m_deleteTool = button("camDeleteTool", tr("Delete Tool"), tr("Takes the selected tool out of the library"),
                        [this] { deleteTool(); });
  toolButtons->addWidget(m_addTool);
  toolButtons->addWidget(m_editTool);
  toolButtons->addWidget(m_deleteTool);
  toolButtons->addStretch(1);
  auto* close = new QDialogButtonBox(QDialogButtonBox::Close);
  connect(close, &QDialogButtonBox::rejected, this, &QDialog::reject);
  toolButtons->addWidget(close);
  right->addLayout(toolButtons);
  layout->addLayout(right, 3);

  connect(m_list, &QListWidget::currentRowChanged, this, [this] { showMachine(); });
  connect(m_tools, &QTreeWidget::itemDoubleClicked, this, [this] { editTool(); });
  connect(m_tools, &QTreeWidget::currentItemChanged, this, [this] {
    m_editTool->setEnabled(currentTool() > 0);
    m_deleteTool->setEnabled(currentTool() > 0);
  });
  resize(900, 460);
  refresh();
}

void CamMachinesDialog::refresh() {
  const QString selected = currentMachine();
  try {
    m_machines = m_host.query({{QStringLiteral("query"), QStringLiteral("cam_machines")}})
                     .toObject()
                     .value(QStringLiteral("machines"))
                     .toArray();
    m_dialects = m_host.query({{QStringLiteral("query"), QStringLiteral("cam_dialects")}})
                     .toObject()
                     .value(QStringLiteral("dialects"))
                     .toArray();
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("CAM machines not read: %1").arg(QString::fromUtf8(e.what()));
    m_machines = QJsonArray();
  }
  {
    const QSignalBlocker blocker(m_list);
    m_list->clear();
    for (const QJsonValue& value : std::as_const(m_machines)) {
      const QJsonObject machine = value.toObject();
      const QJsonObject body = machine.value(QStringLiteral("machine")).toObject();
      auto* item = new QListWidgetItem(QStringLiteral("%1 (%2)").arg(text(body, "name"), text(body, "controller")), m_list);
      item->setData(Qt::UserRole, text(machine, "id"));
    }
  }
  QStringList logged;
  for (const QJsonValue& value : std::as_const(m_machines)) {
    const QJsonObject machine = value.toObject();
    logged << QStringLiteral("%1 %2 tool(s)")
                  .arg(text(machine, "id"))
                  .arg(machine.value(QStringLiteral("tools")).toObject().value(QStringLiteral("tools")).toArray().size());
  }
  const QString line = QStringLiteral("CAM machines dialog: %1 machine(s)%2")
                           .arg(m_machines.size())
                           .arg(logged.isEmpty() ? QString() : QStringLiteral(": ") + logged.join(QStringLiteral(", ")));
  if (line != m_logged) {
    m_logged = line;
    qInfo().noquote() << line;
  }
  selectMachine(selected);
  if (m_list->currentRow() < 0 && m_list->count() > 0) {
    m_list->setCurrentRow(0);
  }
  showMachine();
}

void CamMachinesDialog::selectMachine(const QString& id) {
  for (int row = 0; row < m_list->count(); ++row) {
    if (m_list->item(row)->data(Qt::UserRole).toString() == id) {
      m_list->setCurrentRow(row);
      return;
    }
  }
}

QString CamMachinesDialog::currentMachine() const {
  const QListWidgetItem* item = m_list != nullptr ? m_list->currentItem() : nullptr;
  return item != nullptr ? item->data(Qt::UserRole).toString() : QString();
}

QJsonObject CamMachinesDialog::machineJson(const QString& id) const {
  for (const QJsonValue& value : m_machines) {
    if (text(value.toObject(), "id") == id) {
      return value.toObject();
    }
  }
  return {};
}

int CamMachinesDialog::currentTool() const {
  const QTreeWidgetItem* item = m_tools->currentItem();
  return item != nullptr ? item->data(0, Qt::UserRole).toInt() : 0;
}

void CamMachinesDialog::showMachine() {
  const QJsonObject machine = machineJson(currentMachine());
  const bool any = !machine.isEmpty();
  m_editMachine->setEnabled(any);
  m_deleteMachine->setEnabled(any);
  m_saveMachine->setEnabled(any);
  m_addTool->setEnabled(any);
  m_tools->clear();
  if (!any) {
    m_summary->setText(tr("No machine yet: New Machine… or Import from Library…"));
    m_editTool->setEnabled(false);
    m_deleteTool->setEnabled(false);
    return;
  }
  const QJsonObject body = machine.value(QStringLiteral("machine")).toObject();
  if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT")) {
    const QJsonDocument zones(body.value(QStringLiteral("zones")).toArray());
    qInfo().noquote() << QStringLiteral("CAM machine zones: %1: %2")
                             .arg(text(machine, "id"), QString::fromUtf8(zones.toJson(QJsonDocument::Compact)));
  }
  const QJsonObject travel = body.value(QStringLiteral("travel")).toObject();
  const QJsonArray min = travel.value(QStringLiteral("min")).toArray();
  const QJsonArray max = travel.value(QStringLiteral("max")).toArray();
  const QJsonObject spindle = body.value(QStringLiteral("spindle")).toObject();
  const int users = static_cast<int>(machine.value(QStringLiteral("used_by")).toArray().size());
  QString controller = text(body, "controller");
  for (const QJsonValue& dialect : std::as_const(m_dialects)) {
    if (text(dialect.toObject(), "id") == controller) {
      controller = text(dialect.toObject(), "label");
    }
  }
  const bool lathe = isLathe(body);
  if (lathe) {
    // A lathe's travel in its axes: X a radius from the spindle axis, Z
    // along it (mitcad#2230).
    const QJsonObject latheJson = body.value(QStringLiteral("lathe")).toObject();
    m_summary->setText(tr("%1, a lathe, controller %2. Travel X %3 (a radius), Z %4; spindle %5 to %6 rpm; turret "
                          "of %7 stations; chuck %8 across, %9 long. Used by %n setup(s).",
                          nullptr, users)
                           .arg(text(body, "name"), controller,
                                m_host.lengthText(max.at(0).toDouble() - min.at(0).toDouble()),
                                m_host.lengthText(max.at(2).toDouble() - min.at(2).toDouble()),
                                plain(numberOf(spindle, "min_rpm")), plain(numberOf(spindle, "max_rpm")),
                                QString::number(latheJson.value(QStringLiteral("turret_stations")).toInt(12)),
                                m_host.lengthText(numberOf(latheJson, "chuck_diameter")),
                                m_host.lengthText(numberOf(latheJson, "chuck_length"))));
    m_tools->setHeaderLabels({tr("Station"), tr("Name"), tr("Kind"), tr("Nose Radius"), tr("Width"),
                              tr("Thread Angle"), tr("Holder Size"), tr("Depth of Cut"), tr("Overhang")});
    qInfo().noquote() << QStringLiteral("CAM lathe shown: %1: %2, turret %3 stations, chuck %4 x %5 mm")
                             .arg(text(machine, "id"), text(body, "controller"))
                             .arg(latheJson.value(QStringLiteral("turret_stations")).toInt(12))
                             .arg(plain(numberOf(latheJson, "chuck_diameter")),
                                  plain(numberOf(latheJson, "chuck_length")));
  } else {
    m_summary->setText(tr("%1, controller %2. Travel %3 × %4 × %5; spindle %6 to %7 rpm. Used by %n setup(s).",
                          nullptr, users)
                           .arg(text(body, "name"), controller,
                                m_host.lengthText(max.at(0).toDouble() - min.at(0).toDouble()),
                                m_host.lengthText(max.at(1).toDouble() - min.at(1).toDouble()),
                                m_host.lengthText(max.at(2).toDouble() - min.at(2).toDouble()),
                                plain(numberOf(spindle, "min_rpm")), plain(numberOf(spindle, "max_rpm"))));
    m_tools->setHeaderLabels({tr("Number"), tr("Name"), tr("Shape"), tr("Diameter"), tr("Shank"), tr("Flute Length"),
                              tr("Stick-Out"), tr("Holder"), tr("Speeds and Feeds")});
  }
  QStringList logged;
  for (const QJsonValue& value : machine.value(QStringLiteral("tools")).toObject().value(QStringLiteral("tools")).toArray()) {
    const QJsonObject tool = value.toObject();
    const int number = tool.value(QStringLiteral("number")).toInt();
    if (lathe) {
      const QJsonObject shape = tool.value(QStringLiteral("shape")).toObject();
      const QString kind = text(shape, "kind");
      // A live tool (mitcad#2067) by its holder: the face or the diameter.
      const bool liveTool = shapeKinds().contains(kind);
      // In the B head (mitcad#2319).
      const QString mount = text(tool, "mount");
      const QString kindShown = liveTool ? (mount == QStringLiteral("radial")
                                                ? tr("%1, live on the diameter").arg(shapeName(kind))
                                            : mount == QStringLiteral("head")
                                                ? tr("%1, in the B head").arg(shapeName(kind))
                                                : tr("%1, live on the face").arg(shapeName(kind)))
                                : shape.value(QStringLiteral("inside")).toBool()
                                    ? tr("%1, in a bore").arg(shapeName(kind))
                                    : shapeName(kind);
      const auto shapeLength = [this, &shape](const char* key) {
        return shape.contains(QLatin1String(key)) ? m_host.lengthText(numberOf(shape, key)) : QString();
      };
      const QString angle = kind == QStringLiteral("threading")
                                ? plain(numberOf(shape, "angle", 60.0)) + QStringLiteral("°")
                                : QString();
      auto* row = new QTreeWidgetItem(
          m_tools, {QString::number(number), text(tool, "name"), kindShown, shapeLength("nose_radius"),
                    shapeLength("width"), angle, m_host.lengthText(numberOf(tool, "diameter")),
                    m_host.lengthText(numberOf(tool, "flute_length")), m_host.lengthText(numberOf(tool, "length"))});
      row->setData(0, Qt::UserRole, number);
      logged << QStringLiteral("T%1 %2").arg(number).arg(text(tool, "name"));
      continue;
    }
    const QJsonObject cutting = tool.value(QStringLiteral("cutting")).toObject();
    const QString shank = tool.contains(QStringLiteral("shank_diameter"))
                              ? m_host.lengthText(tool.value(QStringLiteral("shank_diameter")).toDouble())
                              : QString();
    const QString feeds = cutting.isEmpty()
                              ? QString()
                              : tr("%1 rpm, %2/min, plunge %3/min")
                                    .arg(plain(numberOf(cutting, "spindle_rpm")),
                                         m_host.lengthText(numberOf(cutting, "feed")),
                                         m_host.lengthText(numberOf(cutting, "plunge_feed")));
    auto* row = new QTreeWidgetItem(
        m_tools, {QString::number(number), text(tool, "name"),
                  shapeName(text(tool.value(QStringLiteral("shape")).toObject(), "kind")),
                  m_host.lengthText(numberOf(tool, "diameter")), shank, m_host.lengthText(numberOf(tool, "flute_length")),
                  m_host.lengthText(numberOf(tool, "length")), text(tool, "holder"), feeds});
    row->setData(0, Qt::UserRole, number);
    logged << QStringLiteral("T%1 %2").arg(number).arg(text(tool, "name"));
  }
  for (int column = 0; column < m_tools->columnCount() - 1; ++column) {
    m_tools->resizeColumnToContents(column);
  }
  m_editTool->setEnabled(false);
  m_deleteTool->setEnabled(false);
  qInfo().noquote() << QStringLiteral("CAM machine shown: %1: %2")
                           .arg(text(machine, "id"), logged.isEmpty() ? QStringLiteral("no tools")
                                                                      : logged.join(QStringLiteral(" | ")));
}

bool CamMachinesDialog::run(const QJsonObject& command, const QString& what, const QString& action, QJsonObject* result) {
  if (m_host.run(command, result)) {
    refresh();
    return true;
  }
  const QString reason = modelMessage(userText(m_host.lastError()));
  qInfo().noquote() << QStringLiteral("CAM machines: %1 refused: %2").arg(action, m_host.lastError());
  sheetWarning(this, windowTitle(), QStringLiteral("%1:\n%2").arg(what, reason));
  refresh();
  return false;
}

bool CamMachinesDialog::evaluate(const QString& typed, const QString& kind, double* value) const {
  try {
    const QJsonObject answer = m_host
                                   .query({{QStringLiteral("query"), QStringLiteral("evaluate")},
                                           {QStringLiteral("expression"), typed.trimmed()},
                                           {QStringLiteral("kind"), kind}})
                                   .toObject();
    if (!answer.value(QStringLiteral("value")).isDouble()) {
      return false;
    }
    *value = answer.value(QStringLiteral("value")).toDouble();
    return true;
  } catch (const std::exception&) {
    return false;
  }
}

std::optional<QJsonObject> CamMachinesDialog::askZone(QDialog* parent, QJsonObject zone, bool lathe) {
  const QJsonObject bounds = zone.value(QStringLiteral("bounds")).toObject();
  QJsonArray minimum = bounds.value(QStringLiteral("min")).toArray();
  QJsonArray maximum = bounds.value(QStringLiteral("max")).toArray();
  const bool adding = zone.isEmpty();
  if (adding) {
    minimum = QJsonArray{0, 0, 0};
    maximum = QJsonArray{0, 0, 0};
  }

  QDialog dialog(parent);
  dialog.setObjectName(QStringLiteral("camMachineZoneDialog"));
  dialog.setWindowTitle(adding ? tr("Add Zone") : tr("Edit Zone"));
  auto* form = new QFormLayout;
  QLineEdit* name = field(QStringLiteral("camMachineZoneName"), text(zone, "name"));
  form->addRow(tr("Name"), name);
  struct AxisFields {
    QLineEdit* minimum = nullptr;
    QLineEdit* maximum = nullptr;
  };
  std::array<AxisFields, 3> fields{};
  const auto shown = [adding](double value) {
    return adding ? QString() : QString::number(value, 'g', QLocale::FloatingPointShortest) + QStringLiteral(" mm");
  };
  for (int axis = 0; axis < 3; ++axis) {
    if (lathe && axis == 1) {
      continue;
    }
    const QString label = zoneAxis(axis, lathe);
    const QLatin1Char key("XYZ"[axis]);
    const QString tip = !lathe ? QString()
                              : axis == 0 ? tr("Measure X as a radius from the spindle axis.")
                                          : tr("Measure Z along the spindle axis.");
    fields[axis].minimum = field(QStringLiteral("camMachineZoneMin") + key, shown(minimum.at(axis).toDouble()), tip);
    fields[axis].maximum = field(QStringLiteral("camMachineZoneMax") + key, shown(maximum.at(axis).toDouble()), tip);
    form->addRow(tr("Minimum %1").arg(label), fields[axis].minimum);
    form->addRow(tr("Maximum %1").arg(label), fields[axis].maximum);
  }
  auto* message = new QLabel;
  message->setObjectName(QStringLiteral("camMachineZoneMessage"));
  message->setWordWrap(true);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  auto* layout = new QVBoxLayout(&dialog);
  layout->addLayout(form);
  layout->addWidget(message);
  layout->addWidget(buttons);
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  connect(buttons, &QDialogButtonBox::accepted, &dialog, [&] {
    const auto refuse = [message](QLineEdit* edit, const QString& reason) {
      message->setText(reason);
      edit->setFocus();
      edit->selectAll();
    };
    const QString title = name->text().trimmed();
    if (title.isEmpty()) {
      refuse(name, tr("Enter a name for the zone."));
      return;
    }
    // A lathe's checker uses X and Z. Keep an existing zone's Y bounds
    // when editing a machine that came from a file or a library.
    QJsonArray lo = minimum;
    QJsonArray hi = maximum;
    for (int axis = 0; axis < 3; ++axis) {
      const AxisFields& range = fields[axis];
      if (range.minimum == nullptr) {
        continue;
      }
      const QString label = zoneAxis(axis, lathe);
      double lower = 0.0;
      double upper = 0.0;
      if (!evaluate(range.minimum->text(), QStringLiteral("length"), &lower) || !std::isfinite(lower)) {
        refuse(range.minimum, tr("Enter a length for Minimum %1.").arg(label));
        return;
      }
      if (!evaluate(range.maximum->text(), QStringLiteral("length"), &upper) || !std::isfinite(upper)) {
        refuse(range.maximum, tr("Enter a length for Maximum %1.").arg(label));
        return;
      }
      if (lower > upper) {
        refuse(range.maximum, tr("Minimum %1 must be at or below Maximum %1.").arg(label));
        return;
      }
      lo.replace(axis, lower);
      hi.replace(axis, upper);
    }
    zone.insert(QStringLiteral("name"), title);
    zone.insert(QStringLiteral("bounds"), QJsonObject{{QStringLiteral("min"), lo}, {QStringLiteral("max"), hi}});
    dialog.accept();
  });
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    return std::nullopt;
  }
  return zone;
}

QWidget* CamMachinesDialog::machineZonesPage(QDialog* dialog, QJsonArray& zones, bool lathe) {
  auto* page = new QWidget;
  auto* layout = new QVBoxLayout(page);
  auto* guidance = new QLabel(tr("Enter the bounds in machine coordinates."));
  guidance->setWordWrap(true);
  layout->addWidget(guidance);
  auto* list = new QTreeWidget;
  list->setObjectName(QStringLiteral("camMachineZoneList"));
  list->setRootIsDecorated(false);
  panel::setCompactIndentation(list, panel::Indent::Flat);
  QStringList headers{tr("Name"), zoneAxis(0, lathe)};
  if (!lathe) {
    headers << zoneAxis(1, lathe);
  }
  headers << zoneAxis(2, lathe);
  list->setHeaderLabels(headers);
  list->header()->setSectionResizeMode(QHeaderView::ResizeToContents);
  layout->addWidget(list, 1);

  auto* actions = new QHBoxLayout;
  auto* add = new QPushButton(tr("Add Zone…"));
  auto* edit = new QPushButton(tr("Edit Zone…"));
  auto* remove = new QPushButton(tr("Remove Zone"));
  add->setObjectName(QStringLiteral("camMachineZoneAdd"));
  edit->setObjectName(QStringLiteral("camMachineZoneEdit"));
  remove->setObjectName(QStringLiteral("camMachineZoneRemove"));
  for (QPushButton* button : {add, edit, remove}) {
    button->setAutoDefault(false);
    actions->addWidget(button);
  }
  actions->addStretch(1);
  layout->addLayout(actions);

  const auto showZones = [this, list, &zones, lathe](int selected) {
    list->clear();
    for (const QJsonValue& value : std::as_const(zones)) {
      const QJsonObject zone = value.toObject();
      const QJsonObject bounds = zone.value(QStringLiteral("bounds")).toObject();
      const QJsonArray lo = bounds.value(QStringLiteral("min")).toArray();
      const QJsonArray hi = bounds.value(QStringLiteral("max")).toArray();
      const QString name = text(zone, "name");
      QStringList cells{name.isEmpty() ? tr("Unnamed zone") : name};
      for (int axis = 0; axis < 3; ++axis) {
        if (!lathe || axis != 1) {
          cells << tr("%1 to %2").arg(m_host.lengthText(lo.at(axis).toDouble()),
                                     m_host.lengthText(hi.at(axis).toDouble()));
        }
      }
      new QTreeWidgetItem(list, cells);
    }
    if (selected >= 0 && selected < list->topLevelItemCount()) {
      list->setCurrentItem(list->topLevelItem(selected));
    }
  };
  const auto enableActions = [list, edit, remove] {
    const bool selected = list->currentItem() != nullptr;
    edit->setEnabled(selected);
    remove->setEnabled(selected);
  };
  connect(list, &QTreeWidget::currentItemChanged, page, enableActions);
  connect(add, &QPushButton::clicked, page, [this, dialog, &zones, lathe, showZones] {
    if (const auto zone = askZone(dialog, {}, lathe)) {
      zones.append(*zone);
      showZones(static_cast<int>(zones.size()) - 1);
    }
  });
  const auto editZone = [this, dialog, list, &zones, lathe, showZones] {
    const int row = list->indexOfTopLevelItem(list->currentItem());
    if (row >= 0) {
      if (const auto zone = askZone(dialog, zones.at(row).toObject(), lathe)) {
        zones.replace(row, *zone);
        showZones(row);
      }
    }
  };
  connect(edit, &QPushButton::clicked, page, editZone);
  connect(list, &QTreeWidget::itemDoubleClicked, page, editZone);
  connect(remove, &QPushButton::clicked, page, [list, &zones, showZones] {
    const int row = list->indexOfTopLevelItem(list->currentItem());
    if (row >= 0) {
      zones.removeAt(row);
      showZones(std::min(row, static_cast<int>(zones.size()) - 1));
    }
  });
  showZones(0);
  enableActions();
  return page;
}

void CamMachinesDialog::newMachine() {
  // The kind first (mitcad#2230): a milling machine, as before, or a lathe.
  QDialog ask(this);
  ask.setObjectName(QStringLiteral("camNewMachineDialog"));
  ask.setWindowTitle(tr("New Machine"));
  auto* askForm = new QFormLayout;
  auto* kind = new QComboBox;
  kind->setObjectName(QStringLiteral("camNewMachineKind"));
  kind->addItem(tr("Milling machine"), QStringLiteral("mill"));
  kind->addItem(tr("Lathe"), QStringLiteral("lathe"));
  kind->setToolTip(tr("A router or mill for the milling operations, or a lathe for turning"));
  askForm->addRow(tr("Kind"), kind);
  auto* askButtons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  auto* askLayout = new QVBoxLayout(&ask);
  askLayout->addLayout(askForm);
  askLayout->addWidget(askButtons);
  connect(askButtons, &QDialogButtonBox::accepted, &ask, &QDialog::accept);
  connect(askButtons, &QDialogButtonBox::rejected, &ask, &QDialog::reject);
  prepareModal(&ask);
  if (execDialog(&ask) != QDialog::Accepted) {
    return;
  }
  const bool lathe = kind->currentData().toString() == QStringLiteral("lathe");
  qInfo().noquote() << QStringLiteral("CAM new machine: %1").arg(lathe ? QStringLiteral("lathe") : QStringLiteral("mill"));
  // The first controller of that kind (`cam_dialects`' `lathe`).
  QString controller = lathe ? QStringLiteral("fanuc_turning") : QStringLiteral("grbl");
  for (const QJsonValue& value : std::as_const(m_dialects)) {
    if (value.toObject().value(QStringLiteral("lathe")).toBool() == lathe) {
      controller = text(value.toObject(), "id");
      break;
    }
  }
  // A router-sized machine, or a two-axis lathe with a 12-station turret
  // and a 160 mm chuck (its travel in the lathe's axes: X a radius from
  // the spindle axis, Z along it); the form below changes it.
  const QJsonObject machine =
      lathe ? QJsonObject{{QStringLiteral("name"), tr("Lathe")},
                          {QStringLiteral("controller"), controller},
                          {QStringLiteral("travel"), QJsonObject{{QStringLiteral("min"), QJsonArray{-5, 0, -300}},
                                                                 {QStringLiteral("max"), QJsonArray{150, 0, 200}}}},
                          {QStringLiteral("spindle"), QJsonObject{{QStringLiteral("min_rpm"), 50},
                                                                  {QStringLiteral("max_rpm"), 3000},
                                                                  {QStringLiteral("spin_up_seconds"), 2},
                                                                  {QStringLiteral("nose_diameter"), 0},
                                                                  {QStringLiteral("nose_length"), 0}}},
                          {QStringLiteral("max_feed"), 5000},
                          {QStringLiteral("rapid_feed"), 20000},
                          {QStringLiteral("lathe"), QJsonObject{{QStringLiteral("turret_stations"), 12},
                                                                {QStringLiteral("chuck_diameter"), 160},
                                                                {QStringLiteral("chuck_length"), 50}}}}
            : QJsonObject{
                  {QStringLiteral("name"), tr("Router")},
                  {QStringLiteral("controller"), controller},
                  {QStringLiteral("travel"), QJsonObject{{QStringLiteral("min"), QJsonArray{0, 0, -100}},
                                                         {QStringLiteral("max"), QJsonArray{600, 400, 0}}}},
                  {QStringLiteral("spindle"), QJsonObject{{QStringLiteral("min_rpm"), 8000},
                                                          {QStringLiteral("max_rpm"), 24000},
                                                          {QStringLiteral("spin_up_seconds"), 2},
                                                          {QStringLiteral("nose_diameter"), 50},
                                                          {QStringLiteral("nose_length"), 80}}},
                  {QStringLiteral("max_feed"), 5000},
                  {QStringLiteral("rapid_feed"), 5000},
                  {QStringLiteral("tool_changer"), QJsonObject{{QStringLiteral("kind"), QStringLiteral("manual")}}}};
  // A free id: router, router-2, … (lathe, lathe-2, …)
  const QString base = lathe ? QStringLiteral("lathe") : QStringLiteral("router");
  QString id = base;
  for (int n = 2; !machineJson(id).isEmpty(); ++n) {
    id = QStringLiteral("%1-%2").arg(base).arg(n);
  }
  const QJsonObject start{{QStringLiteral("id"), id}, {QStringLiteral("machine"), machine}};
  // Added with these defaults, then Edit Machine…'s form for its own
  // values (Cancel there keeps the defaults; Delete Machine takes it out).
  QJsonObject result;
  const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("cam_machine_put")}, {QStringLiteral("machine"), start}};
  if (run(command, tr("Could not add the machine"), QStringLiteral("new machine"), &result)) {
    qInfo().noquote() << QStringLiteral("CAM machine set: %1 (new)").arg(id);
    selectMachine(id);
    editMachine();
  }
}

void CamMachinesDialog::editMachine() {
  const QJsonObject machine = machineJson(currentMachine());
  if (machine.isEmpty()) {
    return;
  }
  QJsonObject body = machine.value(QStringLiteral("machine")).toObject();
  const QJsonObject travel = body.value(QStringLiteral("travel")).toObject();
  const QJsonArray min = travel.value(QStringLiteral("min")).toArray();
  const QJsonArray max = travel.value(QStringLiteral("max")).toArray();
  QJsonObject spindle = body.value(QStringLiteral("spindle")).toObject();
  QJsonArray zones = body.value(QStringLiteral("zones")).toArray();

  QDialog dialog(this);
  dialog.setObjectName(QStringLiteral("camMachineDialog"));
  dialog.setWindowTitle(tr("Edit Machine"));
  auto* machinePage = new QWidget;
  auto* form = new QFormLayout(machinePage);
  QLineEdit* name = field(QStringLiteral("camMachineName"), text(body, "name"));
  form->addRow(tr("Name"), name);
  auto* controller = new QComboBox;
  controller->setObjectName(QStringLiteral("camMachineController"));
  // Only the dialects of the machine's kind (mitcad#2119): a lathe's
  // controller writes turning, a mill's milling; the model refuses the
  // other kind.
  const bool lathe = body.value(QStringLiteral("lathe")).isObject();
  for (const QJsonValue& value : std::as_const(m_dialects)) {
    const QJsonObject dialect = value.toObject();
    if (dialect.value(QStringLiteral("lathe")).toBool() != lathe) {
      continue;
    }
    controller->addItem(text(dialect, "label"), text(dialect, "id"));
  }
  QStringList controllers;
  for (int i = 0; i < controller->count(); ++i) {
    controllers << controller->itemData(i).toString();
  }
  qInfo().noquote() << QStringLiteral("CAM machine controllers: %1: %2")
                           .arg(text(machine, "id"), controllers.join(QStringLiteral(", ")));
  controller->setCurrentIndex(std::max(0, controller->findData(text(body, "controller"))));
  controller->setToolTip(tr("The controller the machine runs: the G-code it is written for"));
  form->addRow(tr("Controller"), controller);
  const auto mm = [](double value) { return plain(value) + QStringLiteral(" mm"); };
  // A lathe's travel is in its axes (mitcad#2230): X the tool's distance
  // from the spindle axis, Z along the spindle; it has no Y.
  QLineEdit* travelX =
      field(QStringLiteral("camMachineTravelX"), mm(max.at(0).toDouble() - min.at(0).toDouble()),
            lathe ? tr("How far the tool moves away from the spindle axis: a radius") : QString());
  QLineEdit* travelY = field(QStringLiteral("camMachineTravelY"), mm(max.at(1).toDouble() - min.at(1).toDouble()));
  QLineEdit* travelZ = field(QStringLiteral("camMachineTravelZ"), mm(max.at(2).toDouble() - min.at(2).toDouble()),
                             lathe ? tr("How far the tool moves along the spindle axis") : QString());
  form->addRow(tr("Travel X"), travelX);
  form->addRow(tr("Travel Y"), travelY);
  form->addRow(tr("Travel Z"), travelZ);
  form->setRowVisible(travelY, !lathe);
  QLineEdit* minRpm = field(QStringLiteral("camMachineMinRpm"), plain(numberOf(spindle, "min_rpm")));
  QLineEdit* maxRpm = field(QStringLiteral("camMachineMaxRpm"), plain(numberOf(spindle, "max_rpm")));
  QLineEdit* spinUp = field(QStringLiteral("camMachineSpinUp"), plain(numberOf(spindle, "spin_up_seconds")),
                            tr("Seconds the spindle needs to reach its speed"));
  form->addRow(tr("Lowest Spindle Speed (rpm)"), minRpm);
  form->addRow(tr("Highest Spindle Speed (rpm)"), maxRpm);
  form->addRow(tr("Spin-Up Time (s)"), spinUp);
  QLineEdit* noseDiameter = field(QStringLiteral("camMachineNoseDiameter"), mm(numberOf(spindle, "nose_diameter")),
                                  tr("The collet nut, holder and spindle body above the tool, as one cylinder"));
  QLineEdit* noseLength = field(QStringLiteral("camMachineNoseLength"), mm(numberOf(spindle, "nose_length")));
  form->addRow(tr("Spindle Nose Diameter"), noseDiameter);
  form->addRow(tr("Spindle Nose Length"), noseLength);
  form->setRowVisible(noseDiameter, !lathe);
  form->setRowVisible(noseLength, !lathe);
  QLineEdit* maxFeed = field(QStringLiteral("camMachineMaxFeed"), mm(numberOf(body, "max_feed")),
                             tr("The fastest cutting feed, a length per minute"));
  QLineEdit* rapid = field(QStringLiteral("camMachineRapid"), mm(numberOf(body, "rapid_feed")),
                           tr("The rapid rate, a length per minute, for the time estimates"));
  form->addRow(tr("Fastest Feed per Minute"), maxFeed);
  form->addRow(tr("Rapid Rate per Minute"), rapid);
  auto* changer = new QComboBox;
  changer->setObjectName(QStringLiteral("camMachineChanger"));
  const QString changerKind = text(body.value(QStringLiteral("tool_changer")).toObject(), "kind");
  changer->addItem(tr("By hand"), QStringLiteral("manual"));
  changer->addItem(tr("The controller's"), QStringLiteral("controller"));
  if (changerKind == QStringLiteral("macro")) {
    changer->addItem(tr("Driven by the program (rack)"), QStringLiteral("macro"));
  }
  changer->setCurrentIndex(std::max(0, changer->findData(changerKind)));
  changer->setToolTip(tr("Who changes the tools: the operator, or the controller's changer"));
  form->addRow(tr("Tool Changer"), changer);
  // The tool changer of a lathe is its turret.
  form->setRowVisible(changer, !lathe);
  // A lathe's own: the turret and the chuck (mitcad#2230).
  QJsonObject latheJson = body.value(QStringLiteral("lathe")).toObject();
  auto* turret = new QSpinBox;
  turret->setObjectName(QStringLiteral("camMachineTurret"));
  turret->setRange(1, 99);
  turret->setValue(latheJson.value(QStringLiteral("turret_stations")).toInt(12));
  turret->setToolTip(tr("The turret's stations: a lathe tool's number is its station"));
  form->addRow(tr("Turret Stations"), turret);
  QLineEdit* chuckDiameter = field(QStringLiteral("camMachineChuckDiameter"), mm(numberOf(latheJson, "chuck_diameter")),
                                   tr("The chuck's diameter, checked as a zone; 0: not checked"));
  QLineEdit* chuckLength = field(QStringLiteral("camMachineChuckLength"), mm(numberOf(latheJson, "chuck_length")),
                                 tr("How far the jaws reach out along the spindle from the face they grip at"));
  form->addRow(tr("Chuck Diameter"), chuckDiameter);
  form->addRow(tr("Chuck Length"), chuckLength);
  form->setRowVisible(turret, lathe);
  form->setRowVisible(chuckDiameter, lathe);
  form->setRowVisible(chuckLength, lathe);
  // A lathe's C axis and live tools (mitcad#2067): the live spindle's speed
  // and the builder's codes, each one block (the C axis's on and off
  // blocks separated by ";"), {rpm} in the live spindle's.
  const QJsonObject millTurn = latheJson.value(QStringLiteral("mill_turn")).toObject();
  auto* liveTools = new QCheckBox(tr("C axis and live tools"));
  liveTools->setObjectName(QStringLiteral("camMachineLiveTools"));
  liveTools->setChecked(latheMills(body));
  liveTools->setToolTip(tr("The spindle holds the part as a C axis while driven tools on the turret mill and drill"));
  form->addRow(QString(), liveTools);
  const auto code = [&millTurn](const char* key, const QString& fallback) {
    const QJsonValue value = millTurn.value(QLatin1String(key));
    if (value.isArray()) {
      QStringList lines;
      for (const QJsonValue& line : value.toArray()) {
        lines << line.toString();
      }
      return lines.join(QStringLiteral("; "));
    }
    return value.isString() ? value.toString() : fallback;
  };
  QLineEdit* liveMaxRpm =
      field(QStringLiteral("camMachineLiveMaxRpm"),
            millTurn.contains(QStringLiteral("live_max_rpm")) ? plain(numberOf(millTurn, "live_max_rpm"))
                                                              : QStringLiteral("6000"),
            tr("The live tools' fastest speed"));
  form->addRow(tr("Highest Live Tool Speed (rpm)"), liveMaxRpm);
  QLineEdit* cOn = field(QStringLiteral("camMachineCAxisOn"), code("c_axis_on", QStringLiteral("M154; G28 H0.")),
                         tr("The blocks that switch the spindle to its C axis, separated by ;"));
  form->addRow(tr("C Axis On"), cOn);
  QLineEdit* cOff = field(QStringLiteral("camMachineCAxisOff"), code("c_axis_off", QStringLiteral("M155")),
                          tr("The blocks that switch it back to turning, separated by ;"));
  form->addRow(tr("C Axis Off"), cOff);
  QLineEdit* cClamp = field(QStringLiteral("camMachineCClamp"), code("c_clamp", QStringLiteral("M14")),
                            tr("Clamps the C axis where a hole is drilled"));
  form->addRow(tr("C Axis Clamp"), cClamp);
  QLineEdit* cUnclamp = field(QStringLiteral("camMachineCUnclamp"), code("c_unclamp", QStringLiteral("M15")));
  form->addRow(tr("C Axis Unclamp"), cUnclamp);
  QLineEdit* liveCw = field(QStringLiteral("camMachineLiveCw"), code("live_cw", QStringLiteral("M133 P{rpm}")),
                            tr("Starts the live tool forward; {rpm} is its speed"));
  form->addRow(tr("Live Tool Forward"), liveCw);
  QLineEdit* liveCcw = field(QStringLiteral("camMachineLiveCcw"), code("live_ccw", QStringLiteral("M134 P{rpm}")),
                             tr("Starts the live tool in reverse; {rpm} is its speed"));
  form->addRow(tr("Live Tool Reverse"), liveCcw);
  QLineEdit* liveStop = field(QStringLiteral("camMachineLiveStop"), code("live_stop", QStringLiteral("M135")));
  form->addRow(tr("Live Tool Stop"), liveStop);
  const QList<QWidget*> liveRows{liveMaxRpm, cOn, cOff, cClamp, cUnclamp, liveCw, liveCcw, liveStop};
  const auto showLive = [form, liveTools, liveRows, lathe] {
    for (QWidget* row : liveRows) {
      form->setRowVisible(row, lathe && liveTools->isChecked());
    }
  };
  form->setRowVisible(liveTools, lathe);
  // A Y axis and a B head for the live tools, a sub-spindle that takes
  // the part over (mitcad#2319); the travel's Y is the Y axis's.
  const QJsonObject latheAxes = latheJson.value(QStringLiteral("axes")).toObject();
  const auto axisBox = [&form, &latheAxes](const QString& objectName, const QString& label, const char* key,
                                            const QString& tip) {
    auto* box = new QCheckBox(label);
    box->setObjectName(objectName);
    box->setChecked(latheAxes.value(QLatin1String(key)).toBool());
    box->setToolTip(tip);
    form->addRow(QString(), box);
    return box;
  };
  QCheckBox* yAxis = axisBox(QStringLiteral("camMachineYAxis"), tr("Y axis"), "y",
                             tr("The turret moves in Y: live tools mill on planes off the spindle axis"));
  QCheckBox* bHead = axisBox(QStringLiteral("camMachineBHead"), tr("B head"), "b",
                             tr("A milling head that tilts about Y: live tools cut tilted planes"));
  QCheckBox* subSpindle = axisBox(QStringLiteral("camMachineSubSpindle"), tr("Sub-spindle"), "sub_spindle",
                                  tr("A second spindle that takes the part over and turns its back"));
  const auto showAxes = [form, liveTools, yAxis, bHead, subSpindle, travelY, lathe] {
    form->setRowVisible(yAxis, lathe && liveTools->isChecked());
    form->setRowVisible(bHead, lathe && liveTools->isChecked());
    form->setRowVisible(subSpindle, lathe);
    form->setRowVisible(travelY, !lathe || (liveTools->isChecked() && yAxis->isChecked()));
  };
  connect(liveTools, &QCheckBox::toggled, &dialog, showLive);
  connect(liveTools, &QCheckBox::toggled, &dialog, showAxes);
  connect(yAxis, &QCheckBox::toggled, &dialog, showAxes);
  showLive();
  showAxes();
  qInfo().noquote() << QStringLiteral("CAM machine form: %1: %2")
                           .arg(text(machine, "id"), lathe ? QStringLiteral("lathe") : QStringLiteral("mill"));
  auto* message = new QLabel;
  message->setObjectName(QStringLiteral("camMachineMessage"));
  message->setWordWrap(true);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  auto* tabs = new QTabWidget;
  tabs->tabBar()->setObjectName(QStringLiteral("camMachineTabs"));
  tabs->addTab(machinePage, tr("Machine"));
  tabs->addTab(machineZonesPage(&dialog, zones, lathe), tr("Keep-Out Zones"));
  auto* layout = new QVBoxLayout(&dialog);
  layout->addWidget(tabs);
  layout->addWidget(message);
  layout->addWidget(buttons);
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  connect(buttons, &QDialogButtonBox::accepted, &dialog, [&] {
    // Every length as the design evaluates it: "600 mm", "24 in"; the
    // rows a lathe hides keep their values.
    const auto lengthOf = [this, message, tabs](QLineEdit* edit, const QString& label, bool zero, double* value) {
      if (!evaluate(edit->text(), QStringLiteral("length"), value) || *value < 0.0 || (!zero && *value <= 0.0)) {
        tabs->setCurrentIndex(0);
        message->setText(zero ? tr("%1: a length of zero or more.").arg(label)
                              : tr("%1: a length above zero.").arg(label));
        return false;
      }
      return true;
    };
    // Travel X, Y, Z, the nose's diameter and length, the fastest feed,
    // the rapid rate.
    double values[7] = {0.0, max.at(1).toDouble() - min.at(1).toDouble(), 0.0, numberOf(spindle, "nose_diameter"),
                        numberOf(spindle, "nose_length"), 0.0, 0.0};
    // A lathe's Y axis travels as far each way from the spindle axis
    // (mitcad#2319).
    const bool latheY = lathe && liveTools->isChecked() && yAxis->isChecked();
    if (!lengthOf(travelX, tr("Travel X"), false, &values[0]) ||
        ((!lathe || latheY) && !lengthOf(travelY, tr("Travel Y"), false, &values[1])) ||
        !lengthOf(travelZ, tr("Travel Z"), false, &values[2]) ||
        (!lathe && !lengthOf(noseDiameter, tr("Spindle Nose Diameter"), false, &values[3])) ||
        (!lathe && !lengthOf(noseLength, tr("Spindle Nose Length"), false, &values[4])) ||
        !lengthOf(maxFeed, tr("Fastest Feed per Minute"), false, &values[5]) ||
        !lengthOf(rapid, tr("Rapid Rate per Minute"), false, &values[6])) {
      return;
    }
    double chuck[2] = {};
    if (lathe && (!lengthOf(chuckDiameter, tr("Chuck Diameter"), true, &chuck[0]) ||
                  !lengthOf(chuckLength, tr("Chuck Length"), true, &chuck[1]))) {
      return;
    }
    double rpm[3] = {};
    const std::pair<QLineEdit*, QString> numbers[] = {{minRpm, tr("Lowest Spindle Speed (rpm)")},
                                                      {maxRpm, tr("Highest Spindle Speed (rpm)")},
                                                      {spinUp, tr("Spin-Up Time (s)")}};
    for (int i = 0; i < 3; ++i) {
      if (!evaluate(numbers[i].first->text(), QStringLiteral("unitless"), &rpm[i]) || rpm[i] < 0.0) {
        tabs->setCurrentIndex(0);
        message->setText(tr("%1: a number of zero or more.").arg(numbers[i].second));
        return;
      }
    }
    // The live spindle's speed (mitcad#2067).
    const bool mills = lathe && liveTools->isChecked();
    double liveRpm = 0.0;
    if (mills && (!evaluate(liveMaxRpm->text(), QStringLiteral("unitless"), &liveRpm) || liveRpm <= 0.0)) {
      tabs->setCurrentIndex(0);
      message->setText(tr("%1: a number above zero.").arg(tr("Highest Live Tool Speed (rpm)")));
      return;
    }
    const double x0 = min.at(0).toDouble();
    const double y0 = latheY ? -0.5 * values[1] : min.at(1).toDouble();
    const double z1 = max.at(2).toDouble();
    body.insert(QStringLiteral("name"), name->text().trimmed());
    body.insert(QStringLiteral("controller"), controller->currentData().toString());
    // The travel keeps its corner: X and Y from their lowest, Z down from
    // its highest (the spindle's top; a lathe's end away from the chuck).
    // A lathe's Y is unused and stays as it is.
    body.insert(QStringLiteral("travel"),
                QJsonObject{{QStringLiteral("min"), QJsonArray{x0, y0, z1 - values[2]}},
                            {QStringLiteral("max"), QJsonArray{x0 + values[0], y0 + values[1], z1}}});
    spindle.insert(QStringLiteral("min_rpm"), rpm[0]);
    spindle.insert(QStringLiteral("max_rpm"), rpm[1]);
    spindle.insert(QStringLiteral("spin_up_seconds"), rpm[2]);
    spindle.insert(QStringLiteral("nose_diameter"), values[3]);
    spindle.insert(QStringLiteral("nose_length"), values[4]);
    body.insert(QStringLiteral("spindle"), spindle);
    body.insert(QStringLiteral("max_feed"), values[5]);
    body.insert(QStringLiteral("rapid_feed"), values[6]);
    body.insert(QStringLiteral("zones"), zones);
    if (!lathe && changer->currentData().toString() != changerKind) {
      body.insert(QStringLiteral("tool_changer"),
                  QJsonObject{{QStringLiteral("kind"), changer->currentData().toString()}});
    }
    if (lathe) {
      latheJson.insert(QStringLiteral("turret_stations"), turret->value());
      latheJson.insert(QStringLiteral("chuck_diameter"), chuck[0]);
      latheJson.insert(QStringLiteral("chuck_length"), chuck[1]);
      // The C axis and the live tools, with their codes (mitcad#2067).
      QJsonObject axes = latheJson.value(QStringLiteral("axes")).toObject();
      axes.insert(QStringLiteral("c"), mills);
      axes.insert(QStringLiteral("live_tools"), mills);
      // The Y axis and the B head mill with the live tools (mitcad#2319).
      axes.insert(QStringLiteral("y"), mills && yAxis->isChecked());
      axes.insert(QStringLiteral("b"), mills && bHead->isChecked());
      axes.insert(QStringLiteral("sub_spindle"), subSpindle->isChecked());
      latheJson.insert(QStringLiteral("axes"), axes);
      if (mills) {
        const auto blocks = [](const QLineEdit* edit) {
          QJsonArray out;
          for (const QString& line : edit->text().split(QLatin1Char(';'), Qt::SkipEmptyParts)) {
            if (!line.trimmed().isEmpty()) {
              out.append(line.trimmed());
            }
          }
          return out;
        };
        QJsonObject codes = millTurn;
        codes.insert(QStringLiteral("live_max_rpm"), liveRpm);
        codes.insert(QStringLiteral("c_axis_on"), blocks(cOn));
        codes.insert(QStringLiteral("c_axis_off"), blocks(cOff));
        codes.insert(QStringLiteral("c_clamp"), cClamp->text().trimmed());
        codes.insert(QStringLiteral("c_unclamp"), cUnclamp->text().trimmed());
        codes.insert(QStringLiteral("live_cw"), liveCw->text().trimmed());
        codes.insert(QStringLiteral("live_ccw"), liveCcw->text().trimmed());
        codes.insert(QStringLiteral("live_stop"), liveStop->text().trimmed());
        latheJson.insert(QStringLiteral("mill_turn"), codes);
      }
      body.insert(QStringLiteral("lathe"), latheJson);
    }
    qInfo().noquote() << QStringLiteral("CAM machine form accepted: %1: %2")
                             .arg(text(machine, "id"), mills ? QStringLiteral("live tools on a C axis")
                                                             : lathe ? QStringLiteral("lathe")
                                                                     : QStringLiteral("mill"));
    dialog.accept();
  });
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    return;
  }
  QJsonObject put = machine;
  put.remove(QStringLiteral("used_by"));
  put.insert(QStringLiteral("machine"), body);
  const QString id = text(machine, "id");
  if (run({{QStringLiteral("cmd"), QStringLiteral("cam_machine_put")}, {QStringLiteral("machine"), put}},
          tr("Could not change the machine"), QStringLiteral("edit machine"))) {
    qInfo().noquote() << QStringLiteral("CAM machine set: %1").arg(id);
    selectMachine(id);
  }
}

void CamMachinesDialog::deleteMachine() {
  const QString id = currentMachine();
  if (id.isEmpty()) {
    return;
  }
  if (run({{QStringLiteral("cmd"), QStringLiteral("cam_machine_delete")}, {QStringLiteral("machine"), id}},
          tr("Could not delete the machine"), QStringLiteral("delete machine"))) {
    qInfo().noquote() << QStringLiteral("CAM machine deleted: %1").arg(id);
  }
}

void CamMachinesDialog::importMachine() {
  QJsonObject library;
  try {
    QJsonObject asked{{QStringLiteral("query"), QStringLiteral("cam_library")}};
    if (!m_host.projectFolder.isEmpty()) {
      asked.insert(QStringLiteral("project"), m_host.projectFolder);
    }
    library = m_host.query(asked).toObject();
  } catch (const std::exception& e) {
    sheetWarning(this, windowTitle(), tr("Could not read the machine libraries:\n%1")
                                          .arg(modelMessage(userText(QString::fromUtf8(e.what())))));
    return;
  }
  QStringList labels;
  QList<QJsonObject> entries;
  for (const QJsonValue& value : library.value(QStringLiteral("machines")).toArray()) {
    const QJsonObject entry = value.toObject();
    const QString where = text(entry, "library") == QStringLiteral("project") ? tr("This project") : tr("My library");
    labels << tr("%1 (%2), %3, version %4")
                  .arg(text(entry, "name"), text(entry, "controller"), where)
                  .arg(entry.value(QStringLiteral("version")).toInt());
    entries << entry;
  }
  qInfo().noquote() << QStringLiteral("CAM machine library: %1").arg(labels.isEmpty() ? QStringLiteral("empty")
                                                                                       : labels.join(QStringLiteral(" | ")));
  if (entries.isEmpty()) {
    sheetInformation(this, windowTitle(), tr("Neither your library nor the project's has a machine yet."));
    return;
  }
  bool accepted = false;
  const QString chosen = dialogItem(this, tr("Import from Library"), tr("Machine:"), labels, 0, &accepted);
  const qsizetype index = labels.indexOf(chosen);
  if (!accepted || index < 0) {
    return;
  }
  const QJsonObject entry = entries.at(index);
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("cam_machine_from_library")},
                      {QStringLiteral("library"), text(entry, "library")},
                      {QStringLiteral("id"), text(entry, "id")}};
  if (!m_host.projectFolder.isEmpty()) {
    command.insert(QStringLiteral("project"), m_host.projectFolder);
  }
  QJsonObject result;
  if (run(command, tr("Could not import the machine"), QStringLiteral("import"), &result)) {
    const QString id = result.value(QStringLiteral("machine")).toString(text(entry, "id"));
    qInfo().noquote() << QStringLiteral("CAM machine imported: %1 from %2 library").arg(id, text(entry, "library"));
    selectMachine(id);
  }
}

void CamMachinesDialog::saveMachine(const QString& libraryName) {
  const QString id = currentMachine();
  if (id.isEmpty()) {
    return;
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("cam_machine_to_library")},
                      {QStringLiteral("machine"), id},
                      {QStringLiteral("library"), libraryName}};
  if (!m_host.projectFolder.isEmpty()) {
    command.insert(QStringLiteral("project"), m_host.projectFolder);
  }
  QJsonObject result;
  if (run(command, tr("Could not save the machine"), QStringLiteral("save"), &result)) {
    qInfo().noquote() << QStringLiteral("CAM machine saved: %1 to %2 library: %3, version %4")
                             .arg(id, libraryName, result.value(QStringLiteral("path")).toString())
                             .arg(result.value(QStringLiteral("version")).toInt());
    sheetInformation(this, windowTitle(),
                     tr("Saved %1 to %2 as version %3.")
                         .arg(text(machineJson(id).value(QStringLiteral("machine")).toObject(), "name"),
                              libraryName == QStringLiteral("project") ? tr("this project's library") : tr("your library"))
                         .arg(result.value(QStringLiteral("version")).toInt()));
  }
}

void CamMachinesDialog::addTool() {
  const QString id = currentMachine();
  const QJsonObject machine = machineJson(id);
  if (machine.isEmpty()) {
    return;
  }
  QList<int> taken;
  for (const QJsonValue& value : machine.value(QStringLiteral("tools")).toObject().value(QStringLiteral("tools")).toArray()) {
    taken << value.toObject().value(QStringLiteral("number")).toInt();
  }
  int next = 1;
  while (taken.contains(next)) {
    ++next;
  }
  const std::optional<QJsonObject> tool =
      askTool(this, QJsonObject(), next, taken, isLathe(machine.value(QStringLiteral("machine")).toObject()),
              latheMills(machine.value(QStringLiteral("machine")).toObject()));
  if (!tool) {
    return;
  }
  QJsonObject result;
  if (run({{QStringLiteral("cmd"), QStringLiteral("cam_tool_put")}, {QStringLiteral("machine"), id},
           {QStringLiteral("tool"), *tool}},
          tr("Could not add the tool"), QStringLiteral("add tool"), &result)) {
    const QJsonObject kept = result.value(QStringLiteral("tool")).toObject();
    qInfo().noquote() << QStringLiteral("CAM tool set: %1 T%2 %3")
                             .arg(id)
                             .arg(kept.value(QStringLiteral("number")).toInt())
                             .arg(text(kept, "name"));
  }
}

void CamMachinesDialog::editTool() {
  const QString id = currentMachine();
  const int number = currentTool();
  if (id.isEmpty() || number <= 0) {
    return;
  }
  QJsonObject tool;
  QList<int> taken;
  for (const QJsonValue& value :
       machineJson(id).value(QStringLiteral("tools")).toObject().value(QStringLiteral("tools")).toArray()) {
    const int other = value.toObject().value(QStringLiteral("number")).toInt();
    if (other == number) {
      tool = value.toObject();
    } else {
      taken << other;
    }
  }
  const std::optional<QJsonObject> changed =
      askTool(this, tool, number, taken, isLathe(machineJson(id).value(QStringLiteral("machine")).toObject()),
              latheMills(machineJson(id).value(QStringLiteral("machine")).toObject()));
  if (!changed) {
    return;
  }
  const int newNumber = changed->value(QStringLiteral("number")).toInt();
  QJsonObject result;
  if (!run({{QStringLiteral("cmd"), QStringLiteral("cam_tool_put")}, {QStringLiteral("machine"), id},
            {QStringLiteral("tool"), *changed}},
           tr("Could not change the tool"), QStringLiteral("edit tool"), &result)) {
    return;
  }
  // A tool given another number is that tool now: the old number goes.
  if (newNumber != number) {
    run({{QStringLiteral("cmd"), QStringLiteral("cam_tool_delete")}, {QStringLiteral("machine"), id},
         {QStringLiteral("number"), number}},
        tr("Could not change the tool's number"), QStringLiteral("renumber tool"));
  }
  qInfo().noquote() << QStringLiteral("CAM tool set: %1 T%2 %3")
                           .arg(id)
                           .arg(newNumber)
                           .arg(text(result.value(QStringLiteral("tool")).toObject(), "name"));
}

void CamMachinesDialog::deleteTool() {
  const QString id = currentMachine();
  const int number = currentTool();
  if (id.isEmpty() || number <= 0) {
    return;
  }
  if (run({{QStringLiteral("cmd"), QStringLiteral("cam_tool_delete")}, {QStringLiteral("machine"), id},
           {QStringLiteral("number"), number}},
          tr("Could not delete the tool"), QStringLiteral("delete tool"))) {
    qInfo().noquote() << QStringLiteral("CAM tool deleted: %1 T%2").arg(id).arg(number);
  }
}

} // namespace mitcad
