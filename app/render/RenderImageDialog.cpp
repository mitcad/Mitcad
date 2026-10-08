// SPDX-License-Identifier: MIT
#include "render/RenderImageDialog.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <iterator>
#include <utility>

#include <QCheckBox>
#include <QCloseEvent>
#include <QComboBox>
#include <QDir>
#include <QDoubleSpinBox>
#include <QFile>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QImage>
#include <QJsonArray>
#include <QLabel>
#include <QPainter>
#include <QPainterPath>
#include <QPixmap>
#include <QProgressBar>
#include <QPushButton>
#include <QScreen>
#include <QShowEvent>
#include <QSpinBox>
#include <QVBoxLayout>
#include <QtLogging>

#include <Graphic3d_Camera.hxx>

#include "OcctViewer.hpp"
#include "browser/DocumentHost.hpp"
#include "framework/Icons.hpp"
#include "framework/TestSync.hpp"
#include "framework/Theme.hpp"
#include "render/RenderBatch.hpp"
#include "render/RenderDevice.hpp"
#include "render/RenderScene.hpp"

namespace mitcad {

namespace {

// The size presets: width, height (a fixed aspect).
struct SizePreset {
  int width;
  int height;
  const char* name;
};
const SizePreset kPresets[] = {
    {640, 480, nullptr},           {800, 600, nullptr},
    {1280, 720, "HD"},             {1920, 1080, "Full HD"},
    {2560, 1440, "QHD"},           {3840, 2160, "4K UHD"},
    {1080, 1080, "square"},        {1080, 1350, "portrait"},
};

constexpr QSize kPreviewSize(400, 300);

QString number(double value) { return QString::number(value, 'g', 6); }

// Seconds below a minute, else minutes and seconds.
QString duration(double seconds) {
  if (seconds < 59.5) {
    return QObject::tr("%1 s").arg(std::max(0.0, seconds), 0, 'f', seconds < 10.0 ? 1 : 0);
  }
  const int whole = static_cast<int>(std::lround(seconds));
  return QStringLiteral("%1:%2").arg(whole / 60).arg(whole % 60, 2, 10, QLatin1Char('0'));
}

std::array<double, 3> xyz(const gp_XYZ& v) { return {v.X(), v.Y(), v.Z()}; }

} // namespace

// The image's frame over the view (a fixed aspect): the view outside it
// dimmed, its edge drawn. Takes no input.
class RenderFrameOverlay : public QWidget {
public:
  explicit RenderFrameOverlay(QWidget* view) : QWidget(view) {
    setAttribute(Qt::WA_TransparentForMouseEvents);
    setAttribute(Qt::WA_NoSystemBackground);
    setObjectName(QStringLiteral("renderImageFrame"));
  }
  void setFrame(const QRectF& frame) {
    if (frame != m_frame) {
      m_frame = frame;
      update();
    }
  }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    QPainterPath outside;
    outside.addRect(QRectF(rect()));
    QPainterPath inside;
    inside.addRect(m_frame);
    painter.fillPath(outside.subtracted(inside), QColor(0, 0, 0, 60));
    painter.setPen(QPen(QColor(224, 123, 26), 1.5, Qt::DashLine));
    painter.drawRect(m_frame.adjusted(0.5, 0.5, -0.5, -0.5));
  }

private:
  QRectF m_frame;
};

