// SPDX-License-Identifier: MIT
// Drawings kept in the design (mitcad#707, app/COMMANDS.md "Drawings"):
// Create Drawing (the File menu, the browser's Drawings folders and a
// component's menu) with its sheet's size, orientation and projection
// method, a drawing opened in its own view over the 3D view
// (DrawingWorkspace) and back, the drawing tools as commands of the
// window on the ribbon's DRAWING tab (mitcad#1008), and the view following
// the model.
#include "../MainWindow.hpp"

#include <algorithm>
#include <exception>
#include <memory>
#include <optional>
#include <utility>

#include <QAction>
#include <QApplication>
#include <QComboBox>
#include <QCursor>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QFileInfo>
#include <QFormLayout>
#include <QJsonArray>
#include <QJsonObject>
#include <QKeySequence>
#include <QLabel>
#include <QMenu>
#include <QSettings>
#include <QtLogging>

#include <gp_Dir.hxx>

#include "../OcctViewer.hpp"
#include "../browser/BrowserBackground.hpp"
#include "../browser/BrowserController.hpp"
#include "../framework/CommandLayout.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/FloatingLayout.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Ribbon.hpp"
#include "DrawingTemplates.hpp"
#include "DrawingTools.hpp"
#include "DrawingWorkspace.hpp"
#include "DrawingsPanel.hpp"
#include "SheetSizeRows.hpp"
#include "../view/VersionWatermark.hpp"
#ifdef MITCAD_MACOS_NATIVE_UI
#include "../platform/NativeMacWindow.hpp"
#endif

