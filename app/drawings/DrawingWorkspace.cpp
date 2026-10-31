// SPDX-License-Identifier: MIT
#include "DrawingWorkspace.hpp"

#include <algorithm>
#include <exception>
#include <optional>
#include <tuple>
#include <utility>

#include <QAction>
#include <QApplication>
#include <QCheckBox>
#include <QSignalBlocker>
#include <QComboBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QDoubleSpinBox>
#include <QEvent>
#include <QFileInfo>
#include <QFormLayout>
#include <QFrame>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QMenu>
#include <QMouseEvent>
#include <QPushButton>
#include <QSizePolicy>
#include <QSpinBox>
#include <QTabBar>
#include <QTimer>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include "../OcctViewer.hpp"
#include "../browser/DocumentHost.hpp"
#include "../framework/DialogFolders.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Icons.hpp"
#include "../framework/Numbers.hpp"
#include "../framework/PlaceWatcher.hpp"
#include "../view/AnnotationDialogs.hpp"
#include "DrawingSheetView.hpp"

namespace mitcad::drawings {
namespace {

// The width of the panel beside the sheet, pixels.
constexpr int kPanelWidth = 230;

QString compact(const QJsonObject& object) {
  return QString::fromUtf8(QJsonDocument(object).toJson(QJsonDocument::Compact));
}

// The sheet sizes the model has, in its names, and their labels.
const QList<QPair<QString, QString>>& sheetSizes() {
  static const QList<QPair<QString, QString>> sizes = {
      {QStringLiteral("A4"), QStringLiteral("A4")},       {QStringLiteral("A3"), QStringLiteral("A3")},
      {QStringLiteral("A2"), QStringLiteral("A2")},       {QStringLiteral("A1"), QStringLiteral("A1")},
      {QStringLiteral("A0"), QStringLiteral("A0")},       {QStringLiteral("ANSI_A"), QStringLiteral("ANSI A")},
      {QStringLiteral("ANSI_B"), QStringLiteral("ANSI B")}, {QStringLiteral("ANSI_C"), QStringLiteral("ANSI C")},
      {QStringLiteral("ANSI_D"), QStringLiteral("ANSI D")}, {QStringLiteral("ANSI_E"), QStringLiteral("ANSI E")},
  };
  return sizes;
}

QComboBox* comboOf(const QList<QPair<QString, QString>>& choices, const QString& current) {
  auto* combo = new QComboBox;
  for (const auto& [value, label] : choices) {
    combo->addItem(label, value);
  }
  int index = combo->findData(current);
  if (index < 0 && !current.isEmpty()) {
    combo->addItem(current, current);
    index = combo->count() - 1;
  }
  combo->setCurrentIndex(std::max(0, index));
  return combo;
}

// Delete deletes; on macOS the delete key of the keyboard (Backspace) too.
bool isDeleteKey(int key) { return key == Qt::Key_Delete || key == Qt::Key_Backspace; }

// A key pressed without Ctrl, Alt, Shift or Meta (the keypad's flag aside).
bool unmodified(const QKeyEvent* key) {
  return !(key->modifiers() & (Qt::ShiftModifier | Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier));
}

// A revision's fields as its dialog edits them (mitcad#707 phase 4).
struct RevisionFields {
  QString description;
  QString date;
  QString author;
  bool remove = false;
};

// The revision dialog: its description (needed), date (today's when
// empty) and who made it; with `deletable` a Delete button too. None when
// cancelled.
std::optional<RevisionFields> askRevision(QWidget* parent, const QString& title, const RevisionFields& current,
                                          bool deletable) {
  QDialog dialog(parent);
  dialog.setWindowTitle(title);
  auto* form = new QFormLayout(&dialog);
  auto* description = new QLineEdit(current.description);
  description->setObjectName(QStringLiteral("drawingRevisionDescription"));
  form->addRow(QObject::tr("&Description"), description);
  auto* date = new QLineEdit(current.date);
  date->setPlaceholderText(QObject::tr("Today"));
  form->addRow(QObject::tr("Da&te"), date);
  auto* author = new QLineEdit(current.author);
  form->addRow(QObject::tr("&By"), author);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  bool removed = false;
  if (deletable) {
    QPushButton* deleteButton = buttons->addButton(QObject::tr("Delete"), QDialogButtonBox::DestructiveRole);
    QObject::connect(deleteButton, &QPushButton::clicked, &dialog, [&dialog, &removed] {
      removed = true;
      dialog.accept();
    });
  }
  // OK once it has a description.
  QPushButton* ok = buttons->button(QDialogButtonBox::Ok);
  ok->setEnabled(!description->text().trimmed().isEmpty());
  QObject::connect(description, &QLineEdit::textChanged, ok,
                   [ok](const QString& text) { ok->setEnabled(!text.trimmed().isEmpty()); });
  form->addRow(buttons);
  description->setFocus(Qt::OtherFocusReason);
  qDebug().noquote() << QStringLiteral("Drawing revision dialog opened: %1").arg(title);
  if (execDialog(&dialog) != QDialog::Accepted) {
    return std::nullopt;
  }
  RevisionFields out;
  out.description = description->text().trimmed();
  out.date = date->text().trimmed();
  out.author = author->text().trimmed();
  out.remove = removed;
  return out;
}

} // namespace

QList<TemplateChoice> availableTemplates(const DocumentHost& host) {
  QJsonObject query{{QStringLiteral("query"), QStringLiteral("drawing_templates")}};
  const QString project = host.projectFolder();
  if (!project.isEmpty()) {
    query.insert(QStringLiteral("project"), project);
  }
  QJsonObject answer;
  try {
    answer = host.model().queryObject(query);
  } catch (const std::exception& e) {
    qDebug().noquote() << QStringLiteral("Drawing templates not listed: %1").arg(QString::fromUtf8(e.what()));
  }
  for (const QJsonValue& problem : answer.value(QStringLiteral("problems")).toArray()) {
    qDebug().noquote() << QStringLiteral("Drawing template not read: %1").arg(problem.toString());
  }
  return templateChoices(answer);
}

DrawingWorkspace::DrawingWorkspace(OcctViewer* viewer, DocumentHost& host, std::function<QString()> title,
                                   std::function<QJsonObject()> camera,
                                   std::function<bool(const QKeySequence&)> modelShortcut)
    : QWidget(viewer), m_viewer(viewer), m_host(host), m_title(std::move(title)), m_camera(std::move(camera)),
      m_modelShortcut(std::move(modelShortcut)) {
  setObjectName(QStringLiteral("drawingWorkspace"));
  setAutoFillBackground(true);
  auto* column = new QVBoxLayout(this);
  column->setContentsMargins(0, 0, 0, 0);
  column->setSpacing(0);
  m_column = column;

  // The bar, two rows: Close Drawing, the drawing's name, the sheets,
  // Sheet… and Add Revision…; under them the tools in their groups, each
  // with its caption, and Export… last (mitcad#725). In one row the tools
  // left the sheets' tabs too little room once the annotations' tools
  // came: the tab bar was squeezed and a second tab went under its scroll
  // buttons. With the window's ribbon (setRibbonTools, mitcad#1008) the
  // first row alone, without Close Drawing: the ribbon's DRAWING tab has
  // it and the tools.
  m_bar = new QWidget;
  m_bar->setObjectName(QStringLiteral("drawingBar"));
  auto* rows = new QVBoxLayout(m_bar);
  rows->setContentsMargins(6, 4, 6, 4);
  rows->setSpacing(2);
  auto* bar = new QHBoxLayout;
  bar->setContentsMargins(0, 0, 0, 0);
  bar->setSpacing(4);
  rows->addLayout(bar);
  // The tools in a row of their own, which a cut sheet's row replaces
  // (mitcad#853).
  m_toolRow = new QWidget;
  auto* toolRow = new QHBoxLayout(m_toolRow);
  toolRow->setContentsMargins(0, 0, 0, 0);
  toolRow->setSpacing(4);
  rows->addWidget(m_toolRow);
  m_cutRow = new QWidget;
  auto* cutRow = new QHBoxLayout(m_cutRow);
  cutRow->setContentsMargins(0, 0, 0, 0);
  cutRow->setSpacing(4);
  rows->addWidget(m_cutRow);
  m_cutRow->hide();
  const auto button = [](const QString& icon, const QString& text, const QString& tooltip) {
    auto* b = new QToolButton;
    b->setIcon(themeIcon(icon));
    b->setText(text);
    b->setToolTip(tooltip);
    b->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    b->setAutoRaise(true);
    b->setFocusPolicy(Qt::NoFocus);
    return b;
  };
  m_close = button(QStringLiteral("view-arrow-left"), tr("Close Drawing"), tr("Shows the 3D model again"));
  connect(m_close, &QToolButton::clicked, this, [this] { closeDrawing(); });
  bar->addWidget(m_close);
  m_titleLabel = new QLabel;
  QFont bold = m_titleLabel->font();
  bold.setBold(true);
  m_titleLabel->setFont(bold);
  bar->addWidget(m_titleLabel);
  m_tabs = new QTabBar;
  m_tabs->setObjectName(QStringLiteral("drawingSheets"));
  m_tabs->setDocumentMode(true);
  m_tabs->setExpanding(false);
  m_tabs->setFocusPolicy(Qt::NoFocus);
  m_tabs->setContextMenuPolicy(Qt::CustomContextMenu);
  // As wide as its tabs at most, never stretched past them; narrower, with
  // its scroll buttons, only when the bar has no room for them, and not
  // under its minimum (a few tabs, `showSheet`), so that a few sheets'
  // tabs never go under the buttons (mitcad#725).
  m_tabs->setSizePolicy(QSizePolicy::Maximum, QSizePolicy::Fixed);
  // A sheet is shown on the press on its tab, seen by the workspace itself
  // (eventFilter), not only on the tab bar's currentChanged: on Windows a
  // click on a tab did not switch sheets (mitcad#725), and currentChanged
  // only comes when the bar's own current tab changes. The keyboard and the
  // wheel still come through currentChanged.
  m_tabs->installEventFilter(this);
  connect(m_tabs, &QTabBar::currentChanged, this, [this](int index) {
    if (!m_updatingTabs) {
      switchSheet(index);
    }
  });
  connect(m_tabs, &QTabBar::customContextMenuRequested, this,
          [this](const QPoint& at) { sheetMenu(m_tabs->tabAt(at), m_tabs->mapToGlobal(at)); });
  bar->addWidget(m_tabs);
  m_addSheet = button(QStringLiteral("add"), QString(), tr("Adds a sheet"));
  m_addSheet->setToolButtonStyle(Qt::ToolButtonIconOnly);
  connect(m_addSheet, &QToolButton::clicked, this, [this] { addSheet(); });
  bar->addWidget(m_addSheet);
  bar->addStretch(1);
  m_sheetButton = button(QStringLiteral("settings"), tr("Sheet…"),
                         tr("The sheet's size, projection, units, template and title block"));
  connect(m_sheetButton, &QToolButton::clicked, this, [this] { editSheet(); });
  bar->addWidget(m_sheetButton);
  m_addRevision = button(QStringLiteral("version-history"), tr("Add Revision…"),
                         tr("Adds a row to the drawing's revision table"));
  connect(m_addRevision, &QToolButton::clicked, this, [this] { addRevision(); });
  bar->addWidget(m_addRevision);

  // The tools by their groups, each group's buttons over its caption,
  // separated by lines (mitcad#725).
  QFont captionFont = font();
  captionFont.setPointSizeF(std::max(6.0, captionFont.pointSizeF() * 0.85));
  const auto group = [&captionFont, toolRow](const QString& caption, const QList<QWidget*>& buttons,
                                       QList<QLabel*>& captions) {
    if (!captions.isEmpty()) {
      auto* line = new QFrame;
      line->setFrameShape(QFrame::VLine);
      line->setFrameShadow(QFrame::Sunken);
      toolRow->addWidget(line);
    }
    auto* box = new QWidget;
    auto* stack = new QVBoxLayout(box);
    stack->setContentsMargins(0, 0, 0, 0);
    stack->setSpacing(0);
    auto* row = new QHBoxLayout;
    row->setContentsMargins(0, 0, 0, 0);
    row->setSpacing(2);
    for (QWidget* b : buttons) {
      row->addWidget(b);
    }
    stack->addLayout(row);
    auto* label = new QLabel(caption);
    label->setFont(captionFont);
    label->setAlignment(Qt::AlignHCenter);
    label->setEnabled(false); // the palette's quieter text
    stack->addWidget(label);
    toolRow->addWidget(box);
    captions.append(label);
  };
  for (int g = 0; g < kToolGroups; ++g) {
    QList<QWidget*> buttons;
    for (const DrawingToolDef& tool : drawingTools()) {
      if (toolGroup(tool) != g) {
        continue;
      }
      QToolButton* b = button(tool.icon, tool.name, tool.tooltip);
      if (tool.iconOnly) {
        // Their icons, their names in their tooltips.
        b->setToolButtonStyle(Qt::ToolButtonIconOnly);
        b->setToolTip(QStringLiteral("%1: %2").arg(tool.name, tool.tooltip));
      }
      b->setCheckable(true);
      const QString id = tool.id;
      connect(b, &QToolButton::clicked, this, [this, id] { startTool(id); });
      m_toolButtons.insert(tool.id, b);
      buttons.append(b);
    }
    group(toolGroupName(g), buttons, m_groupCaptions);
  }
  m_export = button(QStringLiteral("export"), tr("Export…"), tr("The drawing as PDF, SVG, DXF or DWG"));
  m_export->setPopupMode(QToolButton::InstantPopup);
  auto* formats = new QMenu(m_export);
  for (const auto& [format, label] : {std::pair{"pdf", "PDF…"}, std::pair{"svg", "SVG…"}, std::pair{"dxf", "DXF…"},
                                      std::pair{"dwg", "DWG…"}}) {
    const QString name = QString::fromLatin1(format);
    formats->addAction(tr(label), this, [this, name] { exportDrawing(name); });
  }
  m_export->setMenu(formats);
  group(tr("Export"), {m_export}, m_groupCaptions);
  toolRow->addStretch(1);
  // A cut sheet's row (mitcad#853): Add Parts… and its export as DXF or
  // SVG, with the parts' names on the layer LABEL or not.
  m_addParts = button(QStringLiteral("add"), tr("Add Parts…"),
                      tr("Adds bodies as flat parts, each seen along its largest planar face"));
  connect(m_addParts, &QToolButton::clicked, this, [this] { addParts(); });
  cutRow->addWidget(m_addParts);
  m_cutExport = button(QStringLiteral("export"), tr("Export…"), tr("The parts' outlines at 1:1 as DXF or SVG"));
  m_cutExport->setPopupMode(QToolButton::InstantPopup);
  auto* cutFormats = new QMenu(m_cutExport);
  cutFormats->addAction(tr("DXF…"), this, [this] { exportDrawing(QStringLiteral("dxf")); });
  cutFormats->addAction(tr("SVG…"), this, [this] { exportDrawing(QStringLiteral("svg")); });
  cutFormats->addSeparator();
  m_cutLabelsAction = cutFormats->addAction(tr("Part Names on the LABEL Layer"));
  m_cutLabelsAction->setCheckable(true);
  connect(m_cutLabelsAction, &QAction::toggled, this, [this](bool on) { setCutLabels(on); });
  m_cutExport->setMenu(cutFormats);
  cutRow->addWidget(m_cutExport);
  cutRow->addStretch(1);
  column->addWidget(m_bar);
  // The bar moves in the workspace (the rows over the view's top grew or
  // shrank, a card beside it): its places logged again.
  new PlaceWatcher(m_bar, [this] { logButtons(); });

  // The sheet and the panel beside it.
  auto* body = new QHBoxLayout;
  body->setContentsMargins(0, 0, 0, 0);
  body->setSpacing(0);
  m_sheetView = new DrawingSheetView;
  body->addWidget(m_sheetView, 1);
  m_panel = new QWidget;
  m_panel->setObjectName(QStringLiteral("drawingPanel"));
  m_panel->setFixedWidth(kPanelWidth);
  auto* panel = new QVBoxLayout(m_panel);
  panel->setContentsMargins(8, 8, 8, 8);
  m_panelTitle = new QLabel;
  m_panelTitle->setFont(bold);
  panel->addWidget(m_panelTitle);
  m_form = new QFormLayout;
  panel->addLayout(m_form);
  panel->addStretch(1);
  body->addWidget(m_panel);
  column->addLayout(body, 1);

  connect(m_sheetView, &DrawingSheetView::viewSelected, this, [this] { fillPanel(); });
  connect(m_sheetView, &DrawingSheetView::viewMoved, this, &DrawingWorkspace::viewMoved);
  connect(m_sheetView, &DrawingSheetView::picked, this, &DrawingWorkspace::picked);
  connect(m_sheetView, &DrawingSheetView::deleteRequested, this, &DrawingWorkspace::deleteView);
  connect(m_sheetView, &DrawingSheetView::pickRequested, this, &DrawingWorkspace::pickRequested);
  connect(m_sheetView, &DrawingSheetView::annotationSelected, this, [this] { fillPanel(); });
  connect(m_sheetView, &DrawingSheetView::annotationMoved, this, &DrawingWorkspace::annotationMoved);
  connect(m_sheetView, &DrawingSheetView::annotationDeleteRequested, this, &DrawingWorkspace::deleteAnnotation);
  connect(m_sheetView, &DrawingSheetView::detailCircleMoved, this, [this](int view, QPointF center) {
    run({{QStringLiteral("cmd"), QStringLiteral("edit_drawing_view")},
         {QStringLiteral("drawing"), m_drawing},
         {QStringLiteral("view"), view},
         {QStringLiteral("center"), QJsonArray{center.x(), center.y()}}});
  });
  connect(m_sheetView, &DrawingSheetView::toolCancelled, this, [this] { escape(); });
  connect(m_sheetView, &DrawingSheetView::placing, this, [this] {
    if (m_tool != nullptr) {
      m_sheetView->setHint(m_tool->placeHint);
    }
  });
  // mitcad#725: hover, several selected, the sheet's menu, a note's text.
  connect(m_sheetView, &DrawingSheetView::hoverRequested, this, &DrawingWorkspace::hoverRequested);
  connect(m_sheetView, &DrawingSheetView::selectionChanged, this, [this] { fillPanel(); });
  connect(m_sheetView, &DrawingSheetView::selectionMoved, this, &DrawingWorkspace::selectionMoved);
  connect(m_sheetView, &DrawingSheetView::deleteSelectionRequested, this, &DrawingWorkspace::deleteSelection);
  connect(m_sheetView, &DrawingSheetView::contextMenuRequested, this, &DrawingWorkspace::contextMenu);
  connect(m_sheetView, &DrawingSheetView::annotationDoubleClicked, this, [this](int id) {
    // After the click's events: the dialog has an event loop of its own.
    QTimer::singleShot(0, this, [this, id] { editNoteText(id); });
  });
  m_hoverTimer = new QTimer(this);
  m_hoverTimer->setSingleShot(true);
  m_hoverTimer->setInterval(60);
  connect(m_hoverTimer, &QTimer::timeout, this, &DrawingWorkspace::hoverPick);

  hide();
  viewer->installEventFilter(this);
  // The cards over the view may move: the workspace follows.
  connect(viewer, &OcctViewer::viewChanged, this, [this] {
    if (isVisible()) {
      place();
    }
  });
  // The cards at its sides move or change size (the browser opened or
  // folded): the drawing keeps beside them, its places logged again.
  connect(viewer, &OcctViewer::coveredChanged, this, [this] {
    if (isVisible()) {
      place();
    }
  });
}

bool DrawingWorkspace::open(const QString& uid) {
  const QString previous = m_drawing;
  m_drawing = uid;
  if (uid != previous) {
    m_sheet = 0;
    m_tool = nullptr;
    m_problems.clear();
  }
  place();
  show();
  raise();
  // The view's own controls (the home button, the cube's arrows) raise
  // themselves when the view's insets change: the drawing goes back above
  // them (mitcad#1008: the home button showed over Sheet…).
  for (QWidget* overlay : m_viewer->findChildren<QWidget*>(Qt::FindDirectChildrenOnly)) {
    if (overlay != this) {
      overlay->installEventFilter(this);
    }
  }
  refresh();
  if (!isOpen()) {
    return false;
  }
  // Another drawing, or one opened again, shows its whole sheet; the model's
  // later answers keep a zoom or a pan (mitcad#751).
  if (uid != previous) {
    m_sheetView->fit();
  }
  // Opened with nothing selected and no tool.
  m_sheetView->clearSelection();
  setTool(nullptr);
  m_sheetView->setFocus(Qt::OtherFocusReason);
  const int sheets = static_cast<int>(m_answer.value(QStringLiteral("sheets")).toArray().size());
  qDebug().noquote() << QStringLiteral("Drawing opened: %1 %2, sheet %3 of %4")
                            .arg(m_drawing, m_name)
                            .arg(m_sheet + 1)
                            .arg(sheets);
  QTimer::singleShot(0, this, [this] { logButtons(); });
  return true;
}

void DrawingWorkspace::closeDrawing() {
  if (!isOpen()) {
    hide();
    return;
  }
  m_drawing.clear();
  m_tool = nullptr;
  m_sheetView->setHint(QString());
  hide();
  qDebug().noquote() << QStringLiteral("Drawing closed");
  emit toolChanged(QString());
  emit closed();
}

void DrawingWorkspace::refresh() {
  if (!isOpen()) {
    return;
  }
  QJsonObject query{{QStringLiteral("query"), QStringLiteral("drawing")},
                    {QStringLiteral("drawing"), m_drawing},
                    {QStringLiteral("sheet"), m_sheet}};
  const QString title = m_title ? m_title() : QString();
  if (!title.isEmpty()) {
    query.insert(QStringLiteral("title"), title);
  }
  QJsonObject answer;
  try {
    answer = m_host.model().queryObject(query);
  } catch (const std::exception& e) {
    // A sheet that is gone (an undo): the first one; a drawing that is
    // gone: back to the model.
    if (m_sheet > 0) {
      m_sheet = 0;
      dropPicks();
      refresh();
      return;
    }
    qDebug().noquote() << QStringLiteral("Drawing %1 is gone: %2").arg(m_drawing, QString::fromUtf8(e.what()));
    closeDrawing();
    return;
  }
  showSheet(answer);
}

void DrawingWorkspace::showSheet(const QJsonObject& answer) {
  m_answer = answer;
  // A cut sheet (mitcad#853): its row instead of the tools, one sheet.
  const bool cut = answer.value(QStringLiteral("kind")).toString() == QStringLiteral("cut_sheet");
  if (cut != m_cutSheet) {
    m_cutSheet = cut;
    // The ribbon's tab shows them instead (setRibbonTools).
    m_toolRow->setVisible(!cut && !m_ribbonPlace);
    m_cutRow->setVisible(cut && !m_ribbonPlace);
    m_addSheet->setVisible(!cut);
    m_addRevision->setVisible(!cut);
    m_tabs->setVisible(!cut);
    m_sheetButton->setText(cut ? tr("Material…") : tr("Sheet…"));
    m_sheetButton->setToolTip(cut ? tr("The cut sheet's material: its width and height")
                                  : tr("The sheet's size, projection, units, template and title block"));
    // Said once for the cut sheet opened, "none" too.
    m_cutWarnings = QStringLiteral("\n");
    m_toolsWidth = -1;
    if (m_tool == nullptr) {
      showHint();
    }
    QTimer::singleShot(0, this, [this] { logButtons(); });
  }
  m_name = answer.value(QStringLiteral("name")).toString();
  const QString component = answer.value(QStringLiteral("component_name")).toString();
  m_titleLabel->setText(answer.value(QStringLiteral("component")).toString() == QStringLiteral("C0")
                            ? m_name
                            : QStringLiteral("%1 › %2").arg(component, m_name));
  const QJsonArray sheets = answer.value(QStringLiteral("sheets")).toArray();
  m_updatingTabs = true;
  while (m_tabs->count() > sheets.size()) {
    m_tabs->removeTab(m_tabs->count() - 1);
  }
  for (int i = 0; i < sheets.size(); ++i) {
    const QString name = sheets.at(i).toObject().value(QStringLiteral("name")).toString();
    if (i < m_tabs->count()) {
      m_tabs->setTabText(i, name);
    } else {
      m_tabs->addTab(name);
    }
  }
  m_tabs->setCurrentIndex(m_sheet);
  m_updatingTabs = false;
  // Its minimum: the first three tabs whole (all of them when fewer), so
  // that a few sheets never hide under the scroll buttons; many sheets
  // scroll instead of pushing the bar past the view (mitcad#725).
  int fewTabs = 0;
  for (int i = 0; i < std::min(3, static_cast<int>(m_tabs->count())); ++i) {
    fewTabs += m_tabs->tabRect(i).width();
  }
  m_tabs->setMinimumWidth(std::min(fewTabs, m_tabs->sizeHint().width()));
  const QJsonObject sheet = sheets.at(m_sheet).toObject();
  m_sheetView->setSheet(sheet);
  logTitleBlock(sheet);
  // The views the model could not draw, in the status bar when that
  // changes (their boxes show on the sheet).
  QStringList problems;
  // A cut sheet's warnings (overlaps, parts off the sheet, missing
  // bodies) say it all.
  if (m_cutSheet) {
    QStringList warnings;
    for (const QJsonValue& warning : sheet.value(QStringLiteral("warnings")).toArray()) {
      warnings << warning.toString();
    }
    const QString told = warnings.join(QStringLiteral("; "));
    if (told != m_cutWarnings) {
      m_cutWarnings = told;
      qDebug().noquote() << QStringLiteral("Cut sheet warnings: %1").arg(told.isEmpty() ? QStringLiteral("none") : told);
      setStatus(told, true);
    }
  }
  for (const QJsonValue& value : sheet.value(QStringLiteral("views")).toArray()) {
    if (m_cutSheet) {
      break;
    }
    const QJsonObject view = value.toObject();
    if (view.contains(QStringLiteral("error"))) {
      problems << tr("View %1: %2").arg(view.value(QStringLiteral("id")).toInt())
                      .arg(view.value(QStringLiteral("error")).toString());
    }
  }
  const QString said = problems.join(QStringLiteral("; "));
  if (said != m_problems) {
    m_problems = said;
    setStatus(said, true);
  }
  // After the event that changed the model: a field of the panel may have
  // changed it, and its row goes with the rebuild.
  QTimer::singleShot(0, this, [this] {
    if (isOpen()) {
      fillPanel();
    }
  });
}

void DrawingWorkspace::startTool(const QString& id) {
  const DrawingToolDef* tool = drawingTool(id);
  if (tool == nullptr || !isOpen()) {
    return;
  }
  if (m_cutSheet) {
    setStatus(tr("A cut sheet holds parts: Add Parts… adds them."), true);
    return;
  }
  setTool(m_tool == tool ? nullptr : tool);
}

void DrawingWorkspace::setTool(const DrawingToolDef* tool) {
  clearToolPicks();
  m_hoverTimer->stop();
  m_hoverView = -1;
  m_loggedHover.clear();
  m_tool = tool;
  for (auto it = m_toolButtons.begin(); it != m_toolButtons.end(); ++it) {
    it.value()->setChecked(tool != nullptr && it.key() == tool->id);
  }
  emit toolChanged(tool != nullptr ? tool->id : QString());
  m_sheetView->setPick(tool != nullptr ? tool->pick : SheetPick::None, tool != nullptr && tool->place);
  // A projected view comes from the view selected before, when there is one.
  if (tool != nullptr && tool->pick == SheetPick::OutOfView && m_sheetView->selectedView() != 0) {
    m_sheetView->setParentView(m_sheetView->selectedView());
  }
  qDebug().noquote() << QStringLiteral("Drawing tool: %1").arg(tool != nullptr ? tool->name : QStringLiteral("selection"));
  showHint();
  fillPanel();
}

void DrawingWorkspace::showHint() {
  if (m_tool == nullptr && m_cutSheet) {
    m_sheetView->setHint(tr("Add Parts… adds bodies; drag a part to move it, its menu turns it"));
    return;
  }
  m_sheetView->setHint(m_tool != nullptr ? m_tool->hint : QString());
}

void DrawingWorkspace::escape() {
  // Both: an annotation tool's picks are the workspace's, its placing
  // click the sheet's.
  const bool sheetPicks = m_sheetView->cancelPicks();
  const bool toolPicks = clearToolPicks();
  if (sheetPicks || toolPicks) {
    showHint(); // the tool's own hint again, not the placing one
  } else if (m_tool != nullptr) {
    setTool(nullptr);
  } else if (m_sheetView->selectionSize() > 0) {
    m_sheetView->clearSelection();
    qDebug().noquote() << QStringLiteral("Drawing selection cleared");
  } else {
    // Only Close Drawing closes it (the owner, mitcad#725): a stray Esc
    // never leaves the sheet.
    qDebug().noquote() << QStringLiteral("Drawing Esc: nothing to end");
  }
}

void DrawingWorkspace::fillPanel() {
  // The panel is built again: a field of it with the keyboard goes, so the
  // field of the same name gets it back, else the sheet, and Esc and
  // Delete still reach the drawing.
  QWidget* focus = QApplication::focusWidget();
  const bool inPanel = focus != nullptr && m_panel->isAncestorOf(focus);
  const QString focused = inPanel ? focus->objectName() : QString();
  if (inPanel) {
    // Without its signals: a text field that loses the keyboard would
    // send what was typed (editingFinished), and Esc would keep it.
    const QSignalBlocker block(focus);
    m_sheetView->setFocus(Qt::OtherFocusReason);
  }
  while (m_form->rowCount() > 0) {
    m_form->removeRow(0);
  }
  const auto refocus = [this, inPanel, focused] {
    if (!inPanel) {
      return;
    }
    QWidget* again = focused.isEmpty() ? nullptr : m_panel->findChild<QWidget*>(focused);
    if (again != nullptr) {
      again->setFocus(Qt::OtherFocusReason);
    } else {
      m_sheetView->setFocus(Qt::OtherFocusReason);
    }
  };
  if (m_cutSheet) {
    if (m_sheetView->selectionSize() <= 1 && m_sheetView->selectedView() != 0) {
      fillPartPanel(m_sheetView->selectedView());
    } else {
      fillCutSheetPanel();
    }
    refocus();
    QTimer::singleShot(0, this, [this] { logButtons(); });
    return;
  }
  if (m_tool != nullptr) {
    fillToolPanel(*m_tool);
  } else if (m_sheetView->selectionSize() > 1) {
    fillSelectionPanel();
  } else if (m_sheetView->selectedAnnotation() != 0) {
    fillAnnotationPanel(m_sheetView->selectedAnnotation());
  } else if (m_sheetView->selectedView() != 0) {
    fillViewPanel(m_sheetView->selectedView());
  } else {
    m_panelTitle->setText(tr("Sheet %1").arg(m_sheet + 1));
    const QJsonObject sheet = m_answer.value(QStringLiteral("sheets")).toArray().at(m_sheet).toObject();
    m_form->addRow(tr("Size"), new QLabel(sheet.value(QStringLiteral("size")).toString()));
    m_form->addRow(tr("Scale"), new QLabel(sheet.value(QStringLiteral("scale")).toString()));
    const QString sheetUnits = sheet.value(QStringLiteral("units")).toString();
    m_form->addRow(tr("Units"), new QLabel(sheetUnits.isEmpty() ? tr("The Design's") : sheetUnits));
    m_form->addRow(tr("Views"),
                   new QLabel(QString::number(sheet.value(QStringLiteral("views")).toArray().size())));
    const QJsonObject sheetTemplate = sheet.value(QStringLiteral("template")).toObject();
    m_form->addRow(tr("Template"), new QLabel(sheetTemplate.isEmpty()
                                                  ? tr("Simple Title Block")
                                                  : sheetTemplate.value(QStringLiteral("name")).toString()));
    // The drawing's decimals and letters' height (mitcad#729): every
    // dimension's and annotation's of its sheets without their own.
    const QJsonValue ownDecimals = m_answer.value(QStringLiteral("decimals"));
    QList<QPair<QString, QString>> decimalChoices = {{QStringLiteral("auto"), tr("Automatic")}};
    for (int d = 0; d <= 6; ++d) {
      decimalChoices.append({QString::number(d), QString::number(d)});
    }
    QComboBox* decimals =
        comboOf(decimalChoices, ownDecimals.isDouble() ? QString::number(ownDecimals.toInt()) : QStringLiteral("auto"));
    decimals->setObjectName(QStringLiteral("drawingDefaultDecimals"));
    connect(decimals, &QComboBox::currentIndexChanged, this, [this, decimals] {
      const QJsonObject command =
          drawingDefaultsEdit(m_drawing, QStringLiteral("decimals"), decimals->currentData().toString());
      if (!command.isEmpty()) {
        QTimer::singleShot(0, this, [this, command] { run(command); });
      }
    });
    m_form->addRow(tr("Decimals"), decimals);
    const QJsonValue ownHeight = m_answer.value(QStringLiteral("text_height"));
    const QString shownHeight = ownHeight.isDouble() ? QString::number(ownHeight.toDouble(), 'g', 6) : QString();
    auto* textHeight = new QLineEdit(shownHeight);
    textHeight->setObjectName(QStringLiteral("drawingDefaultTextHeight"));
    textHeight->setPlaceholderText(QStringLiteral("3.5"));
    textHeight->setToolTip(tr("The letters' height of the drawing's dimensions and annotations without their own, mm"));
    connect(textHeight, &QLineEdit::editingFinished, this, [this, textHeight, shownHeight] {
      const QString text = textHeight->text().trimmed();
      if (text == shownHeight) {
        return;
      }
      QString number;
      if (!text.isEmpty() && !parsePlainNumber(text, &number)) {
        setStatus(tr("A text height is a size in millimetres, not %1.").arg(text), true);
        textHeight->setText(shownHeight);
        return;
      }
      const QJsonObject command = drawingDefaultsEdit(m_drawing, QStringLiteral("text_height"), number);
      if (command.isEmpty()) {
        setStatus(tr("A text height is more than 0 and at most 100 mm."), true);
        textHeight->setText(shownHeight);
        return;
      }
      QTimer::singleShot(0, this, [this, command] { run(command); });
    });
    m_form->addRow(tr("Text Height (mm)"), textHeight);
    // The revision table (mitcad#707 phase 4): a row per revision, the
    // latest last; a click edits or deletes it.
    for (const QJsonValue& value : m_answer.value(QStringLiteral("revisions")).toArray()) {
      const QJsonObject revision = value.toObject();
      const QString letter = revision.value(QStringLiteral("letter")).toString();
      auto* row = new QToolButton;
      row->setObjectName(QStringLiteral("drawingRevision_") + letter);
      row->setText(revision.value(QStringLiteral("description")).toString());
      row->setToolTip(QStringLiteral("%1 %2").arg(revision.value(QStringLiteral("date")).toString(),
                                                  revision.value(QStringLiteral("author")).toString()));
      row->setAutoRaise(true);
      connect(row, &QToolButton::clicked, this, [this, letter] { editRevision(letter); });
      m_form->addRow(tr("Revision %1").arg(letter), row);
    }
    auto* add = new QPushButton(tr("Add Revision…"));
    add->setObjectName(QStringLiteral("drawingPanelAddRevision"));
    connect(add, &QPushButton::clicked, this, [this] { addRevision(); });
    m_form->addRow(QString(), add);
  }
  refocus();
  QTimer::singleShot(0, this, [this] { logButtons(); });
}

void DrawingWorkspace::fillToolPanel(const DrawingToolDef& tool) {
  m_panelTitle->setText(tool.name);
  const QString toolId = tool.id;
  for (const ToolOption& option : tool.options) {
    const QString id = option.id;
    const QString value = optionValue(tool, m_options.value(toolId), id);
    QWidget* field = nullptr;
    switch (option.kind) {
    case ToolOption::Kind::Choice: {
      // A base view's Shows: the drawing's component, its occurrences and
      // bodies by name (mitcad#725); a choice no longer there, the whole.
      QList<QPair<QString, QString>> choices = option.choices;
      QString chosen = value;
      if (toolId == QStringLiteral("drawing.base_view") && id == QStringLiteral("subject")) {
        choices = subjects();
        const bool listed = std::any_of(choices.cbegin(), choices.cend(),
                                        [&chosen](const QPair<QString, QString>& c) { return c.first == chosen; });
        if (!listed) {
          chosen = QStringLiteral("design");
          m_options[toolId].insert(id, chosen);
        }
      }
      QComboBox* combo = comboOf(choices, chosen);
      connect(combo, &QComboBox::currentIndexChanged, this, [this, combo, toolId, id] {
        m_options[toolId].insert(id, combo->currentData().toString());
        qDebug().noquote() << QStringLiteral("Drawing option %1: %2").arg(id, combo->currentData().toString());
      });
      field = combo;
      break;
    }
    case ToolOption::Kind::Check: {
      auto* check = new QCheckBox;
      check->setChecked(value == QStringLiteral("true"));
      connect(check, &QCheckBox::toggled, this, [this, toolId, id](bool on) {
        m_options[toolId].insert(id, on ? QStringLiteral("true") : QStringLiteral("false"));
        qDebug().noquote() << QStringLiteral("Drawing option %1: %2").arg(id, on ? QStringLiteral("on")
                                                                             : QStringLiteral("off"));
      });
      field = check;
      break;
    }
    case ToolOption::Kind::Text: {
      auto* edit = new QLineEdit(value);
      if (id == QStringLiteral("letter")) {
        edit->setPlaceholderText(tr("Next free"));
      }
      connect(edit, &QLineEdit::textChanged, this, [this, toolId, id](const QString& text) {
        m_options[toolId].insert(id, text);
        qDebug().noquote() << QStringLiteral("Drawing option %1: %2").arg(id, text);
      });
      field = edit;
      break;
    }
    }
    field->setObjectName(QStringLiteral("drawingOption_") + id);
    m_form->addRow(option.label, field);
  }
}

void DrawingWorkspace::editView(int view, const QString& field, const QString& value) {
  const SheetView* shown = m_sheetView->view(view);
  if (shown == nullptr) {
    return;
  }
  const QJsonObject command =
      viewEdit(m_drawing, view, field, value, shown->json.value(QStringLiteral("style")).toObject());
  if (command.isEmpty()) {
    setStatus(tr("%1 is not a size.").arg(value), true);
    return;
  }
  QTimer::singleShot(0, this, [this, command] { run(command); });
}

void DrawingWorkspace::fillViewPanel(int id) {
  const SheetView* view = m_sheetView->view(id);
  if (view == nullptr) {
    return;
  }
  const QJsonObject json = view->json;
  const QString type = view->type;
  const QString kind = type == QStringLiteral("base")        ? tr("Base View")
                       : type == QStringLiteral("projected") ? tr("Projected View")
                       : type == QStringLiteral("section")   ? tr("Section View")
                                                             : tr("Detail View");
  m_panelTitle->setText(tr("%1 %2").arg(kind).arg(id));
  const QJsonObject viewStyle = json.value(QStringLiteral("style")).toObject();
  const auto edit = [this, id](const QString& field, const QString& value) { editView(id, field, value); };
  const auto named = [](QWidget* widget, const char* name) {
    widget->setObjectName(QString::fromLatin1(name));
    return widget;
  };
  if (!view->error.isEmpty()) {
    auto* why = new QLabel(view->error);
    why->setWordWrap(true);
    why->setStyleSheet(QStringLiteral("color: #c92a2a;"));
    m_form->addRow(why);
  }
  if (type == QStringLiteral("base")) {
    const DrawingToolDef& base = *drawingTool(QStringLiteral("drawing.base_view"));
    const QJsonValue direction = json.value(QStringLiteral("direction"));
    QList<QPair<QString, QString>> directions;
    for (const ToolOption& option : base.options) {
      if (option.id == QStringLiteral("direction")) {
        directions = option.choices;
      }
    }
    // A given direction (Current View when it was placed) is not one of
    // the list's: shown as Given; Current View takes the 3D view's again.
    QComboBox* orientation = comboOf(directions, direction.isString() ? direction.toString() : QString());
    if (!direction.isString()) {
      orientation->addItem(tr("Given"), QString());
      orientation->setCurrentIndex(orientation->count() - 1);
    }
    connect(orientation, &QComboBox::currentIndexChanged, this, [this, id, orientation] {
      const QString value = orientation->currentData().toString();
      if (!value.isEmpty()) {
        setDirection(id, value);
      }
    });
    m_form->addRow(tr("Orientation"), named(orientation, "drawingViewOrientation"));
    // What it shows, by name (mitcad#725): the drawing's component, one of
    // its occurrences or one of its bodies; another subject (several
    // bodies) as it is.
    const QList<QPair<QString, QString>> shown = subjects();
    const QString current = subjectChoice(json.value(QStringLiteral("subject")).toObject());
    QComboBox* shows = comboOf(shown, current);
    if (current.isEmpty()) {
      shows->addItem(tr("Chosen Bodies"), QString());
      shows->setCurrentIndex(shows->count() - 1);
    }
    connect(shows, &QComboBox::currentIndexChanged, this, [edit, shows] {
      const QString value = shows->currentData().toString();
      if (value.isEmpty()) {
        return;
      }
      const QJsonObject subject = subjectOf(value);
      edit(QStringLiteral("subject"), subject.isEmpty() ? QStringLiteral(R"({"type": "design"})") : compact(subject));
    });
    m_form->addRow(tr("Shows"), named(shows, "drawingViewShows"));
  }
  if (type == QStringLiteral("projected")) {
    QComboBox* side = comboOf(sideChoices(), json.value(QStringLiteral("side")).toString());
    connect(side, &QComboBox::currentIndexChanged, this,
            [edit, side] { edit(QStringLiteral("side"), side->currentData().toString()); });
    m_form->addRow(tr("Side"), named(side, "drawingViewSide"));
  }
  if (type == QStringLiteral("section") || type == QStringLiteral("detail")) {
    auto* letter = new QLineEdit(json.value(QStringLiteral("letter")).toString());
    connect(letter, &QLineEdit::editingFinished, this, [edit, letter, json] {
      if (letter->text().trimmed() != json.value(QStringLiteral("letter")).toString()) {
        edit(QStringLiteral("letter"), letter->text().trimmed());
      }
    });
    m_form->addRow(tr("Letter"), named(letter, "drawingViewLetter"));
  }
  if (type == QStringLiteral("section")) {
    auto* hatch = new QCheckBox;
    hatch->setChecked(json.value(QStringLiteral("hatch")).toBool(true));
    connect(hatch, &QCheckBox::toggled, this,
            [edit](bool on) { edit(QStringLiteral("hatch"), on ? QStringLiteral("true") : QStringLiteral("false")); });
    m_form->addRow(tr("Hatching"), named(hatch, "drawingViewHatch"));
    auto* flip = new QPushButton(tr("Look the Other Way"));
    connect(flip, &QPushButton::clicked, this, [edit] { edit(QStringLiteral("flip"), QStringLiteral("true")); });
    m_form->addRow(QString(), named(flip, "drawingViewFlip"));
  }
  if (type == QStringLiteral("detail")) {
    // As the app's number fields take them: a point or a comma, shown
    // with a point (framework/Numbers.hpp); leaving the field unchanged
    // sends nothing.
    const double r = json.value(QStringLiteral("circle")).toObject().value(QStringLiteral("radius")).toDouble();
    const QString shown = QString::number(r, 'g', 6);
    auto* radius = new QLineEdit(shown);
    radius->setToolTip(tr("The circle's radius on the sheet, mm"));
    connect(radius, &QLineEdit::editingFinished, this, [this, edit, radius, shown] {
      const QString text = radius->text().trimmed();
      if (text == shown) {
        return;
      }
      QString number;
      const std::optional<double> value = parsePlainNumber(text, &number);
      if (!value || !(*value > 0.0)) {
        setStatus(tr("A detail's radius is a size in millimetres, not %1.").arg(text), true);
        radius->setText(shown);
        return;
      }
      edit(QStringLiteral("radius"), number);
    });
    m_form->addRow(tr("Radius (mm)"), named(radius, "drawingViewRadius"));
    // The circle's centre on the sheet; also moved by dragging the circle
    // while the detail is selected.
    const QJsonArray center = json.value(QStringLiteral("circle")).toObject().value(QStringLiteral("center")).toArray();
    for (int axis = 0; axis < 2; ++axis) {
      const QString shownAt = QString::number(center.at(axis).toDouble(), 'g', 6);
      auto* field = new QLineEdit(shownAt);
      field->setToolTip(tr("The circle's centre on the sheet, mm from its lower left corner"));
      const double other = center.at(1 - axis).toDouble();
      connect(field, &QLineEdit::editingFinished, this, [this, edit, field, shownAt, axis, other] {
        const QString text = field->text().trimmed();
        if (text == shownAt) {
          return;
        }
        QString number;
        const std::optional<double> value = parsePlainNumber(text, &number);
        if (!value) {
          setStatus(tr("A detail's centre is a place in millimetres, not %1.").arg(text), true);
          field->setText(shownAt);
          return;
        }
        const QString otherText = QString::number(other, 'g', 17);
        edit(QStringLiteral("center"), axis == 0 ? number + QLatin1Char(' ') + otherText
                                                 : otherText + QLatin1Char(' ') + number);
      });
      m_form->addRow(axis == 0 ? tr("Center X (mm)") : tr("Center Y (mm)"),
                     named(field, axis == 0 ? "drawingViewCenterX" : "drawingViewCenterY"));
    }
  }
  // The scale: a base view's fits the sheet when asked (its own scale
  // then; one without any draws at 1:1 and says so); another's follows its
  // parent's (twice it for a detail) unless it has its own.
  const bool base = type == QStringLiteral("base");
  const QString scale = json.value(QStringLiteral("own_scale")).toBool() || base
                            ? json.value(QStringLiteral("scale_text")).toString()
                            : QStringLiteral("auto");
  QComboBox* scales = comboOf(scaleChoices(base), scale);
  const QString automatic = json.value(QStringLiteral("automatic_scale")).toString();
  if (!base) {
    scales->setItemText(0, type == QStringLiteral("detail") ? tr("Twice Its Parent's (%1)").arg(automatic)
                                                            : tr("Its Parent's (%1)").arg(automatic));
  }
  connect(scales, &QComboBox::currentIndexChanged, this,
          [edit, scales] { edit(QStringLiteral("scale"), scales->currentData().toString()); });
  m_form->addRow(tr("Scale"), named(scales, "drawingViewScale"));
  // Turned on the sheet (mitcad#729), the views from it and its
  // annotations with it; a detail turns with its parent.
  if (type != QStringLiteral("detail")) {
    const QString shownTurn = QString::number(json.value(QStringLiteral("rotation")).toDouble(), 'g', 6);
    auto* rotation = new QLineEdit(shownTurn);
    rotation->setToolTip(tr("How far the view is turned on the sheet, degrees counter-clockwise"));
    connect(rotation, &QLineEdit::editingFinished, this, [this, edit, rotation, shownTurn] {
      const QString text = rotation->text().trimmed();
      if (text == shownTurn) {
        return;
      }
      QString number;
      if (!parsePlainNumber(text, &number)) {
        setStatus(tr("A rotation is a number of degrees, not %1.").arg(text), true);
        rotation->setText(shownTurn);
        return;
      }
      edit(QStringLiteral("rotation"), number);
    });
    m_form->addRow(tr("Rotation (°)"), named(rotation, "drawingViewRotation"));
  }
  for (const auto& [field, label] : {std::pair{"hidden_lines", "Hidden Edges"}, std::pair{"tangent_edges", "Tangent Edges"},
                                     std::pair{"shaded", "Shaded"}}) {
    const QString name = QString::fromLatin1(field);
    auto* check = new QCheckBox;
    check->setObjectName(QStringLiteral("drawingViewStyle_") + name);
    check->setChecked(viewStyle.value(name).toBool());
    connect(check, &QCheckBox::toggled, this,
            [edit, name](bool on) { edit(name, on ? QStringLiteral("true") : QStringLiteral("false")); });
    m_form->addRow(tr(label), check);
  }
  auto* label = new QLineEdit(json.value(QStringLiteral("label")).toString());
  connect(label, &QLineEdit::editingFinished, this, [edit, label, json] {
    if (label->text() != json.value(QStringLiteral("label")).toString()) {
      edit(QStringLiteral("label"), label->text());
    }
  });
  m_form->addRow(tr("Label"), named(label, "drawingViewLabel"));
  // What the model works out for the view: centre marks and centrelines
  // on its round edges and faces, the sketch dimensions it sees square on.
  for (const auto& [flag, caption, object] :
       {std::tuple{"center_marks", "Center Marks", "drawingViewCenterMarks"},
        std::tuple{"retrieved_dimensions", "Sketch Dimensions", "drawingViewRetrievedDimensions"}}) {
    const QString flagName = QString::fromLatin1(flag);
    auto* flagBox = new QCheckBox;
    flagBox->setChecked(json.value(flagName).toBool());
    connect(flagBox, &QCheckBox::toggled, this,
            [edit, flagName](bool on) { edit(flagName, on ? QStringLiteral("true") : QStringLiteral("false")); });
    m_form->addRow(tr(caption), named(flagBox, object));
  }
  if (type == QStringLiteral("projected") || type == QStringLiteral("section")) {
    auto* aligned = new QCheckBox;
    aligned->setChecked(!json.value(QStringLiteral("aligned")).isNull());
    connect(aligned, &QCheckBox::toggled, this,
            [edit](bool on) { edit(QStringLiteral("aligned"), on ? QStringLiteral("true") : QStringLiteral("false")); });
    m_form->addRow(tr("In Line with Its Parent"), named(aligned, "drawingViewAligned"));
  }
  if (!json.value(QStringLiteral("breaks")).toArray().isEmpty()) {
    auto* unbreak = new QPushButton(tr("Remove Breaks"));
    connect(unbreak, &QPushButton::clicked, this, [this, id] {
      const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("edit_drawing_view")},
                                {QStringLiteral("drawing"), m_drawing},
                                {QStringLiteral("view"), id},
                                {QStringLiteral("breaks"), QJsonArray()}};
      QTimer::singleShot(0, this, [this, command] { run(command); });
    });
    m_form->addRow(QString(), named(unbreak, "drawingViewRemoveBreaks"));
  }
  auto* remove = new QPushButton(tr("Delete View"));
  connect(remove, &QPushButton::clicked, this,
          [this, id] { QTimer::singleShot(0, this, [this, id] { deleteView(id); }); });
  m_form->addRow(QString(), named(remove, "drawingViewDelete"));
}

