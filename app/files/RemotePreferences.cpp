// SPDX-License-Identifier: MIT
#include "RemotePreferences.hpp"

#include <QCheckBox>
#include <QDir>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QJsonObject>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QSpinBox>
#include <QtLogging>

#include "../framework/Json.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {

RemotePreferencesBox::RemotePreferencesBox(QWidget* parent)
    : QGroupBox(tr("Version Control"), parent), m_settings(RemoteSettings::load()) {
  auto* form = new QFormLayout(this);
  m_git = new QLineEdit(QDir::toNativeSeparators(m_settings.git));
  m_git->setPlaceholderText(tr("Found automatically"));
  m_git->setToolTip(tr("The git program that sends and gets the versions of projects with a remote repository; "
                       "signing in is its own (SSH keys, a credential helper). The local version history needs "
                       "no git program."));
  auto* browse = new QPushButton(tr("Bro&wse..."));
  auto* gitRow = new QHBoxLayout;
  gitRow->addWidget(m_git, 1);
  gitRow->addWidget(browse);
  auto* gitLabel = new QLabel(tr("Gi&t program:"));
  gitLabel->setBuddy(m_git);
  form->addRow(gitLabel, gitRow);
  // One line (a wrapping label would take room it does not use).
  m_found = new QLabel;
  m_found->setTextInteractionFlags(Qt::TextSelectableByMouse);
  form->addRow(QString(), m_found);
  m_minutes = new QSpinBox;
  m_minutes->setRange(0, RemoteSettings::kMaxCheckMinutes);
  m_minutes->setSuffix(tr(" min"));
  m_minutes->setSpecialValueText(tr("never"));
  m_minutes->setValue(m_settings.checkMinutes);
  m_minutes->setToolTip(tr("When a design of a project with a remote opens, and then this often, Mitcad asks "
                           "the remote for newer versions (git fetch); never: only Sync and Check for Newer "
                           "Versions do"));
  m_push = new QCheckBox(tr("Send each saved version to the rem&ote"));
  m_push->setChecked(m_settings.autoPush);
  m_push->setToolTip(tr("Without a connection the versions wait, and go when the remote can be reached again; "
                        "off: only Sync sends them"));
  auto* row = new QHBoxLayout;
  auto* minutesLabel = new QLabel(tr("Check the remote for newer versions e&very"));
  minutesLabel->setBuddy(m_minutes);
  row->addWidget(minutesLabel);
  row->addWidget(m_minutes);
  row->addSpacing(16);
  row->addWidget(m_push);
  row->addStretch(1);
  form->addRow(row);
  connect(browse, &QPushButton::clicked, this, [this] {
    const QString chosen = QFileDialog::getOpenFileName(this, tr("Git Program"), QFileInfo(m_git->text()).path());
    if (!chosen.isEmpty()) {
      m_git->setText(QDir::toNativeSeparators(chosen));
      detect();
    }
  });
  connect(m_git, &QLineEdit::editingFinished, this, &RemotePreferencesBox::detect);
  detect();
}

void RemotePreferencesBox::detect() {
  const QString program = QDir::fromNativeSeparators(m_git->text().trimmed());
  if (program.isEmpty() && !m_settings.git.isEmpty()) {
    // What remote work finds now is still the program set before.
    m_found->setText(tr("The git program on PATH (or where Git for Windows installs itself), once saved."));
    return;
  }
  const rust::String json = git_info_at(rustStr(program.toUtf8()));
  const QJsonObject info = parseObject(json);
  QString text;
  if (!info.value(QStringLiteral("error")).isNull()) {
    text = info.value(QStringLiteral("error")).toObject().value(QStringLiteral("message")).toString();
  } else {
    const QString lfs = info.value(QStringLiteral("lfs")).toString();
    text = tr("git %1 at %2").arg(info.value(QStringLiteral("version")).toString(),
                                  QDir::toNativeSeparators(info.value(QStringLiteral("path")).toString()));
    text += lfs.isEmpty() ? tr(", without git-lfs") : tr(", git-lfs %1").arg(lfs);
    if (!info.value(QStringLiteral("supported")).toBool()) {
      text += QLatin1Char(' ') + tr("(older than %1, which Mitcad is tested with)")
                                     .arg(info.value(QStringLiteral("minimum")).toString());
    }
  }
  m_found->setText(text);
  qInfo().noquote() << QStringLiteral("Preferences: version control: %1").arg(text);
}

RemoteSettings RemotePreferencesBox::chosen() const {
  RemoteSettings settings = m_settings;
  settings.git = QDir::fromNativeSeparators(m_git->text().trimmed());
  settings.checkMinutes = m_minutes->value();
  settings.autoPush = m_push->isChecked();
  return settings;
}

} // namespace mitcad
