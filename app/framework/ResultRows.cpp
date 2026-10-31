// SPDX-License-Identifier: MIT
#include "ResultRows.hpp"

#include <algorithm>
#include <cstddef>

#include <QAction>
#include <QClipboard>
#include <QFont>
#include <QGuiApplication>
#include <QLabel>
#include <QMenu>
#include <QPointer>
#include <QResizeEvent>
#include <QSizePolicy>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include "Dialogs.hpp"
#include "GlassCard.hpp"

namespace mitcad {

// A row of a name and its value: side by side, the name in its group's
// column; where the width is too small for both, the value on a line of
// its own under its name, indented, so that neither is cut.
class ValueLine : public QWidget {
public:
  ValueLine(QLabel* name, QLabel* value, QWidget* parent) : QWidget(parent), m_name(name), m_value(value) {
    name->setParent(this);
    value->setParent(this);
    QSizePolicy policy(QSizePolicy::Preferred, QSizePolicy::Preferred);
    policy.setHeightForWidth(true);
    setSizePolicy(policy);
  }
  void setNameWidth(int width) {
    m_nameWidth = width;
    updateGeometry();
    place();
  }
  // The value under its name now.
  bool stacked() const { return !fits(width()); }
  QSize sizeHint() const override { return {m_nameWidth + kGap + m_value->sizeHint().width(), lineHeight()}; }
  QSize minimumSizeHint() const override {
    return {std::max(m_name->sizeHint().width(), kIndent + m_value->sizeHint().width()), lineHeight()};
  }
  bool hasHeightForWidth() const override { return true; }
  int heightForWidth(int width) const override {
    return fits(width) ? lineHeight() : m_name->sizeHint().height() + m_value->sizeHint().height();
  }

protected:
  void resizeEvent(QResizeEvent* event) override {
    QWidget::resizeEvent(event);
    place();
  }

private:
  static constexpr int kGap = 12;
  static constexpr int kIndent = 12;

  bool fits(int width) const { return m_nameWidth + kGap + m_value->sizeHint().width() <= width; }
  int lineHeight() const { return std::max(m_name->sizeHint().height(), m_value->sizeHint().height()); }
  void place() {
    const int w = width();
    if (fits(w)) {
      const int h = lineHeight();
      m_name->setGeometry(0, 0, m_nameWidth, h);
      m_value->setGeometry(m_nameWidth + kGap, 0, w - m_nameWidth - kGap, h);
    } else {
      const int nameHeight = m_name->sizeHint().height();
      m_name->setGeometry(0, 0, w, nameHeight);
      m_value->setGeometry(kIndent, nameHeight, std::max(0, w - kIndent), m_value->sizeHint().height());
    }
  }