QList<QPair<QString, QString>> DrawingWorkspace::subjects() const {
  const QString component = m_answer.value(QStringLiteral("component")).toString();
  const bool root = component.isEmpty() || component == QStringLiteral("C0");
  // The root's drawings show the design: by the design's name.
  const QString title = m_title ? m_title() : QString();
  const QString name = root && !title.isEmpty() ? title : m_answer.value(QStringLiteral("component_name")).toString();
  QJsonObject components;
  QJsonArray bodies;
  try {
    components = m_host.model().queryObject({{QStringLiteral("query"), QStringLiteral("components")}});
    bodies = m_host.model().queryArray(QStringLiteral("bodies"));
  } catch (const std::exception& e) {
    qDebug().noquote() << QStringLiteral("Drawing subjects not listed: %1").arg(QString::fromUtf8(e.what()));
  }
  return subjectChoices(component, name, components, bodies);
}

QJsonObject DrawingWorkspace::cameraDirection() {
  const QJsonObject camera = m_camera ? m_camera() : QJsonObject();
  if (camera.isEmpty()) {
    setStatus(tr("The 3D view has no direction to take."), true);
  }
  return camera;
}

void DrawingWorkspace::setDirection(int view, const QString& direction) {
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("edit_drawing_view")},
                      {QStringLiteral("drawing"), m_drawing},
                      {QStringLiteral("view"), view}};
  if (direction == QStringLiteral("current")) {
    const QJsonObject camera = cameraDirection();
    if (camera.isEmpty()) {
      return;
    }
    command.insert(QStringLiteral("direction"), camera);
  } else {
    command.insert(QStringLiteral("direction"), direction);
  }
  QTimer::singleShot(0, this, [this, command] { run(command); });
}

