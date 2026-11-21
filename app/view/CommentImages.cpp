// SPDX-License-Identifier: MIT
#include "CommentImages.hpp"

#include <algorithm>
#include <cmath>
#include <functional>
#include <utility>

#include <QBuffer>
#include <QButtonGroup>
#include <QCoreApplication>
#include <QDialog>
#include <QEvent>
#include <QFile>
#include <QFileInfo>
#include <QHBoxLayout>
#include <QIcon>
#include <QJsonObject>
#include <QKeyEvent>
#include <QKeySequence>
#include <QLabel>
#include <QLineF>
#include <QLocale>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QPixmap>
#include <QPointer>
#include <QPushButton>
#include <QShortcut>
#include <QVBoxLayout>
#include <QWheelEvent>
#include <QWidget>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/Icons.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "CommentMarkup.hpp"

namespace mitcad {
namespace {

// A drag shorter than this (pixels on the screen) is a click.
constexpr double kClickSlop = 3.0;
// How near a handle or a line a press takes it (pixels on the screen).
constexpr double kPickTolerance = 6.0;
// A capture's smallest side (pixels of the view).
constexpr int kSmallestCapture = 8;
// The largest zoom of the editor's picture and of the viewer.
constexpr double kLargestZoom = 4.0;
constexpr double kViewerLargestZoom = 16.0;

bool isPng(const QByteArray& bytes) { return bytes.startsWith("\x89PNG\r\n\x1a\n"); }
bool isJpeg(const QByteArray& bytes) { return bytes.startsWith("\xff\xd8\xff"); }

QByteArray pngBytes(const QImage& image) {
  QByteArray bytes;
  QBuffer buffer(&bytes);
  buffer.open(QIODevice::WriteOnly);
  image.save(&buffer, "PNG");
  return bytes;
}

// "x,y" of a widget's point in the coordinates of `origin`'s window, as
// the other logged places.
QString placeIn(const QWidget* origin, const QWidget* widget, const QPoint& point) {
  const QPoint base = origin != nullptr ? origin->window()->mapToGlobal(QPoint(0, 0)) : QPoint();
  const QPoint at = widget->mapToGlobal(point) - base;
  return QStringLiteral("%1,%2").arg(at.x()).arg(at.y());
}

// The editor's picture: in the capture stage the view's whole picture, a
// rectangle dragged over it the capture; in the markup stage the capture
// and the shapes drawn over it.
class MarkupCanvas : public QWidget {
  Q_DECLARE_TR_FUNCTIONS(mitcad::CommentImages)

public:
  enum class Stage { Capture, Markup };

  explicit MarkupCanvas(QWidget* parent) : QWidget(parent) {
    setObjectName(QStringLiteral("markupCanvas"));
    setAccessibleName(tr("Picture"));
    setFocusPolicy(Qt::StrongFocus);
    setMouseTracking(true);
    setCursor(Qt::CrossCursor);
  }

  // What the canvas tells its dialog.
  std::function<void(const QRect& pixels)> captured;
  std::function<void()> shapesChanged; // a change done: for undo
  std::function<void()> selectionChanged;
  std::function<QString()> askText; // the Text tool's words

  Stage stage = Stage::Capture;
  MarkupTool tool = MarkupTool::Line;
  QColor ink;
  double lineWidth = 4.0;
  QVector<MarkupShape> shapes;
  int selected = -1;

  void showFrame(const QImage& frame) {
    stage = Stage::Capture;
    m_image = frame;
    m_image.setDevicePixelRatio(1.0);
    update();
  }

  void showPicture(const QImage& picture) {
    stage = Stage::Markup;
    m_image = picture;
    m_image.setDevicePixelRatio(1.0);
    shapes.clear();
    selected = -1;
    m_drafting = false;
    update();
  }

  const QImage& image() const { return m_image; }

  // Where the image is drawn, in the canvas.
  QRectF shownRect() const {
    if (m_image.isNull() || width() <= 0 || height() <= 0) {
      return QRectF();
    }
    const double s = scale();
    const QSizeF size(m_image.width() * s, m_image.height() * s);
    return QRectF(QPointF((width() - size.width()) / 2.0, (height() - size.height()) / 2.0), size);
  }

  // Ends a polyline being drawn: kept with two points or more.
  void finishDraft() {
    if (!m_drafting || m_draft.kind != MarkupTool::Polyline) {
      return;
    }
    m_drafting = false;
    QVector<QPointF>& points = m_draft.points;
    // The point that followed the pointer, and a double-click's second
    // point at the same place.
    if (!points.isEmpty()) {
      points.removeLast();
    }
    while (points.size() >= 2 && points.at(points.size() - 1) == points.at(points.size() - 2)) {
      points.removeLast();
    }
    if (points.size() >= 2) {
      shapes.append(m_draft);
      emitChanged();
    }
    update();
  }

  void cancelDraft() {
    m_drafting = false;
    update();
  }

