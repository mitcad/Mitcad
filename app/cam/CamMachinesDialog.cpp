// SPDX-License-Identifier: MIT
// Machines and Tools… (mitcad#1013; CamMachinesDialog.hpp).
#include "CamMachinesDialog.hpp"

#include <algorithm>
#include <exception>
#include <optional>
#include <utility>

#include <QAction>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonDocument>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QMenu>
#include <QPushButton>
#include <QSignalBlocker>
#include <QSpinBox>
#include <QStringList>
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

QString shapeName(const QString& kind) {
  if (kind == QStringLiteral("flat")) return CamMachinesDialog::tr("Flat end mill");
  if (kind == QStringLiteral("ball")) return CamMachinesDialog::tr("Ball end mill");
  if (kind == QStringLiteral("bull")) return CamMachinesDialog::tr("Bull nose end mill");
  if (kind == QStringLiteral("drill")) return CamMachinesDialog::tr("Drill");
  if (kind == QStringLiteral("chamfer")) return CamMachinesDialog::tr("Chamfer mill");
  if (kind == QStringLiteral("engraver")) return CamMachinesDialog::tr("Engraving cutter");
  return kind;
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
// cancelled. `taken`: the numbers of the machine's other tools.
std::optional<QJsonObject> askTool(QWidget* parent, const QJsonObject& tool, int toolNumber, const QList<int>& taken) {
  QDialog dialog(parent);
  dialog.setObjectName(QStringLiteral("camToolDialog"));
  dialog.setWindowTitle(tool.isEmpty() ? CamMachinesDialog::tr("Add Tool") : CamMachinesDialog::tr("Edit Tool"));
  auto* form = new QFormLayout;
  auto* numberBox = new QSpinBox;
  numberBox->setObjectName(QStringLiteral("camToolNumber"));
  numberBox->setRange(1, 999);
  numberBox->setValue(toolNumber);
  numberBox->setToolTip(CamMachinesDialog::tr("The tool number, also its place in a tool changer"));
  form->addRow(CamMachinesDialog::tr("Number"), numberBox);
  QLineEdit* name = field(QStringLiteral("camToolName"), text(tool, "name"),
                          CamMachinesDialog::tr("Empty: the diameter and the shape"));
  form->addRow(CamMachinesDialog::tr("Name"), name);
  auto* shape = new QComboBox;
  shape->setObjectName(QStringLiteral("camToolShape"));
  for (const QString& kind : shapeKinds()) {
    shape->addItem(shapeName(kind), kind);
  }
  const QJsonObject shapeJson = tool.value(QStringLiteral("shape")).toObject();
  const QString kind = text(shapeJson, "kind").isEmpty() ? QStringLiteral("flat") : text(shapeJson, "kind");
  shape->setCurrentIndex(std::max(0, static_cast<int>(shapeKinds().indexOf(kind))));
  form->addRow(CamMachinesDialog::tr("Shape"), shape);
  const auto mm = [&tool](const char* key, const QString& fallback) {
    return tool.contains(QLatin1String(key)) ? plain(numberOf(tool, key)) + QStringLiteral(" mm") : fallback;
  };
  QLineEdit* diameter = field(QStringLiteral("camToolDiameter"), mm("diameter", QStringLiteral("6 mm")),
                              CamMachinesDialog::tr("The cutting diameter; a length with its unit"));
  form->addRow(CamMachinesDialog::tr("Diameter"), diameter);
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
  QLineEdit* flutes = field(QStringLiteral("camToolFluteLength"), mm("flute_length", QString()),
                            CamMachinesDialog::tr("The deepest cut in one pass; empty: three diameters"));
  form->addRow(CamMachinesDialog::tr("Flute Length"), flutes);
  QLineEdit* length = field(QStringLiteral("camToolStickOut"), mm("length", QString()),
                            CamMachinesDialog::tr("From the holder to the tip; empty: the flutes and 5 mm"));
  form->addRow(CamMachinesDialog::tr("Stick-Out"), length);
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
  const auto shapeChanged = [form, shape, corner, point, angle, tip] {
    const QString now = shape->currentData().toString();
    form->setRowVisible(corner, now == QStringLiteral("bull"));
    form->setRowVisible(point, now == QStringLiteral("drill"));
    const bool angled = now == QStringLiteral("chamfer") || now == QStringLiteral("engraver");
    form->setRowVisible(angle, angled);
    form->setRowVisible(tip, angled);
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
    if (diameter->text().trimmed().isEmpty()) {
      message->setText(CamMachinesDialog::tr("Type the tool's diameter."));
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
    const QString chosen = shape->currentData().toString();
    QJsonObject shapeOut{{QStringLiteral("kind"), chosen}};
    if (chosen == QStringLiteral("bull")) {
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
  m_editMachine = button("camEditMachine", tr("Edit Machine…"), tr("Changes the machine's controller, travel and spindle"),
                         [this] { editMachine(); });
  left->addWidget(m_editMachine);
  m_deleteMachine = button("camDeleteMachine", tr("Delete Machine"), tr("Takes the machine out of the design"),
                           [this] { deleteMachine(); });
  left->addWidget(m_deleteMachine);
  left->addWidget(button("camImportMachine", tr("Import from Library…"),
                         tr("Copies a machine with its tools from your or the project's library"),
                         [this] { importMachine(); }));
  m_saveMachine = new QPushButton(tr("Save to Library"));
  m_saveMachine->setObjectName(QStringLiteral("camSaveMachine"));
  m_saveMachine->setToolTip(tr("Writes the machine with its tools to your or the project's library"));
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
  m_summary->setText(tr("%1, controller %2. Travel %3 × %4 × %5; spindle %6 to %7 rpm. Used by %n setup(s).", nullptr,
                        users)
                         .arg(text(body, "name"), controller,
                              m_host.lengthText(max.at(0).toDouble() - min.at(0).toDouble()),
                              m_host.lengthText(max.at(1).toDouble() - min.at(1).toDouble()),
                              m_host.lengthText(max.at(2).toDouble() - min.at(2).toDouble()),
                              plain(numberOf(spindle, "min_rpm")), plain(numberOf(spindle, "max_rpm"))));
  QStringList logged;
  for (const QJsonValue& value : machine.value(QStringLiteral("tools")).toObject().value(QStringLiteral("tools")).toArray()) {
    const QJsonObject tool = value.toObject();
    const int number = tool.value(QStringLiteral("number")).toInt();
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

void CamMachinesDialog::newMachine() {
  // A router-sized machine on the first controller; the form below changes it.
  QJsonObject machine{
      {QStringLiteral("name"), tr("Router")},
      {QStringLiteral("controller"),
       m_dialects.isEmpty() ? QStringLiteral("grbl") : text(m_dialects.first().toObject(), "id")},
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
  // A free id: router, router-2, …
  QString id = QStringLiteral("router");
  for (int n = 2; !machineJson(id).isEmpty(); ++n) {
    id = QStringLiteral("router-%1").arg(n);
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

  QDialog dialog(this);
  dialog.setObjectName(QStringLiteral("camMachineDialog"));
  dialog.setWindowTitle(tr("Edit Machine"));
  auto* form = new QFormLayout;
  QLineEdit* name = field(QStringLiteral("camMachineName"), text(body, "name"));
  form->addRow(tr("Name"), name);
  auto* controller = new QComboBox;
  controller->setObjectName(QStringLiteral("camMachineController"));
  for (const QJsonValue& value : std::as_const(m_dialects)) {
    controller->addItem(text(value.toObject(), "label"), text(value.toObject(), "id"));
  }
  controller->setCurrentIndex(std::max(0, controller->findData(text(body, "controller"))));
  controller->setToolTip(tr("The controller the machine runs: the G-code it is written for"));
  form->addRow(tr("Controller"), controller);
  const auto mm = [](double value) { return plain(value) + QStringLiteral(" mm"); };
  QLineEdit* travelX = field(QStringLiteral("camMachineTravelX"), mm(max.at(0).toDouble() - min.at(0).toDouble()));
  QLineEdit* travelY = field(QStringLiteral("camMachineTravelY"), mm(max.at(1).toDouble() - min.at(1).toDouble()));
  QLineEdit* travelZ = field(QStringLiteral("camMachineTravelZ"), mm(max.at(2).toDouble() - min.at(2).toDouble()));
  form->addRow(tr("Travel X"), travelX);
  form->addRow(tr("Travel Y"), travelY);
  form->addRow(tr("Travel Z"), travelZ);
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
  auto* message = new QLabel;
  message->setObjectName(QStringLiteral("camMachineMessage"));
  message->setWordWrap(true);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  auto* layout = new QVBoxLayout(&dialog);
  layout->addLayout(form);
  layout->addWidget(message);
  layout->addWidget(buttons);
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  connect(buttons, &QDialogButtonBox::accepted, &dialog, [&] {
    // Every length as the design evaluates it: "600 mm", "24 in".
    const std::pair<QLineEdit*, QString> lengths[] = {
        {travelX, tr("Travel X")}, {travelY, tr("Travel Y")}, {travelZ, tr("Travel Z")},
        {noseDiameter, tr("Spindle Nose Diameter")}, {noseLength, tr("Spindle Nose Length")},
        {maxFeed, tr("Fastest Feed per Minute")},    {rapid, tr("Rapid Rate per Minute")}};
    double values[7] = {};
    for (int i = 0; i < 7; ++i) {
      if (!evaluate(lengths[i].first->text(), QStringLiteral("length"), &values[i]) || values[i] <= 0.0) {
        message->setText(tr("%1: a length above zero.").arg(lengths[i].second));
        return;
      }
    }
    double rpm[3] = {};
    const std::pair<QLineEdit*, QString> numbers[] = {{minRpm, tr("Lowest Spindle Speed (rpm)")},
                                                      {maxRpm, tr("Highest Spindle Speed (rpm)")},
                                                      {spinUp, tr("Spin-Up Time (s)")}};
    for (int i = 0; i < 3; ++i) {
      if (!evaluate(numbers[i].first->text(), QStringLiteral("unitless"), &rpm[i]) || rpm[i] < 0.0) {
        message->setText(tr("%1: a number of zero or more.").arg(numbers[i].second));
        return;
      }
    }
    const double x0 = min.at(0).toDouble();
    const double y0 = min.at(1).toDouble();
    const double z1 = max.at(2).toDouble();
    body.insert(QStringLiteral("name"), name->text().trimmed());
    body.insert(QStringLiteral("controller"), controller->currentData().toString());
    // The travel keeps its corner: X and Y from their lowest, Z down from
    // its highest (the spindle's top).
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
    if (changer->currentData().toString() != changerKind) {
      body.insert(QStringLiteral("tool_changer"),
                  QJsonObject{{QStringLiteral("kind"), changer->currentData().toString()}});
    }
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
  const std::optional<QJsonObject> tool = askTool(this, QJsonObject(), next, taken);
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
  const std::optional<QJsonObject> changed = askTool(this, tool, number, taken);
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