RenderImageDialog::RenderImageDialog(DocumentHost& host, OcctViewer& viewer, QWidget* parent)
    : QDialog(parent), m_host(host), m_viewer(viewer) {
  setWindowTitle(tr("Render Image"));
  setWindowIcon(themeIcon(QStringLiteral("render-image")));
  auto* layout = new QVBoxLayout(this);
  auto* columns = new QHBoxLayout;
  layout->addLayout(columns);
  auto* settings = new QVBoxLayout;
  columns->addLayout(settings);
  const auto group = [settings](const QString& title) {
    auto* box = new QGroupBox(title);
    auto* form = new QFormLayout(box);
    settings->addWidget(box);
    return form;
  };
  const auto spin = [this](const QString& field, int low, int high, const QString& suffix, const QString& tip) {
    auto* box = new QSpinBox;
    box->setRange(low, high);
    box->setSuffix(suffix);
    box->setKeyboardTracking(false);
    box->setToolTip(tip);
    connect(box, &QSpinBox::valueChanged, this, [this, field](int value) {
      if (!m_building) {
        change(field, value);
      }
    });
    return box;
  };
  const auto check = [this](const QString& field, const QString& text, const QString& tip) {
    auto* box = new QCheckBox(text);
    box->setToolTip(tip);
    connect(box, &QCheckBox::toggled, this, [this, field](bool on) {
      if (!m_building) {
        change(field, on);
      }
    });
    return box;
  };
  const auto choice = [this](const QString& field, const QVector<std::pair<QString, QString>>& items) {
    auto* box = new QComboBox;
    for (const auto& [id, text] : items) {
      box->addItem(text, id);
    }
    connect(box, &QComboBox::currentIndexChanged, this, [this, field, box](int index) {
      if (!m_building && index >= 0) {
        change(field, box->itemData(index).toString());
      }
    });
    return box;
  };

  QFormLayout* image = group(tr("Image"));
  m_camera = new QComboBox;
  m_camera->setToolTip(tr("What the image shows: the view as it is now, or a named view of the design"));
  image->addRow(tr("Camera"), m_camera);
  connect(m_camera, &QComboBox::currentIndexChanged, this, [this] {
    if (!m_building) {
      qDebug().noquote() << QStringLiteral("Render image camera %1")
                                .arg(m_camera->currentData().toString().isEmpty() ? QStringLiteral("current view")
                                                                                   : m_camera->currentData().toString());
      updateFrame();
    }
  });
  m_preset = new QComboBox;
  m_preset->addItem(tr("Custom"));
  for (const SizePreset& preset : kPresets) {
    const QString size = QStringLiteral("%1 × %2").arg(preset.width).arg(preset.height);
    m_preset->addItem(preset.name != nullptr ? QStringLiteral("%1 (%2)").arg(size, tr(preset.name)) : size);
  }
  m_preset->setToolTip(tr("A common image size; it fixes the aspect"));
  image->addRow(tr("Size"), m_preset);
  connect(m_preset, &QComboBox::currentIndexChanged, this, &RenderImageDialog::choosePreset);
  m_width = spin(QStringLiteral("width"), 16, 16384, QStringLiteral(" px"), tr("The image's width in pixels"));
  image->addRow(tr("Width"), m_width);
  m_height = spin(QStringLiteral("height"), 16, 16384, QStringLiteral(" px"),
                  tr("The image's height in pixels (with a fixed aspect)"));
  image->addRow(tr("Height"), m_height);
  m_aspect = choice(QStringLiteral("aspect"),
                    {{QStringLiteral("view"), tr("From the View")}, {QStringLiteral("fixed"), tr("Fixed")}});
  m_aspect->setToolTip(tr("From the view: as high as the view is in proportion. Fixed: width × height, and the "
                          "view shows the image's frame"));
  image->addRow(tr("Aspect"), m_aspect);

  QFormLayout* quality = group(tr("Quality"));
  m_samples = spin(QStringLiteral("samples"), 1, 65536, QString(),
                   tr("Samples per pixel: more give less noise and take longer"));
  quality->addRow(tr("Samples"), m_samples);
  m_timeLimit = new QDoubleSpinBox;
  m_timeLimit->setRange(0.0, 86400.0);
  m_timeLimit->setDecimals(1);
  m_timeLimit->setSingleStep(10.0);
  m_timeLimit->setSuffix(QStringLiteral(" s"));
  m_timeLimit->setSpecialValueText(tr("None"));
  m_timeLimit->setKeyboardTracking(false);
  m_timeLimit->setToolTip(tr("Stop after this time even before all samples are done"));
  connect(m_timeLimit, &QDoubleSpinBox::valueChanged, this, [this](double seconds) {
    if (!m_building) {
      change(QStringLiteral("time_limit"), seconds);
    }
  });
  quality->addRow(tr("Time limit"), m_timeLimit);
  m_denoise = check(QStringLiteral("denoise"), tr("Denoise"), tr("Removes the remaining noise once the samples are done"));
  quality->addRow(m_denoise);

  QFormLayout* file = group(tr("File"));
  m_format = choice(QStringLiteral("format"), {{QStringLiteral("png"), tr("PNG")},
                                               {QStringLiteral("png16"), tr("PNG, 16 bits per channel")},
                                               {QStringLiteral("jpeg"), tr("JPEG")},
                                               {QStringLiteral("exr"), tr("OpenEXR (linear light)")}});
  m_format->setToolTip(tr("PNG and JPEG as the view shows the render (exposure and view transform); OpenEXR keeps "
                          "the render's linear light as it is, without them"));
  file->addRow(tr("Format"), m_format);
  m_quality = spin(QStringLiteral("quality"), 1, 100, QString(), tr("JPEG's quality: higher is larger and finer"));
  file->addRow(tr("JPEG quality"), m_quality);
  m_transparent = check(QStringLiteral("transparent"), tr("Transparent background"),
                        tr("Only the bodies and their shadows, with an alpha channel (not JPEG)"));
  file->addRow(m_transparent);
  settings->addStretch(1);

  auto* result = new QVBoxLayout;
  columns->addLayout(result, 1);
  m_preview = new QLabel;
  m_preview->setObjectName(QStringLiteral("renderImagePreview"));
  m_preview->setMinimumSize(kPreviewSize);
  m_preview->setAlignment(Qt::AlignCenter);
  m_preview->setFrameShape(QFrame::StyledPanel);
  m_preview->setText(tr("Render shows the image here as it refines."));
  m_preview->setWordWrap(true);
  result->addWidget(m_preview, 1);
  m_progress = new QProgressBar;
  m_progress->setRange(0, 1);
  m_progress->setValue(0);
  m_progress->setFormat(tr("%v of %m samples"));
  result->addWidget(m_progress);
  m_status = new QLabel;
  m_status->setWordWrap(true);
  result->addWidget(m_status);
  m_message = new QLabel;
  m_message->setWordWrap(true);
  setErrorStyleSheet(m_message);
  result->addWidget(m_message);

  auto* buttons = new QHBoxLayout;
  m_render = new QPushButton(themeIcon(QStringLiteral("render-image")), tr("Render"));
  m_render->setToolTip(tr("Renders the image in the background; Mitcad stays usable"));
  m_cancel = new QPushButton(tr("Cancel"));
  m_cancel->setToolTip(tr("Stops the render"));
  m_save = new QPushButton(tr("Save..."));
  m_save->setToolTip(tr("Saves the rendered image"));
  m_close = new QPushButton(tr("Close"));
  // No default button: Return in a field takes its value, nothing more.
  for (QPushButton* button : {m_render, m_cancel, m_save, m_close}) {
    button->setAutoDefault(false);
  }
  buttons->addWidget(m_render);
  buttons->addWidget(m_cancel);
  buttons->addWidget(m_save);
  buttons->addStretch(1);
  buttons->addWidget(m_close);
  layout->addLayout(buttons);
  connect(m_render, &QPushButton::clicked, this, &RenderImageDialog::startRender);
  connect(m_cancel, &QPushButton::clicked, this, &RenderImageDialog::cancelRender);
  connect(m_save, &QPushButton::clicked, this, &RenderImageDialog::save);
  connect(m_close, &QPushButton::clicked, this, &QDialog::close);

  m_job = new render::FinalRender(this);
  connect(m_job, &render::FinalRender::ready, this, [](const QString& renderer) {
    qDebug().noquote() << QStringLiteral("Render image worker ready: %1").arg(renderer);
  });
  connect(m_job, &render::FinalRender::progress, this, &RenderImageDialog::showProgress);
  connect(m_job, &render::FinalRender::preview, this,
          [this](const QString& path, int samples) {
            qDebug().noquote() << QStringLiteral("Render image preview (%1 samples)").arg(samples);
            showPreview(path);
          });
  connect(m_job, &render::FinalRender::warning, this, [this](const QString& text) {
    qWarning().noquote() << QStringLiteral("Render image warning: %1").arg(text);
    m_message->setText(text);
  });
  connect(m_job, &render::FinalRender::finished, this,
          [this](const QString& path, const QSize& size, int samples, double seconds) {
            m_image = path;
            setRendering(false);
            m_progress->setRange(0, std::max(1, samples));
            m_progress->setValue(samples);
            m_status->setText(tr("Done: %1 × %2 pixels, %3 samples in %4.")
                                  .arg(size.width())
                                  .arg(size.height())
                                  .arg(samples)
                                  .arg(duration(seconds)));
            showPreview(m_dir ? m_dir->filePath(QStringLiteral("preview.png")) : QString());
            qInfo().noquote() << QStringLiteral("Render image done: %1 x %2, %3 samples in %4 s")
                                     .arg(size.width())
                                     .arg(size.height())
                                     .arg(samples)
                                     .arg(seconds, 0, 'f', 2);
          });
  // Without a preview yet, the preview says why there is none.
  const auto noImage = [this](const QString& text) {
    if (m_preview->pixmap().isNull()) {
      m_preview->setText(text);
    }
  };
  connect(m_job, &render::FinalRender::failed, this, [this, noImage](const QString& reason) {
    setRendering(false);
    noImage(tr("No image."));
    m_status->clear();
    m_message->setText(tr("The render failed: %1.").arg(reason));
    qWarning().noquote() << QStringLiteral("Render image failed: %1").arg(reason);
  });
  connect(m_job, &render::FinalRender::cancelled, this, [this, noImage] {
    setRendering(false);
    noImage(tr("Cancelled."));
    m_status->setText(tr("Cancelled."));
    qInfo().noquote() << "Render image cancelled";
  });
  m_viewer.installEventFilter(this);
  setRendering(false);
  refresh();
}