void DrawingWorkspace::picked(ToolPicks picks) {
  if (m_tool == nullptr || m_tool->command == nullptr) {
    return;
  }
  QHash<QString, QString> options = m_options.value(m_tool->id);
  // Along the 3D view's Current View (mitcad#725): its camera now.
  if (m_tool->id == QStringLiteral("drawing.base_view") &&
      optionValue(*m_tool, options, QStringLiteral("direction")) == QStringLiteral("current")) {
    picks.camera = cameraDirection();
    if (picks.camera.isEmpty()) {
      return;
    }
  }
  // An annotation's tool: the model's picks the workspace found, on their
  // view; a dialog for what its options leave out. A line or an arc drawn
  // outside the model (mitcad#729) keeps its tool for the next one too.
  const bool annotating = m_tool->pick == SheetPick::Picks || m_tool->pick == SheetPick::Segment ||
                          m_tool->pick == SheetPick::ArcPoints || m_tool->id == QStringLiteral("drawing.text");
  if (m_tool->pick == SheetPick::Picks) {
    picks.view = m_pickView;
    picks.picks = m_toolPicks;
  }
  if (annotating && !askAnnotationFields(*m_tool, options)) {
    clearToolPicks();
    showHint();
    return;
  }
  const QJsonObject command = m_tool->command(m_drawing, m_sheet, picks, options);
  if (command.isEmpty()) {
    if (annotating) {
      clearToolPicks();
    }
    showHint();
    return;
  }
  QJsonObject result;
  const bool done = run(command, &result);
  if (annotating) {
    // The tool stays for the next one.
    clearToolPicks();
    showHint();
    return;
  }
  showHint();
  if (!done) {
    return;
  }
  const int added = result.value(QStringLiteral("view")).toInt();
  const DrawingToolDef* tool = m_tool;
  if (tool->id == QStringLiteral("drawing.base_view") && added != 0) {
    // Then views projected from it, as the views around a base view come.
    setTool(drawingTool(QStringLiteral("drawing.projected_view")));
    m_sheetView->setParentView(added);
  } else if (tool->pick != SheetPick::OutOfView) {
    // One section, detail, break or table at a time; a table selected,
    // its panel shown.
    setTool(nullptr);
    const int table = result.value(QStringLiteral("annotation")).toInt();
    if (table != 0) {
      m_sheetView->selectAnnotation(table);
    } else {
      m_sheetView->selectView(added != 0 ? added : picks.view);
    }
  }
}

