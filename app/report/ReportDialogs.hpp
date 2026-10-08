// SPDX-License-Identifier: MIT
#pragma once

// The dialogs of feedback and error reports (mitcad#61, mitcad#62):
// Help > Send Feedback's form and the preview every report goes through
// before anything is sent (ReportCenter.hpp).

#include <functional>

#include <QDialog>
#include <QImage>
#include <QList>
#include <QRect>
#include <QString>
#include <QWidget>

#include "report/ReportText.hpp"

class QCheckBox;
class QComboBox;
class QGroupBox;
class QLabel;
class QLineEdit;
class QPlainTextEdit;

namespace mitcad::report {

// A screenshot the user can crop: a drag selects the part to keep, Reset
// takes the whole again.
class ScreenshotView : public QWidget {
  Q_OBJECT

public:
  explicit ScreenshotView(QWidget* parent = nullptr);

  void setImage(const QImage& image);
  // The image as cropped (the whole one without a crop).
  QImage image() const;
  void reset();
  // Logs where the image is, for UI tests ("<name> image at x,y w h").
  void logPlace(const QString& name) const;

signals:
  void cropped();

protected:
  void paintEvent(QPaintEvent* event) override;
  void mousePressEvent(QMouseEvent* event) override;
  void mouseMoveEvent(QMouseEvent* event) override;
  void mouseReleaseEvent(QMouseEvent* event) override;
  QSize sizeHint() const override;

private:
  QRect shownRect() const;  // where the image is drawn
  QPoint toImage(const QPoint& point) const;

  QImage m_image;
  QRect m_crop; // in image pixels; null: the whole image
  QPoint m_from;
  QPoint m_to;
  bool m_dragging = false;
};

// What Help > Send Feedback's form gathered.
struct Feedback {
  QString kind; // "bug", "wish", "other"
  QString summary;
  QString description;
  QString contact;
  bool diagnostics = true;
  QImage screenshot; // null when left out
};

// Help > Send Feedback: the kind, a summary, a description, an optional
// contact address, diagnostics (on) and a screenshot of the window (off;
// croppable). Preview builds the report and shows ReportPreviewDialog;
// the form closes when that sends.
class FeedbackDialog : public QDialog {
  Q_OBJECT

public:
  // `preview` shows the report's preview for what the form holds and
  // returns whether it was sent.
  FeedbackDialog(const QImage& screenshot, std::function<bool(const Feedback&)> preview, QWidget* parent);

  Feedback feedback() const;

private:
  void preview();

  std::function<bool(const Feedback&)> m_preview;
  QComboBox* m_kind = nullptr;
  QLineEdit* m_summary = nullptr;
  QPlainTextEdit* m_description = nullptr;
  QLineEdit* m_contact = nullptr;
  QCheckBox* m_diagnostics = nullptr;
  QCheckBox* m_screenshot = nullptr;
  ScreenshotView* m_view = nullptr;
  QWidget* m_viewRow = nullptr;
};

// The preview of a report: everything it contains, masked, as editable
// sections that can be left out, the title, the duplicate key, and where
// Send takes it. Nothing is sent from here: Send accepts the dialog, and
// ReportCenter delivers what report() then returns.
class ReportPreviewDialog : public QDialog {
  Q_OBJECT

public:
  ReportPreviewDialog(const Report& report, const QImage& screenshot, const QString& destination, QWidget* parent);

  // The report as edited, with the sections left out unchecked.
  Report report() const;
  // The screenshot when it is still included, else a null image.
  QImage screenshot() const;

protected:
  void showEvent(QShowEvent* event) override;

private:
  void logContent() const;

  Report m_report;
  QImage m_screenshot;
  QLineEdit* m_title = nullptr;
  QList<QGroupBox*> m_boxes;
  QList<QPlainTextEdit*> m_texts;
  QGroupBox* m_screenshotBox = nullptr;
};

// Logs "<what> at x,y" for UI tests, in the main window's coordinates (the
// centre of `rect` in `widget`'s).
void logPlace(const QString& what, const QWidget* widget, const QRect& rect);

} // namespace mitcad::report