RenderImageDialog::~RenderImageDialog() {
  m_viewer.removeEventFilter(this);
  if (m_frame) {
    m_frame->deleteLater();
  }
}

bool RenderImageDialog::eventFilter(QObject* watched, QEvent* event) {
  if (watched == &m_viewer && event->type() == QEvent::Resize) {
    m_building = true;
    m_height->setValue(render::outputSize(m_settings.value(QStringLiteral("output")).toObject(), viewAspect()).height());
    m_building = false;
    updateFrame();
  }
  return QDialog::eventFilter(watched, event);
}

QJsonValue RenderImageDialog::value(const QString& field) const {
  return m_settings.value(QStringLiteral("output")).toObject().value(field);
}

void RenderImageDialog::setDocument(const QString& folder, const QString& name) {
  m_documentFolder = folder;
  m_documentName = name;
}

bool RenderImageDialog::isRendering() const { return m_waiting || m_job->isRunning(); }

void RenderImageDialog::refresh() {
  try {
    m_settings = m_host.model().queryObject({{QStringLiteral("query"), QStringLiteral("render_settings")}});
  } catch (const std::exception& e) {
    m_message->setText(QString::fromUtf8(e.what()));
    return;
  }
  m_building = true;
  // The named views, keeping the chosen one while it is there.
  const QString chosen = m_camera->currentData().toString();
  m_camera->clear();
  m_camera->addItem(tr("Current View"), QString());
  try {
    for (const QJsonValue& view : m_host.model().queryArray(QStringLiteral("named_views"))) {
      const QString name = view.toObject().value(QStringLiteral("name")).toString();
      m_camera->addItem(name, name);
    }
  } catch (const std::exception&) {
    // No named views: the current view only.
  }
  m_camera->setCurrentIndex(std::max(0, m_camera->findData(chosen)));
  const int width = value(QStringLiteral("width")).toInt();
  const int height = value(QStringLiteral("height")).toInt();
  const bool fixed = value(QStringLiteral("aspect")).toString() == QLatin1String("fixed");
  m_width->setValue(width);
  // With the view's aspect, the height the view gives.
  m_height->setValue(render::outputSize(m_settings.value(QStringLiteral("output")).toObject(), viewAspect()).height());
  m_aspect->setCurrentIndex(m_aspect->findData(value(QStringLiteral("aspect")).toString()));
  int preset = 0;
  for (int i = 0; fixed && i < static_cast<int>(std::size(kPresets)); ++i) {
    if (kPresets[i].width == width && kPresets[i].height == height) {
      preset = i + 1;
    }
  }
  m_preset->setCurrentIndex(preset);
  m_samples->setValue(value(QStringLiteral("samples")).toInt());
  m_timeLimit->setValue(value(QStringLiteral("time_limit")).toDouble());
  m_denoise->setChecked(value(QStringLiteral("denoise")).toBool());
  m_transparent->setChecked(value(QStringLiteral("transparent")).toBool());
  m_format->setCurrentIndex(m_format->findData(value(QStringLiteral("format")).toString()));
  m_quality->setValue(value(QStringLiteral("quality")).toInt());
  m_building = false;
  updateEnabled();
  updateFrame();
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void RenderImageDialog::updateEnabled() {
  const bool fixed = value(QStringLiteral("aspect")).toString() == QLatin1String("fixed");
  const QString format = value(QStringLiteral("format")).toString();
  m_height->setEnabled(fixed);
  m_quality->setEnabled(format == QLatin1String("jpeg"));
  m_transparent->setEnabled(format != QLatin1String("jpeg"));
}

void RenderImageDialog::change(const QString& field, const QJsonValue& value) {
  if (!m_host.canChangeModel()) {
    m_message->setText(tr("Finish the command or the sketch first."));
    qDebug().noquote() << "Render settings: finish the command or the sketch first";
    refresh();
    return;
  }
  const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_render_settings")},
                            {QStringLiteral("output"), QJsonObject{{field, value}}}};
  const QString text = value.isDouble() ? number(value.toDouble())
                       : value.isBool() ? (value.toBool() ? QStringLiteral("true") : QStringLiteral("false"))
                                        : value.toString();
  const QString key = QStringLiteral("output.%1").arg(field);
  if (m_host.runModelCommand(command)) {
    m_message->clear();
    qDebug().noquote() << QStringLiteral("Render setting %1 = %2").arg(key, text);
  } else {
    m_message->setText(m_host.lastError());
    qDebug().noquote() << QStringLiteral("Render setting %1 = %2 refused: %3").arg(key, text, m_host.lastError());
  }
  refresh();
}

