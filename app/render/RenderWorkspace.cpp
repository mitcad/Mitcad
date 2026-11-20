// SPDX-License-Identifier: MIT
#include "render/RenderWorkspace.hpp"

#include <algorithm>
#include <array>
#include <utility>

#include <QAbstractButton>
#include <QAction>
#include <QActionGroup>
#include <QAbstractScrollArea>
#include <QAbstractSpinBox>
#include <QButtonGroup>
#include <QComboBox>
#include <QFormLayout>
#include <QFileInfo>
#include <QFrame>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QInputDialog>
#include <QJsonValue>
#include <QLabel>
#include <QLayout>
#include <QLineEdit>
#include <QListWidget>
#include <QMenu>
#include <QPainter>
#include <QPushButton>
#include <QScrollArea>
#include <QScrollBar>
#include <QSettings>
#include <QStackedWidget>
#include <QTabBar>
#include <QTabWidget>
#include <QToolButton>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>
#include <QVBoxLayout>
#include <QtLogging>

#include "OcctViewer.hpp"
#include "browser/DocumentHost.hpp"
#include "framework/GlassCard.hpp"
#include "framework/Icons.hpp"
#include "framework/ModelMessages.hpp"
#include "framework/PanelTree.hpp"
#include "framework/PlaceWatcher.hpp"
#include "framework/Ribbon.hpp"
#include "framework/TestSync.hpp"
#include "framework/Theme.hpp"
#include "render/RenderClient.hpp"
#include "render/RenderDevice.hpp"
#include "render/RenderEnvironmentPanel.hpp"
#include "render/RenderImagePanel.hpp"
#include "render/RenderMode.hpp"
#include "render/RenderReadyCheck.hpp"

namespace mitcad {

namespace {

// The library's filters: a class of the appearances' (mitcad#1102), or a
// look named so (Paint, Glass).
struct Filter {
  const char* key;
  const char* name;
};
const std::array<Filter, 7> kFilters = {{
    {"all", QT_TRANSLATE_NOOP("mitcad::RenderWorkspace", "All")},
    {"metal", QT_TRANSLATE_NOOP("mitcad::RenderWorkspace", "Metal")},
    {"plastic", QT_TRANSLATE_NOOP("mitcad::RenderWorkspace", "Plastic")},
    {"paint", QT_TRANSLATE_NOOP("mitcad::RenderWorkspace", "Paint")},
    {"glass", QT_TRANSLATE_NOOP("mitcad::RenderWorkspace", "Glass")},
    {"wood", QT_TRANSLATE_NOOP("mitcad::RenderWorkspace", "Wood")},
    {"other", QT_TRANSLATE_NOOP("mitcad::RenderWorkspace", "Other")},
}};

bool matchesFilter(const Appearance& a, const QString& filter) {
  const QString id = a.id.toLower();
  const bool paint = id.startsWith(QLatin1String("paint")) || a.name.contains(QLatin1String("Paint"), Qt::CaseInsensitive);
  const bool glass = a.transmission >= 0.5;
  if (filter == QLatin1String("all")) {
    return true;
  }
  if (filter == QLatin1String("paint")) {
    return paint;
  }
  if (filter == QLatin1String("glass")) {
    return glass;
  }
  if (filter == QLatin1String("metal")) {
    return a.materialClass == QLatin1String("metal");
  }
  if (filter == QLatin1String("plastic")) {
    return !glass && (a.materialClass == QLatin1String("plastic") || a.materialClass == QLatin1String("fdm_plastic") ||
                      a.materialClass == QLatin1String("resin"));
  }
  if (filter == QLatin1String("wood")) {
    return a.materialClass == QLatin1String("wood");
  }
  // Other: none of the above.
  return !paint && !glass && a.materialClass != QLatin1String("metal") &&
         a.materialClass != QLatin1String("plastic") && a.materialClass != QLatin1String("fdm_plastic") &&
         a.materialClass != QLatin1String("resin") && a.materialClass != QLatin1String("wood");
}

// The user's favourite and recent appearances (application settings: what
// a person likes to reach, not the design's).
const char* const kFavoritesKey = "render/favoriteAppearances";
const char* const kRecentKey = "render/recentAppearances";
constexpr int kRecentCount = 12;

QStringList savedList(const char* key) { return QSettings().value(QLatin1String(key)).toStringList(); }
void saveList(const char* key, const QStringList& list) { QSettings().setValue(QLatin1String(key), list); }

const QString kDefaultLook = QStringLiteral("default");

// The environment's preset by name, as Render Environment's choice names
// it (Light's list's summary of the Environment row).
QString presetName(const QString& preset) {
  if (preset == QLatin1String("studio_white")) {
    return RenderWorkspace::tr("White Studio");
  }
  if (preset == QLatin1String("studio_dark")) {
    return RenderWorkspace::tr("Dark Studio");
  }
  if (preset == QLatin1String("outdoor")) {
    return RenderWorkspace::tr("Outdoor");
  }
  if (preset == QLatin1String("image")) {
    return RenderWorkspace::tr("HDR Image");
  }
  return RenderWorkspace::tr("Studio");
}

QJsonArray xyzJson(const gp_XYZ& v) { return {v.X(), v.Y(), v.Z()}; }

QJsonObject cameraFields(const CameraState& camera) {
  return {{QStringLiteral("eye"), xyzJson(camera.eye.XYZ())},
          {QStringLiteral("target"), xyzJson(camera.target.XYZ())},
          {QStringLiteral("up"), xyzJson(camera.up.XYZ())},
          {QStringLiteral("perspective"), camera.perspective},
          {QStringLiteral("height"), camera.height}};
}

CameraState cameraOf(const QJsonObject& view) {
  const auto xyz = [](const QJsonValue& v) {
    const QJsonArray a = v.toArray();
    return gp_XYZ(a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble());
  };
  CameraState camera;
  camera.eye = gp_Pnt(xyz(view.value(QStringLiteral("eye"))));
  camera.target = gp_Pnt(xyz(view.value(QStringLiteral("target"))));
  const gp_XYZ up = xyz(view.value(QStringLiteral("up")));
  if (up.Modulus() > 1e-12) {
    camera.up = gp_Dir(up);
  }
  camera.perspective = view.value(QStringLiteral("perspective")).toBool();
  camera.height = view.value(QStringLiteral("height")).toDouble(250.0);
  return camera;
}

// The library's swatches; empty, its empty text in the viewport's top
// left corner in the placeholder color (docs/ui-conventions.md, Lists and
// trees: an empty list says how to fill it), as the Configurations
// window's lists do.
class SwatchList : public QListWidget {
public:
  void setEmptyText(const QString& text) {
    if (text != m_emptyText) {
      m_emptyText = text;
      viewport()->update();
    }
  }

protected:
  void paintEvent(QPaintEvent* event) override {
    QListWidget::paintEvent(event);
    if (count() == 0 && !m_emptyText.isEmpty()) {
      QPainter painter(viewport());
      painter.setPen(palette().color(QPalette::PlaceholderText));
      painter.drawText(viewport()->rect().adjusted(4, 4, -4, -4), Qt::AlignLeft | Qt::AlignTop | Qt::TextWordWrap,
                       m_emptyText);
    }
  }

private:
  QString m_emptyText;
};

QLabel* hint(const QString& text) {
  auto* label = new QLabel(text);
  label->setWordWrap(true);
  setRoleStyleSheet(label, ColorRole::TextSecondary);
  return label;
}

// A side panel's page: its content, scrolling down when longer, never
// sideways. The content is as wide as the page's viewport, never wider:
// the fields (spin boxes, choices, text fields, buttons) may narrow below
// their text's width rather than run under the card's edge or the scroll
// bar, and while the scroll bar shows the content keeps a gap before it as
// the card's margin does at the left. The card's header is its title
// (leftTitle, rightTitle).
QWidget* page(const QString& objectName, QWidget* content) {
  auto* scroll = new QScrollArea;
  scroll->setObjectName(objectName);
  scroll->setWidgetResizable(true);
  scroll->setFrameShape(QFrame::NoFrame);
  scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  auto* holder = new QWidget;
  // An explicit minimum width: the scroll area then makes the holder as
  // wide as its viewport rather than as wide as the content asks.
  holder->setMinimumWidth(1);
  auto* column = new QVBoxLayout(holder);
  column->setContentsMargins(0, 0, 0, 0);
  column->addWidget(content, 1);
  for (QWidget* field : content->findChildren<QWidget*>()) {
    const bool narrows = qobject_cast<QAbstractSpinBox*>(field) != nullptr || qobject_cast<QComboBox*>(field) != nullptr ||
                         qobject_cast<QLineEdit*>(field) != nullptr || qobject_cast<QPushButton*>(field) != nullptr;
    // Not a spin box's or a choice's own text field: its owner places it.
    QWidget* owner = field->parentWidget();
    const bool part = qobject_cast<QAbstractSpinBox*>(owner) != nullptr || qobject_cast<QComboBox*>(owner) != nullptr;
    if (narrows && !part && field->minimumWidth() == 0) {
      field->setMinimumWidth(48);
    }
  }
  scroll->setWidget(holder);
  QObject::connect(scroll->verticalScrollBar(), &QScrollBar::rangeChanged, holder, [column](int minimum, int maximum) {
    column->setContentsMargins(0, 0, maximum > minimum ? 6 : 0, 0);
  });
  return scroll;
}

// The library's filter chips in rows that wrap at the panel's width, so
// that they never make it wider than its card.
class FlowLayout : public QLayout {
public:
  explicit FlowLayout(QWidget* parent, int spacing) : QLayout(parent), m_spacing(spacing) {
    setContentsMargins(0, 0, 0, 0);
  }
  ~FlowLayout() override {
    while (QLayoutItem* item = takeAt(0)) {
      delete item;
    }
  }
  void addItem(QLayoutItem* item) override { m_items.append(item); }
  int count() const override { return static_cast<int>(m_items.size()); }
  QLayoutItem* itemAt(int index) const override { return index >= 0 && index < m_items.size() ? m_items.at(index) : nullptr; }
  QLayoutItem* takeAt(int index) override {
    return index >= 0 && index < m_items.size() ? m_items.takeAt(index) : nullptr;
  }
  Qt::Orientations expandingDirections() const override { return {}; }
  bool hasHeightForWidth() const override { return true; }
  int heightForWidth(int width) const override { return arrange(QRect(0, 0, width, 0), false); }
  void setGeometry(const QRect& rect) override {
    QLayout::setGeometry(rect);
    arrange(rect, true);
  }
  QSize sizeHint() const override { return minimumSize(); }
  QSize minimumSize() const override {
    QSize size(0, 0);
    for (const QLayoutItem* item : m_items) {
      size = size.expandedTo(item->minimumSize());
    }
    const QMargins margins = contentsMargins();
    return size + QSize(margins.left() + margins.right(), margins.top() + margins.bottom());
  }

private:
  // Each item at its size, left to right, a new row where one does not
  // fit; the height they take.
  int arrange(const QRect& rect, bool apply) const {
    const QRect area = rect.marginsRemoved(contentsMargins());
    int x = area.x();
    int y = area.y();
    int line = 0;
    for (QLayoutItem* item : m_items) {
      const QSize hint = item->sizeHint();
      if (x + hint.width() > area.right() + 1 && line > 0) {
        x = area.x();
        y += line + m_spacing;
        line = 0;
      }
      if (apply) {
        item->setGeometry(QRect(QPoint(x, y), hint));
      }
      x += hint.width() + m_spacing;
      line = std::max(line, hint.height());
    }
    return y + line - rect.y() + contentsMargins().bottom();
  }