  bool drafting() const { return m_drafting; }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.fillRect(rect(), themeColor(ColorRole::Window, palette()));
    const QRectF shown = shownRect();
    if (m_image.isNull() || shown.isEmpty()) {
      return;
    }
    painter.setRenderHint(QPainter::SmoothPixmapTransform, true);
    painter.drawImage(shown, m_image);
    const QColor select = themeColor(ColorRole::Select, palette());
    if (stage == Stage::Capture) {
      if (m_pressed) {
        const QRectF chosen = QRectF(m_from, m_to).normalized().intersected(shown);
        QPainterPath outside;
        outside.addRect(shown);
        QPainterPath inside;
        inside.addRect(chosen);
        painter.fillPath(outside.subtracted(inside), QColor(0, 0, 0, 110));
        painter.setPen(QPen(select, 2.0));
        painter.setBrush(Qt::NoBrush);
        painter.drawRect(chosen);
      }
      return;
    }
    painter.save();
    painter.translate(shown.topLeft());
    painter.scale(scale(), scale());
    for (const MarkupShape& shape : shapes) {
      paintMarkupShape(painter, shape);
    }
    if (m_drafting) {
      paintMarkupShape(painter, m_draft);
    }
    painter.restore();
    // The selected shape: its box and its handles.
    if (selected >= 0 && selected < shapes.size()) {
      const MarkupShape& shape = shapes.at(selected);
      painter.setRenderHint(QPainter::Antialiasing, true);
      QPen dashed(select, 1.0, Qt::DashLine);
      painter.setPen(dashed);
      painter.setBrush(Qt::NoBrush);
      const QRectF bounds = markupBounds(shape);
      painter.drawRect(QRectF(toWidget(bounds.topLeft()), toWidget(bounds.bottomRight())));
      painter.setPen(QPen(select, 1.5));
      painter.setBrush(palette().color(QPalette::Base));
      for (const QPointF& handle : markupHandles(shape)) {
        const QPointF at = toWidget(handle);
        painter.drawRect(QRectF(at - QPointF(4.0, 4.0), QSizeF(8.0, 8.0)));
      }
    }
  }

  void mousePressEvent(QMouseEvent* event) override {
    setFocus();
    const QPointF at = event->position();
    if (stage == Stage::Capture) {
      if (event->button() == Qt::LeftButton) {
        m_pressed = true;
        m_from = at;
        m_to = at;
        update();
      }
      return;
    }
    const QPointF point = toImage(at);
    if (tool == MarkupTool::Polyline) {
      if (event->button() == Qt::RightButton) {
        finishDraft();
        return;
      }
      if (event->button() != Qt::LeftButton) {
        return;
      }
      if (!m_drafting) {
        m_draft = MarkupShape{MarkupTool::Polyline, {point, point}, ink, lineWidth, QString()};
        m_drafting = true;
      } else {
        m_draft.points.last() = point;
        m_draft.points.append(point);
      }
      update();
      return;
    }
    if (event->button() != Qt::LeftButton) {
      return;
    }
    m_pressed = true;
    m_from = at;
    m_to = at;
    if (tool == MarkupTool::Select) {
      m_handle = -1;
      m_moving = -1;
      const double tolerance = kPickTolerance / scale();
      if (selected >= 0 && selected < shapes.size()) {
        m_handle = markupHandleAt(shapes.at(selected), point, tolerance);
      }
      if (m_handle < 0) {
        const int hit = markupShapeAt(shapes, point, tolerance);
        setSelected(hit);
      }
      if (selected >= 0) {
        m_moving = selected;
        m_original = shapes.at(selected);
      }
      return;
    }
    if (tool == MarkupTool::Text) {
      m_pressed = false;
      const QString words = askText ? askText().trimmed() : QString();
      if (!words.isEmpty()) {
        shapes.append(MarkupShape{MarkupTool::Text, {point}, ink, lineWidth, words});
        emitChanged();
        update();
      }
      return;
    }
    m_drafting = true;
    if (tool == MarkupTool::Freehand) {
      m_draft = MarkupShape{MarkupTool::Freehand, {point}, ink, lineWidth, QString()};
    } else {
      m_draft = markupFromDrag(tool, point, point, ink, lineWidth);
    }
    update();
  }

  void mouseMoveEvent(QMouseEvent* event) override {
    const QPointF at = event->position();
    if (stage == Stage::Capture) {
      if (m_pressed) {
        m_to = at;
        update();
      }
      return;
    }
    const QPointF point = toImage(at);
    if (tool == MarkupTool::Polyline) {
      if (m_drafting) {
        m_draft.points.last() = point;
        update();
      }
      return;
    }
    if (!m_pressed) {
      return;
    }
    m_to = at;
    if (tool == MarkupTool::Select) {
      if (m_moving >= 0 && m_moving < shapes.size()) {
        shapes[m_moving] = m_handle >= 0 ? markupWithHandle(m_original, m_handle, point)
                                         : markupMoved(m_original, point - toImage(m_from));
        update();
      }
      return;
    }
    if (!m_drafting) {
      return;
    }
    if (tool == MarkupTool::Freehand) {
      if (m_draft.points.isEmpty() || QLineF(m_draft.points.last(), point).length() * scale() >= 1.0) {
        m_draft.points.append(point);
      }
    } else {
      m_draft = markupFromDrag(tool, toImage(m_from), point, ink, lineWidth);
    }
    update();
  }