void RenderImageDialog::choosePreset(int index) {
  if (m_building || index <= 0 || index > static_cast<int>(std::size(kPresets))) {
    return;
  }
  if (!m_host.canChangeModel()) {
    m_message->setText(tr("Finish the command or the sketch first."));
    refresh();
    return;
  }
  const SizePreset& preset = kPresets[index - 1];
  const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_render_settings")},
                            {QStringLiteral("output"), QJsonObject{{QStringLiteral("width"), preset.width},
                                                                   {QStringLiteral("height"), preset.height},
                                                                   {QStringLiteral("aspect"), QStringLiteral("fixed")}}}};
  if (m_host.runModelCommand(command)) {
    m_message->clear();
    qDebug().noquote() << QStringLiteral("Render setting output.size = %1 x %2").arg(preset.width).arg(preset.height);
  } else {
    m_message->setText(m_host.lastError());
  }
  refresh();
}

double RenderImageDialog::viewAspect() const {
  return m_viewer.height() > 0 ? static_cast<double>(m_viewer.width()) / m_viewer.height() : 1.0;
}

void RenderImageDialog::updateFrame() {
  const bool fixed = value(QStringLiteral("aspect")).toString() == QLatin1String("fixed");
  const bool current = m_camera->currentData().toString().isEmpty();
  if (!isVisible() || !fixed || !current) {
    if (m_frame && m_frame->isVisible()) {
      m_frame->hide();
      qDebug().noquote() << "Render image frame hidden";
    }
    return;
  }
  if (!m_frame) {
    m_frame = new RenderFrameOverlay(&m_viewer);
  }
  const QRectF frame = render::imageFrame(QSizeF(m_viewer.size()),
                                          QSizeF(value(QStringLiteral("width")).toInt(),
                                                 value(QStringLiteral("height")).toInt()));
  const bool shown = m_frame->isVisible();
  const QRect before = m_frame->geometry();
  m_frame->setGeometry(m_viewer.rect());
  m_frame->setFrame(frame);
  m_frame->show();
  m_frame->raise();
  // In the main window's coordinates, as the view's area is logged.
  const QPoint corner = m_viewer.mapTo(m_viewer.window(), frame.topLeft().toPoint());
  const QString line = QStringLiteral("Render image frame %1 %2 %3 %4")
                           .arg(corner.x())
                           .arg(corner.y())
                           .arg(qRound(frame.width()))
                           .arg(qRound(frame.height()));
  if (!shown || before != m_frame->geometry() || line != m_frame->property("logged").toString()) {
    m_frame->setProperty("logged", line);
    qDebug().noquote() << line;
  }
}