  QList<QLayoutItem*> m_items;
  int m_spacing;
};

// The fields of a panel in sight that do not show their whole value or
// label (mitcad#2013: three spin boxes beside a label in a card cut their
// values to "-0" and "0.5", two buttons beside a field their texts): a
// spin box narrower than its text, a button narrower than its label, a
// choice narrower than its current entry. Named by the object name, else
// by kind and text.
QStringList cutFields(const QWidget* panel) {
  QStringList cut;
  const auto name = [](const QWidget* widget, const QString& text) {
    return !widget->objectName().isEmpty()
               ? widget->objectName()
               : QStringLiteral("%1 \"%2\"").arg(QString::fromLatin1(widget->metaObject()->className()), text);
  };
  for (const QWidget* widget : panel->findChildren<QWidget*>()) {
    if (!widget->isVisibleTo(panel) || !widget->isVisible() || widget->width() <= 0) {
      continue;
    }
    if (const auto* spin = qobject_cast<const QAbstractSpinBox*>(widget)) {
      // Its own line edit, inside the frame and the buttons.
      const auto* edit = spin->findChild<QLineEdit*>();
      if (edit != nullptr && edit->width() < edit->fontMetrics().horizontalAdvance(edit->text()) + 4) {
        cut << name(spin, edit->text());
      }
    } else if (const auto* button = qobject_cast<const QPushButton*>(widget)) {
      QString text = button->text();
      text.remove(QLatin1Char('&'));
      const int icon = button->icon().isNull() ? 0 : button->iconSize().width() + 4;
      if (!text.isEmpty() && button->width() < button->fontMetrics().horizontalAdvance(text) + icon + 8) {
        cut << name(button, text);
      }
    } else if (const auto* tabs = qobject_cast<const QTabBar*>(widget)) {
      // Every tab's whole name (mitcad#2083: the Appearances panel's scopes
      // were cut to "Libr…" at 240 px).
      for (int i = 0; i < tabs->count(); ++i) {
        if (tabs->isTabVisible(i) && !tabs->tabText(i).isEmpty() &&
            tabs->tabRect(i).width() < tabs->fontMetrics().horizontalAdvance(tabs->tabText(i)) + 8) {
          cut << name(tabs, tabs->tabText(i));
        }
      }
    } else if (const auto* choice = qobject_cast<const QComboBox*>(widget)) {
      // Room for the arrow beside the entry.
      if (!choice->isEditable() && !choice->currentText().isEmpty() &&
          choice->width() < choice->fontMetrics().horizontalAdvance(choice->currentText()) + 24) {
        cut << name(choice, choice->currentText());
      }
    }
  }
  return cut;
}

} // namespace

QString RenderWorkspace::stageName(Stage stage) {
  switch (stage) {
  case Stage::Light:
    return QStringLiteral("Light");
  case Stage::Frame:
    return QStringLiteral("Frame");
  case Stage::Output:
    return QStringLiteral("Output");
  case Stage::Look:
    break;
  }
  return QStringLiteral("Look");
}

RenderWorkspace::RenderWorkspace(DocumentHost& host, OcctViewer& viewer, Hooks hooks, QObject* parent)
    : QObject(parent), m_host(host), m_viewer(viewer), m_hooks(std::move(hooks)), m_chosen(kDefaultLook) {
  buildStageBar();
  m_left = new QStackedWidget;
  m_left->setObjectName(QStringLiteral("renderLeft"));
  m_right = new QStackedWidget;
  m_right->setObjectName(QStringLiteral("renderSettings"));
  buildActions();
  buildLook();
  buildLight();
  buildFrame();
  buildOutput();
  buildBottom();
  // Flat sections, no frames (the left panel's and the stages' settings).
  flattenSections(m_left);
  flattenSections(m_right);
  m_left->setMinimumWidth(240);
  m_right->setMinimumWidth(280);
  // The cards' fields are logged whole or cut as the cards' widths settle
  // (logLayout, mitcad#2013).
  new PlaceWatcher(m_left, [this] { logLayout(); });
  new PlaceWatcher(m_right, [this] { logLayout(); });
  updateTitles();
}

RenderWorkspace::~RenderWorkspace() = default;

QWidget* RenderWorkspace::stageBar() const { return m_stages; }
QWidget* RenderWorkspace::leftPanel() const { return m_left; }
QWidget* RenderWorkspace::rightPanel() const { return m_right; }

QString RenderWorkspace::leftTitle() const { return m_leftTitle; }
QString RenderWorkspace::rightTitle() const { return m_rightTitle; }

void RenderWorkspace::updateTitles() {
  // The stage's list at the left; at the right what is chosen in it.
  QString left;
  QString right;
  switch (m_stage) {
  case Stage::Look:
    left = tr("Appearances");
    right = tr("Look");
    break;
  case Stage::Light: {
    left = tr("Lights");
    right = tr("Environment");
    if (m_lightRow != QLatin1String("environment")) {
      for (const QJsonValue& value : m_settings.value(QStringLiteral("lights")).toArray()) {
        if (value.toObject().value(QStringLiteral("id")).toString() == m_lightRow) {
          right = value.toObject().value(QStringLiteral("name")).toString();
        }
      }
    }
    break;
  }
  case Stage::Frame: {
    left = tr("Shots");
    const QString shot = shotObject(m_shot).value(QStringLiteral("name")).toString();
    right = shot.isEmpty() ? tr("Shot") : shot;
    break;
  }
  case Stage::Output:
    left = tr("Shots to Render");
    right = tr("Output");
    break;
  }
  if (left == m_leftTitle && right == m_rightTitle) {
    return;
  }
  m_leftTitle = left;
  m_rightTitle = right;
  qDebug().noquote() << QStringLiteral("Render panels: %1 | %2").arg(left, right);
  emit titlesChanged();
  // The cards' fields under their new titles (logLayout).
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

// ---------------------------------------------------------------------------
// The stages

void RenderWorkspace::buildStageBar() {
  // The stages as the Render tab's own tab row (the toolbar panel shows it
  // in place of the workspace's tabs), numbered, looking as those tabs do:
  // the shown one filled with the accent, a dim › between them: the
  // steps of the work, in order.
  m_stages = Ribbon::createPanelTabBar(nullptr, true);
  m_stages->setObjectName(QStringLiteral("renderStages"));
  for (const QString& name : {tr("1  Look"), tr("2  Light"), tr("3  Frame"), tr("4  Output")}) {
    m_stages->addTab(name);
  }
  m_stages->setTabToolTip(0, tr("How the bodies look: their appearances"));
  m_stages->setTabToolTip(1, tr("The environment and the lights"));
  m_stages->setTabToolTip(2, tr("The camera: saved views and shots"));
  m_stages->setTabToolTip(3, tr("The image: its size, quality and file, and the render"));
  // Render and Cancel at the row's right, beside the search field.
  m_stageButtons = new QWidget;
  m_stageButtons->setObjectName(QStringLiteral("renderStageButtons"));
  auto* buttons = new QHBoxLayout(m_stageButtons);
  buttons->setContentsMargins(0, 0, 4, 0);
  buttons->setSpacing(6);
  m_render = new CapsuleButton(tr("Render"), true);
  m_render->setObjectName(QStringLiteral("renderRender"));
  m_render->setToolTip(tr("Renders the image of the current shot, or of the view"));
  m_cancel = new CapsuleButton(tr("Cancel"));
  m_cancel->setObjectName(QStringLiteral("renderCancel"));
  m_cancel->setToolTip(tr("Stops the render"));
  m_cancel->setEnabled(false);
  buttons->addWidget(m_render);
  buttons->addWidget(m_cancel);
  connect(m_stages, &QTabBar::currentChanged, this, [this](int index) {
    if (index >= 0) {
      showStage(static_cast<Stage>(index));
    }
  });
  connect(m_render, &QToolButton::clicked, this, &RenderWorkspace::render);
  connect(m_cancel, &QToolButton::clicked, this, &RenderWorkspace::cancel);
  // Their places logged again when the toolbar panel places them.
  new PlaceWatcher(m_stages, [this] { logLayout(); });
  new PlaceWatcher(m_stageButtons, [this] { logLayout(); });
}

void RenderWorkspace::buildActions() {
  // The Render tab's page: the stage's actions as the toolbar's groups
  // (owner, 18.11.2026): icon buttons with their labels under them, a
  // caption with ˅ under each group that opens its menu, a thin line
  // between two groups, as high as Design's groups. The Live group first,
  // then the stage's own groups (shown with their stage).
  m_actions = new QWidget;
  m_actions->setObjectName(QStringLiteral("renderStageActions"));
  auto* row = new QHBoxLayout(m_actions);
  row->setContentsMargins(0, 0, 0, 0);
  row->setSpacing(0);
  const auto action = [this](const QString& icon, const QString& text, const QString& tip) {
    auto* made = new QAction(themeIcon(icon), text, this);
    made->setToolTip(tip);
    return made;
  };
  const auto button = [](QAction* command, const QString& name, const QString& shortLabel = QString(),
                         bool menuArrow = false) {
    QToolButton* made = Ribbon::createContentButton(command, shortLabel, menuArrow);
    made->setObjectName(name);
    return made;
  };
  // A stage's groups in a holder of their own, a line between two groups.
  const auto holder = [](const QList<QWidget*>& groups) {
    auto* made = new QWidget;
    auto* line = new QHBoxLayout(made);
    line->setContentsMargins(0, 0, 0, 0);
    line->setSpacing(0);
    for (int i = 0; i < groups.size(); ++i) {
      if (i > 0) {
        line->addWidget(Ribbon::createContentSeparator());
      }
      line->addWidget(groups.at(i));
    }
    return made;
  };

  // Live: Pause or Resume (one button, its icon and label following), the
  // render method with its choices; the caption is the live view's status.
  m_pauseAction = action(QStringLiteral("render-pause"), tr("Pause"), tr("The live view stops refining; Resume goes on"));
  m_pause = button(m_pauseAction, QStringLiteral("renderPauseLive"));
  m_methodAuto = new QAction(tr("GPU where available"), this);
  m_methodAuto->setCheckable(true);
  m_methodAuto->setData(QStringLiteral("auto"));
  m_methodCpu = new QAction(tr("CPU"), this);
  m_methodCpu->setCheckable(true);
  m_methodCpu->setData(QStringLiteral("cpu"));
  auto* methods = new QActionGroup(this);
  methods->setExclusive(true);
  methods->addAction(m_methodAuto);
  methods->addAction(m_methodCpu);
  m_methodAction = action(QStringLiteral("render-method"), tr("GPU"),
                          tr("What renders: a graphics card where one can, else the processor"));
  auto* methodMenu = new QMenu(m_actions);
  methodMenu->addAction(m_methodAuto);
  methodMenu->addAction(m_methodCpu);
  m_methodAction->setMenu(methodMenu);
  m_method = button(m_methodAction, QStringLiteral("renderMethod"), QString(), true);
  auto* liveMenu = new QMenu(m_actions);
  liveMenu->addAction(m_pauseAction);
  liveMenu->addSeparator();
  liveMenu->addAction(m_methodAuto);
  liveMenu->addAction(m_methodCpu);
  m_liveGroup = Ribbon::createContentGroup(QStringLiteral("renderLive"), tr("Live"), {m_pause, m_method}, liveMenu);
  row->addWidget(m_liveGroup);

  // Look: the Pick group (what a pick takes: one of three) and the Look
  // group (Paint, Pick Up).
  auto* modes = new QActionGroup(this);
  modes->setExclusive(true);
  struct Mode {
    PickMode mode;
    const char* key;
    const char* icon;
    QString text;
    QString tip;
  };
  const std::array<Mode, 3> pickModes = {{
      {PickMode::Body, "body", "body", tr("Body"), tr("A pick in the view takes a whole body")},
      {PickMode::Face, "face", "pick-face", tr("Face"), tr("A pick in the view takes one face of a body")},
      {PickMode::Component, "component", "component", tr("Component"), tr("A pick in the view takes a component's bodies")},
  }};
  auto* pickMenu = new QMenu(m_actions);
  QList<QToolButton*> pickButtons;
  for (const Mode& mode : pickModes) {
    QAction* pick = action(QLatin1String(mode.icon), mode.text, mode.tip);
    pick->setCheckable(true);
    pick->setChecked(mode.mode == PickMode::Body);
    modes->addAction(pick);
    pickMenu->addAction(pick);
    m_modes.append(pick);
    m_modeButtons.append(button(pick, QStringLiteral("renderPick_%1").arg(QLatin1String(mode.key))));
    pickButtons.append(m_modeButtons.constLast());
    const PickMode chosen = mode.mode;
    connect(pick, &QAction::triggered, this, [this, chosen] { setPickMode(chosen); });
  }
  QWidget* pickGroup = Ribbon::createContentGroup(QStringLiteral("renderPick"), tr("Pick"), pickButtons, pickMenu);
  m_paint = action(QStringLiteral("render-paint"), tr("Paint"), tr("Each pick in the view takes the chosen appearance"));
  m_paint->setCheckable(true);
  m_pickUp = action(QStringLiteral("render-pick-up"), tr("Pick Up"),
                    tr("The next pick in the view gives its appearance to paint others with"));
  m_pickUp->setCheckable(true);
  m_paintButton = button(m_paint, QStringLiteral("renderPaint"));
  m_pickUpButton = button(m_pickUp, QStringLiteral("renderPickUp"));
  auto* lookMenu = new QMenu(m_actions);
  lookMenu->addAction(m_paint);
  lookMenu->addAction(m_pickUp);
  QWidget* lookGroup =
      Ribbon::createContentGroup(QStringLiteral("renderLook"), tr("Look"), {m_paintButton, m_pickUpButton}, lookMenu);
  m_lookActions = holder({pickGroup, lookGroup});

  // Light: Add Light (its kinds in its menu), Reset Environment.
  QAction* addLight =
      action(QStringLiteral("render-add-light"), tr("Add Light"), tr("Adds a light at the camera, shining where it looks"));
  auto* kinds = new QMenu(m_actions);
  for (const QString& type : {QStringLiteral("point"), QStringLiteral("spot"), QStringLiteral("area"), QStringLiteral("sun")}) {
    kinds->addAction(renderLightTypeName(type), this, [this, type] { m_environment->addLight(type); });
  }
  addLight->setMenu(kinds);
  m_addLight = button(addLight, QStringLiteral("renderAddLight"), QString(), true);
  QAction* reset = action(QStringLiteral("render-reset-environment"), tr("Reset Environment"),
                          tr("The default environment: the studio, shadows on the ground"));
  m_resetEnvironment = button(reset, QStringLiteral("renderResetEnvironment"), tr("Reset"));
  auto* lightMenu = new QMenu(m_actions);
  lightMenu->addAction(addLight);
  lightMenu->addAction(reset);
  m_lightActions = holder({Ribbon::createContentGroup(QStringLiteral("renderLight"), tr("Light"),
                                                      {m_addLight, m_resetEnvironment}, lightMenu)});
  connect(reset, &QAction::triggered, this, [this] { m_environment->reset(); });

  // Frame: Save Shot, Update Shot to View, Look Through (the last two for
  // the chosen shot).
  QAction* save =
      action(QStringLiteral("render-save-shot"), tr("Save Shot"), tr("Saves the view's camera as a shot of the design"));
  QAction* update = action(QStringLiteral("render-update-shot"), tr("Update Shot to View"),
                           tr("The chosen shot takes the view's camera as it is now"));
  QAction* through =
      action(QStringLiteral("render-look-through"), tr("Look Through"), tr("The view takes the chosen shot's camera"));
  update->setEnabled(false);
  through->setEnabled(false);
  m_saveShot = button(save, QStringLiteral("renderSaveShot"));
  m_updateShot = button(update, QStringLiteral("renderUpdateShot"), tr("Update"));
  m_lookThrough = button(through, QStringLiteral("renderLookThrough"));
  auto* frameMenu = new QMenu(m_actions);
  frameMenu->addAction(save);
  frameMenu->addAction(update);
  frameMenu->addAction(through);
  m_frameActions = holder({Ribbon::createContentGroup(QStringLiteral("renderFrame"), tr("Frame"),
                                                      {m_saveShot, m_updateShot, m_lookThrough}, frameMenu)});
  connect(save, &QAction::triggered, this, &RenderWorkspace::saveShot);
  connect(update, &QAction::triggered, this, &RenderWorkspace::updateShotToView);
  connect(through, &QAction::triggered, this, [this] { lookThrough(m_shot); });

  // Output: Check Now, Render All Shots (which checks first too).
  QAction* check = action(QStringLiteral("render-check"), tr("Check Now"),
                          tr("Finds hidden bodies, bodies without an appearance, a missing image or folder"));
  QAction* renderAll = action(QStringLiteral("render-all-shots"), tr("Render All Shots"),
                              tr("Renders each shot one after the other and saves its image in the folder"));
  m_checkNow = button(check, QStringLiteral("renderCheckNow"), tr("Check"));
  m_renderAll = button(renderAll, QStringLiteral("renderAllShots"), tr("Render All"));
  auto* outputMenu = new QMenu(m_actions);
  outputMenu->addAction(check);
  outputMenu->addAction(renderAll);
  m_outputActions = holder({Ribbon::createContentGroup(QStringLiteral("renderOutput"), tr("Output"),
                                                       {m_checkNow, m_renderAll}, outputMenu)});
  connect(check, &QAction::triggered, this, [this] { checkReady(false); });
  connect(renderAll, &QAction::triggered, this, [this] { m_output->renderAllShots(); });

  row->addWidget(Ribbon::createContentSeparator());
  for (QWidget* stage : {m_lookActions, m_lightActions, m_frameActions, m_outputActions}) {
    row->addWidget(stage);
  }
  row->addStretch(1);

  connect(m_pauseAction, &QAction::triggered, this, [this] { setPaused(!m_userPaused); });
  connect(methods, &QActionGroup::triggered, this, [this](QAction* chosen) {
    const QString choice = chosen->data().toString();
    if (choice == render::renderDeviceChoice() ||
        (choice == QLatin1String("auto") && render::renderDeviceChoice() != QLatin1String("cpu"))) {
      return;
    }
    render::setRenderDeviceChoice(choice);
    qInfo().noquote() << QStringLiteral("Render method %1").arg(choice);
    updateLive();
    if (m_hooks.deviceChanged) {
      m_hooks.deviceChanged();
    }
  });
  new PlaceWatcher(m_actions, [this] { logLayout(); });
}

void RenderWorkspace::showStage(Stage stage) {
  const bool changed = stage != m_stage;
  m_stage = stage;
  if (m_stages->currentIndex() != static_cast<int>(stage)) {
    const QSignalBlocker block(m_stages);
    m_stages->setCurrentIndex(static_cast<int>(stage));
  }
  // Each stage its own list and card.
  switch (stage) {
  case Stage::Look:
    m_left->setCurrentWidget(m_library);
    m_right->setCurrentWidget(m_lookSettings);
    showPicked();
    break;
  case Stage::Light:
    m_left->setCurrentWidget(m_lightsPanel);
    m_right->setCurrentWidget(m_lightSettings);
    lightRowChosen(m_lightRow);
    break;
  case Stage::Frame:
    m_left->setCurrentWidget(m_shotsPanel);
    m_right->setCurrentWidget(m_frameSettings);
    break;
  case Stage::Output:
    m_left->setCurrentWidget(m_outputPanel);
    m_right->setCurrentWidget(m_outputSettings);
    break;
  }
  // The stage's own groups after the Live group.
  m_lookActions->setVisible(stage == Stage::Look);
  m_lightActions->setVisible(stage == Stage::Light);
  m_frameActions->setVisible(stage == Stage::Frame);
  m_outputActions->setVisible(stage == Stage::Output);
  if (stage != Stage::Look) {
    // Paint and Pick Up are Look's.
    m_paint->setChecked(false);
    m_pickUp->setChecked(false);
  }
  updateTitles();
  if (changed || m_open) {
    qInfo().noquote() << QStringLiteral("Render stage %1").arg(stageName(stage));
  }
  if (m_hooks.pickFilterChanged) {
    m_hooks.pickFilterChanged();
  }
  emit stageChanged();
  m_logged.clear();
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void RenderWorkspace::setOpen(bool open) {
  if (open == m_open) {
    return;
  }
  m_open = open;
  if (open) {
    connectRenderMode();
    qInfo().noquote() << "Render workspace opened";
    refresh();
    showStage(m_stage);
  } else {
    if (m_output->isRendering()) {
      m_output->cancelRender();
    }
    m_output->clearViewImage();
    m_paint->setChecked(false);
    m_pickUp->setChecked(false);
    if (m_renderMode) {
      m_renderMode->setPaused(false);
    }
    m_userPaused = false;
    qInfo().noquote() << "Render workspace closed";
    if (m_hooks.pickFilterChanged) {
      m_hooks.pickFilterChanged();
    }
  }
  updateLive();
}

void RenderWorkspace::setDocument(const QString& folder, const QString& name) {
  m_documentFolder = folder;
  m_documentName = name;
  m_output->setDocument(folder, name);
}

void RenderWorkspace::refresh() {
  if (!m_open) {
    return;
  }
  try {
    m_settings = m_host.model().queryObject({{QStringLiteral("query"), QStringLiteral("render_settings")}});
    m_bodies = m_host.model().queryArray(QStringLiteral("bodies"));
    m_appearances = appearancesOf(m_host.model().queryArray(QStringLiteral("appearances")));
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Render workspace: the model was not read: %1").arg(e.what());
    return;
  }
  // A shot that is gone (undo) is no longer the current one.
  if (!m_shot.isEmpty() && shotObject(m_shot).isEmpty()) {
    m_shot.clear();
  }
  fillLibrary();
  showChosen();
  m_environment->refresh();
  m_output->refresh();
  refreshLightList();
  refreshShots();
  updateTitles();
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

bool RenderWorkspace::runCommands(const QJsonArray& commands, const QString& label) {
  if (!m_host.canChangeModel()) {
    m_host.showStatus(m_host.changeBlockedReason(), NoticeLevel::Warning);
    return false;
  }
  const bool ok = commands.size() == 1 ? m_host.runModelCommand(commands.first().toObject())
                                       : m_host.runModelCommands(commands, label);
  if (!ok && !m_host.lastCancelled()) {
    m_host.showStatus(mitcad::modelMessage(m_host.lastError()), NoticeLevel::Error);
  }
  refresh();
  return ok;
}

QString RenderWorkspace::bodyName(const QString& uid) const {
  for (const QJsonValue& value : m_bodies) {
    if (value.toObject().value(QStringLiteral("uid")).toString() == uid) {
      return value.toObject().value(QStringLiteral("name")).toString();
    }
  }
  return tr("Body");
}

// ---------------------------------------------------------------------------
// 1 Look

void RenderWorkspace::buildLook() {
  // The Appearances library at the left.
  auto* library = new QWidget;
  auto* column = new QVBoxLayout(library);
  column->setContentsMargins(0, 0, 0, 0);
  m_search = new QLineEdit;
  m_search->setObjectName(QStringLiteral("renderAppearanceSearch"));
  m_search->setPlaceholderText(tr("Search appearances"));
  m_search->setClearButtonEnabled(true);
  column->addWidget(m_search);
  // Which appearances the list shows, as a drop-down choice: four whole
  // names do not fit a row of tabs at the panel's 240 px (mitcad#2083,
  // docs/ui-conventions.md "Choices in a narrow card").
  m_scope = new QComboBox;
  m_scope->setObjectName(QStringLiteral("renderAppearanceScope"));
  m_scope->setToolTip(tr("Which appearances the list shows"));
  m_scope->addItems({tr("In Design"), tr("Library"), tr("Recent"), tr("Favorites")});
  m_scope->setCurrentIndex(1);
  column->addWidget(m_scope);
  // The classes as chips in rows that wrap at the card's width.
  auto* filters = new QWidget;
  auto* filterRow = new FlowLayout(filters, 2);
  m_filters = new QButtonGroup(this);
  m_filters->setExclusive(true);
  for (std::size_t i = 0; i < kFilters.size(); ++i) {
    auto* button = new QToolButton;
    button->setObjectName(QStringLiteral("renderFilter_%1").arg(QLatin1String(kFilters[i].key)));
    button->setText(tr(kFilters[i].name));
    button->setCheckable(true);
    button->setAutoRaise(true);
    button->setChecked(i == 0);
    button->setProperty("filter", QLatin1String(kFilters[i].key));
    m_filters->addButton(button, static_cast<int>(i));
    filterRow->addWidget(button);
  }
  column->addWidget(filters);
  m_swatches = new SwatchList;
  m_swatches->setObjectName(QStringLiteral("renderAppearances"));
  m_swatches->setViewMode(QListView::IconMode);
  m_swatches->setIconSize(QSize(48, 48));
  m_swatches->setGridSize(QSize(76, 82));
  m_swatches->setResizeMode(QListView::Adjust);
  m_swatches->setMovement(QListView::Static);
  m_swatches->setWordWrap(true);
  m_swatches->setUniformItemSizes(true);
  m_swatches->setContextMenuPolicy(Qt::CustomContextMenu);
  m_swatches->setMinimumHeight(260);
  // The columns that fit the card, never a sideways scroll bar.
  m_swatches->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  m_swatches->setFrameShape(QFrame::NoFrame);
  m_swatches->setTextElideMode(Qt::ElideRight);
  column->addWidget(m_swatches, 1);
  // A double-click is not seen on the list: it says so (a click gives the
  // appearance to the selection, which shows at once).
  column->addWidget(hint(tr("Double-click an appearance to paint with it.")));
  m_library = page(QStringLiteral("renderLibraryPage"), library);
  m_left->addWidget(m_library);
  connect(m_search, &QLineEdit::textChanged, this, [this] { fillLibrary(); });
  connect(m_scope, &QComboBox::currentIndexChanged, this, [this](int index) {
    qInfo().noquote() << QStringLiteral("Render appearances scope %1").arg(index);
    fillLibrary();
  });
  connect(m_filters, &QButtonGroup::idClicked, this, [this](int index) {
    qDebug().noquote() << QStringLiteral("Render appearances filter %1")
                              .arg(QLatin1String(kFilters[static_cast<std::size_t>(index)].key));
    fillLibrary();
  });
  // After the click's own handling: the model command fills the list anew.
  connect(m_swatches, &QListWidget::itemClicked, this, [this](QListWidgetItem* item) {
    const QString id = item->data(Qt::UserRole).toString();
    TestSync::singleShot(0, this, [this, id] { chooseAppearance(id, true); });
  });
  connect(m_swatches, &QListWidget::itemDoubleClicked, this, [this](QListWidgetItem* item) {
    const QString id = item->data(Qt::UserRole).toString();
    TestSync::singleShot(0, this, [this, id] {
      chooseAppearance(id, false);
      m_paint->setChecked(true);
    });
  });
  connect(m_swatches, &QListWidget::customContextMenuRequested, this, [this](const QPoint& at) {
    QListWidgetItem* item = m_swatches->itemAt(at);
    if (item == nullptr) {
      return;
    }
    const QString id = item->data(Qt::UserRole).toString();
    const bool favorite = savedList(kFavoritesKey).contains(id);
    QMenu menu(m_swatches);
    QAction* toggle = menu.addAction(favorite ? tr("Remove from Favorites") : tr("Add to Favorites"));
    if (menu.exec(m_swatches->viewport()->mapToGlobal(at)) == toggle) {
      setFavorite(id, !favorite);
    }
  });

  // Look's settings at the right: the chosen appearance, given to the
  // selection or edited (the pick mode, Paint and Pick Up are the Pick and
  // Look groups').
  auto* settings = new QWidget;
  auto* form = new QVBoxLayout(settings);
  form->setContentsMargins(0, 0, 0, 0);

  auto* chosenBox = new QGroupBox(tr("Appearance"));
  auto* chosenColumn = new QVBoxLayout(chosenBox);
  // What is picked in the view, whose appearance the swatch shows.
  m_pickedName = new QLabel;
  m_pickedName->setObjectName(QStringLiteral("renderLookPicked"));
  m_pickedName->setWordWrap(true);
  setRoleStyleSheet(m_pickedName, ColorRole::TextSecondary);
  chosenColumn->addWidget(m_pickedName);
  auto* chosenRow = new QHBoxLayout;
  m_chosenSwatch = new QLabel;
  m_chosenSwatch->setFixedSize(56, 56);
  m_chosenName = new QLabel;
  m_chosenName->setWordWrap(true);
  chosenRow->addWidget(m_chosenSwatch);
  chosenRow->addWidget(m_chosenName, 1);
  chosenColumn->addLayout(chosenRow);
  m_apply = new QPushButton(tr("Apply to Selection"));
  m_apply->setToolTip(tr("The chosen appearance for the selected bodies, faces or components"));
  chosenColumn->addWidget(m_apply);
  m_editAppearances = new QPushButton(tr("Edit Appearances…"));
  m_editAppearances->setToolTip(tr("The library's and the design's appearances and their parameters"));
  chosenColumn->addWidget(m_editAppearances);
  form->addWidget(chosenBox);
  m_lookMessage = hint(tr("Pick in the view what the appearance is for, then click an appearance in the library."));
  form->addWidget(m_lookMessage);
  form->addStretch(1);
  m_lookSettings = page(QStringLiteral("renderLookPage"), settings);
  m_right->addWidget(m_lookSettings);
  connect(m_paint, &QAction::toggled, this, [this](bool on) {
    if (on) {
      m_pickUp->setChecked(false);
      m_lookMessage->setText(tr("Click in the view to paint with the chosen appearance."));
    }
    qInfo().noquote() << QStringLiteral("Render look paint %1").arg(on ? "on" : "off");
  });
  connect(m_pickUp, &QAction::toggled, this, [this](bool on) {
    if (on) {
      m_paint->setChecked(false);
      m_lookMessage->setText(tr("Click in the view on what has the appearance to pick up."));
    }
    qInfo().noquote() << QStringLiteral("Render look pick up %1").arg(on ? "on" : "off");
  });
  connect(m_apply, &QPushButton::clicked, this, [this] {
    if (m_hooks.selection) {
      applyAppearance(m_hooks.selection(), m_chosen);
    }
  });
  connect(m_editAppearances, &QPushButton::clicked, this, [this] {
    if (m_hooks.runCommand) {
      m_hooks.runCommand(QStringLiteral("solid.appearances"));
    }
  });
}

SelectFilter RenderWorkspace::pickFilter() const {
  switch (m_mode) {
  case PickMode::Face:
    return SelectKind::Face;
  case PickMode::Component:
    return SelectKind::Component;
  case PickMode::Body:
    break;
  }
  return SelectKind::Body;
}

void RenderWorkspace::setPickMode(PickMode mode) {
  m_mode = mode;
  if (QAction* action = m_modes.value(static_cast<int>(mode)); action != nullptr && !action->isChecked()) {
    action->setChecked(true);
  }
  static const char* const names[] = {"body", "face", "component"};
  qInfo().noquote() << QStringLiteral("Render look picks %1").arg(QLatin1String(names[static_cast<int>(mode)]));
  // What was selected of another kind goes: the picks are of this one now.
  if (m_hooks.select) {
    m_hooks.select(Selection());
  }
  if (m_hooks.pickFilterChanged) {
    m_hooks.pickFilterChanged();
  }
}

QIcon RenderWorkspace::swatch(const Appearance& appearance) {
  auto found = m_swatchCache.find(appearance.id);
  if (found != m_swatchCache.end() && found->first.sameLook(appearance)) {
    return found->second;
  }
  const qreal ratio = m_swatches->devicePixelRatioF();
  const QIcon icon(QPixmap::fromImage(appearanceSwatch(appearance, 48, ratio)));
  m_swatchCache.insert(appearance.id, {appearance, icon});
  return icon;
}

void RenderWorkspace::fillLibrary() {
  const QString search = m_search->text().trimmed();
  const int scope = m_scope->currentIndex();
  const QString filter =
      m_filters->checkedButton() != nullptr ? m_filters->checkedButton()->property("filter").toString() : QString();
  const QStringList favorites = savedList(kFavoritesKey);
  const QStringList recent = savedList(kRecentKey);
  QVector<const Appearance*> shown;
  // The default look first, where the scope lists everything.
  for (const Appearance& a : std::as_const(m_appearances)) {
    const bool inDesign = !a.library || !a.bodies.isEmpty() || !a.faces.isEmpty();
    if ((scope == 0 && !inDesign) || (scope == 2 && !recent.contains(a.id)) || (scope == 3 && !favorites.contains(a.id))) {
      continue;
    }
    if (!search.isEmpty() && !a.name.contains(search, Qt::CaseInsensitive) &&
        !a.id.contains(search, Qt::CaseInsensitive)) {
      continue;
    }
    if (!filter.isEmpty() && !matchesFilter(a, filter)) {
      continue;
    }
    shown.append(&a);
  }
  if (scope == 2) {
    // The most recent first.
    std::stable_sort(shown.begin(), shown.end(), [&recent](const Appearance* x, const Appearance* y) {
      return recent.indexOf(x->id) < recent.indexOf(y->id);
    });
  }
  const QSignalBlocker block(m_swatches);
  m_swatches->clear();
  if (scope == 1 && (filter.isEmpty() || filter == QLatin1String("all")) && search.isEmpty()) {
    auto* item = new QListWidgetItem(tr("Default"));
    item->setData(Qt::UserRole, kDefaultLook);
    item->setToolTip(tr("The default look: the body's material's, or plain gray"));
    item->setIcon(themeIcon(QStringLiteral("appearance")));
    m_swatches->addItem(item);
  }
  for (const Appearance* a : std::as_const(shown)) {
    auto* item = new QListWidgetItem(swatch(*a), a->name);
    item->setData(Qt::UserRole, a->id);
    item->setToolTip(favorites.contains(a->id) ? tr("%1 (a favorite)").arg(a->name) : a->name);
    m_swatches->addItem(item);
    if (a->id == m_chosen) {
      item->setSelected(true);
    }
  }
  // Favorites without any: how to add one, which a right-click does
  // unseen (mitcad#2083).
  const QString empty =
      scope == 3 && favorites.isEmpty() ? tr("No favorites yet. Right-click an appearance to add it to Favorites.")
                                        : QString();
  static_cast<SwatchList*>(m_swatches)->setEmptyText(empty);
  if (empty != m_swatches->property("loggedEmpty").toString()) {
    m_swatches->setProperty("loggedEmpty", empty);
    if (!empty.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Render appearances empty: %1").arg(empty);
    }
  }
  const QString line = QStringLiteral("Render appearances shown: %1").arg(m_swatches->count());
  if (line != m_swatches->property("logged").toString()) {
    m_swatches->setProperty("logged", line);
    qDebug().noquote() << line;
  }
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void RenderWorkspace::showChosen() {
  const Appearance* a = m_chosen == kDefaultLook ? nullptr : findAppearance(m_appearances, m_chosen);
  if (a == nullptr) {
    m_chosen = kDefaultLook;
    m_chosenName->setText(tr("Default"));
    m_chosenSwatch->setPixmap(themeIcon(QStringLiteral("appearance")).pixmap(48, 48));
    return;
  }
  m_chosenName->setText(a->name);
  m_chosenSwatch->setPixmap(swatch(*a).pixmap(48, 48));
}

void RenderWorkspace::chooseAppearance(const QString& id, bool apply) {
  m_chosen = id.isEmpty() ? kDefaultLook : id;
  showChosen();
  qInfo().noquote() << QStringLiteral("Render look chosen %1").arg(m_chosen);
  if (apply && m_hooks.selection) {
    const Selection picked = m_hooks.selection();
    if (!picked.isEmpty()) {
      applyAppearance(picked, m_chosen);
    }
  }
}

void RenderWorkspace::setFavorite(const QString& id, bool favorite) {
  QStringList list = savedList(kFavoritesKey);
  list.removeAll(id);
  if (favorite) {
    list.append(id);
  }
  saveList(kFavoritesKey, list);
  qInfo().noquote() << QStringLiteral("Render appearance %1 %2 the favorites").arg(id, favorite ? "added to" : "removed from");
  fillLibrary();
}

void RenderWorkspace::applyAppearance(const Selection& items, const QString& id) {
  const QJsonValue look = id == kDefaultLook || id.isEmpty() ? QJsonValue() : QJsonValue(id);
  QJsonArray commands;
  QStringList bodies;
  QHash<QString, QJsonArray> faces;
  for (const SelectionItem& item : items) {
    if (item.kind == SelectKind::Body && !bodies.contains(item.owner)) {
      bodies << item.owner;
    } else if (item.kind == SelectKind::Face) {
      faces[item.owner].append(item.name);
    } else if (item.kind == SelectKind::Component) {
      // Every body of the component (its own, in every place it is).
      for (const QJsonValue& value : std::as_const(m_bodies)) {
        const QJsonObject body = value.toObject();
        const QString component = body.value(QStringLiteral("component")).toString();
        const QString uid = body.value(QStringLiteral("uid")).toString();
        if (component == item.owner && !bodies.contains(uid)) {
          bodies << uid;
        }
      }
    }
  }
  for (const QString& uid : std::as_const(bodies)) {
    commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("set_body_appearance")},
                                {QStringLiteral("uid"), uid},
                                {QStringLiteral("appearance"), look}});
  }
  for (auto it = faces.cbegin(); it != faces.cend(); ++it) {
    commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("set_face_appearance")},
                                {QStringLiteral("uid"), it.key()},
                                {QStringLiteral("faces"), it.value()},
                                {QStringLiteral("appearance"), look}});
  }
  if (commands.isEmpty()) {
    m_lookMessage->setText(tr("Pick a body, a face or a component in the view first."));
    return;
  }
  int faceCount = 0;
  for (const QJsonArray& names : std::as_const(faces)) {
    faceCount += static_cast<int>(names.size());
  }
  const Appearance* a = findAppearance(m_appearances, id);
  const QString name = a != nullptr ? a->name : tr("Default");
  if (!runCommands(commands, tr("Paint %1").arg(name))) {
    return;
  }
  qInfo().noquote() << QStringLiteral("Render look %1 on %2 body(ies), %3 face(s)")
                           .arg(id)
                           .arg(bodies.size())
                           .arg(faceCount);
  m_lookMessage->setText(tr("%1 given.").arg(name));
  if (id != kDefaultLook) {
    QStringList recent = savedList(kRecentKey);
    recent.removeAll(id);
    recent.prepend(id);
    while (recent.size() > kRecentCount) {
      recent.removeLast();
    }
    saveList(kRecentKey, recent);
  }
}