namespace mitcad {
namespace {

// The 3D view's direction as a base view's Current View takes it
// (mitcad#725): toward the eye (the camera looks the other way) and up.
QJsonObject cameraOf(const OcctViewer& viewer) {
  const gp_Dir eye = viewer.viewDirection().Reversed();
  const gp_Dir up = viewer.viewUp();
  return {{QStringLiteral("toward_eye"), QJsonArray{eye.X() + 0.0, eye.Y() + 0.0, eye.Z() + 0.0}},
          {QStringLiteral("up"), QJsonArray{up.X() + 0.0, up.Y() + 0.0, up.Z() + 0.0}}};
}

// Create Drawing's question: a drawing (the first sheet's template, size,
// orientation and projection method, the model's names) or a cut sheet
// (mitcad#853: the material's width and height, mm, as `kind`
// "cut_sheet"), or none when cancelled. A template chosen brings its own
// sheet size; it opens with `start` (mitcad#1218: the last choice, else
// ISO), and a drawing's answer names its template's choice as
// `template_choice`, to be kept for the next time.
std::optional<QJsonObject> askNewDrawing(QWidget* parent, const QList<drawings::TemplateChoice>& templates,
                                         const drawings::StartingChoice& start) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Create Drawing"));
  auto* form = new QFormLayout(&dialog);
  const auto combo = [](const QList<QPair<QString, QString>>& choices) {
    auto* box = new QComboBox;
    for (const auto& [value, label] : choices) {
      box->addItem(label, value);
    }
    return box;
  };
  // A drawing, or a cut sheet of flat parts at 1:1 (mitcad#853).
  QComboBox* kind = combo({{QStringLiteral("drawing"), QObject::tr("Drawing")},
                           {QStringLiteral("cut_sheet"), QObject::tr("Cut Sheet")}});
  kind->setObjectName(QStringLiteral("drawingKind"));
  kind->setToolTip(QObject::tr("A drawing of views, or a cut sheet of flat components at 1:1 for cutting"));
  form->addRow(QObject::tr("&Kind"), kind);
  auto* templateBox = new QComboBox;
  templateBox->setObjectName(QStringLiteral("drawingTemplate"));
  for (const drawings::TemplateChoice& choice : templates) {
    templateBox->addItem(choice.label, choice.choice);
  }
  templateBox->setCurrentIndex(std::max(0, drawings::templateIndex(templates, start.choice.choice)));
  form->addRow(QObject::tr("&Template"), templateBox);
  // The named sizes and Custom, a width and height of the user's
  // (mitcad#1625).
  drawings::SheetSizeRows sizeRows(form, QStringLiteral("drawingSize"));
  QComboBox* sizes = sizeRows.combo();
  QComboBox* orientation = combo({{QStringLiteral("landscape"), QObject::tr("Landscape")},
                                  {QStringLiteral("portrait"), QObject::tr("Portrait")}});
  orientation->setObjectName(QStringLiteral("drawingOrientation"));
  form->addRow(QObject::tr("Orientation"), orientation);
  sizeRows.linkOrientation(orientation);
  QComboBox* projection = combo({{QStringLiteral("third_angle"), QObject::tr("Third Angle")},
                                 {QStringLiteral("first_angle"), QObject::tr("First Angle")}});
  projection->setObjectName(QStringLiteral("drawingProjection"));
  form->addRow(QObject::tr("Projection"), projection);
  // A template chosen brings its sheet size and its projection method
  // (ISO: A3, first angle; ANSI: B, third angle; mitcad#725).
  const auto templateSheet = [templateBox, &sizeRows, projection, &templates] {
    const int at = drawings::templateIndex(templates, templateBox->currentData().toString());
    if (at >= 0) {
      sizeRows.set(templates.at(at).size);
    }
    const int method = at < 0 ? -1 : projection->findData(templates.at(at).projection);
    if (method >= 0) {
      projection->setCurrentIndex(method);
    }
  };
  QObject::connect(templateBox, &QComboBox::currentIndexChanged, sizes, templateSheet);
  // The template chosen first brings its sheet too, as the handler was not
  // connected when it was chosen (mitcad#722).
  templateSheet();
  // Then the sheet chosen last with it (mitcad#1218); a value the lists do
  // not have stays as the template brought it.
  sizeRows.set(start.choice.size);
  for (const auto& [box, value] : {std::pair<QComboBox*, QString>{orientation, start.choice.orientation},
                                   std::pair<QComboBox*, QString>{projection, start.choice.projection}}) {
    const int at = value.isEmpty() ? -1 : box->findData(value);
    if (at >= 0) {
      box->setCurrentIndex(at);
    }
  }
  const auto choiceName = [](const QString& choice) {
    return choice.isEmpty() ? QStringLiteral("simple") : choice;
  };
  if (!start.note.isEmpty()) {
    qDebug().noquote() << start.note;
  }
  qDebug().noquote() << QStringLiteral("Create Drawing dialog starts with template %1, %2, %3, %4 (%5)")
                            .arg(choiceName(templateBox->currentData().toString()), sizeRows.text(),
                                 orientation->currentData().toString(), projection->currentData().toString(),
                                 drawings::startSourceName(start.source));
  // A cut sheet's material, 600 x 300 mm unless typed.
  const auto length = [](double value, const char* name) {
    auto* box = new QDoubleSpinBox;
    box->setObjectName(QString::fromLatin1(name));
    box->setRange(1.0, 100000.0);
    box->setDecimals(1);
    box->setSuffix(QStringLiteral(" mm"));
    box->setValue(value);
    return box;
  };
  QDoubleSpinBox* width = length(600.0, "drawingCutWidth");
  QDoubleSpinBox* height = length(300.0, "drawingCutHeight");
  form->addRow(QObject::tr("&Width"), width);
  form->addRow(QObject::tr("&Height"), height);
  const auto showKind = [form, kind, templateBox, orientation, projection, width, height, &sizeRows] {
    const bool cut = kind->currentData().toString() == QStringLiteral("cut_sheet");
    for (QWidget* field : {static_cast<QWidget*>(templateBox), static_cast<QWidget*>(orientation),
                           static_cast<QWidget*>(projection)}) {
      form->setRowVisible(field, !cut);
    }
    // The sheet's size, and its own width and height while Custom is
    // chosen (mitcad#1625).
    sizeRows.setShown(!cut);
    form->setRowVisible(width, cut);
    form->setRowVisible(height, cut);
  };
  QObject::connect(kind, &QComboBox::currentIndexChanged, &dialog, showKind);
  showKind();
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  form->addRow(buttons);
  qDebug().noquote() << QStringLiteral("Create Drawing dialog opened");
  if (execDialog(&dialog) != QDialog::Accepted) {
    qDebug().noquote() << QStringLiteral("Create Drawing cancelled");
    return std::nullopt;
  }
  if (kind->currentData().toString() == QStringLiteral("cut_sheet")) {
    qDebug().noquote() << QStringLiteral("Create Drawing dialog: cut sheet %1 x %2")
                              .arg(width->value(), 0, 'g', 7)
                              .arg(height->value(), 0, 'g', 7);
    return QJsonObject{{QStringLiteral("kind"), QStringLiteral("cut_sheet")},
                       {QStringLiteral("width"), width->value()},
                       {QStringLiteral("height"), height->value()}};
  }
  QJsonObject out{{QStringLiteral("size"), sizeRows.value()},
                  {QStringLiteral("orientation"), orientation->currentData().toString()},
                  {QStringLiteral("projection"), projection->currentData().toString()},
                  {QStringLiteral("template_choice"), templateBox->currentData().toString()}};
  const int chosen = drawings::templateIndex(templates, templateBox->currentData().toString());
  if (chosen >= 0) {
    out.insert(QStringLiteral("template"), templates.at(chosen).value);
  }
  qDebug().noquote() << QStringLiteral("Create Drawing dialog: template %1, %2, %3")
                            .arg(chosen >= 0 && !templates.at(chosen).choice.isEmpty() ? templates.at(chosen).choice
                                                                                       : QStringLiteral("simple"),
                                 sizeRows.text(), projection->currentData().toString());
  return out;
}

} // namespace

