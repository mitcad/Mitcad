// SPDX-License-Identifier: MIT
#include "view/TurntableDialog.hpp"

#include <algorithm>
#include <cmath>
#include <utility>

#include <QComboBox>
#include <QCoreApplication>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QDoubleSpinBox>
#include <QEvent>
#include <QFileInfo>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPushButton>
#include <QSettings>
#include <QSignalBlocker>
#include <QSpinBox>
#include <QStandardItemModel>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../platform/MacDialogs.hpp"

namespace mitcad {
namespace {

const QString kGroup = QStringLiteral("turntable/");
// The largest side of a frame: what the video encoders take.
constexpr int kLargest = 8192;

// The frames' size for a size key: the view's, a preset or `custom`.
QSize sizeOf(const QString& key, const QSize& view, const QSize& custom) {
  if (key == QLatin1String("720p")) {
    return QSize(1280, 720);
  }
  if (key == QLatin1String("1080p")) {
    return QSize(1920, 1080);
  }
  if (key == QLatin1String("2160p")) {
    return QSize(3840, 2160);
  }
  if (key == QLatin1String("custom")) {
    return custom;
  }
  return QSize(std::clamp(view.width(), 16, kLargest), std::clamp(view.height(), 16, kLargest));
}

// The file field's text for another output: the video's extension, or the
// frames' folder named after it.
QString pathFor(const QString& path, TurntableOutput output) {
  const QFileInfo file(path.trimmed());
  QString base = file.completeBaseName();
  if (file.suffix().isEmpty() && base.endsWith(QLatin1String("-frames"))) {
    base.chop(7);
  }
  if (base.isEmpty()) {
    base = QStringLiteral("turntable");
  }
  const QString folder = file.path();
  const QString name = turntableHasVideo(output) ? base + QLatin1Char('.') + turntableExtension(output)
                                                 : base + QStringLiteral("-frames");
  return folder.isEmpty() || folder == QLatin1String(".") ? name : QDir(folder).filePath(name);
}

// The frames' folder of a video: beside it, named after it.
QString framesBeside(const QString& video) {
  const QFileInfo file(video);
  return QDir(file.path()).filePath(file.completeBaseName() + QStringLiteral("-frames"));
}

} // namespace

std::optional<TurntableChoice> askTurntable(QWidget* parent, const TurntableDialogInput& input) {
  QSettings store;
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Export Turntable"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* form = new QFormLayout;
  layout->addLayout(form);

  QString ffmpeg = findFfmpeg(store.value(kGroup + QStringLiteral("ffmpeg")).toString());

  auto* output = new QComboBox;
  output->setObjectName(QStringLiteral("turntableOutput"));
  output->addItem(QObject::tr("MP4 video"), turntableOutputKey(TurntableOutput::Mp4));
  output->addItem(QObject::tr("MP4 video and PNG frames"), turntableOutputKey(TurntableOutput::Mp4Frames));
  output->addItem(QObject::tr("WebM video"), turntableOutputKey(TurntableOutput::WebM));
  output->addItem(QObject::tr("WebM video and PNG frames"), turntableOutputKey(TurntableOutput::WebMFrames));
  output->addItem(QObject::tr("PNG frames"), turntableOutputKey(TurntableOutput::Frames));
  form->addRow(QObject::tr("&Save as:"), output);

  auto* file = new QLineEdit;
  file->setObjectName(QStringLiteral("turntablePath"));
  auto* browse = new QPushButton(QObject::tr("Browse…"));
  mac::auxiliaryButton(browse);
  auto* fileRow = new QHBoxLayout;
  fileRow->addWidget(file, 1);
  fileRow->addWidget(browse);
  auto* fileLabel = new QLabel(QObject::tr("&File:"));
  fileLabel->setBuddy(file);
  form->addRow(fileLabel, fileRow);

  auto* ffmpegText = new QLabel;
  ffmpegText->setObjectName(QStringLiteral("turntableFfmpeg"));
  ffmpegText->setWordWrap(true);
  ffmpegText->setTextInteractionFlags(Qt::TextSelectableByMouse);
  auto* locate = new QPushButton(QObject::tr("Locate FFmpeg…"));
  locate->setObjectName(QStringLiteral("turntableLocateFfmpeg"));
  locate->setToolTip(QObject::tr("Chooses the FFmpeg program that encodes the video"));
  mac::auxiliaryButton(locate);
  auto* ffmpegRow = new QHBoxLayout;
  ffmpegRow->addWidget(ffmpegText, 1);
  ffmpegRow->addWidget(locate);
  form->addRow(QObject::tr("Video encoder:"), ffmpegRow);

  auto* axis = new QComboBox;
  axis->setObjectName(QStringLiteral("turntableAxis"));
  axis->addItem(QObject::tr("Z axis of the design"), QStringLiteral("z"));
  axis->addItem(QObject::tr("Y axis of the design"), QStringLiteral("y"));
  axis->addItem(QObject::tr("X axis of the design"), QStringLiteral("x"));
  axis->addItem(QObject::tr("Up in the view"), QStringLiteral("up"));
  if (!input.selectedAxis.isEmpty()) {
    axis->addItem(QObject::tr("Axis of the selection (%1)").arg(input.selectedAxis), QStringLiteral("selection"));
  }
  axis->setToolTip(QObject::tr("The camera turns about this axis through the middle of the shown bodies"));
  form->addRow(QObject::tr("&Axis:"), axis);

  auto* angle = new QDoubleSpinBox;
  angle->setObjectName(QStringLiteral("turntableAngle"));
  angle->setRange(-3600.0, 3600.0);
  angle->setDecimals(1);
  angle->setSuffix(QObject::tr(" °"));
  angle->setToolTip(QObject::tr("The whole turn; a negative angle turns the other way"));
  form->addRow(QObject::tr("A&ngle:"), angle);

  auto* frames = new QSpinBox;
  frames->setObjectName(QStringLiteral("turntableFrames"));
  frames->setRange(1, kTurntableMaxFrames);
  form->addRow(QObject::tr("F&rames:"), frames);
  auto* fps = new QDoubleSpinBox;
  fps->setObjectName(QStringLiteral("turntableFps"));
  fps->setRange(1.0, kTurntableMaxFps);
  fps->setDecimals(2);
  fps->setSuffix(QObject::tr(" fps"));
  form->addRow(QObject::tr("Frame ra&te:"), fps);
  auto* duration = new QDoubleSpinBox;
  duration->setObjectName(QStringLiteral("turntableDuration"));
  duration->setRange(0.01, kTurntableMaxFrames);
  duration->setDecimals(2);
  duration->setSuffix(QObject::tr(" s"));
  duration->setToolTip(QObject::tr("Sets the frame count for the frame rate"));
  form->addRow(QObject::tr("&Duration:"), duration);
  auto* step = new QLabel;
  step->setObjectName(QStringLiteral("turntableStep"));
  form->addRow(QObject::tr("Between frames:"), step);

  auto* size = new QComboBox;
  size->setObjectName(QStringLiteral("turntableSize"));
  const QSize viewSize = sizeOf(QStringLiteral("view"), input.view, QSize());
  size->addItem(QObject::tr("The view (%1 × %2)").arg(viewSize.width()).arg(viewSize.height()), QStringLiteral("view"));
  size->addItem(QObject::tr("1280 × 720"), QStringLiteral("720p"));
  size->addItem(QObject::tr("1920 × 1080"), QStringLiteral("1080p"));
  size->addItem(QObject::tr("3840 × 2160"), QStringLiteral("2160p"));
  size->addItem(QObject::tr("Custom"), QStringLiteral("custom"));
  form->addRow(QObject::tr("Si&ze:"), size);
  auto* width = new QSpinBox;
  width->setObjectName(QStringLiteral("turntableWidth"));
  width->setRange(16, kLargest);
  width->setSuffix(QObject::tr(" px"));
  auto* height = new QSpinBox;
  height->setObjectName(QStringLiteral("turntableHeight"));
  height->setRange(16, kLargest);
  height->setSuffix(QObject::tr(" px"));
  auto* sizeRow = new QHBoxLayout;
  sizeRow->addWidget(width);
  sizeRow->addWidget(new QLabel(QObject::tr("×")));
  sizeRow->addWidget(height);
  sizeRow->addStretch(1);
  form->addRow(QObject::tr("&Width and height:"), sizeRow);

  auto* style = new QComboBox;
  style->setObjectName(QStringLiteral("turntableStyle"));
  style->addItem(QObject::tr("The view's visual style"), QStringLiteral("view"));
  style->addItem(QObject::tr("Rendered"), QStringLiteral("rendered"));
  if (!input.renderer) {
    if (auto* items = qobject_cast<QStandardItemModel*>(style->model())) {
      items->item(1)->setEnabled(false);
      items->item(1)->setToolTip(QObject::tr("No render worker next to MitCad"));
    }
  }
  style->setToolTip(QObject::tr("Rendered frames use the render settings of Render Image"));
  form->addRow(QObject::tr("St&yle:"), style);

  auto* message = new QLabel;
  message->setWordWrap(true);
  setErrorStyleSheet(message);
  layout->addWidget(message);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  QPushButton* exportButton = buttons->button(QDialogButtonBox::Ok);
  exportButton->setText(QObject::tr("Export"));
  exportButton->setObjectName(QStringLiteral("turntableExport"));
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  layout->addWidget(buttons);

  // The saved choices.
  const auto pick = [](QComboBox* box, const QString& key, int fallback) {
    const int index = box->findData(key);
    const bool usable = index >= 0 && box->model()->flags(box->model()->index(index, 0)).testFlag(Qt::ItemIsEnabled);
    box->setCurrentIndex(usable ? index : fallback);
  };
  pick(output, store.value(kGroup + QStringLiteral("output"), turntableOutputKey(TurntableOutput::Mp4)).toString(), 0);
  pick(axis, store.value(kGroup + QStringLiteral("axis"), QStringLiteral("z")).toString(), 0);
  pick(size, store.value(kGroup + QStringLiteral("size"), QStringLiteral("view")).toString(), 0);
  pick(style, store.value(kGroup + QStringLiteral("style"), QStringLiteral("view")).toString(), 0);
  angle->setValue(store.value(kGroup + QStringLiteral("angle"), 360.0).toDouble());
  frames->setValue(store.value(kGroup + QStringLiteral("frames"), 72).toInt());
  fps->setValue(store.value(kGroup + QStringLiteral("fps"), 24.0).toDouble());
  width->setValue(store.value(kGroup + QStringLiteral("width"), 1280).toInt());
  height->setValue(store.value(kGroup + QStringLiteral("height"), 720).toInt());

  const auto chosenOutput = [output] {
    return turntableOutputOf(output->currentData().toString()).value_or(TurntableOutput::Mp4);
  };
  // The video outputs need the FFmpeg program.
  const auto showFfmpeg = [&ffmpeg, output, ffmpegText, pick] {
    if (auto* items = qobject_cast<QStandardItemModel*>(output->model())) {
      for (int row = 0; row < items->rowCount(); ++row) {
        const auto kind = turntableOutputOf(items->item(row)->data(Qt::UserRole).toString());
        if (kind && turntableHasVideo(*kind)) {
          items->item(row)->setEnabled(!ffmpeg.isEmpty());
          items->item(row)->setToolTip(ffmpeg.isEmpty() ? QObject::tr("Needs the FFmpeg program") : QString());
        }
      }
    }
    if (!output->model()->flags(output->model()->index(output->currentIndex(), 0)).testFlag(Qt::ItemIsEnabled)) {
      pick(output, turntableOutputKey(TurntableOutput::Frames), output->count() - 1);
    }
    ffmpegText->setText(ffmpeg.isEmpty()
                            ? QObject::tr("No FFmpeg program found: install FFmpeg for video, or save PNG frames.")
                            : QObject::tr("FFmpeg, %1").arg(QDir::toNativeSeparators(ffmpeg)));
  };
  showFfmpeg();
  file->setText(pathFor(input.start, chosenOutput()));

  const auto follow = [=] {
    const TurntableOutput kind = chosenOutput();
    fileLabel->setText(turntableHasVideo(kind) ? QObject::tr("&File:") : QObject::tr("&Folder:"));
    const bool custom = size->currentData().toString() == QLatin1String("custom");
    width->setEnabled(custom);
    height->setEnabled(custom);
    {
      const QSignalBlocker block(duration);
      duration->setValue(frames->value() / fps->value());
    }
    const double degrees = turntableStep(angle->value(), frames->value());
    step->setText(QObject::tr("%1°").arg(QString::number(degrees, 'f', 2)));
  };
  follow();
  QObject::connect(output, &QComboBox::currentIndexChanged, &dialog, [=] {
    file->setText(pathFor(file->text(), chosenOutput()));
    follow();
    qDebug().noquote() << QStringLiteral("Turntable dialog: output = %1").arg(output->currentData().toString());
  });
  QObject::connect(size, &QComboBox::currentIndexChanged, &dialog, [=] {
    follow();
    qDebug().noquote() << QStringLiteral("Turntable dialog: size = %1").arg(size->currentData().toString());
  });
  QObject::connect(axis, &QComboBox::currentIndexChanged, &dialog, [axis] {
    qDebug().noquote() << QStringLiteral("Turntable dialog: axis = %1").arg(axis->currentData().toString());
  });
  QObject::connect(style, &QComboBox::currentIndexChanged, &dialog, [style] {
    qDebug().noquote() << QStringLiteral("Turntable dialog: style = %1").arg(style->currentData().toString());
  });
  QObject::connect(frames, &QSpinBox::valueChanged, &dialog, [follow] { follow(); });
  QObject::connect(fps, &QDoubleSpinBox::valueChanged, &dialog, [follow] { follow(); });
  QObject::connect(angle, &QDoubleSpinBox::valueChanged, &dialog, [follow] { follow(); });
  QObject::connect(duration, &QDoubleSpinBox::valueChanged, &dialog, [=](double seconds) {
    const QSignalBlocker block(frames);
    frames->setValue(std::clamp(static_cast<int>(std::lround(seconds * fps->value())), 1, kTurntableMaxFrames));
    const double degrees = turntableStep(angle->value(), frames->value());
    step->setText(QObject::tr("%1°").arg(QString::number(degrees, 'f', 2)));
  });
  QObject::connect(browse, &QPushButton::clicked, &dialog, [&dialog, file, chosenOutput] {
    const TurntableOutput kind = chosenOutput();
    QString picked;
    if (turntableHasVideo(kind)) {
      const QString filter = turntableExtension(kind) == QLatin1String("webm") ? QObject::tr("WebM video (*.webm)")
                                                                              : QObject::tr("MP4 video (*.mp4)");
      picked = dialogSaveFileName(&dialog, QObject::tr("Export Turntable"), file->text(), filter);
    } else {
      picked = dialogDirectory(&dialog, QObject::tr("Export Turntable"), QFileInfo(file->text()).path());
    }
    if (!picked.isEmpty()) {
      file->setText(turntableHasVideo(kind) ? pathFor(picked, kind) : picked);
    }
  });
  QObject::connect(locate, &QPushButton::clicked, &dialog, [&dialog, &ffmpeg, &store, showFfmpeg] {
    const QString picked = dialogOpenFileName(&dialog, QObject::tr("Locate FFmpeg"), QFileInfo(ffmpeg).path());
    if (picked.isEmpty()) {
      return;
    }
    const QString found = findFfmpeg(picked);
    if (found != QFileInfo(picked).absoluteFilePath()) {
      sheetWarning(&dialog, QObject::tr("Locate FFmpeg"),
                   QObject::tr("%1 is not a program.").arg(QDir::toNativeSeparators(picked)));
      return;
    }
    ffmpeg = found;
    store.setValue(kGroup + QStringLiteral("ffmpeg"), ffmpeg);
    qDebug().noquote() << QStringLiteral("Turntable dialog: FFmpeg = %1").arg(ffmpeg);
    showFfmpeg();
  });

  file->setFocus();
  file->selectAll();
  dialog.resize(560, dialog.sizeHint().height());
  qDebug().noquote() << QStringLiteral("Turntable dialog opened: %1, axis %2, %3 deg, %4 frames at %5 fps, size %6, "
                                       "style %7, FFmpeg %8")
                            .arg(output->currentData().toString(), axis->currentData().toString())
                            .arg(angle->value())
                            .arg(frames->value())
                            .arg(fps->value())
                            .arg(size->currentData().toString(), style->currentData().toString(),
                                 ffmpeg.isEmpty() ? QStringLiteral("none") : ffmpeg);
  // Where the inputs are, in the dialog's own coordinates.
  TestSync::singleShot(200, &dialog, [&dialog, output, file, axis, angle, frames, fps, size, style, buttons] {
    QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
    const auto at = [&dialog](const QWidget* widget) {
      const QPoint p = widget->mapTo(&dialog, widget->rect().center());
      return QStringLiteral("%1,%2").arg(p.x()).arg(p.y());
    };
    const std::pair<const char*, QWidget*> places[] = {{"output", output}, {"file", file},   {"axis", axis},
                                                       {"angle", angle},   {"frames", frames}, {"fps", fps},
                                                       {"size", size},     {"style", style}};
    for (const auto& [name, widget] : places) {
      qDebug().noquote() << QStringLiteral("Turntable dialog %1 at %2").arg(QLatin1String(name), at(widget));
    }
    if (QPushButton* ok = buttons->button(QDialogButtonBox::Ok)) {
      qDebug().noquote() << QStringLiteral("Turntable dialog Export at %1").arg(at(ok));
    }
  });

  prepareModal(&dialog);
  while (execDialog(&dialog) == QDialog::Accepted) {
    TurntableChoice choice;
    choice.output = chosenOutput();
    choice.axis = axis->currentData().toString();
    choice.angle = angle->value();
    choice.frames = frames->value();
    choice.fps = fps->value();
    choice.sizeKey = size->currentData().toString();
    choice.size = turntableFrameSize(sizeOf(choice.sizeKey, input.view, QSize(width->value(), height->value())),
                                     choice.output);
    choice.rendered = style->currentData().toString() == QLatin1String("rendered");
    choice.ffmpeg = ffmpeg;
    const QString path = file->text().trimmed();
    if (path.isEmpty()) {
      message->setText(turntableHasVideo(choice.output) ? QObject::tr("Give the video file to write.")
                                                        : QObject::tr("Give the folder for the frames."));
      continue;
    }
    if (choice.angle == 0.0) {
      message->setText(QObject::tr("Give an angle to turn."));
      continue;
    }
    if (turntableHasVideo(choice.output)) {
      if (ffmpeg.isEmpty()) {
        message->setText(QObject::tr("A video needs the FFmpeg program: install FFmpeg or locate it."));
        continue;
      }
      choice.video = QFileInfo(pathFor(path, choice.output)).absoluteFilePath();
      if (turntableKeepsFrames(choice.output)) {
        choice.framesFolder = framesBeside(choice.video);
      }
    } else {
      choice.framesFolder = QFileInfo(path).absoluteFilePath();
    }
    if (!choice.video.isEmpty() && QFileInfo::exists(choice.video) &&
        sheetQuestion(&dialog, QObject::tr("Export Turntable"),
                      QObject::tr("%1 exists. Replace it?").arg(QFileInfo(choice.video).fileName())) !=
            QMessageBox::Yes) {
      continue;
    }
    if (!choice.framesFolder.isEmpty() &&
        !QDir(choice.framesFolder).entryList({QStringLiteral("frame-*.png")}, QDir::Files).isEmpty() &&
        sheetQuestion(&dialog, QObject::tr("Export Turntable"),
                      QObject::tr("%1 has frames. Replace them?").arg(QDir::toNativeSeparators(choice.framesFolder))) !=
            QMessageBox::Yes) {
      continue;
    }
    store.setValue(kGroup + QStringLiteral("output"), turntableOutputKey(choice.output));
    store.setValue(kGroup + QStringLiteral("axis"), choice.axis);
    store.setValue(kGroup + QStringLiteral("angle"), choice.angle);
    store.setValue(kGroup + QStringLiteral("frames"), choice.frames);
    store.setValue(kGroup + QStringLiteral("fps"), choice.fps);
    store.setValue(kGroup + QStringLiteral("size"), choice.sizeKey);
    store.setValue(kGroup + QStringLiteral("width"), width->value());
    store.setValue(kGroup + QStringLiteral("height"), height->value());
    store.setValue(kGroup + QStringLiteral("style"), choice.rendered ? QStringLiteral("rendered") : QStringLiteral("view"));
    qDebug().noquote() << QStringLiteral("Turntable dialog: %1, %2 frames of %3 deg about %4 at %5 fps, %6 x %7, %8, "
                                         "video %9, frames %10")
                              .arg(turntableOutputKey(choice.output))
                              .arg(choice.frames)
                              .arg(choice.angle)
                              .arg(choice.axis)
                              .arg(choice.fps)
                              .arg(choice.size.width())
                              .arg(choice.size.height())
                              .arg(choice.rendered ? QStringLiteral("rendered") : QStringLiteral("view style"),
                                   choice.video.isEmpty() ? QStringLiteral("none") : choice.video,
                                   choice.framesFolder.isEmpty() ? QStringLiteral("none") : choice.framesFolder);
    return choice;
  }
  return std::nullopt;
}

} // namespace mitcad
