// SPDX-License-Identifier: MIT
#include "render/RenderEnvironmentDialog.hpp"

#include <algorithm>
#include <cmath>
#include <utility>

#include <QCheckBox>
#include <QColorDialog>
#include <QComboBox>
#include <QDoubleSpinBox>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QPixmap>
#include <QPushButton>
#include <QScreen>
#include <QShowEvent>
#include <QTabBar>
#include <QTabWidget>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include <QEvent>
#include <QHideEvent>
#include <QKeyEvent>
#include <QMouseEvent>

#include "browser/DocumentHost.hpp"
#include "framework/Appearances.hpp"
#include "framework/Icons.hpp"
#include "framework/TestSync.hpp"
#include "framework/Theme.hpp"
#include "OcctViewer.hpp"
#include "render/RenderLights.hpp"

namespace mitcad {
namespace {

constexpr double kDegrees = 180.0 / 3.14159265358979323846;

QString number(double value) { return QString::number(value, 'g', 6); }

QIcon colorIcon(const QColor& color) {
  QPixmap pixmap(16, 16);
  pixmap.fill(color.isValid() ? color : QColor(Qt::transparent));
  return QIcon(pixmap);
}

QString sectionOf(const QString& key) { return key.section(QLatin1Char('.'), 0, 0); }
QString fieldOf(const QString& key) { return key.section(QLatin1Char('.'), 1); }

} // namespace

RenderEnvironmentDialog::RenderEnvironmentDialog(DocumentHost& host, OcctViewer& viewer,
                                                 std::function<void(bool)> showRendered, QWidget* parent)
    : QDialog(parent), m_host(host), m_viewer(viewer), m_showRendered(std::move(showRendered)) {
  setWindowTitle(tr("Render Environment"));
  setWindowIcon(themeIcon(QStringLiteral("environment")));
  // The environment's settings on a page, the lights (mitcad#54) on
  // another, so that the dialog stays narrow beside the view.
  auto* outer = new QVBoxLayout(this);
  m_rendered = new QCheckBox(tr("Rendered view"));
  m_rendered->setToolTip(tr("View > Rendered: the view shows the bodies path traced in these settings"));
  outer->addWidget(m_rendered);
  m_pages = new QTabWidget;
  outer->addWidget(m_pages);
  auto* environmentPage = new QWidget;
  auto* layout = new QVBoxLayout(environmentPage);
  m_pages->addTab(environmentPage, tr("Environment"));
  auto* lightsPage = new QWidget;
  auto* lightsLayout = new QVBoxLayout(lightsPage);
  lightsLayout->addWidget(buildLights());
  lightsLayout->addStretch(1);
  m_pages->addTab(lightsPage, tr("Lights"));
  connect(m_pages, &QTabWidget::currentChanged, this, [this](int page) {
    qDebug().noquote() << QStringLiteral("Render settings page %1").arg(page == 1 ? "lights" : "environment");
    // The page's fields where they are now, logged again.
    m_logged.clear();
    TestSync::singleShot(0, this, [this] { logLayout(); });
  });

  connect(m_rendered, &QCheckBox::toggled, this, [this](bool on) {
    if (!m_building && m_showRendered) {
      m_showRendered(on);
    }
  });

  const auto group = [layout](const QString& title) {
    auto* box = new QGroupBox(title);
    auto* form = new QFormLayout(box);
    layout->addWidget(box);
    return form;
  };

  QFormLayout* environment = group(tr("Environment"));
  environment->addRow(tr("Light"), addChoice(QStringLiteral("environment.preset"),
                                             {{QStringLiteral("studio"), tr("Studio")},
                                              {QStringLiteral("studio_white"), tr("White Studio")},
                                              {QStringLiteral("studio_dark"), tr("Dark Studio")},
                                              {QStringLiteral("outdoor"), tr("Outdoor")},
                                              {QStringLiteral("image"), tr("HDR Image")}}));
  auto* imageRow = new QHBoxLayout;
  m_image = new QLineEdit;
  m_image->setPlaceholderText(tr("An .hdr or .exr file"));
  m_image->setToolTip(tr("An equirectangular HDR image all around the design, relative to the design's folder or "
                         "absolute"));
  m_browse = new QPushButton(tr("Browse..."));
  m_browse->setAutoDefault(false);
  imageRow->addWidget(m_image, 1);
  imageRow->addWidget(m_browse);
  environment->addRow(tr("Image"), imageRow);
  connect(m_image, &QLineEdit::editingFinished, this, [this] {
    const QString path = m_image->text().trimmed();
    if (!m_building && path != value(QStringLiteral("environment.image")).toString()) {
      change(QStringLiteral("environment.image"), path.isEmpty() ? QJsonValue() : QJsonValue(path));
    }
  });
  connect(m_browse, &QPushButton::clicked, this, &RenderEnvironmentDialog::browseImage);
  environment->addRow(tr("Strength"), addNumber(QStringLiteral("environment.strength"), 0.0, 100.0, 0.1, 2,
                                                QString(), 1.0, tr("A factor on all of the environment's light")));
  environment->addRow(tr("Rotation"),
                      addNumber(QStringLiteral("environment.rotation"), -720.0, 720.0, 15.0, 1,
                                QStringLiteral(" °"), kDegrees,
                                tr("Turns the studio's lights or the image about the Z axis")));
  environment->addRow(tr("Sun elevation"),
                      addNumber(QStringLiteral("environment.sun_elevation"), 0.0, 90.0, 5.0, 1,
                                QStringLiteral(" °"), kDegrees, tr("Outdoor: the sun's height above the horizon")));
  environment->addRow(tr("Sun direction"),
                      addNumber(QStringLiteral("environment.sun_azimuth"), -720.0, 720.0, 15.0, 1,
                                QStringLiteral(" °"), kDegrees,
                                tr("Outdoor: where the sun is, counter-clockwise from the X axis seen from above")));

  QFormLayout* background = group(tr("Background"));
  background->addRow(tr("Show"), addChoice(QStringLiteral("background.mode"),
                                           {{QStringLiteral("view"), tr("View Background")},
                                            {QStringLiteral("color"), tr("Colour")},
                                            {QStringLiteral("environment"), tr("Environment")}}));
  auto* colorRow = new QHBoxLayout;
  m_colorButton = new QToolButton;
  m_colorButton->setToolTip(tr("Choose a colour"));
  m_color = new QLineEdit;
  m_color->setToolTip(tr("sRGB as #rrggbb"));
  m_color->setMaximumWidth(110);
  colorRow->addWidget(m_colorButton);
  colorRow->addWidget(m_color);
  colorRow->addStretch(1);
  background->addRow(tr("Colour"), colorRow);
  connect(m_colorButton, &QToolButton::clicked, this, &RenderEnvironmentDialog::pickColor);
  connect(m_color, &QLineEdit::editingFinished, this, [this] {
    if (m_building) {
      return;
    }
    const QColor color(m_color->text().trimmed());
    if (!color.isValid()) {
      m_message->setText(tr("'%1' is no colour: write it as #rrggbb.").arg(m_color->text()));
      refresh();
      return;
    }
    if (color.name() != colorOf(value(QStringLiteral("background.color"))).name()) {
      change(QStringLiteral("background.color"), colorJson(color));
    }
  });

  QFormLayout* ground = group(tr("Ground"));
  ground->addRow(addCheck(QStringLiteral("ground.shadows"), tr("Shadows on the ground"),
                          tr("A ground under the bodies shows their shadows; without it there is no ground")));
  ground->addRow(addCheck(QStringLiteral("ground.reflections"), tr("Reflections on the ground"),
                          tr("The ground is polished and shows the bodies' reflections too")));
  m_lowest = new QCheckBox(tr("Under the lowest body"));
  m_lowest->setToolTip(tr("The ground follows the bodies; otherwise it stays at the height below"));
  ground->addRow(m_lowest);
  connect(m_lowest, &QCheckBox::toggled, this, [this](bool lowest) {
    if (m_building) {
      return;
    }
    QDoubleSpinBox* height = nullptr;
    for (const Number& n : std::as_const(m_numbers)) {
      if (n.key == QLatin1String("ground.height")) {
        height = n.box;
      }
    }
    change(QStringLiteral("ground.height"), lowest || height == nullptr ? QJsonValue() : QJsonValue(height->value()));
  });
  ground->addRow(tr("Height"), addNumber(QStringLiteral("ground.height"), -1e6, 1e6, 1.0, 3, QStringLiteral(" mm"),
                                         1.0, tr("Where the ground is on the Z axis")));

  QFormLayout* film = group(tr("Film"));
  film->addRow(tr("Exposure"), addNumber(QStringLiteral("film.exposure"), -10.0, 10.0, 0.25, 2, QStringLiteral(" EV"),
                                         1.0, tr("Each step of 1 doubles the light")));
  film->addRow(tr("View transform"), addChoice(QStringLiteral("film.view_transform"),
                                               {{QStringLiteral("standard"), tr("Standard")},
                                                {QStringLiteral("filmic"), tr("Filmic")},
                                                {QStringLiteral("neutral"), tr("Neutral")}}));

  m_message = new QLabel;
  m_message->setWordWrap(true);
  setErrorStyleSheet(m_message);
  outer->addWidget(m_message);

  auto* buttons = new QHBoxLayout;
  m_reset = new QPushButton(tr("Reset"));
  m_reset->setToolTip(tr("The default settings: the studio over the view's background, shadows on the ground"));
  m_close = new QPushButton(tr("Close"));
  for (QPushButton* button : {m_reset, m_close}) {
    button->setAutoDefault(false);
  }
  buttons->addWidget(m_reset);
  buttons->addStretch(1);
  buttons->addWidget(m_close);
  outer->addLayout(buttons);
  connect(m_reset, &QPushButton::clicked, this, &RenderEnvironmentDialog::reset);
  connect(m_close, &QPushButton::clicked, this, &QDialog::accept);
  refresh();
}

QDoubleSpinBox* RenderEnvironmentDialog::addNumber(const QString& key, double low, double high, double step,
                                                   int decimals, const QString& suffix, double factor,
                                                   const QString& tip) {
  auto* box = new QDoubleSpinBox;
  box->setRange(low, high);
  box->setSingleStep(step);
  box->setDecimals(decimals);
  box->setSuffix(suffix);
  // A typed value counts on Enter or when the field is left, not per key.
  box->setKeyboardTracking(false);
  box->setToolTip(tip);
  m_numbers.append({key, box, factor});
  connect(box, &QDoubleSpinBox::valueChanged, this, [this, key, factor](double shown) {
    if (!m_building) {
      change(key, shown / factor);
    }
  });
  return box;
}

QComboBox* RenderEnvironmentDialog::addChoice(const QString& key, const QVector<std::pair<QString, QString>>& items) {
  auto* box = new QComboBox;
  for (const auto& [id, text] : items) {
    box->addItem(text, id);
  }
  m_choices.append({key, box});
  connect(box, &QComboBox::currentIndexChanged, this, [this, key, box](int index) {
    if (!m_building && index >= 0) {
      change(key, box->itemData(index).toString());
    }
  });
  return box;
}

QCheckBox* RenderEnvironmentDialog::addCheck(const QString& key, const QString& text, const QString& tip) {
  auto* box = new QCheckBox(text);
  box->setToolTip(tip);
  m_checks.append({key, box});
  connect(box, &QCheckBox::toggled, this, [this, key](bool on) {
    if (!m_building) {
      change(key, on);
    }
  });
  return box;
}

QJsonValue RenderEnvironmentDialog::value(const QString& key) const {
  return m_settings.value(sectionOf(key)).toObject().value(fieldOf(key));
}

void RenderEnvironmentDialog::refresh() {
  try {
    m_settings = m_host.model().queryObject({{QStringLiteral("query"), QStringLiteral("render_settings")}});
  } catch (const std::exception& e) {
    m_message->setText(QString::fromUtf8(e.what()));
    return;
  }
  m_building = true;
  for (const Number& n : std::as_const(m_numbers)) {
    const QJsonValue v = value(n.key);
    if (v.isDouble()) {
      n.box->setValue(v.toDouble() * n.factor);
    }
  }
  for (const Choice& c : std::as_const(m_choices)) {
    c.box->setCurrentIndex(c.box->findData(value(c.key).toString()));
  }
  for (const auto& [key, box] : std::as_const(m_checks)) {
    box->setChecked(value(key).toBool());
  }
  m_lowest->setChecked(!value(QStringLiteral("ground.height")).isDouble());
  m_image->setText(value(QStringLiteral("environment.image")).toString());
  const QColor color = colorOf(value(QStringLiteral("background.color")));
  m_colorButton->setIcon(colorIcon(color));
  m_color->setText(color.name());
  m_building = false;
  refreshLights();
  updateEnabled();
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void RenderEnvironmentDialog::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  // The lights' glyphs over the view while the dialog is open.
  if (!m_overlay) {
    m_overlay = new render::RenderLightsOverlay(m_viewer);
  }
  m_overlay->setLights(m_settings.value(QStringLiteral("lights")).toArray(), m_lightId);
  m_overlay->show();
  m_overlay->raise();
  if (m_placed || parentWidget() == nullptr) {
    return;
  }
  m_placed = true;
  // Beside the main window, where it leaves the view free to watch the
  // render, as far as the screen allows.
  QWidget* window = parentWidget()->window();
  const QScreen* screen = window->screen();
  if (screen == nullptr) {
    return;
  }
  const QRect available = screen->availableGeometry();
  const QRect main = window->frameGeometry();
  const QSize size = frameGeometry().size();
  const int x = std::max(available.left(), std::min(main.right() + 8, available.right() + 1 - size.width()));
  const int y = std::clamp(main.top(), available.top(), std::max(available.top(), available.bottom() + 1 - size.height()));
  move(x, y);
}

void RenderEnvironmentDialog::setRenderedShown(bool shown) {
  m_building = true;
  m_rendered->setChecked(shown);
  m_building = false;
}

void RenderEnvironmentDialog::updateEnabled() {
  const QString preset = value(QStringLiteral("environment.preset")).toString();
  const bool image = preset == QLatin1String("image");
  const bool outdoor = preset == QLatin1String("outdoor");
  const bool shadows = value(QStringLiteral("ground.shadows")).toBool();
  m_image->setEnabled(image);
  m_browse->setEnabled(image);
  const bool color = value(QStringLiteral("background.mode")).toString() == QLatin1String("color");
  m_colorButton->setEnabled(color);
  m_color->setEnabled(color);
  m_lowest->setEnabled(shadows);
  for (const Number& n : std::as_const(m_numbers)) {
    if (n.key == QLatin1String("environment.rotation")) {
      n.box->setEnabled(!outdoor);
    } else if (n.key.startsWith(QLatin1String("environment.sun_"))) {
      n.box->setEnabled(outdoor);
    } else if (n.key == QLatin1String("ground.height")) {
      n.box->setEnabled(shadows && !m_lowest->isChecked());
    }
  }
  for (const auto& [key, box] : std::as_const(m_checks)) {
    if (key == QLatin1String("ground.reflections")) {
      box->setEnabled(shadows);
    }
  }
}

void RenderEnvironmentDialog::change(const QString& key, const QJsonValue& value) {
  // Not in the middle of a command or a sketch, whose undo steps the
  // change would come between.
  if (!m_host.canChangeModel()) {
    m_message->setText(tr("Finish the command or the sketch first."));
    qDebug().noquote() << "Render settings: finish the command or the sketch first";
    refresh();
    return;
  }
  const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_render_settings")},
                            {sectionOf(key), QJsonObject{{fieldOf(key), value}}}};
  const QString text = value.isArray()    ? colorOf(value).name()
                       : value.isDouble() ? number(value.toDouble())
                       : value.isBool()   ? (value.toBool() ? QStringLiteral("true") : QStringLiteral("false"))
                       : value.isNull()   ? QStringLiteral("null")
                                          : value.toString();
  if (m_host.runModelCommand(command)) {
    m_message->clear();
    qDebug().noquote() << QStringLiteral("Render setting %1 = %2").arg(key, text);
  } else {
    m_message->setText(m_host.lastError());
    qDebug().noquote() << QStringLiteral("Render setting %1 = %2 refused: %3").arg(key, text, m_host.lastError());
  }
  refresh();
}