void RenderImageDialog::setRendering(bool rendering) {
  m_render->setEnabled(!rendering);
  m_cancel->setEnabled(rendering);
  m_save->setEnabled(!rendering && !m_image.isEmpty());
  updateEnabled();
}

void RenderImageDialog::startRender() {
  if (isRendering()) {
    return;
  }
  m_message->clear();
  m_image.clear();
  m_waiting = true;
  setRendering(true);
  m_status->setText(tr("Waiting for the model..."));
  // The bodies' triangulations are read only while no job computes them.
  m_host.whenIdle(this, [this] { renderScene(); });
}

void RenderImageDialog::renderScene() {
  if (!m_waiting) {
    return; // cancelled meanwhile
  }
  m_waiting = false;
  refresh();
  const QJsonObject output = m_settings.value(QStringLiteral("output")).toObject();
  const double aspect = viewAspect();
  const QSize size = render::outputSize(output, aspect);
  render::Camera camera;
  const QString named = m_camera->currentData().toString();
  bool found = named.isEmpty();
  if (named.isEmpty()) {
    const occ::handle<Graphic3d_Camera>& view = m_viewer.viewCamera();
    camera.eye = xyz(view->Eye().XYZ());
    camera.target = xyz(view->Center().XYZ());
    camera.up = xyz(view->Up().XYZ());
    camera.perspective = !view->IsOrthographic();
    camera.halfHeight = view->ViewDimensions().Y() / 2.0;
    camera.aspect = aspect;
  } else {
    for (const QJsonValue& view : m_host.model().queryArray(QStringLiteral("named_views"))) {
      if (view.toObject().value(QStringLiteral("name")).toString() == named) {
        camera = render::namedViewCamera(view.toObject(), static_cast<double>(size.width()) / size.height());
        found = true;
      }
    }
  }
  if (!found) {
    setRendering(false);
    m_message->setText(tr("There is no named view %1.").arg(named));
    return;
  }
  // A folder of its own for each render (a finished image stays until the
  // next one).
  m_dir = std::make_unique<QTemporaryDir>();
  if (!m_dir->isValid()) {
    setRendering(false);
    m_message->setText(tr("No temporary folder for the render: %1.").arg(m_dir->errorString()));
    return;
  }
  std::vector<render::SceneBody> bodies;
  const std::vector<BodyDisplay> shown = m_viewer.shownBodies();
  std::vector<const Appearance*> looks;
  for (const BodyDisplay& body : shown) {
    render::SceneBody scene;
    scene.name = body.occurrence.empty() ? body.uid : body.uid + "@" + body.occurrence;
    scene.shape = body.shape->occt();
    scene.placement = body.placement.Transformation();
    scene.appearance = body.appearance;
    looks.push_back(&body.appearance);
    for (const BodyDisplay::FaceLook& look : body.faceLooks) {
      scene.faces.push_back({look.faces, look.appearance});
      looks.push_back(&look.appearance);
    }
    bodies.push_back(std::move(scene));
  }
  // Textures whose images are missing show their base colour (mitcad#53).
  const QStringList missing = render::missingTextures(looks);
  const QString scenePath = m_dir->filePath(QStringLiteral("scene.glb"));
  std::string error;
  if (!render::writeSceneFile(render::sceneOf(bodies), QDir::toNativeSeparators(scenePath).toStdString(), error)) {
    setRendering(false);
    m_message->setText(tr("The scene cannot be written: %1.").arg(QString::fromStdString(error)));
    return;
  }
  QColor top;
  QColor bottom;
  const QJsonObject background = m_settings.value(QStringLiteral("background")).toObject();
  if (background.value(QStringLiteral("mode")).toString() == QLatin1String("color")) {
    const QJsonArray rgb = background.value(QStringLiteral("color")).toArray();
    top = bottom = QColor::fromRgbF(static_cast<float>(rgb.at(0).toDouble()), static_cast<float>(rgb.at(1).toDouble()),
                                    static_cast<float>(rgb.at(2).toDouble()));
  } else {
    m_viewer.backgroundColors(top, bottom);
  }
  m_imageFormat = output.value(QStringLiteral("format")).toString();
  const QString imagePath =
      m_dir->filePath(QStringLiteral("render.%1").arg(render::outputExtension(m_imageFormat)));
  QJsonObject job = render::finalJob(m_settings, m_documentFolder, scenePath, render::finalView(camera, size),
                                     imagePath, top, bottom, m_dir->filePath(QStringLiteral("preview.png")));
  // The render device of Preferences (mitcad#50).
  job.insert(QStringLiteral("device"), render::renderDeviceChoice());
  m_progress->setRange(0, output.value(QStringLiteral("samples")).toInt(1));
  m_progress->setValue(0);
  m_preview->setPixmap(QPixmap());
  m_preview->setText(tr("Starting the renderer..."));
  m_status->setText(tr("Rendering %1 × %2 pixels...").arg(size.width()).arg(size.height()));
  if (m_job->start(render::workerExecutable(), job, m_dir->filePath(QStringLiteral("job.json")))) {
    qInfo().noquote() << QStringLiteral("Render image started: %1 x %2, %3 samples, %4, %5, %6 bodies (pid %7)")
                             .arg(size.width())
                             .arg(size.height())
                             .arg(output.value(QStringLiteral("samples")).toInt())
                             .arg(m_imageFormat, named.isEmpty() ? QStringLiteral("current view") : named)
                             .arg(bodies.size())
                             .arg(m_job->processId());
  }
  if (!missing.isEmpty()) {
    const QString text = missing.join(QStringLiteral("; "));
    qWarning().noquote() << QStringLiteral("Render image textures not drawn: %1").arg(text);
    m_message->setText(tr("A texture is not drawn, its base colour is shown instead (%1).").arg(text));
  }
  setRendering(true);
}

