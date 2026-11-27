// SPDX-License-Identifier: MIT
// A cut sheet's nesting, cutting paths and plasma program (mitcad#2553;
// app/COMMANDS.md, "Nest Flat Patterns and the cutting program"): the Cut
// Sheet tab's Nest Components…, Show Cutting Paths and Cutting Program…
// and the cut sheet panel's rows of them, members of DrawingWorkspace kept
// apart from its own file. The model works the paths out
// (core/model-api/src/cut_sheets/cutting.rs); this file asks and shows.
#include "DrawingWorkspace.hpp"

#include <exception>

#include <QComboBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QJsonArray>
#include <QLabel>
#include <QPushButton>
#include <QSettings>
#include <QTimer>
#include <QVBoxLayout>
#include <QtLogging>

#include "../browser/DocumentHost.hpp"
#include "../framework/Command.hpp"
#include "../framework/DialogFolders.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/ModelMessages.hpp"
#include "../framework/Theme.hpp"
#include "../framework/UserText.hpp"

namespace mitcad::drawings {
namespace {

// The spacing Nest Components… offers first: the last one chosen.
const char* const kNestGapKey = "drawings/cutSheetNestGap";

// The layout's problems of a cut_sheet_cutting answer, in the interface
// language.
QStringList cuttingProblems(const QJsonObject& cutting) {
  QStringList problems;
  for (const QJsonValue& value : cutting.value(QStringLiteral("problems")).toArray()) {
    problems << mitcad::modelMessage(userText(value.toObject().value(QStringLiteral("message")).toString()));
  }
  return problems;
}

// What a ready layout's program does, in words.
QString readyText(const QJsonObject& cutting) {
  const int cuts = cutting.value(QStringLiteral("pierces")).toInt();
  const int holes = cutting.value(QStringLiteral("holes")).toInt();
  const double length = cutting.value(QStringLiteral("cut_length")).toDouble();
  const double seconds = cutting.value(QStringLiteral("arc_time")).toDouble();
  // Two counts: each its own text with its plural, put into a whole
  // sentence (docs/ui-conventions.md, rule 12).
  return DrawingWorkspace::tr("Ready to cut: %1 (%2), %3 mm cut, the arc on for %4 s.")
      .arg(DrawingWorkspace::tr("%n loop(s)", nullptr, cuts), DrawingWorkspace::tr("%n hole(s)", nullptr, holes))
      .arg(length, 0, 'f', 0)
      .arg(seconds, 0, 'f', 0);
}

} // namespace

void DrawingWorkspace::nestParts() {
  if (!isOpen() || !m_cutSheet) {
    return;
  }
  QDialog dialog(this);
  dialog.setWindowTitle(tr("Nest Components"));
  auto* form = new QFormLayout(&dialog);
  auto* gap = new QDoubleSpinBox;
  gap->setObjectName(QStringLiteral("cutSheetNestGap"));
  gap->setRange(0.0, 1000.0);
  gap->setDecimals(1);
  gap->setSuffix(QStringLiteral(" mm"));
  gap->setValue(QSettings().value(QString::fromLatin1(kNestGapKey), 5.0).toDouble());
  gap->setToolTip(tr("The room between the components and from the sheet's edges"));
  form->addRow(tr("&Spacing"), gap);
  auto* how = new QLabel(tr("Each component is turned so that its longer side lies along the sheet's width, then "
                            "placed in rows from the lower left corner, the tallest first."));
  how->setWordWrap(true);
  form->addRow(how);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  buttons->button(QDialogButtonBox::Ok)->setText(tr("Nest"));
  connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  form->addRow(buttons);
  qDebug().noquote() << QStringLiteral("Cut sheet nest dialog opened");
  if (execDialog(&dialog) != QDialog::Accepted) {
    return;
  }
  QSettings().setValue(QString::fromLatin1(kNestGapKey), gap->value());
  QJsonObject result;
  if (!run({{QStringLiteral("cmd"), QStringLiteral("nest_cut_sheet")},
            {QStringLiteral("drawing"), m_drawing},
            {QStringLiteral("gap"), gap->value()}},
           &result)) {
    return;
  }
  const int warnings = static_cast<int>(result.value(QStringLiteral("warnings")).toArray().size());
  qInfo().noquote() << QStringLiteral("Cut sheet nested: gap %1 mm, %2 warning(s)").arg(gap->value()).arg(warnings);
  // A warning (a component that does not fit) is shown as the sheet is
  // drawn again.
  if (warnings == 0) {
    setStatus(tr("Components nested %1 mm apart.").arg(gap->value()), false);
  }
}

void DrawingWorkspace::setCuttingPaths(bool on) {
  if (on == m_cuttingPaths) {
    return;
  }
  m_cuttingPaths = on;
  m_loggedCutting.clear();
  qDebug().noquote() << QStringLiteral("Cut sheet cutting paths: %1").arg(on ? QStringLiteral("on") : QStringLiteral("off"));
  refresh();
}

void DrawingWorkspace::showCutting(const QJsonObject& answer) {
  const QJsonObject cutting = answer.value(QStringLiteral("cutting_paths")).toObject();
  if (!m_cuttingPaths || cutting.isEmpty()) {
    m_loggedCutting.clear();
    return;
  }
  const QStringList problems = cuttingProblems(cutting);
  const QString line =
      problems.isEmpty()
          ? QStringLiteral("Cut sheet cutting: ready, %1 cuts, %2 holes")
                .arg(cutting.value(QStringLiteral("pierces")).toInt())
                .arg(cutting.value(QStringLiteral("holes")).toInt())
          : QStringLiteral("Cut sheet cutting: %1").arg(problems.join(QLatin1Char(' ')));
  if (line == m_loggedCutting) {
    return;
  }
  m_loggedCutting = line;
  qDebug().noquote() << line;
  if (problems.isEmpty()) {
    setStatus(readyText(cutting), false);
  } else {
    setStatus(problems.join(QLatin1Char(' ')), true);
  }
}

void DrawingWorkspace::addCuttingRows() {
  m_form->addRow(tr("Kerf"), new QLabel(tr("%1 mm").arg(m_answer.value(QStringLiteral("kerf")).toDouble())));
  // With Show Cutting Paths on, whether the layout can be cut.
  const QJsonObject cutting = m_answer.value(QStringLiteral("cutting_paths")).toObject();
  if (m_cuttingPaths && !cutting.isEmpty()) {
    const QStringList problems = cuttingProblems(cutting);
    if (problems.isEmpty()) {
      auto* ready = new QLabel(readyText(cutting));
      ready->setWordWrap(true);
      m_form->addRow(ready);
    }
    for (const QString& problem : problems) {
      auto* said = new QLabel(problem);
      said->setWordWrap(true);
      setErrorStyleSheet(said);
      m_form->addRow(said);
    }
  }
  auto* nest = new QPushButton(tr("Nest Components…"));
  nest->setObjectName(QStringLiteral("cutSheetNest"));
  connect(nest, &QPushButton::clicked, this, [this] { QTimer::singleShot(0, this, [this] { nestParts(); }); });
  m_form->addRow(QString(), nest);
  auto* program = new QPushButton(tr("Cutting Program…"));
  program->setObjectName(QStringLiteral("cutSheetProgram"));
  connect(program, &QPushButton::clicked, this,
          [this] { QTimer::singleShot(0, this, [this] { postCuttingProgram(); }); });
  m_form->addRow(QString(), program);
}

void DrawingWorkspace::postCuttingProgram() {
  if (!isOpen() || !m_cutSheet) {
    return;
  }
  const QJsonObject stored = m_answer.value(QStringLiteral("cutting")).toObject();
  const double storedKerf = m_answer.value(QStringLiteral("kerf")).toDouble();
  QDialog dialog(this);
  dialog.setWindowTitle(tr("Cutting Program"));
  auto* column = new QVBoxLayout(&dialog);
  auto* about = new QLabel(tr("A plasma torch on a grbl 1.1 controller: the torch on M3, off M5, its height on Z. The "
                              "work zero is the sheet's lower left corner on its top surface. Bend lines and marks "
                              "are not cut."));
  about->setWordWrap(true);
  column->addWidget(about);
  auto* form = new QFormLayout;
  column->addLayout(form);
  const auto box = [](double value, double low, double high, int decimals, const QString& suffix, const char* name,
                      const QString& tooltip) {
    auto* spin = new QDoubleSpinBox;
    spin->setObjectName(QString::fromLatin1(name));
    spin->setRange(low, high);
    spin->setDecimals(decimals);
    spin->setSuffix(suffix);
    spin->setValue(value);
    spin->setToolTip(tooltip);
    return spin;
  };
  const auto number = [&stored](const char* key, double fallback) {
    return stored.value(QString::fromLatin1(key)).toDouble(fallback);
  };
  const QString mm = QStringLiteral(" mm");
  QDoubleSpinBox* kerf = box(storedKerf, 0.0, 9.99, 2, mm, "cutKerf",
                             tr("The width the arc takes away: the paths are moved half of it off the components"));
  QDoubleSpinBox* feed = box(number("feed", 2000.0), 1.0, 100000.0, 0, QStringLiteral(" mm/min"), "cutFeed",
                             tr("How fast the torch cuts"));
  QDoubleSpinBox* pierceHeight = box(number("pierce_height", 3.8), 0.01, 100.0, 2, mm, "cutPierceHeight",
                                     tr("The torch's height above the sheet while it pierces"));
  QDoubleSpinBox* pierceDelay = box(number("pierce_delay", 0.5), 0.0, 60.0, 2, QStringLiteral(" s"), "cutPierceDelay",
                                    tr("How long the torch waits after it fires before it moves"));
  QDoubleSpinBox* cutHeight = box(number("cut_height", 1.5), 0.01, 100.0, 2, mm, "cutCutHeight",
                                  tr("The torch's height above the sheet while it cuts"));
  QDoubleSpinBox* travelHeight = box(number("travel_height", 10.0), 0.1, 1000.0, 1, mm, "cutTravelHeight",
                                     tr("The height the torch moves across the sheet at"));
  QDoubleSpinBox* leadIn = box(number("lead_in", 4.0), 0.0, 100.0, 1, mm, "cutLeadIn",
                               tr("The longest straight run from the pierce point onto the outline"));
  auto* units = new QComboBox;
  units->setObjectName(QStringLiteral("cutUnits"));
  units->addItem(tr("Millimeters"), QStringLiteral("millimetres"));
  units->addItem(tr("Inches"), QStringLiteral("inches"));
  units->setCurrentIndex(stored.value(QStringLiteral("units")).toString() == QStringLiteral("inches") ? 1 : 0);
  units->setToolTip(tr("The units of the program's numbers"));
  form->addRow(tr("&Kerf"), kerf);
  form->addRow(tr("&Feed"), feed);
  form->addRow(tr("&Pierce height"), pierceHeight);
  form->addRow(tr("Pierce &delay"), pierceDelay);
  form->addRow(tr("&Cut height"), cutHeight);
  form->addRow(tr("&Travel height"), travelHeight);
  form->addRow(tr("&Lead-in"), leadIn);
  form->addRow(tr("&Units"), units);
  auto* state = new QLabel;
  state->setObjectName(QStringLiteral("cutProgramState"));
  state->setWordWrap(true);
  column->addWidget(state);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Cancel);
  QPushButton* save = buttons->addButton(tr("Save Program…"), QDialogButtonBox::AcceptRole);
  save->setObjectName(QStringLiteral("cutProgramSave"));
  connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  column->addWidget(buttons);
  const auto settings = [=] {
    QJsonObject values{{QStringLiteral("feed"), feed->value()},
                       {QStringLiteral("pierce_height"), pierceHeight->value()},
                       {QStringLiteral("pierce_delay"), pierceDelay->value()},
                       {QStringLiteral("cut_height"), cutHeight->value()},
                       {QStringLiteral("travel_height"), travelHeight->value()},
                       {QStringLiteral("lead_in"), leadIn->value()},
                       {QStringLiteral("units"), units->currentData().toString()}};
    // The line numbers as the cut sheet keeps them.
    values.insert(QStringLiteral("line_numbers"), stored.value(QStringLiteral("line_numbers")).toBool());
    return values;
  };
  // Tried with the dialog's values: the model says whether the layout can
  // be cut with them, and why not.
  const auto check = [this, settings, kerf, state, save] {
    QJsonObject answer;
    try {
      answer = m_host.model().queryObject({{QStringLiteral("query"), QStringLiteral("cut_sheet_cutting")},
                                           {QStringLiteral("drawing"), m_drawing},
                                           {QStringLiteral("cutting"), settings()},
                                           {QStringLiteral("kerf"), kerf->value()},
                                           {QStringLiteral("geometry"), false}});
    } catch (const std::exception& e) {
      state->setText(mitcad::modelMessage(userText(QString::fromUtf8(e.what()))));
      setErrorStyleSheet(state);
      save->setEnabled(false);
      return;
    }
    const QStringList problems = cuttingProblems(answer);
    const bool ready = answer.value(QStringLiteral("ready")).toBool();
    if (ready) {
      state->setText(readyText(answer));
      state->setStyleSheet(QString());
    } else {
      state->setText(problems.join(QLatin1Char('\n')));
      setErrorStyleSheet(state);
    }
    save->setEnabled(ready);
    qDebug().noquote() << QStringLiteral("Cutting program check: %1")
                              .arg(ready ? QStringLiteral("ready") : problems.join(QLatin1Char(' ')));
  };
  for (QDoubleSpinBox* spin : {kerf, feed, pierceHeight, pierceDelay, cutHeight, travelHeight, leadIn}) {
    connect(spin, &QDoubleSpinBox::valueChanged, &dialog, [check] { check(); });
  }
  connect(units, &QComboBox::currentIndexChanged, &dialog, [check] { check(); });
  check();
  qDebug().noquote() << QStringLiteral("Cutting program dialog opened");
  if (execDialog(&dialog) != QDialog::Accepted) {
    return;
  }
  const QJsonObject chosen = settings();
  // The settings kept with the cut sheet when they changed and the model
  // can change now (one undo step); the program takes them either way.
  if ((chosen != stored || kerf->value() != storedKerf) && m_host.canChangeModel()) {
    run({{QStringLiteral("cmd"), QStringLiteral("edit_cut_sheet")},
         {QStringLiteral("drawing"), m_drawing},
         {QStringLiteral("kerf"), kerf->value()},
         {QStringLiteral("cutting"), chosen}});
  }
  const QString suggested =
      QDir(dialogStartFolder(DialogGroup::Export, QDir::homePath())).filePath(m_name + QStringLiteral(".nc"));
  const QString path = dialogSaveFileName(this, tr("Save Cutting Program"), suggested, tr("G-code (*.nc)"));
  if (path.isEmpty()) {
    return;
  }
  rememberDialogFolder(DialogGroup::Export, path);
  QJsonObject result;
  qDebug().noquote() << QStringLiteral("Cutting program: %1").arg(QDir::toNativeSeparators(path));
  if (!m_host.runModelCommand({{QStringLiteral("cmd"), QStringLiteral("post_cut_sheet")},
                               {QStringLiteral("drawing"), m_drawing},
                               {QStringLiteral("path"), path},
                               {QStringLiteral("cutting"), chosen},
                               {QStringLiteral("kerf"), kerf->value()}},
                              &result)) {
    setStatus(mitcad::modelMessage(userText(m_host.lastError())), true);
    return;
  }
  const int cuts = result.value(QStringLiteral("pierces")).toInt();
  qInfo().noquote() << QStringLiteral("Cutting program written %1: cut sheet %2, %3 cuts, %4 lines")
                           .arg(path, m_name)
                           .arg(cuts)
                           .arg(result.value(QStringLiteral("lines")).toInt());
  setStatus(tr("Saved the cutting program %1 (%n loop(s)).", nullptr, cuts).arg(QDir::toNativeSeparators(path)),
            false);
}

} // namespace mitcad::drawings