  void mouseReleaseEvent(QMouseEvent* event) override {
    if (event->button() != Qt::LeftButton || !m_pressed) {
      return;
    }
    m_pressed = false;
    m_to = event->position();
    const bool dragged = QLineF(m_from, m_to).length() >= kClickSlop;
    if (stage == Stage::Capture) {
      update();
      const QRectF chosen = QRectF(m_from, m_to).normalized().intersected(shownRect());
      const QRect pixels = QRectF(toImage(chosen.topLeft()), toImage(chosen.bottomRight()))
                               .toAlignedRect()
                               .intersected(m_image.rect());
      if (dragged && pixels.width() >= kSmallestCapture && pixels.height() >= kSmallestCapture && captured) {
        captured(pixels);
      }
      return;
    }
    if (tool == MarkupTool::Select) {
      if (m_moving >= 0 && m_moving < shapes.size() && !(shapes.at(m_moving) == m_original)) {
        emitChanged();
      }
      m_moving = -1;
      m_handle = -1;
      update();
      return;
    }
    if (!m_drafting) {
      return;
    }
    m_drafting = false;
    if (tool == MarkupTool::Freehand ? m_draft.points.size() >= 2 : dragged) {
      if (tool != MarkupTool::Freehand) {
        m_draft = markupFromDrag(tool, toImage(m_from), toImage(m_to), ink, lineWidth);
      }
      shapes.append(m_draft);
      emitChanged();
    }
    update();
  }

  void mouseDoubleClickEvent(QMouseEvent* event) override {
    if (stage == Stage::Markup && tool == MarkupTool::Polyline && m_drafting) {
      finishDraft();
      return;
    }
    QWidget::mouseDoubleClickEvent(event);
  }

  void keyPressEvent(QKeyEvent* event) override {
    if (stage == Stage::Markup) {
      if (m_drafting && event->key() == Qt::Key_Escape) {
        // The shape being drawn, not the window.
        cancelDraft();
        event->accept();
        return;
      }
      if (m_drafting && (event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter)) {
        finishDraft();
        event->accept();
        return;
      }
    }
    QWidget::keyPressEvent(event);
  }

  void resizeEvent(QResizeEvent* event) override {
    QWidget::resizeEvent(event);
    update();
  }

private:
  // Screen pixels per image pixel: the whole view fits; a capture fits,
  // enlarged up to four times.
  double scale() const {
    if (m_image.isNull()) {
      return 1.0;
    }
    const double fit =
        std::min(width() / static_cast<double>(m_image.width()), height() / static_cast<double>(m_image.height()));
    return std::max(0.01, std::min(fit, stage == Stage::Capture ? 1.0 : kLargestZoom));
  }

  QPointF toImage(const QPointF& at) const { return (at - shownRect().topLeft()) / scale(); }
  QPointF toWidget(const QPointF& point) const { return shownRect().topLeft() + point * scale(); }

  void setSelected(int index) {
    if (index != selected) {
      selected = index;
      if (selectionChanged) {
        selectionChanged();
      }
      update();
    }
  }

  void emitChanged() {
    if (shapesChanged) {
      shapesChanged();
    }
  }