QString RenderWorkspace::appearanceOfPick(const SelectionItem& item) const {
  const auto ofBody = [this](const QString& uid, const QString& face) {
    for (const QJsonValue& value : m_bodies) {
      const QJsonObject body = value.toObject();
      if (body.value(QStringLiteral("uid")).toString() != uid) {
        continue;
      }
      if (!face.isEmpty()) {
        for (const QJsonValue& own : body.value(QStringLiteral("face_appearances")).toArray()) {
          const QJsonObject f = own.toObject();
          if (f.value(QStringLiteral("face")).toString() == face ||
              f.value(QStringLiteral("faces")).toArray().contains(QJsonValue(face))) {
            return f.value(QStringLiteral("appearance")).toString();
          }
        }
      }
      return body.value(QStringLiteral("appearance")).toString();
    }
    return QString();
  };
  if (item.kind == SelectKind::Component) {
    for (const QJsonValue& value : m_bodies) {
      const QJsonObject body = value.toObject();
      if (body.value(QStringLiteral("component")).toString() == item.owner) {
        return body.value(QStringLiteral("appearance")).toString();
      }
    }
    return QString();
  }
  return ofBody(item.owner, item.kind == SelectKind::Face ? item.name : QString());
}

bool RenderWorkspace::takePick(const Selection& items) {
  if (!takesPicks() || (!m_paint->isChecked() && !m_pickUp->isChecked())) {
    return false;
  }
  if (items.isEmpty()) {
    return true; // a click beside the bodies paints nothing
  }
  if (m_pickUp->isChecked()) {
    const QString found = appearanceOfPick(items.first());
    chooseAppearance(found.isEmpty() ? kDefaultLook : found, false);
    qInfo().noquote() << QStringLiteral("Render look picked up %1").arg(m_chosen);
    m_paint->setChecked(true); // Pick Up then paints with it
    return true;
  }
  applyAppearance(Selection{items.first()}, m_chosen);
  return true;
}