drawings::DrawingWorkspace* MainWindow::drawingWorkspace() {
  if (m_drawingWorkspace == nullptr) {
    m_drawingWorkspace = new drawings::DrawingWorkspace(
        m_viewer, *this, [this] { return drawingTitle(); }, [this] { return cameraOf(*m_viewer); },
        [this](const QKeySequence& key) {
          // A shortcut of the model's commands (a feature, a command of a
          // ribbon tab other than the drawing's), which would leave the
          // drawing (trigger, mitcad#725).
          for (const auto& def : m_registry->commands()) {
            if (leavesDrawing(*def) && m_registry->effectiveShortcuts(def->id).contains(key)) {
              return true;
            }
          }
          return false;
        });
    // The Drawings card marks the sheet shown (mitcad#1990).
    connect(m_drawingWorkspace, &drawings::DrawingWorkspace::sheetShown, this, [this] { refreshDrawingsCard(); });
    connect(m_drawingWorkspace, &drawings::DrawingWorkspace::closed, this, [this] {
      m_drawingDesign.clear();
      m_viewer->setFocus(Qt::OtherFocusReason);
      updateDrawingChrome();
      updateActions();
    });
    // The tool on shows checked on the ribbon's DRAWING tab (mitcad#1008).
    connect(m_drawingWorkspace, &drawings::DrawingWorkspace::toolChanged, this, [this](const QString& active) {
      for (const drawings::DrawingToolDef& tool : drawings::drawingTools()) {
        if (QAction* action = m_registry->action(tool.id)) {
          action->setCheckable(true);
          action->setChecked(tool.id == active);
        }
      }
#ifdef MITCAD_MACOS_NATIVE_UI
      // The AppKit ribbon's buttons and menus too (mitcad#1073).
      if (m_nativeWindow) {
        refreshNativeActions();
      }
#endif
    });
    if (m_cutLabelsAction != nullptr) {
      m_drawingWorkspace->setCutLabels(m_cutLabelsAction->isChecked());
    }
    // Edit Parameters one click away while a drawing is open (mitcad#1582;
    // the navigation bar, which has it in the 3D view, is hidden here).
    m_drawingWorkspace->addBarAction(m_registry->action(QStringLiteral("solid.parameters")));
    // The tools are the ribbon's (mitcad#1008), the AppKit frontend's too
    // (mitcad#1073): its ribbon shows the retained ribbon's DRAWING tab
    // (MainWindow::refreshNativeActions).
    if (m_ribbon != nullptr) {
      m_drawingWorkspace->setRibbonTools([this](const QString& name) { return m_ribbon->placeOf(name); });
      connect(m_ribbon, &Ribbon::placesLogged, m_drawingWorkspace, &drawings::DrawingWorkspace::logPlaces);
    }
  }
  return m_drawingWorkspace;
}

bool MainWindow::isDrawingCommand(const CommandDef& def) {
  // A command of a drawing's tabs in the default layout (mitcad#1951).
  const QString tab = defaultHome(def.id).tab;
  return tab == QStringLiteral("DRAWING") || tab == QStringLiteral("CUT SHEET");
}