  QImage m_image;
  bool m_pressed = false;
  QPointF m_from;
  QPointF m_to;
  bool m_drafting = false;
  MarkupShape m_draft;
  int m_moving = -1;
  int m_handle = -1;
  MarkupShape m_original;
};

// Capture and Annotate's window, over the view.
class CaptureAnnotateDialog : public QDialog {
  Q_DECLARE_TR_FUNCTIONS(mitcad::CommentImages)

public:
  CaptureAnnotateDialog(QWidget* parent, QWidget* view, const QImage& frame)
      : QDialog(parent, Qt::Dialog | Qt::FramelessWindowHint), m_view(view) {
    setWindowTitle(tr("Capture and Annotate"));
    setObjectName(QStringLiteral("captureAnnotateDialog"));
    setWindowModality(Qt::ApplicationModal);
    auto* layout = new QVBoxLayout(this);
    layout->setContentsMargins(6, 6, 6, 6);
    layout->setSpacing(6);

    // The capture stage's bar: what to do, the whole view, Cancel.
    m_captureBar = new QWidget(this);
    auto* captureRow = new QHBoxLayout(m_captureBar);
    captureRow->setContentsMargins(0, 0, 0, 0);
    auto* hint = new QLabel(tr("Drag a rectangle over the part of the view to capture."), m_captureBar);
    hint->setObjectName(QStringLiteral("markupHint"));
    captureRow->addWidget(hint);
    captureRow->addStretch(1);
    m_whole = addButton(captureRow, m_captureBar, tr("Whole View"), QStringLiteral("markupWholeView"),
                        tr("Captures the whole view"));
    auto* cancelCapture = addButton(captureRow, m_captureBar, tr("Cancel"), QStringLiteral("markupCancelCapture"),
                                    QString());
    layout->addWidget(m_captureBar);

    // The markup stage's bar: the tools, the inks, the widths, undo, redo,
    // delete, Attach and Cancel.
    m_markupBar = new QWidget(this);
    auto* markupRow = new QHBoxLayout(m_markupBar);
    markupRow->setContentsMargins(0, 0, 0, 0);
    markupRow->setSpacing(4);
    auto* tools = new QButtonGroup(this);
    tools->setExclusive(true);
    for (MarkupTool tool : markupTools()) {
      QPushButton* button = addButton(markupRow, m_markupBar, markupToolLabel(tool),
                                      QStringLiteral("markupTool_%1").arg(markupToolKey(tool)), QString());
      button->setCheckable(true);
      tools->addButton(button);
      m_tools.append({tool, button});
      connect(button, &QPushButton::clicked, this, [this, tool] { chooseTool(tool); });
    }
    markupRow->addSpacing(8);
    auto* inks = new QButtonGroup(this);
    const QVector<QColor> colors = markupInks();
    for (int i = 0; i < colors.size(); ++i) {
      QPixmap swatch(16, 16);
      swatch.fill(colors.at(i));
      {
        QPainter painter(&swatch);
        painter.setPen(themeColor(ColorRole::Line, palette()));
        painter.drawRect(0, 0, 15, 15);
      }
      auto* button = new QPushButton(QIcon(swatch), QString(), m_markupBar);
      button->setObjectName(QStringLiteral("markupInk_%1").arg(i));
      button->setToolTip(markupInkName(i));
      button->setAccessibleName(tr("Ink %1").arg(markupInkName(i)));
      button->setCheckable(true);
      button->setAutoDefault(false);
      button->setChecked(i == 0);
      inks->addButton(button);
      markupRow->addWidget(button);
      m_inks.append(button);
      connect(button, &QPushButton::clicked, this, [this, i] { chooseInk(i); });
    }
    markupRow->addSpacing(8);
    auto* widths = new QButtonGroup(this);
    const QStringList widthNames{tr("Thin"), tr("Medium"), tr("Thick")};
    const QVector<double> widthValues = markupWidths();
    for (int i = 0; i < widthValues.size() && i < widthNames.size(); ++i) {
      QPushButton* button = addButton(markupRow, m_markupBar, widthNames.at(i),
                                      QStringLiteral("markupWidth_%1").arg(i), tr("Line width for new shapes and the selected one"));
      button->setCheckable(true);
      button->setChecked(i == 1);
      widths->addButton(button);
      m_widths.append(button);
      connect(button, &QPushButton::clicked, this, [this, i] { chooseWidth(i); });
    }
    markupRow->addSpacing(8);
    m_undo = addButton(markupRow, m_markupBar, QString(), QStringLiteral("markupUndo"), tr("Undo"));
    m_undo->setIcon(themeIcon(QStringLiteral("undo")));
    m_redo = addButton(markupRow, m_markupBar, QString(), QStringLiteral("markupRedo"), tr("Redo"));
    m_redo->setIcon(themeIcon(QStringLiteral("redo")));
    m_delete = addButton(markupRow, m_markupBar, QString(), QStringLiteral("markupDelete"), tr("Delete the selected shape"));
    m_delete->setIcon(themeIcon(QStringLiteral("delete")));
    markupRow->addStretch(1);
    m_attach = addButton(markupRow, m_markupBar, tr("Attach"), QStringLiteral("markupAttach"),
                         tr("Attaches the picture with its shapes"));
    auto* cancelMarkup = addButton(markupRow, m_markupBar, tr("Cancel"), QStringLiteral("markupCancel"), QString());
    layout->addWidget(m_markupBar);
    m_markupBar->hide();

    m_canvas = new MarkupCanvas(this);
    m_canvas->ink = colors.value(0);
    m_canvas->lineWidth = widthValues.value(1, 4.0);
    layout->addWidget(m_canvas, 1);
    m_canvas->captured = [this](const QRect& pixels) { capture(pixels); };
    m_canvas->shapesChanged = [this] { shapesChanged(); };
    m_canvas->selectionChanged = [this] { selectionChanged(); };
    m_canvas->askText = [this] {
      bool accepted = false;
      const QString words = dialogText(this, tr("Text"), tr("Text:"), QString(), &accepted);
      return accepted ? words : QString();
    };

    connect(m_whole, &QPushButton::clicked, this, [this] { capture(m_canvas->image().rect()); });
    connect(cancelCapture, &QPushButton::clicked, this, &QDialog::reject);
    connect(cancelMarkup, &QPushButton::clicked, this, &QDialog::reject);
    connect(m_undo, &QPushButton::clicked, this, [this] { undo(); });
    connect(m_redo, &QPushButton::clicked, this, [this] { redo(); });
    connect(m_delete, &QPushButton::clicked, this, [this] { deleteSelected(); });
    connect(m_attach, &QPushButton::clicked, this, [this] { attach(); });
    auto* undoKey = new QShortcut(QKeySequence::Undo, this);
    connect(undoKey, &QShortcut::activated, this, [this] { undo(); });
    auto* redoKey = new QShortcut(QKeySequence::Redo, this);
    connect(redoKey, &QShortcut::activated, this, [this] { redo(); });
    for (const QKeySequence& key : {QKeySequence(QKeySequence::Delete), QKeySequence(Qt::Key_Backspace)}) {
      auto* deleteKey = new QShortcut(key, this);
      connect(deleteKey, &QShortcut::activated, this, [this] { deleteSelected(); });
    }

    m_canvas->showFrame(frame);
    // Over the view, as large as it.
    if (m_view != nullptr) {
      setGeometry(QRect(m_view->mapToGlobal(QPoint(0, 0)), m_view->size()));
    } else {
      resize(960, 640);
    }
    qDebug().noquote() << QStringLiteral("Markup stage: capture");
    chooseTool(MarkupTool::Line);
    updateButtons();
  }