void RenderWorkspace::selectionChanged() {
  if (!m_open || m_stage != Stage::Look) {
    return;
  }
  showPicked();
}

void RenderWorkspace::showPicked() {
  const Selection picked = m_hooks.selection ? m_hooks.selection() : Selection();
  if (picked.isEmpty()) {
    m_pickedName->setText(tr("Nothing picked: pick a body, a face or a component in the view."));
    showChosen();
    return;
  }
  // The first pick's name and appearance (a face's own where it has one).
  const SelectionItem& first = picked.first();
  QString name = bodyName(first.owner);
  if (first.kind == SelectKind::Face) {
    name = tr("A face of %1").arg(name);
  } else if (first.kind == SelectKind::Component) {
    name = tr("A component");
    for (const QJsonValue& value : std::as_const(m_bodies)) {
      const QJsonObject body = value.toObject();
      if (body.value(QStringLiteral("component")).toString() == first.owner) {
        name = body.value(QStringLiteral("component_name")).toString(name);
        break;
      }
    }
  }
  if (picked.size() > 1) {
    name = tr("%1 and %n more", nullptr, static_cast<int>(picked.size()) - 1).arg(name);
  }
  m_pickedName->setText(name);
  const QString look = appearanceOfPick(first);
  m_chosen = look.isEmpty() ? kDefaultLook : look;
  showChosen();
  qInfo().noquote() << QStringLiteral("Render look for %1: %2").arg(name, m_chosen);
}