void RenderImageDialog::cancelRender() {
  if (m_waiting) {
    m_waiting = false;
    setRendering(false);
    m_preview->setText(tr("Cancelled."));
    m_status->setText(tr("Cancelled."));
    qInfo().noquote() << "Render image cancelled";
    return;
  }
  if (m_job->isRunning()) {
    qDebug().noquote() << "Render image cancelling";
    m_status->setText(tr("Cancelling..."));
    m_job->cancel();
  }
}

void RenderImageDialog::showProgress(int samples, int total, double seconds, double remaining) {
  m_progress->setRange(0, std::max(1, total));
  m_progress->setValue(std::min(samples, total));
  QString text = tr("%1 of %2 samples, %3").arg(samples).arg(total).arg(duration(seconds));
  if (samples >= total) {
    text += tr(", denoising");
  } else if (remaining >= 0.0) {
    text += tr(", about %1 left").arg(duration(remaining));
  }
  m_status->setText(text);
  qDebug().noquote() << QStringLiteral("Render image progress %1/%2").arg(samples).arg(total);
}

void RenderImageDialog::showPreview(const QString& path) {
  const QImage image(path);
  if (image.isNull()) {
    return;
  }
  m_preview->setPixmap(QPixmap::fromImage(
      image.scaled(m_preview->contentsRect().size(), Qt::KeepAspectRatio, Qt::SmoothTransformation)));
}