  std::optional<CommentImageFile> result() const { return m_result; }

  void reject() override {
    // Esc first ends a polyline being drawn.
    if (m_canvas->drafting()) {
      m_canvas->cancelDraft();
      return;
    }
    qDebug().noquote() << QStringLiteral("Markup cancelled");
    QDialog::reject();
  }

protected:
  void showEvent(QShowEvent* event) override {
    QDialog::showEvent(event);
    m_canvas->setFocus();
    TestSync::singleShot(0, this, [this] { logPlaces(); });
  }

  void resizeEvent(QResizeEvent* event) override {
    QDialog::resizeEvent(event);
    TestSync::singleShot(0, this, [this] { logPlaces(); });
  }

private:
  QPushButton* addButton(QHBoxLayout* row, QWidget* parent, const QString& text, const QString& name,
                         const QString& tip) {
    auto* button = new QPushButton(text, parent);
    button->setObjectName(name);
    button->setAutoDefault(false);
    if (!tip.isEmpty()) {
      button->setToolTip(tip);
      if (text.isEmpty()) {
        button->setAccessibleName(tip);
      }
    }
    row->addWidget(button);
    return button;
  }

  void capture(const QRect& pixels) {
    const QImage picture = m_canvas->image().copy(pixels);
    if (picture.isNull()) {
      return;
    }
    m_history = MarkupHistory();
    m_canvas->showPicture(picture);
    m_captureBar->hide();
    m_markupBar->show();
    qDebug().noquote() << QStringLiteral("Markup stage: markup %1 x %2").arg(picture.width()).arg(picture.height());
    updateButtons();
    TestSync::singleShot(0, this, [this] { logPlaces(); });
  }

  void chooseTool(MarkupTool tool) {
    m_canvas->finishDraft();
    m_canvas->tool = tool;
    for (const auto& [kind, button] : std::as_const(m_tools)) {
      button->setChecked(kind == tool);
    }
    m_canvas->setCursor(tool == MarkupTool::Select ? Qt::ArrowCursor : Qt::CrossCursor);
    qDebug().noquote() << QStringLiteral("Markup tool: %1").arg(markupToolKey(tool));
  }

  void chooseInk(int index) {
    const QVector<QColor> colors = markupInks();
    if (index < 0 || index >= colors.size()) {
      return;
    }
    m_canvas->ink = colors.at(index);
    qDebug().noquote() << QStringLiteral("Markup ink: %1").arg(colors.at(index).name());
    restyleSelected([color = colors.at(index)](MarkupShape& shape) { shape.color = color; });
  }

  void chooseWidth(int index) {
    const QVector<double> widths = markupWidths();
    if (index < 0 || index >= widths.size()) {
      return;
    }
    m_canvas->lineWidth = widths.at(index);
    qDebug().noquote() << QStringLiteral("Markup width: %1").arg(widths.at(index));
    restyleSelected([width = widths.at(index)](MarkupShape& shape) { shape.width = width; });
  }

  // The selected shape takes the ink or the width chosen.
  void restyleSelected(const std::function<void(MarkupShape&)>& change) {
    const int selected = m_canvas->selected;
    if (selected < 0 || selected >= m_canvas->shapes.size()) {
      return;
    }
    MarkupShape shape = m_canvas->shapes.at(selected);
    change(shape);
    if (!(shape == m_canvas->shapes.at(selected))) {
      m_canvas->shapes[selected] = shape;
      m_canvas->update();
      shapesChanged();
    }
  }

  void shapesChanged() {
    m_history.push(m_canvas->shapes);
    logShapes();
    updateButtons();
  }

  void selectionChanged() {
    const int selected = m_canvas->selected;
    qDebug().noquote() << QStringLiteral("Markup selected: %1")
                              .arg(selected >= 0 && selected < m_canvas->shapes.size()
                                       ? QStringLiteral("%1 %2").arg(selected + 1).arg(
                                             markupToolKey(m_canvas->shapes.at(selected).kind))
                                       : QStringLiteral("none"));
    updateButtons();
  }

  void undo() {
    m_canvas->cancelDraft();
    if (m_history.undo()) {
      restore();
      qDebug().noquote() << QStringLiteral("Markup undo");
    }
  }

  void redo() {
    if (m_history.redo()) {
      restore();
      qDebug().noquote() << QStringLiteral("Markup redo");
    }
  }

  void restore() {
    m_canvas->shapes = m_history.shapes();
    if (m_canvas->selected >= m_canvas->shapes.size()) {
      m_canvas->selected = -1;
      selectionChanged();
    }
    m_canvas->update();
    logShapes();
    updateButtons();
  }

  void deleteSelected() {
    const int selected = m_canvas->selected;
    if (m_canvas->stage != MarkupCanvas::Stage::Markup || selected < 0 || selected >= m_canvas->shapes.size()) {
      return;
    }
    m_canvas->shapes.removeAt(selected);
    m_canvas->selected = -1;
    m_canvas->update();
    selectionChanged();
    shapesChanged();
  }

