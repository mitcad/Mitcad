// SPDX-License-Identifier: MIT
#include "OwnSettingsDialog.hpp"
#include "framework/ModelMessages.hpp"

#include <exception>
#include <utility>

#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonValue>
#include <QLabel>
#include <QPoint>
#include <QPushButton>
#include <QStringList>
#include <QVBoxLayout>
#include <QtLogging>

#include "../browser/DocumentHost.hpp"
#include "../framework/Icons.hpp"
#include "../framework/IdleCall.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../platform/MacDialogs.hpp"
#include "PrintLibrary.hpp"
#include "SettingsForm.hpp"

namespace mitcad {
namespace {

QString text(const QJsonObject& object, const char* field) {
  return object.value(QLatin1String(field)).toString();
}

} // namespace

OwnSettingsDialog::OwnSettingsDialog(DocumentHost& host, QWidget* parent) : QDialog(parent), m_host(host) {
  setWindowIcon(themeIcon(QStringLiteral("print-3d")));
  resize(720, 560);
  auto* layout = new QVBoxLayout(this);
  m_form = new SettingsForm;
  layout->addWidget(m_form, 1);
  m_message = new QLabel;
  m_message->setWordWrap(true);
  setErrorStyleSheet(m_message);
  m_message->hide();
  layout->addWidget(m_message);
  auto* buttons = new QHBoxLayout;
  buttons->addStretch(1);
  m_close = new QPushButton(tr("Close"));
  buttons->addWidget(m_close);
  layout->addLayout(buttons);
  // Enter in a field saves it: no button takes Enter.
  mac::auxiliaryButton(m_close);
  mac::styleDialog(this);
  connect(m_form, &SettingsForm::valueChanged, this, &OwnSettingsDialog::edit);
  connect(m_form, &SettingsForm::layoutChanged, this, &OwnSettingsDialog::scheduleLogPlaces);
  connect(m_close, &QPushButton::clicked, this, &QDialog::reject);
}

OwnSettingsDialog::~OwnSettingsDialog() = default;

void OwnSettingsDialog::setOwner(const QString& owner, const QString& name) {
  m_owner = owner;
  m_plate.clear();
  m_name = name;
  setWindowTitle(tr("%1 · Own settings").arg(name));
  qDebug().noquote() << QStringLiteral("Own settings opened: %1").arg(windowTitle());
  refresh();
}

void OwnSettingsDialog::setPlate(const QString& plate, const QString& name) {
  m_plate = plate;
  m_owner = plate;
  m_name = name;
  setWindowTitle(tr("%1 · Plate settings").arg(name));
  qDebug().noquote() << QStringLiteral("Own settings opened: %1").arg(windowTitle());
  refresh();
}

QJsonObject OwnSettingsDialog::query() const {
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("print_settings")}};
  if (!m_plate.isEmpty()) {
    // The design's settings on the plate (mitcad#1050).
    request.insert(QStringLiteral("component"), QStringLiteral("C0"));
    request.insert(QStringLiteral("plate"), m_plate);
  } else if (m_owner.contains(QStringLiteral(".b"))) {
    request.insert(QStringLiteral("body"), m_owner);
  } else if (m_owner.startsWith(QLatin1Char('O'))) {
    request.insert(QStringLiteral("occurrence"), m_owner);
  } else {
    request.insert(QStringLiteral("component"), m_owner);
  }
  return m_host.model().queryObject(request);
}