void RenderImageDialog::save() {
  if (m_image.isEmpty() || !QFileInfo::exists(m_image)) {
    return;
  }
  const QString extension = render::outputExtension(m_imageFormat);
  const QString filter = m_imageFormat == QLatin1String("jpeg") ? tr("JPEG images (*.jpg *.jpeg)")
                         : m_imageFormat == QLatin1String("exr") ? tr("OpenEXR images (*.exr)")
                                                                  : tr("PNG images (*.png)");
  const QString name = (m_documentName.isEmpty() ? tr("Render") : m_documentName) + QLatin1Char('.') + extension;
  const QString start = m_documentFolder.isEmpty() ? name : QDir(m_documentFolder).filePath(name);
  qDebug().noquote() << "Render image save dialog";
  QString path = QFileDialog::getSaveFileName(this, tr("Save Rendered Image"), start, filter);
  if (path.isEmpty()) {
    return;
  }
  if (QFileInfo(path).suffix().isEmpty()) {
    path += QLatin1Char('.') + extension;
  }
  if (QFileInfo::exists(path)) {
    QFile::remove(path); // the dialog asked before replacing it
  }
  if (!QFile::copy(m_image, path)) {
    m_message->setText(tr("The image cannot be saved as %1.").arg(QDir::toNativeSeparators(path)));
    qWarning().noquote() << QStringLiteral("Render image not saved: %1").arg(path);
    return;
  }
  m_message->clear();
  m_status->setText(tr("Saved as %1.").arg(QDir::toNativeSeparators(path)));
  qInfo().noquote() << QStringLiteral("Render image saved %1").arg(path);
}

