// SPDX-License-Identifier: MIT
#include "PrintPanel.hpp"

#include <algorithm>
#include <cmath>
#include <iterator>
#include <utility>
#include <vector>

#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QLabel>
#include <QLocale>
#include <QPushButton>
#include <QRadioButton>
#include <QSettings>
#include <QSignalBlocker>
#include <QVBoxLayout>
#include <QtLogging>

#include "../platform/MacDialogs.hpp"

namespace mitcad::printing {
namespace {

const char* const kPaperKey = "print/paper";

// The papers offered, the first the default of a metric system.
const QPageSize::PageSizeId kPapers[] = {QPageSize::A4,     QPageSize::A3,    QPageSize::A5,
                                         QPageSize::Letter, QPageSize::Legal, QPageSize::Tabloid};

QString number(double value) { return QString::number(value, 'g', 6); }

} // namespace

PrintPanel::PrintPanel(const QString& title, PictureFor pictureFor, bool sketchOptions, QWidget* parent)
    : QDialog(parent), m_pictureFor(std::move(pictureFor)) {
  setWindowTitle(title);
  setObjectName(QStringLiteral("printPanel"));
  auto* layout = new QVBoxLayout(this);
  auto* form = new QFormLayout;
  auto* scaleBox = new QWidget;
  auto* scaleRows = new QVBoxLayout(scaleBox);
  scaleRows->setContentsMargins(0, 0, 0, 0);
  m_actual = new QRadioButton(tr("1:&1, tiled over pages"));
  m_actual->setToolTip(tr("At its size: larger than the paper, it is printed on several pages to tape together"));
  m_fit = new QRadioButton(tr("&Fit to page"));
  m_fit->setToolTip(tr("Scaled to fill one page"));
  m_actual->setChecked(true);
  scaleRows->addWidget(m_actual);
  scaleRows->addWidget(m_fit);
  form->addRow(tr("Scale:"), scaleBox);
  m_paper = new QComboBox;
  m_paper->setObjectName(QStringLiteral("printPaper"));
  for (const QPageSize::PageSizeId id : kPapers) {
    m_paper->addItem(QPageSize::name(id), static_cast<int>(id));
  }
  // The paper chosen last (by its key, whatever the UI's language), else
  // the one of the system's measurement system.
  const QPageSize::PageSizeId fallback =
      QLocale::system().measurementSystem() == QLocale::ImperialUSSystem ? QPageSize::Letter : kPapers[0];
  const QPageSize::PageSizeId saved =
      paperFromSetting(QSettings().value(QLatin1String(kPaperKey)).toString(),
                       std::vector<QPageSize::PageSizeId>(std::begin(kPapers), std::end(kPapers)), fallback);
  m_paper->setCurrentIndex(std::max(m_paper->findData(static_cast<int>(saved)), 0));
  form->addRow(tr("&Paper:"), m_paper);
  m_orientation = new QComboBox;
  m_orientation->setObjectName(QStringLiteral("printOrientation"));
  m_orientation->addItem(tr("Portrait"), static_cast<int>(QPageLayout::Portrait));
  m_orientation->addItem(tr("Landscape"), static_cast<int>(QPageLayout::Landscape));
  form->addRow(tr("&Orientation:"), m_orientation);
  if (sketchOptions) {
    m_dimensions = new QCheckBox(tr("&Dimensions"));
    m_dimensions->setToolTip(tr("Prints the sketch's dimensions with their values"));
    m_constraints = new QCheckBox(tr("&Constraints"));
    m_constraints->setToolTip(tr("Prints a symbol beside each constrained curve or point"));
    form->addRow(QString(), m_dimensions);
    form->addRow(QString(), m_constraints);
  }
  m_pages = new QLabel;
  m_pages->setObjectName(QStringLiteral("printPages"));
  form->addRow(tr("Pages:"), m_pages);
  layout->addLayout(form);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Cancel);
  m_print = buttons->addButton(tr("P&rint…"), QDialogButtonBox::AcceptRole);
  m_print->setDefault(true);
  m_print->setObjectName(QStringLiteral("printPanelPrint"));
  // Print… is the default on every platform (Return prints): the panel has
  // no field whose Return would take a typed value.
  mac::styleDialogButtons(buttons, m_print, true);
  layout->addWidget(buttons);
  connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  connect(m_actual, &QRadioButton::toggled, this, [this] { refresh(false); });
  connect(m_paper, &QComboBox::currentIndexChanged, this, [this] {
    QSettings().setValue(QLatin1String(kPaperKey), paperSetting(paper()));
    refresh(false);
  });
  connect(m_orientation, &QComboBox::activated, this, [this] { m_orientationChosen = true; });
  connect(m_orientation, &QComboBox::currentIndexChanged, this, [this] { refresh(false); });
  if (m_dimensions != nullptr) {
    connect(m_dimensions, &QCheckBox::toggled, this, [this] { refresh(true); });
    connect(m_constraints, &QCheckBox::toggled, this, [this] { refresh(true); });
  }
  refresh(true);
  m_opened = true;
  qInfo().noquote() << QStringLiteral("Print panel opened: %1, %2").arg(m_picture.name, summary());
  m_logged = summary();
  mac::styleDialog(this);
}