void RenderWorkspace::refreshLightList() {
  // Made anew with the chosen row and the scroll position kept: a model
  // change (a light added, turned off, renamed) never moves the list.
  m_listBuilding = true;
  const int scrolled = m_lightList->verticalScrollBar()->value();
  m_lightList->clear();
  const QJsonObject environment = m_settings.value(QStringLiteral("environment")).toObject();
  const QString preset = environment.value(QStringLiteral("preset")).toString(QStringLiteral("studio"));
  QString summary = presetName(preset);
  if (preset == QLatin1String("image")) {
    const QString image = environment.value(QStringLiteral("image")).toString();
    if (!image.isEmpty()) {
      summary = QFileInfo(image).fileName();
    }
  }
  auto* env = new QTreeWidgetItem(m_lightList, {tr("Environment"), summary});
  env->setData(0, Qt::UserRole, QStringLiteral("environment"));
  env->setIcon(0, themeIcon(QStringLiteral("environment")));
  env->setForeground(1, themeColor(ColorRole::TextSecondary));
  env->setToolTip(0, tr("The light all around the design, its background and its ground"));
  QTreeWidgetItem* current = env;
  for (const QJsonValue& value : m_settings.value(QStringLiteral("lights")).toArray()) {
    const QJsonObject light = value.toObject();
    const QString id = light.value(QStringLiteral("id")).toString();
    const bool on = light.value(QStringLiteral("enabled")).toBool(true);
    auto* item = new QTreeWidgetItem(m_lightList, {light.value(QStringLiteral("name")).toString()});
    item->setData(0, Qt::UserRole, id);
    item->setData(1, Qt::UserRole, on);
    item->setIcon(1, themeIcon(on ? QStringLiteral("eye") : QStringLiteral("eye-off")));
    const QString type = renderLightTypeName(light.value(QStringLiteral("type")).toString());
    item->setToolTip(0, tr("A %1: its place, power and color").arg(type));
    item->setToolTip(1, on ? tr("On: click to turn the light off (it is kept)") : tr("Off: click to turn the light on"));
    if (!on) {
      item->setForeground(0, themeColor(ColorRole::TextSecondary));
    }
    if (id == m_lightRow) {
      current = item;
    }
  }
  // The chosen light gone (undo, deleted): the Environment again.
  const bool lost = current == env && m_lightRow != QLatin1String("environment");
  m_lightList->setCurrentItem(current);
  m_lightList->verticalScrollBar()->setValue(scrolled);
  m_listBuilding = false;
  if (lost) {
    lightRowChosen(QStringLiteral("environment"));
  }
}

