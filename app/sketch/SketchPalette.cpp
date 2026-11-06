// SPDX-License-Identifier: MIT
#include "SketchPalette.hpp"

#include <QAction>
#include <QButtonGroup>
#include <QCheckBox>
#include <QComboBox>
#include <QHBoxLayout>
#include <QEvent>
#include <QGridLayout>
#include <QGroupBox>
#include <QHash>
#include <QJsonObject>
#include <QLabel>
#include <QAbstractButton>
#include <QPushButton>
#include <QRadioButton>
#include <QSignalBlocker>
#include <QStyle>
#include <QStyleOptionButton>
#include <QTextDocumentFragment>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include <algorithm>
#include <initializer_list>
#include <utility>

#include "../framework/ChromeStyle.hpp"
#include "../framework/Command.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/NoticeLevel.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "AutoConstraint.hpp"
#include "PaletteCreateTools.hpp"
#include "PaletteWarnings.hpp"
#include "SketchController.hpp"

namespace mitcad::sketch {
namespace {

// A titled section of the palette: a group box; in a floating card a flat
// caption (semibold, small, subdued) over the widgets, without a frame.
// `host` is the widget that takes the section's layout.
struct Section {
  QWidget* box = nullptr;
  QWidget* host = nullptr;
};

Section makeSection(const QString& title) {
  if (chromeStyle() != ChromeStyle::Floating) {
    auto* box = new QGroupBox(title);
    return {box, box};
  }
  auto* box = new QWidget;
  auto* column = new QVBoxLayout(box);
  column->setContentsMargins(0, 4, 0, 0);
  column->setSpacing(4);
  auto* caption = new QLabel(title);
  QFont font = caption->font();
  font.setWeight(QFont::DemiBold);
  font.setPointSizeF(font.pointSizeF() * 0.92);
  caption->setFont(font);
  caption->setForegroundRole(QPalette::PlaceholderText);
  auto* host = new QWidget;
  column->addWidget(caption);
  column->addWidget(host);
  return {box, host};
}

QString autoConstraintTip(const QString& type) {
  if (type == QStringLiteral("equal")) {
    return QObject::tr("As long as the reference line, or the radius of the reference circle or arc");
  }
  if (type == QStringLiteral("horizontal_vertical")) {
    return QObject::tr("The center level with the reference's center, or plumb with it, whichever it is "
                       "nearer to");
  }
  if (type == QStringLiteral("concentric")) {
    return QObject::tr("The center of the reference circle or arc");
  }
  if (type == QStringLiteral("parallel")) {
    return QObject::tr("Parallel to the reference line");
  }
  if (type == QStringLiteral("perpendicular")) {
    return QObject::tr("Perpendicular to the reference line");
  }
  return {};
}

// A check box's or radio button's indicator in the main window: a click
// there toggles it whatever the width of its label.
QPoint indicatorCenter(const QAbstractButton* button, QStyle::SubElement element) {
  QStyleOptionButton option;
  option.initFrom(button);
  option.text = button->text();
  const QRect indicator = button->style()->subElementRect(element, &option, button);
  return GlassCard::mapToHost(button, indicator.center());
}

} // namespace

SketchPalette::SketchPalette(SketchController& controller, CommandRegistry& registry,
                             const QStringList& constraints, QWidget* parent)
    : QWidget(parent), m_c(controller), m_registry(registry) {
  setObjectName(QStringLiteral("sketchPalette"));
  auto* layout = new QVBoxLayout(this);
  if (chromeStyle() == ChromeStyle::Floating) {
    layout->setContentsMargins(4, 4, 4, 4); // the card has its own margins
  }
  m_title = new QLabel;
  m_status = new QLabel;
  m_status->setWordWrap(true);
  layout->addWidget(m_title);
  layout->addWidget(m_status);
  m_solve = new QLabel;
  m_solve->setObjectName(QStringLiteral("paletteSolveNotices"));
  m_solve->setWordWrap(true);
  m_solve->setTextFormat(Qt::RichText);
  m_solve->hide();
  layout->addWidget(m_solve);
  // The warnings under the status; a click on one selects what it is about
  // (mitcad#264).
  m_warnings = new PaletteWarnings;
  m_warnings->activated = [this](int index) { selectWarning(index); };
  layout->addWidget(m_warnings);
  // Under a warning about linked projections that cannot follow their
  // sources: the command that tidies them (mitcad#252).
  if (chromeStyle() == ChromeStyle::Floating) {
    m_cleanUp = new CapsuleButton(tr("Clean Up Stale Projections"));
  } else {
    m_cleanUp = new QPushButton(tr("Clean Up Stale Projections"));
    m_cleanUp->setFocusPolicy(Qt::NoFocus);
  }
  m_cleanUp->setObjectName(QStringLiteral("paletteCleanUpStale"));
  m_cleanUp->setToolTip(tr("Removes, trims or unlinks projected curves that no longer match their source"));
  m_cleanUp->hide();
  // Beside it, Project Again (mitcad#1303): the projections made anew from
  // their sources, the constraints and dimensions on them carried over.
  if (chromeStyle() == ChromeStyle::Floating) {
    m_projectAgain = new CapsuleButton(tr("Project Again"));
  } else {
    m_projectAgain = new QPushButton(tr("Project Again"));
    m_projectAgain->setFocusPolicy(Qt::NoFocus);
  }
  m_projectAgain->setObjectName(QStringLiteral("paletteProjectAgain"));
  m_projectAgain->setToolTip(
      tr("Projects the source again and carries the constraints and dimensions over to the new curves"));
  m_projectAgain->hide();
  auto* repairs = new QHBoxLayout;
  repairs->setContentsMargins(0, 0, 0, 0);
  repairs->addWidget(m_projectAgain);
  repairs->addWidget(m_cleanUp);
  repairs->addStretch(1);
  layout->addLayout(repairs);
  CommandRegistry* commands = &registry;
  connect(m_cleanUp, &QAbstractButton::clicked, this, [commands] {
    if (QAction* action = commands->action(QStringLiteral("sketch.clean_up_stale_projections"))) {
      qDebug().noquote() << QStringLiteral("Palette: Clean Up Stale Projections from the warning");
      action->trigger();
    }
  });
  connect(m_projectAgain, &QAbstractButton::clicked, this, [commands] {
    if (QAction* action = commands->action(QStringLiteral("sketch.project_again"))) {
      qDebug().noquote() << QStringLiteral("Palette: Project Again from the warning");
      action->trigger();
    }
  });

  const Section options = makeSection(tr("Settings"));
  auto* optionLayout = new QVBoxLayout(options.host);
  if (chromeStyle() == ChromeStyle::Floating) {
    optionLayout->setContentsMargins(0, 0, 0, 0);
    optionLayout->setSpacing(4);
  }
  const auto check = [optionLayout](const QString& text, const QString& name, const QString& tip) {
    auto* box = new QCheckBox(text);
    box->setObjectName(name);
    box->setToolTip(tip);
    box->setFocusPolicy(Qt::NoFocus);
    optionLayout->addWidget(box);
    return box;
  };
  m_construction = check(tr("Construction (X)"), QStringLiteral("paletteConstruction"),
                         tr("New geometry is construction geometry: not in profiles"));
  m_signed = check(tr("Signed distances"), QStringLiteral("paletteSignedDistances"),
                   tr("Sketch Dimension makes horizontal and vertical distances signed"));
  m_grid = check(tr("Snap to grid"), QStringLiteral("paletteGrid"),
                 tr("Points that snap to nothing else snap to a grid that follows the zoom"));
  m_dimensions = check(tr("Show dimensions"), QStringLiteral("paletteDimensions"), QString());
  // What dimensions show (mitcad#231).
  {
    auto* row = new QHBoxLayout;
    row->setContentsMargins(0, 0, 0, 0);
    auto* label = new QLabel(tr("Dimension text"));
    m_dimensionText = new QComboBox;
    m_dimensionText->setObjectName(QStringLiteral("paletteDimensionText"));
    m_dimensionText->addItems({tr("Value"), tr("Name"), tr("Name = value")});
    m_dimensionText->setToolTip(tr("What a dimension shows: its value, its parameter's name (d3) or both"));
    m_dimensionText->setFocusPolicy(Qt::NoFocus);
    label->setBuddy(m_dimensionText);
    row->addWidget(label);
    row->addWidget(m_dimensionText, 1);
    optionLayout->addLayout(row);
  }
  m_constraints = check(tr("Show constraints"), QStringLiteral("paletteConstraints"), QString());
  m_profiles = check(tr("Show profiles"), QStringLiteral("paletteProfiles"),
                     tr("Shade the closed regions, which Extrude and Revolve take"));
  m_hideAbove = check(tr("Hide Above Sketch"), QStringLiteral("paletteHideAbove"),
                      tr("Cut the bodies at the sketch plane and hide what is in front of it, "
                         "as Section Analysis does"));
  QAbstractButton* lookAt = nullptr;
  if (chromeStyle() == ChromeStyle::Floating) {
    lookAt = new CapsuleButton(tr("Look At"));
  } else {
    lookAt = new QPushButton(tr("Look At"));
    lookAt->setFocusPolicy(Qt::NoFocus);
  }
  lookAt->setToolTip(tr("Look straight at the sketch plane"));
  optionLayout->addWidget(lookAt);
  m_lookAt = lookAt;
  layout->addWidget(options.box);

  connect(m_construction, &QCheckBox::toggled, &m_c, &SketchController::setConstruction);
  connect(m_signed, &QCheckBox::toggled, &m_c, &SketchController::setSignedDistances);
  connect(m_grid, &QCheckBox::toggled, &m_c, &SketchController::setGridSnap);
  connect(m_dimensions, &QCheckBox::toggled, &m_c, &SketchController::setShowDimensions);
  connect(m_constraints, &QCheckBox::toggled, &m_c, &SketchController::setShowConstraints);
  connect(m_profiles, &QCheckBox::toggled, &m_c, &SketchController::setShowProfiles);
  connect(m_hideAbove, &QCheckBox::toggled, &m_c, &SketchController::setHideAbove);
  connect(m_dimensionText, qOverload<int>(&QComboBox::currentIndexChanged), &m_c, [this](int index) {
    m_c.setDimensionTextMode(static_cast<SketchController::DimensionText>(index));
  });
  connect(lookAt, &QAbstractButton::clicked, &m_c, &SketchController::lookAt);

  // The user's most used sketch tools (mitcad#1027), above the
  // constraints: two rows of seven, chosen as the palette opens
  // (chooseCreateTools). The two sections' grids are alike, so that their
  // columns line up and both fit the palette's width.
  const auto buttonGrid = [](QWidget* host) {
    auto* made = new QGridLayout(host);
    if (chromeStyle() == ChromeStyle::Floating) {
      made->setContentsMargins(0, 0, 0, 0);
    }
    made->setHorizontalSpacing(2);
    made->setVerticalSpacing(2);
    return made;
  };
  const Section createBox = makeSection(tr("Create"));
  createBox.box->setObjectName(QStringLiteral("paletteCreate"));
  m_createGrid = buttonGrid(createBox.host);
  layout->addWidget(createBox.box);

  const Section constraintBox = makeSection(tr("Constraints"));
  QGridLayout* grid = buttonGrid(constraintBox.host);
  int index = 0;
  for (const QString& id : constraints) {
    QToolButton* button = commandButton(id, QStringLiteral("palette_") + id);
    if (button == nullptr) {
      continue;
    }
    // Seven to a row: the 13 constraints on two rows (mitcad#1027).
    grid->addWidget(button, index / kPaletteCreateColumns, index % kPaletteCreateColumns);
    m_constraintButtons.append(button);
    ++index;
  }
  layout->addWidget(constraintBox.box);

  // Auto constraints while drawing (mitcad#189): shown once the drawing
  // tool has drawn an entity; the choices hold for the tool's next ones.
  const QString autoTitle = tr("Auto constraint");
  const Section autoBox = makeSection(autoTitle);
  m_autoSection = autoBox.box;
  m_autoSection->setObjectName(QStringLiteral("paletteAutoConstraint"));
  m_autoSection->setAccessibleName(autoTitle);
  auto* autoLayout = new QVBoxLayout(autoBox.host);
  if (chromeStyle() == ChromeStyle::Floating) {
    autoLayout->setContentsMargins(0, 0, 0, 0);
    autoLayout->setSpacing(4);
  }
  const auto radio = [autoLayout](const QString& text, const QString& name, const QString& tip) {
    auto* button = new QRadioButton(text);
    button->setObjectName(name);
    button->setToolTip(tip);
    button->setFocusPolicy(Qt::NoFocus);
    autoLayout->addWidget(button);
    return button;
  };
  m_autoFirst = radio(tr("Link to first"), QStringLiteral("paletteAutoFirst"),
                      tr("Constrain each new entity to the first one this tool drew"));
  m_autoLast = radio(tr("Link to last"), QStringLiteral("paletteAutoLast"),
                     tr("Constrain each new entity to the one this tool drew just before it"));
  auto* links = new QButtonGroup(this);
  links->setExclusive(true);
  links->addButton(m_autoFirst);
  links->addButton(m_autoLast);
  m_autoLast->setChecked(true);
  for (const QString& type : autoConstraintTypes()) {
    auto* box = new QCheckBox(autoConstraintLabel(type));
    box->setObjectName(QStringLiteral("paletteAuto_") + type);
    box->setToolTip(autoConstraintTip(type));
    box->setFocusPolicy(Qt::NoFocus);
    autoLayout->addWidget(box);
    m_autoChecks.insert(type, box);
    connect(box, &QCheckBox::toggled, this, [this, type](bool on) { m_c.setAutoConstraint(type, on); });
  }
  connect(m_autoFirst, &QRadioButton::toggled, this, [this](bool on) {
    if (on) {
      m_c.setAutoLink(AutoConstraintSession::Link::First);
    }
  });
  connect(m_autoLast, &QRadioButton::toggled, this, [this](bool on) {
    if (on) {
      m_c.setAutoLink(AutoConstraintSession::Link::Last);
    }
  });
  // The pointer over the section highlights the reference.
  m_autoSection->installEventFilter(this);
  m_autoSection->hide();
  m_autoLogged = QStringLiteral("hidden");
  layout->addWidget(m_autoSection);
  connect(&m_c, &SketchController::autoConstraintsChanged, this, &SketchPalette::refreshAuto);

  m_tool = new QLabel;
  m_tool->setWordWrap(true);
  layout->addWidget(m_tool);

  if (QAction* finish = registry.action(QStringLiteral("sketch.finish"))) {
    QToolButton* button = nullptr;
    if (chromeStyle() == ChromeStyle::Floating) {
      button = new CapsuleButton(finish->text(), true);
      button->setDefaultAction(finish); // text and state follow the action
      button->setToolButtonStyle(Qt::ToolButtonTextOnly);
    } else {
      button = new QToolButton;
      button->setDefaultAction(finish);
      button->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
      button->setFocusPolicy(Qt::NoFocus);
    }
    button->setObjectName(QStringLiteral("paletteFinish"));
    layout->addWidget(button, 0, Qt::AlignRight);
    m_finish = button;
  }
  layout->addStretch();

  connect(&m_c, &SketchController::changed, this, &SketchPalette::refresh);
  connect(&m_c, &SketchController::optionsChanged, this, &SketchPalette::refresh);
  connect(&m_c, &SketchController::toolChanged, this, [this](const QString& name) {
    // Keep the actionable hint readable with the ordinary text palette;
    // the tool's bold name supplies the visual hierarchy.
    m_tool->setText(name.isEmpty() ? QString()
                                 : tr("<b>%1</b><br>Esc ends the tool; Enter accepts typed values.")
                                       .arg(name.toHtmlEscaped()));
  });
  refresh();
}

QWidget* SketchPalette::takeFinish() {
  if (m_finish != nullptr) {
    layout()->removeWidget(m_finish);
  }
  return m_finish;
}

QToolButton* SketchPalette::commandButton(const QString& id, const QString& name) const {
  QAction* action = m_registry.action(id);
  if (action == nullptr) {
    return nullptr;
  }
  auto* button = new QToolButton;
  // The action's icon, tooltip (with its shortcut, as the ribbon's) and
  // enabled and checked states (the tool on, mitcad#1027).
  button->setDefaultAction(action);
  button->setIconSize(QSize(22, 22));
  button->setAutoRaise(true);
  button->setFocusPolicy(Qt::NoFocus);
  if (chromeStyle() == ChromeStyle::Floating) {
    setFlatCardButton(button);
    button->setFixedSize(34, 34);
    styleButton(button);
  }
  button->setObjectName(name);
  return button;
}

void SketchPalette::styleButton(QToolButton* button) const {
#ifdef MITCAD_MAC_UI
  Q_UNUSED(button);
#else
  // The card's flat button (setFlatCardButton), checked as the ribbon's
  // are: tinted and outlined in the accent colour (mitcad#1027: the tool
  // on). Set again when the palette changes (changeEvent).
  const QColor accent = accentColor(palette());
  button->setStyleSheet(QStringLiteral(
      "QToolButton { border: none; background: transparent; border-radius: 6px; padding: 2px; }"
      "QToolButton:hover { background: rgba(128, 128, 128, 40); }"
      "QToolButton:pressed { background: rgba(128, 128, 128, 80); }"
      "QToolButton:checked { border: 1px solid %1; background: rgba(%2, %3, %4, 51); padding: 1px; }"
      "QToolButton:disabled { background: transparent; }")
                            .arg(accent.name())
                            .arg(accent.red())
                            .arg(accent.green())
                            .arg(accent.blue()));
#endif
}

bool SketchPalette::isCreateTool(const CommandDef& def) {
  return def.tab == QStringLiteral("SKETCH") && def.mode == CommandDef::Mode::Sketch &&
         def.kind != CommandDef::Kind::Action &&
         (def.group == QStringLiteral("CREATE") || def.group == QStringLiteral("MODIFY"));
}

QStringList SketchPalette::createCandidates(const CommandRegistry& registry) {
  static const QStringList groups = {QStringLiteral("CREATE"), QStringLiteral("MODIFY")};
  QStringList ids;
  for (const QString& group : groups) {
    // As Ribbon::createGroup orders a group's menu: registration order, a
    // subgroup's commands together where its first command is.
    QList<std::pair<QString, QStringList>> entries; // subgroup, ids
    for (const auto& def : registry.commands()) {
      if (def->group != group || !isCreateTool(*def) || registry.action(def->id) == nullptr) {
        continue;
      }
      const QString& subgroup = def->subgroup;
      const auto same =
          std::find_if(entries.begin(), entries.end(), [&subgroup](const std::pair<QString, QStringList>& entry) {
            return !subgroup.isEmpty() && entry.first == subgroup;
          });
      if (same != entries.end()) {
        same->second.append(def->id);
      } else {
        entries.append(std::make_pair(subgroup, QStringList{def->id}));
      }
    }
    for (const std::pair<QString, QStringList>& entry : entries) {
      ids += entry.second;
    }
  }
  return ids;
}

void SketchPalette::chooseCreateTools(const QStringList& pinned) {
  const PaletteCreateChoice choice =
      choosePaletteCreateTools(createCandidates(m_registry), pinned, paletteToolCounts());
  QStringList names;
  for (const QString& id : choice.ids) {
    names.append(paletteToolName(id));
  }
  // "Palette create tools: line, rectangle, … (defaults)", for UI tests.
  qDebug().noquote() << QStringLiteral("Palette create tools: %1 (%2)")
                            .arg(names.join(QStringLiteral(", ")),
                                 choice.fromCounts ? QStringLiteral("counts") : QStringLiteral("defaults"));
  if (choice.ids == m_createIds) {
    return;
  }
  for (QToolButton* button : std::as_const(m_createButtons)) {
    delete button; // out of the grid with it
  }
  m_createButtons.clear();
  m_createIds = choice.ids;
  int index = 0;
  for (const QString& id : std::as_const(m_createIds)) {
    QToolButton* button = commandButton(id, QStringLiteral("paletteCreate_") + id);
    if (button == nullptr) {
      continue;
    }
    // The tool on shows checked (its action's state, MainWindow).
    const QString name = paletteToolName(id);
    connect(button, &QToolButton::toggled, this, [name](bool on) {
      if (on) {
        qDebug().noquote() << QStringLiteral("Palette create checked: %1").arg(name);
      }
    });
    m_createGrid->addWidget(button, index / kPaletteCreateColumns, index % kPaletteCreateColumns);
    m_createButtons.append(button);
    ++index;
  }
}

void SketchPalette::logButtons() const {
  // The widget the palette is shown in (a scroll area's viewport): a button
  // beyond its width is clipped.
  const QWidget* view = parentWidget();
  if (view == nullptr) {
    view = this;
  }
  QStringList clipped;
  const auto section = [this, view, &clipped](const QString& title, const QList<QToolButton*>& buttons) {
    QHash<int, int> perRow; // buttons by their top
    for (const QToolButton* button : buttons) {
      // Where to click it, or out of sight when the palette has scrolled
      // it away.
      const QRect shown = button->visibleRegion().boundingRect();
      if (shown.isEmpty()) {
        qDebug().noquote() << QStringLiteral("Palette %1 out of sight").arg(button->objectName());
      } else {
        const QPoint center = GlassCard::mapToHost(button, shown.center());
        qDebug().noquote()
            << QStringLiteral("Palette %1 at %2,%3").arg(button->objectName()).arg(center.x()).arg(center.y());
      }
      const QPoint corner = button->mapTo(view, QPoint(0, 0));
      if (corner.x() < 0 || corner.x() + button->width() > view->width()) {
        clipped.append(button->objectName());
      }
      const int top = button->mapTo(this, QPoint(0, 0)).y();
      perRow[top] += 1;
    }
    int columns = 0;
    for (const int count : perRow) {
      columns = std::max(columns, count);
    }
    // "Palette Constraints: 13 buttons in 2 rows of 7", for UI tests.
    qDebug().noquote() << QStringLiteral("Palette %1: %2 buttons in %3 rows of %4")
                              .arg(title)
                              .arg(buttons.size())
                              .arg(perRow.size())
                              .arg(columns);
  };
  section(QStringLiteral("Create"), m_createButtons);
  section(QStringLiteral("Constraints"), m_constraintButtons);
  qDebug().noquote() << QStringLiteral("Palette buttons clipped: %1 (palette %2 wide in %3)")
                            .arg(clipped.isEmpty() ? QStringLiteral("none") : clipped.join(QStringLiteral(", ")))
                            .arg(width())
                            .arg(view->width());
}

void SketchPalette::changeEvent(QEvent* event) {
  QWidget::changeEvent(event);
  if ((event->type() == QEvent::PaletteChange || event->type() == QEvent::ApplicationPaletteChange) &&
      m_title != nullptr) {
    refreshTitle();
    if (chromeStyle() == ChromeStyle::Floating) {
      for (QToolButton* button : std::as_const(m_createButtons)) {
        styleButton(button);
      }
      for (QToolButton* button : std::as_const(m_constraintButtons)) {
        styleButton(button);
      }
    }
  }
}

void SketchPalette::refreshTitle() {
  // Match the palette's subdued section captions, including their alpha on
  // a translucent card. A fixed grey can disappear against a dark card.
  const QColor secondary = palette().color(QPalette::PlaceholderText);
  const QString color = QStringLiteral("rgba(%1,%2,%3,%4)")
                            .arg(secondary.red())
                            .arg(secondary.green())
                            .arg(secondary.blue())
                            .arg(secondary.alpha());
  m_title->setText(tr("<b>Sketch Palette</b> <span style='color:%2'>%1</span>")
                       .arg(m_c.model().name.toHtmlEscaped(), color));
}

void SketchPalette::refresh() {
  const SketchModel& model = m_c.model();
  refreshTitle();
  QString status;
  // The solve's states at their levels (mitcad#1040): conflicting
  // constraints and a solve that did not converge are errors, with the
  // error's icon.
  if (!model.conflicts.empty()) {
    status = QStringLiteral("%1&nbsp;<span style='color:%2'>%3</span>")
                 .arg(noticeIconHtml(NoticeLevel::Error), errorColor(palette()).name(), tr("Conflicting constraints"));
  } else if (!model.error.isEmpty()) {
    status = QStringLiteral("%1&nbsp;<span style='color:%2'>%3</span>")
                 .arg(noticeIconHtml(NoticeLevel::Error), errorColor(palette()).name(),
                      tr("Did not solve: %1").arg(model.error).toHtmlEscaped());
  } else if (model.fullyConstrained()) {
    status = tr("Fully constrained");
  } else {
    status = tr("%n degree(s) of freedom", nullptr, model.dof);
  }
  m_status->setText(status);
  // Redundant constraints a warning, unmet inactive ones information, as
  // the model's notices say (mitcad#1040).
  QStringList solve;
  QStringList solveLog;
  for (const QJsonValue& value : model.notices) {
    const QJsonObject notice = value.toObject();
    const NoticeLevel level = noticeLevelFromModel(notice.value(QStringLiteral("level")).toString());
    if (level == NoticeLevel::Error) {
      continue; // the status says it
    }
    const QString message = notice.value(QStringLiteral("message")).toString();
    if (level == NoticeLevel::Warning && model.warnings.contains(message)) {
      continue; // the warnings below list it
    }
    solve << noticeHtml(level, message, palette());
    solveLog << QStringLiteral("%1: %2").arg(noticeLevelName(level), message);
  }
  m_solve->setText(solve.join(QStringLiteral("<br>")));
  m_solve->setVisible(!solve.isEmpty());
  const QString solveLine = solveLog.join(QStringLiteral("; "));
  if (solveLine != m_loggedSolve) {
    m_loggedSolve = solveLine;
    qDebug().noquote() << QStringLiteral("Sketch Palette solve: %1")
                              .arg(solveLine.isEmpty() ? QStringLiteral("none") : solveLine);
  }
  // What the evaluation warns about (mitcad#232: projections that could
  // not follow their sources), as links to what they are about (mitcad#264);
  // also the solve's redundant constraints, grouped by the offset, pattern
  // or sketch they come from, as links to what they name (mitcad#1171).
  m_warnings->setWarnings(model.warnings, model.warningEntities);
  // "Sketch Palette warnings: Offset k1: 2 dimensions follow from the
  // others" and the redundant constraints' ids ("Sketch Palette redundant:
  // k7, k9"), when they change, for UI tests and the log.
  const QString warningsLine =
      model.warnings.isEmpty() ? QStringLiteral("none") : model.warnings.join(QStringLiteral("; "));
  if (warningsLine != m_loggedWarnings) {
    m_loggedWarnings = warningsLine;
    qDebug().noquote() << QStringLiteral("Sketch Palette warnings: %1").arg(warningsLine);
  }
  const QString redundantLine =
      model.redundant.isEmpty() ? QStringLiteral("none") : model.redundant.join(QStringLiteral(", "));
  if (redundantLine != m_loggedRedundant) {
    m_loggedRedundant = redundantLine;
    qDebug().noquote() << QStringLiteral("Sketch Palette redundant: %1").arg(redundantLine);
  }
  // The clean-up offered while projections cannot follow (mitcad#252).
  // Project Again beside it, for those whose sources are there (mitcad#1303).
  if (model.staleProjections != m_cleanUpOffered || model.projectAgain != m_projectAgainOffered) {
    m_cleanUpOffered = model.staleProjections;
    m_projectAgainOffered = model.projectAgain;
    m_cleanUp->setVisible(m_cleanUpOffered);
    m_projectAgain->setVisible(m_projectAgainOffered);
    if (m_cleanUpOffered) {
      // Once it is laid out.
      TestSync::singleShot(50, this, [this] { logCleanUp(); });
    }
  }
  const auto set = [](QCheckBox* box, bool on) {
    const QSignalBlocker block(box);
    box->setChecked(on);
  };
  set(m_construction, m_c.construction());
  set(m_signed, m_c.signedDistances());
  set(m_grid, m_c.gridSnap());
  set(m_dimensions, m_c.showDimensions());
  set(m_constraints, m_c.showConstraints());
  set(m_profiles, m_c.showProfiles());
  set(m_hideAbove, m_c.hideAbove());
  {
    const QSignalBlocker block(m_dimensionText);
    m_dimensionText->setCurrentIndex(static_cast<int>(m_c.dimensionTextMode()));
  }
  // Undo may have taken the tool's entities back.
  refreshAuto();
}

void SketchPalette::refreshAuto() {
  const AutoConstraintSession& session = m_c.autoConstraints();
  const bool shown = m_c.autoConstraintsShown();
  {
    const QSignalBlocker blockFirst(m_autoFirst);
    const QSignalBlocker blockLast(m_autoLast);
    (session.link() == AutoConstraintSession::Link::First ? m_autoFirst : m_autoLast)->setChecked(true);
  }
  const QStringList offered = session.types();
  QStringList states;
  for (const QString& type : autoConstraintTypes()) {
    QCheckBox* box = m_autoChecks.value(type);
    const QSignalBlocker block(box);
    box->setVisible(offered.contains(type));
    box->setChecked(session.checked(type));
    if (offered.contains(type)) {
      states << QStringLiteral("%1 (%2)").arg(box->text(), box->isChecked() ? QStringLiteral("x") : QStringLiteral(" "));
    }
  }
  m_autoSection->setVisible(shown);
  if (!shown) {
    m_c.setAutoHover(false);
  }
  const auto mark = [](const QRadioButton* button) {
    return QStringLiteral("%1 (%2)").arg(button->text(), button->isChecked() ? QStringLiteral("x") : QStringLiteral(" "));
  };
  const QString summary = shown ? QStringLiteral("%1, %2; %3")
                                      .arg(mark(m_autoFirst), mark(m_autoLast), states.join(QStringLiteral(", ")))
                                : QStringLiteral("hidden");
  if (summary == m_autoLogged) {
    return;
  }
  m_autoLogged = summary;
  // "Palette Auto constraint: Link to first ( ), Link to last (x); Equal ( ), …", for UI tests.
  qDebug().noquote() << QStringLiteral("Palette %1: %2").arg(m_autoSection->accessibleName(), summary);
  if (shown) {
    // Once the section is laid out.
    TestSync::singleShot(50, this, [this] {
      emit autoSectionShown(m_autoSection);
      logAutoLayout();
    });
  }
}

void SketchPalette::logPlaces() {
  logLayout();
  logAutoLayout();
}

void SketchPalette::logAutoLayout() {
  if (!m_autoSection->isVisible()) {
    return;
  }
  const QPoint section = GlassCard::mapToHost(m_autoSection, m_autoSection->rect().center());
  qDebug().noquote() << QStringLiteral("Palette %1 at %2,%3").arg(m_autoSection->objectName()).arg(section.x()).arg(section.y());
  for (const QRadioButton* button : {m_autoFirst, m_autoLast}) {
    const QPoint center = indicatorCenter(button, QStyle::SE_RadioButtonIndicator);
    qDebug().noquote() << QStringLiteral("Palette %1 at %2,%3").arg(button->objectName()).arg(center.x()).arg(center.y());
  }
  for (const QString& type : autoConstraintTypes()) {
    const QCheckBox* box = m_autoChecks.value(type);
    if (box->isVisible()) {
      const QPoint center = indicatorCenter(box, QStyle::SE_CheckBoxIndicator);
      qDebug().noquote() << QStringLiteral("Palette %1 at %2,%3").arg(box->objectName()).arg(center.x()).arg(center.y());
    }
  }
}

bool SketchPalette::eventFilter(QObject* watched, QEvent* event) {
  if (watched == m_autoSection) {
    if (event->type() == QEvent::Enter) {
      m_c.setAutoHover(true);
    } else if (event->type() == QEvent::Leave || event->type() == QEvent::Hide) {
      m_c.setAutoHover(false);
    }
  }
  return QWidget::eventFilter(watched, event);
}

void SketchPalette::logLayout() const {
  for (const QCheckBox* box :
       {m_construction, m_signed, m_grid, m_dimensions, m_constraints, m_profiles, m_hideAbove}) {
    // The box's indicator: a click there toggles it whatever the width of
    // the label, which a click on the widget's middle may miss.
    QStyleOptionButton option;
    option.initFrom(box);
    option.text = box->text();
    const QRect indicator = box->style()->subElementRect(QStyle::SE_CheckBoxIndicator, &option, box);
    const QPoint center = GlassCard::mapToHost(box, indicator.center());
    qDebug().noquote() << QStringLiteral("Palette %1 at %2,%3").arg(box->objectName()).arg(center.x()).arg(center.y());
  }
  const QPoint combo = GlassCard::mapToHost(m_dimensionText, m_dimensionText->rect().center());
  qDebug().noquote() << QStringLiteral("Palette %1 at %2,%3").arg(m_dimensionText->objectName()).arg(combo.x()).arg(combo.y());
  if (const QWidget* finish = m_finish) {
    const QPoint center = GlassCard::mapToHost(finish, finish->rect().center());
    qDebug().noquote() << QStringLiteral("Palette Finish Sketch at %1,%2").arg(center.x()).arg(center.y());
  }
  // Look At, for UI tests that click it (mitcad#1060).
  if (m_lookAt != nullptr && m_lookAt->isVisible()) {
    const QPoint center = GlassCard::mapToHost(m_lookAt, m_lookAt->rect().center());
    qDebug().noquote() << QStringLiteral("Palette Look At at %1,%2").arg(center.x()).arg(center.y());
  }
  // Each option row's place and size against its size hint, and the
  // palette's against its own: a row laid out below its hint overlaps the
  // next one (mitcad#470).
  for (const QWidget* row : std::initializer_list<const QWidget*>{
           m_construction, m_signed, m_grid, m_dimensions, m_constraints, m_profiles, m_hideAbove, m_dimensionText,
           m_finish}) {
    if (row == nullptr) {
      continue;
    }
    const QPoint corner = GlassCard::mapToHost(row, QPoint(0, 0));
    qDebug().noquote() << QStringLiteral("Palette row %1 at %2,%3 size %4x%5, hint %6x%7")
                              .arg(row->objectName())
                              .arg(corner.x())
                              .arg(corner.y())
                              .arg(row->width())
                              .arg(row->height())
                              .arg(row->sizeHint().width())
                              .arg(row->sizeHint().height());
  }
  qDebug().noquote() << QStringLiteral("Palette size %1x%2, hint %3x%4, minimum hint %5x%6")
                            .arg(width())
                            .arg(height())
                            .arg(sizeHint().width())
                            .arg(sizeHint().height())
                            .arg(minimumSizeHint().width())
                            .arg(minimumSizeHint().height());
  logButtons();
  m_warnings->logLayout();
  logCleanUp();
}

void SketchPalette::selectWarning(int index) {
  const SketchModel& model = m_c.model();
  if (index < 0 || static_cast<std::size_t>(index) >= model.warningEntities.size()) {
    return;
  }
  const QStringList& entities = model.warningEntities[static_cast<std::size_t>(index)];
  // "Palette: warning 0 selects 8 entities: p1, p2, c3, ...", for UI tests.
  qDebug().noquote() << QStringLiteral("Palette: warning %1 selects %2 entities: %3")
                            .arg(index)
                            .arg(entities.size())
                            .arg(entities.join(QStringLiteral(", ")));
  m_c.selectEntities(entities, true);
}

void SketchPalette::logCleanUp() const {
  if (!m_cleanUp->isVisible()) {
    return;
  }
  // "Palette paletteCleanUpStale at x,y" (and paletteProjectAgain, when it
  // is offered), for UI tests.
  for (QAbstractButton* button : {m_projectAgain, m_cleanUp}) {
    if (!button->isVisible()) {
      continue;
    }
    const QPoint center = GlassCard::mapToHost(button, button->rect().center());
    qDebug().noquote() << QStringLiteral("Palette %1 at %2,%3").arg(button->objectName()).arg(center.x()).arg(center.y());
  }
}

QString SketchPalette::statusText() const {
  // (Without the level's icon, mitcad#1040.)
  return QTextDocumentFragment::fromHtml(m_status->text()).toPlainText().remove(QChar(0xFFFC)).trimmed();
}

} // namespace mitcad::sketch
