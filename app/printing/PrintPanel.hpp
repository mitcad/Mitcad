// SPDX-License-Identifier: MIT
#pragma once

// File › Print…'s panel for a sketch or a cut sheet (mitcad#1089): the
// scale (1:1, tiled over pages, or fitted to one page), the paper and its
// orientation, a sketch's dimensions and constraint symbols (off by
// default), and how many pages that makes, before the system's print
// dialog. The paper is remembered (`print/paper`, its QPageSize key, the
// same in every language: mitcad#1156); the orientation follows
// the paper and the scale (the one with fewer pages at 1:1, the larger
// picture fitted) until it is chosen.
//
// Logged for the UI tests: "Print panel opened: Sketch1, 304 x 204 mm,
// 1:1, A4 portrait, 2 page(s), 1 x 2" (the picture's size, rows x
// columns), the same after "Print panel: " when a choice changes it, and
// "Print panel cancelled".

#include <functional>

#include <QDialog>
#include <QPageLayout>
#include <QPageSize>
#include <QString>

#include "PaperPrint.hpp"
#include "SketchPicture.hpp"

class QCheckBox;
class QComboBox;
class QLabel;
class QPushButton;
class QRadioButton;

namespace mitcad::printing {

class PrintPanel : public QDialog {
  Q_OBJECT

public:
  using PictureFor = std::function<PrintPicture(const SketchPrintOptions&)>;

  // `title`: the window's ("Print Sketch"); `pictureFor` gives what is printed
  // for a sketch's choices; `sketchOptions` offers them.
  PrintPanel(const QString& title, PictureFor pictureFor, bool sketchOptions, QWidget* parent = nullptr);

  PrintScale scale() const;
  QPageLayout pageLayout() const;
  SketchPrintOptions options() const;
  const PrintPicture& picture() const { return m_picture; }
  const PrintPlan& plan() const { return m_plan; }
  // The choices for the log: "304 x 204 mm, 1:1, A4 portrait, 2 page(s),
  // 1 x 2" (and ", scale 0.911" fitted).
  QString summary() const;

  void done(int result) override;

private:
  // A choice changed: the picture made again (`remake`), the orientation
  // when it follows the others, the pages counted.
  void refresh(bool remake);
  QPageSize::PageSizeId paper() const;

  PictureFor m_pictureFor;
  PrintPicture m_picture;
  PrintPlan m_plan;
  QRadioButton* m_actual = nullptr;
  QRadioButton* m_fit = nullptr;
  QComboBox* m_paper = nullptr;
  QComboBox* m_orientation = nullptr;
  QCheckBox* m_dimensions = nullptr;
  QCheckBox* m_constraints = nullptr;
  QLabel* m_pages = nullptr;
  QPushButton* m_print = nullptr;
  bool m_orientationChosen = false;
  bool m_opened = false;
  QString m_logged;
};

} // namespace mitcad::printing