bool DrawingWorkspace::clearToolPicks() {
  const bool had = !m_toolPicks.isEmpty();
  m_toolPicks = QJsonArray();
  m_pickItems = QJsonArray();
  m_pickPoints.clear();
  m_pickView = 0;
  if (had) {
    m_sheetView->setPickMarks(QJsonArray());
    m_sheetView->setPlacingPreview(QString(), {});
  }
  return had;
}

void DrawingWorkspace::showPlacingPreview() {
  if (m_tool == nullptr) {
    return;
  }
  // A linear dimension's lines from its two picks; any other placed
  // annotation (a radius, a note, a symbol, a balloon) on a leader.
  const QString type =
      m_tool->annotationType != nullptr ? m_tool->annotationType(m_options.value(m_tool->id)) : QString();
  const bool lines = type == QStringLiteral("linear") && m_pickPoints.size() >= 2;
  m_sheetView->setPlacingPreview(lines ? QStringLiteral("dimension") : QStringLiteral("leader"), m_pickPoints);
}

void DrawingWorkspace::hoverRequested(int view, QPointF at) {
  m_hoverView = view;
  m_hoverAt = at;
  if (view < 0) {
    m_hoverTimer->stop();
    hoverPick();
    return;
  }
  if (!m_hoverTimer->isActive()) {
    m_hoverTimer->start();
  }
}

void DrawingWorkspace::hoverPick() {
  QString seen = QStringLiteral("nothing");
  QJsonArray items;
  const int on = m_pickView != 0 ? m_pickView : m_hoverView;
  // Nothing while what was picked is placed (mitcad#725).
  if (m_hoverView >= 0 && on > 0 && m_tool != nullptr && m_tool->pick == SheetPick::Picks && isOpen() &&
      !m_sheetView->isPlacing()) {
    if (m_host.modelBusy()) {
      // A job computes the model (its timers still fire while it is
      // waited for): asked once it is done (mitcad#753), the marks kept
      // meanwhile.
      if (!m_hoverWaits) {
        m_hoverWaits = true;
        m_host.whenIdle(this, [this] {
          m_hoverWaits = false;
          hoverPick();
        });
      }
      return;
    }
    QJsonObject query{{QStringLiteral("query"), QStringLiteral("drawing_pick")},
                      {QStringLiteral("drawing"), m_drawing},
                      {QStringLiteral("sheet"), m_sheet},
                      {QStringLiteral("view"), on},
                      {QStringLiteral("at"), QJsonArray{m_hoverAt.x(), m_hoverAt.y()}}};
    if (m_tool->annotationType != nullptr) {
      query.insert(QStringLiteral("type"), m_tool->annotationType(m_options.value(m_tool->id)));
    }
    try {
      const QJsonObject answer = m_host.model().queryObject(query);
      if (answer.value(QStringLiteral("pick")).isObject()) {
        seen = answer.value(QStringLiteral("seen")).toString();
        items = answer.value(QStringLiteral("items")).toArray();
      }
    } catch (const std::exception&) {
      // Nothing to show: the click says why when it comes.
    }
  }
  m_sheetView->setHoverMarks(items);
  if (seen != m_loggedHover) {
    m_loggedHover = seen;
    qDebug().noquote() << QStringLiteral("Drawing hover: %1").arg(seen);
  }
}

void DrawingWorkspace::pickRequested(int view, QPointF at) {
  if (m_tool == nullptr || m_tool->pick != SheetPick::Picks) {
    return;
  }
  // The click is the pick: no hover's question comes after it.
  m_hoverTimer->stop();
  m_hoverView = -1;
  m_sheetView->setHoverMarks(QJsonArray());
  const QHash<QString, QString> options = m_options.value(m_tool->id);
  const int needed = m_tool->pickCount != nullptr ? m_tool->pickCount(options) : 1;
  // Every pick of one annotation is on the view of the first.
  const int on = m_pickView != 0 ? m_pickView : view;
  if (on == 0) {
    setStatus(tr("Click on a view."), true);
    return;
  }
  QJsonObject query{{QStringLiteral("query"), QStringLiteral("drawing_pick")},
                    {QStringLiteral("drawing"), m_drawing},
                    {QStringLiteral("sheet"), m_sheet},
                    {QStringLiteral("view"), on},
                    {QStringLiteral("at"), QJsonArray{at.x(), at.y()}}};
  if (m_tool->annotationType != nullptr) {
    query.insert(QStringLiteral("type"), m_tool->annotationType(options));
  }
  QJsonObject answer;
  try {
    answer = m_host.model().queryObject(query);
  } catch (const std::exception& e) {
    setStatus(QString::fromUtf8(e.what()), true);
    return;
  }
  const QJsonValue pick = answer.value(QStringLiteral("pick"));
  if (!pick.isObject()) {
    if (needed == kOpenPicks && m_toolPicks.size() >= 2) {
      // A free click places a series where it is.
      ToolPicks series;
      series.points = {at};
      picked(series);
      return;
    }
    qDebug().noquote() << QStringLiteral("Drawing pick: nothing at %1,%2").arg(at.x(), 0, 'f', 1).arg(at.y(), 0, 'f', 1);
    setStatus(tr("Nothing to pick there: click an edge, a circle or a corner of the view."), true);
    return;
  }
  m_pickView = on;
  m_toolPicks.append(pick);
  for (const QJsonValue& item : answer.value(QStringLiteral("items")).toArray()) {
    m_pickItems.append(item);
  }
  // Where it is on the sheet, for the preview while it is placed.
  const QJsonArray point = answer.value(QStringLiteral("at")).toArray();
  m_pickPoints.append(point.size() == 2 ? QPointF(point.at(0).toDouble(), point.at(1).toDouble()) : at);
  m_sheetView->setHoverMarks(QJsonArray());
  m_sheetView->setPickMarks(m_pickItems);
  qDebug().noquote() << QStringLiteral("Drawing picked %1 (%2): %3")
                            .arg(answer.value(QStringLiteral("seen")).toString())
                            .arg(m_toolPicks.size())
                            .arg(compact(pick.toObject()));
  if (needed != kOpenPicks && m_toolPicks.size() >= needed) {
    if (m_tool->place) {
      m_sheetView->startPlacing(); // its hint comes with `placing`
      showPlacingPreview();
    } else {
      picked(ToolPicks());
    }
  }
}

bool DrawingWorkspace::askAnnotationFields(const DrawingToolDef& tool, QHash<QString, QString>& options) {
  const QString what = m_pickView != 0 ? tr("View %1").arg(m_pickView) : tr("The sheet");
  if (tool.id == QStringLiteral("drawing.text") || tool.id == QStringLiteral("drawing.note")) {
    if (!optionValue(tool, options, QStringLiteral("text")).trimmed().isEmpty()) {
      return true;
    }
    const std::optional<QString> text = askNoteText(this, what);
    if (!text) {
      return false;
    }
    options.insert(QStringLiteral("text"), *text);
    return true;
  }
  if (tool.id == QStringLiteral("drawing.frame")) {
    const QStringList datums = datumLetters();
    QJsonArray characteristics;
    try {
      characteristics = m_host.model().queryArray(QStringLiteral("geometric_characteristics"));
    } catch (const std::exception& e) {
      setStatus(QString::fromUtf8(e.what()), true);
      return false;
    }
    const std::optional<QJsonObject> frame =
        askFeatureControlFrame(this, characteristics, QJsonObject(), datums, what);
    if (!frame) {
      return false;
    }
    options.insert(QStringLiteral("frame"), compact(*frame));
  }
  return true;
}

QStringList DrawingWorkspace::datumLetters() const {
  // The model's datum labels and every drawing's, as the query lists them
  // (mitcad#717: the model's were left out).
  QStringList datums;
  for (const QJsonValue& letter : m_answer.value(QStringLiteral("datum_letters")).toArray()) {
    datums << letter.toString();
  }
  return datums;
}

void DrawingWorkspace::annotationMoved(int annotation, QPointF delta) {
  run({{QStringLiteral("cmd"), QStringLiteral("edit_drawing_annotation")},
       {QStringLiteral("drawing"), m_drawing},
       {QStringLiteral("annotation"), annotation},
       {QStringLiteral("move"), QJsonArray{delta.x(), delta.y()}}});
}

void DrawingWorkspace::deleteAnnotation(int annotation) {
  if (run({{QStringLiteral("cmd"), QStringLiteral("delete_drawing_annotation")},
           {QStringLiteral("drawing"), m_drawing},
           {QStringLiteral("annotation"), annotation}})) {
    m_sheetView->selectAnnotation(0);
  }
}

void DrawingWorkspace::editAnnotation(const QJsonObject& command) {
  if (command.isEmpty()) {
    return;
  }
  QTimer::singleShot(0, this, [this, command] { run(command); });
}