bool MainWindow::leavesDrawing(const CommandDef& def) {
  if (def.kind == CommandDef::Kind::Feature) {
    return true;
  }
  // Edit Parameters opens its table over the drawing (mitcad#1582): the
  // drawing follows the design's changes. Design Properties… is a dialog
  // of the File menu that also shows on the Manage tab (mitcad#1975).
  if (def.id == QStringLiteral("solid.parameters") || def.id == QStringLiteral("file.design_properties")) {
    return false;
  }
  return !defaultHome(def.id).isEmpty() && !isDrawingCommand(def);
}

void MainWindow::updateDrawingChrome() {
  const bool open = drawingOpen();
  // The model's idle hint ("Create a sketch to start modelling…") says
  // nothing about the drawing: it goes; a warning or an error stays, and
  // the drawing's own messages come in the same place (mitcad#1008).
  if (open && m_statusIsHint && !m_statusLabel->text().isEmpty()) {
    showHint(QString());
  }
  // The version watermark (mitcad#1209) at the sheet's lower right.
  if (m_versionWatermark != nullptr) {
    m_versionWatermark->setArea(open ? m_drawingWorkspace->sheetView() : nullptr);
  }
  if (m_ribbon == nullptr) {
    return;
  }
  // A drawing opened from the browser while Render is shown leaves Render
  // first (mitcad#1990).
  if (open) {
    setRenderWorkspace(false);
  }
  // The AppKit frontend reads its tabs from this ribbon (mitcad#1073).
  m_ribbon->setDrawingMode(open, open && m_drawingWorkspace->isCutSheet());
  // The Docked layout's model docks out of the sheet's way while it is
  // open, back as they were after (mitcad#2082); the Browser and the Chat
  // panel stay.
  updateDrawingDocks(open);
  // The sheet takes the whole view: of the cards over it only the Drawings
  // card stays (mitcad#1008), the design's drawings and their sheets in
  // place of the whole model's Browser, from which another drawing or sheet
  // is opened (mitcad#1990); the Browser's card comes back with the 3D view.
  if (m_floating != nullptr) {
    if (m_drawingsCard != nullptr) {
      m_drawingsCard->setVisible(open);
    }
    m_floating->showOnlyCards(open ? drawingCards() : QStringList());
    if (open) {
      refreshDrawingsCard();
    }
  }
#ifdef MITCAD_MACOS_NATIVE_UI
  if (m_nativeWindow) {
    m_nativeWindow->setDrawingOpen(open);
  }
#endif
  // The navigation bar of the 3D view (mitcad#1086): in the Docked
  // layout's status bar too, not while the sheet takes the view.
  updateNavigationBar();
}

void MainWindow::addDrawingRibbonExtras() {
  // A cut sheet's tab (mitcad#853, mitcad#1008): Close Drawing as the
  // drawing's, and its Export: DXF…, SVG… and whether the parts' names go
  // on the layer LABEL.
  QAction* closeAction = m_registry->action(QStringLiteral("drawing.close"));
  QAction* dxfAction = m_registry->action(QStringLiteral("drawing.export_dxf"));
  QAction* svgAction = m_registry->action(QStringLiteral("drawing.export_svg"));
  m_cutLabelsAction = new QAction(tr("Component Names on the LABEL Layer"), this);
  m_cutLabelsAction->setCheckable(true);
  connect(m_cutLabelsAction, &QAction::toggled, this, [this](bool on) {
    if (m_drawingWorkspace != nullptr) {
      m_drawingWorkspace->setCutLabels(on);
    }
  });
  auto* separator = new QAction(this);
  separator->setSeparator(true);
  if (closeAction != nullptr) {
    m_ribbon->addGroupExtras(QStringLiteral("cut_sheet.close"), {closeAction}, {closeAction});
  }
  if (dxfAction != nullptr && svgAction != nullptr) {
    m_ribbon->addGroupExtras(QStringLiteral("cut_sheet.export"),
                             {dxfAction, svgAction, separator, m_cutLabelsAction});
  }
}

QString MainWindow::drawingTitle() const {
  return m_filePath.isEmpty() ? documentName() : QFileInfo(m_filePath).completeBaseName();
}

bool MainWindow::drawingOpen() const { return m_drawingWorkspace != nullptr && m_drawingWorkspace->isOpen(); }

