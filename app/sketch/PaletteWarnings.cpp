// SPDX-License-Identifier: MIT
#include "PaletteWarnings.hpp"

#include <utility>

#include <QCursor>
#include <QLabel>
#include <QMouseEvent>
#include <QPalette>
#include <QPoint>
#include <QVBoxLayout>
#include <QDebug>
#include <QtLogging>

#include "../framework/GlassCard.hpp"
#include "../framework/NoticeLevel.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"

namespace mitcad::sketch {
namespace {

// A warning's text; with `clicked` set it reacts to clicks anywhere on it,
// whatever line of the wrapped text the pointer is over.
class WarningLabel : public QLabel {
public:
  std::function<void()> clicked;

protected:
  void mousePressEvent(QMouseEvent* event) override {
    if (clicked && event->button() == Qt::LeftButton) {
      event->accept();
      return;
    }
    QLabel::mousePressEvent(event);
  }

  void mouseReleaseEvent(QMouseEvent* event) override {
    if (clicked && event->button() == Qt::LeftButton) {
      event->accept();
      if (rect().contains(event->position().toPoint())) {
        clicked();
      }
      return;
    }
    QLabel::mouseReleaseEvent(event);
  }
};

} // namespace

PaletteWarnings::PaletteWarnings(QWidget* parent) : QWidget(parent) {
  setObjectName(QStringLiteral("paletteWarnings"));
  m_layout = new QVBoxLayout(this);
  m_layout->setContentsMargins(0, 0, 0, 0);
  m_layout->setSpacing(2);
  hide();
}

void PaletteWarnings::setWarnings(const QStringList& warnings, const std::vector<QStringList>& entities) {
  std::vector<QStringList> about = entities;
  about.resize(static_cast<std::size_t>(warnings.size()));
  if (warnings == m_shown && about == m_entities) {
    return;
  }
  m_shown = warnings;
  m_entities = about;
  for (QLabel* label : m_labels) {
    label->deleteLater();
  }
  m_labels.clear();
  m_linked.clear();
  const QString color = warningColor(palette()).name();
  bool anyLinked = false;
  for (int i = 0; i < warnings.size(); ++i) {
    auto* label = new WarningLabel;
    label->setObjectName(QStringLiteral("paletteWarning%1").arg(i));
    label->setWordWrap(true);
    label->setTextFormat(Qt::RichText);
    const bool linked = !m_entities[static_cast<std::size_t>(i)].isEmpty();
    if (linked) {
      // A link: underlined, a pointing hand; a click selects the curves
      // and points the warning is about.
      label->setText(noticeIconHtml(NoticeLevel::Warning) +
                     QStringLiteral("&nbsp;<span style='color:%1; text-decoration: underline;'>%2</span>")
                         .arg(color, warnings[i].toHtmlEscaped()));
      label->setCursor(Qt::PointingHandCursor);
      label->setToolTip(tr("Click to select the curves this is about"));
      label->clicked = [this, i] {
        if (activated) {
          activated(i);
        }
      };
      anyLinked = true;
    } else {
      // The warning's icon as well as its colour (mitcad#1040).
      label->setText(noticeIconHtml(NoticeLevel::Warning) +
                     QStringLiteral("&nbsp;<span style='color:%1'>%2</span>").arg(color, warnings[i].toHtmlEscaped()));
    }
    m_layout->addWidget(label);
    m_labels.push_back(label);
    m_linked.push_back(linked);
  }
  setVisible(!warnings.isEmpty());
  if (anyLinked) {
    // Once laid out.
    TestSync::singleShot(50, this, [this] { logLayout(); });
  }
}

void PaletteWarnings::logLayout() const {
  for (std::size_t i = 0; i < m_labels.size(); ++i) {
    const QLabel* label = m_labels[i];
    if (!m_linked[i] || !label->isVisible()) {
      continue;
    }
    // "Palette warning 0 at x,y: a link (pointing hand), tooltip "Click to
    // select ..."", for UI tests: the place and the label's own look.
    const QPoint center = GlassCard::mapToHost(label, label->rect().center());
    const bool hand = label->cursor().shape() == Qt::PointingHandCursor;
    const bool underlined = label->text().contains(QStringLiteral("text-decoration: underline"));
    qDebug().noquote() << QStringLiteral("Palette warning %1 at %2,%3: %4 (%5), tooltip \"%6\"")
                              .arg(i)
                              .arg(center.x())
                              .arg(center.y())
                              .arg(underlined ? QStringLiteral("a link") : QStringLiteral("plain text"),
                                   hand ? QStringLiteral("pointing hand") : QStringLiteral("arrow"),
                                   label->toolTip());
  }
}

} // namespace mitcad::sketch