  void attach() {
    m_canvas->finishDraft();
    QImage flat = flattenMarkup(m_canvas->image(), m_canvas->shapes);
    QByteArray bytes = pngBytes(flat);
    // Too large for a comment: smaller until it fits.
    for (int i = 0; i < 12 && bytes.size() > kMaxCommentImageBytes && flat.width() > 16; ++i) {
      flat = flat.scaled(flat.size() * 0.8, Qt::KeepAspectRatio, Qt::SmoothTransformation);
      bytes = pngBytes(flat);
    }
    if (bytes.isEmpty() || bytes.size() > kMaxCommentImageBytes) {
      sheetWarning(this, windowTitle(), tr("Could not attach the picture: it stays larger than 4 MB."));
      return;
    }
    m_result = CommentImageFile{bytes, QStringLiteral("capture.png"), flat.width(), flat.height()};
    qDebug().noquote() << QStringLiteral("Markup attached: capture.png %1 x %2, %3 shapes: %4")
                              .arg(flat.width())
                              .arg(flat.height())
                              .arg(m_canvas->shapes.size())
                              .arg(markupSummary(m_canvas->shapes));
    accept();
  }

  void updateButtons() {
    m_undo->setEnabled(m_history.canUndo());
    m_redo->setEnabled(m_history.canRedo());
    m_delete->setEnabled(m_canvas->selected >= 0);
    TestSync::singleShot(0, this, [this] { logPlaces(); });
  }

  void logShapes() { qDebug().noquote() << QStringLiteral("Markup shapes: %1").arg(markupSummary(m_canvas->shapes)); }

  void logPlaces() {
    if (!isVisible()) {
      return;
    }
    QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
    const QWidget* origin = m_view != nullptr ? m_view.data() : this;
    QStringList lines;
    const QRectF shown = m_canvas->shownRect();
    lines << QStringLiteral("Markup picture at %1 size %2x%3")
                 .arg(placeIn(origin, m_canvas, shown.center().toPoint()))
                 .arg(qRound(shown.width()))
                 .arg(qRound(shown.height()));
    const auto place = [&](const QString& name, const QPushButton* button) {
      if (button->isVisibleTo(this) && button->isVisible()) {
        lines << QStringLiteral("Markup %1 at %2%3")
                     .arg(name, placeIn(origin, button, button->rect().center()),
                          button->isEnabled() ? QString() : QStringLiteral(" (disabled)"));
      }
    };
    place(QStringLiteral("Whole View"), m_whole);
    for (const auto& [tool, button] : std::as_const(m_tools)) {
      place(QStringLiteral("tool %1").arg(markupToolKey(tool)), button);
    }
    const QStringList inkKeys{QStringLiteral("red"),  QStringLiteral("yellow"), QStringLiteral("green"),
                              QStringLiteral("blue"), QStringLiteral("black"),  QStringLiteral("white")};
    for (int i = 0; i < m_inks.size() && i < inkKeys.size(); ++i) {
      place(QStringLiteral("ink %1").arg(inkKeys.at(i)), m_inks.at(i));
    }
    const QStringList widthKeys{QStringLiteral("thin"), QStringLiteral("medium"), QStringLiteral("thick")};
    for (int i = 0; i < m_widths.size() && i < widthKeys.size(); ++i) {
      place(QStringLiteral("width %1").arg(widthKeys.at(i)), m_widths.at(i));
    }
    place(QStringLiteral("Undo"), m_undo);
    place(QStringLiteral("Redo"), m_redo);
    place(QStringLiteral("Delete"), m_delete);
    place(QStringLiteral("Attach"), m_attach);
    const QString logged = lines.join(QLatin1Char('\n'));
    if (logged != m_loggedPlaces) {
      m_loggedPlaces = logged;
      for (const QString& line : std::as_const(lines)) {
        qDebug().noquote() << line;
      }
    }
  }

  QPointer<QWidget> m_view;
  MarkupCanvas* m_canvas = nullptr;
  QWidget* m_captureBar = nullptr;
  QWidget* m_markupBar = nullptr;
  QPushButton* m_whole = nullptr;
  QVector<std::pair<MarkupTool, QPushButton*>> m_tools;
  QVector<QPushButton*> m_inks;
  QVector<QPushButton*> m_widths;
  QPushButton* m_undo = nullptr;
  QPushButton* m_redo = nullptr;
  QPushButton* m_delete = nullptr;
  QPushButton* m_attach = nullptr;
  MarkupHistory m_history;
  std::optional<CommentImageFile> m_result;
  QString m_loggedPlaces;
};

// The viewer's picture: fit at first, zoomed by the wheel, panned by a
// drag.
class ImageView : public QWidget {
public:
  explicit ImageView(QWidget* parent) : QWidget(parent) {
    setObjectName(QStringLiteral("commentImageView"));
    setMouseTracking(false);
    setCursor(Qt::OpenHandCursor);
  }

  std::function<void()> zoomed;

  void setImage(const QImage& image) {
    m_image = image;
    m_image.setDevicePixelRatio(1.0);
    fit();
  }