void MainWindow::registerDrawingCommands() {
  // The drawing tools, Close Drawing and the exports as the window's
  // commands: the command search and the shortcuts find them, enabled
  // while a drawing is open. On the ribbon's DRAWING tab (mitcad#1008),
  // shown only while a drawing is open; a command of the drawing's tabs
  // does not lead back to the 3D view (trigger, isDrawingCommand).
  // `when`: for a drawing ("drawing"), a cut sheet ("cut") or either ("").
  const auto add = [this](const QString& id, const QString& name, const QString& icon, const QString& tooltip,
                          const QString& when, std::function<void()> run) {
    CommandDef def;
    def.id = id;
    def.name = name;
    def.icon = icon;
    def.tooltip = tooltip;
    def.kind = CommandDef::Kind::Action;
    def.mode = CommandDef::Mode::Model;
    // The tools' other words (Leader, Chain, BOM, …; mitcad#725).
    def.keywords = drawings::toolKeywords(id);
    def.run = std::move(run);
    def.enabled = [this, when] {
      if (!drawingOpen()) {
        return false;
      }
      return when.isEmpty() || m_drawingWorkspace->isCutSheet() == (when == QStringLiteral("cut"));
    };
    m_registry->add(def);
  };
  add(QStringLiteral("drawing.close"), tr("Close Drawing"), QStringLiteral("view-arrow-left"),
      tr("Shows the 3D view again"), QString(), [this] { leaveDrawing(); });
  for (const drawings::DrawingToolDef& tool : drawings::drawingTools()) {
    const QString id = tool.id;
    // The drawing's Bill of Materials is told apart from the Build tab's in the
    // command search (mitcad#718).
    const QString name = id == QStringLiteral("drawing.parts_list") ? tr("Bill of Materials (Drawing)") : tool.name;
    add(id, name, tool.icon, tool.tooltip, QStringLiteral("drawing"), [this, id] {
          if (drawingOpen()) {
            m_drawingWorkspace->startTool(id);
          }
        });
  }
  // Export: the menu of the group's caption, as the bar's Export… had it;
  // DXF and SVG also a cut sheet's (its tab's extras, addDrawingRibbonExtras).
  struct ExportFormat {
    const char* format;
    const char* label;
    const char* kind;
  };
  static const ExportFormat formats[] = {
      {"pdf", QT_TRANSLATE_NOOP("mitcad::MainWindow", "PDF…"), "drawing"},
      {"svg", QT_TRANSLATE_NOOP("mitcad::MainWindow", "SVG…"), ""},
      {"dxf", QT_TRANSLATE_NOOP("mitcad::MainWindow", "DXF…"), ""},
      {"dwg", QT_TRANSLATE_NOOP("mitcad::MainWindow", "DWG…"), "drawing"}};
  for (const ExportFormat& exported : formats) {
    const QString suffix = QString::fromLatin1(exported.format);
    add(QStringLiteral("drawing.export_") + suffix, tr(exported.label), QStringLiteral("export"),
        tr("Writes the drawing as a %1 file").arg(suffix.toUpper()), QString::fromLatin1(exported.kind),
        [this, suffix] {
          if (drawingOpen()) {
            m_drawingWorkspace->exportDrawing(suffix);
          }
        });
  }
  // A cut sheet's Add Parts… (mitcad#853), on its CUT SHEET tab.
  add(QStringLiteral("drawing.add_parts"), tr("Add Components…"), QStringLiteral("add"),
      tr("Adds bodies as flat components, each seen along its largest planar face"), QStringLiteral("cut"), [this] {
        if (drawingOpen()) {
          m_drawingWorkspace->addParts();
        }
      });
  // The sheet's size, projection, units, template and title block, on the
  // DRAWING tab's Sheet group (mitcad#1990; the drawing's bar had it).
  add(QStringLiteral("drawing.edit_sheet"), tr("Sheet…"), QStringLiteral("settings"),
      tr("The sheet's size, projection, units, template and title block"), QStringLiteral("drawing"), [this] {
        if (drawingOpen()) {
          m_drawingWorkspace->editSheetSettings();
        }
      });
  // A row of the drawing's revision table (mitcad#707 phase 4), on the
  // DRAWING tab's Sheet group (mitcad#1990).
  add(QStringLiteral("drawing.add_revision"), tr("Add Revision…"), QStringLiteral("version-history"),
      tr("Adds a row to the drawing's revision table"), QStringLiteral("drawing"), [this] {
        if (drawingOpen()) {
          m_drawingWorkspace->addRevision();
        }
      });
}