void OwnSettingsDialog::refresh() {
  if (m_owner.isEmpty()) {
    return;
  }
  QJsonObject print;
  try {
    if (m_keys.isEmpty()) {
      m_keys = PrintKeyTable(m_host.model().queryObject({{QStringLiteral("query"), QStringLiteral("print_keys")}}));
    }
    print = query();
  } catch (const std::exception& e) {
    m_message->setText(m_keys.plainText(mitcad::modelMessage(e.what())));
    m_message->show();
    return;
  }
  m_form->setKeys(m_keys, QStringLiteral("process"), designDialectOf(m_host.model()));
  // The owner's own values set; what it prints with otherwise as the
  // placeholders.
  QJsonObject own;
  QJsonObject inherited;
  for (const QJsonValue& value : print.value(QStringLiteral("process")).toObject().value(QStringLiteral("values")).toArray()) {
    const QJsonObject entry = value.toObject();
    const bool mine = m_plate.isEmpty()
                          ? text(entry, "from") == QStringLiteral("property") && text(entry, "owner") == m_owner
                          : text(entry, "from") == QStringLiteral("plate") && text(entry, "plate") == m_plate;
    (mine ? own : inherited).insert(text(entry, "key"), entry.value(QStringLiteral("value")));
  }
  m_form->setValues(own);
  m_form->setPlaceholderValues(inherited);
  scheduleLogPlaces();
}

void OwnSettingsDialog::edit(const QString& key, const QJsonValue& value) {
  const QString owner = m_owner;
  const QString plate = m_plate;
  // The model command rebuilds the form: not while the field handles its
  // signal.
  afterEventWhenIdle(m_host, this, [this, owner, plate, key, value] {
    if (!m_host.canChangePrintSettings()) {
      m_message->setText(m_host.changeBlockedReason());
      m_message->show();
      return;
    }
    const bool clear = value.isNull() || value.isUndefined();
    QJsonObject command{{QStringLiteral("cmd"), clear ? QStringLiteral("remove_property") : QStringLiteral("set_property")},
                        {QStringLiteral("owner"), owner},
                        {QStringLiteral("set"), QStringLiteral("Print Process")},
                        {QStringLiteral("key"), key}};
    if (!clear) {
      command.insert(QStringLiteral("value"), value);
    }
    if (!plate.isEmpty()) {
      // A print plate's value (mitcad#1050): null clears it.
      QJsonObject values;
      values.insert(key, clear ? QJsonValue(QJsonValue::Null) : value);
      command = QJsonObject{{QStringLiteral("cmd"), QStringLiteral("edit_print_plate")},
                            {QStringLiteral("plate"), plate},
                            {QStringLiteral("values"), values}};
    }
    if (m_host.runModelCommand(command)) {
      m_message->hide();
      m_host.commandInputsChanged();
      const PrintKey* printKey = m_keys.find(key);
      qDebug().noquote() << QStringLiteral("%1 %2 %3 = %4")
                                .arg(plate.isEmpty() ? QStringLiteral("Own setting") : QStringLiteral("Plate setting"),
                                     owner, key,
                                     clear ? QStringLiteral("cleared")
                                           : (printKey != nullptr ? printValueText(*printKey, value)
                                                                  : value.toVariant().toString()));
    } else if (!m_host.lastCancelled()) {
      qDebug().noquote() << QStringLiteral("Own settings: %1 refused: %2").arg(key, m_host.lastError());
      refresh();
      m_form->setError(key, m_keys.plainText(mitcad::modelMessage(m_host.lastError())));
      return;
    }
    refresh();
  });
}

void OwnSettingsDialog::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  m_loggedPlaces.clear();
  scheduleLogPlaces();
}

void OwnSettingsDialog::scheduleLogPlaces() {
  TestSync::singleShot(0, this, [this] { logPlaces(); });
}

void OwnSettingsDialog::logPlaces() {
  if (!isVisible() || parentWidget() == nullptr) {
    return;
  }
  // In the main window's coordinates, as the other logged places.
  const QPoint origin = parentWidget()->window()->mapToGlobal(QPoint(0, 0));
  const auto at = [&origin](QWidget* widget, const QPoint& point) {
    const QPoint global = widget->mapToGlobal(point) - origin;
    return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
  };
  QStringList lines = m_form->places(QStringLiteral("Own settings"), at);
  lines << QStringLiteral("Own settings close at %1").arg(at(m_close, m_close->rect().center()));
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedPlaces) {
    m_loggedPlaces = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