void RenderWorkspace::lightRowChosen(const QString& row) {
  const bool changed = row != m_lightRow;
  m_lightRow = row.isEmpty() ? QStringLiteral("environment") : row;
  if (m_lightRow == QLatin1String("environment")) {
    m_environment->setPart(RenderEnvironmentPanel::Part::Environment);
  } else {
    m_environment->setPart(RenderEnvironmentPanel::Part::Lights);
    m_environment->chooseLight(m_lightRow);
  }
  // The list follows (a light chosen through the view's markers or added).
  for (int i = 0; i < m_lightList->topLevelItemCount(); ++i) {
    QTreeWidgetItem* item = m_lightList->topLevelItem(i);
    if (item->data(0, Qt::UserRole).toString() == m_lightRow && m_lightList->currentItem() != item) {
      const QSignalBlocker block(m_lightList);
      m_lightList->setCurrentItem(item);
    }
  }
  if (changed) {
    qInfo().noquote() << QStringLiteral("Render light row %1").arg(m_lightRow);
  }
  updateTitles();
  m_logged.clear();
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

// ---------------------------------------------------------------------------
// 2 Light

void RenderWorkspace::buildLight() {
  // Light's list at the left: the Environment, then the Lights, each with
  // an eye (the Light group adds them).
  auto* list = new QWidget;
  auto* listColumn = new QVBoxLayout(list);
  listColumn->setContentsMargins(0, 0, 0, 0);
  m_lightList = new QTreeWidget;
  m_lightList->setObjectName(QStringLiteral("renderLights"));
  m_lightList->setColumnCount(2);
  m_lightList->setHeaderHidden(true);
  m_lightList->header()->setStretchLastSection(false);
  m_lightList->header()->setSectionResizeMode(0, QHeaderView::Stretch);
  m_lightList->header()->setSectionResizeMode(1, QHeaderView::ResizeToContents);
  // Rows as the Browser's: no branches, the chosen one filled across.
  m_lightList->setRootIsDecorated(false);
  panel::setCompactIndentation(m_lightList, panel::Indent::Flat);
  m_lightList->setSelectionMode(QAbstractItemView::SingleSelection);
  m_lightList->setSelectionBehavior(QAbstractItemView::SelectRows);
  m_lightList->setFrameShape(QFrame::NoFrame);
  m_lightList->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  m_lightList->setTextElideMode(Qt::ElideRight);
  m_lightList->setMinimumHeight(200);
  listColumn->addWidget(m_lightList, 1);
  m_lightsPanel = page(QStringLiteral("renderLightsPage"), list);
  m_left->addWidget(m_lightsPanel);
  // A row chosen shows its settings at the right, Light staying shown.
  connect(m_lightList, &QTreeWidget::currentItemChanged, this, [this](QTreeWidgetItem* item) {
    if (m_listBuilding || item == nullptr) {
      return;
    }
    const QString row = item->data(0, Qt::UserRole).toString();
    TestSync::singleShot(0, this, [this, row] { lightRowChosen(row); });
  });
  // A light's eye: on or off (one undo step), the row chosen as it was.
  connect(m_lightList, &QTreeWidget::itemClicked, this, [this](QTreeWidgetItem* item, int column) {
    const QString id = item->data(0, Qt::UserRole).toString();
    if (m_listBuilding || column != 1 || id == QLatin1String("environment")) {
      return;
    }
    const bool on = !item->data(1, Qt::UserRole).toBool();
    TestSync::singleShot(0, this, [this, id, on] {
      m_environment->setLightEnabled(id, on);
      refresh();
    });
  });

  // Light's settings: Render Environment's, docked (mitcad#2013): the
  // environment's or the chosen light's.
  auto* lightBox = new QWidget;
  auto* lightColumn = new QVBoxLayout(lightBox);
  lightColumn->setContentsMargins(0, 0, 0, 0);
  m_environment = new RenderEnvironmentPanel(m_host, m_viewer);
  m_environment->setPart(RenderEnvironmentPanel::Part::Environment);
  m_environment->setLightListShown(false); // the stage's list lists them
  lightColumn->addWidget(m_environment, 1);
  // Its pages without the tab widget's frame (the stage's list chooses).
  for (QTabWidget* pages : m_environment->findChildren<QTabWidget*>()) {
    pages->setDocumentMode(true);
  }
  m_lightSettings = page(QStringLiteral("renderLightPage"), lightBox);
  m_right->addWidget(m_lightSettings);
  // Add Light and Reset Environment are the Light group's.
  connect(m_environment, &RenderEnvironmentPanel::changed, this, [this] { refresh(); });
  // A light chosen elsewhere (added, or its marker in the view): its row.
  connect(m_environment, &RenderEnvironmentPanel::lightChosen, this, [this](const QString& id) {
    if (!id.isEmpty() && id != m_lightRow) {
      refreshLightList();
      lightRowChosen(id);
    }
  });
}

// ---------------------------------------------------------------------------
// 3 Frame

void RenderWorkspace::buildFrame() {
  // Frame's list at the left: the shots (Rename… and Delete under them;
  // the Frame group saves them), then the saved views.
  auto* list = new QWidget;
  auto* listColumn = new QVBoxLayout(list);
  listColumn->setContentsMargins(0, 0, 0, 0);
  m_shotList = new QListWidget;
  m_shotList->setObjectName(QStringLiteral("renderShotList"));
  m_shotList->setFrameShape(QFrame::NoFrame);
  m_shotList->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  m_shotList->setTextElideMode(Qt::ElideRight);
  m_shotList->setMinimumHeight(120);
  listColumn->addWidget(m_shotList, 1);
  auto* shotButtons = new QHBoxLayout;
  shotButtons->setSpacing(4);
  m_renameShot = new QPushButton(tr("Rename…"));
  m_renameShot->setToolTip(tr("Another name for the chosen shot"));
  m_deleteShot = new QPushButton(tr("Delete"));
  m_deleteShot->setToolTip(tr("Deletes the chosen shot"));
  shotButtons->addWidget(m_renameShot);
  shotButtons->addWidget(m_deleteShot);
  shotButtons->addStretch(1);
  listColumn->addLayout(shotButtons);
  auto* viewsBox = new QGroupBox(tr("Saved views"));
  auto* viewsColumn = new QVBoxLayout(viewsBox);
  m_views = new QListWidget;
  m_views->setObjectName(QStringLiteral("renderSavedViews"));
  m_views->setMaximumHeight(160);
  m_views->setFrameShape(QFrame::NoFrame);
  m_views->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  viewsColumn->addWidget(m_views);
  listColumn->addWidget(viewsBox);
  m_shotsPanel = page(QStringLiteral("renderShotsPage"), list);
  m_left->addWidget(m_shotsPanel);
  connect(m_renameShot, &QPushButton::clicked, this, &RenderWorkspace::renameShot);
  connect(m_deleteShot, &QPushButton::clicked, this, &RenderWorkspace::deleteShot);
  connect(m_shotList, &QListWidget::itemClicked, this, [this](QListWidgetItem* item) {
    const QString id = item->data(Qt::UserRole).toString();
    TestSync::singleShot(0, this, [this, id] { lookThrough(id); });
  });
  connect(m_views, &QListWidget::itemClicked, this, [this](QListWidgetItem* item) {
    const QString name = item->text();
    for (const QJsonValue& value : m_host.model().queryArray(QStringLiteral("named_views"))) {
      if (value.toObject().value(QStringLiteral("name")).toString() == name) {
        m_viewer.setCamera(cameraOf(value.toObject()));
        qInfo().noquote() << QStringLiteral("Render frame view %1").arg(name);
      }
    }
  });

  // Frame's card: the chosen shot's camera.
  auto* frameBox = new QWidget;
  auto* frameColumn = new QVBoxLayout(frameBox);
  frameColumn->setContentsMargins(0, 0, 0, 0);
  auto* shotBox = new QGroupBox(tr("Camera"));
  auto* shotColumn = new QVBoxLayout(shotBox);
  m_shotTitle = new QLabel;
  m_shotTitle->setWordWrap(true);
  shotColumn->addWidget(m_shotTitle);
  frameColumn->addWidget(shotBox);
  frameColumn->addStretch(1);
  m_frameSettings = page(QStringLiteral("renderFramePage"), frameBox);
  m_right->addWidget(m_frameSettings);
}

// ---------------------------------------------------------------------------
// 4 Output

void RenderWorkspace::buildOutput() {
  // Output's list at the left: what Render renders, the view or a shot.
  auto* list = new QWidget;
  auto* listColumn = new QVBoxLayout(list);
  listColumn->setContentsMargins(0, 0, 0, 0);
  m_outputShots = new QListWidget;
  m_outputShots->setObjectName(QStringLiteral("renderOutputShots"));
  m_outputShots->setFrameShape(QFrame::NoFrame);
  m_outputShots->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  m_outputShots->setTextElideMode(Qt::ElideRight);
  m_outputShots->setMinimumHeight(120);
  listColumn->addWidget(m_outputShots, 1);
  m_outputPanel = page(QStringLiteral("renderOutputShotsPage"), list);
  m_left->addWidget(m_outputPanel);
  connect(m_outputShots, &QListWidget::itemClicked, this, [this](QListWidgetItem* item) {
    const QString id = item->data(Qt::UserRole).toString();
    TestSync::singleShot(0, this, [this, id] { chooseOutputShot(id); });
  });

  auto* box = new QWidget;
  auto* column = new QVBoxLayout(box);
  column->setContentsMargins(0, 0, 0, 0);
  auto* check = new QGroupBox(tr("Ready to render"));
  auto* checkColumn = new QVBoxLayout(check);
  m_findings = new QLabel(tr("Check the design before a long render."));
  m_findings->setObjectName(QStringLiteral("renderFindings"));
  m_findings->setWordWrap(true);
  checkColumn->addWidget(m_findings);
  column->addWidget(check);
  // Render Image's settings, docked; the image refines in the view.
  m_output = new RenderImagePanel(m_host, m_viewer);
  m_output->setInView(true);
  column->addWidget(m_output, 1);
  m_outputSettings = page(QStringLiteral("renderOutputPage"), box);
  m_right->addWidget(m_outputSettings);
  // Render All Shots checks first too, the shots and the folder with it.
  m_output->beforeRenderAll = [this] {
    refresh();
    return checkReady(true);
  };
  connect(m_output, &RenderImagePanel::renderingChanged, this, [this](bool rendering) {
    m_render->setEnabled(!rendering);
    m_cancel->setEnabled(rendering);
    updateRenderAll();
    // The live view rests while the final image renders (both trace).
    if (m_renderMode) {
      m_renderMode->setPaused(rendering || m_userPaused);
    }
    updateLive();
  });
}

bool RenderWorkspace::checkReady(bool allShots) {
  render::ReadyInput input;
  input.bodies = m_bodies;
  input.settings = m_settings;
  input.documentFolder = m_documentFolder;
  input.folder = m_output->outputFolder();
  input.allShots = allShots;
  const QList<render::ReadyFinding> findings = render::readyFindings(input);
  QStringList lines;
  for (const render::ReadyFinding& finding : findings) {
    lines << finding.text;
  }
  m_findings->setText(lines.isEmpty() ? tr("Ready to render.") : lines.join(QLatin1Char('\n')));
  if (render::hasProblem(findings)) {
    setErrorStyleSheet(m_findings);
  } else {
    m_findings->setStyleSheet(QString());
  }
  qInfo().noquote() << QStringLiteral("Render check: %1")
                           .arg(lines.isEmpty() ? QStringLiteral("Ready to render.") : lines.join(QStringLiteral(" | ")));
  return !render::hasProblem(findings);
}

void RenderWorkspace::render() {
  refresh();
  showStage(Stage::Output);
  // A problem the check finds (nothing shown, the environment's image
  // gone) stops the render before it starts; the findings say why.
  if (!checkReady(false)) {
    return;
  }
  m_output->setShot(m_shot);
  m_output->startRender();
}

void RenderWorkspace::cancel() { m_output->cancelRender(); }

// ---------------------------------------------------------------------------
// The bottom bar: the Shots row

void RenderWorkspace::buildBottom() {
  // The Shots bar: its name and a chip per shot to look through (Save Shot
  // is the Frame group's, Rename… and Delete are under Frame's list; Pause
  // and the method are the Live group's).
  m_bottom = new QWidget;
  m_bottom->setObjectName(QStringLiteral("renderBar"));
  // One row on the bar's centre line, as high as Design's timeline bar
  // (MainWindow: the bar's height and margins are the timeline's).
  auto* shotRow = new QHBoxLayout(m_bottom);
  shotRow->setContentsMargins(0, 0, 0, 0);
  shotRow->setSpacing(8);
  auto* shotsLabel = new QLabel(tr("Shots"));
  QFont bold = shotsLabel->font();
  bold.setWeight(QFont::DemiBold);
  shotsLabel->setFont(bold);
  shotRow->addWidget(shotsLabel, 0, Qt::AlignVCenter);
  m_shotButtons = new QWidget;
  m_shotRow = new QHBoxLayout(m_shotButtons);
  m_shotRow->setContentsMargins(0, 0, 0, 0);
  m_shotRow->setSpacing(4);
  auto* scroll = new QScrollArea;
  scroll->setWidget(m_shotButtons);
  scroll->setWidgetResizable(true);
  scroll->setFrameShape(QFrame::NoFrame);
  scroll->setVerticalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  scroll->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Ignored);
  shotRow->addWidget(scroll, 1);
  m_shotRow->setAlignment(Qt::AlignVCenter | Qt::AlignLeft);
  m_shotGroup = new QButtonGroup(this);
  m_shotGroup->setExclusive(true);
  connect(m_shotGroup, &QButtonGroup::buttonClicked, this, [this](QAbstractButton* button) {
    lookThrough(button->property("shot").toString());
  });
}

