// SPDX-License-Identifier: MIT
#include "ConfigurationBar.hpp"

#include <algorithm>
#include <cmath>
#include <exception>

#include <QComboBox>
#include <QFontMetrics>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QSignalBlocker>
#include <QSlider>
#include <QtLogging>

#include "../framework/GlassCard.hpp"
#include "../framework/PlaceWatcher.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "DocumentHost.hpp"

namespace mitcad {
namespace {

namespace cfg = configurations;

constexpr int kMaxSteps = 10000; // a slider's positions at the most

} // namespace

ConfigurationBar::ConfigurationBar(DocumentHost& host, QWidget* parent) : QWidget(parent), m_host(host) {
  setObjectName(QStringLiteral("configurationBar"));
  setProperty("nativeCompactPanel", true);
  setProperty("nativeLayoutContract", true);
  m_grid = new QGridLayout(this);
  m_grid->setContentsMargins(6, 4, 6, 6);
  m_grid->setColumnStretch(1, 1);
  m_title = new QLabel(tr("Configuration"));
  QFont font = m_title->font();
  font.setBold(true);
  m_title->setFont(font);
  m_row = new QComboBox;
  m_row->setObjectName(QStringLiteral("configurationRow"));
  m_row->setToolTip(tr("Rows: presets of the options' values"));
  m_row->setFocusPolicy(Qt::StrongFocus);
  m_grid->addWidget(m_title, 0, 0);
  m_grid->addWidget(m_row, 0, 1);
  m_notice = new QLabel;
  m_notice->setObjectName(QStringLiteral("configurationNotice"));
  m_notice->setWordWrap(true);
  m_notice->hide();
  m_grid->addWidget(m_notice, 1, 0, 1, 2);
  connect(m_row, &QComboBox::activated, this, [this](int index) {
    const QString row = m_row->itemData(index).toString();
    if (!row.isEmpty()) {
      chooseRow(row);
    }
  });
  new PlaceWatcher(this, [this] { logLayout(); });
  hide();
}

void ConfigurationBar::refresh() {
  m_table = cfg::Table();
  m_report = cfg::Report();
  try {
    m_table = cfg::readTable(m_host.model().queryObject(cfg::configurationsQuery()));
    if (!m_table.options.isEmpty()) {
      m_report = cfg::readReport(m_host.model().queryObject(cfg::configureQuery()));
    }
  } catch (const std::exception&) {
    // A table the model cannot report on: the window says why; the bar
    // offers nothing.
    m_table = cfg::Table();
    m_report = cfg::Report();
  }
  const bool shown = !m_report.options.isEmpty() || !m_table.rows.isEmpty();
  if (isHidden() == shown) {
    setVisible(shown);
  }
  if (!shown) {
    if (m_loggedState != QStringLiteral("none")) {
      m_loggedState = QStringLiteral("none");
      qDebug().noquote() << "Configuration bar: none";
    }
    return;
  }
  // The widgets are made again only when the choices change (another
  // option, another list of allowed values, a range with or without a
  // slider); otherwise the values are shown in them.
  QStringList structure;
  for (const cfg::Row& row : std::as_const(m_table.rows)) {
    structure << row.name;
  }
  for (const cfg::OptionReport& option : std::as_const(m_report.options)) {
    structure << QStringLiteral("%1|%2|%3|%4|%5")
                     .arg(option.name, option.text ? QStringLiteral("text") : QStringLiteral("number"))
                     .arg(option.driven ? 1 : 0)
                     .arg(static_cast<int>(option.allowed.kind))
                     .arg(option.allowed.kind == cfg::Domain::Kind::List
                              ? option.allowed.summary()
                              : QString::number(option.allowed.hasSlider() ? 1 : 0));
  }
  const QString key = structure.join(QLatin1Char('\n'));
  if (key != m_structure) {
    m_structure = key;
    rebuild();
  }
  showValues();
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void ConfigurationBar::rebuild() {
  for (QWidget* widget : std::as_const(m_owned)) {
    m_grid->removeWidget(widget);
    widget->hide();
    widget->deleteLater();
  }
  m_owned.clear();
  m_choosers.clear();
  m_grid->removeWidget(m_notice);
  int line = 1;
  for (const cfg::OptionReport& option : std::as_const(m_report.options)) {
    const QString name = option.name;
    auto* label = new QLabel(name, this);
    m_owned << label;
    m_grid->addWidget(label, line, 0);
    Chooser chooser;
    chooser.option = name;
    if (option.driven) {
      chooser.label = new QLabel(this);
      chooser.label->setObjectName(QStringLiteral("configurationDriven_%1").arg(name));
      m_owned << chooser.label;
      m_grid->addWidget(chooser.label, line, 1);
    } else if (option.allowed.kind == cfg::Domain::Kind::List) {
      auto* combo = new QComboBox(this);
      combo->setObjectName(QStringLiteral("configurationChooser_%1").arg(name));
      combo->setFocusPolicy(Qt::StrongFocus);
      for (const cfg::DomainValue& value : option.allowed.values) {
        combo->addItem(value.text, cfg::chooserValue(option, value).toVariant());
      }
      connect(combo, &QComboBox::activated, this, [this, name, combo](int index) {
        const QVariant data = combo->itemData(index);
        if (data.isValid()) {
          choose(name, QJsonValue::fromVariant(data), combo->itemText(index));
        }
      });
      chooser.combo = combo;
      m_owned << combo;
      m_grid->addWidget(combo, line, 1);
    } else {
      auto* holder = new QWidget(this);
      auto* row = new QHBoxLayout(holder);
      row->setContentsMargins(0, 0, 0, 0);
      auto* field = new QLineEdit(holder);
      field->setObjectName(QStringLiteral("configurationField_%1").arg(name));
      // Wide enough for the range's ends and the value as shown (`1200 mm`),
      // so the slider beside it cannot squeeze a value out of sight
      // (mitcad#969).
      const QStringList shownTexts{option.allowed.minText, option.allowed.maxText, option.valueText};
      int widestText = 0;
      for (const QString& shown : std::as_const(shownTexts)) {
        widestText = std::max(widestText, field->fontMetrics().horizontalAdvance(shown));
      }
      const int fieldWidth = widestText + 16; // the frame and the text margins
      field->setMinimumWidth(fieldWidth);
      row->addWidget(field, 1);
      connect(field, &QLineEdit::editingFinished, this, [this, name, field] {
        if (!field->isModified()) {
          return; // the focus left without an edit
        }
        field->setModified(false);
        if (const cfg::OptionReport* report = m_report.option(name)) {
          choose(name, cfg::chooserValue(*report, field->text()), field->text().trimmed());
        }
      });
      chooser.field = field;
      if (option.allowed.hasSlider()) {
        const double span = *option.allowed.max - *option.allowed.min;
        const double steps =
            option.allowed.step && *option.allowed.step > 0 ? std::floor(span / *option.allowed.step + 1e-9) : 100.0;
        chooser.steps = static_cast<int>(std::clamp(steps, 1.0, static_cast<double>(kMaxSteps)));
        auto* slider = new QSlider(Qt::Horizontal, holder);
        slider->setObjectName(QStringLiteral("configurationSlider_%1").arg(name));
        slider->setProperty("nativeSliderOnRelease", true);
        slider->setRange(0, chooser.steps);
        slider->setFocusPolicy(Qt::StrongFocus);
        field->setMaximumWidth(std::max(96, fieldWidth));
        row->addWidget(slider, 2);
        chooser.slider = slider;
        const int positions = chooser.steps;
        // The value in mm or rad, as `configure` takes a number.
        const auto send = [this, name, slider, positions] {
          Chooser at;
          at.option = name;
          at.steps = positions;
          const double value = sliderValue(at, slider->value());
          choose(name, value, QString::number(value, 'g', 12));
        };
        // A drag sets the value when it is let go; the keys and a click on
        // the groove at once.
        connect(slider, &QSlider::sliderReleased, this, send);
        connect(slider, &QSlider::valueChanged, this, [slider, send] {
          if (!slider->isSliderDown()) {
            send();
          }
        });
      }
      m_owned << holder;
      m_grid->addWidget(holder, line, 1);
    }
    m_choosers << chooser;
    ++line;
  }
  m_grid->addWidget(m_notice, line, 0, 1, 2);
}

void ConfigurationBar::showValues() {
  {
    const QSignalBlocker blocker(m_row);
    m_row->clear();
    if (m_report.row.isEmpty() || m_table.row(m_report.row) == nullptr) {
      m_row->addItem(tr("No row"), QString());
    }
    for (const cfg::Row& row : std::as_const(m_table.rows)) {
      m_row->addItem(row.name, row.name);
    }
    m_row->setCurrentIndex(std::max(0, m_row->findData(m_report.row)));
    m_row->setEnabled(!m_table.rows.isEmpty());
  }
  QStringList state;
  for (Chooser& chooser : m_choosers) {
    const cfg::OptionReport* option = m_report.option(chooser.option);
    if (option == nullptr) {
      continue;
    }
    QString why = tr("Allowed: %1").arg(option->allowed.summary());
    if (!option->restrictedBy.isEmpty()) {
      why += QLatin1Char('\n') + tr("Narrowed by %1").arg(option->restrictedBy.join(QStringLiteral(", ")));
    }
    if (chooser.label != nullptr) {
      chooser.label->setText(option->valueText);
      chooser.label->setToolTip(tr("Driven by its formula %1").arg(option->expression));
    }
    if (chooser.combo != nullptr) {
      const QSignalBlocker blocker(chooser.combo);
      for (int i = chooser.combo->count() - 1; i >= 0; --i) {
        if (!chooser.combo->itemData(i).isValid()) {
          chooser.combo->removeItem(i);
        }
      }
      int index = chooser.combo->findText(option->valueText);
      if (index < 0) {
        // A value the rules do not allow now (a broken rule, a driven
        // option before it): shown, not offered.
        chooser.combo->insertItem(0, option->valueText, QVariant());
        index = 0;
      }
      chooser.combo->setCurrentIndex(index);
      chooser.combo->setToolTip(why);
    }
    if (chooser.field != nullptr) {
      chooser.field->setText(option->valueText);
      if (!chooser.field->hasFocus()) {
        // Shown from its start: a value wider than the field keeps its
        // first digits in sight, not only its unit (mitcad#969).
        chooser.field->setCursorPosition(0);
      }
      chooser.field->setToolTip(why);
    }
    if (chooser.slider != nullptr) {
      const QSignalBlocker blocker(chooser.slider);
      chooser.slider->setValue(sliderPosition(chooser, option->value.toDouble()));
      chooser.slider->setToolTip(why);
    }
    state << QStringLiteral("%1 = %2 (%3)")
                 .arg(option->name, option->valueText,
                      option->driven ? QStringLiteral("driven")
                      : option->allowed.kind == cfg::Domain::Kind::List
                          ? option->allowed.summary().replace(QStringLiteral(", "), QStringLiteral(" | "))
                          : option->allowed.summary());
  }
  const QString logged = QStringLiteral("%1; row %2")
                             .arg(state.join(QStringLiteral("; ")),
                                  m_report.row.isEmpty() ? QStringLiteral("none") : m_report.row);
  if (logged != m_loggedState) {
    m_loggedState = logged;
    qDebug().noquote() << QStringLiteral("Configuration bar: %1").arg(logged);
  }
}

double ConfigurationBar::sliderValue(const Chooser& chooser, int position) const {
  const cfg::OptionReport* option = m_report.option(chooser.option);
  if (option == nullptr || !option->allowed.hasSlider() || chooser.steps <= 0) {
    return 0.0;
  }
  const cfg::Domain& allowed = option->allowed;
  if (allowed.step && *allowed.step > 0) {
    return std::min(*allowed.max, *allowed.min + position * *allowed.step);
  }
  return *allowed.min + (*allowed.max - *allowed.min) * position / chooser.steps;
}

int ConfigurationBar::sliderPosition(const Chooser& chooser, double value) const {
  const cfg::OptionReport* option = m_report.option(chooser.option);
  if (option == nullptr || !option->allowed.hasSlider() || chooser.steps <= 0) {
    return 0;
  }
  const cfg::Domain& allowed = option->allowed;
  const double unit = allowed.step && *allowed.step > 0 ? *allowed.step
                                                        : (*allowed.max - *allowed.min) / chooser.steps;
  const long position = std::lround((value - *allowed.min) / unit);
  return static_cast<int>(std::clamp(position, 0L, static_cast<long>(chooser.steps)));
}

void ConfigurationBar::choose(const QString& option, const QJsonValue& value, const QString& shown) {
  qDebug().noquote() << QStringLiteral("Configuration chosen: %1 = %2").arg(option, shown);
  run(cfg::configureCommand({{option, value}}), QStringLiteral("%1 = %2").arg(option, shown));
}

void ConfigurationBar::chooseRow(const QString& row) {
  qDebug().noquote() << QStringLiteral("Configuration row chosen: %1").arg(row);
  run(cfg::applyCommand(row), row);
}

void ConfigurationBar::run(const QJsonObject& command, const QString& what) {
  // After the chooser's own handling: the command refreshes the bar, which
  // may make the chooser again.
  TestSync::singleShot(0, this, [this, command, what] {
    const auto go = [this, command, what] {
      QString blocked = m_host.readOnlyReason();
      if (blocked.isEmpty() && !m_host.canChangeModel()) {
        blocked = m_host.changeBlockedReason();
      }
      if (!blocked.isEmpty()) {
        showNotice(blocked, true);
        qDebug().noquote() << QStringLiteral("Configuration refused: %1").arg(blocked);
        refresh();
        return;
      }
      QJsonObject result;
      if (m_host.runModelCommand(command, &result)) {
        if (const std::optional<cfg::Refusal> refused = cfg::readRefusal(result.value(QStringLiteral("refused")))) {
          const QString text = tr("%1 is refused: %2").arg(what, refused->describe());
          showNotice(text, true);
          m_host.showStatus(text, true);
          qDebug().noquote() << QStringLiteral("Configuration refused: %1").arg(refused->describe());
        } else {
          const QString notice = cfg::movesNotice(cfg::readMoves(result.value(QStringLiteral("moves"))));
          showNotice(notice, false);
          if (!notice.isEmpty()) {
            m_host.showStatus(notice, false);
            qDebug().noquote() << QStringLiteral("Configuration moves: %1").arg(notice);
          }
        }
      } else if (!m_host.lastCancelled()) {
        showNotice(m_host.lastError(), true);
        qDebug().noquote() << QStringLiteral("Configuration refused: %1").arg(m_host.lastError());
      }
      refresh();
    };
    if (m_host.modelBusy()) {
      m_host.whenIdle(this, go);
    } else {
      go();
    }
  });
}

void ConfigurationBar::showNotice(const QString& text, bool error) {
  m_notice->setText(text);
  if (error) {
    setErrorStyleSheet(m_notice);
  } else {
    setWarningStyleSheet(m_notice);
  }
  m_notice->setVisible(!text.isEmpty());
}

void ConfigurationBar::logLayout() {
  if (!isVisible()) {
    return;
  }
  QStringList lines;
  const auto place = [&lines](const QString& what, const QWidget* widget) {
    if (widget == nullptr || !widget->isVisible()) {
      return;
    }
    const QPoint at = GlassCard::mapToHost(widget, widget->rect().center());
    lines << QStringLiteral("%1 at %2,%3").arg(what).arg(at.x()).arg(at.y());
  };
  place(QStringLiteral("Configuration bar row"), m_row);
  for (const Chooser& chooser : std::as_const(m_choosers)) {
    const QString what = QStringLiteral("Configuration bar %1").arg(chooser.option);
    place(what, chooser.combo != nullptr  ? static_cast<const QWidget*>(chooser.combo)
                : chooser.field != nullptr ? static_cast<const QWidget*>(chooser.field)
                                           : static_cast<const QWidget*>(chooser.label));
    place(what + QStringLiteral(" slider"), chooser.slider);
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_logged) {
    m_logged = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