PrintScale PrintPanel::scale() const { return m_fit->isChecked() ? PrintScale::Fit : PrintScale::Actual; }

QPageSize::PageSizeId PrintPanel::paper() const {
  return static_cast<QPageSize::PageSizeId>(m_paper->currentData().toInt());
}

QPageLayout PrintPanel::pageLayout() const {
  return pictureLayout(paper(), static_cast<QPageLayout::Orientation>(m_orientation->currentData().toInt()));
}

SketchPrintOptions PrintPanel::options() const {
  SketchPrintOptions out;
  out.dimensions = m_dimensions != nullptr && m_dimensions->isChecked();
  out.constraints = m_constraints != nullptr && m_constraints->isChecked();
  return out;
}

QString PrintPanel::summary() const {
  const bool portrait = m_orientation->currentData().toInt() == static_cast<int>(QPageLayout::Portrait);
  QString text = QStringLiteral("%1 x %2 mm, %3, %4 %5, %6 page(s), %7 x %8")
                     .arg(number(m_picture.size.width()), number(m_picture.size.height()),
                          scale() == PrintScale::Fit ? QStringLiteral("fit") : QStringLiteral("1:1"),
                          QPageSize::key(paper()),
                          portrait ? QStringLiteral("portrait") : QStringLiteral("landscape"))
                     .arg(m_plan.pageCount())
                     .arg(m_plan.rows)
                     .arg(m_plan.columns);
  if (scale() == PrintScale::Fit) {
    text += QStringLiteral(", scale %1").arg(m_plan.scale, 0, 'f', 3);
  }
  return text;
}

void PrintPanel::refresh(bool remake) {
  if (remake) {
    m_picture = m_pictureFor(options());
  }
  if (!m_orientationChosen) {
    const QPageLayout::Orientation best = bestOrientation(scale(), m_picture, paper());
    const QSignalBlocker block(m_orientation);
    m_orientation->setCurrentIndex(m_orientation->findData(static_cast<int>(best)));
  }
  m_plan = printing::plan(scale(), m_picture, printableSize(pageLayout()));
  const int pages = m_plan.pageCount();
  // A picture of nothing (mitcad#1156) is not printed: no blank page.
  m_print->setEnabled(m_picture.valid());
  if (!m_picture.valid()) {
    m_pages->setText(tr("Nothing to print"));
  } else if (scale() == PrintScale::Fit) {
    m_pages->setText(tr("1 page, scaled to %1%").arg(std::lround(m_plan.scale * 100.0)));
  } else if (pages == 1) {
    m_pages->setText(tr("1 page"));
  } else {
    m_pages->setText(tr("%1 pages: %2 × %3 (rows × columns), overlapping by %4 mm")
                         .arg(pages)
                         .arg(m_plan.rows)
                         .arg(m_plan.columns)
                         .arg(number(m_plan.overlap)));
  }
  if (m_opened && summary() != m_logged) {
    m_logged = summary();
    qInfo().noquote() << QStringLiteral("Print panel: %1").arg(m_logged);
  }
}

void PrintPanel::done(int result) {
  if (result != QDialog::Accepted) {
    qInfo().noquote() << QStringLiteral("Print panel cancelled");
  }
  QDialog::done(result);
}

} // namespace mitcad::printing
