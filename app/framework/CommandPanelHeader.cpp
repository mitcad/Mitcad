// SPDX-License-Identifier: MIT
// Buttons at the end of a command panel's title row (mitcad#564: Change
// Parameters, which opens the parameter table while the panel stays open),
// and sections of a command's own under the result (mitcad#648,
// mitcad#1010: Measure's actions and history).
#include "CommandPanel.hpp"

#include <functional>
#include <utility>

#include <QAction>
#include <QBoxLayout>
#include <QEvent>
#include <QLabel>
#include <QLayout>
#include <QLayoutItem>
#include <QObject>
#include <QPoint>
#include <QPointer>
#include <QToolButton>
#include <QWidget>
#include <QtLogging>

#include "Command.hpp"
#include "CommandMessage.hpp"
#include "GlassCard.hpp"
#include "TestSync.hpp"

namespace mitcad {
namespace {

// Logs where a title row button is, in the main window's coordinates as the
// panel's other places ("Panel Extrude parameters at x,y"), once it shows
// and whenever it or what it is in moves, when that changed.
class HeaderButtonPlace : public QObject {
public:
  HeaderButtonPlace(QToolButton* button, QString line)
      : QObject(button), m_button(button), m_line(std::move(line)) {
    button->installEventFilter(this);
  }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    const QEvent::Type type = event->type();
    if (type == QEvent::Show && watched == m_button.data() && !m_watching) {
      // The rows it is in are there by now: their moves move it too.
      m_watching = true;
      for (QWidget* parent = m_button->parentWidget(); parent != nullptr; parent = parent->parentWidget()) {
        parent->installEventFilter(this);
      }
    }
    if (type == QEvent::Show || type == QEvent::Move || type == QEvent::Resize) {
      TestSync::singleShot(0, this, [this] { log(); });
    }
    return QObject::eventFilter(watched, event);
  }

private:
  void log() {
    if (m_button == nullptr || !m_button->isVisible()) {
      return;
    }
    const QPoint at = GlassCard::mapToHost(m_button, m_button->rect().center());
    const QString line = QStringLiteral("%1 at %2,%3").arg(m_line).arg(at.x()).arg(at.y());
    if (line != m_logged) {
      m_logged = line;
      qDebug().noquote() << line;
    }
  }

  QPointer<QToolButton> m_button;
  QString m_line;
  QString m_logged;
  bool m_watching = false;
};

} // namespace

void CommandPanel::addHeaderAction(QAction* action) {
  if (action == nullptr || layout() == nullptr || layout()->count() == 0) {
    return;
  }
  // The title row is the panel's first row (the icon and the name).
  QLayoutItem* first = layout()->itemAt(0);
  auto* header = first != nullptr ? qobject_cast<QBoxLayout*>(first->layout()) : nullptr;
  if (header == nullptr) {
    return;
  }
  // "command_solid.parameters": "parameters".
  const QString id = action->objectName().section(QLatin1Char('.'), -1);
  auto* button = new QToolButton(this);
  button->setObjectName(QStringLiteral("commandHeader_") + id);
  button->setDefaultAction(action);
  button->setToolButtonStyle(Qt::ToolButtonIconOnly);
  button->setAutoRaise(true);
  // The keyboard stays in the value fields, as with OK and Cancel.
  button->setFocusPolicy(Qt::NoFocus);
  header->addWidget(button);
  new HeaderButtonPlace(button, QStringLiteral("Panel %1 %2").arg(m_def.name, id));
}

void CommandPanel::addSection(QWidget* section, std::function<QStringList()> places) {
  auto* box = qobject_cast<QBoxLayout*>(layout());
  if (section == nullptr || box == nullptr) {
    return;
  }
  // Under the result and the message, above the buttons: the panel in the
  // order of the task, the picks, what they measure, what to do with it
  // (mitcad#1010).
  const int at = box->indexOf(m_message);
  box->insertWidget(at < 0 ? box->count() : at + 1, section);
  // Its moves move what it logs (mitcad#528), as the rows' do.
  section->installEventFilter(this);
  if (places) {
    m_sectionPlaces.push_back(std::move(places));
  }
  scheduleLogLayout();
}

} // namespace mitcad