void DrawingWorkspace::fillAnnotationPanel(int id) {
  const SheetAnnotation* shown = m_sheetView->annotation(id);
  if (shown == nullptr) {
    return;
  }
  const QJsonObject json = shown->json;
  const QString type = shown->type;
  m_panelTitle->setText(tr("%1 %2").arg(annotationName(type)).arg(id));
  const auto named = [](QWidget* widget, const char* name) {
    widget->setObjectName(QString::fromLatin1(name));
    return widget;
  };
  const auto field = [this, id](const QString& name, const QString& value, const QJsonObject& finish) {
    editAnnotation(annotationEdit(m_drawing, id, name, value, finish));
  };
  if (shown->broken) {
    auto* why = new QLabel(shown->error);
    why->setWordWrap(true);
    why->setStyleSheet(QStringLiteral("color: #c92a2a;"));
    m_form->addRow(why);
  } else if (!shown->label.isEmpty()) {
    auto* value = new QLabel(shown->label);
    value->setWordWrap(true);
    m_form->addRow(tr("Shows"), named(value, "drawingAnnotationValue"));
  }
  const QString what = tr("%1 %2").arg(annotationName(type)).arg(id);
  const bool dimension = type == QStringLiteral("linear") || type == QStringLiteral("angle") ||
                         type == QStringLiteral("radius") || type == QStringLiteral("diameter") ||
                         type == QStringLiteral("ordinate");
  // A text field that sends its text when it is left changed.
  const auto line = [this, field](const QString& name, const QString& text, const QString& label, const char* object) {
    auto* edit = new QLineEdit(text);
    connect(edit, &QLineEdit::editingFinished, this, [field, edit, name, text] {
      if (edit->text() != text) {
        field(name, edit->text(), QJsonObject());
      }
    });
    edit->setObjectName(QString::fromLatin1(object));
    m_form->addRow(label, edit);
  };
  if (type == QStringLiteral("linear") || type == QStringLiteral("ordinate")) {
    QList<QPair<QString, QString>> directions = {{QStringLiteral("horizontal"), tr("Horizontal")},
                                                 {QStringLiteral("vertical"), tr("Vertical")}};
    if (type == QStringLiteral("linear")) {
      directions.append({QStringLiteral("aligned"), tr("Aligned")});
    }
    QComboBox* along = comboOf(directions, json.value(QStringLiteral("along")).toString());
    connect(along, &QComboBox::currentIndexChanged, this,
            [field, along] { field(QStringLiteral("along"), along->currentData().toString(), QJsonObject()); });
    m_form->addRow(tr("Direction"), named(along, "drawingAnnotationAlong"));
  }
  if (dimension) {
    const QJsonObject format = json.value(QStringLiteral("format")).toObject();
    // #702's tolerance and decimals, in its dialogs.
    const QJsonValue current = json.value(QStringLiteral("tolerance"));
    auto* tolerance = new QPushButton(current.isObject() ? tr("Tolerance…") : tr("Add Tolerance…"));
    const bool angle = type == QStringLiteral("angle");
    connect(tolerance, &QPushButton::clicked, this, [this, id, current, angle, what] {
      const std::optional<QJsonValue> chosen = askAnnotationTolerance(this, current, angle, what);
      if (chosen && *chosen != current) {
        editAnnotation({{QStringLiteral("cmd"), QStringLiteral("edit_drawing_annotation")},
                        {QStringLiteral("drawing"), m_drawing},
                        {QStringLiteral("annotation"), id},
                        {QStringLiteral("tolerance"), *chosen}});
      }
    });
    m_form->addRow(tr("Tolerance"), named(tolerance, "drawingAnnotationTolerance"));
    const QJsonValue decimals = format.value(QStringLiteral("decimals"));
    auto* places = new QPushButton(decimals.isDouble() ? QString::number(decimals.toInt()) : tr("Automatic"));
    connect(places, &QPushButton::clicked, this, [this, id, decimals, what] {
      const std::optional<QJsonValue> chosen = askAnnotationDecimals(this, decimals, what);
      if (chosen && *chosen != decimals) {
        editAnnotation({{QStringLiteral("cmd"), QStringLiteral("edit_drawing_annotation")},
                        {QStringLiteral("drawing"), m_drawing},
                        {QStringLiteral("annotation"), id},
                        {QStringLiteral("format"), QJsonObject{{QStringLiteral("decimals"), *chosen}}}});
      }
    });
    m_form->addRow(tr("Decimals"), named(places, "drawingAnnotationDecimals"));
    line(QStringLiteral("before"), format.value(QStringLiteral("before")).toString(), tr("Text Before"),
         "drawingAnnotationBefore");
    line(QStringLiteral("after"), format.value(QStringLiteral("after")).toString(), tr("Text After"),
         "drawingAnnotationAfter");
  }
  if (type == QStringLiteral("note") || type == QStringLiteral("text")) {
    auto* change = new QPushButton(tr("Text…"));
    connect(change, &QPushButton::clicked, this, [this, id] { editNoteText(id); });
    m_form->addRow(tr("Text"), named(change, "drawingAnnotationText"));
  }
  if (type == QStringLiteral("datum")) {
    line(QStringLiteral("letter"), json.value(QStringLiteral("letter")).toString(), tr("Letter"),
         "drawingAnnotationLetter");
  }
  if (type == QStringLiteral("frame")) {
    const QJsonObject current = json.value(QStringLiteral("frame")).toObject();
    auto* change = new QPushButton(tr("Tolerance…"));
    connect(change, &QPushButton::clicked, this, [this, id, current, what] {
      QJsonArray characteristics;
      try {
        characteristics = m_host.model().queryArray(QStringLiteral("geometric_characteristics"));
      } catch (const std::exception& e) {
        setStatus(QString::fromUtf8(e.what()), true);
        return;
      }
      const std::optional<QJsonObject> chosen =
          askFeatureControlFrame(this, characteristics, current, datumLetters(), what);
      if (chosen && *chosen != current) {
        editAnnotation({{QStringLiteral("cmd"), QStringLiteral("edit_drawing_annotation")},
                        {QStringLiteral("drawing"), m_drawing},
                        {QStringLiteral("annotation"), id},
                        {QStringLiteral("frame"), *chosen}});
      }
    });
    m_form->addRow(tr("Tolerance"), named(change, "drawingAnnotationFrame"));
  }
  if (type == QStringLiteral("finish")) {
    const QJsonObject finish = json.value(QStringLiteral("finish")).toObject();
    QComboBox* process = comboOf({{QStringLiteral("any"), tr("Any Process")},
                                  {QStringLiteral("removal"), tr("Material Removed")},
                                  {QStringLiteral("no_removal"), tr("No Material Removed")}},
                                 finish.value(QStringLiteral("process")).toString(QStringLiteral("any")));
    connect(process, &QComboBox::currentIndexChanged, this, [field, process, finish] {
      field(QStringLiteral("process"), process->currentData().toString(), finish);
    });
    m_form->addRow(tr("Surface"), named(process, "drawingAnnotationProcess"));
    const QString requirement = finish.value(QStringLiteral("text")).toString();
    auto* text = new QLineEdit(requirement);
    connect(text, &QLineEdit::editingFinished, this, [field, text, requirement, finish] {
      if (text->text() != requirement) {
        field(QStringLiteral("finish_text"), text->text(), finish);
      }
    });
    m_form->addRow(tr("Requirement"), named(text, "drawingAnnotationRequirement"));
  }
  if (type == QStringLiteral("hole_table") || type == QStringLiteral("parts_list")) {
    fillTablePanel(id, json);
  }
  // A line's or an arc's pen (mitcad#729).
  if (type == QStringLiteral("line") || type == QStringLiteral("arc")) {
    QComboBox* pen = comboOf(penChoices(), json.value(QStringLiteral("pen")).toString(QStringLiteral("thin")));
    connect(pen, &QComboBox::currentIndexChanged, this,
            [field, pen] { field(QStringLiteral("pen"), pen->currentData().toString(), QJsonObject()); });
    m_form->addRow(tr("Line"), named(pen, "drawingAnnotationPen"));
  }
  // An edge extension of one edge: how far it runs on (mitcad#729).
  if (type == QStringLiteral("edge_extension") && json.value(QStringLiteral("picks")).toArray().size() == 1) {
    const QJsonValue reach = json.value(QStringLiteral("line_offset"));
    line(QStringLiteral("length"), reach.isDouble() ? QString::number(reach.toDouble(), 'g', 6) : QString(),
         tr("Length (mm)"), "drawingAnnotationLength");
  }
  // Its own letters' height over the drawing's (mitcad#729): what writes.
  const bool writes = type != QStringLiteral("line") && type != QStringLiteral("arc") &&
                      type != QStringLiteral("center_mark") && type != QStringLiteral("centerline") &&
                      type != QStringLiteral("center_pattern") && type != QStringLiteral("edge_extension");
  if (writes) {
    const QJsonValue own = json.value(QStringLiteral("height"));
    const QString shownHeight = own.isDouble() ? QString::number(own.toDouble(), 'g', 6) : QString();
    auto* height = new QLineEdit(shownHeight);
    height->setPlaceholderText(tr("The drawing's"));
    connect(height, &QLineEdit::editingFinished, this, [this, field, height, shownHeight] {
      const QString text = height->text().trimmed();
      if (text == shownHeight) {
        return;
      }
      QString number;
      if (!text.isEmpty() && !parsePlainNumber(text, &number)) {
        setStatus(tr("A text height is a size in millimetres, not %1.").arg(text), true);
        height->setText(shownHeight);
        return;
      }
      field(QStringLiteral("height"), number, QJsonObject());
    });
    m_form->addRow(tr("Text Height (mm)"), named(height, "drawingAnnotationHeight"));
  }
  auto* remove = new QPushButton(tr("Delete"));
  connect(remove, &QPushButton::clicked, this,
          [this, id] { QTimer::singleShot(0, this, [this, id] { deleteAnnotation(id); }); });
  m_form->addRow(QString(), named(remove, "drawingAnnotationDelete"));
}

void DrawingWorkspace::fillTablePanel(int id, const QJsonObject& json) {
  const bool holes = json.value(QStringLiteral("type")).toString() == QStringLiteral("hole_table");
  const QJsonObject table = json.value(QStringLiteral("table")).toObject();
  const auto field = [this, id](const QString& name, const QString& value) {
    editAnnotation(annotationEdit(m_drawing, id, name, value));
  };
  QStringList shown;
  for (const QJsonValue& column : json.value(QStringLiteral("columns")).toArray()) {
    shown << column.toString();
  }
  // What it can show: its type's own columns (and a parts list's property
  // columns), each a check box, in that order.
  QList<QPair<QString, QString>> available;
  for (const QJsonValue& value : json.value(QStringLiteral("available")).toArray()) {
    const QJsonObject column = value.toObject();
    available.append({column.value(QStringLiteral("column")).toString(), column.value(QStringLiteral("header")).toString()});
  }
  bool first = true;
  for (const QPair<QString, QString>& entry : std::as_const(available)) {
    auto* box = new QCheckBox(entry.second);
    box->setObjectName(QStringLiteral("drawingTableColumn_") + entry.first);
    box->setChecked(shown.contains(entry.first));
    const QString key = entry.first;
    connect(box, &QCheckBox::toggled, this, [this, field, shown, available, key, box](bool on) {
      QStringList next;
      for (const QPair<QString, QString>& other : available) {
        if (other.first == key ? on : shown.contains(other.first)) {
          next << other.first;
        }
      }
      if (next.isEmpty()) {
        const QSignalBlocker block(box);
        box->setChecked(true);
        setStatus(tr("A table shows one column at least."), true);
        return;
      }
      field(QStringLiteral("columns"), next.join(QLatin1Char(',')));
    });
    m_form->addRow(first ? tr("Columns") : QString(), box);
    first = false;
  }
  if (!holes) {
    // Property columns of the components (`Key` or `Set/Key`): the
    // columns that are not the list's own.
    const QStringList listOwn = {QStringLiteral("item"),        QStringLiteral("quantity"),
                                 QStringLiteral("name"),        QStringLiteral("part_number"),
                                 QStringLiteral("description"), QStringLiteral("material")};
    QStringList own;
    QStringList properties;
    for (const QString& column : std::as_const(shown)) {
      (listOwn.contains(column) ? own : properties) << column;
    }
    auto* more = new QLineEdit(properties.join(QStringLiteral(", ")));
    more->setObjectName(QStringLiteral("drawingTableMoreColumns"));
    more->setPlaceholderText(tr("Property names"));
    const QString before = more->text();
    connect(more, &QLineEdit::editingFinished, this, [field, more, own, before] {
      if (more->text() == before) {
        return;
      }
      QStringList next = own;
      for (const QString& key : more->text().split(QLatin1Char(','), Qt::SkipEmptyParts)) {
        if (!key.trimmed().isEmpty() && !next.contains(key.trimmed())) {
          next << key.trimmed();
        }
      }
      field(QStringLiteral("columns"), next.join(QLatin1Char(',')));
    });
    m_form->addRow(tr("More Columns"), more);
  }
  // The sort: by a column, either way.
  QComboBox* sort = comboOf(available, json.value(QStringLiteral("sort")).toString());
  sort->setObjectName(QStringLiteral("drawingTableSort"));
  connect(sort, &QComboBox::currentIndexChanged, this,
          [field, sort] { field(QStringLiteral("sort"), sort->currentData().toString()); });
  m_form->addRow(tr("Sort By"), sort);
  auto* descending = new QCheckBox;
  descending->setObjectName(QStringLiteral("drawingTableDescending"));
  descending->setChecked(table.value(QStringLiteral("descending")).toBool());
  connect(descending, &QCheckBox::toggled, this, [field](bool on) {
    field(QStringLiteral("descending"), on ? QStringLiteral("true") : QStringLiteral("false"));
  });
  m_form->addRow(tr("Descending"), descending);
  if (holes) {
    // Its origin: a point of the model it was given, or one of its own.
    const bool hasPick = !json.value(QStringLiteral("picks")).toArray().isEmpty();
    QList<QPair<QString, QString>> origins = {{QStringLiteral("corner"), tr("Lower Left Corner")},
                                              {QStringLiteral("model"), tr("Component Origin")}};
    if (hasPick) {
      origins.append({QStringLiteral("pick"), tr("A Point of the Model")});
    }
    QComboBox* origin = comboOf(origins, hasPick ? QStringLiteral("pick")
                                                 : table.value(QStringLiteral("origin")).toString(QStringLiteral("corner")));
    origin->setObjectName(QStringLiteral("drawingTableOrigin"));
    connect(origin, &QComboBox::currentIndexChanged, this, [this, id, origin] {
      const QString value = origin->currentData().toString();
      if (value == QStringLiteral("pick")) {
        return;
      }
      // Its own origin: the point given goes.
      editAnnotation({{QStringLiteral("cmd"), QStringLiteral("edit_drawing_annotation")},
                      {QStringLiteral("drawing"), m_drawing},
                      {QStringLiteral("annotation"), id},
                      {QStringLiteral("picks"), QJsonArray()},
                      {QStringLiteral("table"), QJsonObject{{QStringLiteral("origin"), value}}}});
    });
    m_form->addRow(tr("Origin"), origin);
    QComboBox* tags =
        comboOf({{QStringLiteral("by_size"), tr("By Size")}, {QStringLiteral("numbers"), tr("Numbers")}},
                table.value(QStringLiteral("tags")).toString(QStringLiteral("by_size")));
    tags->setObjectName(QStringLiteral("drawingTableTags"));
    connect(tags, &QComboBox::currentIndexChanged, this,
            [field, tags] { field(QStringLiteral("tags"), tags->currentData().toString()); });
    m_form->addRow(tr("Tags"), tags);
  } else {
    QComboBox* levels =
        comboOf({{QStringLiteral("top"), tr("First Level")}, {QStringLiteral("parts"), tr("Parts Only")}},
                table.value(QStringLiteral("levels")).toString(QStringLiteral("top")));
    levels->setObjectName(QStringLiteral("drawingTableLevels"));
    connect(levels, &QComboBox::currentIndexChanged, this,
            [field, levels] { field(QStringLiteral("levels"), levels->currentData().toString()); });
    m_form->addRow(tr("Rows"), levels);
  }
  const QString title = table.value(QStringLiteral("title")).toString();
  auto* heading = new QLineEdit(title);
  heading->setObjectName(QStringLiteral("drawingTableTitle"));
  heading->setPlaceholderText(holes ? tr("HOLES") : tr("PARTS LIST"));
  connect(heading, &QLineEdit::editingFinished, this, [field, heading, title] {
    if (heading->text() != title) {
      field(QStringLiteral("title"), heading->text());
    }
  });
  m_form->addRow(tr("Title"), heading);
}

void DrawingWorkspace::viewMoved(int view, QPointF delta) {
  run({{QStringLiteral("cmd"), QStringLiteral("edit_drawing_view")},
       {QStringLiteral("drawing"), m_drawing},
       {QStringLiteral("view"), view},
       {QStringLiteral("move"), QJsonArray{delta.x(), delta.y()}}});
}

void DrawingWorkspace::deleteView(int view) {
  QJsonObject result;
  if (run({{QStringLiteral("cmd"), QStringLiteral("delete_drawing_view")},
           {QStringLiteral("drawing"), m_drawing},
           {QStringLiteral("view"), view}},
          &result)) {
    m_sheetView->selectView(0);
  }
}

void DrawingWorkspace::editNoteText(int id) {
  const SheetAnnotation* shown = m_sheetView->annotation(id);
  if (shown == nullptr || (shown->type != QStringLiteral("note") && shown->type != QStringLiteral("text"))) {
    return;
  }
  const QString text = shown->json.value(QStringLiteral("text")).toString();
  const std::optional<QString> chosen = askNoteText(this, tr("%1 %2").arg(annotationName(shown->type)).arg(id), text);
  if (chosen && *chosen != text) {
    editAnnotation(annotationEdit(m_drawing, id, QStringLiteral("text"), *chosen));
  }
}

void DrawingWorkspace::switchSheet(int index) {
  if (!isOpen() || index < 0 || index >= m_tabs->count() || index == m_sheet) {
    return;
  }
  m_sheet = index;
  // An annotation tool's picks, a half-drawn line or circle and a placing
  // click awaited were on the sheet left (mitcad#759); the tool stays.
  dropPicks();
  qDebug().noquote() << QStringLiteral("Drawing sheet %1 shown").arg(index + 1);
  refresh();
}

bool DrawingWorkspace::isModelShortcut(const QKeyEvent* key) const {
  const int code = key->key();
  if (!m_modelShortcut || code == 0 || code == Qt::Key_unknown || code == Qt::Key_Shift ||
      code == Qt::Key_Control || code == Qt::Key_Alt || code == Qt::Key_Meta || isDeleteKey(code) ||
      code == Qt::Key_Escape) {
    return false;
  }
  Qt::KeyboardModifiers modifiers = key->modifiers();
  modifiers.setFlag(Qt::KeypadModifier, false);
  return m_modelShortcut(QKeySequence(QKeyCombination(modifiers, static_cast<Qt::Key>(code))));
}

void DrawingWorkspace::fillSelectionPanel() {
  const QList<int> views = m_sheetView->selectedViews();
  const QList<int> notes = m_sheetView->selectedAnnotations();
  m_panelTitle->setText(tr("%1 Items Selected").arg(views.size() + notes.size()));
  QStringList names;
  for (const int view : views) {
    names << tr("View %1").arg(view);
  }
  for (const int note : notes) {
    const SheetAnnotation* shown = m_sheetView->annotation(note);
    names << tr("%1 %2").arg(shown != nullptr ? annotationName(shown->type) : tr("Annotation")).arg(note);
  }
  auto* list = new QLabel(names.join(QStringLiteral(", ")));
  list->setWordWrap(true);
  m_form->addRow(list);
  auto* remove = new QPushButton(tr("Delete"));
  remove->setObjectName(QStringLiteral("drawingSelectionDelete"));
  connect(remove, &QPushButton::clicked, this, [this] { QTimer::singleShot(0, this, [this] { deleteSelection(); }); });
  m_form->addRow(QString(), remove);
}

void DrawingWorkspace::deleteSelection() {
  const QList<int> notes = m_sheetView->selectedAnnotations();
  QList<int> views = m_sheetView->selectedViews();
  if (notes.isEmpty() && views.isEmpty()) {
    return;
  }
  // The annotations first, then the views, the later ones first: a view
  // takes the views from it and their annotations along, which have later
  // ids, so nothing is deleted twice.
  QJsonArray commands;
  if (!notes.isEmpty()) {
    QJsonArray ids;
    for (const int note : notes) {
      ids.append(note);
    }
    commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("delete_drawing_annotation")},
                                {QStringLiteral("drawing"), m_drawing},
                                {QStringLiteral("annotations"), ids}});
  }
  std::sort(views.begin(), views.end(), std::greater<int>());
  for (const int view : std::as_const(views)) {
    // A view from another one selected goes with it.
    bool fromSelected = false;
    for (const SheetView* v = m_sheetView->view(view); v != nullptr && v->parent != 0; v = m_sheetView->view(v->parent)) {
      fromSelected = fromSelected || views.contains(v->parent);
    }
    if (!fromSelected) {
      commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("delete_drawing_view")},
                                  {QStringLiteral("drawing"), m_drawing},
                                  {QStringLiteral("view"), view}});
    }
  }
  if (!m_host.canChangeModel()) {
    setStatus(m_host.changeBlockedReason(), true);
    return;
  }
  qDebug().noquote() << QStringLiteral("Drawing command: %1")
                            .arg(QString::fromUtf8(QJsonDocument(commands).toJson(QJsonDocument::Compact)));
  if (m_host.runModelCommands(commands, tr("Delete %1 items of %2").arg(notes.size() + views.size()).arg(m_name))) {
    m_sheetView->clearSelection();
  } else if (!m_host.lastCancelled()) {
    qDebug().noquote() << QStringLiteral("Drawing command refused: %1").arg(m_host.lastError());
    setStatus(m_host.lastError(), true);
  }
}