void RenderWorkspace::flattenSections(QWidget* root) {
  // Never a sideways scroll bar in the panels: they are as wide as their
  // cards.
  for (QAbstractScrollArea* area : root->findChildren<QAbstractScrollArea*>()) {
    area->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  }
  for (QGroupBox* box : root->findChildren<QGroupBox*>()) {
    QWidget* parent = box->parentWidget();
    QLayout* holder = parent != nullptr ? parent->layout() : nullptr;
    if (box->title().isEmpty() || box->isCheckable() || holder == nullptr) {
      continue;
    }
    // A heading in sentence case, as its box's title has it (UI
    // conventions, Text style), with an arrow that folds the section.
    auto* section = new QWidget;
    section->setObjectName(QStringLiteral("renderSection_") + box->objectName());
    auto* column = new QVBoxLayout(section);
    column->setContentsMargins(0, 6, 0, 2);
    column->setSpacing(2);
    auto* heading = new QToolButton;
    const QString name = box->title();
    heading->setObjectName(QStringLiteral("renderSectionHeading"));
    heading->setText(name);
    heading->setToolTip(tr("Shows or hides %1").arg(name));
    heading->setCheckable(true);
    heading->setChecked(true);
    heading->setAutoRaise(true);
    heading->setArrowType(Qt::DownArrow);
    heading->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    heading->setIconSize(QSize(9, 9));
    heading->setFocusPolicy(Qt::TabFocus);
    QFont font = heading->font();
    if (font.pointSizeF() > 0) {
      font.setPointSizeF(font.pointSizeF() * 0.85);
    }
    font.setWeight(QFont::DemiBold);
    heading->setFont(font);
    setRoleStyleSheet(heading, ColorRole::TextSecondary,
                      QStringLiteral("QToolButton { color: %1; border: none; background: transparent; padding: 2px 0px; }"));
    QLayoutItem* replaced = holder->replaceWidget(box, section);
    if (replaced == nullptr) {
      delete section;
      continue;
    }
    delete replaced; // its item, not the box
    column->addWidget(heading, 0, Qt::AlignLeft);
    column->addWidget(box);
    section->setVisible(!box->isHidden());
    // The box without its frame and title; its rows as they are (a form's
    // label | field in two columns).
    box->setTitle(QString());
    box->setFlat(true);
    // Its text in the theme's colour, set again when the theme changes: a
    // box with a style sheet keeps the palette it was polished with, so its
    // labels stayed dark on the Docked layout's dark docks (mitcad#2077).
    setRoleStyleSheet(box, ColorRole::Text,
                      QStringLiteral("QGroupBox { border: none; margin: 0px; padding: 0px; color: %1; }"));
    if (QLayout* inside = box->layout()) {
      inside->setContentsMargins(2, 0, 2, 4);
      if (auto* form = qobject_cast<QFormLayout*>(inside)) {
        form->setLabelAlignment(Qt::AlignLeft | Qt::AlignVCenter);
        form->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
      }
    }
    connect(heading, &QToolButton::toggled, this, [this, heading, box, name](bool open) {
      heading->setArrowType(open ? Qt::DownArrow : Qt::RightArrow);
      box->setVisible(open);
      qInfo().noquote() << QStringLiteral("Render section %1 %2").arg(name, open ? "shown" : "folded");
      m_logged.clear();
      TestSync::singleShot(0, this, [this] { logLayout(); });
    });
  }
}

void RenderWorkspace::connectRenderMode() {
  RenderMode* mode = m_hooks.renderMode ? m_hooks.renderMode() : nullptr;
  if (mode == m_renderMode) {
    return;
  }
  m_renderMode = mode;
  if (mode != nullptr) {
    connect(mode, &RenderMode::frameShown, this, &RenderWorkspace::showLive);
  }
}

void RenderWorkspace::setPaused(bool paused) {
  m_userPaused = paused;
  if (m_renderMode) {
    m_renderMode->setPaused(paused || m_output->isRendering());
  }
  updateLive();
}

void RenderWorkspace::showLive(int width, int height, int samples, int total) {
  if (!m_open) {
    return;
  }
  m_liveText = tr("%1 × %2 · %3/%4", "the live view: width × height · samples/total").arg(width).arg(height).arg(samples).arg(total);
  updateLive();
  if (samples >= total) {
    qInfo().noquote() << QStringLiteral("Render live: %1 x %2, %3/%4 samples").arg(width).arg(height).arg(samples).arg(total);
  }
}

void RenderWorkspace::updateLive() {
  const bool live = m_renderMode != nullptr;
  m_pauseAction->setEnabled(live);
  m_methodAction->setEnabled(live);
  m_methodAuto->setEnabled(live);
  m_methodCpu->setEnabled(live);
  // One button: Pause while refining, Resume while paused.
  m_pauseAction->setText(m_userPaused ? tr("Resume") : tr("Pause"));
  m_pauseAction->setIcon(themeIcon(m_userPaused ? QStringLiteral("render-resume") : QStringLiteral("render-pause")));
  m_pauseAction->setToolTip(m_userPaused ? tr("The live view goes on refining")
                                         : tr("The live view stops refining; Resume goes on"));
  const bool cpu = render::renderDeviceChoice() == QLatin1String("cpu");
  m_methodCpu->setChecked(cpu);
  m_methodAuto->setChecked(!cpu);
  m_methodAction->setText(cpu ? tr("CPU") : tr("GPU"));
  // The Live group's caption is the live view's status, its tooltip says
  // it in words; as wide as the widest status, so that nothing moves.
  QString caption;
  QString tip;
  if (!live) {
    caption = tr("No renderer");
    tip = tr("No live view: this MitCad has no renderer");
  } else if (m_output->isRendering()) {
    caption = tr("Live · resting");
    tip = tr("The image renders in the view; the live view rests meanwhile");
  } else if (m_userPaused) {
    caption = m_liveText.isEmpty() ? tr("Paused") : tr("Paused · %1").arg(m_liveText);
    tip = tr("The live view is paused; Resume goes on refining");
  } else {
    caption = m_liveText.isEmpty() ? tr("Live · starting…") : tr("Live · %1").arg(m_liveText);
    tip = tr("The live view: its size and the samples it has of all it takes");
  }
  Ribbon::setContentGroupCaption(m_liveGroup, caption, tip, QStringLiteral("Paused · 8888 × 8888 · 888/888"));
}

void RenderWorkspace::updateRenderAll() {
  // Render All Shots: while nothing renders and there are shots.
  m_renderAll->defaultAction()->setEnabled(!m_output->isRendering() &&
                                           !m_settings.value(QStringLiteral("shots")).toArray().isEmpty());
}

QJsonObject RenderWorkspace::shotObject(const QString& id) const {
  for (const QJsonValue& value : m_settings.value(QStringLiteral("shots")).toArray()) {
    if (value.toObject().value(QStringLiteral("id")).toString() == id) {
      return value.toObject();
    }
  }
  return {};
}

void RenderWorkspace::refreshShots() {
  // The Shots row's buttons, one per shot, the current one checked.
  for (QAbstractButton* button : m_shotGroup->buttons()) {
    m_shotGroup->removeButton(button);
    button->deleteLater();
  }
  while (QLayoutItem* item = m_shotRow->takeAt(0)) {
    delete item;
  }
  for (const QJsonValue& value : m_settings.value(QStringLiteral("shots")).toArray()) {
    const QJsonObject shot = value.toObject();
    const QString id = shot.value(QStringLiteral("id")).toString();
    auto* button = new QToolButton;
    button->setObjectName(QStringLiteral("renderShot_%1").arg(id));
    button->setText(shot.value(QStringLiteral("name")).toString());
    button->setIcon(themeIcon(QStringLiteral("named-view")));
    button->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    button->setCheckable(true);
    button->setChecked(id == m_shot);
    button->setProperty("shot", id);
    button->setToolTip(tr("Look through this shot; Render renders it"));
    m_shotGroup->addButton(button);
    m_shotRow->addWidget(button);
    new PlaceWatcher(button, [this] { logLayout(); });
  }
  m_shotRow->addStretch(1);
  // Frame's and Output's lists, the current shot chosen in both; their
  // scroll positions kept.
  const int shotsScrolled = m_shotList->verticalScrollBar()->value();
  const int outputScrolled = m_outputShots->verticalScrollBar()->value();
  {
    const QSignalBlocker blockShots(m_shotList);
    const QSignalBlocker blockOutput(m_outputShots);
    m_shotList->clear();
    m_outputShots->clear();
    auto* view = new QListWidgetItem(themeIcon(QStringLiteral("named-view")), tr("The View"));
    view->setData(Qt::UserRole, QString());
    view->setToolTip(tr("The view as it is now"));
    m_outputShots->addItem(view);
    if (m_shot.isEmpty()) {
      m_outputShots->setCurrentItem(view);
    }
    for (const QJsonValue& value : m_settings.value(QStringLiteral("shots")).toArray()) {
      const QJsonObject shot = value.toObject();
      const QString id = shot.value(QStringLiteral("id")).toString();
      const QString name = shot.value(QStringLiteral("name")).toString();
      for (QListWidget* list : {m_shotList, m_outputShots}) {
        auto* item = new QListWidgetItem(themeIcon(QStringLiteral("named-view")), name);
        item->setData(Qt::UserRole, id);
        item->setToolTip(list == m_shotList ? tr("Look through this shot; Render renders it")
                                            : tr("Render renders this shot"));
        list->addItem(item);
        if (id == m_shot) {
          list->setCurrentItem(item);
        }
      }
    }
  }
  m_shotList->verticalScrollBar()->setValue(shotsScrolled);
  m_outputShots->verticalScrollBar()->setValue(outputScrolled);
  m_renameShot->setEnabled(!m_shot.isEmpty());
  m_deleteShot->setEnabled(!m_shot.isEmpty());
  m_updateShot->defaultAction()->setEnabled(!m_shot.isEmpty());
  m_lookThrough->defaultAction()->setEnabled(!m_shot.isEmpty());
  updateRenderAll();
  const QJsonObject shot = shotObject(m_shot);
  m_shotTitle->setText(shot.isEmpty() ? tr("No shot chosen: choose one in the list, or save the view as one. Render "
                                           "renders the view as it is.")
                                      : tr("%1: Render renders this shot.").arg(shot.value(QStringLiteral("name")).toString()));
  // The saved views of Frame.
  m_views->clear();
  try {
    for (const QJsonValue& value : m_host.model().queryArray(QStringLiteral("named_views"))) {
      m_views->addItem(value.toObject().value(QStringLiteral("name")).toString());
    }
  } catch (const std::exception&) {
    // None.
  }
}

