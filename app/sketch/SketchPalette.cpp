// SPDX-License-Identifier: MIT
#include "SketchPalette.hpp"

#include <QAction>
#include <QCheckBox>
#include <QGridLayout>
#include <QGroupBox>
#include <QLabel>
#include <QAbstractButton>
#include <QPushButton>
#include <QSignalBlocker>
#include <QStyle>
#include <QStyleOptionButton>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/ChromeStyle.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Theme.hpp"
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

} // namespace

SketchPalette::SketchPalette(SketchController& controller, CommandRegistry& registry,
                             const QStringList& constraints, QWidget* parent)
    : QWidget(parent), m_c(controller) {
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

  const Section options = makeSection(tr("Options"));
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
  m_grid = check(tr("Snap to grid"), QStringLiteral("paletteGrid"),
                 tr("Points that snap to nothing else snap to a grid that follows the zoom"));
  m_dimensions = check(tr("Show dimensions"), QStringLiteral("paletteDimensions"), QString());
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
  layout->addWidget(options.box);

  connect(m_construction, &QCheckBox::toggled, &m_c, &SketchController::setConstruction);
  connect(m_grid, &QCheckBox::toggled, &m_c, &SketchController::setGridSnap);
  connect(m_dimensions, &QCheckBox::toggled, &m_c, &SketchController::setShowDimensions);
  connect(m_constraints, &QCheckBox::toggled, &m_c, &SketchController::setShowConstraints);
  connect(m_profiles, &QCheckBox::toggled, &m_c, &SketchController::setShowProfiles);
  connect(m_hideAbove, &QCheckBox::toggled, &m_c, &SketchController::setHideAbove);
  connect(lookAt, &QAbstractButton::clicked, &m_c, &SketchController::lookAt);

  const Section constraintBox = makeSection(tr("Constraints"));
  auto* grid = new QGridLayout(constraintBox.host);
  if (chromeStyle() == ChromeStyle::Floating) {
    grid->setContentsMargins(0, 0, 0, 0);
  }
  int index = 0;
  for (const QString& id : constraints) {
    QAction* action = registry.action(id);
    if (action == nullptr) {
      continue;
    }
    auto* button = new QToolButton;
    button->setDefaultAction(action);
    button->setIconSize(QSize(22, 22));
    button->setAutoRaise(true);
    button->setFocusPolicy(Qt::NoFocus);
    if (chromeStyle() == ChromeStyle::Floating) {
      setFlatCardButton(button);
      button->setFixedSize(34, 34);
    }
    button->setObjectName(QStringLiteral("palette_") + id);
    grid->addWidget(button, index / 6, index % 6);
    ++index;
  }
  layout->addWidget(constraintBox.box);

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
    layout->addWidget(button, 0, Qt::AlignRight);
  }
  layout->addStretch();

  connect(&m_c, &SketchController::changed, this, &SketchPalette::refresh);
  connect(&m_c, &SketchController::optionsChanged, this, &SketchPalette::refresh);
  connect(&m_c, &SketchController::toolChanged, this, [this](const QString& name) {
    m_tool->setText(name.isEmpty() ? QString()
                                   : tr("<b>%1</b><br><span style='color:gray'>Esc ends the tool; "
                                        "Enter accepts typed values.</span>")
                                         .arg(name.toHtmlEscaped()));
  });
  refresh();
}

void SketchPalette::refresh() {
  const SketchModel& model = m_c.model();
  m_title->setText(tr("<b>Sketch Palette</b> <span style='color:gray'>%1</span>").arg(model.name.toHtmlEscaped()));
  QString status;
  if (!model.conflicts.empty() || !model.error.isEmpty()) {
    status = tr("<span style='color:%1'>Conflicting constraints</span>").arg(errorColor().name());
  } else if (model.fullyConstrained()) {
    status = tr("Fully constrained");
  } else {
    status = tr("%n degree(s) of freedom", nullptr, model.dof);
  }
  m_status->setText(status);
  const auto set = [](QCheckBox* box, bool on) {
    const QSignalBlocker block(box);
    box->setChecked(on);
  };
  set(m_construction, m_c.construction());
  set(m_grid, m_c.gridSnap());
  set(m_dimensions, m_c.showDimensions());
  set(m_constraints, m_c.showConstraints());
  set(m_profiles, m_c.showProfiles());
  set(m_hideAbove, m_c.hideAbove());
}

void SketchPalette::logLayout() const {
  for (const QCheckBox* box : {m_construction, m_grid, m_dimensions, m_constraints, m_profiles, m_hideAbove}) {
    // The box's indicator: a click there toggles it whatever the width of
    // the label, which a click on the widget's middle may miss.
    QStyleOptionButton option;
    option.initFrom(box);
    option.text = box->text();
    const QRect indicator = box->style()->subElementRect(QStyle::SE_CheckBoxIndicator, &option, box);
    const QPoint center = GlassCard::mapToHost(box, indicator.center());
    qDebug().noquote() << QStringLiteral("Palette %1 at %2,%3").arg(box->objectName()).arg(center.x()).arg(center.y());
  }
}

} // namespace mitcad::sketch
