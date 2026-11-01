// SPDX-License-Identifier: MIT
#include "PolicyDialog.hpp"

#include <exception>
#include <utility>

#include <QCheckBox>
#include <QDialogButtonBox>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/Theme.hpp"

namespace mitcad {
namespace {

QString countText(int count) {
  return count == 1 ? QObject::tr("1 violation") : QObject::tr("%1 violations").arg(count);
}

QString presetText(const QString& preset) {
  if (preset == QStringLiteral("assembly")) {
    return QObject::tr("Assembly");
  }
  if (preset == QStringLiteral("part")) {
    return QObject::tr("Component");
  }
  // `defaults` is the Hybrid preset's name before mitcad#421.
  if (preset == QStringLiteral("hybrid") || preset == QStringLiteral("defaults")) {
    return QObject::tr("Hybrid");
  }
  return QObject::tr("Custom");
}

} // namespace

PolicyDialog::PolicyDialog(Query query, const QString& component, QWidget* parent)
    : QDialog(parent), m_query(std::move(query)), m_component(component) {
  QJsonObject report;
  try {
    report = m_query({{QStringLiteral("query"), QStringLiteral("policy")},
                      {QStringLiteral("component"), component}});
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Design Policy: %1").arg(QString::fromUtf8(e.what()));
  }
  m_name = report.value(QStringLiteral("name")).toString(component);
  m_initial = report.value(QStringLiteral("policy")).toObject();
  m_initial.remove(QStringLiteral("preset"));
  setWindowTitle(tr("Design Policy of %1").arg(m_name));

  auto* layout = new QVBoxLayout(this);
  auto* intro = new QLabel(
      tr("What %1 may hold directly, and which links between its geometry and other components' it "
         "allows. Each component and the root has its own settings; they are not inherited.")
          .arg(m_name),
      this);
  intro->setWordWrap(true);
  layout->addWidget(intro);

  auto* presets = new QHBoxLayout;
  presets->addWidget(new QLabel(tr("Preset:"), this));
  m_preset = new QLabel(this);
  presets->addWidget(m_preset);
  presets->addStretch(1);
  const std::pair<QString, QString> presetButtons[] = {
      {QStringLiteral("hybrid"), tr("&Hybrid")},
      {QStringLiteral("part"), tr("Compo&nent")},
      {QStringLiteral("assembly"), tr("&Assembly")},
  };
  for (const auto& [preset, text] : presetButtons) {
    auto* button = new QPushButton(text, this);
    button->setAutoDefault(false);
    connect(button, &QPushButton::clicked, this, [this, preset = preset] { applyPreset(preset); });
    presets->addWidget(button);
  }
  layout->addLayout(presets);
  auto* presetHelp = new QLabel(tr("Hybrid (the defaults): everything allowed. Component: sketches and features, no "
                                   "components. Assembly: components only. Component and Assembly refuse sketch "
                                   "attachment in both directions, except a component's first sketch, and allow "
                                   "references."),
                                this);
  presetHelp->setWordWrap(true);
  layout->addWidget(presetHelp);

  struct Text {
    const char* key;
    QString heading; // a group starts here
    QString label;
    QString explanation;
  };
  const Text texts[] = {
      {"sketches", tr("Contents"), tr("&Sketches"),
       tr("Sketches can be created in %1 or moved into it.").arg(m_name)},
      {"features", QString(), tr("F&eatures"),
       tr("Modeling features (extrudes, fillets, construction geometry, imported bodies) can be created in %1 "
          "or moved into it. Joints and other assembly features are not governed.")
           .arg(m_name)},
      {"components", QString(), tr("&Components"),
       tr("Components can be created, inserted, pasted or moved directly under %1.").arg(m_name)},
      {"attach_to_others", tr("Sketch attachment"), tr("Attach sketches to &other components"),
       tr("Outgoing: sketches of %1 can lie on faces and planes of other components.").arg(m_name)},
      {"attachments_from_others", QString(), tr("Allow sketch a&ttachments from other components"),
       tr("Incoming: sketches of other components can lie on faces and planes of %1. Both sides must allow an "
          "attachment.")
           .arg(m_name)},
      // A component's first sketch, its start in context (mitcad#293).
      {"first_sketch_attaches", QString(), tr("F&irst sketch may attach to other components"),
       tr("Outgoing, first sketch only: the first sketch of %1 (no sketch or modeling feature of it before it) can "
          "lie on faces and planes of other components, also when its sketches do not attach to others.")
           .arg(m_name)},
      {"first_sketch_attachments_from_others", QString(), tr("Allow first s&ketches of other components to attach"),
       tr("Incoming, first sketches only: the first sketch of another component can lie on faces and planes of %1, "
          "also when it allows no other sketch attachments.")
           .arg(m_name)},
      {"reference_others", tr("References"), tr("&Reference other components"),
       tr("Outgoing: sketches and features of %1 can keep links to other components' geometry (linked "
          "projections, tools and objects of other components). Projections without a link are always allowed.")
           .arg(m_name)},
      {"references_from_others", QString(), tr("Allow references &from other components"),
       tr("Incoming: other components can keep links to the geometry of %1. Both sides must allow a reference.")
           .arg(m_name)},
  };
  auto* grid = new QGridLayout;
  int line = 0;
  for (const Text& text : texts) {
    if (!text.heading.isEmpty()) {
      auto* heading = new QLabel(QStringLiteral("<b>%1</b>").arg(text.heading.toHtmlEscaped()), this);
      grid->addWidget(heading, line++, 0, 1, 2);
    }
    Row row;
    row.key = QString::fromLatin1(text.key);
    row.box = new QCheckBox(text.label, this);
    row.box->setChecked(m_initial.value(row.key).toBool());
    row.count = new QPushButton(this);
    row.count->setAutoDefault(false);
    row.count->setFlat(true);
    setWarningStyleSheet(row.count, QStringLiteral("QPushButton { color: %1; }"));
    row.explanation = new QLabel(text.explanation, this);
    row.explanation->setWordWrap(true);
    row.explanation->setEnabled(false);
    grid->addWidget(row.box, line, 0);
    grid->addWidget(row.count, line++, 1, Qt::AlignRight);
    grid->addWidget(row.explanation, line++, 0, 1, 2);
    const int index = static_cast<int>(m_rows.size());
    connect(row.box, &QCheckBox::toggled, this, [this] {
      if (!m_building) {
        refresh();
      }
    });
    connect(row.count, &QPushButton::clicked, this, [this, index] { showItems(index); });
    m_rows.append(row);
  }
  layout->addLayout(grid);

  auto* note = new QLabel(tr("A policy governs new work only: changing it removes, moves or detaches nothing. "
                             "Objects that break a blocked setting are counted next to it and stay editable."),
                          this);
  note->setWordWrap(true);
  layout->addWidget(note);
  m_message = new QLabel(this);
  m_message->setWordWrap(true);
  setErrorStyleSheet(m_message, QStringLiteral("QLabel { color: %1; }"));
  m_message->hide();
  layout->addWidget(m_message);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
  connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  layout->addWidget(buttons);
  setMinimumWidth(520);
  refresh();
}

QJsonObject PolicyDialog::policy() const {
  QJsonObject settings;
  for (const Row& row : m_rows) {
    settings.insert(row.key, row.box->isChecked());
  }
  return settings;
}

bool PolicyDialog::changed() const { return policy() != m_initial; }

void PolicyDialog::applyPreset(const QString& preset) {
  QJsonObject values;
  try {
    values = m_query({{QStringLiteral("query"), QStringLiteral("policy")},
                      {QStringLiteral("component"), m_component},
                      {QStringLiteral("preset"), preset}})
                 .value(QStringLiteral("policy"))
                 .toObject();
  } catch (const std::exception& e) {
    m_message->setText(QString::fromUtf8(e.what()));
    m_message->show();
    return;
  }
  m_building = true;
  for (Row& row : m_rows) {
    row.box->setChecked(values.value(row.key).toBool());
  }
  m_building = false;
  qDebug().noquote() << QStringLiteral("Policy dialog preset %1").arg(preset);
  refresh();
}

void PolicyDialog::refresh() {
  QJsonObject report;
  try {
    report = m_query({{QStringLiteral("query"), QStringLiteral("policy")},
                      {QStringLiteral("component"), m_component},
                      {QStringLiteral("policy"), policy()}});
    m_message->hide();
  } catch (const std::exception& e) {
    m_message->setText(QString::fromUtf8(e.what()));
    m_message->show();
    return;
  }
  const QString preset = report.value(QStringLiteral("policy")).toObject().value(QStringLiteral("preset")).toString();
  m_preset->setText(presetText(preset));
  QStringList states;
  const QJsonArray settings = report.value(QStringLiteral("settings")).toArray();
  for (Row& row : m_rows) {
    QJsonObject setting;
    for (const QJsonValue& value : settings) {
      if (value.toObject().value(QStringLiteral("setting")).toString() == row.key) {
        setting = value.toObject();
      }
    }
    const bool allowed = setting.value(QStringLiteral("allowed")).toBool();
    row.items = setting.value(QStringLiteral("items")).toArray();
    const int count = static_cast<int>(row.items.size());
    row.count->setText(countText(count));
    row.count->setVisible(!allowed && count > 0);
    row.count->setToolTip(tr("Lists the objects that break this setting"));
    states << (allowed ? QStringLiteral("%1 allowed").arg(row.key)
                       : QStringLiteral("%1 blocked (%2)").arg(row.key).arg(count));
  }
  const QString line = QStringLiteral("Policy dialog %1: %2; %3")
                           .arg(m_name, preset.isEmpty() ? QStringLiteral("custom") : preset,
                                states.join(QStringLiteral(", ")));
  if (line != m_logged) {
    m_logged = line;
    qDebug().noquote() << line;
  }
}

void PolicyDialog::showItems(int index) {
  if (index < 0 || index >= m_rows.size()) {
    return;
  }
  const Row& row = m_rows[index];
  QStringList lines;
  QStringList names;
  for (const QJsonValue& value : row.items) {
    const QJsonObject item = value.toObject();
    QStringList others;
    for (const QJsonValue& other : item.value(QStringLiteral("others")).toArray()) {
      others << other.toObject().value(QStringLiteral("name")).toString();
    }
    const QString name = item.value(QStringLiteral("name")).toString();
    names << name;
    QString text = tr("%1 (%2 in %3)")
                       .arg(name, item.value(QStringLiteral("type")).toString(),
                            item.value(QStringLiteral("component_name")).toString());
    if (!others.isEmpty()) {
      text += tr(", linked with %1").arg(others.join(QStringLiteral(", ")));
    }
    lines << text;
  }
  qDebug().noquote() << QStringLiteral("Policy violations of %1 %2: %3")
                            .arg(m_name, row.key, names.join(QStringLiteral(", ")));
  sheetInformation(this, tr("Design Policy of %1").arg(m_name),
                   tr("%1 breaks “%2”:\n\n%3")
                       .arg(lines.size() == 1 ? tr("This object") : tr("These objects"),
                            QString(row.box->text()).remove(QLatin1Char('&')), lines.join(QLatin1Char('\n'))));
}

} // namespace mitcad