void RenderWorkspace::saveShot() {
  QJsonObject command = cameraFields(m_viewer.camera());
  command.insert(QStringLiteral("cmd"), QStringLiteral("add_render_shot"));
  if (!m_host.canChangeModel()) {
    m_host.showStatus(m_host.changeBlockedReason(), NoticeLevel::Warning);
    return;
  }
  QJsonObject result;
  if (!m_host.runModelCommand(command, &result)) {
    m_host.showStatus(mitcad::modelMessage(m_host.lastError()), NoticeLevel::Error);
    return;
  }
  m_shot = result.value(QStringLiteral("id")).toString();
  refresh();
  m_output->setShot(m_shot);
  qInfo().noquote() << QStringLiteral("Render shot saved %1 (%2)")
                           .arg(m_shot, shotObject(m_shot).value(QStringLiteral("name")).toString());
}

void RenderWorkspace::renameShot() {
  const QJsonObject shot = shotObject(m_shot);
  if (shot.isEmpty()) {
    return;
  }
  bool ok = false;
  const QString name = QInputDialog::getText(m_bottom, tr("Rename Shot"), tr("Name"), QLineEdit::Normal,
                                             shot.value(QStringLiteral("name")).toString(), &ok)
                           .trimmed();
  if (!ok || name.isEmpty() || name == shot.value(QStringLiteral("name")).toString()) {
    return;
  }
  if (runCommands({QJsonObject{{QStringLiteral("cmd"), QStringLiteral("edit_render_shot")},
                               {QStringLiteral("id"), m_shot},
                               {QStringLiteral("name"), name}}},
                  QString())) {
    qInfo().noquote() << QStringLiteral("Render shot %1 renamed to %2").arg(m_shot, name);
  }
}

void RenderWorkspace::updateShotToView() {
  if (m_shot.isEmpty()) {
    return;
  }
  QJsonObject command = cameraFields(m_viewer.camera());
  command.insert(QStringLiteral("cmd"), QStringLiteral("edit_render_shot"));
  command.insert(QStringLiteral("id"), m_shot);
  if (runCommands({command}, QString())) {
    qInfo().noquote() << QStringLiteral("Render shot %1 updated to the view").arg(m_shot);
  }
}

void RenderWorkspace::deleteShot() {
  if (m_shot.isEmpty()) {
    return;
  }
  const QString id = m_shot;
  if (runCommands({QJsonObject{{QStringLiteral("cmd"), QStringLiteral("delete_render_shot")}, {QStringLiteral("id"), id}}},
                  QString())) {
    m_shot.clear();
    refresh();
    m_output->setShot(m_shot);
    qInfo().noquote() << QStringLiteral("Render shot %1 deleted").arg(id);
  }
}

void RenderWorkspace::lookThrough(const QString& id) {
  const QJsonObject shot = shotObject(id);
  if (shot.isEmpty()) {
    return;
  }
  m_shot = id;
  m_output->clearViewImage();
  m_viewer.setCamera(cameraOf(shot));
  m_output->setShot(id);
  for (QAbstractButton* button : m_shotGroup->buttons()) {
    button->setChecked(button->property("shot").toString() == id);
  }
  for (QListWidget* list : {m_shotList, m_outputShots}) {
    const QSignalBlocker block(list);
    for (int i = 0; i < list->count(); ++i) {
      if (list->item(i)->data(Qt::UserRole).toString() == id) {
        list->setCurrentRow(i);
      }
    }
  }
  m_renameShot->setEnabled(true);
  m_deleteShot->setEnabled(true);
  m_updateShot->defaultAction()->setEnabled(true);
  m_lookThrough->defaultAction()->setEnabled(true);
  m_shotTitle->setText(tr("%1: Render renders this shot.").arg(shot.value(QStringLiteral("name")).toString()));
  updateTitles();
  qInfo().noquote() << QStringLiteral("Render shot %1 looked through").arg(id);
}

void RenderWorkspace::chooseOutputShot(const QString& id) {
  if (!id.isEmpty()) {
    lookThrough(id);
    return;
  }
  // The view as it is: no shot.
  m_shot.clear();
  m_output->setShot(m_shot);
  refreshShots();
  updateTitles();
  qInfo().noquote() << QStringLiteral("Render output the view");
}

// ---------------------------------------------------------------------------
// Places

void RenderWorkspace::logLayout() {
  if (!m_open || !m_stages->isVisible()) {
    return;
  }
  const QPoint origin = m_viewer.window()->mapToGlobal(QPoint(0, 0));
  const auto at = [&origin](QWidget* widget, QPoint inside) {
    const QPoint global = widget->mapToGlobal(inside) - origin;
    return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
  };
  const auto center = [&at](QWidget* widget) { return at(widget, widget->rect().center()); };
  QStringList lines;
  for (int i = 0; i < m_stages->count(); ++i) {
    lines << QStringLiteral("Render stage %1 at %2").arg(stageName(static_cast<Stage>(i)), at(m_stages, m_stages->tabRect(i).center()));
  }
  lines << QStringLiteral("Render workspace render at %1").arg(center(m_render))
        << QStringLiteral("Render workspace cancel at %1").arg(center(m_cancel));
  // The stage's groups: Live's, then the stage's own; the Live group's
  // caption (the live view's status) as it reads.
  if (m_actions->isVisible()) {
    QStringList groups;
    for (QWidget* group : m_actions->findChildren<QWidget*>()) {
      const QString name = group->objectName();
      if (name.startsWith(QLatin1String("ribbonContentGroup_render")) && group->isVisible()) {
        groups << name.mid(25);
      }
    }
    lines << QStringLiteral("Render groups: %1").arg(groups.join(QStringLiteral(" | ")))
          << QStringLiteral("Render live pause at %1").arg(center(m_pause))
          << QStringLiteral("Render live method at %1").arg(center(m_method));
    if (m_lookActions->isVisible()) {
      for (QToolButton* button : std::as_const(m_modeButtons)) {
        lines << QStringLiteral("Render look mode %1 at %2").arg(button->text().toLower(), center(button));
      }
      lines << QStringLiteral("Render look paint at %1").arg(center(m_paintButton))
            << QStringLiteral("Render look pick_up at %1").arg(center(m_pickUpButton));
    }
    if (m_lightActions->isVisible()) {
      lines << QStringLiteral("Render light add at %1").arg(center(m_addLight))
            << QStringLiteral("Render light reset at %1").arg(center(m_resetEnvironment));
    }
    if (m_frameActions->isVisible()) {
      lines << QStringLiteral("Render shot save at %1").arg(center(m_saveShot))
            << QStringLiteral("Render frame update at %1").arg(center(m_updateShot))
            << QStringLiteral("Render frame look through at %1").arg(center(m_lookThrough));
    }
    if (m_outputActions->isVisible()) {
      lines << QStringLiteral("Render check now at %1").arg(center(m_checkNow))
            << QStringLiteral("Render output render all at %1").arg(center(m_renderAll));
    }
  }
  if (m_bottom->isVisible()) {
    for (QAbstractButton* button : m_shotGroup->buttons()) {
      if (button->isVisible()) {
        lines << QStringLiteral("Render shot %1 at %2").arg(button->text(), center(button));
      }
    }
  }
  // A list's rows in sight: "<prefix> <name> at x,y".
  const auto rows = [&lines, &at](QListWidget* list, const QString& prefix) {
    for (int i = 0; i < list->count(); ++i) {
      const QRect rect = list->visualItemRect(list->item(i));
      if (!rect.isEmpty() && list->viewport()->rect().contains(rect.center())) {
        lines << QStringLiteral("%1 %2 at %3").arg(prefix, list->item(i)->text(), at(list->viewport(), rect.center()));
      }
    }
  };
  if (m_left->isVisible() && m_left->currentWidget() == m_lightsPanel) {
    for (int i = 0; i < m_lightList->topLevelItemCount(); ++i) {
      QTreeWidgetItem* item = m_lightList->topLevelItem(i);
      const QRect rect = m_lightList->visualItemRect(item);
      if (rect.isEmpty() || !m_lightList->viewport()->rect().contains(rect.center())) {
        continue;
      }
      lines << QStringLiteral("Render light row %1 at %2").arg(item->text(0), at(m_lightList->viewport(), rect.center()));
      if (item->data(0, Qt::UserRole).toString() != QLatin1String("environment")) {
        const QRect eye = m_lightList->visualRect(m_lightList->indexFromItem(item, 1));
        lines << QStringLiteral("Render light row %1 eye at %2").arg(item->text(0), at(m_lightList->viewport(), eye.center()));
      }
    }
  }
  if (m_left->isVisible() && m_left->currentWidget() == m_shotsPanel) {
    rows(m_shotList, QStringLiteral("Render frame shot"));
    lines << QStringLiteral("Render shot rename at %1").arg(center(m_renameShot))
          << QStringLiteral("Render shot delete at %1").arg(center(m_deleteShot));
  }
  if (m_left->isVisible() && m_left->currentWidget() == m_outputPanel) {
    rows(m_outputShots, QStringLiteral("Render output shot"));
  }
  if (m_left->isVisible() && m_left->currentWidget() == m_library) {
    lines << QStringLiteral("Render appearance search at %1").arg(center(m_search));
    lines << QStringLiteral("Render appearance scope at %1").arg(center(m_scope));
    for (QAbstractButton* button : m_filters->buttons()) {
      lines << QStringLiteral("Render appearance filter %1 at %2").arg(button->property("filter").toString(), center(button));
    }
    for (int i = 0; i < m_swatches->count(); ++i) {
      QListWidgetItem* item = m_swatches->item(i);
      const QRect rect = m_swatches->visualItemRect(item);
      if (m_swatches->viewport()->rect().contains(rect.center())) {
        lines << QStringLiteral("Render appearance %1 at %2")
                     .arg(item->data(Qt::UserRole).toString(), at(m_swatches->viewport(), rect.center()));
      }
    }
  }
  if (m_right->isVisible()) {
    if (m_right->currentWidget() == m_lookSettings && m_apply->isVisible()) {
      lines << QStringLiteral("Render look apply at %1").arg(center(m_apply));
    }
    // Every field of the stage's card shows its whole value or label
    // (mitcad#2013), and of its list's panel too; named by their titles
    // (the chosen light's, the shot's), so that each card's line is new.
    const QStringList cut = cutFields(m_right->currentWidget());
    lines << QStringLiteral("Render card %1 fields cut: %2")
                 .arg(m_rightTitle, cut.isEmpty() ? QStringLiteral("none") : cut.join(QStringLiteral(", ")));
  }
  if (m_left->isVisible()) {
    const QStringList cut = cutFields(m_left->currentWidget());
    lines << QStringLiteral("Render list %1 fields cut: %2")
                 .arg(m_leftTitle, cut.isEmpty() ? QStringLiteral("none") : cut.join(QStringLiteral(", ")));
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_logged) {
    m_logged = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