  // Zoom 0: fit.
  void fit() {
    m_zoom = 0.0;
    m_pan = QPointF();
    update();
    if (zoomed) zoomed();
  }

  void zoomBy(double factor) {
    m_zoom = std::clamp(scale() * factor, 0.05, kViewerLargestZoom);
    update();
    if (zoomed) zoomed();
  }

  bool fitted() const { return m_zoom == 0.0; }

  double scale() const {
    if (m_zoom > 0.0) {
      return m_zoom;
    }
    if (m_image.isNull() || width() <= 0 || height() <= 0) {
      return 1.0;
    }
    return std::min(1.0, std::min(width() / static_cast<double>(m_image.width()),
                                  height() / static_cast<double>(m_image.height())));
  }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.fillRect(rect(), themeColor(ColorRole::Window, palette()));
    if (m_image.isNull()) {
      return;
    }
    const double s = scale();
    const QSizeF size(m_image.width() * s, m_image.height() * s);
    const QPointF topLeft = QPointF((width() - size.width()) / 2.0, (height() - size.height()) / 2.0) + m_pan;
    painter.setRenderHint(QPainter::SmoothPixmapTransform, s < 1.0);
    painter.drawImage(QRectF(topLeft, size), m_image);
  }

  void wheelEvent(QWheelEvent* event) override {
    const int steps = event->angleDelta().y() / 120;
    if (steps != 0) {
      zoomBy(std::pow(1.25, steps));
    }
    event->accept();
  }

  void mousePressEvent(QMouseEvent* event) override {
    if (event->button() == Qt::LeftButton) {
      m_dragging = true;
      m_last = event->position();
      setCursor(Qt::ClosedHandCursor);
    }
  }

  void mouseMoveEvent(QMouseEvent* event) override {
    if (m_dragging) {
      m_pan += event->position() - m_last;
      m_last = event->position();
      update();
    }
  }

  void mouseReleaseEvent(QMouseEvent*) override {
    m_dragging = false;
    setCursor(Qt::OpenHandCursor);
  }

  void resizeEvent(QResizeEvent* event) override {
    QWidget::resizeEvent(event);
    if (fitted() && zoomed) {
      zoomed();
    }
  }

private:
  QImage m_image;
  double m_zoom = 0.0;
  QPointF m_pan;
  bool m_dragging = false;
  QPointF m_last;
};

} // namespace

QString commentImageProblem(const QByteArray& bytes, CommentImageFile* image) {
  if (bytes.size() > kMaxCommentImageBytes) {
    return QCoreApplication::translate("mitcad::CommentImages",
                                       "the image is %1 MB, more than the 4 MB a comment's image may be")
        .arg(QLocale().toString(static_cast<double>(bytes.size()) / (1 << 20), 'f', 1));
  }
  if (!isPng(bytes) && !isJpeg(bytes)) {
    return QCoreApplication::translate("mitcad::CommentImages", "only PNG and JPEG images can be attached");
  }
  const QImage read = QImage::fromData(bytes);
  if (read.isNull()) {
    return QCoreApplication::translate("mitcad::CommentImages", "the file is no image MitCad can read");
  }
  if (image != nullptr) {
    image->bytes = bytes;
    image->width = read.width();
    image->height = read.height();
  }
  return QString();
}

std::optional<CommentImageFile> chooseCommentImage(QWidget* parent) {
  const QString title = QCoreApplication::translate("mitcad::CommentImages", "Attach Image");
  const QString path = dialogOpenFileName(
      parent, title, QString(), QCoreApplication::translate("mitcad::CommentImages", "PNG and JPEG images (*.png *.jpg *.jpeg)"));
  if (path.isEmpty()) {
    return std::nullopt;
  }
  const QString name = QFileInfo(path).fileName();
  QFile file(path);
  if (!file.open(QIODevice::ReadOnly)) {
    const QString why = QCoreApplication::translate("mitcad::CommentImages", "Could not read %1: %2")
                            .arg(name, file.errorString());
    qDebug().noquote() << QStringLiteral("Comment image refused: %1").arg(why);
    sheetWarning(parent, title, why);
    return std::nullopt;
  }
  // One byte past the limit is enough to tell.
  const QByteArray bytes = file.read(kMaxCommentImageBytes + 1);
  CommentImageFile image;
  image.name = name;
  const QString problem = commentImageProblem(bytes, &image);
  if (!problem.isEmpty()) {
    const QString why =
        QCoreApplication::translate("mitcad::CommentImages", "Could not attach %1: %2.").arg(name, problem);
    qDebug().noquote() << QStringLiteral("Comment image refused: %1").arg(why);
    sheetWarning(parent, title, why);
    return std::nullopt;
  }
  qDebug().noquote() << QStringLiteral("Comment image chosen: %1, %2 x %3").arg(name).arg(image.width).arg(image.height);
  return image;
}

std::optional<CommentImageFile> captureAndAnnotate(QWidget* parent, QWidget* view, const QImage& frame) {
  if (frame.isNull()) {
    qDebug().noquote() << QStringLiteral("Markup cancelled: the view gave no picture");
    return std::nullopt;
  }
  CaptureAnnotateDialog dialog(parent, view, frame);
  if (execDialog(&dialog) != QDialog::Accepted) {
    return std::nullopt;
  }
  return dialog.result();
}

