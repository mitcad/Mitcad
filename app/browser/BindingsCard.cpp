// SPDX-License-Identifier: MIT
#include "BindingsCard.hpp"

#include <algorithm>
#include <utility>

#include <QCoreApplication>
#include <QCursor>
#include <QFont>
#include <QGuiApplication>
#include <QHBoxLayout>
#include <QHideEvent>
#include <QKeyEvent>
#include <QLabel>
#include <QPainter>
#include <QPixmap>
#include <QPushButton>
#include <QScreen>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Theme.hpp"

namespace mitcad {
namespace {

// The widest the card gets, and its texts (mitcad#862 review): a longer
// text is elided, whole in its tooltip.
constexpr int kCardWidth = 460;
constexpr int kTextWidth = 300;
constexpr int kSwatch = 12;

// A colour's swatch for a line, or the room of one for none (an empty
// outline would read as a check box).
QLabel* swatch(const QColor& color) {
  auto* label = new QLabel;
  if (color.isValid()) {
    label->setPixmap(bindingSwatch(color, kSwatch));
  }
  label->setFixedSize(kSwatch, kSwatch);
  return label;
}

// A plain text, elided to `width` pixels with the whole text in its
// tooltip (or `tip` when given).
QLabel* textLabel(const QString& text, int width = kTextWidth, const QString& tip = QString()) {
  auto* label = new QLabel;
  label->setTextFormat(Qt::PlainText);
  label->setWordWrap(false);
  const QString shown = label->fontMetrics().elidedText(text, Qt::ElideRight, width);
  label->setText(shown);
  if (!tip.isEmpty()) {
    label->setToolTip(tip);
  } else if (shown != text) {
    label->setToolTip(text);
  }
  return label;
}

// A flat button that looks and behaves as a link: the link colour,
// underlined, darker under the pointer (mitcad#862 review: Edit… read as
// plain text).
void styleAsLink(QPushButton* button) {
  const QColor link = button->palette().color(QPalette::Link);
  button->setFlat(true);
  button->setAutoDefault(false);
  button->setFocusPolicy(Qt::NoFocus);
  button->setCursor(Qt::PointingHandCursor);
  button->setStyleSheet(QStringLiteral("QPushButton { color: %1; text-decoration: underline; border: none; "
                                       "padding: 0px 2px; text-align: left; background: transparent; }"
                                       "QPushButton:hover { color: %2; }"
                                       "QPushButton:pressed { color: %3; }")
                            .arg(link.name(), link.darker(140).name(), link.darker(170).name()));
}

} // namespace

QPixmap bindingSwatch(const QColor& color, int side) {
  QPixmap pixmap(side, side);
  pixmap.fill(Qt::transparent);
  QPainter painter(&pixmap);
  painter.setRenderHint(QPainter::Antialiasing);
  // A filled disc, as the appearance library draws them, its border darker
  // than the colour so that white and grey stay discs.
  painter.setPen(QPen(color.darker(color.lightness() > 200 ? 180 : 150), 1.0));
  painter.setBrush(color);
  painter.drawEllipse(QRectF(0.5, 0.5, side - 1.0, side - 1.0));
  painter.end();
  return pixmap;
}

BindingsCard::BindingsCard(const CommandContext& model, Actions actions, QWidget* window)
    : QFrame(window, Qt::ToolTip | Qt::FramelessWindowHint), m_model(model), m_actions(std::move(actions)) {
  setObjectName(QStringLiteral("bindingsCard"));
  setFrameShape(QFrame::StyledPanel);
  setAutoFillBackground(true);
  setBackgroundRole(QPalette::Window);
  setAttribute(Qt::WA_ShowWithoutActivating);
  m_layout = new QVBoxLayout(this);
  m_layout->setContentsMargins(10, 8, 10, 8);
  setMaximumWidth(kCardWidth); // its texts are elided to fit
  m_follow.setInterval(150);
  connect(&m_follow, &QTimer::timeout, this, &BindingsCard::followPointer);
  m_native = createNativeBindingsCardView(this, [this] { hide(); });
  if (m_native) {
    setAttribute(Qt::WA_DontShowOnScreen);
    setProperty("nativeCompactPanel", true);
  }
}

BindingsCard::~BindingsCard() = default;

void BindingsCard::showFor(const BindingItem& item, const QRect& anchor, bool pinned) {
  if (isVisible() && item == m_item && m_pinned && !pinned) {
    return; // pinned stays pinned while the pointer passes its badge
  }
  m_item = item;
  m_anchor = anchor;
  if (!rebuild()) {
    hide();
    return;
  }
  if (pinned != m_pinned || !isVisible()) {
    // A pinned card is a popup: a click outside closes it, Esc too.
    const bool wasVisible = isVisible();
    m_pinned = pinned;
    if (wasVisible) {
      hide();
    }
    setWindowFlags(pinned && !m_native ? (Qt::Popup | Qt::FramelessWindowHint)
                                      : (Qt::ToolTip | Qt::FramelessWindowHint));
    setAttribute(Qt::WA_ShowWithoutActivating, !pinned);
  }
  place();
  show();
  if (m_native) {
    m_native->show(anchor, pinned);
  }
  if (pinned) {
    m_follow.stop();
    if (!m_native) {
      activateWindow();
      setFocus(Qt::PopupFocusReason);
    }
  } else {
    m_away = 0;
    m_follow.start();
  }
  qDebug().noquote() << QStringLiteral("Bindings card shown for %1 (%2)")
                            .arg(m_name, pinned ? QStringLiteral("pinned") : QStringLiteral("hover"));
  QTimer::singleShot(0, this, &BindingsCard::logPlaces);
}

void BindingsCard::refresh() {
  if (!isVisible()) {
    return;
  }
  if (!rebuild()) {
    hide();
    return;
  }
  place();
  if (m_native) {
    m_native->refresh();
  }
  QTimer::singleShot(0, this, &BindingsCard::logPlaces);
}

bool BindingsCard::rebuild() {
  const QVector<BindingEntry> entries = bindingsOf(readBindings(m_model, true), m_item);
  const bool bound = std::any_of(entries.begin(), entries.end(), [](const BindingEntry& e) { return e.bound(); });
  if (!bound) {
    return false;
  }
  const BindingLookup lookup = BindingLookup::read(m_model);
  if (m_content != nullptr) {
    m_content->hide();
    // A refresh can run before deferred deletion. The native presenter
    // must only discover the replacement's controls, never both trees.
    m_layout->removeWidget(m_content);
    m_content->setParent(nullptr);
    m_content->deleteLater(); // a button of it may be the one clicked
  }
  m_edits.clear();
  m_content = new QWidget;
  auto* column = new QVBoxLayout(m_content);
  column->setContentsMargins(0, 0, 0, 0);
  column->setSpacing(4);
  const QString name = entries.first().item;
  m_name = name;
  setWindowTitle(tr("%1 follows").arg(name));

  // The title and the options it uses, with their values now.
  auto* title = textLabel(tr("%1 follows").arg(name));
  title->setObjectName(QStringLiteral("bindingsCardTitle"));
  QFont bold = title->font();
  bold.setBold(true);
  title->setFont(bold);
  column->addWidget(title);
  QStringList options;
  for (const BindingEntry& entry : entries) {
    for (const QString& option : expressionNames(entry.expression, lookup.values)) {
      if (!options.contains(option)) {
        options << option;
      }
    }
  }
  QStringList now;
  for (const QString& option : std::as_const(options)) {
    now << QStringLiteral("%1 = %2").arg(option, lookup.values.value(option));
  }
  if (!now.isEmpty()) {
    auto* values = textLabel(tr("now: %1").arg(now.join(QStringLiteral(", "))));
    values->setObjectName(QStringLiteral("bindingsCardNow"));
    QPalette faint = values->palette();
    faint.setColor(QPalette::WindowText, faint.color(QPalette::PlaceholderText));
    values->setPalette(faint);
    column->addWidget(values);
  }
  qDebug().noquote() << QStringLiteral("Bindings card %1 follows; now: %2")
                            .arg(name, now.isEmpty() ? QStringLiteral("none") : now.join(QStringLiteral(", ")));

  const QColor warning = warningColor(palette());
  const QColor accent = accentColor(palette());
  for (const BindingEntry& entry : entries) {
    const ReadableBinding readable = readBinding(entry, lookup);
    auto* line = new QWidget;
    auto* lines = new QVBoxLayout(line);
    lines->setContentsMargins(0, 4, 0, 0);
    lines->setSpacing(2);
    auto* head = new QHBoxLayout;
    head->setSpacing(6);
    // A face by its short label, its topological name in the tooltip.
    auto* propertyLabel = textLabel(entry.property(), kTextWidth / 2,
                                    entry.kind == BindingEntry::Kind::FaceAppearance
                                        ? tr("Face %1 of %2").arg(entry.face, entry.item)
                                        : QString());
    propertyLabel->setFont(bold);
    head->addWidget(propertyLabel);
    if (readable.form != ReadableBinding::Form::Mapping && readable.nowColor.isValid()) {
      head->addWidget(swatch(readable.nowColor));
    }
    // Another expression: elided, whole in the tooltip as written.
    QLabel* text = textLabel(readable.form == ReadableBinding::Form::Mapping ? readable.option + QLatin1Char(':')
                                                                              : readable.line(),
                             kTextWidth,
                             readable.form == ReadableBinding::Form::Expression ? entry.expression : QString());
    if (!entry.bound()) {
      QPalette faint = text->palette();
      faint.setColor(QPalette::WindowText, faint.color(QPalette::PlaceholderText));
      text->setPalette(faint);
    }
    head->addWidget(text);
    head->addStretch(1);
    auto* edit = new QPushButton(tr("Edit…"));
    edit->setObjectName(QStringLiteral("bindingsCardEdit_") + entry.key());
    styleAsLink(edit);
    edit->setToolTip(entry.kind == BindingEntry::Kind::Appearance || entry.kind == BindingEntry::Kind::FaceAppearance
                         ? tr("Open the Appearance window for it")
                     : entry.kind == BindingEntry::Kind::Material ? tr("Open the Materials window for it")
                     : entry.kind == BindingEntry::Kind::Suppressed ? tr("Suppress When…")
                                                                     : tr("Show When…"));
    head->addWidget(edit);
    lines->addLayout(head);
    // A mapping: a case per value, its result's swatch, the current one
    // marked.
    for (const BindingCase& c : readable.cases) {
      auto* row = new QHBoxLayout;
      row->setContentsMargins(16, 0, 0, 0);
      row->setSpacing(6);
      row->addWidget(swatch(c.color));
      auto* caseText =
          textLabel(QStringLiteral("%1 → %2").arg(c.value.isEmpty() ? tr("otherwise") : c.value, c.result));
      if (!c.known) {
        QPalette marked = caseText->palette();
        marked.setColor(QPalette::WindowText, warning);
        caseText->setPalette(marked);
        caseText->setToolTip(tr("No appearance or material is named “%1”").arg(c.resultText));
      }
      row->addWidget(caseText);
      if (c.current) {
        auto* mark = textLabel(QStringLiteral("●"));
        QPalette marked = mark->palette();
        marked.setColor(QPalette::WindowText, accent);
        mark->setPalette(marked);
        mark->setToolTip(tr("The current value"));
        row->addWidget(mark);
      }
      row->addStretch(1);
      lines->addLayout(row);
    }
    for (const QString& problem : readable.warnings) {
      auto* label = textLabel(QStringLiteral("⚠ %1").arg(problem), kCardWidth - 40);
      QPalette marked = label->palette();
      marked.setColor(QPalette::WindowText, warning);
      label->setPalette(marked);
      label->setContentsMargins(16, 0, 0, 0);
      lines->addWidget(label);
      qDebug().noquote() << QStringLiteral("Bindings card warning %1: %2").arg(entry.property(), problem);
    }
    column->addWidget(line);
    qDebug().noquote() << QStringLiteral("Bindings card line %1: %2").arg(entry.property(), readable.line());
    const BindingEntry chosen = entry;
    connect(edit, &QPushButton::clicked, this, [this, chosen] {
      qDebug().noquote() << QStringLiteral("Bindings card Edit %1 of %2").arg(chosen.property(), chosen.item);
      hide();
      if (m_actions.edit) {
        const auto action = m_actions.edit;
        QTimer::singleShot(0, this, [action, chosen] { action(chosen); });
      }
    });
    m_edits.append({entry.property(), edit});
  }

  // The link to the design's whole list.
  auto* bottom = new QHBoxLayout;
  bottom->setContentsMargins(0, 6, 0, 0);
  m_all = new QPushButton(tr("All bindings of the design"));
  m_all->setObjectName(QStringLiteral("bindingsCardAll"));
  styleAsLink(m_all);
  bottom->addWidget(m_all);
  bottom->addStretch(1);
  column->addLayout(bottom);
  connect(m_all, &QPushButton::clicked, this, [this] {
    qDebug().noquote() << "Bindings card all bindings";
    hide();
    if (m_actions.showAll) {
      const auto action = m_actions.showAll;
      QTimer::singleShot(0, this, [action] { action(); });
    }
  });
  m_layout->addWidget(m_content);
  // The native view can refresh synchronously, before Qt's deferred show
  // of a widget added to an already visible layout.
  m_content->show();
  adjustSize();
  return true;
}

void BindingsCard::place() {
  adjustSize();
  const QSize wanted = sizeHint().boundedTo(maximumSize());
  const QScreen* display = QGuiApplication::screenAt(m_anchor.center());
  const QRect room = display != nullptr ? display->availableGeometry() : QRect(QPoint(0, 0), QSize(4096, 4096));
  // Below the badge, starting at it; above it when it does not fit below.
  QPoint at(m_anchor.left(), m_anchor.bottom() + 4);
  if (at.y() + wanted.height() > room.bottom()) {
    at.setY(m_anchor.top() - 4 - wanted.height());
  }
  at.setX(std::clamp(at.x(), room.left(), std::max(room.left(), room.right() - wanted.width())));
  at.setY(std::clamp(at.y(), room.top(), std::max(room.top(), room.bottom() - wanted.height())));
  resize(wanted);
  move(at);
}

void BindingsCard::followPointer() {
  if (m_pinned || !isVisible()) {
    m_follow.stop();
    return;
  }
  const QPoint pointer = QCursor::pos();
  if (m_anchor.adjusted(-4, -4, 4, 4).contains(pointer) ||
      (m_native ? m_native->contains(pointer) : frameGeometry().adjusted(-8, -8, 8, 8).contains(pointer))) {
    m_away = 0;
    return;
  }
  // Twice in a row: the pointer left, not crossed the gap to the card.
  if (++m_away >= 2) {
    hide();
  }
}

void BindingsCard::keyPressEvent(QKeyEvent* event) {
  if (event->key() == Qt::Key_Escape) {
    hide();
    return;
  }
  QFrame::keyPressEvent(event);
}

void BindingsCard::hideEvent(QHideEvent* event) {
  QFrame::hideEvent(event);
  m_follow.stop();
  if (m_native) {
    m_native->hide();
  }
  // setWindowFlags hides a shown card on its way to the other kind.
  if (!event->spontaneous()) {
    qDebug().noquote() << "Bindings card closed";
  }
}

void BindingsCard::logPlaces() {
  if (!isVisible()) {
    return;
  }
  const QWidget* host = parentWidget() != nullptr ? parentWidget()->window() : this;
  const QPoint origin = host->mapToGlobal(QPoint(0, 0));
  const auto logAt = [&origin](const QString& what, const QWidget* widget) {
    const QPoint at = widget->mapToGlobal(widget->rect().center()) - origin;
    qDebug().noquote() << QStringLiteral("%1 at %2,%3").arg(what).arg(at.x()).arg(at.y());
  };
  for (const auto& [label, button] : std::as_const(m_edits)) {
    logAt(QStringLiteral("Bindings card Edit %1").arg(label), button);
  }
  if (m_all != nullptr) {
    logAt(QStringLiteral("Bindings card all bindings"), m_all);
  }
}

} // namespace mitcad
