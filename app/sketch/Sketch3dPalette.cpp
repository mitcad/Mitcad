// SPDX-License-Identifier: MIT
#include "Sketch3dPalette.hpp"

#include <algorithm>
#include <iterator>
#include <utility>

#include <QAction>
#include <QButtonGroup>
#include <QCheckBox>
#include <QEvent>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonObject>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QScrollArea>
#include <QScrollBar>
#include <QSizePolicy>
#include <QStringList>
#include <QStyle>
#include <QStyleOption>
#include <QTimer>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/ChromeStyle.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "Sketch3dController.hpp"
#include "Sketch3dEdit.hpp"
#include "Sketch3dModel.hpp"

namespace mitcad::sketch {
namespace {

const char* const kFieldIds[] = {"dx",     "dy",    "dz",    "length", "radius", "arc_length", "diameter", "major",
                                 "minor",  "width", "height", "sides", "text",   "text_height"};

// The columns of the relations and dimensions buttons (mitcad#1592).
constexpr int kEditColumns = 2;

// The property that gives a list row's words the id of what they name
// (mitcad#1652: the pointer over them paints it).
constexpr const char kRowProperty[] = "sketch3dRow";

QToolButton* choiceButton(const QString& text, const QString& tip) {
  auto* button = new QToolButton;
  button->setText(text);
  button->setToolTip(tip);
  button->setCheckable(true);
  button->setFocusPolicy(Qt::NoFocus);
  button->setToolButtonStyle(Qt::ToolButtonTextOnly);
  return button;
}

} // namespace

Sketch3dPalette::Sketch3dPalette(Sketch3dController& controller, CommandRegistry& registry, QWidget* parent)
    : QWidget(parent), m_c(controller) {
  setObjectName(QStringLiteral("sketch3dPalette"));
  auto* layout = new QVBoxLayout(this);
  if (chromeStyle() == ChromeStyle::Floating) {
    layout->setContentsMargins(4, 4, 4, 4); // the card has its own margins
  }
  const auto label = [layout](const QString& name) {
    auto* made = new QLabel;
    made->setObjectName(name);
    made->setWordWrap(true);
    made->setTextInteractionFlags(Qt::TextSelectableByMouse);
    layout->addWidget(made);
    return made;
  };
  m_title = label(QStringLiteral("sketch3dTitle"));
  // The 2D Sketch Palette's `3D Sketch` box, here checked (mitcad#1544).
  // Unchecking it turns the sketch back into 2D when it all lies in its
  // sketch plane (mitcad#1550); otherwise it is disabled naming what does
  // not (refresh).
  m_kind = new QCheckBox(tr("3D Sketch"));
  m_kind->setObjectName(QStringLiteral("sketch3dKind"));
  m_kind->setChecked(true);
  m_kind->setEnabled(false);
  m_kind->setFocusPolicy(Qt::NoFocus);
  connect(m_kind, &QCheckBox::clicked, this, [this](bool checked) {
    if (!checked) {
      m_kind->setChecked(true); // the model's answer decides
      emit convertTo2dRequested();
    }
  });
  layout->addWidget(m_kind);
  m_status = label(QStringLiteral("sketch3dStatus"));
  // A drawing tool's options right under the status, where they are seen
  // first (mitcad#1525); the details give way to them meanwhile.
  buildTool();
  layout->addWidget(m_toolBox);
  // With no tool, the editing (mitcad#1526) in the same place.
  buildEdit();
  layout->addWidget(m_editBox);
  // What is selected, right under the relations and dimensions that apply
  // to it, above the lists: under them a long card had it out of sight
  // (mitcad#1592); a new selection's is scrolled into sight
  // (logEditLayout).
  m_selection = label(QStringLiteral("sketch3dSelection"));
  layout->addWidget(m_itemsBox);
  layout->addWidget(m_refsBox);
  m_details = label(QStringLiteral("sketch3dDetails"));
  m_hover = label(QStringLiteral("sketch3dHover"));
  m_tools = label(QStringLiteral("sketch3dTools"));
  m_tools->setForegroundRole(QPalette::PlaceholderText);
  m_tools->setText(tr("Point, Line, the arcs and the splines of the SKETCH tab draw in space, and Sketch Fillet "
                      "rounds corners of lines; Coincident, Tangent, Equal, Fix, Midpoint and Sketch Dimension "
                      "apply to the selection, and the relations and dimensions above (Parallel, Perpendicular, "
                      "Angle, Symmetric, Concentric, Curvature, On Plane among them); the other 2D sketch tools do "
                      "not apply to a 3D sketch. Drag a point, a curve or a tangent handle to move it."));
  if (QAction* finish = registry.action(QStringLiteral("sketch.finish"))) {
    if (chromeStyle() == ChromeStyle::Floating) {
      m_finish = new CapsuleButton(finish->text(), true);
      m_finish->setDefaultAction(finish); // text and state follow the action
      m_finish->setToolButtonStyle(Qt::ToolButtonTextOnly);
    } else {
      m_finish = new QToolButton;
      m_finish->setDefaultAction(finish);
      m_finish->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
      m_finish->setFocusPolicy(Qt::NoFocus);
    }
    m_finish->setObjectName(QStringLiteral("sketch3dFinish"));
    layout->addWidget(m_finish, 0, Qt::AlignRight);
  }
  layout->addStretch();
  connect(&m_c, &Sketch3dController::changed, this, &Sketch3dPalette::refresh);
  connect(&m_c, &Sketch3dController::toolStateChanged, this, &Sketch3dPalette::refreshTool);
  connect(&m_c, &Sketch3dController::toolChanged, this, [this] {
    // The 3D Sketch box is disabled while a tool runs: its look follows the
    // tool's start and end at once, not the next change of the sketch
    // (mitcad#1592).
    refreshKind();
    refreshTool();
    // The editing rows give way to the tool's at once, before its places
    // are logged (mitcad#1526), and come back when it ends.
    refreshEdit();
    // Where the tool's controls are, once the card is laid out again.
    m_loggedLayout.clear();
    TestSync::singleShot(200, this, [this] { logToolLayout(); });
  });
  refresh();
}

void Sketch3dPalette::buildTool() {
  m_toolBox = new QWidget;
  m_toolBox->setObjectName(QStringLiteral("sketch3dTool"));
  auto* column = new QVBoxLayout(m_toolBox);
  column->setContentsMargins(0, 4, 0, 4);
  m_toolTitle = new QLabel;
  column->addWidget(m_toolTitle);
  m_toolHint = new QLabel;
  m_toolHint->setWordWrap(true);
  m_toolHint->setForegroundRole(QPalette::PlaceholderText);
  column->addWidget(m_toolHint);

  const auto row = [column](const QString& caption) {
    auto* line = new QWidget;
    auto* layout = new QHBoxLayout(line);
    layout->setContentsMargins(0, 0, 0, 0);
    if (!caption.isEmpty()) {
      layout->addWidget(new QLabel(caption));
    }
    column->addWidget(line);
    return layout;
  };
  const auto group = [this](QHBoxLayout* layout, const QList<std::pair<QString, QToolButton*>>& buttons) {
    auto* exclusive = new QButtonGroup(this);
    exclusive->setExclusive(true);
    for (const auto& [name, button] : buttons) {
      button->setObjectName(QStringLiteral("sketch3dControl ") + name);
      layout->addWidget(button);
      exclusive->addButton(button);
      m_controls.insert(name, button);
    }
    layout->addStretch(1);
  };

  // The temporary plane through the anchor.
  QHBoxLayout* planes = row(tr("Plane"));
  m_planeRow = planes->parentWidget();
  QToolButton* xy = choiceButton(QStringLiteral("XY"), tr("Place points on the XY plane through the anchor"));
  QToolButton* yz = choiceButton(QStringLiteral("YZ"), tr("Place points on the YZ plane through the anchor"));
  QToolButton* zx = choiceButton(QStringLiteral("ZX"), tr("Place points on the ZX plane through the anchor"));
  group(planes, {{QStringLiteral("Plane XY"), xy}, {QStringLiteral("Plane YZ"), yz}, {QStringLiteral("Plane ZX"), zx}});
  connect(xy, &QToolButton::clicked, this, [this] { m_c.setPlane(Plane3::XY); });
  connect(yz, &QToolButton::clicked, this, [this] { m_c.setPlane(Plane3::YZ); });
  connect(zx, &QToolButton::clicked, this, [this] { m_c.setPlane(Plane3::ZX); });

  // How the pointer places a point.
  QHBoxLayout* places = row(tr("Place"));
  m_placeRow = places->parentWidget();
  QToolButton* onPlane = choiceButton(tr("On Plane"), tr("On the active plane"));
  QToolButton* alongX = choiceButton(tr("X"), tr("Along the X axis through the anchor"));
  QToolButton* alongY = choiceButton(tr("Y"), tr("Along the Y axis through the anchor"));
  QToolButton* alongZ = choiceButton(tr("Z"), tr("Along the Z axis through the anchor"));
  QToolButton* normal = choiceButton(tr("Normal"),
                                     tr("Keep the point's place in the plane and change its elevation along the "
                                        "plane's normal"));
  group(places, {{QStringLiteral("On Plane"), onPlane},
                 {QStringLiteral("Axis X"), alongX},
                 {QStringLiteral("Axis Y"), alongY},
                 {QStringLiteral("Axis Z"), alongZ},
                 {QStringLiteral("Normal"), normal}});
  connect(onPlane, &QToolButton::clicked, this, [this] { m_c.setPlacementMode(Placement::Mode::Plane); });
  connect(alongX, &QToolButton::clicked, this, [this] { m_c.setPlacementMode(Placement::Mode::Axis, Axis3::X); });
  connect(alongY, &QToolButton::clicked, this, [this] { m_c.setPlacementMode(Placement::Mode::Axis, Axis3::Y); });
  connect(alongZ, &QToolButton::clicked, this, [this] { m_c.setPlacementMode(Placement::Mode::Axis, Axis3::Z); });
  connect(normal, &QToolButton::clicked, this, [this] { m_c.setPlacementMode(Placement::Mode::Normal); });

  // The plane's free rotation (mitcad#1661): an angle per manipulator axis
  // (Enter rotates the plane by it; a handle's drag shows its angle here),
  // and Reset back to the sketch's XY at the anchor.
  QHBoxLayout* rotate = row(tr("Rotate"));
  m_rotateRow = rotate->parentWidget();
  for (const Axis3 axis : {Axis3::X, Axis3::Y, Axis3::Z}) {
    const QString key = axisKey(axis);
    rotate->addWidget(new QLabel(axisLabel(axis)));
    auto* edit = new QLineEdit;
    edit->setObjectName(QStringLiteral("sketch3dField rotate_") + key);
    const QString tip =
        tr("Type an angle and press Enter to rotate the plane about the manipulator's %1 axis").arg(axisLabel(axis));
    edit->setToolTip(tip);
    edit->setProperty("defaultTip", tip);
    edit->setMaximumWidth(64);
    edit->installEventFilter(this);
    rotate->addWidget(edit, 1);
    m_rotateFields.insert(key, edit);
    // What is typed, for UI tests (Enter rotates by it).
    connect(edit, &QLineEdit::textEdited, this, [key](const QString& text) {
      qDebug().noquote() << QStringLiteral("3D sketch field rotate_%1: %2").arg(key, text);
    });
  }
  auto* reset = new QToolButton;
  reset->setText(tr("Reset"));
  reset->setToolTip(tr("Back to the sketch's XY plane at the anchor, not rotated"));
  reset->setFocusPolicy(Qt::NoFocus);
  reset->setObjectName(QStringLiteral("sketch3dControl Reset Plane"));
  m_controls.insert(QStringLiteral("Reset Plane"), reset);
  rotate->addWidget(reset);
  connect(reset, &QToolButton::clicked, this, [this] { m_c.resetPlane(); });

  // What the manipulator draws: the temporary plane and the rotation
  // handles (the placement is the same either way).
  QHBoxLayout* drawn = row(tr("Show"));
  m_showRow = drawn->parentWidget();
  m_showPlane = new QCheckBox(tr("Plane"));
  m_showPlane->setToolTip(tr("Draw the temporary plane through the anchor"));
  m_showHandles = new QCheckBox(tr("Handles"));
  m_showHandles->setToolTip(tr("Draw the rotation handles: drag one to rotate the plane about its axis"));
  for (QCheckBox* box : {m_showPlane, m_showHandles}) {
    box->setFocusPolicy(Qt::NoFocus);
    drawn->addWidget(box);
  }
  drawn->addStretch(1);
  connect(m_showPlane, &QCheckBox::clicked, this, [this](bool on) { m_c.setShowPlane(on); });
  connect(m_showHandles, &QCheckBox::clicked, this, [this](bool on) { m_c.setShowHandles(on); });

  // A Center Point Arc's normal: the active plane's, or picked from a
  // line, a straight edge, an axis or a planar face.
  QHBoxLayout* normals = row(tr("Normal"));
  m_normalRow = normals->parentWidget();
  m_normalText = new QLabel;
  m_normalText->setWordWrap(true);
  normals->addWidget(m_normalText, 1);
  QToolButton* pickNormal =
      choiceButton(tr("Pick"), tr("Pick a line, a straight edge, an axis or a planar face for the arc's normal"));
  QToolButton* fromPlane = new QToolButton;
  fromPlane->setText(tr("Plane"));
  fromPlane->setToolTip(tr("The arc's normal is the active plane's"));
  fromPlane->setFocusPolicy(Qt::NoFocus);
  for (const auto& [name, button] :
       QList<std::pair<QString, QToolButton*>>{{QStringLiteral("Pick Normal"), pickNormal},
                                               {QStringLiteral("Normal from Plane"), fromPlane}}) {
    button->setObjectName(QStringLiteral("sketch3dControl ") + name);
    normals->addWidget(button);
    m_controls.insert(name, button);
  }
  connect(pickNormal, &QToolButton::clicked, this, [this] { m_c.startNormalPick(); });
  connect(fromPlane, &QToolButton::clicked, this, [this] { m_c.useActivePlaneNormal(); });

  // The options that store intent.
  QHBoxLayout* options = row(QString());
  m_keepAxis = new QCheckBox(tr("Keep Axis"));
  m_keepAxis->setToolTip(tr("Keep the line along the placement's axis: an axis relation is stored"));
  m_keepPlane = new QCheckBox(tr("Keep Plane"));
  m_keepPlane->setToolTip(tr("Keep the arc's normal along the active plane's normal: a relation is stored"));
  m_construction = new QCheckBox(tr("Construction"));
  for (QCheckBox* box : {m_keepAxis, m_keepPlane, m_construction}) {
    box->setFocusPolicy(Qt::NoFocus);
    options->addWidget(box);
  }
  options->addStretch(1);
  connect(m_keepAxis, &QCheckBox::clicked, this, [this](bool on) { m_c.setKeepAxis(on); });
  connect(m_keepPlane, &QCheckBox::clicked, this, [this](bool on) { m_c.setKeepPlane(on); });
  connect(m_construction, &QCheckBox::clicked, this, [this](bool on) { m_c.setConstruction(on); });

  // The input mode of a line.
  QHBoxLayout* modes = row(tr("Input"));
  m_modeRow = modes->parentWidget();
  QToolButton* offsets = choiceButton(tr("Offsets"), tr("Signed Delta X, Y and Z from the segment's start"));
  QToolButton* axisLength =
      choiceButton(tr("Axis and Length"), tr("An axis, its sense and the segment's true length"));
  group(modes, {{QStringLiteral("Offsets"), offsets}, {QStringLiteral("Axis and Length"), axisLength}});
  connect(offsets, &QToolButton::clicked, this, [this] { m_c.setInputMode(SegmentInput::Mode::Offsets); });
  connect(axisLength, &QToolButton::clicked, this, [this] { m_c.setInputMode(SegmentInput::Mode::AxisLength); });
  QHBoxLayout* axes = row(tr("Axis"));
  m_axisRow = axes->parentWidget();
  QToolButton* lengthX = choiceButton(QStringLiteral("X"), tr("Along X"));
  QToolButton* lengthY = choiceButton(QStringLiteral("Y"), tr("Along Y"));
  QToolButton* lengthZ = choiceButton(QStringLiteral("Z"), tr("Along Z"));
  QToolButton* sense = choiceButton(QStringLiteral("+"), tr("The sense along the axis: + or -"));
  group(axes, {{QStringLiteral("Length axis X"), lengthX},
               {QStringLiteral("Length axis Y"), lengthY},
               {QStringLiteral("Length axis Z"), lengthZ}});
  axes->insertWidget(axes->count() - 1, sense);
  sense->setObjectName(QStringLiteral("sketch3dControl Sense"));
  m_controls.insert(QStringLiteral("Sense"), sense);
  connect(lengthX, &QToolButton::clicked, this, [this] { m_c.setInputAxis(Axis3::X); });
  connect(lengthY, &QToolButton::clicked, this, [this] { m_c.setInputAxis(Axis3::Y); });
  connect(lengthZ, &QToolButton::clicked, this, [this] { m_c.setInputAxis(Axis3::Z); });
  connect(sense, &QToolButton::clicked, this, [this] { m_c.setInputNegative(!m_c.input().negative); });

  // The fields: a typed value drives, an untouched one measures.
  auto* fields = new QWidget;
  auto* grid = new QGridLayout(fields);
  grid->setContentsMargins(0, 0, 0, 0);
  int line = 0;
  for (const char* id : kFieldIds) {
    const QString key = QString::fromLatin1(id);
    auto* caption = new QLabel(Sketch3dController::fieldLabel(key));
    auto* edit = new QLineEdit;
    edit->setObjectName(QStringLiteral("sketch3dField ") + key);
    edit->setToolTip(tr("Type a value or an expression to make it driving; empty, it shows the measurement"));
    edit->installEventFilter(this);
    grid->addWidget(caption, line, 0);
    grid->addWidget(edit, line, 1);
    m_fieldRows.insert(key, caption); // shown with its field
    m_fields.insert(key, edit);
    connect(edit, &QLineEdit::textEdited, this, [this, key](const QString& text) { m_c.setField(key, text); });
    ++line;
  }
  column->addWidget(fields);

  m_snap = new QLabel;
  m_snap->setWordWrap(true);
  column->addWidget(m_snap);
  m_problem = new QLabel;
  m_problem->setWordWrap(true);
  setErrorStyleSheet(m_problem);
  column->addWidget(m_problem);
  m_release = new QToolButton;
  m_release->setText(tr("Release from Plane"));
  m_release->setToolTip(tr("Lets the point leave the sketch plane: On Sketch Plane no longer keeps it"));
  m_release->setFocusPolicy(Qt::NoFocus);
  m_release->setObjectName(QStringLiteral("sketch3dControl Release from Plane"));
  m_controls.insert(QStringLiteral("Release from Plane"), m_release);
  connect(m_release, &QToolButton::clicked, this, [this] { m_c.releaseHeldPoint(); });
  column->addWidget(m_release, 0, Qt::AlignLeft);
  // The rows from the top: when the snap or problem line under the fields
  // goes (the pointer leaves the view for a field), the box keeps its old
  // height for one layout pass, and without a stretch its rows were spread
  // over it, the field moving from under the pointer for that moment.
  column->addStretch(1);
  m_toolBox->hide();
}

void Sketch3dPalette::refreshKeyTips() {
  // The manipulator's keys in its tooltips (mitcad#1661), as Keyboard
  // Shortcuts has them now.
  const QString next = m_c.keyText(Sketch3dController::kNextPlaneCommand);
  const QString previous = m_c.keyText(Sketch3dController::kPreviousPlaneCommand);
  const QString normal = m_c.keyText(Sketch3dController::kNormalCommand);
  QStringList planeKeys;
  if (!next.isEmpty()) {
    planeKeys << tr("%1: the next plane").arg(next);
  }
  if (!previous.isEmpty()) {
    planeKeys << tr("%1: the previous plane").arg(previous);
  }
  const QString normalKeys = normal.isEmpty() ? QString() : tr("%1: On Plane or Normal").arg(normal);
  const auto tip = [this](const QString& name, const QString& text, const QString& keys) {
    QToolButton* button = m_controls.value(name);
    const QString shown = keys.isEmpty() ? text : tr("%1 (%2)").arg(text, keys);
    if (button != nullptr && button->toolTip() != shown) {
      button->setToolTip(shown);
    }
  };
  const QString planes = planeKeys.join(QStringLiteral(", "));
  tip(QStringLiteral("Plane XY"), tr("Place points on the XY plane through the anchor"), planes);
  tip(QStringLiteral("Plane YZ"), tr("Place points on the YZ plane through the anchor"), planes);
  tip(QStringLiteral("Plane ZX"), tr("Place points on the ZX plane through the anchor"), planes);
  tip(QStringLiteral("On Plane"), tr("On the active plane"), normalKeys);
  tip(QStringLiteral("Normal"),
      tr("Keep the point's place in the plane and change its elevation along the plane's normal"), normalKeys);
  // What the tooltips say, for UI tests (once per change).
  const QString logged = QStringLiteral("3D sketch control keys: planes %1; normal %2")
                             .arg(planes.isEmpty() ? QStringLiteral("none") : planes,
                                  normalKeys.isEmpty() ? QStringLiteral("none") : normalKeys);
  if (logged != m_loggedKeyTips) {
    m_loggedKeyTips = logged;
    qDebug().noquote() << logged;
  }
}

void Sketch3dPalette::opened() {
  // The 2D editor may have taken the sketch between (a conversion and its
  // undo), so the last list built may be this sketch's: an opening never
  // reveals the list, which scrolled the 3D Sketch box out of the card's
  // sight.
  m_itemsSketch.clear();
  m_revealItems = false;
  m_loggedEditLayout.clear();
  for (QWidget* up = parentWidget(); up != nullptr; up = up->parentWidget()) {
    if (auto* scroll = qobject_cast<QScrollArea*>(up)) {
      scroll->verticalScrollBar()->setValue(0);
      break;
    }
  }
  refresh();
}

void Sketch3dPalette::refresh() {
  const Sketch3dModel& model = m_c.model();
  if (!m_c.isActive()) {
    m_title->clear();
    m_status->clear();
    m_details->clear();
    m_selection->clear();
    m_hover->clear();
    m_logged.clear();
    m_itemsSketch.clear(); // the next opening scrolls nothing
    // The next opening logs its places again, also where they are as
    // they were when this one ended: a test waits for them before it
    // clicks (the same list of an edited sketch is not built again).
    m_loggedEditLayout.clear();
    m_selectionShown.clear();
    m_revealSelection = false;
    refreshKind();
    refreshTool();
    refreshEdit();
    return;
  }
  m_title->setText(tr("<b>3D Sketch</b> %1").arg(model.name.toHtmlEscaped()));
  refreshKind();
  m_status->setText(statusText());
  m_details->setText(detailLines(model).join(QLatin1Char('\n')));
  const QStringList selected = m_c.selectionTexts();
  m_selection->setText(selected.isEmpty() ? QString() : tr("Selected:\n%1").arg(selected.join(QLatin1Char('\n'))));
  m_selection->setVisible(!selected.isEmpty());
  // A new selection's description is brought into the card's sight once
  // the card is laid out again (logEditLayout): below the lists of a long
  // card it was out of it (mitcad#1592). What is selected counts, not its
  // words: a drag that moves the selected point does not scroll the card.
  const EditSelection chosen = m_c.editSelection();
  QString shown = (chosen.points + chosen.curves).join(QLatin1Char(','));
  if (shown.isEmpty()) {
    shown = selected.join(QLatin1Char('\n'));
  }
  if (shown != m_selectionShown) {
    m_selectionShown = shown;
    m_revealSelection = !shown.isEmpty() && !m_c.toolActive();
  }
  m_hover->setText(m_c.hoverText().isEmpty() ? QString() : tr("Under the pointer: %1").arg(m_c.hoverText()));
  m_hover->setVisible(!m_c.hoverText().isEmpty() && !m_c.toolActive());
  // What the palette says, for UI tests (once per change): the selection
  // for agents, with the ids and coordinates, then as shown (mitcad#1652).
  const QString picked = selected.isEmpty() ? QString()
                                            : QStringLiteral(" | selected %1 | shown: %2")
                                                  .arg(m_c.selectionDetails().join(QStringLiteral(" / ")),
                                                       selected.join(QStringLiteral(" / ")));
  const QString logged = QStringLiteral("3D Sketch Palette: %1 | %2%3").arg(model.name, m_status->text(), picked);
  if (logged != m_logged) {
    m_logged = logged;
    qDebug().noquote() << logged;
  }
  refreshTool();
  refreshEdit();
}

void Sketch3dPalette::refreshKind() {
  if (!m_c.isActive()) {
    m_loggedKind.clear(); // the next opening logs it again
    return;
  }
  // The 3D Sketch box (mitcad#1550): unchecking it turns the sketch back
  // into 2D when it all lies in its sketch plane and no tool is active;
  // disabled with why otherwise.
  const Sketch3dModel& model = m_c.model();
  const bool enabled = model.flat && !m_c.toolActive();
  QString why;
  if (!model.flat) {
    why = model.offPlane.isEmpty() ? tr("It must solve before it can turn back into 2D.")
                                   : tr("Not all in its sketch plane: %1").arg(model.offPlaneShown.value(0));
  } else if (m_c.toolActive()) {
    why = tr("Finish or Escape the active tool first.");
  } else {
    why = tr("Uncheck to turn it back into a 2D sketch.");
  }
  if (m_kind->isEnabled() != enabled) {
    m_kind->setEnabled(enabled); // repaints it greyed or not
  }
  if (m_kind->toolTip() != why) {
    m_kind->setToolTip(why);
  }
  // What the box shows, for UI tests (once per change): the log and the
  // look agree whenever a tool starts or ends (mitcad#1592).
  const QString logged = QStringLiteral("3D Sketch Palette 3D Sketch box: enabled %1: %2")
                             .arg(enabled ? QStringLiteral("yes") : QStringLiteral("no"), why);
  if (logged != m_loggedKind) {
    m_loggedKind = logged;
    qDebug().noquote() << logged;
  }
}

void Sketch3dPalette::refreshTool() {
  const bool active = m_c.isActive() && m_c.toolActive();
  m_toolBox->setVisible(active);
  m_tools->setVisible(!active);
  m_details->setVisible(!active);
  if (active) {
    // A tool's options take the editing rows' place (mitcad#1526).
    m_editBox->hide();
    m_itemsBox->hide();
    m_refsBox->hide();
  }
  if (!active) {
    m_toolShown = false;
    return;
  }
  m_toolTitle->setText(QStringLiteral("<b>%1</b>").arg(m_c.toolName().toHtmlEscaped()));
  m_toolHint->setText(m_c.toolHint());
  const Placement& placement = m_c.placement();
  const auto check = [this](const QString& name, bool on) {
    if (QToolButton* button = m_controls.value(name)) {
      button->setChecked(on);
    }
  };
  check(QStringLiteral("Plane XY"), placement.plane == Plane3::XY);
  check(QStringLiteral("Plane YZ"), placement.plane == Plane3::YZ);
  check(QStringLiteral("Plane ZX"), placement.plane == Plane3::ZX);
  check(QStringLiteral("On Plane"), placement.mode == Placement::Mode::Plane);
  check(QStringLiteral("Axis X"), placement.mode == Placement::Mode::Axis && placement.axis == Axis3::X);
  check(QStringLiteral("Axis Y"), placement.mode == Placement::Mode::Axis && placement.axis == Axis3::Y);
  check(QStringLiteral("Axis Z"), placement.mode == Placement::Mode::Axis && placement.axis == Axis3::Z);
  check(QStringLiteral("Normal"), placement.mode == Placement::Mode::Normal);
  refreshKeyTips();
  // Fillet places no point (mitcad#1548): no plane, no placement.
  const bool fillet = m_c.tool() == Sketch3dTool::Fillet;
  m_planeRow->setVisible(!fillet);
  m_placeRow->setVisible(!fillet);
  // The plane's free rotation (mitcad#1661): the point tools'; a handle's
  // drag shows its angle in its axis' field.
  const bool rotation = m_c.offersRotation();
  m_rotateRow->setVisible(rotation);
  m_showRow->setVisible(!fillet);
  m_showHandles->setVisible(rotation);
  m_showPlane->setChecked(m_c.showPlane());
  m_showHandles->setChecked(m_c.showHandles());
  const auto dragged = m_c.rotationDrag();
  for (auto it = m_rotateFields.cbegin(); it != m_rotateFields.cend(); ++it) {
    const QString angle = dragged && axisKey(dragged->first) == it.key() ? degreesText(dragged->second) : QString();
    if (it.value()->placeholderText() != angle) {
      it.value()->setPlaceholderText(angle);
    }
  }
  const bool centerArc = m_c.tool() == Sketch3dTool::ArcCenter;
  m_normalRow->setVisible(centerArc);
  if (centerArc) {
    m_normalText->setText(m_c.pickingNormal() ? tr("click a line or a planar face") : m_c.arcNormalText());
    check(QStringLiteral("Pick Normal"), m_c.pickingNormal());
  }
  m_keepAxis->setVisible(m_c.offersKeepAxis());
  m_keepAxis->setChecked(m_c.keepAxis());
  m_keepPlane->setVisible(m_c.offersKeepPlane());
  m_keepPlane->setChecked(m_c.keepPlane());
  m_construction->setVisible(m_c.tool() != Sketch3dTool::Point && m_c.tool() != Sketch3dTool::Text && !fillet);
  m_construction->setChecked(m_c.construction());
  const SegmentInput& input = m_c.input();
  const QStringList shown = m_c.fieldIds();
  m_modeRow->setVisible(m_c.offersAxisLength() && !shown.isEmpty());
  check(QStringLiteral("Offsets"), input.mode == SegmentInput::Mode::Offsets);
  check(QStringLiteral("Axis and Length"), input.mode == SegmentInput::Mode::AxisLength);
  m_axisRow->setVisible(m_c.offersAxisLength() && input.mode == SegmentInput::Mode::AxisLength && !shown.isEmpty());
  check(QStringLiteral("Length axis X"), input.axis == Axis3::X);
  check(QStringLiteral("Length axis Y"), input.axis == Axis3::Y);
  check(QStringLiteral("Length axis Z"), input.axis == Axis3::Z);
  if (QToolButton* sense = m_controls.value(QStringLiteral("Sense"))) {
    sense->setText(input.negative ? QStringLiteral("-") : QStringLiteral("+"));
    sense->setChecked(input.negative);
  }
  for (auto it = m_fields.begin(); it != m_fields.end(); ++it) {
    const QString& id = it.key();
    QLineEdit* edit = it.value();
    const bool visible = shown.contains(id);
    edit->setVisible(visible);
    m_fieldRows.value(id)->setVisible(visible);
    if (!visible) {
      continue;
    }
    const TypedValue& value = m_c.field(id);
    // An untouched field shows the live measurement greyed; a typed one
    // its text (not changed while it is typed in).
    if (!value.edited) {
      if (!edit->text().isEmpty()) {
        edit->clear(); // taken back, or the segment it typed committed
      }
      edit->setPlaceholderText(m_c.measured(id));
    } else if (!edit->hasFocus() && edit->text() != value.text) {
      edit->setText(value.text);
    }
    // Set only when they change: the pointer's moves refresh the fields.
    // The error border of the app's fields, in the error role.
    const bool invalid = value.edited && !value.valid;
    if (edit->property("invalid").toBool() != invalid) {
      edit->setProperty("invalid", invalid);
      setErrorStyleSheet(edit, invalid ? QStringLiteral("QLineEdit { border: 1px solid %1; }") : QString());
    }
    const QString tip = value.edited && !value.valid
                            ? value.problem
                            : tr("Type a value or an expression to make it driving; empty, it shows the measurement");
    if (edit->toolTip() != tip) {
      edit->setToolTip(tip);
    }
  }
  const QString snap = m_c.snapText();
  m_snap->setText(snap.isEmpty() ? QString() : tr("Snap: %1").arg(snap));
  m_snap->setVisible(!snap.isEmpty());
  m_problem->setText(m_c.problem());
  m_problem->setVisible(!m_c.problem().isEmpty());
  m_release->setVisible(!m_c.planeHeldPoint().isEmpty());
  // The places move when a row above them shows or hides, or the hint
  // wraps on another number of lines: every row counts (Keep Axis going
  // with the snap moved Text's fields up a row without a new log, and a
  // click on the field's old place hit the row below it). The snap and
  // problem lines under the fields move only Release from Plane.
  QString layout = shown.join(QLatin1Char(',')) + QLatin1Char('|') + m_toolHint->text();
  // The placement too (mitcad#1661): a switch by the buttons or by the
  // manipulator's keys (Tab, Shift+Tab, Up Arrow) logs the places again,
  // so a test clicks where the controls are after it.
  layout += QStringLiteral("|%1 %2 %3")
                .arg(static_cast<int>(placement.plane))
                .arg(static_cast<int>(placement.mode))
                .arg(static_cast<int>(placement.axis));
  const QWidget* const rows[] = {m_planeRow, m_placeRow,  m_rotateRow,    m_showRow, m_showHandles, m_normalRow,
                                 m_modeRow,  m_axisRow,   m_keepAxis,     m_keepPlane, m_construction, m_release};
  for (const QWidget* row : rows) {
    layout += row->isVisibleTo(m_toolBox) ? QLatin1Char('1') : QLatin1Char('0');
  }
  if (m_release->isVisibleTo(m_toolBox)) {
    layout += m_snap->isVisibleTo(m_toolBox) ? QStringLiteral("|snap") : QString();
    layout += m_problem->isVisibleTo(m_toolBox) ? QStringLiteral("|problem ") + m_problem->text() : QString();
  }
  if (!m_toolShown || layout != m_loggedLayout) {
    m_toolShown = true;
    m_loggedLayout = layout;
    TestSync::singleShot(200, this, [this] { logToolLayout(); });
  }
}

void Sketch3dPalette::logToolLayout() {
  if (!m_c.toolActive() || !m_toolBox->isVisible()) {
    return;
  }
  for (auto it = m_controls.cbegin(); it != m_controls.cend(); ++it) {
    if (it.value()->isVisible()) {
      const QPoint at = GlassCard::mapToHost(it.value(), it.value()->rect().center());
      qDebug().noquote() << QStringLiteral("3D sketch control %1 at %2,%3").arg(it.key()).arg(at.x()).arg(at.y());
    }
  }
  for (QCheckBox* box : {m_keepAxis, m_keepPlane, m_construction}) {
    if (box->isVisible()) {
      const QPoint at = GlassCard::mapToHost(box, box->rect().center());
      qDebug().noquote() << QStringLiteral("3D sketch control %1 at %2,%3").arg(box->text()).arg(at.x()).arg(at.y());
    }
  }
  for (auto it = m_fields.cbegin(); it != m_fields.cend(); ++it) {
    if (it.value()->isVisible()) {
      const QPoint at = GlassCard::mapToHost(it.value(), it.value()->rect().center());
      qDebug().noquote() << QStringLiteral("3D sketch field %1 at %2,%3").arg(it.key()).arg(at.x()).arg(at.y());
    }
  }
  // The plane's rotation (mitcad#1661): "3D sketch field rotate_x at x,y",
  // "3D sketch control Show Plane at x,y".
  for (auto it = m_rotateFields.cbegin(); it != m_rotateFields.cend(); ++it) {
    if (it.value()->isVisible()) {
      const QPoint at = GlassCard::mapToHost(it.value(), it.value()->rect().center());
      qDebug().noquote() << QStringLiteral("3D sketch field rotate_%1 at %2,%3").arg(it.key()).arg(at.x()).arg(at.y());
    }
  }
  for (const auto& [name, box] : {std::pair<QString, QCheckBox*>{QStringLiteral("Show Plane"), m_showPlane},
                                  std::pair<QString, QCheckBox*>{QStringLiteral("Show Handles"), m_showHandles}}) {
    if (box->isVisible()) {
      const QPoint at = GlassCard::mapToHost(box, box->rect().center());
      qDebug().noquote() << QStringLiteral("3D sketch control %1 at %2,%3").arg(name).arg(at.x()).arg(at.y());
    }
  }
}

bool Sketch3dPalette::eventFilter(QObject* watched, QEvent* event) {
  // A list row's words under the pointer (mitcad#1652): the view paints
  // what the row names and its source until the pointer leaves them.
  const QString row = watched->property(kRowProperty).toString();
  if (!row.isEmpty()) {
    switch (event->type()) {
    case QEvent::Enter:
    case QEvent::MouseMove:
    case QEvent::HoverMove:
      if (m_c.paletteHover() != row) {
        m_c.setPaletteHover(row);
      }
      break;
    case QEvent::Leave:
    case QEvent::Hide:
      if (m_c.paletteHover() == row) {
        m_c.setPaletteHover(QString());
      }
      break;
    default:
      break;
    }
    return QWidget::eventFilter(watched, event);
  }
  auto* edit = qobject_cast<QLineEdit*>(watched);
  if (edit == nullptr) {
    return QWidget::eventFilter(watched, event);
  }
  // A dimension's value in the list (mitcad#1526).
  if (!m_itemFields.key(edit).isEmpty()) {
    return itemFieldEvent(edit, event) || QWidget::eventFilter(watched, event);
  }
  // A Rotate field (mitcad#1661).
  if (const QString axis = m_rotateFields.key(edit); !axis.isEmpty()) {
    return rotateFieldEvent(edit, axis, event) || QWidget::eventFilter(watched, event);
  }
  const QString id = m_fields.key(edit);
  switch (event->type()) {
  case QEvent::MouseButtonPress:
  case QEvent::MouseButtonDblClick:
    // A click on a field gives it the keyboard back from the view, also
    // after the manipulator's keys (mitcad#1661) and where the press does
    // not come from the system (a card's window activated by the press
    // gives the keyboard to the widget it had, not to the one clicked).
    if (!edit->hasFocus()) {
      edit->setFocus(Qt::MouseFocusReason);
      qDebug().noquote() << QStringLiteral("3D sketch field %1 takes the keyboard (click)").arg(id);
    }
    break;
  case QEvent::FocusIn:
    m_c.setFocusField(id);
    // A field Tab reaches is scrolled into the card's sight.
    for (QWidget* up = parentWidget(); up != nullptr; up = up->parentWidget()) {
      if (auto* scroll = qobject_cast<QScrollArea*>(up)) {
        scroll->ensureWidgetVisible(edit);
        break;
      }
    }
    break;
  case QEvent::FocusOut:
    if (m_c.focusField() == id) {
      m_c.setFocusField(QString());
    }
    break;
  case QEvent::ShortcutOverride: {
    // Enter and Esc are the field's: the window's OK and Cancel would
    // otherwise take them first; so is Tab, which steps through the
    // shown fields only.
    const int key = static_cast<QKeyEvent*>(event)->key();
    if (key == Qt::Key_Return || key == Qt::Key_Enter || key == Qt::Key_Escape || key == Qt::Key_Tab ||
        key == Qt::Key_Backtab) {
      event->accept();
      return true;
    }
    break;
  }
  case QEvent::KeyPress: {
    const auto* press = static_cast<QKeyEvent*>(event);
    const int key = press->key();
    if (key == Qt::Key_Tab || key == Qt::Key_Backtab) {
      // Tab and Shift+Tab: the next or the previous shown field, in the
      // order Delta X, Y, Z, Length, Radius, Arc Length; nothing commits.
      const bool back = key == Qt::Key_Backtab || press->modifiers().testFlag(Qt::ShiftModifier);
      QList<QLineEdit*> shown;
      for (const char* name : kFieldIds) {
        QLineEdit* field = m_fields.value(QString::fromLatin1(name));
        if (field != nullptr && m_c.fieldIds().contains(QString::fromLatin1(name))) {
          shown << field;
        }
      }
      const qsizetype at = shown.indexOf(edit);
      if (at >= 0 && !shown.isEmpty()) {
        const qsizetype count = shown.size();
        QLineEdit* next = shown.at((at + (back ? count - 1 : 1)) % count);
        next->setFocus(back ? Qt::BacktabFocusReason : Qt::TabFocusReason);
        next->selectAll();
        qDebug().noquote() << QStringLiteral("3D sketch field focus: %1").arg(m_fields.key(next));
      }
      return true;
    }
    if (key == Qt::Key_Return || key == Qt::Key_Enter) {
      // The field's value is the controller's already (textEdited); Enter
      // checks and commits once.
      m_c.setFocusField(id);
      m_c.confirm();
      m_c.setFocusField(edit->hasFocus() ? id : QString());
      refreshTool();
      return true;
    }
    if (key == Qt::Key_Escape) {
      m_c.setFocusField(id);
      m_c.cancel();
      refreshTool();
      return true;
    }
    break;
  }
  default:
    break;
  }
  return QWidget::eventFilter(watched, event);
}

bool Sketch3dPalette::rotateFieldEvent(QLineEdit* edit, const QString& axis, QEvent* event) {
  const QString id = QStringLiteral("rotate_") + axis;
  const auto showProblem = [edit](const QString& problem) {
    const bool invalid = !problem.isEmpty();
    if (edit->property("invalid").toBool() != invalid) {
      edit->setProperty("invalid", invalid);
      setErrorStyleSheet(edit, invalid ? QStringLiteral("QLineEdit { border: 1px solid %1; }") : QString());
    }
    edit->setToolTip(invalid ? problem : edit->property("defaultTip").toString());
  };
  switch (event->type()) {
  case QEvent::FocusIn:
    m_c.setFocusField(id); // a field has the keyboard
    qDebug().noquote() << QStringLiteral("3D sketch field focus: %1").arg(id);
    break;
  case QEvent::FocusOut:
    if (m_c.focusField() == id) {
      m_c.setFocusField(QString());
    }
    break;
  case QEvent::ShortcutOverride: {
    // Enter and Esc are the field's, not the window's OK and Cancel.
    const int key = static_cast<QKeyEvent*>(event)->key();
    if (key == Qt::Key_Return || key == Qt::Key_Enter || key == Qt::Key_Escape) {
      event->accept();
      return true;
    }
    break;
  }
  case QEvent::KeyPress: {
    const int key = static_cast<QKeyEvent*>(event)->key();
    if (key == Qt::Key_Return || key == Qt::Key_Enter) {
      // The plane rotated by the angle typed; the field empties for the
      // next one.
      const Axis3 about = axis == QStringLiteral("x")   ? Axis3::X
                          : axis == QStringLiteral("y") ? Axis3::Y
                                                        : Axis3::Z;
      const QString problem = m_c.rotatePlaneBy(about, edit->text());
      if (problem.isEmpty()) {
        edit->clear();
      }
      showProblem(problem);
      return true;
    }
    if (key == Qt::Key_Escape) {
      edit->clear();
      showProblem(QString());
      m_c.setFocusField(QString());
      qDebug().noquote() << QStringLiteral("3D sketch Esc: field %1 cleared").arg(id);
      m_c.focusView();
      return true;
    }
    break;
  }
  default:
    break;
  }
  return false;
}

// ---------------------------------------------------------------------------
// Editing (mitcad#1526)

void Sketch3dPalette::buildEdit() {
  m_editBox = new QWidget;
  m_editBox->setObjectName(QStringLiteral("sketch3dEdit"));
  auto* column = new QVBoxLayout(m_editBox);
  column->setContentsMargins(0, 4, 0, 4);
  const auto button = [this](const QString& name, const QString& text, const QString& tip) {
    auto* made = new QToolButton;
    made->setText(text);
    made->setToolTip(tip);
    made->setFocusPolicy(Qt::NoFocus);
    made->setToolButtonStyle(Qt::ToolButtonTextOnly);
    made->setObjectName(QStringLiteral("sketch3dEdit ") + name);
    m_editControls.insert(name, made);
    return made;
  };

  // What the last edit or pick says, and what it offers.
  m_notice = new QLabel;
  m_notice->setObjectName(QStringLiteral("sketch3dNotice"));
  m_notice->setWordWrap(true);
  column->addWidget(m_notice);
  auto* actions = new QWidget;
  auto* actionRow = new QHBoxLayout(actions);
  actionRow->setContentsMargins(0, 0, 0, 0);
  QToolButton* dependents =
      button(QStringLiteral("delete dependents"), tr("Delete with Dependents"),
             tr("Deletes the selected points with the curves, relations and dimensions that use them"));
  connect(dependents, &QToolButton::clicked, this, [this] { m_c.deleteWithDependents(); });
  QToolButton* endPick = button(QStringLiteral("end pick"), tr("End Pick"), tr("Ends picking a source (Esc)"));
  connect(endPick, &QToolButton::clicked, this, [this] { m_c.cancelSourcePick(); });
  actionRow->addWidget(dependents);
  actionRow->addWidget(endPick);
  // A plane of the sketch's own frame for Symmetric and On Plane
  // (mitcad#1661), shown while their plane is picked.
  const struct {
    const char* name;
    const char* text;
    int axis;
  } framePlanes[] = {{"plane XY", "XY", 2}, {"plane YZ", "YZ", 0}, {"plane ZX", "ZX", 1}};
  for (const auto& entry : framePlanes) {
    QToolButton* made = button(QString::fromLatin1(entry.name), QString::fromLatin1(entry.text),
                               tr("The sketch's %1 plane through its origin").arg(QString::fromLatin1(entry.text)));
    const int axis = entry.axis;
    connect(made, &QToolButton::clicked, this, [this, axis] { m_c.takeFramePlane(axis); });
    actionRow->addWidget(made);
  }
  actionRow->addStretch(1);
  column->addWidget(actions);

  // The plane or the axis a drag moves the place grabbed on.
  auto* dragLine = new QWidget;
  auto* dragRow = new QHBoxLayout(dragLine);
  dragRow->setContentsMargins(0, 0, 0, 0);
  dragRow->addWidget(new QLabel(tr("Drag on")));
  auto* exclusive = new QButtonGroup(this);
  exclusive->setExclusive(true);
  const struct {
    const char* name;
    const char* text;
    Plane3 plane;
  } planes[] = {{"drag XY", "XY", Plane3::XY}, {"drag YZ", "YZ", Plane3::YZ}, {"drag ZX", "ZX", Plane3::ZX}};
  for (const auto& entry : planes) {
    QToolButton* made = choiceButton(QString::fromLatin1(entry.text),
                                     tr("Drag on the %1 plane through the place grabbed").arg(QString::fromLatin1(entry.text)));
    made->setObjectName(QStringLiteral("sketch3dEdit ") + QString::fromLatin1(entry.name));
    m_editControls.insert(QString::fromLatin1(entry.name), made);
    exclusive->addButton(made);
    dragRow->addWidget(made);
    const Plane3 plane = entry.plane;
    connect(made, &QToolButton::clicked, this, [this, plane] { m_c.setDragPlane(plane); });
  }
  const struct {
    const char* name;
    const char* text;
    Axis3 axis;
  } axes[] = {{"drag axis X", "X", Axis3::X}, {"drag axis Y", "Y", Axis3::Y}, {"drag axis Z", "Z", Axis3::Z}};
  for (const auto& entry : axes) {
    QToolButton* made = choiceButton(QString::fromLatin1(entry.text),
                                     tr("Drag along %1 through the place grabbed").arg(QString::fromLatin1(entry.text)));
    made->setObjectName(QStringLiteral("sketch3dEdit ") + QString::fromLatin1(entry.name));
    m_editControls.insert(QString::fromLatin1(entry.name), made);
    exclusive->addButton(made);
    dragRow->addWidget(made);
    const Axis3 axis = entry.axis;
    connect(made, &QToolButton::clicked, this, [this, axis] { m_c.setDragAxis(axis); });
  }
  dragRow->addStretch(1);
  column->addWidget(dragLine);

  // The relations and dimensions the selection takes: two columns of
  // buttons as wide as their column, from the card's left edge. Three
  // columns of the longest names (Release from Plane, On Sketch Plane,
  // Point on Curve) were wider than the card in either layout, which cut
  // the third column at its right edge (mitcad#1592).
  const auto grid = [column](const QString& caption) {
    column->addWidget(new QLabel(caption));
    auto* area = new QWidget;
    auto* layout = new QGridLayout(area);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setHorizontalSpacing(2);
    layout->setVerticalSpacing(2);
    for (int at = 0; at < kEditColumns; ++at) {
      layout->setColumnStretch(at, 1);
    }
    column->addWidget(area);
    return layout;
  };
  const auto place = [](QGridLayout* layout, QToolButton* made, int index) {
    made->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    layout->addWidget(made, index / kEditColumns, index % kEditColumns);
  };
  QGridLayout* relations = grid(tr("Relations"));
  int index = 0;
  for (const Relation3 relation : allRelations()) {
    QToolButton* made = button(relationKey(relation), relationName(relation), QString());
    connect(made, &QToolButton::clicked, this, [this, relation] { m_c.applyRelation(relation); });
    place(relations, made, index++);
  }
  QToolButton* attach =
      button(QStringLiteral("attach"), tr("Attach"),
             tr("Attaches the selected point to the vertex, edge, point or curve you click next"));
  connect(attach, &QToolButton::clicked, this, [this] { m_c.startAttach(); });
  place(relations, attach, index);
  QGridLayout* dimensions = grid(tr("Dimensions"));
  index = 0;
  for (const Dimension3 dimension : allDimensions()) {
    QToolButton* made = button(dimensionKey(dimension), dimensionName(dimension), QString());
    connect(made, &QToolButton::clicked, this, [this, dimension] { m_c.applyDimension(dimension); });
    place(dimensions, made, index++);
  }
  m_editBox->hide();

  // Every relation and dimension, and every linked reference.
  const auto list = [](const QString& name, const QString& caption, QGridLayout** layout) {
    auto* box = new QWidget;
    box->setObjectName(name);
    auto* inner = new QVBoxLayout(box);
    inner->setContentsMargins(0, 4, 0, 4);
    auto* title = new QLabel(caption);
    title->setObjectName(name + QStringLiteral("Title"));
    title->setWordWrap(true);
    inner->addWidget(title);
    auto* area = new QWidget;
    *layout = new QGridLayout(area);
    (*layout)->setContentsMargins(0, 0, 0, 0);
    (*layout)->setColumnStretch(3, 1);
    inner->addWidget(area);
    box->hide();
    return box;
  };
  m_itemsBox = list(QStringLiteral("sketch3dItems"), tr("<b>Relations and dimensions</b>"), &m_itemsGrid);
  m_refsBox = list(QStringLiteral("sketch3dReferences"), tr("<b>Linked references</b>"), &m_refsGrid);
}

void Sketch3dPalette::refreshEdit() {
  const bool shown = m_c.isActive() && !m_c.toolActive();
  m_editBox->setVisible(shown);
  if (!shown) {
    m_itemsBox->setVisible(false);
    m_refsBox->setVisible(false);
    return;
  }
  const auto set = [](QToolButton* made, bool enabled, const QString& tip) {
    if (made == nullptr) {
      return;
    }
    if (made->isEnabled() != enabled) {
      made->setEnabled(enabled);
    }
    if (made->toolTip() != tip) {
      made->setToolTip(tip);
    }
  };
  // The drag's plane or axis.
  const Placement& drag = m_c.dragPlacement();
  const bool onPlane = drag.mode != Placement::Mode::Axis;
  const auto check = [this](const QString& name, bool on) {
    if (QToolButton* made = m_editControls.value(name)) {
      made->setChecked(on);
    }
  };
  check(QStringLiteral("drag XY"), onPlane && drag.plane == Plane3::XY);
  check(QStringLiteral("drag YZ"), onPlane && drag.plane == Plane3::YZ);
  check(QStringLiteral("drag ZX"), onPlane && drag.plane == Plane3::ZX);
  check(QStringLiteral("drag axis X"), !onPlane && drag.axis == Axis3::X);
  check(QStringLiteral("drag axis Y"), !onPlane && drag.axis == Axis3::Y);
  check(QStringLiteral("drag axis Z"), !onPlane && drag.axis == Axis3::Z);
  // Each relation and dimension: disabled with the reason when the
  // selection takes none.
  for (const Relation3 relation : allRelations()) {
    const EditCommand edit = m_c.relationFor(relation);
    set(m_editControls.value(relationKey(relation)), edit.valid(),
        edit.valid() ? tr("%1: adds it to the selection").arg(relationName(relation)) : edit.problem);
  }
  for (const Dimension3 dimension : allDimensions()) {
    const EditCommand edit = m_c.dimensionFor(dimension);
    set(m_editControls.value(dimensionKey(dimension)), edit.valid(),
        edit.valid() ? tr("%1: dimensions the selection").arg(dimensionName(dimension)) : edit.problem);
  }
  const EditSelection selection = m_c.editSelection();
  const bool onePoint = selection.points.size() == 1 && selection.curves.isEmpty();
  set(m_editControls.value(QStringLiteral("attach")), onePoint,
      onePoint ? tr("Attaches %1 to a source you click next")
                     .arg(entityName(m_c.model(), selection.points.front()))
               : tr("Select one point of the sketch to attach."));
  // The notice and what it offers.
  const QString notice = m_c.editNotice();
  if (m_notice->text() != notice) {
    m_notice->setText(notice);
  }
  m_notice->setVisible(!notice.isEmpty());
  m_editControls.value(QStringLiteral("delete dependents"))->setVisible(!m_c.pendingDelete().isEmpty());
  m_editControls.value(QStringLiteral("end pick"))
      ->setVisible(m_c.sourcePick() != Sketch3dController::SourcePick::None);
  const bool planePick = m_c.sourcePick() == Sketch3dController::SourcePick::Plane;
  for (const char* name : {"plane XY", "plane YZ", "plane ZX"}) {
    m_editControls.value(QString::fromLatin1(name))->setVisible(planePick);
  }
  refreshItems();
  refreshReferences();
  scheduleEditLayout();
}

namespace {

// Takes a grid's widgets away; the rebuild that calls it runs from the
// event loop, never inside an input event of one of them.
void clearGrid(QGridLayout* grid) {
  while (QLayoutItem* item = grid->takeAt(0)) {
    if (QWidget* widget = item->widget()) {
      widget->hide();
      widget->deleteLater();
    }
    delete item;
  }
}

} // namespace

void Sketch3dPalette::refreshItems() {
  // The rows are rebuilt from the event loop: a press and its release on
  // a row's button survive a change that comes between them.
  if (m_itemsQueued) {
    return;
  }
  m_itemsQueued = true;
  QTimer::singleShot(0, this, [this] {
    m_itemsQueued = false;
    buildItems();
  });
}

void Sketch3dPalette::refreshReferences() {
  if (m_refsQueued) {
    return;
  }
  m_refsQueued = true;
  QTimer::singleShot(0, this, [this] {
    m_refsQueued = false;
    buildReferences();
  });
}

void Sketch3dPalette::buildItems() {
  if (!m_c.isActive() || m_c.toolActive()) {
    m_itemsBox->hide();
    return;
  }
  const Sketch3dModel& model = m_c.model();
  const QString unit = m_c.unit();
  const double millimetres = m_c.millimetres();
  // Each row in words; the log and the agents read it with the ids
  // (mitcad#1652).
  QStringList lines;
  QStringList words;
  QString signature;
  for (const Sketch3dItem& item : model.items) {
    const QString line = itemText(model, item, unit, millimetres);
    lines << line;
    words << itemWords(model, item, unit, millimetres, m_c.sourceNames());
    signature += QStringLiteral("%1|%2|%3|%4\n").arg(line, words.last(), itemValueText(item, millimetres),
                                                     itemParameter(item));
  }
  m_itemsBox->setVisible(!model.items.empty());
  if (signature == m_itemsShown) {
    return;
  }
  // A value being typed keeps its row until it is entered.
  for (const QLineEdit* field : std::as_const(m_itemFields)) {
    if (field->hasFocus()) {
      return;
    }
  }
  m_itemsShown = signature;
  clearGrid(m_itemsGrid);
  m_itemFields.clear();
  for (auto it = m_editPlaces.begin(); it != m_editPlaces.end();) {
    it = it.key().startsWith(QStringLiteral("item ")) ? m_editPlaces.erase(it) : std::next(it);
  }
  // Each item on two rows: its words across the card, under them its
  // value, Driven and Remove from the left, so that none is pushed past a
  // narrow card's edge.
  int index = 0;
  for (const Sketch3dItem& item : model.items) {
    const QString id = item.id;
    const int row = 2 * index;
    QLabel* text = rowText(words.at(index), id);
    m_itemsGrid->addWidget(text, row, 0, 1, 4);
    m_editPlaces.insert(QStringLiteral("item %1 text").arg(id), text);
    if (item.dimension) {
      const QString value = itemValueText(item, millimetres);
      auto* field = new QLineEdit;
      field->setObjectName(QStringLiteral("sketch3dItem ") + id);
      if (item.driven) {
        field->setPlaceholderText(value);
        field->setToolTip(tr("Driven: it measures %1 %2; type a value to make it driving").arg(value, unit));
      } else {
        field->setText(value);
        field->setToolTip(tr("Parameter %1: type a value, an expression or a parameter's name; Enter sets it")
                              .arg(itemParameter(item)));
      }
      field->setMaximumWidth(110);
      field->installEventFilter(this);
      m_itemFields.insert(id, field);
      m_itemsGrid->addWidget(field, row + 1, 0);
      m_editPlaces.insert(QStringLiteral("item %1 field").arg(id), field);
      auto* driven = new QCheckBox(tr("Driven"));
      driven->setChecked(item.driven);
      driven->setFocusPolicy(Qt::NoFocus);
      driven->setToolTip(tr("A driven dimension only measures; a driving one sets the geometry"));
      connect(driven, &QCheckBox::clicked, this, [this, id](bool on) {
        qDebug().noquote() << QStringLiteral("3D sketch item %1 driven clicked").arg(id);
        m_c.setDriven(id, on);
      });
      m_itemsGrid->addWidget(driven, row + 1, 1);
      m_editPlaces.insert(QStringLiteral("item %1 driven").arg(id), driven);
    }
    auto* remove = new QToolButton;
    remove->setText(tr("Remove"));
    remove->setToolTip(tr("Removes %1").arg(entityName(model, id)));
    remove->setFocusPolicy(Qt::NoFocus);
    connect(remove, &QToolButton::clicked, this, [this, id] {
      qDebug().noquote() << QStringLiteral("3D sketch item %1 remove clicked").arg(id);
      m_c.removeItems({id});
    });
    m_itemsGrid->addWidget(remove, row + 1, 2);
    m_editPlaces.insert(QStringLiteral("item %1 remove").arg(id), remove);
    ++index;
  }
  // The new rows' places are logged again once laid out, also where they
  // did not move: a test waits for them before it clicks. The list is
  // brought into sight first.
  m_revealItems = m_itemsSketch == m_c.uid(); // an edit's, not the opening's
  m_itemsSketch = m_c.uid();
  m_loggedEditLayout.clear();
  scheduleEditLayout();
  // The list as it is now, for UI tests (once per change).
  // "3D sketch item k3 Length c3: 40.00 mm | shown: Length 1 on Line 1:
  // 40.00 mm".
  QStringList logLines;
  for (int i = 0; i < lines.size(); ++i) {
    logLines << QStringLiteral("3D sketch item %1 | shown: %2").arg(lines.at(i), words.at(i));
  }
  const QString logged = logLines.join(QLatin1Char('\n'));
  if (logged != m_loggedItems) {
    m_loggedItems = logged;
    for (const QString& line : std::as_const(logLines)) {
      qDebug().noquote() << line;
    }
  }
}

QLabel* Sketch3dPalette::rowText(const QString& words, const QString& id) {
  // Its words; the pointer over them paints the row's entities and source
  // in the view (mitcad#1652, eventFilter). Its id is the agents'.
  auto* text = new QLabel(words);
  text->setWordWrap(true);
  text->setObjectName(QStringLiteral("sketch3dRow ") + id);
  text->setProperty(kRowProperty, id);
  text->setMouseTracking(true);
  text->installEventFilter(this);
  return text;
}

void Sketch3dPalette::buildReferences() {
  if (!m_c.isActive() || m_c.toolActive()) {
    m_refsBox->hide();
    return;
  }
  const Sketch3dModel& model = m_c.model();
  // Each record in words; the log and the agents read it with the ids
  // (mitcad#1652).
  QString signature;
  QStringList texts;
  QStringList words;
  for (const Sketch3dReference& reference : model.references) {
    texts << referenceText(reference);
    words << referenceWords(model, reference, m_c.sourceNames());
    signature += texts.last() + QLatin1Char('|') + words.last() + QLatin1Char('\n');
  }
  // An upstream failure: the sketch keeps its definition, and the way
  // back is the reference's own Replace or Detach.
  const bool failed = !model.failedReferences().isEmpty() || model.failureKind == QStringLiteral("reference");
  signature += failed ? QStringLiteral("failed") : QString();
  m_refsBox->setVisible(!model.references.empty());
  if (signature == m_refsShown) {
    return;
  }
  m_refsShown = signature;
  if (auto* title = m_refsBox->findChild<QLabel*>(QStringLiteral("sketch3dReferencesTitle"))) {
    title->setText(failed ? tr("<b>Linked references</b><br>A source was not found: the sketch keeps its "
                               "definition and nothing was detached. Replace or Detach the failed reference to "
                               "recover, or put its source back.")
                          : tr("<b>Linked references</b>"));
  }
  clearGrid(m_refsGrid);
  for (auto it = m_editPlaces.begin(); it != m_editPlaces.end();) {
    it = it.key().startsWith(QStringLiteral("reference ")) ? m_editPlaces.erase(it) : std::next(it);
  }
  // Each record on two rows, as the items: its words, then its buttons.
  int index = 0;
  for (const Sketch3dReference& reference : model.references) {
    const QString id = reference.id;
    const int row = 2 * index;
    QLabel* text = rowText(words.at(index), id);
    if (reference.status == QStringLiteral("failed")) {
      setErrorStyleSheet(text);
    }
    m_refsGrid->addWidget(text, row, 0, 1, 4);
    m_editPlaces.insert(QStringLiteral("reference %1 text").arg(id), text);
    const QString name = entityName(model, id);
    auto* replace = new QToolButton;
    replace->setText(tr("Replace"));
    replace->setToolTip(tr("Replace Reference: %1 follows a source of its kind you click next").arg(name));
    replace->setFocusPolicy(Qt::NoFocus);
    connect(replace, &QToolButton::clicked, this, [this, id] {
      qDebug().noquote() << QStringLiteral("3D sketch reference %1 replace clicked").arg(id);
      m_c.startReplace(id);
    });
    m_refsGrid->addWidget(replace, row + 1, 0);
    m_editPlaces.insert(QStringLiteral("reference %1 replace").arg(id), replace);
    auto* detach = new QToolButton;
    detach->setText(tr("Detach"));
    detach->setToolTip(tr("Detach: the points on %1 come off it and stay where they are").arg(name));
    detach->setFocusPolicy(Qt::NoFocus);
    connect(detach, &QToolButton::clicked, this, [this, id] {
      qDebug().noquote() << QStringLiteral("3D sketch reference %1 detach clicked").arg(id);
      m_c.detachRecord(id);
    });
    m_refsGrid->addWidget(detach, row + 1, 1);
    m_editPlaces.insert(QStringLiteral("reference %1 detach").arg(id), detach);
    qDebug().noquote() << QStringLiteral("3D sketch reference %1 | shown: %2").arg(texts.at(index), words.at(index));
    ++index;
  }
  // The new rows' places are logged again once laid out, also where they
  // did not move: a test waits for them before it clicks.
  m_loggedEditLayout.clear();
  scheduleEditLayout();
}

bool Sketch3dPalette::itemFieldEvent(QLineEdit* edit, QEvent* event) {
  const QString id = m_itemFields.key(edit);
  switch (event->type()) {
  case QEvent::ShortcutOverride: {
    // Enter and Esc are the field's: the window's OK and Cancel would
    // otherwise take them first.
    const int key = static_cast<QKeyEvent*>(event)->key();
    if (key == Qt::Key_Return || key == Qt::Key_Enter || key == Qt::Key_Escape) {
      event->accept();
      return true;
    }
    return false;
  }
  case QEvent::KeyPress: {
    const int key = static_cast<QKeyEvent*>(event)->key();
    if (key == Qt::Key_Return || key == Qt::Key_Enter) {
      const QString text = edit->text();
      edit->clearFocus();
      m_c.focusView();
      const bool set = !text.trimmed().isEmpty() && m_c.setDimensionValue(id, text);
      // A refused or empty value goes back to the model's; a value set
      // rebuilt the list with the sketch's reload already (once: a second
      // rebuild would move the rows under a click on their way).
      if (!set) {
        m_itemsShown.clear();
        refreshItems();
      }
      return true;
    }
    if (key == Qt::Key_Escape) {
      edit->clearFocus();
      m_c.focusView();
      m_itemsShown.clear();
      refreshItems();
      qDebug().noquote() << QStringLiteral("3D sketch item %1: Esc took the typed value back").arg(id);
      return true;
    }
    return false;
  }
  default:
    return false;
  }
}

void Sketch3dPalette::scheduleEditLayout() {
  if (m_editLayoutPending) {
    return;
  }
  m_editLayoutPending = true;
  TestSync::singleShot(200, this, [this] {
    m_editLayoutPending = false;
    logEditLayout();
  });
}

void Sketch3dPalette::logEditLayout() {
  if (!m_c.isActive() || m_c.toolActive() || !m_editBox->isVisible()) {
    return;
  }
  // The card's scroll area, when the palette is in one.
  QScrollArea* scroll = nullptr;
  for (QWidget* up = parentWidget(); up != nullptr && scroll == nullptr; up = up->parentWidget()) {
    scroll = qobject_cast<QScrollArea*>(up);
  }
  // Scrolling the card (the wheel, its bar) moves every row: the places
  // are logged again where they are then.
  if (scroll != nullptr && !m_scrollWatched) {
    m_scrollWatched = true;
    connect(scroll->verticalScrollBar(), &QScrollBar::valueChanged, this, [this] { scheduleEditLayout(); });
  }
  // A list that the user's edit changed is brought into the card's sight
  // (a long card, a small screen: the row just edited stays where it can
  // be clicked), before the places are read: vertically only, its top
  // rows, as little as it takes. (ensureWidgetVisible also scrolled the
  // card sideways when the list was wider than it, which put the rows'
  // first column, the value fields, out of sight.)
  if (m_revealItems && scroll != nullptr && scroll->widget() != nullptr && m_itemsBox->isVisible()) {
    QScrollBar* bar = scroll->verticalScrollBar();
    const int height = scroll->viewport()->height();
    const int top = m_itemsBox->mapTo(scroll->widget(), QPoint(0, 0)).y();
    const int bottom = top + std::min(m_itemsBox->height(), std::max(height / 2, 1));
    if (top < bar->value()) {
      bar->setValue(top);
    } else if (bottom > bar->value() + height) {
      bar->setValue(bottom - height);
    }
  }
  m_revealItems = false;
  // A new selection's description is brought into sight the same way, its
  // whole height where the card is tall enough, else its top (mitcad#1592:
  // in the Docked card it was below the visible part).
  if (m_revealSelection && scroll != nullptr && scroll->widget() != nullptr && m_selection->isVisible()) {
    QScrollBar* bar = scroll->verticalScrollBar();
    const int height = scroll->viewport()->height();
    const int top = m_selection->mapTo(scroll->widget(), QPoint(0, 0)).y();
    const int bottom = top + std::min(m_selection->height(), std::max(height, 1));
    if (top < bar->value()) {
      bar->setValue(top);
    } else if (bottom > bar->value() + height) {
      bar->setValue(bottom - height);
    }
  }
  m_revealSelection = false;
  // While Symmetric or On Plane waits for its plane (mitcad#1661), the
  // sketch's plane buttons are in sight: the selection's reveal above may
  // have scrolled them out of the card's top.
  if (scroll != nullptr && scroll->widget() != nullptr &&
      m_c.sourcePick() == Sketch3dController::SourcePick::Plane) {
    if (const QWidget* first = m_editControls.value(QStringLiteral("plane XY")); first != nullptr && first->isVisible()) {
      QScrollBar* bar = scroll->verticalScrollBar();
      const int height = scroll->viewport()->height();
      const int top = first->mapTo(scroll->widget(), QPoint(0, 0)).y();
      const int bottom = top + first->height();
      if (top < bar->value()) {
        bar->setValue(top);
      } else if (bottom > bar->value() + height) {
        bar->setValue(bottom - height);
      }
    }
  }
  if (scroll != nullptr) {
    scroll->horizontalScrollBar()->setValue(0); // the card never shows a sideways part
  }
  QStringList lines;
  const auto place = [scroll](const QWidget* widget) {
    // Out of the card's sight a click would hit something else.
    if (scroll != nullptr) {
      const QRect shown(widget->mapTo(scroll->viewport(), QPoint(0, 0)), widget->size());
      if (!scroll->viewport()->rect().contains(shown.center())) {
        return QStringLiteral("out of sight");
      }
    }
    const QPoint at = GlassCard::mapToHost(widget, widget->rect().center());
    return QStringLiteral("at %1,%2").arg(at.x()).arg(at.y());
  };
  for (auto it = m_editControls.cbegin(); it != m_editControls.cend(); ++it) {
    if (it.value()->isVisible()) {
      lines << QStringLiteral("3D sketch edit control %1 %2").arg(it.key(), place(it.value()));
    }
  }
  for (auto it = m_editPlaces.cbegin(); it != m_editPlaces.cend(); ++it) {
    if (it.value()->isVisible()) {
      lines << QStringLiteral("3D sketch %1 %2").arg(it.key(), place(it.value()));
    }
  }
  // The selection's description (mitcad#1592).
  if (m_selection->isVisible()) {
    lines << QStringLiteral("3D sketch selection description %1").arg(place(m_selection));
  }
  // Whether every relation and dimension button is whole across the card
  // (mitcad#1592: the third column was cut at its right edge).
  if (scroll != nullptr) {
    QStringList cut;
    for (auto it = m_editControls.cbegin(); it != m_editControls.cend(); ++it) {
      const QWidget* control = it.value();
      if (control->isVisible()) {
        const QRect shown(control->mapTo(scroll->viewport(), QPoint(0, 0)), control->size());
        if (shown.left() < 0 || shown.right() >= scroll->viewport()->width()) {
          cut << it.key();
        }
      }
    }
    lines << QStringLiteral("3D sketch edit controls across the card: %1")
                 .arg(cut.isEmpty() ? QStringLiteral("all whole") : QStringLiteral("cut ") + cut.join(QStringLiteral(", ")));
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged == m_loggedEditLayout) {
    return;
  }
  m_loggedEditLayout = logged;
  for (const QString& line : std::as_const(lines)) {
    qDebug().noquote() << line;
  }
}

QString Sketch3dPalette::statusText() const { return sketch::statusText(m_c.model()); }

QJsonArray Sketch3dPalette::listState() const {
  QJsonArray rows;
  if (!m_c.isActive()) {
    return rows;
  }
  const Sketch3dModel& model = m_c.model();
  for (const Sketch3dItem& item : model.items) {
    rows.append(QJsonObject{
        {QStringLiteral("id"), item.id},
        {QStringLiteral("kind"), item.dimension ? QStringLiteral("dimension") : QStringLiteral("relation")},
        {QStringLiteral("text"), itemWords(model, item, m_c.unit(), m_c.millimetres(), m_c.sourceNames())},
        {QStringLiteral("detail"), itemText(model, item, m_c.unit(), m_c.millimetres())}});
  }
  for (const Sketch3dReference& reference : model.references) {
    rows.append(QJsonObject{{QStringLiteral("id"), reference.id},
                            {QStringLiteral("kind"), QStringLiteral("reference")},
                            {QStringLiteral("text"), referenceWords(model, reference, m_c.sourceNames())},
                            {QStringLiteral("detail"), referenceText(reference)}});
  }
  return rows;
}

void Sketch3dPalette::logLayout() const {
  if (m_finish != nullptr && m_finish->isVisible()) {
    const QPoint center = GlassCard::mapToHost(m_finish, m_finish->rect().center());
    qDebug().noquote() << QStringLiteral("3D Sketch Palette Finish Sketch at %1,%2").arg(center.x()).arg(center.y());
  }
  // The 3D Sketch box (mitcad#1544, mitcad#1550): checked, enabled when
  // unchecking it turns the sketch back into 2D, with why.
  if (m_kind != nullptr && m_kind->isVisible()) {
    // Its box, not the middle of the row: a click beside the label does not
    // toggle a check box, and the row is as wide as the card.
    QStyleOptionButton option;
    option.initFrom(m_kind);
    option.text = m_kind->text();
    const QRect box = m_kind->style()->subElementRect(QStyle::SE_CheckBoxIndicator, &option, m_kind);
    const QPoint center = GlassCard::mapToHost(m_kind, box.isValid() ? box.center() : m_kind->rect().center());
    qDebug().noquote() << QStringLiteral("3D Sketch Palette 3D Sketch at %1,%2 checked %3 enabled %4: %5")
                              .arg(center.x())
                              .arg(center.y())
                              .arg(m_kind->isChecked() ? QStringLiteral("yes") : QStringLiteral("no"))
                              .arg(m_kind->isEnabled() ? QStringLiteral("yes") : QStringLiteral("no"))
                              .arg(m_kind->toolTip());
  }
}

} // namespace mitcad::sketch