void DrawingWorkspace::selectionMoved(QPointF delta) {
  const QList<int> views = m_sheetView->selectedViews();
  // A view moves the views from it and its annotations along: those are
  // not moved again.
  const auto movedAlong = [this, &views](int view) {
    for (const SheetView* v = m_sheetView->view(view); v != nullptr; v = m_sheetView->view(v->parent)) {
      if (views.contains(v->id)) {
        return true;
      }
      if (v->parent == 0) {
        break;
      }
    }
    return false;
  };
  QJsonArray commands;
  for (const int view : views) {
    const SheetView* v = m_sheetView->view(view);
    if (v != nullptr && v->parent != 0 && movedAlong(v->parent)) {
      continue;
    }
    commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("edit_drawing_view")},
                                {QStringLiteral("drawing"), m_drawing},
                                {QStringLiteral("view"), view},
                                {QStringLiteral("move"), QJsonArray{delta.x(), delta.y()}}});
  }
  for (const int note : m_sheetView->selectedAnnotations()) {
    const SheetAnnotation* a = m_sheetView->annotation(note);
    // A dimension, a leader or a symbol is placed from its picks and goes
    // with its view; a hole table or a parts list stands on the sheet
    // whatever its view's place, and is moved itself (mitcad#752).
    const bool table = a != nullptr && (a->type == QStringLiteral("hole_table") || a->type == QStringLiteral("parts_list"));
    if (a != nullptr && !table && a->view != 0 && movedAlong(a->view)) {
      continue;
    }
    commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("edit_drawing_annotation")},
                                {QStringLiteral("drawing"), m_drawing},
                                {QStringLiteral("annotation"), note},
                                {QStringLiteral("move"), QJsonArray{delta.x(), delta.y()}}});
  }
  if (commands.isEmpty()) {
    return;
  }
  if (!m_host.canChangeModel()) {
    setStatus(m_host.changeBlockedReason(), true);
    return;
  }
  qDebug().noquote() << QStringLiteral("Drawing command: %1")
                            .arg(QString::fromUtf8(QJsonDocument(commands).toJson(QJsonDocument::Compact)));
  if (!m_host.runModelCommands(commands, tr("Move %1 items of %2").arg(commands.size()).arg(m_name)) &&
      !m_host.lastCancelled()) {
    qDebug().noquote() << QStringLiteral("Drawing command refused: %1").arg(m_host.lastError());
    setStatus(m_host.lastError(), true);
  }
}

void DrawingWorkspace::contextMenu(QPointF at, const QPoint& global) {
  Q_UNUSED(at);
  QMenu menu(this);
  QStringList entries;
  const auto add = [this, &menu, &entries](const QString& text, std::function<void()> act) {
    entries << text;
    return menu.addAction(text, this, std::move(act));
  };
  const auto later = [this](std::function<void()> act) {
    // After the menu: a dialog or a tool has the events to itself.
    return [this, act] { QTimer::singleShot(0, this, act); };
  };
  const int count = m_sheetView->selectionSize();
  const int viewId = m_sheetView->selectedView();
  const int note = m_sheetView->selectedAnnotation();
  // A cut sheet's parts (mitcad#853): turned a quarter, copied, deleted.
  if (m_cutSheet) {
    if (count > 1) {
      add(tr("Delete"), later([this] { deleteSelection(); }));
    } else if (viewId != 0) {
      add(tr("Rotate 90° Left"),
          later([this, viewId] { editPart(viewId, {{QStringLiteral("turn"), 90.0}}); }));
      add(tr("Rotate 90° Right"),
          later([this, viewId] { editPart(viewId, {{QStringLiteral("turn"), -90.0}}); }));
      add(tr("Add Copy"), later([this, viewId] { addCopy(viewId); }));
      menu.addSeparator();
      add(tr("Delete Part"), later([this, viewId] { deleteView(viewId); }));
    } else {
      add(tr("Add Parts…"), later([this] { addParts(); }));
      add(tr("Material…"), later([this] { editCutSheet(); }));
    }
    qDebug().noquote() << QStringLiteral("Context menu: %1").arg(entries.join(QStringLiteral(" | ")));
    execMenu(&menu, global);
    return;
  }
  if (count > 1) {
    add(tr("Delete"), later([this] { deleteSelection(); }));
  } else if (note != 0) {
    const SheetAnnotation* shown = m_sheetView->annotation(note);
    const QString type = shown != nullptr ? shown->type : QString();
    if (type == QStringLiteral("note") || type == QStringLiteral("text")) {
      add(tr("Edit Text…"), later([this, note] { editNoteText(note); }));
    }
    const bool dimension = type == QStringLiteral("linear") || type == QStringLiteral("angle") ||
                           type == QStringLiteral("radius") || type == QStringLiteral("diameter") ||
                           type == QStringLiteral("ordinate");
    if (dimension || type == QStringLiteral("frame")) {
      // The panel's Tolerance… button.
      add(tr("Tolerance…"), later([this, type] {
            QPushButton* button = m_panel->findChild<QPushButton*>(
                type == QStringLiteral("frame") ? QStringLiteral("drawingAnnotationFrame")
                                                : QStringLiteral("drawingAnnotationTolerance"));
            if (button != nullptr) {
              button->click();
            }
          }));
    }
    add(tr("Delete"), later([this, note] { deleteAnnotation(note); }));
  } else if (viewId != 0) {
    const SheetView* shown = m_sheetView->view(viewId);
    // The views made from it: their tools, from this view.
    add(tr("Projected View"), later([this, viewId] {
          setTool(drawingTool(QStringLiteral("drawing.projected_view")));
          m_sheetView->setParentView(viewId);
        }));
    add(tr("Section View"), later([this] { setTool(drawingTool(QStringLiteral("drawing.section_view"))); }));
    add(tr("Detail View"), later([this] { setTool(drawingTool(QStringLiteral("drawing.detail_view"))); }));
    add(tr("Break"), later([this] { setTool(drawingTool(QStringLiteral("drawing.break"))); }));
    // A quarter turn either way (mitcad#729); a detail turns with its
    // parent.
    if (shown != nullptr && shown->type != QStringLiteral("detail")) {
      const double turned = shown->json.value(QStringLiteral("rotation")).toDouble();
      for (const auto& [text, by] : {std::pair{"Rotate 90° Left", 90.0}, std::pair{"Rotate 90° Right", -90.0}}) {
        const QString degrees = QString::number(turned + by, 'g', 10);
        add(tr(text), later([this, viewId, degrees] { editView(viewId, QStringLiteral("rotation"), degrees); }));
      }
    }
    menu.addSeparator();
    // The panel's check boxes, as checkable entries.
    const QJsonObject json = shown != nullptr ? shown->json : QJsonObject();
    const QJsonObject style = json.value(QStringLiteral("style")).toObject();
    const auto toggle = [this, viewId, &add](const QString& text, const QString& field, bool on) {
      QAction* action = add(text, [this, viewId, field, on] {
        const SheetView* v = m_sheetView->view(viewId);
        const QJsonObject command =
            viewEdit(m_drawing, viewId, field, on ? QStringLiteral("false") : QStringLiteral("true"),
                     v != nullptr ? v->json.value(QStringLiteral("style")).toObject() : QJsonObject());
        QTimer::singleShot(0, this, [this, command] { run(command); });
      });
      action->setCheckable(true);
      action->setChecked(on);
    };
    toggle(tr("Hidden Edges"), QStringLiteral("hidden_lines"), style.value(QStringLiteral("hidden_lines")).toBool());
    toggle(tr("Tangent Edges"), QStringLiteral("tangent_edges"), style.value(QStringLiteral("tangent_edges")).toBool());
    toggle(tr("Shaded"), QStringLiteral("shaded"), style.value(QStringLiteral("shaded")).toBool());
    toggle(tr("Center Marks"), QStringLiteral("center_marks"), json.value(QStringLiteral("center_marks")).toBool());
    if (shown != nullptr && shown->type == QStringLiteral("section")) {
      toggle(tr("Hatching"), QStringLiteral("hatch"), json.value(QStringLiteral("hatch")).toBool(true));
    }
    menu.addSeparator();
    add(tr("Delete View"), later([this, viewId] { deleteView(viewId); }));
  } else {
    add(tr("Sheet…"), later([this] { editSheet(); }));
    add(tr("Add Revision…"), later([this] { addRevision(); }));
    add(tr("Add Sheet"), later([this] { addSheet(); }));
  }
  qDebug().noquote() << QStringLiteral("Context menu: %1").arg(entries.join(QStringLiteral(" | ")));
  execMenu(&menu, global);
}

bool DrawingWorkspace::run(const QJsonObject& command, QJsonObject* result) {
  if (!m_host.canChangeModel()) {
    setStatus(m_host.changeBlockedReason(), true);
    return false;
  }
  qDebug().noquote() << QStringLiteral("Drawing command: %1").arg(compact(command));
  // The main window refreshes the scene after it, and so the sheet.
  if (!m_host.runModelCommand(command, result)) {
    if (!m_host.lastCancelled()) {
      qDebug().noquote() << QStringLiteral("Drawing command refused: %1").arg(m_host.lastError());
      setStatus(m_host.lastError(), true);
    }
    return false;
  }
  return true;
}

void DrawingWorkspace::editSheet() {
  if (m_cutSheet) {
    editCutSheet();
    return;
  }
  const QJsonObject sheet = m_answer.value(QStringLiteral("sheets")).toArray().at(m_sheet).toObject();
  const QJsonObject block = sheet.value(QStringLiteral("title_block")).toObject();
  QDialog dialog(this);
  dialog.setWindowTitle(tr("Sheet"));
  auto* form = new QFormLayout(&dialog);
  // Its template (mitcad#707 phase 4): one of the libraries', or the one it
  // has when they no longer have it, or the simple title block.
  const QList<TemplateChoice> choices = availableTemplates(m_host);
  const QJsonObject ownTemplate = sheet.value(QStringLiteral("template")).toObject();
  const QString currentTemplate = ownTemplate.value(QStringLiteral("choice")).toString();
  auto* templateBox = new QComboBox;
  templateBox->setObjectName(QStringLiteral("drawingSheetTemplate"));
  for (const TemplateChoice& choice : choices) {
    templateBox->addItem(choice.label, choice.choice);
  }
  int templateAt = templateIndex(choices, currentTemplate);
  if (templateAt < 0) {
    templateBox->insertItem(0, ownTemplate.value(QStringLiteral("name")).toString(), currentTemplate);
    templateAt = 0;
  }
  templateBox->setCurrentIndex(templateAt);
  form->addRow(tr("&Template"), templateBox);
  QComboBox* sizes = comboOf(sheetSizes(), sheet.value(QStringLiteral("size")).toString());
  form->addRow(tr("Size"), sizes);
  QComboBox* orientation = comboOf({{QStringLiteral("landscape"), tr("Landscape")}, {QStringLiteral("portrait"), tr("Portrait")}},
                                   sheet.value(QStringLiteral("orientation")).toString());
  form->addRow(tr("Orientation"), orientation);
  QComboBox* projection =
      comboOf({{QStringLiteral("third_angle"), tr("Third Angle")}, {QStringLiteral("first_angle"), tr("First Angle")}},
              sheet.value(QStringLiteral("projection")).toString());
  projection->setObjectName(QStringLiteral("drawingSheetProjection"));
  form->addRow(tr("Projection"), projection);
  // Another template brings its size and projection method, as in Create
  // Drawing (mitcad#725); the projected views turn with the method, the
  // sections and details drawn on them following (mitcad#729).
  connect(templateBox, &QComboBox::currentIndexChanged, &dialog, [templateBox, sizes, projection, &choices] {
    const int at = templateIndex(choices, templateBox->currentData().toString());
    if (at < 0) {
      return;
    }
    const int size = sizes->findData(choices.at(at).size);
    if (size >= 0) {
      sizes->setCurrentIndex(size);
    }
    const int method = projection->findData(choices.at(at).projection);
    if (method >= 0) {
      projection->setCurrentIndex(method);
    }
  });
  // The units of its dimensions and tables (mitcad#725): the design's, or
  // a length unit of its own.
  const QString sheetUnits = sheet.value(QStringLiteral("units")).toString();
  QComboBox* units = comboOf({{QString(), tr("The Design's")},
                              {QStringLiteral("mm"), tr("Millimetres (mm)")},
                              {QStringLiteral("cm"), tr("Centimetres (cm)")},
                              {QStringLiteral("m"), tr("Metres (m)")},
                              {QStringLiteral("in"), tr("Inches (in)")},
                              {QStringLiteral("ft"), tr("Feet (ft)")},
                              {QStringLiteral("um"), tr("Micrometres (um)")},
                              {QStringLiteral("mil"), tr("Mils (mil)")}},
                             sheetUnits);
  units->setObjectName(QStringLiteral("drawingSheetUnits"));
  form->addRow(tr("&Units"), units);
  const auto field = [&form, &block](const QString& key, const QString& label, const QString& placeholder) {
    auto* edit = new QLineEdit(block.value(key).toString());
    edit->setPlaceholderText(placeholder);
    form->addRow(label, edit);
    return edit;
  };
  QLineEdit* title = field(QStringLiteral("title"), tr("Title"), tr("The design's name"));
  QLineEdit* number = field(QStringLiteral("number"), tr("Drawing Number"), QString());
  QLineEdit* author = field(QStringLiteral("author"), tr("Drawn By"), QString());
  QLineEdit* date = field(QStringLiteral("date"), tr("Date"), tr("Today"));
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  form->addRow(buttons);
  qDebug().noquote() << QStringLiteral("Drawing sheet dialog opened: %1").arg(sheet.value(QStringLiteral("size")).toString());
  if (execDialog(&dialog) != QDialog::Accepted) {
    return;
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("edit_drawing_sheet")},
                      {QStringLiteral("drawing"), m_drawing},
                      {QStringLiteral("sheet"), m_sheet},
                      {QStringLiteral("size"), sizes->currentData().toString()},
                      {QStringLiteral("orientation"), orientation->currentData().toString()},
                      {QStringLiteral("projection"), projection->currentData().toString()},
                      {QStringLiteral("units"), units->currentData().toString()},
                      {QStringLiteral("title_block"), QJsonObject{{QStringLiteral("title"), title->text()},
                                                                  {QStringLiteral("number"), number->text()},
                                                                  {QStringLiteral("author"), author->text()},
                                                                  {QStringLiteral("date"), date->text()}}}};
  // A template another than its own: as the libraries give it now.
  const QString chosen = templateBox->currentData().toString();
  const int chosenAt = templateIndex(choices, chosen);
  if (chosen != currentTemplate && chosenAt >= 0) {
    command.insert(QStringLiteral("template"), choices.at(chosenAt).value);
  }
  run(command);
}

void DrawingWorkspace::addSheet() {
  QJsonObject result;
  if (run({{QStringLiteral("cmd"), QStringLiteral("add_drawing_sheet")}, {QStringLiteral("drawing"), m_drawing}},
          &result)) {
    m_sheet = result.value(QStringLiteral("sheet")).toInt();
    // The tool's picks were on the sheet left (mitcad#759).
    dropPicks();
    refresh();
  }
}

void DrawingWorkspace::deleteSheet(int index) {
  // The sheet shown stays shown (mitcad#759): the sheets after the one
  // deleted move up a place; the shown one deleted, the next one shows
  // (the one before when it was the last).
  const int shown = m_sheet;
  const int count = static_cast<int>(m_tabs->count());
  int after = shown;
  if (index < shown) {
    after = shown - 1;
  } else if (index == shown) {
    after = std::min(shown, count - 2);
  }
  // Before the command: the refresh after it asks for this sheet.
  m_sheet = std::max(0, after);
  if (!run({{QStringLiteral("cmd"), QStringLiteral("delete_drawing_sheet")},
            {QStringLiteral("drawing"), m_drawing},
            {QStringLiteral("sheet"), index}})) {
    m_sheet = shown;
    refresh();
    return;
  }
  if (index == shown) {
    // The tool's picks were on the sheet deleted.
    dropPicks();
  }
  if (index <= shown) {
    qDebug().noquote() << QStringLiteral("Drawing sheet %1 shown").arg(m_sheet + 1);
  }
}

void DrawingWorkspace::dropPicks() {
  m_hoverTimer->stop();
  m_hoverView = -1;
  m_sheetView->setHoverMarks(QJsonArray());
  const bool sheetPicks = m_sheetView->cancelPicks();
  const bool toolPicks = clearToolPicks();
  if (sheetPicks || toolPicks) {
    showHint(); // the tool's own hint again, not the placing one
  }
}