  QLabel* m_name;
  QLabel* m_value;
  int m_nameWidth = 0;
};

namespace {

QString at(const QWidget* widget) {
  const QPoint point = GlassCard::mapToHost(widget, widget->rect().center());
  return QStringLiteral("%1,%2").arg(point.x()).arg(point.y());
}

// A label of one line: never wrapped, its text whole (a row's width is
// checked and logged, not cut by eliding).
QLabel* lineLabel(const QString& text) {
  auto* label = new QLabel(text);
  label->setTextFormat(Qt::PlainText);
  label->setWordWrap(false);
  label->setAlignment(Qt::AlignLeft | Qt::AlignVCenter);
  label->setContextMenuPolicy(Qt::CustomContextMenu);
  return label;
}

} // namespace

ResultRows::ResultRows(const QString& command, QWidget* parent) : QFrame(parent), m_command(command) {
  setObjectName(QStringLiteral("commandResultRows"));
  auto* layout = new QVBoxLayout(this);
  layout->setContentsMargins(8, 6, 8, 6);
  layout->setSpacing(0);
  setVisible(false);
}

void ResultRows::setFloatingStyle(bool floating) {
  if (floating) {
    setFrameShape(QFrame::NoFrame);
    setStyleSheet(QStringLiteral(
        "QFrame#commandResultRows { background: rgba(128, 128, 128, 18); border: none; border-radius: 10px; }"));
  } else {
    setStyleSheet(QString());
    setFrameShape(QFrame::StyledPanel);
  }
}

void ResultRows::set(const Inspection& result) {
  if (m_content != nullptr) {
    // Later: a row's menu may be open (its own event loop) as the answer
    // changes.
    m_content->hide();
    layout()->removeWidget(m_content);
    m_content->deleteLater();
    m_content = nullptr;
  }
  m_rows.clear();
  m_headers.clear();
  m_bodies.clear();
  if (result.values.empty() && result.groups.empty()) {
    setVisible(false);
    return;
  }
  m_content = new QWidget(this);
  auto* column = new QVBoxLayout(m_content);
  column->setContentsMargins(0, 0, 0, 0);
  column->setSpacing(4);
  // The pair's values.
  if (!result.values.empty()) {
    auto* pair = new QWidget(m_content);
    auto* rows = new QVBoxLayout(pair);
    rows->setContentsMargins(0, 0, 0, 0);
    rows->setSpacing(2);
    addRows(rows, result.values, -1);
    column->addWidget(pair);
  }
  // A group for each pick, under a header that folds it.
  for (std::size_t g = 0; g < result.groups.size(); ++g) {
    const Inspection::Group& group = result.groups[g];
    const int index = static_cast<int>(g);
    if (m_folded.size() <= g) {
      m_folded.resize(g + 1, false);
    }
    auto* header = new QToolButton(m_content);
    header->setObjectName(QStringLiteral("commandResultGroup%1").arg(index + 1));
    header->setText(group.title);
    header->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    header->setAutoRaise(true);
    header->setCheckable(true);
    header->setFocusPolicy(Qt::NoFocus);
    QFont bold = header->font();
    bold.setBold(true);
    header->setFont(bold);
    column->addWidget(header, 0, Qt::AlignLeft);
    auto* body = new QWidget(m_content);
    auto* rows = new QVBoxLayout(body);
    // Indented under its header's arrow.
    rows->setContentsMargins(18, 0, 0, 0);
    rows->setSpacing(2);
    addRows(rows, group.values, index);
    column->addWidget(body);
    m_headers.push_back(header);
    m_bodies.push_back(body);
    QObject::connect(header, &QToolButton::clicked, this, [this, index](bool open) {
      fold(index, !open);
      qDebug().noquote() << QStringLiteral("Panel %1 result group %2 %3")
                                .arg(m_command)
                                .arg(index + 1)
                                .arg(open ? QStringLiteral("unfolded") : QStringLiteral("folded"));
    });
    fold(index, m_folded[g]);
  }
  static_cast<QVBoxLayout*>(layout())->addWidget(m_content);
  setVisible(true);
  // What the rows say, for the UI tests.
  const auto listed = [](const std::vector<Inspection::Value>& values) {
    QStringList entries;
    for (const Inspection::Value& value : values) {
      entries << QStringLiteral("%1 = %2").arg(value.name, value.value);
    }
    return entries.join(QStringLiteral(", "));
  };
  if (!result.values.empty()) {
    qDebug().noquote() << QStringLiteral("Panel %1 result values: %2").arg(m_command, listed(result.values));
  }
  for (const Inspection::Group& group : result.groups) {
    qDebug().noquote() << QStringLiteral("Panel %1 result %2: %3").arg(m_command, group.title, listed(group.values));
  }
}

void ResultRows::fold(int group, bool folded) {
  const auto index = static_cast<std::size_t>(group);
  if (index >= m_headers.size()) {
    return;
  }
  m_folded[index] = folded;
  m_headers[index]->setChecked(!folded);
  m_headers[index]->setArrowType(folded ? Qt::RightArrow : Qt::DownArrow);
  m_bodies[index]->setVisible(!folded);
}

void ResultRows::addRows(QVBoxLayout* column, const std::vector<Inspection::Value>& values, int group) {
  std::vector<ValueLine*> lines;
  int nameWidth = 0;
  for (const Inspection::Value& value : values) {
    Row row;
    row.key = group < 0 ? value.name : QStringLiteral("%1 %2").arg(group + 1).arg(value.name);
    row.value = value.value;
    row.group = group;
    row.name = lineLabel(value.name);
    row.text = lineLabel(value.value);
    row.text->setObjectName(QStringLiteral("commandResultValue"));
    if (value.main) {
      // The main value in a larger type (a font in points or in pixels).
      QFont font = row.text->font();
      if (font.pointSizeF() > 0) {
        font.setPointSizeF(font.pointSizeF() * 1.35);
      } else if (font.pixelSize() > 0) {
        font.setPixelSize(static_cast<int>(font.pixelSize() * 1.35));
      }
      font.setBold(true);
      row.text->setFont(font);
      QFont name = row.name->font();
      name.setBold(true);
      row.name->setFont(name);
    }
    if (!value.tip.isEmpty()) {
      row.name->setToolTip(value.tip);
      row.text->setToolTip(value.tip);
    }
    row.line = new ValueLine(row.name, row.text, column->parentWidget());
    column->addWidget(row.line);
    lines.push_back(row.line);
    nameWidth = std::max(nameWidth, row.name->sizeHint().width());
    const QString key = row.key;
    const QString text = row.value;
    for (QLabel* label : {row.name, row.text}) {
      QObject::connect(label, &QWidget::customContextMenuRequested, this,
                       [this, key, text, label](const QPoint& point) { menu(key, text, label->mapToGlobal(point)); });
    }
    m_rows.push_back(row);
  }
  // The names in one column.
  for (ValueLine* line : lines) {
    line->setNameWidth(nameWidth);
  }
}

void ResultRows::menu(const QString& key, const QString& value, const QPoint& global) {
  // No parent, and this guarded: the panel may close while the menu's own
  // event loop runs.
  const QPointer<ResultRows> self(this);
  QMenu menu;
  QAction* copy = menu.addAction(tr("Copy"));
  qDebug().noquote() << QStringLiteral("Context menu: Copy");
  if (execMenu(&menu, global) != copy || self.isNull()) {
    return;
  }
  if (QClipboard* clipboard = QGuiApplication::clipboard()) {
    clipboard->setText(value);
  }
  qDebug().noquote() << QStringLiteral("Panel %1 result copied %2: %3").arg(m_command, key, value);
}

QStringList ResultRows::places() const {
  QStringList lines;
  if (m_rows.empty() || !isVisible()) {
    return lines;
  }
  QStringList stacked;
  QStringList cut;
  int shown = 0;
  for (const Row& row : m_rows) {
    const QString name = QStringLiteral("Panel %1 result %2").arg(m_command, row.key);
    if (!row.text->isVisible()) {
      lines << name + QStringLiteral(" out of sight");
      continue;
    }
    ++shown;
    lines << QStringLiteral("%1 at %2").arg(name, at(row.text));
    if (row.line->stacked()) {
      stacked << row.key;
    }
    // Its name and its value each whole.
    for (const QLabel* label : {row.name, row.text}) {
      const int needed = label->sizeHint().width();
      if (label->width() < needed) {
        cut << QStringLiteral("%1 (%2 of %3)").arg(row.key).arg(needed).arg(label->width());
        break;
      }
    }
  }
  for (std::size_t g = 0; g < m_headers.size(); ++g) {
    const QToolButton* header = m_headers[g];
    lines << QStringLiteral("Panel %1 result group %2 at %3%4")
                 .arg(m_command)
                 .arg(g + 1)
                 .arg(at(header), m_folded[g] ? QStringLiteral(" (folded)") : QString());
    if (header->width() < header->sizeHint().width()) {
      cut << QStringLiteral("group %1 (%2 of %3)").arg(g + 1).arg(header->sizeHint().width()).arg(header->width());
    }
  }
  QStringList fit;
  if (!stacked.isEmpty()) {
    fit << QStringLiteral("under their names: %1").arg(stacked.join(QStringLiteral(", ")));
  }
  if (!cut.isEmpty()) {
    fit << QStringLiteral("cut: %1").arg(cut.join(QStringLiteral(", ")));
  }
  lines << QStringLiteral("Panel %1 result: %2 rows, %3")
               .arg(m_command)
               .arg(shown)
               .arg(fit.isEmpty() ? QStringLiteral("one line each") : fit.join(QStringLiteral("; ")));
  return lines;
}

} // namespace mitcad