void MainWindow::openDrawing(const QString& uid) {
  if (m_mode != Mode::Idle) {
    showStatus(changeBlockedReason(), NoticeLevel::Error);
    return;
  }
  m_drawingDesign = designId();
  if (!drawingWorkspace()->open(uid)) {
    m_drawingDesign.clear();
    showStatus(tr("There is no drawing %1.").arg(uid), NoticeLevel::Error);
  }
  updateDrawingChrome();
  updateActions();
}

void MainWindow::createDrawing(const QString& component) {
  if (!canChangeModel()) {
    showStatus(changeBlockedReason(), NoticeLevel::Error);
    return;
  }
  // The dialog opens with the last choice (mitcad#1218), kept in the
  // user's settings.
  QSettings settings;
  const QList<drawings::TemplateChoice> templates = drawings::availableTemplates(*this);
  const std::optional<QJsonObject> sheet =
      askNewDrawing(this, templates, drawings::startingChoice(templates, drawings::NewDrawingChoice::load(settings)));
  if (!sheet) {
    return;
  }
  QJsonObject command = *sheet;
  const QString templateChoice = command.take(QStringLiteral("template_choice")).toString();
  // A cut sheet (mitcad#853) starts with the bodies selected in the model.
  const bool cut = command.take(QStringLiteral("kind")).toString() == QStringLiteral("cut_sheet");
  command.insert(QStringLiteral("cmd"), cut ? QStringLiteral("add_cut_sheet") : QStringLiteral("add_drawing"));
  if (cut) {
    const QJsonArray bodies = selectedModelBodies();
    if (!bodies.isEmpty()) {
      command.insert(QStringLiteral("bodies"), bodies);
    }
  }
  if (!component.isEmpty() && component != QStringLiteral("C0")) {
    command.insert(QStringLiteral("component"), component);
  }
  QJsonObject result;
  if (!runModelCommand(command, &result)) {
    return;
  }
  const QString uid = result.value(QStringLiteral("drawing")).toString();
  qDebug().noquote() << QStringLiteral("Drawing created: %1 %2 in %3")
                            .arg(uid, result.value(QStringLiteral("name")).toString(),
                                 result.value(QStringLiteral("component")).toString());
  if (!cut) {
    // The next Create Drawing opens with this choice (mitcad#1218).
    drawings::NewDrawingChoice kept;
    kept.choice = templateChoice;
    kept.size = drawings::sheetSizeText(command.value(QStringLiteral("size")));
    kept.orientation = command.value(QStringLiteral("orientation")).toString();
    kept.projection = command.value(QStringLiteral("projection")).toString();
    kept.save(settings);
    qDebug().noquote() << QStringLiteral("Create Drawing choice kept: template %1, %2, %3, %4")
                              .arg(kept.choice.isEmpty() ? QStringLiteral("simple") : kept.choice, kept.size,
                                   kept.orientation, kept.projection);
  }
  openDrawing(uid);
  // A new drawing's first view comes next (mitcad#725): Base View is on.
  // A cut sheet takes its parts from Add Parts.
  if (drawingOpen() && !cut) {
    m_drawingWorkspace->startTool(QStringLiteral("drawing.base_view"));
  }
}

bool MainWindow::queryInJob(const QString& label, const QJsonObject& query, QObject* context, QueryDone done) {
  if (modelBusy()) {
    return false;
  }
  // Read on the worker, handed over when the job ends.
  auto answer = std::make_shared<QJsonObject>();
  return m_documentSession->queryAsync(
      label, query, [answer](const QJsonValue& value) { *answer = value.toObject(); }, context,
      [answer, done = std::move(done)](const DocumentSession::AsyncResult& outcome) {
        done(*answer, outcome.cancelled, outcome.cancelled ? QString() : outcome.error);
      });
}

QJsonArray MainWindow::selectedModelBodies() const {
  QJsonArray out;
  for (const SelectionItem& item : m_selection) {
    if (item.kind != SelectKind::Body || item.owner.isEmpty()) {
      continue;
    }
    const QJsonValue body = item.occurrence.isEmpty()
                                ? QJsonValue(item.owner)
                                : QJsonValue(QJsonObject{{QStringLiteral("body"), item.owner},
                                                         {QStringLiteral("occurrence"), item.occurrence}});
    if (!out.contains(body)) {
      out.append(body);
    }
  }
  return out;
}

