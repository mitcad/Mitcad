// SPDX-License-Identifier: MIT
#include "ReportDialogs.hpp"

#include <algorithm>
#include <utility>

#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QDir>
#include <QDialogButtonBox>
#include <QFileInfo>
#include <QFontDatabase>
#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QMainWindow>
#include <QMouseEvent>
#include <QPainter>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QScrollArea>
#include <QStyle>
#include <QStyleOptionGroupBox>
#include <QVBoxLayout>
#include <QtLogging>

#include "framework/TestSync.hpp"

#include "../platform/MacDialogs.hpp"

namespace mitcad::report {
namespace {

// A section's text as one log line.
QString oneLine(const QString& text) {
  QString line = text;
  line.replace(QLatin1Char('\n'), QStringLiteral(" | "));
  return line;
}

QPlainTextEdit* textEdit(const QString& text, bool code, QWidget* parent) {
  auto* edit = new QPlainTextEdit(text, parent);
  edit->setTabChangesFocus(true);
  if (code) {
    edit->setFont(QFontDatabase::systemFont(QFontDatabase::FixedFont));
    edit->setLineWrapMode(QPlainTextEdit::NoWrap);
  }
  // Long enough to read, up to a few lines more than it has.
  const int lines = std::clamp(static_cast<int>(text.count(QLatin1Char('\n'))) + 2, 3, code ? 12 : 8);
  edit->setFixedHeight(edit->fontMetrics().lineSpacing() * lines + 12);
  return edit;
}

// Where a checkable group box's check box is.
QRect checkBoxRect(const QGroupBox* box) {
  QStyleOptionGroupBox option;
  option.initFrom(box);
  option.text = box->title();
  option.subControls = QStyle::SC_GroupBoxCheckBox | QStyle::SC_GroupBoxLabel | QStyle::SC_GroupBoxFrame;
  option.features = QStyleOptionFrame::None;
  option.lineWidth = 1;
  return box->style()->subControlRect(QStyle::CC_GroupBox, &option, QStyle::SC_GroupBoxCheckBox, box);
}

} // namespace

void logPlace(const QString& what, const QWidget* widget, const QRect& rect) {
  const QPoint global = widget->mapToGlobal(rect.center());
  QPoint at = global;
  for (QWidget* top : QApplication::topLevelWidgets()) {
    if (qobject_cast<QMainWindow*>(top) != nullptr) {
      at = top->mapFromGlobal(global);
      break;
    }
  }
  qDebug().noquote() << QStringLiteral("%1 at %2,%3").arg(what).arg(at.x()).arg(at.y());
}

// ---------------------------------------------------------------------------
// The screenshot

ScreenshotView::ScreenshotView(QWidget* parent) : QWidget(parent) {
  setObjectName(QStringLiteral("reportScreenshot"));
  setCursor(Qt::CrossCursor);
  setMinimumSize(320, 200);
  setToolTip(tr("Drag over the part of the screenshot to keep"));
}

void ScreenshotView::setImage(const QImage& image) {
  m_image = image;
  m_crop = QRect();
  update();
}

QImage ScreenshotView::image() const { return m_crop.isNull() ? m_image : m_image.copy(m_crop); }

void ScreenshotView::setCropRect(const QRect& crop) {
  const QRect bounded = crop.normalized().intersected(m_image.rect());
  m_crop = bounded.width() >= 8 && bounded.height() >= 8 ? bounded : QRect();
  update();
  emit cropped();
}

void ScreenshotView::reset() {
  m_crop = QRect();
  update();
  emit cropped();
}

QSize ScreenshotView::sizeHint() const { return {480, 300}; }

QRect ScreenshotView::shownRect() const {
  if (m_image.isNull()) {
    return {};
  }
  const QSize size = m_image.size().scaled(this->size(), Qt::KeepAspectRatio);
  return {QPoint((width() - size.width()) / 2, (height() - size.height()) / 2), size};
}

QPoint ScreenshotView::toImage(const QPoint& point) const {
  const QRect shown = shownRect();
  if (shown.isEmpty()) {
    return {};
  }
  const double scale = static_cast<double>(m_image.width()) / shown.width();
  const QPoint inside(std::clamp(point.x() - shown.left(), 0, shown.width()),
                      std::clamp(point.y() - shown.top(), 0, shown.height()));
  return {static_cast<int>(inside.x() * scale), static_cast<int>(inside.y() * scale)};
}

void ScreenshotView::logPlace(const QString& name) const {
  const QRect shown = shownRect();
  report::logPlace(QStringLiteral("%1 image").arg(name), this, shown);
  qDebug().noquote() << QStringLiteral("%1 image size %2 %3").arg(name).arg(shown.width()).arg(shown.height());
}

void ScreenshotView::paintEvent(QPaintEvent*) {
  QPainter painter(this);
  const QRect shown = shownRect();
  if (shown.isEmpty()) {
    return;
  }
  painter.drawImage(shown, m_image);
  const double scale = static_cast<double>(shown.width()) / m_image.width();
  const auto toView = [&](const QRect& rect) {
    return QRect(shown.left() + static_cast<int>(rect.left() * scale), shown.top() + static_cast<int>(rect.top() * scale),
                 static_cast<int>(rect.width() * scale), static_cast<int>(rect.height() * scale));
  };
  QRect keep;
  if (m_dragging) {
    keep = QRect(m_from, m_to).normalized().intersected(shown);
  } else if (!m_crop.isNull()) {
    keep = toView(m_crop);
  }
  if (!keep.isNull()) {
    // What is cut away, dimmed.
    QRegion outside(shown);
    outside -= keep;
    painter.setClipRegion(outside);
    painter.fillRect(shown, QColor(0, 0, 0, 120));
    painter.setClipping(false);
    painter.setPen(QPen(palette().highlight().color(), 2));
    painter.drawRect(keep.adjusted(0, 0, -1, -1));
  }
}

void ScreenshotView::mousePressEvent(QMouseEvent* event) {
  if (event->button() != Qt::LeftButton || m_image.isNull()) {
    return;
  }
  m_dragging = true;
  m_from = m_to = event->position().toPoint();
  update();
}

void ScreenshotView::mouseMoveEvent(QMouseEvent* event) {
  if (m_dragging) {
    m_to = event->position().toPoint();
    update();
  }
}

void ScreenshotView::mouseReleaseEvent(QMouseEvent* event) {
  if (!m_dragging || event->button() != Qt::LeftButton) {
    return;
  }
  m_dragging = false;
  m_to = event->position().toPoint();
  const QRect crop = QRect(toImage(m_from), toImage(m_to)).normalized().intersected(m_image.rect());
  // A click, or a drag of a few pixels, keeps the crop as it was.
  if (crop.width() >= 8 && crop.height() >= 8) {
    m_crop = crop;
    qDebug().noquote() << QStringLiteral("Report screenshot cropped to %1x%2").arg(crop.width()).arg(crop.height());
    emit cropped();
  }
  update();
}

// ---------------------------------------------------------------------------
// Help > Send Feedback

FeedbackDialog::FeedbackDialog(const QImage& screenshot, bool designOpen,
                               std::function<bool(const Feedback&)> preview, QWidget* parent)
    : QDialog(parent), m_preview(std::move(preview)) {
  setObjectName(QStringLiteral("feedbackDialog"));
  setWindowTitle(tr("Send Feedback"));
  auto* layout = new QVBoxLayout(this);
  auto* intro = new QLabel(tr("Report a problem or suggest an improvement. You will see everything the report "
                              "contains before anything is sent."),
                           this);
  intro->setWordWrap(true);
  layout->addWidget(intro);

  auto* form = new QFormLayout;
  m_kind = new QComboBox(this);
  m_kind->addItem(tr("Bug report"), QStringLiteral("bug"));
  m_kind->addItem(tr("Wish"), QStringLiteral("wish"));
  m_kind->addItem(tr("Other"), QStringLiteral("other"));
  form->addRow(tr("&Kind:"), m_kind);
  m_summary = new QLineEdit(this);
  m_summary->setPlaceholderText(tr("A short title"));
  form->addRow(tr("&Summary:"), m_summary);
  m_description = new QPlainTextEdit(this);
  m_description->setTabChangesFocus(true);
  m_description->setPlaceholderText(tr("What happened, what you expected, or what you would like"));
  form->addRow(tr("&Description:"), m_description);
  m_contact = new QLineEdit(this);
  m_contact->setPlaceholderText(tr("Optional; the report is public"));
  m_contact->setToolTip(tr("An e-mail address or name for questions about the report; the report is public"));
  form->addRow(tr("C&ontact:"), m_contact);
  layout->addLayout(form);

  m_diagnostics = new QCheckBox(tr("Include &diagnostics (MitCad's version, the system, the graphics)"), this);
  m_diagnostics->setChecked(true);
  layout->addWidget(m_diagnostics);
  m_screenshot = new QCheckBox(tr("Include a &screenshot of the window"), this);
  m_screenshot->setEnabled(!screenshot.isNull());
  layout->addWidget(m_screenshot);
  m_viewRow = new QWidget(this);
  auto* viewLayout = new QHBoxLayout(m_viewRow);
  viewLayout->setContentsMargins(0, 0, 0, 0);
  m_view = new ScreenshotView(m_viewRow);
  m_view->setImage(screenshot);
  viewLayout->addWidget(m_view, 1);
  auto* reset = new QPushButton(tr("&Whole Window"), m_viewRow);
  reset->setToolTip(tr("Undo the crop"));
  reset->setAutoDefault(false);
  viewLayout->addWidget(reset, 0, Qt::AlignTop);
  connect(reset, &QPushButton::clicked, m_view, &ScreenshotView::reset);
  m_viewRow->setVisible(false);
  layout->addWidget(m_viewRow, 1);
  // Include the design (mitcad#1795): off, offered while a design is open.
  m_design = new QCheckBox(tr("Include the desi&gn (a zip to attach to the issue)"), this);
  m_design->setObjectName(QStringLiteral("feedbackDesign"));
  m_design->setEnabled(designOpen);
  m_design->setToolTip(designOpen ? tr("A zip of the design, the files it refers to, the report and the "
                                       "screenshot, to attach or e-mail")
                                  : tr("Open a design to include it"));
  layout->addWidget(m_design);
  connect(m_design, &QCheckBox::toggled, this, [](bool on) {
    qInfo().noquote() << QStringLiteral("Feedback design %1").arg(on ? QStringLiteral("on") : QStringLiteral("off"));
  });
  connect(m_screenshot, &QCheckBox::toggled, this, [this](bool on) {
    m_viewRow->setVisible(on);
    adjustSize();
    if (on) {
      TestSync::singleShot(100, this, [this] {
        m_view->logPlace(QStringLiteral("Feedback screenshot"));
        qDebug().noquote() << QStringLiteral("Feedback screenshot %1x%2")
                                  .arg(m_view->image().width())
                                  .arg(m_view->image().height());
      });
    }
  });

  auto* buttons = new QDialogButtonBox(this);
  QPushButton* previewButton = buttons->addButton(tr("&Preview…"), QDialogButtonBox::AcceptRole);
  previewButton->setDefault(true);
  previewButton->setToolTip(tr("See and edit everything the report contains before sending it"));
  buttons->addButton(QDialogButtonBox::Cancel);
  // Preview keeps the form open; it closes once the report is sent.
  connect(buttons, &QDialogButtonBox::accepted, this, &FeedbackDialog::preview);
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  layout->addWidget(buttons);
  m_summary->setFocus();
  resize(560, sizeHint().height());
}

Feedback FeedbackDialog::feedback() const {
  Feedback feedback;
  feedback.kind = m_kind->currentData().toString();
  feedback.summary = m_summary->text().trimmed();
  feedback.description = m_description->toPlainText().trimmed();
  feedback.contact = m_contact->text().trimmed();
  feedback.diagnostics = m_diagnostics->isChecked();
  if (m_screenshot->isChecked()) {
    feedback.screenshot = m_view->image();
  }
  feedback.includeDesign = m_design->isEnabled() && m_design->isChecked();
  return feedback;
}

void FeedbackDialog::preview() {
  const Feedback form = feedback();
  qInfo().noquote() << QStringLiteral("Feedback form: kind %1, summary '%2', description %3 characters, contact %4, "
                                      "diagnostics %5, screenshot %6, design %7")
                           .arg(form.kind, form.summary)
                           .arg(form.description.size())
                           .arg(form.contact.isEmpty() ? QStringLiteral("none") : QStringLiteral("given"),
                                form.diagnostics ? QStringLiteral("on") : QStringLiteral("off"),
                                form.screenshot.isNull() ? QStringLiteral("none")
                                                         : QStringLiteral("%1x%2")
                                                               .arg(form.screenshot.width())
                                                               .arg(form.screenshot.height()),
                                form.includeDesign ? QStringLiteral("on") : QStringLiteral("off"));
  if (m_preview(form)) {
    accept();
  }
}

// ---------------------------------------------------------------------------
// The preview

ReportPreviewDialog::ReportPreviewDialog(const Report& report, const QImage& screenshot, const QString& destination,
                                         QWidget* parent, const QString& logFile, const DesignZip& design)
    : QDialog(parent), m_report(report), m_screenshot(screenshot), m_logFile(logFile), m_design(design) {
  setObjectName(QStringLiteral("reportPreview"));
  setWindowTitle(tr("Review Report"));
  auto* layout = new QVBoxLayout(this);
  auto* intro = new QLabel(tr("This is everything the report contains. Edit any part, or uncheck what should "
                              "not be sent. Folders, your user name and the computer's name are masked."),
                           this);
  intro->setWordWrap(true);
  layout->addWidget(intro);
  auto* titleRow = new QFormLayout;
  m_title = new QLineEdit(report.title, this);
  m_title->setObjectName(QStringLiteral("reportTitle"));
  titleRow->addRow(tr("&Title:"), m_title);
  layout->addLayout(titleRow);

  auto* scroll = new QScrollArea(this);
  scroll->setWidgetResizable(true);
  auto* content = new QWidget(scroll);
  auto* sections = new QVBoxLayout(content);
  for (const Section& section : report.sections) {
    auto* box = new QGroupBox(section.title, content);
    box->setObjectName(QStringLiteral("reportSection_") + section.id);
    box->setCheckable(true);
    box->setChecked(section.included);
    mac::styleSection(box);
    auto* boxLayout = new QVBoxLayout(box);
    QPlainTextEdit* text = textEdit(section.text, section.code, box);
    text->setObjectName(QStringLiteral("reportText_") + section.id);
    if (section.id == QStringLiteral("description") && section.text.isEmpty()) {
      text->setPlaceholderText(tr("What were you doing when it happened? (optional)"));
    }
    boxLayout->addWidget(text);
    connect(box, &QGroupBox::toggled, this, [id = section.id](bool on) {
      qInfo().noquote() << QStringLiteral("Report section %1 %2").arg(id, on ? QStringLiteral("included")
                                                                                : QStringLiteral("left out"));
    });
    sections->addWidget(box);
    m_boxes << box;
    m_texts << text;
  }
  if (!screenshot.isNull()) {
    m_screenshotBox = new QGroupBox(tr("Screenshot (attached by you in the issue form)"), content);
    m_screenshotBox->setObjectName(QStringLiteral("reportSection_screenshot"));
    m_screenshotBox->setCheckable(true);
    m_screenshotBox->setChecked(true);
    mac::styleSection(m_screenshotBox);
    auto* boxLayout = new QVBoxLayout(m_screenshotBox);
    auto* image = new QLabel(m_screenshotBox);
    image->setPixmap(QPixmap::fromImage(screenshot.scaled(QSize(400, 250), Qt::KeepAspectRatio,
                                                          Qt::SmoothTransformation)));
    boxLayout->addWidget(image);
    auto* note = new QLabel(tr("An issue form opened from a link cannot take files: Send saves the screenshot "
                               "and tells you where, to drag it into the form."),
                            m_screenshotBox);
    note->setWordWrap(true);
    boxLayout->addWidget(note);
    connect(m_screenshotBox, &QGroupBox::toggled, this, [](bool on) {
      qInfo().noquote() << QStringLiteral("Report section screenshot %1")
                               .arg(on ? QStringLiteral("included") : QStringLiteral("left out"));
    });
    sections->addWidget(m_screenshotBox);
  }
  if (!logFile.isEmpty()) {
    // The debug log (mitcad#269): only when the user checks it.
    m_logFileBox = new QGroupBox(tr("Debug log (attached by you in the issue form)"), content);
    m_logFileBox->setObjectName(QStringLiteral("reportSection_logfile"));
    m_logFileBox->setCheckable(true);
    m_logFileBox->setChecked(false);
    mac::styleSection(m_logFileBox);
    auto* boxLayout = new QVBoxLayout(m_logFileBox);
    auto* note = new QLabel(tr("The latest debug log, %1. It stays on this computer unless you check this: Send "
                               "then tells you where it is, to drag it into the form. A log may contain file "
                               "paths, design names and entity names.")
                                .arg(QDir::toNativeSeparators(logFile).toHtmlEscaped()),
                            m_logFileBox);
    note->setWordWrap(true);
    note->setTextInteractionFlags(Qt::TextSelectableByMouse);
    boxLayout->addWidget(note);
    connect(m_logFileBox, &QGroupBox::toggled, this, [](bool on) {
      qInfo().noquote() << QStringLiteral("Report section logfile %1")
                               .arg(on ? QStringLiteral("included") : QStringLiteral("left out"));
    });
    sections->addWidget(m_logFileBox);
  }
  if (!m_design.path.isEmpty()) {
    // Include the design (mitcad#1795): the zip, made for the preview to
    // show its size; Send makes it again with the report as sent.
    m_designBox = new QGroupBox(tr("Design (a zip you attach to the issue or e-mail)"), content);
    m_designBox->setObjectName(QStringLiteral("reportSection_design"));
    m_designBox->setCheckable(true);
    m_designBox->setChecked(m_design.error.isEmpty());
    m_designBox->setEnabled(m_design.error.isEmpty());
    mac::styleSection(m_designBox);
    auto* boxLayout = new QVBoxLayout(m_designBox);
    QString text;
    if (!m_design.error.isEmpty()) {
      text = tr("Could not make the design's zip: %1").arg(m_design.error.toHtmlEscaped());
    } else {
      text = tr("%1, %2. It holds the open design as it is now, the designs and files it refers to, this report, "
                "the screenshot and the log of this run. Names in it are not masked.")
                 .arg(QFileInfo(m_design.path).fileName().toHtmlEscaped(), designZipSizeText(m_design.size));
      if (m_design.size > attachmentLimit()) {
        text += QLatin1Char(' ') + tr("It is larger than the 25 MB an issue takes: e-mail it to %1 and write "
                                      "its name in the issue.")
                                       .arg(QLatin1String(kTestFileAddress));
      }
    }
    auto* note = new QLabel(text, m_designBox);
    note->setObjectName(QStringLiteral("reportDesignNote"));
    note->setWordWrap(true);
    note->setTextInteractionFlags(Qt::TextSelectableByMouse);
    boxLayout->addWidget(note);
    connect(m_designBox, &QGroupBox::toggled, this, [](bool on) {
      qInfo().noquote() << QStringLiteral("Report section design %1")
                               .arg(on ? QStringLiteral("included") : QStringLiteral("left out"));
    });
    sections->addWidget(m_designBox);
  }
  sections->addStretch(1);
  scroll->setWidget(content);
  layout->addWidget(scroll, 1);

  if (!report.key.isEmpty()) {
    auto* key = new QLabel(tr("Duplicate key: mitcad-%1 (the same error gives the same key, so that reports of it "
                              "can be found together)")
                               .arg(report.key),
                           this);
    key->setWordWrap(true);
    key->setTextInteractionFlags(Qt::TextSelectableByMouse);
    layout->addWidget(key);
  }
  auto* where = new QLabel(m_design.path.isEmpty()
                               ? tr("Send opens the issue form of %1 in your web browser with this report filled "
                                    "in. Nothing is sent until you submit the form there. Issues are public. Your "
                                    "design and its files are never attached.")
                                     .arg(destination.toHtmlEscaped())
                               : tr("Send opens the issue form of %1 in your web browser with this report filled "
                                    "in. Nothing is sent until you submit the form there. Issues are public. Your "
                                    "design goes only into the zip, which you attach or e-mail yourself.")
                                     .arg(destination.toHtmlEscaped()),
                           this);
  where->setWordWrap(true);
  layout->addWidget(where);
  auto* buttons = new QDialogButtonBox(this);
  QPushButton* send = buttons->addButton(tr("&Send"), QDialogButtonBox::AcceptRole);
  send->setDefault(true);
  buttons->addButton(QDialogButtonBox::Cancel);
  connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  layout->addWidget(buttons);
  resize(760, 700);
  mac::styleDialogButtons(buttons, send, true);
  mac::styleDialog(this);
}

Report ReportPreviewDialog::report() const {
  Report edited = m_report;
  edited.title = m_title->text().trimmed();
  for (qsizetype i = 0; i < edited.sections.size(); ++i) {
    edited.sections[i].included = m_boxes[i]->isChecked();
    edited.sections[i].text = m_texts[i]->toPlainText();
  }
  return edited;
}

QImage ReportPreviewDialog::screenshot() const {
  return m_screenshotBox != nullptr && m_screenshotBox->isChecked() ? m_screenshot : QImage();
}

QString ReportPreviewDialog::logFile() const {
  return m_logFileBox != nullptr && m_logFileBox->isChecked() ? m_logFile : QString();
}

bool ReportPreviewDialog::designIncluded() const {
  return m_designBox != nullptr && m_design.error.isEmpty() && m_designBox->isChecked();
}

void ReportPreviewDialog::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  TestSync::singleShot(100, this, [this] { logContent(); });
}