void DrawingWorkspace::sheetMenu(int index, const QPoint& at) {
  if (index < 0) {
    return;
  }
  QMenu menu(this);
  menu.addAction(tr("Rename…"), this, [this, index] {
    bool accepted = false;
    const QString name = dialogText(this, tr("Rename Sheet"), tr("Name:"), m_tabs->tabText(index), &accepted);
    if (accepted && !name.trimmed().isEmpty()) {
      run({{QStringLiteral("cmd"), QStringLiteral("edit_drawing_sheet")},
           {QStringLiteral("drawing"), m_drawing},
           {QStringLiteral("sheet"), index},
           {QStringLiteral("name"), name.trimmed()}});
    }
  });
  QAction* remove = menu.addAction(tr("Delete Sheet"), this, [this, index] { deleteSheet(index); });
  remove->setEnabled(m_tabs->count() > 1);
  QAction* save = menu.addAction(tr("Save as Template…"), this, [this, index] { saveTemplate(index); });
  save->setEnabled(m_answer.value(QStringLiteral("sheets"))
                       .toArray()
                       .at(index)
                       .toObject()
                       .value(QStringLiteral("template"))
                       .isObject());
  qDebug().noquote() << QStringLiteral("Drawing sheet menu: sheet %1").arg(index + 1);
  execMenu(&menu, at);
}

void DrawingWorkspace::saveTemplate(int sheet) {
  QDialog dialog(this);
  dialog.setWindowTitle(tr("Save as Template"));
  auto* form = new QFormLayout(&dialog);
  auto* name = new QLineEdit;
  name->setObjectName(QStringLiteral("drawingTemplateName"));
  form->addRow(tr("&Name"), name);
  auto* library = new QComboBox;
  library->addItem(tr("User"), QStringLiteral("user"));
  const QString project = m_host.projectFolder();
  if (!project.isEmpty()) {
    library->addItem(tr("Project"), QStringLiteral("project"));
  }
  form->addRow(tr("&Library"), library);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  QPushButton* ok = buttons->button(QDialogButtonBox::Ok);
  ok->setEnabled(false);
  connect(name, &QLineEdit::textChanged, ok, [ok](const QString& text) { ok->setEnabled(!text.trimmed().isEmpty()); });
  form->addRow(buttons);
  qDebug().noquote() << QStringLiteral("Drawing save template dialog opened: sheet %1").arg(sheet + 1);
  if (execDialog(&dialog) != QDialog::Accepted) {
    return;
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("save_drawing_template")},
                      {QStringLiteral("drawing"), m_drawing},
                      {QStringLiteral("sheet"), sheet},
                      {QStringLiteral("library"), library->currentData().toString()},
                      {QStringLiteral("name"), name->text().trimmed()}};
  if (!project.isEmpty()) {
    command.insert(QStringLiteral("project"), project);
  }
  QJsonObject result;
  if (run(command, &result)) {
    const QString path = result.value(QStringLiteral("path")).toString();
    qInfo().noquote() << QStringLiteral("Drawing template saved: %1 %2")
                             .arg(result.value(QStringLiteral("choice")).toString(), path);
    setStatus(tr("Template saved to %1.").arg(QDir::toNativeSeparators(path)), false);
  }
}

void DrawingWorkspace::addRevision() {
  if (!isOpen()) {
    return;
  }
  const std::optional<RevisionFields> fields = askRevision(this, tr("Add Revision"), RevisionFields(), false);
  if (!fields) {
    return;
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("add_drawing_revision")},
                      {QStringLiteral("drawing"), m_drawing},
                      {QStringLiteral("description"), fields->description}};
  if (!fields->date.isEmpty()) {
    command.insert(QStringLiteral("date"), fields->date);
  }
  if (!fields->author.isEmpty()) {
    command.insert(QStringLiteral("author"), fields->author);
  }
  run(command);
}

void DrawingWorkspace::editRevision(const QString& letter) {
  RevisionFields current;
  for (const QJsonValue& value : m_answer.value(QStringLiteral("revisions")).toArray()) {
    const QJsonObject revision = value.toObject();
    if (revision.value(QStringLiteral("letter")).toString() == letter) {
      current.description = revision.value(QStringLiteral("description")).toString();
      current.date = revision.value(QStringLiteral("date")).toString();
      current.author = revision.value(QStringLiteral("author")).toString();
    }
  }
  const std::optional<RevisionFields> fields = askRevision(this, tr("Revision %1").arg(letter), current, true);
  if (!fields) {
    return;
  }
  if (fields->remove) {
    run({{QStringLiteral("cmd"), QStringLiteral("delete_drawing_revision")},
         {QStringLiteral("drawing"), m_drawing},
         {QStringLiteral("revision"), letter}});
    return;
  }
  run({{QStringLiteral("cmd"), QStringLiteral("edit_drawing_revision")},
       {QStringLiteral("drawing"), m_drawing},
       {QStringLiteral("revision"), letter},
       {QStringLiteral("description"), fields->description},
       {QStringLiteral("date"), fields->date},
       {QStringLiteral("author"), fields->author}});
}

void DrawingWorkspace::logTitleBlock(const QJsonObject& sheet) {
  QStringList fields;
  for (const QJsonValue& value : sheet.value(QStringLiteral("title_fields")).toArray()) {
    const QJsonObject field = value.toObject();
    const QString label = field.value(QStringLiteral("label")).toString();
    if (!label.isEmpty()) {
      fields << QStringLiteral("%1=%2").arg(label, field.value(QStringLiteral("text")).toString());
    }
  }
  const QJsonObject own = sheet.value(QStringLiteral("template")).toObject();
  const QString line = QStringLiteral("Drawing title block (%1): %2")
                           .arg(own.isEmpty() ? QStringLiteral("simple") : own.value(QStringLiteral("choice")).toString(),
                                fields.join(QStringLiteral(" | ")));
  if (line != m_loggedTitle) {
    m_loggedTitle = line;
    qDebug().noquote() << line;
  }
  QStringList rows;
  for (const QJsonValue& value : m_answer.value(QStringLiteral("revisions")).toArray()) {
    const QJsonObject revision = value.toObject();
    rows << QStringLiteral("%1 %2").arg(revision.value(QStringLiteral("letter")).toString(),
                                        revision.value(QStringLiteral("description")).toString());
  }
  const QString revisions =
      QStringLiteral("Drawing revisions: %1").arg(rows.isEmpty() ? QStringLiteral("none") : rows.join(QStringLiteral("; ")));
  if (revisions != m_loggedRevisions) {
    m_loggedRevisions = revisions;
    qDebug().noquote() << revisions;
  }
}

void DrawingWorkspace::exportDrawing(const QString& format) {
  const QString suggested =
      QDir(dialogStartFolder(DialogGroup::Export, QDir::homePath())).filePath(m_name + QLatin1Char('.') + format);
  const QString filter = format == QStringLiteral("pdf")   ? tr("PDF (*.pdf)")
                         : format == QStringLiteral("svg") ? tr("SVG (*.svg)")
                         : format == QStringLiteral("dwg") ? tr("DWG (*.dwg)")
                                                           : tr("DXF (*.dxf)");
  const QString path =
      dialogSaveFileName(this, m_cutSheet ? tr("Export Cut Sheet") : tr("Export Drawing"), suggested, filter);
  if (path.isEmpty()) {
    return;
  }
  rememberDialogFolder(DialogGroup::Export, path);
  exportTo(path);
}

bool DrawingWorkspace::exportTo(const QString& path) {
  // A cut sheet (mitcad#853): its parts' outlines, their names with Part
  // Names on the LABEL Layer.
  if (m_cutSheet) {
    const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("export_cut_sheet")},
                              {QStringLiteral("path"), path},
                              {QStringLiteral("drawing"), m_drawing},
                              {QStringLiteral("labels"), m_cutLabels}};
    QJsonObject result;
    qDebug().noquote() << QStringLiteral("Drawing export: %1").arg(QDir::toNativeSeparators(path));
    if (!m_host.runModelCommand(command, &result)) {
      setStatus(m_host.lastError(), true);
      return false;
    }
    const int parts = result.value(QStringLiteral("parts")).toInt();
    qInfo().noquote() << QStringLiteral("Exported %1: cut sheet %2, %3 parts").arg(path, m_name).arg(parts);
    setStatus(tr("Exported %1 (%2 part(s)).").arg(QDir::toNativeSeparators(path)).arg(parts), false);
    return true;
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("export_drawing")},
                      {QStringLiteral("path"), path},
                      {QStringLiteral("drawing"), m_drawing}};
  const QString title = m_title ? m_title() : QString();
  if (!title.isEmpty()) {
    command.insert(QStringLiteral("title"), title);
  }
  QJsonObject result;
  qDebug().noquote() << QStringLiteral("Drawing export: %1").arg(QDir::toNativeSeparators(path));
  if (!m_host.runModelCommand(command, &result)) {
    setStatus(m_host.lastError(), true);
    return false;
  }
  const int sheets = result.value(QStringLiteral("sheets")).toInt();
  qInfo().noquote() << QStringLiteral("Exported %1: drawing %2, %3 sheets").arg(path, m_name).arg(sheets);
  setStatus(tr("Exported %1 (%2 sheet(s)).").arg(QDir::toNativeSeparators(path)).arg(sheets), false);
  return true;
}

void DrawingWorkspace::setStatus(const QString& text, bool error) {
  if (!text.isEmpty()) {
    m_host.showStatus(text, error);
  }
}

void DrawingWorkspace::place() {
  // The whole view, edge to edge (mitcad#1008): no 3D view shows around the
  // drawing. Its bar and sheet start below the rows that float over the
  // view's top (the Floating layout's title row and toolbar) and keep clear
  // of the view's other overlays (overlayInsets); what is above and beside
  // them is the drawing's background.
  const QRect whole = m_viewer->rect();
  const QMargins insets = m_viewer->overlayInsets();
  const QRect area = whole.marginsRemoved(insets);
  // The cards floating at the view's sides (the Floating layout's browser)
  // cover the bar where they reach its row: its controls start beside
  // them, so that they stay clickable (mitcad#725: Close Drawing lay under
  // the browser). The sheet goes on under them, fitted beside them.
  const int band = std::max(m_bar->sizeHint().height(), 40);
  const QRect barRow(area.left(), area.top(), area.width(), band);
  const QRect body(area.left(), area.top() + band, area.width(), std::max(0, area.height() - band));
  constexpr int kEdge = 24; // how near a side a card stands to count
  constexpr int kGap = 8;
  QRect room = area;
  int coveredLeft = 0; // of the sheet, from its left
  for (const QRect& card : m_viewer->coveredRects()) {
    const bool atLeft = card.left() <= area.left() + kEdge && card.right() < area.center().x();
    const bool atRight = card.right() >= area.right() - kEdge && card.left() > area.center().x();
    if (card.intersects(barRow)) {
      if (atLeft) {
        room.setLeft(std::max(room.left(), card.right() + 1 + kGap));
      } else if (atRight) {
        room.setRight(std::min(room.right(), card.left() - 1 - kGap));
      }
    }
    if (atLeft && card.intersects(body)) {
      coveredLeft = std::max(coveredLeft, card.right() + 1 + kGap - area.left());
    }
  }
  if (room.width() < area.width() / 2) {
    room = area;
  }
  m_column->setContentsMargins(insets);
  m_bar->layout()->setContentsMargins(6 + room.left() - area.left(), 4, 6 + area.right() - room.right(), 4);
  m_sheetView->setFitMargins(QMargins(std::min(coveredLeft, body.width() / 2), 0, 0, 0));
  fitTools(room.width());
  if (whole != geometry()) {
    setGeometry(whole);
  }
}

void DrawingWorkspace::setRibbonTools(std::function<QWidget*(const QString&)> buttonOf) {
  m_ribbonPlace = std::move(buttonOf);
  const bool own = !m_ribbonPlace;
  m_close->setVisible(own);
  m_toolRow->setVisible(own && !m_cutSheet);
  m_cutRow->setVisible(own && m_cutSheet);
  m_toolsWidth = -1;
  if (isVisible()) {
    place();
  }
}

void DrawingWorkspace::setCutLabels(bool on) {
  if (on != m_cutLabelsAction->isChecked()) {
    m_cutLabelsAction->setChecked(on); // comes back here
    return;
  }
  if (on == m_cutLabels) {
    return;
  }
  m_cutLabels = on;
  qDebug().noquote() << QStringLiteral("Cut sheet labels: %1").arg(on ? QStringLiteral("on") : QStringLiteral("off"));
}

void DrawingWorkspace::logPlaces() { logButtons(); }

void DrawingWorkspace::fitTools(int width) {
  if (m_ribbonPlace) {
    return; // the ribbon's tab has the tools
  }
  if (width == m_toolsWidth) {
    return; // as fitted for this width (place runs on every frame)
  }
  m_toolsWidth = width;
  // The views' tools show their names beside their icons when the row has
  // room for them, else their icons only, their names in their tooltips
  // (mitcad#725: beside a floating card the bar is narrower).
  const auto names = [this](bool shown) {
    for (const DrawingToolDef& tool : drawingTools()) {
      QToolButton* b = m_toolButtons.value(tool.id);
      if (tool.iconOnly || b == nullptr) {
        continue;
      }
      b->setToolButtonStyle(shown ? Qt::ToolButtonTextBesideIcon : Qt::ToolButtonIconOnly);
      b->setToolTip(shown ? tool.tooltip : QStringLiteral("%1: %2").arg(tool.name, tool.tooltip));
    }
    m_bar->layout()->invalidate();
  };
  names(true);
  const bool compact = m_bar->sizeHint().width() > width;
  if (compact) {
    names(false);
  }
  if (compact != m_compactTools) {
    m_compactTools = compact;
    qDebug().noquote() << QStringLiteral("Drawing tools: %1").arg(compact ? QStringLiteral("icons") : QStringLiteral("names"));
    QTimer::singleShot(0, this, [this] { logButtons(); });
  }
}

bool DrawingWorkspace::event(QEvent* event) {
  // Delete and Esc are the drawing's wherever the focus is in it, before
  // the window's shortcuts: they never reach the model behind the sheet.
  // A text field takes them first (it accepts them for itself).
  if (event->type() == QEvent::ShortcutOverride && isOpen()) {
    const auto* pressed = static_cast<QKeyEvent*>(event);
    const int key = pressed->key();
    if (unmodified(pressed) && (isDeleteKey(key) || key == Qt::Key_Escape)) {
      event->accept();
      return true;
    }
    // A model command's shortcut (Extrude's E, …) would close the drawing
    // and start the command (MainWindow::trigger): taken here, it does
    // nothing (mitcad#725; keyPressEvent says so).
    if (isModelShortcut(pressed)) {
      event->accept();
      return true;
    }
  }
  return QWidget::event(event);
}

void DrawingWorkspace::keyPressEvent(QKeyEvent* event) {
  if (isOpen() && isModelShortcut(event)) {
    qDebug().noquote() << QStringLiteral("Drawing shortcut ignored: %1")
                              .arg(QKeySequence(event->keyCombination()).toString(QKeySequence::PortableText));
    event->accept();
    return;
  }
  if (!unmodified(event)) {
    QWidget::keyPressEvent(event);
    return;
  }
  if (isOpen() && isDeleteKey(event->key())) {
    if (m_tool == nullptr && m_sheetView->selectionSize() > 1) {
      deleteSelection();
    } else if (m_tool == nullptr && m_sheetView->selectedAnnotation() != 0) {
      deleteAnnotation(m_sheetView->selectedAnnotation());
    } else if (m_tool == nullptr && m_sheetView->selectedView() != 0) {
      deleteView(m_sheetView->selectedView());
    }
    return;
  }
  if (isOpen() && event->key() == Qt::Key_Escape) {
    escape();
    return;
  }
  QWidget::keyPressEvent(event);
}

bool DrawingWorkspace::eventFilter(QObject* watched, QEvent* event) {
  if (watched == m_viewer && event->type() == QEvent::Resize && isVisible()) {
    place();
  }
  // Another of the view's children came above the drawing (open): back
  // under it, once that raise is done.
  if (watched != m_viewer && event->type() == QEvent::ZOrderChange && isVisible()) {
    QTimer::singleShot(0, this, [this] {
      if (isVisible()) {
        raise();
      }
    });
  }
  // The 3D view under the sheet can have the keyboard (a click on the strip
  // between the floating cards): its keys are the drawing's then too, as
  // escapeDrawing has it, so a model shortcut does not leave the drawing
  // there either (mitcad#725).
  if (watched == m_viewer && isOpen() && isVisible() &&
      (event->type() == QEvent::ShortcutOverride || event->type() == QEvent::KeyPress)) {
    const auto* key = static_cast<QKeyEvent*>(event);
    if (isModelShortcut(key)) {
      if (event->type() == QEvent::KeyPress) {
        qDebug().noquote() << QStringLiteral("Drawing shortcut ignored: %1")
                                  .arg(QKeySequence(key->keyCombination()).toString(QKeySequence::PortableText));
      }
      event->accept();
      return true;
    }
  }
  // A press on a sheet's tab shows its sheet (mitcad#725), whatever the
  // tab bar then does with it: it makes the same tab current, which is no
  // change. Logged with the tab Qt finds there, for the Windows test.
  if (watched == m_tabs && event->type() == QEvent::MouseButtonPress) {
    const auto* press = static_cast<QMouseEvent*>(event);
    if (press->button() == Qt::LeftButton) {
      const int index = m_tabs->tabAt(press->position().toPoint());
      qDebug().noquote() << QStringLiteral("Drawing sheet tab pressed: %1")
                                .arg(index >= 0 ? QString::number(index + 1) : QStringLiteral("none"));
      switchSheet(index);
    }
  }
  return QWidget::eventFilter(watched, event);
}