void MainWindow::followDrawing() {
  if (!drawingOpen()) {
    return;
  }
  // Another design: its drawings are not this one's.
  if (designId() != m_drawingDesign) {
    m_drawingWorkspace->closeDrawing();
    return;
  }
  m_drawingWorkspace->refresh();
  // The Drawings card follows the design's drawings (mitcad#1990).
  refreshDrawingsCard();
}

QStringList MainWindow::drawingCards() const {
  return {m_drawingsCard != nullptr ? QStringLiteral("drawingsDock") : QStringLiteral("browserDock")};
}

void MainWindow::addDrawingsCard() {
  if (m_floating == nullptr || m_drawingsCard != nullptr || m_controller == nullptr) {
    return;
  }
  drawings::DrawingsPanel::Hooks hooks;
  hooks.open = [this](const QString& drawing) { openDrawing(drawing); };
  hooks.showSheet = [this](int sheet) {
    if (drawingOpen()) {
      m_drawingWorkspace->chooseSheet(sheet);
    }
  };
  // The command file.create_drawing's run: a drawing of the design.
  hooks.create = [this] { createDrawing(QStringLiteral("C0")); };
  hooks.menu = [this](const QString& drawing, const QString& component, const QString& name, const QPoint& at,
                      const std::function<void()>& startRename) {
    m_controller->drawingMenu(drawing, component, name, at, startRename);
  };
  hooks.rename = [this](const QString& drawing, const QString& name) { m_controller->renameDrawing(drawing, name); };
  m_drawingsPanel = new drawings::DrawingsPanel(std::move(hooks));
  m_drawingsCard = new GlassCard;
  m_drawingsCard->setObjectName(QStringLiteral("drawingsDock"));
  m_drawingsCard->setTitle(tr("Drawings"));
  m_drawingsCard->setContent(m_drawingsPanel, QMargins(8, 0, 8, 8));
  // In the left column, as high as its rows (and its + Drawing); hidden
  // until a drawing opens.
  FloatingLayout::Sizing sizing{kBrowserCardWidth, [panel = m_drawingsPanel] { return panel->contentHeight(); },
                                1.0, false};
  m_floating->addCard(m_drawingsCard, FloatingLayout::Anchor::TopLeft, sizing);
  connect(m_drawingsPanel, &drawings::DrawingsPanel::contentHeightChanged, m_floating,
          &FloatingLayout::requestLayout);
  m_floating->watchContent(m_drawingsPanel);
}

void MainWindow::refreshDrawingsCard() {
  if (m_drawingsPanel == nullptr || !drawingOpen()) {
    return;
  }
  m_drawingsPanel->setDrawings(m_snapshot.drawings, m_drawingWorkspace->drawing(), m_drawingWorkspace->sheet());
}

bool MainWindow::escapeDrawing() {
  if (!drawingOpen()) {
    return false;
  }
  const QWidget* focus = QApplication::focusWidget();
  if (focus != nullptr && focus != m_viewer && !m_drawingWorkspace->isAncestorOf(focus)) {
    return false; // the browser's, a panel's, ...
  }
  m_drawingWorkspace->escape();
  return true;
}

void MainWindow::leaveDrawing() {
  if (drawingOpen()) {
    m_drawingWorkspace->closeDrawing();
  }
}

void MainWindow::setRenderWorkspace(bool on) {
  if (m_ribbon == nullptr || m_ribbon->renderTabShown() == on) {
    return;
  }
  m_ribbon->setRenderMode(on);
  holdRenderedView(on);
  // Its stages, panels and live line (mitcad#2013), also when the tab
  // shown did not change.
  updateRenderWorkspace(on ? QStringLiteral("RENDER") : m_ribbon->currentTab());
}