void RenderImageDialog::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  updateFrame();
  if (m_placed || parentWidget() == nullptr) {
    return;
  }
  m_placed = true;
  // Beside the main window, where it leaves the view free, as far as the
  // screen allows.
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

void RenderImageDialog::hideEvent(QHideEvent* event) {
  QDialog::hideEvent(event);
  updateFrame();
}

void RenderImageDialog::closeEvent(QCloseEvent* event) {
  // Closing stops a render: nothing would show or save it.
  if (isRendering()) {
    cancelRender();
  }
  QDialog::closeEvent(event);
}

void RenderImageDialog::logLayout() {
  if (!isVisible() || parentWidget() == nullptr) {
    return;
  }
  const QPoint origin = parentWidget()->window()->mapToGlobal(QPoint(0, 0));
  const auto at = [&origin](QWidget* widget) {
    const QPoint global = widget->mapToGlobal(widget->rect().center()) - origin;
    return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
  };
  const std::pair<const char*, QWidget*> places[] = {
      {"field output.width", m_width},         {"field output.height", m_height},
      {"field output.aspect", m_aspect},       {"field output.samples", m_samples},
      {"field output.time_limit", m_timeLimit}, {"field output.denoise", m_denoise},
      {"field output.transparent", m_transparent}, {"field output.format", m_format},
      {"field output.quality", m_quality},     {"camera", m_camera},
      {"size", m_preset},                      {"render", m_render},
      {"cancel", m_cancel},                    {"save", m_save},
      {"close", m_close}};
  QStringList lines;
  for (const auto& [name, widget] : places) {
    lines << QStringLiteral("Render image %1 at %2").arg(QLatin1String(name), at(widget));
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