void RenderEnvironmentDialog::pickColor() {
  const QColor color =
      QColorDialog::getColor(colorOf(value(QStringLiteral("background.color"))), this, tr("Background Colour"));
  if (color.isValid()) {
    change(QStringLiteral("background.color"), colorJson(color));
  }
}

void RenderEnvironmentDialog::browseImage() {
  const QString current = m_image->text();
  const QString path = QFileDialog::getOpenFileName(this, tr("Environment Image"),
                                                    current.isEmpty() ? QString() : QFileInfo(current).absolutePath(),
                                                    tr("HDR images (*.hdr *.exr);;All files (*)"));
  if (!path.isEmpty()) {
    change(QStringLiteral("environment.image"), path);
  }
}

void RenderEnvironmentDialog::reset() {
  if (!m_host.canChangeModel()) {
    m_message->setText(tr("Finish the command or the sketch first."));
    return;
  }
  if (m_host.runModelCommand({{QStringLiteral("cmd"), QStringLiteral("reset_render_settings")}})) {
    m_message->clear();
    qDebug().noquote() << "Render settings reset";
  } else {
    m_message->setText(m_host.lastError());
  }
  refresh();
}

void RenderEnvironmentDialog::logLayout() {
  if (!isVisible() || parentWidget() == nullptr) {
    return;
  }
  // In the main window's coordinates, as the other logged places.
  const QPoint origin = parentWidget()->window()->mapToGlobal(QPoint(0, 0));
  const auto at = [&origin](QWidget* widget) {
    // A check box takes clicks on its box and text only: its box, there.
    const QPoint inside = qobject_cast<QCheckBox*>(widget) != nullptr
                              ? QPoint(widget->rect().left() + 8, widget->rect().center().y())
                              : widget->rect().center();
    const QPoint global = widget->mapToGlobal(inside) - origin;
    return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
  };
  const auto tabAt = [this, &origin](int index) {
    QTabBar* bar = m_pages->tabBar();
    const QPoint global = bar->mapToGlobal(bar->tabRect(index).center()) - origin;
    return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
  };
  QStringList lines;
  for (const Number& n : std::as_const(m_numbers)) {
    lines << QStringLiteral("Render settings field %1 at %2").arg(n.key, at(n.box));
  }
  for (const Choice& c : std::as_const(m_choices)) {
    lines << QStringLiteral("Render settings field %1 at %2").arg(c.key, at(c.box));
  }
  for (const auto& [key, box] : std::as_const(m_checks)) {
    lines << QStringLiteral("Render settings field %1 at %2").arg(key, at(box));
  }
  const auto lightField = [&lines, &at](const QString& name, QWidget* widget) {
    lines << QStringLiteral("Render light field %1 at %2").arg(name, at(widget));
  };
  lightField(QStringLiteral("list"), m_lightList);
  lightField(QStringLiteral("new"), m_newLightKind);
  lightField(QStringLiteral("add"), m_addLight);
  lightField(QStringLiteral("delete"), m_deleteLight);
  lightField(QStringLiteral("name"), m_lightName);
  lightField(QStringLiteral("type"), m_lightType);
  lightField(QStringLiteral("enabled"), m_lightOn);
  lightField(QStringLiteral("camera"), m_lightCamera);
  for (int i = 0; i < 3; ++i) {
    lightField(QStringLiteral("position.%1").arg(i), m_lightPosition[static_cast<std::size_t>(i)]);
    lightField(QStringLiteral("direction.%1").arg(i), m_lightDirection[static_cast<std::size_t>(i)]);
  }
  lightField(QStringLiteral("color"), m_lightColor);
  lightField(QStringLiteral("power"), m_lightPower);
  lightField(QStringLiteral("size"), m_lightSize);
  lightField(QStringLiteral("size_y"), m_lightSizeY);
  lightField(QStringLiteral("shape"), m_lightShape);
  lightField(QStringLiteral("spot_angle"), m_spotAngle);
  lightField(QStringLiteral("spot_blend"), m_spotBlend);
  lightField(QStringLiteral("angle"), m_sunAngle);
  lightField(QStringLiteral("distance"), m_aimDistance);
  lightField(QStringLiteral("aim"), m_aim);
  lightField(QStringLiteral("from_camera"), m_fromCamera);
  lines << QStringLiteral("Render settings field environment.image at %1").arg(at(m_image))
        << QStringLiteral("Render settings field background.color at %1").arg(at(m_color))
        << QStringLiteral("Render settings field ground.lowest at %1").arg(at(m_lowest))
        << QStringLiteral("Render settings rendered at %1").arg(at(m_rendered))
        << QStringLiteral("Render settings page environment at %1").arg(tabAt(0))
        << QStringLiteral("Render settings page lights at %1").arg(tabAt(1))
        << QStringLiteral("Render settings reset at %1").arg(at(m_reset))
        << QStringLiteral("Render settings close at %1").arg(at(m_close));
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_logged) {
    m_logged = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

// ---------------------------------------------------------------------------
// Lights (mitcad#54)

namespace {

QDoubleSpinBox* lightNumber(double low, double high, double step, int decimals, const QString& suffix,
                            const QString& tip) {
  auto* box = new QDoubleSpinBox;
  box->setRange(low, high);
  box->setSingleStep(step);
  box->setDecimals(decimals);
  box->setSuffix(suffix);
  box->setKeyboardTracking(false);
  box->setToolTip(tip);
  return box;
}

QString lightTypeName(const QString& type) {
  if (type == QLatin1String("spot")) {
    return RenderEnvironmentDialog::tr("Spot");
  }
  if (type == QLatin1String("area")) {
    return RenderEnvironmentDialog::tr("Area");
  }
  if (type == QLatin1String("sun")) {
    return RenderEnvironmentDialog::tr("Sun");
  }
  return RenderEnvironmentDialog::tr("Point");
}

} // namespace

QWidget* RenderEnvironmentDialog::buildLights() {
  auto* box = new QWidget;
  auto* column = new QVBoxLayout(box);
  column->setContentsMargins(0, 0, 0, 0);
  auto* listRow = new QHBoxLayout;
  m_lightList = new QComboBox;
  m_lightList->setToolTip(tr("The light whose settings are below"));
  m_lightList->setMinimumWidth(150);
  listRow->addWidget(m_lightList, 1);
  m_deleteLight = new QPushButton(tr("Delete"));
  m_deleteLight->setToolTip(tr("Deletes the chosen light"));
  listRow->addWidget(m_deleteLight);
  column->addLayout(listRow);
  auto* addRow = new QHBoxLayout;
  m_newLightKind = new QComboBox;
  for (const QString& type : {QStringLiteral("point"), QStringLiteral("spot"), QStringLiteral("area"),
                              QStringLiteral("sun")}) {
    m_newLightKind->addItem(lightTypeName(type), type);
  }
  m_newLightKind->setToolTip(tr("The kind of light Add adds"));
  m_addLight = new QPushButton(tr("Add"));
  m_addLight->setToolTip(tr("Adds a light of this kind, placed at the camera and shining where it looks"));
  addRow->addWidget(m_newLightKind, 1);
  addRow->addWidget(m_addLight);
  column->addLayout(addRow);
  for (QPushButton* button : {m_addLight, m_deleteLight}) {
    button->setAutoDefault(false);
  }
  connect(m_addLight, &QPushButton::clicked, this, &RenderEnvironmentDialog::addLight);
  connect(m_deleteLight, &QPushButton::clicked, this, &RenderEnvironmentDialog::deleteLight);
  connect(m_lightList, &QComboBox::currentIndexChanged, this, [this](int index) {
    if (!m_building && index >= 0) {
      m_lightId = m_lightList->itemData(index).toString();
      stopAiming();
      refreshLights();
      qDebug().noquote() << QStringLiteral("Render light %1 chosen").arg(m_lightId);
    }
  });

  m_lightEditor = new QWidget;
  auto* form = new QFormLayout(m_lightEditor);
  form->setContentsMargins(0, 0, 0, 0);
  column->addWidget(m_lightEditor);
  m_lightName = new QLineEdit;
  form->addRow(tr("Name"), m_lightName);
  connect(m_lightName, &QLineEdit::editingFinished, this, [this] {
    const QString name = m_lightName->text().trimmed();
    if (!m_building && name != chosenLight().value(QStringLiteral("name")).toString()) {
      changeLight({{QStringLiteral("name"), name}}, QStringLiteral("name = %1").arg(name));
    }
  });
  m_lightType = new QComboBox;
  for (int i = 0; i < m_newLightKind->count(); ++i) {
    m_lightType->addItem(m_newLightKind->itemText(i), m_newLightKind->itemData(i));
  }
  form->addRow(tr("Kind"), m_lightType);
  connect(m_lightType, &QComboBox::currentIndexChanged, this, [this](int index) {
    if (!m_building && index >= 0) {
      const QString type = m_lightType->itemData(index).toString();
      changeLight({{QStringLiteral("type"), type}}, QStringLiteral("type = %1").arg(type));
    }
  });
  m_lightOn = new QCheckBox(tr("On"));
  m_lightOn->setToolTip(tr("An off light is kept but lights nothing"));
  m_lightCamera = new QCheckBox(tr("Follows the camera"));
  m_lightCamera->setToolTip(tr("The light's place and direction are relative to the camera (x to the right, y up, "
                               "z toward you): it moves with the view"));
  auto* checks = new QHBoxLayout;
  checks->addWidget(m_lightOn);
  checks->addWidget(m_lightCamera);
  checks->addStretch(1);
  form->addRow(checks);
  connect(m_lightOn, &QCheckBox::toggled, this, [this](bool on) {
    if (!m_building) {
      changeLight({{QStringLiteral("enabled"), on}}, QStringLiteral("enabled = %1").arg(on ? "true" : "false"));
    }
  });
  connect(m_lightCamera, &QCheckBox::toggled, this, [this](bool on) {
    if (!m_building) {
      setLightRelativeToCamera(on);
    }
  });
  // A vector field of three numbers; a change sends the whole vector.
  const auto vectorRow = [this, form](const QString& label, const QString& field, std::array<QDoubleSpinBox*, 3>& boxes,
                                      double range, int decimals, const QString& suffix, const QString& tip) {
    auto* row = new QHBoxLayout;
    for (std::size_t i = 0; i < 3; ++i) {
      boxes[i] = lightNumber(-range, range, decimals > 2 ? 0.1 : 10.0, decimals, suffix, tip);
      row->addWidget(boxes[i]);
      connect(boxes[i], &QDoubleSpinBox::valueChanged, this, [this, field, &boxes] {
        if (m_building) {
          return;
        }
        const QJsonArray vector{boxes[0]->value(), boxes[1]->value(), boxes[2]->value()};
        changeLight({{field, vector}}, QStringLiteral("%1 = %2, %3, %4")
                                           .arg(field, number(boxes[0]->value()), number(boxes[1]->value()),
                                                number(boxes[2]->value())));
      });
    }
    form->addRow(label, row);
  };
  vectorRow(tr("Position"), QStringLiteral("position"), m_lightPosition, 1e7, 1, QStringLiteral(" mm"),
            tr("Where the light is (x, y, z)"));
  vectorRow(tr("Direction"), QStringLiteral("direction"), m_lightDirection, 1e3, 3, QString(),
            tr("Where the light shines (x, y, z; its length does not matter)"));
  auto* colorRow = new QHBoxLayout;
  m_lightColorButton = new QToolButton;
  m_lightColorButton->setToolTip(tr("Choose the light's colour"));
  m_lightColor = new QLineEdit;
  m_lightColor->setToolTip(tr("sRGB as #rrggbb"));
  m_lightColor->setMaximumWidth(110);
  colorRow->addWidget(m_lightColorButton);
  colorRow->addWidget(m_lightColor);
  colorRow->addStretch(1);
  form->addRow(tr("Colour"), colorRow);
  connect(m_lightColorButton, &QToolButton::clicked, this, &RenderEnvironmentDialog::pickLightColor);
  connect(m_lightColor, &QLineEdit::editingFinished, this, [this] {
    if (m_building) {
      return;
    }
    const QColor color(m_lightColor->text().trimmed());
    if (!color.isValid()) {
      m_message->setText(tr("'%1' is no colour: write it as #rrggbb.").arg(m_lightColor->text()));
      refreshLights();
      return;
    }
    if (color.name() != colorOf(chosenLight().value(QStringLiteral("color"))).name()) {
      changeLight({{QStringLiteral("color"), colorJson(color)}}, QStringLiteral("color = %1").arg(color.name()));
    }
  });
  // A number field of the light.
  const auto numberRow = [this, form](const QString& label, const QString& field, QDoubleSpinBox* box,
                                      double factor) {
    form->addRow(label, box);
    connect(box, &QDoubleSpinBox::valueChanged, this, [this, field, factor](double shown) {
      if (!m_building) {
        changeLight({{field, shown / factor}}, QStringLiteral("%1 = %2").arg(field, number(shown / factor)));
      }
    });
  };
  m_lightPower = lightNumber(0.0, 1e6, 1.0, 2, QStringLiteral(" W"),
                             tr("Point, spot and area lights: watts (5 W light 300 mm away about as much as the "
                                "studio's key light); a sun: W/m²"));
  numberRow(tr("Power"), QStringLiteral("power"), m_lightPower, 1.0);
  m_lightSize = lightNumber(0.0, 1e6, 5.0, 1, QStringLiteral(" mm"),
                            tr("The light's size: a larger one gives softer shadows (point and spot: the ball's "
                               "diameter; area: the width, or the disc's diameter)"));
  numberRow(tr("Size"), QStringLiteral("size"), m_lightSize, 1.0);
  m_lightSizeY = lightNumber(0.0, 1e6, 5.0, 1, QStringLiteral(" mm"), tr("An area light's rectangle: its height"));
  numberRow(tr("Height"), QStringLiteral("size_y"), m_lightSizeY, 1.0);
  m_lightShape = new QComboBox;
  m_lightShape->addItem(tr("Rectangle"), QStringLiteral("rectangle"));
  m_lightShape->addItem(tr("Disc"), QStringLiteral("disc"));
  form->addRow(tr("Shape"), m_lightShape);
  connect(m_lightShape, &QComboBox::currentIndexChanged, this, [this](int index) {
    if (!m_building && index >= 0) {
      const QString shape = m_lightShape->itemData(index).toString();
      changeLight({{QStringLiteral("shape"), shape}}, QStringLiteral("shape = %1").arg(shape));
    }
  });
  m_spotAngle = lightNumber(0.1, 180.0, 5.0, 1, QStringLiteral(" °"), tr("A spot's cone: its full angle"));
  numberRow(tr("Cone"), QStringLiteral("spot_angle"), m_spotAngle, kDegrees);
  m_spotBlend = lightNumber(0.0, 1.0, 0.05, 2, QString(), tr("How softly the cone's edge fades: 0 sharp, 1 soft"));
  numberRow(tr("Blend"), QStringLiteral("spot_blend"), m_spotBlend, 1.0);
  m_sunAngle = lightNumber(0.0, 90.0, 0.5, 2, QStringLiteral(" °"),
                           tr("A sun's angular diameter: larger gives softer shadows"));
  numberRow(tr("Sun size"), QStringLiteral("angle"), m_sunAngle, kDegrees);

  auto* place = new QHBoxLayout;
  m_aimDistance = lightNumber(0.1, 1e6, 10.0, 1, QStringLiteral(" mm"),
                              tr("How far from the face Aim at Face puts the light"));
  m_aimDistance->setValue(200.0);
  m_aim = new QPushButton(tr("Aim at Face"));
  m_aim->setCheckable(true);
  m_aim->setToolTip(tr("Click a face in the view: the light goes this far out along the face's normal and "
                       "shines at the point (Esc cancels)"));
  m_fromCamera = new QPushButton(tr("At Camera"));
  m_fromCamera->setToolTip(tr("The light goes to the camera and shines where it looks"));
  for (QPushButton* button : {m_aim, m_fromCamera}) {
    button->setAutoDefault(false);
  }
  place->addWidget(m_aimDistance);
  place->addWidget(m_aim);
  place->addWidget(m_fromCamera);
  form->addRow(tr("Place"), place);
  connect(m_aim, &QPushButton::clicked, this, [this](bool on) {
    if (on) {
      aimAtFace();
    } else {
      stopAiming();
    }
  });
  connect(m_fromCamera, &QPushButton::clicked, this, &RenderEnvironmentDialog::placeAtCamera);
  return box;
}

QJsonObject RenderEnvironmentDialog::chosenLight() const {
  for (const QJsonValue& light : m_settings.value(QStringLiteral("lights")).toArray()) {
    if (light.toObject().value(QStringLiteral("id")).toString() == m_lightId) {
      return light.toObject();
    }
  }
  return {};
}

void RenderEnvironmentDialog::refreshLights() {
  const QJsonArray lights = m_settings.value(QStringLiteral("lights")).toArray();
  const bool building = m_building;
  m_building = true;
  m_lightList->clear();
  bool found = false;
  for (const QJsonValue& value : lights) {
    const QJsonObject light = value.toObject();
    const QString id = light.value(QStringLiteral("id")).toString();
    m_lightList->addItem(QStringLiteral("%1 (%2)").arg(light.value(QStringLiteral("name")).toString(),
                                                       lightTypeName(light.value(QStringLiteral("type")).toString())),
                         id);
    found = found || id == m_lightId;
  }
  if (!found) {
    m_lightId = lights.isEmpty() ? QString() : lights.first().toObject().value(QStringLiteral("id")).toString();
  }
  m_lightList->setCurrentIndex(m_lightList->findData(m_lightId));
  const QJsonObject light = chosenLight();
  const bool has = !light.isEmpty();
  m_lightEditor->setEnabled(has);
  m_deleteLight->setEnabled(has);
  if (has) {
    const QString type = light.value(QStringLiteral("type")).toString();
    m_lightName->setText(light.value(QStringLiteral("name")).toString());
    m_lightType->setCurrentIndex(m_lightType->findData(type));
    m_lightOn->setChecked(light.value(QStringLiteral("enabled")).toBool(true));
    m_lightCamera->setChecked(light.value(QStringLiteral("space")).toString() == QLatin1String("camera"));
    const QJsonArray position = light.value(QStringLiteral("position")).toArray();
    const QJsonArray direction = light.value(QStringLiteral("direction")).toArray();
    for (std::size_t i = 0; i < 3; ++i) {
      m_lightPosition[i]->setValue(position.at(static_cast<qsizetype>(i)).toDouble());
      m_lightDirection[i]->setValue(direction.at(static_cast<qsizetype>(i)).toDouble());
    }
    const QColor color = colorOf(light.value(QStringLiteral("color")));
    m_lightColorButton->setIcon(colorIcon(color));
    m_lightColor->setText(color.name());
    const bool sun = type == QLatin1String("sun");
    m_lightPower->setSuffix(sun ? QStringLiteral(" W/m²") : QStringLiteral(" W"));
    m_lightPower->setValue(light.value(QStringLiteral("power")).toDouble());
    m_lightSize->setValue(light.value(QStringLiteral("size")).toDouble());
    m_lightSizeY->setValue(light.value(QStringLiteral("size_y")).toDouble());
    m_lightShape->setCurrentIndex(m_lightShape->findData(light.value(QStringLiteral("shape")).toString()));
    m_spotAngle->setValue(light.value(QStringLiteral("spot_angle")).toDouble() * kDegrees);
    m_spotBlend->setValue(light.value(QStringLiteral("spot_blend")).toDouble());
    m_sunAngle->setValue(light.value(QStringLiteral("angle")).toDouble() * kDegrees);
    const bool area = type == QLatin1String("area");
    for (QDoubleSpinBox* box : m_lightPosition) {
      box->setEnabled(!sun);
    }
    m_lightSize->setEnabled(!sun);
    m_lightSizeY->setEnabled(area && light.value(QStringLiteral("shape")).toString() != QLatin1String("disc"));
    m_lightShape->setEnabled(area);
    m_spotAngle->setEnabled(type == QLatin1String("spot"));
    m_spotBlend->setEnabled(type == QLatin1String("spot"));
    m_sunAngle->setEnabled(sun);
    m_aimDistance->setEnabled(!sun);
  }
  m_building = building;
  if (m_overlay) {
    m_overlay->setLights(lights, m_lightId);
  }
}

void RenderEnvironmentDialog::addLight() {
  if (!m_host.canChangeModel()) {
    m_message->setText(tr("Finish the command or the sketch first."));
    return;
  }
  // At the camera, shining where it looks (a sun: from behind the camera).
  const QString type = m_newLightKind->currentData().toString();
  const CameraState camera = m_viewer.camera();
  render::LightPose pose;
  pose.position = camera.eye;
  const gp_XYZ forward = camera.target.XYZ() - camera.eye.XYZ();
  if (forward.Modulus() > 1e-9) {
    pose.direction = gp_Dir(forward);
  }
  QJsonObject command = render::poseFields(pose, QStringLiteral("world"), camera);
  command.insert(QStringLiteral("cmd"), QStringLiteral("add_render_light"));
  command.insert(QStringLiteral("type"), type);
  QJsonObject result;
  if (m_host.runModelCommand(command, &result)) {
    m_message->clear();
    m_lightId = result.value(QStringLiteral("id")).toString();
    qDebug().noquote() << QStringLiteral("Render light %1 added (%2)").arg(m_lightId, type);
  } else {
    m_message->setText(m_host.lastError());
  }
  refresh();
}

void RenderEnvironmentDialog::deleteLight() {
  if (m_lightId.isEmpty()) {
    return;
  }
  if (!m_host.canChangeModel()) {
    m_message->setText(tr("Finish the command or the sketch first."));
    return;
  }
  stopAiming();
  const QString id = m_lightId;
  if (m_host.runModelCommand({{QStringLiteral("cmd"), QStringLiteral("delete_render_light")},
                              {QStringLiteral("id"), id}})) {
    m_message->clear();
    qDebug().noquote() << QStringLiteral("Render light %1 deleted").arg(id);
  } else {
    m_message->setText(m_host.lastError());
  }
  refresh();
}

void RenderEnvironmentDialog::changeLight(const QJsonObject& fields, const QString& what) {
  if (m_lightId.isEmpty()) {
    return;
  }
  if (!m_host.canChangeModel()) {
    m_message->setText(tr("Finish the command or the sketch first."));
    refresh();
    return;
  }
  QJsonObject command = fields;
  command.insert(QStringLiteral("cmd"), QStringLiteral("edit_render_light"));
  command.insert(QStringLiteral("id"), m_lightId);
  if (m_host.runModelCommand(command)) {
    m_message->clear();
    qDebug().noquote() << QStringLiteral("Render light %1 %2").arg(m_lightId, what);
  } else {
    m_message->setText(m_host.lastError());
    qDebug().noquote() << QStringLiteral("Render light %1 %2 refused: %3").arg(m_lightId, what, m_host.lastError());
  }
  refresh();
}

void RenderEnvironmentDialog::pickLightColor() {
  const QColor color =
      QColorDialog::getColor(colorOf(chosenLight().value(QStringLiteral("color"))), this, tr("Light Colour"));
  if (color.isValid()) {
    changeLight({{QStringLiteral("color"), colorJson(color)}}, QStringLiteral("color = %1").arg(color.name()));
  }
}

void RenderEnvironmentDialog::setLightRelativeToCamera(bool camera) {
  // The light stays where it is now; its numbers change.
  const QJsonObject light = chosenLight();
  const CameraState view = m_viewer.camera();
  const QString space = camera ? QStringLiteral("camera") : QStringLiteral("world");
  QJsonObject fields = render::poseFields(render::worldPose(light, view), space, view);
  fields.insert(QStringLiteral("space"), space);
  changeLight(fields, QStringLiteral("space = %1").arg(space));
}

void RenderEnvironmentDialog::placeAtCamera() {
  const QJsonObject light = chosenLight();
  if (light.isEmpty()) {
    return;
  }
  const CameraState camera = m_viewer.camera();
  render::LightPose pose;
  pose.position = camera.eye;
  const gp_XYZ forward = camera.target.XYZ() - camera.eye.XYZ();
  if (forward.Modulus() > 1e-9) {
    pose.direction = gp_Dir(forward);
  }
  changeLight(render::poseFields(pose, light.value(QStringLiteral("space")).toString(), camera),
              QStringLiteral("placed at the camera"));
}

void RenderEnvironmentDialog::aimAtFace() {
  if (chosenLight().isEmpty()) {
    m_aim->setChecked(false);
    return;
  }
  m_aiming = true;
  m_aim->setChecked(true);
  m_viewer.installEventFilter(this);
  m_viewer.setCursor(Qt::CrossCursor);
  m_message->setText(tr("Click a face in the view to aim the light at (Esc cancels)."));
  qDebug().noquote() << QStringLiteral("Render light %1: click a face to aim at").arg(m_lightId);
}

void RenderEnvironmentDialog::stopAiming() {
  if (!m_aiming) {
    if (m_aim != nullptr) {
      m_aim->setChecked(false);
    }
    return;
  }
  m_aiming = false;
  m_aim->setChecked(false);
  m_viewer.removeEventFilter(this);
  m_viewer.unsetCursor();
  m_message->clear();
}

bool RenderEnvironmentDialog::eventFilter(QObject* watched, QEvent* event) {
  if (!m_aiming || watched != &m_viewer) {
    return QDialog::eventFilter(watched, event);
  }
  if (event->type() == QEvent::KeyPress && static_cast<QKeyEvent*>(event)->key() == Qt::Key_Escape) {
    stopAiming();
    qDebug().noquote() << "Render light aim cancelled";
    return true;
  }
  if (event->type() == QEvent::MouseButtonRelease || event->type() == QEvent::MouseButtonDblClick) {
    auto* mouse = static_cast<QMouseEvent*>(event);
    return mouse->button() == Qt::LeftButton;
  }
  if (event->type() != QEvent::MouseButtonPress || static_cast<QMouseEvent*>(event)->button() != Qt::LeftButton) {
    return QDialog::eventFilter(watched, event);
  }
  const QPointF at = static_cast<QMouseEvent*>(event)->position();
  const auto hit = render::surfaceAt(m_viewer, at);
  if (!hit) {
    m_message->setText(tr("No face there: click a body's face (Esc cancels)."));
    qDebug().noquote() << "Render light aim: no face there";
    return true;
  }
  const QJsonObject light = chosenLight();
  const QString id = m_lightId;
  stopAiming();
  const auto& [point, normal] = *hit;
  render::LightPose pose;
  pose.position = gp_Pnt(point.XYZ() + normal.XYZ() * m_aimDistance->value());
  pose.direction = normal.Reversed();
  qDebug().noquote() << QStringLiteral("Render light %1 aimed at %2, %3, %4 (normal %5, %6, %7)")
                            .arg(id, number(point.X()), number(point.Y()), number(point.Z()),
                                 number(normal.X()), number(normal.Y()), number(normal.Z()));
  // A sun only turns.
  QJsonObject fields = render::poseFields(pose, light.value(QStringLiteral("space")).toString(), m_viewer.camera());
  if (light.value(QStringLiteral("type")).toString() == QLatin1String("sun")) {
    fields.remove(QStringLiteral("position"));
  }
  changeLight(fields, QStringLiteral("aimed at a face"));
  return true;
}

void RenderEnvironmentDialog::hideEvent(QHideEvent* event) {
  stopAiming();
  if (m_overlay) {
    m_overlay->hide();
  }
  QDialog::hideEvent(event);
}

} // namespace mitcad
