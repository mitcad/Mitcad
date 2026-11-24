// SPDX-License-Identifier: MIT
#include "ImportItemProgress.hpp"

#include <algorithm>

#include <QCoreApplication>
#include <QRegularExpression>

namespace mitcad {
namespace {

// The file's texts in the context ImportItemProgress, where lupdate records
// them too.
struct Text {
  Q_DECLARE_TR_FUNCTIONS(ImportItemProgress)
};

const QRegularExpression& itemStart() {
  static const QRegularExpression pattern(QStringLiteral(R"(^import: \[[0-9.]+\] item ([0-9]+): (.*)$)"));
  return pattern;
}

const QRegularExpression& itemComponent() {
  static const QRegularExpression pattern(QStringLiteral(R"(^import: \[[0-9.]+\] item component: (.+)$)"));
  return pattern;
}

const QRegularExpression& itemDone() {
  static const QRegularExpression pattern(
      QStringLiteral(R"(^import: \[[0-9.]+\] (.+): (parametric|partial|fallback|skipped) in [0-9.]+ s$)"));
  return pattern;
}

} // namespace

ImportItemProgress::Line ImportItemProgress::read(const QString& line) {
  if (line.startsWith(QStringLiteral("import-items "))) {
    m_items = line.mid(13).toInt();
    return Line::Items;
  }
  if (const QRegularExpressionMatch match = itemComponent().match(line); match.hasMatch()) {
    m_component = match.captured(1);
    return Line::Component;
  }
  if (const QRegularExpressionMatch match = itemStart().match(line); match.hasMatch()) {
    m_position = match.captured(1).toInt();
    m_current = match.captured(2);
    // (Its component's line follows, when it has one.)
    m_component.clear();
    return Line::Start;
  }
  if (const QRegularExpressionMatch match = itemDone().match(line); match.hasMatch()) {
    ++m_done;
    m_lastDone = QStringLiteral("%1 %2").arg(match.captured(1), match.captured(2));
    return Line::Done;
  }
  return Line::Other;
}

void ImportItemProgress::startPart() {
  m_position = 0;
  m_current.clear();
  m_component.clear();
}

int ImportItemProgress::shown() const { return std::clamp(m_done + 1, 1, std::max(m_items, 1)); }

QString ImportItemProgress::text() const {
  if (m_items <= 0) {
    return QString();
  }
  if (m_current.isEmpty()) {
    return Text::tr("Timeline item %1 of %2").arg(shown()).arg(m_items);
  }
  if (m_component.isEmpty()) {
    return Text::tr("Timeline item %1 of %2: %3").arg(shown()).arg(m_items).arg(m_current);
  }
  return Text::tr("Timeline item %1 of %2: %3, component %4").arg(shown()).arg(m_items).arg(m_current, m_component);
}

QString ImportItemProgress::partText(int part, int parts, const QString& file) const {
  if (m_current.isEmpty()) {
    return Text::tr("Component %1 of %2: %3").arg(part).arg(parts).arg(file);
  }
  return Text::tr("Component %1 of %2: %3, item %4 of its timeline: %5")
      .arg(part)
      .arg(parts)
      .arg(file)
      .arg(std::max(m_position, 1))
      .arg(m_current);
}

} // namespace mitcad
