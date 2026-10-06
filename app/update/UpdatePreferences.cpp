// SPDX-License-Identifier: MIT
#include "UpdatePreferences.hpp"

#include <QCheckBox>
#include <QComboBox>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QLabel>

namespace mitcad {

UpdatePreferencesBox::UpdatePreferencesBox(QWidget* parent)
    : QGroupBox(tr("Updates"), parent), m_settings(UpdateSettings::load()) {
  auto* form = new QFormLayout(this);
  m_automatic = new QCheckBox(tr("Check for &updates at start-up, once a day"));
  m_automatic->setChecked(m_settings.automatic);
  m_automatic->setToolTip(tr("Asks Mitcad's release page for a newer version, telling it nothing but this "
                             "version, the platform and the channel; nothing is installed without asking"));
  m_channel = new QComboBox;
  m_channel->addItem(tr("Releases"), static_cast<int>(UpdateSettings::Channel::Stable));
  m_channel->addItem(tr("Pre-releases too"), static_cast<int>(UpdateSettings::Channel::Prerelease));
  m_channel->setCurrentIndex(m_channel->findData(static_cast<int>(m_settings.channel)));
  m_channel->setToolTip(tr("Pre-releases come before a release, for trying it out"));
  auto* channelLabel = new QLabel(tr("C&hannel:"));
  channelLabel->setBuddy(m_channel);
  auto* row = new QHBoxLayout;
  row->addWidget(m_automatic);
  row->addStretch(1);
  row->addWidget(channelLabel);
  row->addWidget(m_channel);
  form->addRow(row);
  if (updatesDisabledByAdministrator()) {
    m_automatic->setChecked(false);
    m_automatic->setEnabled(false);
    m_channel->setEnabled(false);
    form->addRow(new QLabel(tr("Update checks are turned off by your administrator.")));
  }
}

UpdateSettings UpdatePreferencesBox::chosen() const {
  // As they are now: a check may have finished meanwhile.
  UpdateSettings settings = UpdateSettings::load();
  if (m_automatic->isEnabled()) {
    settings.automatic = m_automatic->isChecked();
    settings.channel = static_cast<UpdateSettings::Channel>(m_channel->currentData().toInt());
  }
  return settings;
}

} // namespace mitcad