void MainWindow::holdRenderedView(bool on) {
  if (on == m_renderedViewHeld) {
    return;
  }
  m_renderedViewHeld = on;
  // The rendered view on while the Render workspace is shown; the view
  // mode that was on before it comes back when it is left (the owner,
  // 17.11.2026).
  QAction* rendered = m_registry->action(QStringLiteral("view.rendered"));
  if (on) {
    m_renderedBeforeRender = rendered != nullptr && rendered->isChecked();
    if (rendered != nullptr && rendered->isEnabled() && !rendered->isChecked()) {
      rendered->trigger();
    }
    qDebug().noquote() << QStringLiteral("Workspace RENDER: the rendered view %1")
                              .arg(rendered != nullptr && rendered->isChecked() ? QStringLiteral("on")
                                                                                 : QStringLiteral("not available"));
  } else {
    if (rendered != nullptr && !m_renderedBeforeRender && rendered->isChecked()) {
      rendered->trigger();
    }
    qDebug().noquote() << QStringLiteral("Workspace RENDER left: the rendered view %1")
                              .arg(rendered != nullptr && rendered->isChecked() ? QStringLiteral("on")
                                                                                 : QStringLiteral("off"));
  }
}

void MainWindow::requestWorkspace(const QString& workspace) {
  if (workspace == QLatin1String("RENDER")) {
    // From a drawing: it closes first, then the Render tab (mitcad#1990).
    if (drawingOpen()) {
      qDebug().noquote() << QStringLiteral("Workspace RENDER: the drawing closed");
      leaveDrawing();
    }
    setRenderWorkspace(true);
    return;
  }
  // Another workspace leaves Render first.
  setRenderWorkspace(false);
  if (workspace != QLatin1String("DRAWING")) {
    // Design or Make while a drawing is open: the drawing closes, and the
    // ribbon shows that workspace's tabs (mitcad#1990).
    if (drawingOpen()) {
      qDebug().noquote() << QStringLiteral("Workspace %1: the drawing closed").arg(workspace);
      leaveDrawing();
    }
    return;
  }
  if (drawingOpen()) {
    return;
  }
  // The design's drawings and cut sheets, as the browser's Drawings
  // folders list them.
  QJsonArray drawings;
  try {
    drawings = queryArray(QStringLiteral("drawings"));
  } catch (const std::exception&) {
  }
  if (drawings.isEmpty()) {
    // None yet: Create Drawing, as File › Create Drawing… asks for it.
    qDebug().noquote() << QStringLiteral("Workspace DRAWING: no drawing, Create Drawing asked");
    createDrawing(QStringLiteral("C0"));
    return;
  }
  if (drawings.size() == 1) {
    const QString uid = drawings.at(0).toObject().value(QStringLiteral("drawing")).toString();
    qDebug().noquote() << QStringLiteral("Workspace DRAWING: opened %1").arg(uid);
    openDrawing(uid);
    return;
  }
  // Several: the one to open, or a new one, from a menu under the selector.
  QMenu menu(this);
  menu.setObjectName(QStringLiteral("workspaceDrawingsMenu"));
  QStringList entries;
  for (const QJsonValue& value : std::as_const(drawings)) {
    const QJsonObject drawing = value.toObject();
    const QString uid = drawing.value(QStringLiteral("drawing")).toString();
    QString name = drawing.value(QStringLiteral("name")).toString();
    const QString component = drawing.value(QStringLiteral("component_name")).toString();
    if (!component.isEmpty() && drawing.value(QStringLiteral("component")).toString() != QLatin1String("C0")) {
      name = tr("%1 (%2)").arg(name, component);
    }
    QAction* open = menu.addAction(name);
    connect(open, &QAction::triggered, this, [this, uid] {
      qDebug().noquote() << QStringLiteral("Workspace DRAWING: opened %1").arg(uid);
      openDrawing(uid);
    });
    entries << name;
  }
  menu.addSeparator();
  QAction* create = menu.addAction(tr("Create Drawing…"));
  connect(create, &QAction::triggered, this, [this] { createDrawing(QStringLiteral("C0")); });
  // As the context menus log their entries (ui_menu_choose): those the
  // arrow keys reach, in order.
  entries << create->text();
  qDebug().noquote() << QStringLiteral("Context menu: %1").arg(entries.join(QStringLiteral(" | ")));
  const QWidget* selector = m_ribbon != nullptr ? m_ribbon->findChild<QWidget*>(QStringLiteral("ribbonWorkspace")) : nullptr;
  const QPoint at = selector != nullptr ? selector->mapToGlobal(QPoint(0, selector->height() + 4)) : QCursor::pos();
  execMenu(&menu, at);
}

} // namespace mitcad