void DrawingWorkspace::resizeEvent(QResizeEvent* event) {
  QWidget::resizeEvent(event);
  QTimer::singleShot(0, this, [this] { logButtons(); });
}

void DrawingWorkspace::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  QTimer::singleShot(0, this, [this] { logButtons(); });
}

void DrawingWorkspace::logButtons() {
  if (!isVisible()) {
    return;
  }
  // The places as they will be drawn: the layouts settled first (a panel
  // just built is laid out once its layout runs).
  if (layout() != nullptr) {
    layout()->activate();
  }
  if (m_bar->layout() != nullptr) {
    m_bar->layout()->activate();
  }
  if (m_panel->layout() != nullptr) {
    m_panel->layout()->activate();
  }
  QStringList lines;
  const auto at = [this](const QWidget* widget) {
    const QPoint p = widget->mapTo(window(), widget->rect().center());
    return QStringLiteral("%1,%2").arg(p.x()).arg(p.y());
  };
  // The drawing's area and the 3D view's, in the window (mitcad#1008: the
  // same rectangle).
  const auto rectAt = [this](const QWidget* widget, const QRect& box) {
    const QPoint p = widget->mapTo(window(), box.topLeft());
    return QStringLiteral("%1,%2 %3x%4").arg(p.x()).arg(p.y()).arg(box.width()).arg(box.height());
  };
  lines << QStringLiteral("Drawing area %1, view %2").arg(rectAt(this, rect()), rectAt(m_viewer, m_viewer->rect()));
  // A control of the bar, or its button on the ribbon's tab (by the
  // command's id, or a group's "DRAWING/EXPORT") with setRibbonTools: a
  // line when it is in sight.
  const auto add = [this, &lines, &at](const QString& what, const QWidget* own, const QString& onRibbon) {
    const QWidget* widget = m_ribbonPlace ? m_ribbonPlace(onRibbon) : own;
    if (widget != nullptr) {
      lines << QStringLiteral("Drawing %1 at %2").arg(what, at(widget));
    }
  };
  const QString ribbonTab = m_cutSheet ? QStringLiteral("CUT SHEET") : QStringLiteral("DRAWING");
  add(QStringLiteral("Close Drawing"), m_close, QStringLiteral("drawing.close"));
  lines << QStringLiteral("Drawing Add Sheet at %1").arg(at(m_addSheet));
  lines << QStringLiteral("Drawing Sheet at %1").arg(at(m_sheetButton));
  add(QStringLiteral("Export"), m_cutSheet ? m_cutExport : m_export, ribbonTab + QStringLiteral("/EXPORT"));
  if (m_cutSheet) {
    add(QStringLiteral("Add Parts"), m_addParts, QStringLiteral("drawing.add_parts"));
  }
  lines << QStringLiteral("Drawing Add Revision at %1").arg(at(m_addRevision));
  for (const DrawingToolDef& tool : drawingTools()) {
    add(QStringLiteral("tool ") + tool.name, m_toolButtons.value(tool.id), tool.id);
  }
  if (m_ribbonPlace) {
    for (int g = 0; g < kToolGroups; ++g) {
      add(QStringLiteral("group ") + toolGroupName(g), nullptr, ribbonTab + QLatin1Char('/') + toolGroupId(g));
    }
    add(QStringLiteral("group ") + tr("Export"), nullptr, ribbonTab + QStringLiteral("/EXPORT"));
  } else {
    for (const QLabel* caption : std::as_const(m_groupCaptions)) {
      lines << QStringLiteral("Drawing group %1 at %2").arg(caption->text(), at(caption));
    }
  }
  for (int i = 0; i < m_tabs->count(); ++i) {
    const QPoint p = m_tabs->mapTo(window(), m_tabs->tabRect(i).center());
    lines << QStringLiteral("Drawing Sheet tab %1 at %2,%3").arg(i + 1).arg(p.x()).arg(p.y());
  }
  for (int row = 0; row < m_form->rowCount(); ++row) {
    QLayoutItem* item = m_form->itemAt(row, QFormLayout::FieldRole);
    if (item != nullptr && item->widget() != nullptr && !item->widget()->objectName().isEmpty()) {
      // A check box takes a click on its box and label only, not
      // across the row it stretches over: its place is its box.
      const QWidget* field = item->widget();
      QString place = at(field);
      if (qobject_cast<const QCheckBox*>(field) != nullptr) {
        const QPoint box = field->mapTo(window(), QPoint(field->height() / 2, field->height() / 2));
        place = QStringLiteral("%1,%2").arg(box.x()).arg(box.y());
      }
      lines << QStringLiteral("Drawing panel %1 at %2").arg(field->objectName(), place);
    }
  }
  const QString text = lines.join(QLatin1Char('\n'));
  if (text == m_loggedButtons) {
    return;
  }
  m_loggedButtons = text;
  for (const QString& line : std::as_const(lines)) {
    qDebug().noquote() << line;
  }
}

// --- Cut sheets (mitcad#853) -------------------------------------------------

void DrawingWorkspace::editPart(int part, const QJsonObject& fields) {
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("edit_cut_sheet_part")},
                      {QStringLiteral("drawing"), m_drawing},
                      {QStringLiteral("part"), part}};
  for (auto it = fields.begin(); it != fields.end(); ++it) {
    command.insert(it.key(), it.value());
  }
  QTimer::singleShot(0, this, [this, command] { run(command); });
}

void DrawingWorkspace::addCopy(int part) {
  const SheetView* shown = m_sheetView->view(part);
  if (shown == nullptr) {
    return;
  }
  const QJsonObject json = shown->json;
  QJsonObject body{{QStringLiteral("body"), json.value(QStringLiteral("body"))}};
  if (json.value(QStringLiteral("occurrence")).isString()) {
    body.insert(QStringLiteral("occurrence"), json.value(QStringLiteral("occurrence")));
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("add_cut_sheet_parts")},
                      {QStringLiteral("drawing"), m_drawing},
                      {QStringLiteral("bodies"), QJsonArray{body}}};
  if (json.value(QStringLiteral("own_face")).toBool()) {
    command.insert(QStringLiteral("face"), json.value(QStringLiteral("face")));
  }
  QTimer::singleShot(0, this, [this, command] { run(command); });
}

void DrawingWorkspace::addParts() {
  if (!isOpen() || !m_cutSheet) {
    return;
  }
  QJsonArray bodies;
  try {
    bodies = m_host.model().queryArray(QStringLiteral("bodies"));
  } catch (const std::exception& e) {
    setStatus(QString::fromUtf8(e.what()), true);
    return;
  }
  const QString component = m_answer.value(QStringLiteral("component")).toString();
  const bool root = component.isEmpty() || component == QStringLiteral("C0");
  const QJsonArray selected = m_host.selectedModelBodies();
  QDialog dialog(this);
  dialog.setWindowTitle(tr("Add Parts"));
  auto* form = new QFormLayout(&dialog);
  auto* list = new QListWidget;
  list->setObjectName(QStringLiteral("cutSheetBodies"));
  list->setSelectionMode(QAbstractItemView::ExtendedSelection);
  list->setToolTip(tr("The bodies to add as parts; Ctrl+click chooses several"));
  for (const QJsonValue& value : bodies) {
    const QJsonObject body = value.toObject();
    const QString owner = body.value(QStringLiteral("component")).toString();
    const bool own = root ? owner.isEmpty() || owner == QStringLiteral("C0") : owner == component;
    const QString uid = body.value(QStringLiteral("uid")).toString();
    if (!own || uid.isEmpty()) {
      continue;
    }
    const QString name = body.value(QStringLiteral("name")).toString();
    auto* item = new QListWidgetItem(name.isEmpty() ? uid : name, list);
    item->setData(Qt::UserRole, uid);
    item->setSelected(selected.contains(QJsonValue(uid)));
  }
  form->addRow(tr("&Bodies"), list);
  auto* count = new QSpinBox;
  count->setObjectName(QStringLiteral("cutSheetCount"));
  count->setRange(1, 1000);
  count->setToolTip(tr("How many parts of each body"));
  form->addRow(tr("&Count"), count);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  form->addRow(buttons);
  list->setFocus(Qt::OtherFocusReason);
  qDebug().noquote() << QStringLiteral("Cut sheet Add Parts dialog opened: %1 bodies").arg(list->count());
  if (execDialog(&dialog) != QDialog::Accepted) {
    return;
  }
  QJsonArray chosen;
  for (int row = 0; row < list->count(); ++row) {
    if (list->item(row)->isSelected()) {
      chosen.append(list->item(row)->data(Qt::UserRole).toString());
    }
  }
  if (chosen.isEmpty()) {
    setStatus(tr("Choose the bodies to add as parts."), true);
    return;
  }
  run({{QStringLiteral("cmd"), QStringLiteral("add_cut_sheet_parts")},
       {QStringLiteral("drawing"), m_drawing},
       {QStringLiteral("bodies"), chosen},
       {QStringLiteral("count"), count->value()}});
}

void DrawingWorkspace::editCutSheet() {
  if (!isOpen() || !m_cutSheet) {
    return;
  }
  const double width = m_answer.value(QStringLiteral("width")).toDouble();
  const double height = m_answer.value(QStringLiteral("height")).toDouble();
  QDialog dialog(this);
  dialog.setWindowTitle(tr("Cut Sheet"));
  auto* form = new QFormLayout(&dialog);
  const auto length = [](double value, const char* name) {
    auto* box = new QDoubleSpinBox;
    box->setObjectName(QString::fromLatin1(name));
    box->setRange(1.0, 100000.0);
    box->setDecimals(1);
    box->setSuffix(QStringLiteral(" mm"));
    box->setValue(value);
    return box;
  };
  QDoubleSpinBox* widthBox = length(width, "cutSheetWidth");
  QDoubleSpinBox* heightBox = length(height, "cutSheetHeight");
  form->addRow(tr("&Width"), widthBox);
  form->addRow(tr("&Height"), heightBox);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  form->addRow(buttons);
  qDebug().noquote() << QStringLiteral("Cut sheet material dialog opened");
  if (execDialog(&dialog) != QDialog::Accepted) {
    return;
  }
  if (widthBox->value() == width && heightBox->value() == height) {
    return;
  }
  run({{QStringLiteral("cmd"), QStringLiteral("edit_cut_sheet")},
       {QStringLiteral("drawing"), m_drawing},
       {QStringLiteral("width"), widthBox->value()},
       {QStringLiteral("height"), heightBox->value()}});
}

void DrawingWorkspace::fillCutSheetPanel() {
  m_panelTitle->setText(tr("Cut Sheet"));
  const QJsonObject sheet = m_answer.value(QStringLiteral("sheets")).toArray().at(0).toObject();
  m_form->addRow(tr("Material"), new QLabel(sheet.value(QStringLiteral("size")).toString()));
  m_form->addRow(tr("Scale"), new QLabel(QStringLiteral("1:1")));
  m_form->addRow(tr("Parts"), new QLabel(QString::number(sheet.value(QStringLiteral("views")).toArray().size())));
  for (const QJsonValue& warning : sheet.value(QStringLiteral("warnings")).toArray()) {
    auto* said = new QLabel(warning.toString());
    said->setWordWrap(true);
    said->setStyleSheet(QStringLiteral("color: #c92a2a;"));
    m_form->addRow(said);
  }
  auto* add = new QPushButton(tr("Add Parts…"));
  add->setObjectName(QStringLiteral("cutSheetAddParts"));
  connect(add, &QPushButton::clicked, this, [this] { QTimer::singleShot(0, this, [this] { addParts(); }); });
  m_form->addRow(QString(), add);
  auto* material = new QPushButton(tr("Material…"));
  material->setObjectName(QStringLiteral("cutSheetMaterial"));
  connect(material, &QPushButton::clicked, this,
          [this] { QTimer::singleShot(0, this, [this] { editCutSheet(); }); });
  m_form->addRow(QString(), material);
}

void DrawingWorkspace::fillPartPanel(int id) {
  const SheetView* view = m_sheetView->view(id);
  if (view == nullptr) {
    fillCutSheetPanel();
    return;
  }
  const QJsonObject json = view->json;
  m_panelTitle->setText(tr("Part %1").arg(id));
  if (!view->error.isEmpty()) {
    auto* why = new QLabel(view->error);
    why->setWordWrap(true);
    why->setStyleSheet(QStringLiteral("color: #c92a2a;"));
    m_form->addRow(why);
  }
  m_form->addRow(tr("Body"), new QLabel(view->label));
  // The planar face it lies on: the largest, or one of the body's.
  QList<QPair<QString, QString>> faces{{QString(), tr("Largest Planar Face")}};
  for (const QJsonValue& value : json.value(QStringLiteral("faces")).toArray()) {
    const QJsonObject face = value.toObject();
    const QString name = face.value(QStringLiteral("face")).toString();
    faces.append({name, tr("%1 (%2 mm²)").arg(name).arg(face.value(QStringLiteral("area")).toDouble(), 0, 'f', 0)});
  }
  const bool own = json.value(QStringLiteral("own_face")).toBool();
  QComboBox* face = comboOf(faces, own ? json.value(QStringLiteral("face")).toString() : QString());
  face->setObjectName(QStringLiteral("cutPartFace"));
  face->setToolTip(tr("The planar face the part is seen along"));
  connect(face, &QComboBox::currentIndexChanged, this, [this, id, face] {
    const QString chosen = face->currentData().toString();
    editPart(id, {{QStringLiteral("face"), chosen.isEmpty() ? QJsonValue(QJsonValue::Null) : QJsonValue(chosen)}});
  });
  m_form->addRow(tr("Face"), face);
  const QString shownTurn = QString::number(json.value(QStringLiteral("rotation")).toDouble(), 'g', 6);
  auto* rotation = new QLineEdit(shownTurn);
  rotation->setObjectName(QStringLiteral("cutPartRotation"));
  rotation->setToolTip(tr("How far the part is turned on the sheet, degrees counter-clockwise"));
  connect(rotation, &QLineEdit::editingFinished, this, [this, id, rotation, shownTurn] {
    const QString text = rotation->text().trimmed();
    if (text == shownTurn) {
      return;
    }
    const std::optional<double> degrees = parsePlainNumber(text);
    if (!degrees) {
      setStatus(tr("A rotation is a number of degrees, not %1.").arg(text), true);
      rotation->setText(shownTurn);
      return;
    }
    editPart(id, {{QStringLiteral("rotation"), *degrees}});
  });
  m_form->addRow(tr("Rotation (°)"), rotation);
  auto* turns = new QWidget;
  auto* turnRow = new QHBoxLayout(turns);
  turnRow->setContentsMargins(0, 0, 0, 0);
  struct Turn {
    const char* text;
    double degrees;
    const char* name;
  };
  static const Turn kTurns[] = {{QT_TR_NOOP("Rotate 90° Left"), 90.0, "cutPartLeft"},
                                {QT_TR_NOOP("Rotate 90° Right"), -90.0, "cutPartRight"}};
  for (const Turn& quarter : kTurns) {
    auto* turn = new QPushButton(tr(quarter.text));
    turn->setObjectName(QString::fromLatin1(quarter.name));
    const double degrees = quarter.degrees;
    connect(turn, &QPushButton::clicked, this,
            [this, id, degrees] { editPart(id, {{QStringLiteral("turn"), degrees}}); });
    turnRow->addWidget(turn);
  }
  m_form->addRow(turns);
  auto* copy = new QPushButton(tr("Add Copy"));
  copy->setObjectName(QStringLiteral("cutPartAddCopy"));
  copy->setToolTip(tr("Adds another part of the same body and face"));
  connect(copy, &QPushButton::clicked, this, [this, id] { addCopy(id); });
  m_form->addRow(QString(), copy);
  // What is wrong with where it is.
  QStringList wrong;
  if (json.value(QStringLiteral("off_sheet")).toBool()) {
    wrong << tr("It leaves the sheet.");
  }
  QStringList others;
  for (const QJsonValue& other : json.value(QStringLiteral("overlaps")).toArray()) {
    others << QString::number(other.toInt());
  }
  if (!others.isEmpty()) {
    wrong << tr("It overlaps part %1.").arg(others.join(QStringLiteral(", ")));
  }
  if (!wrong.isEmpty()) {
    auto* said = new QLabel(wrong.join(QLatin1Char(' ')));
    said->setWordWrap(true);
    said->setStyleSheet(QStringLiteral("color: #c92a2a;"));
    m_form->addRow(said);
  }
}

} // namespace mitcad::drawings