void ReportPreviewDialog::logContent() const {
  qInfo().noquote() << QStringLiteral("Report preview: %1").arg(m_title->text());
  for (qsizetype i = 0; i < m_boxes.size(); ++i) {
    const QString id = m_report.sections[i].id;
    // The recent log by its length: its lines are in the log already.
    const QString text = m_texts[i]->toPlainText();
    qInfo().noquote() << QStringLiteral("Report section %1: %2")
                             .arg(id, id == QStringLiteral("log")
                                          ? QStringLiteral("%1 lines").arg(text.count(QLatin1Char('\n')) + 1)
                                          : oneLine(text));
    logPlace(QStringLiteral("Report section %1").arg(id), m_boxes[i], checkBoxRect(m_boxes[i]));
    logPlace(QStringLiteral("Report text %1").arg(id), m_texts[i], m_texts[i]->rect());
  }
  if (m_screenshotBox != nullptr) {
    qInfo().noquote() << QStringLiteral("Report section screenshot: %1x%2")
                             .arg(m_screenshot.width())
                             .arg(m_screenshot.height());
    logPlace(QStringLiteral("Report section screenshot"), m_screenshotBox, checkBoxRect(m_screenshotBox));
  }
  if (m_logFileBox != nullptr) {
    qInfo().noquote() << QStringLiteral("Report section logfile: %1").arg(m_logFile);
    logPlace(QStringLiteral("Report section logfile"), m_logFileBox, checkBoxRect(m_logFileBox));
  }
  if (m_designBox != nullptr) {
    if (m_design.error.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Report section design: %1, %2 bytes%3")
                               .arg(QFileInfo(m_design.path).fileName())
                               .arg(m_design.size)
                               .arg(m_design.size > attachmentLimit() ? QStringLiteral(", over the attachment limit")
                                                                     : QString());
    } else {
      qInfo().noquote() << QStringLiteral("Report section design: not made: %1").arg(m_design.error);
    }
    logPlace(QStringLiteral("Report section design"), m_designBox, checkBoxRect(m_designBox));
  }
  if (!m_report.key.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Report key: mitcad-%1").arg(m_report.key);
  }
}

} // namespace mitcad::report