void showCommentImage(QWidget* parent, const QByteArray& bytes, const QString& title, const QString& name) {
  const QImage image = QImage::fromData(bytes);
  QDialog dialog(parent);
  dialog.setWindowTitle(title);
  dialog.setObjectName(QStringLiteral("commentImageViewer"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* view = new ImageView(&dialog);
  layout->addWidget(view, 1);
  auto* buttons = new QHBoxLayout;
  const auto add = [&](const QString& text, const QString& objectName, const QString& tip) {
    auto* button = new QPushButton(text, &dialog);
    button->setObjectName(objectName);
    button->setAutoDefault(false);
    if (!tip.isEmpty()) button->setToolTip(tip);
    buttons->addWidget(button);
    return button;
  };
  QPushButton* fit = add(QCoreApplication::translate("mitcad::CommentImages", "Fit"), QStringLiteral("commentImageFit"), QCoreApplication::translate("mitcad::CommentImages", "Shows the whole image"));
  QPushButton* zoomIn = add(QCoreApplication::translate("mitcad::CommentImages", "Zoom In"), QStringLiteral("commentImageZoomIn"), QString());
  QPushButton* zoomOut = add(QCoreApplication::translate("mitcad::CommentImages", "Zoom Out"), QStringLiteral("commentImageZoomOut"), QString());
  buttons->addStretch(1);
  QPushButton* save = add(QCoreApplication::translate("mitcad::CommentImages", "Save Image As…"), QStringLiteral("commentImageSave"), QCoreApplication::translate("mitcad::CommentImages", "Saves the image as a file"));
  QPushButton* close = add(QCoreApplication::translate("mitcad::CommentImages", "Close"), QStringLiteral("commentImageClose"), QString());
  layout->addLayout(buttons);
  QObject::connect(fit, &QPushButton::clicked, view, [view] { view->fit(); });
  QObject::connect(zoomIn, &QPushButton::clicked, view, [view] { view->zoomBy(1.25); });
  QObject::connect(zoomOut, &QPushButton::clicked, view, [view] { view->zoomBy(0.8); });
  QObject::connect(close, &QPushButton::clicked, &dialog, &QDialog::accept);
  QObject::connect(save, &QPushButton::clicked, &dialog, [&dialog, bytes, name] {
    const bool jpeg = isJpeg(bytes);
    const QString path = dialogSaveFileName(&dialog, QCoreApplication::translate("mitcad::CommentImages", "Save Image As"), name,
                                            jpeg ? QCoreApplication::translate("mitcad::CommentImages", "JPEG image (*.jpg *.jpeg)") : QCoreApplication::translate("mitcad::CommentImages", "PNG image (*.png)"));
    if (path.isEmpty()) {
      return;
    }
    QFile file(path);
    if (!file.open(QIODevice::WriteOnly) || file.write(bytes) != bytes.size()) {
      sheetCritical(&dialog, QCoreApplication::translate("mitcad::CommentImages", "Save Image As"),
                    QCoreApplication::translate("mitcad::CommentImages", "Could not write %1: %2.")
                        .arg(QFileInfo(path).fileName(), file.errorString()));
      return;
    }
    qDebug().noquote() << QStringLiteral("Comment image saved: %1").arg(path);
  });
  QString lastZoom;
  view->zoomed = [view, &lastZoom] {
    const QString zoom = view->fitted() ? QStringLiteral("fit %1").arg(view->scale(), 0, 'f', 2)
                                        : QStringLiteral("%1").arg(view->scale(), 0, 'f', 2);
    if (zoom != lastZoom) {
      lastZoom = zoom;
      qDebug().noquote() << QStringLiteral("Comment image viewer zoom: %1").arg(zoom);
    }
  };
  view->setImage(image);
  // Large enough for the image, within the screen.
  dialog.resize(std::clamp(image.width() + 40, 480, 1200), std::clamp(image.height() + 90, 360, 860));
  qDebug().noquote() << QStringLiteral("Comment image viewer opened: %1: %2 x %3")
                            .arg(title)
                            .arg(image.width())
                            .arg(image.height());
  prepareModal(&dialog);
  execDialog(&dialog);
  view->zoomed = nullptr;
  qDebug().noquote() << QStringLiteral("Comment image viewer closed: %1").arg(title);
}

QImage commentThumbnail(const QByteArray& bytes, const QSize& size) {
  const QImage image = QImage::fromData(bytes);
  if (image.isNull()) {
    return image;
  }
  if (image.width() <= size.width() && image.height() <= size.height()) {
    return image;
  }
  return image.scaled(size, Qt::KeepAspectRatio, Qt::SmoothTransformation);
}

QJsonArray commentAttachmentsJson(const QVector<CommentImageFile>& images) {
  QJsonArray attachments;
  for (const CommentImageFile& image : images) {
    attachments.append(QJsonObject{{QStringLiteral("data"), QString::fromLatin1(image.bytes.toBase64())},
                                   {QStringLiteral("name"), image.name}});
  }
  return attachments;
}

} // namespace mitcad
