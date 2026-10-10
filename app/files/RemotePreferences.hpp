// SPDX-License-Identifier: MIT
#pragma once

// The Cloud page of Preferences (P12 remote, mitcad#89): the git program
// that Cloud projects use (found automatically, or chosen), with the
// version found; the default author of versions, used when git has none;
// the defaults of projects for sending each saved version at once and for
// how often the remote is checked for newer versions; whether live updates
// (MQTT) are allowed, and the broker New Project offers; the known brokers
// (the trust question's answers, LiveBrokers.hpp, with the user signed in
// as): Sign Out removes a broker's credentials from the keychain, Forget its
// answer too, both at once.

#include <QGroupBox>

#include "../framework/AppSettings.hpp"

class QCheckBox;
class QLabel;
class QLineEdit;
class QPushButton;
class QSpinBox;
class QTreeWidget;

namespace mitcad {

class RemotePreferencesBox : public QGroupBox {
  Q_OBJECT

public:
  explicit RemotePreferencesBox(QWidget* parent = nullptr);

  // The settings with the box's choices, to save when Preferences is
  // accepted.
  RemoteSettings chosen() const;
  // The default author with the box's choices.
  VersionSettings chosenAuthor() const;

private:
  // Shows the git program the field names (or the one found) and its
  // version.
  void detect();
  // Lists the known brokers (users from this session or the keychain).
  void listBrokers();
  void signOut();
  void forget();

  RemoteSettings m_settings;
  VersionSettings m_author;
  QLineEdit* m_git = nullptr;
  QLabel* m_found = nullptr;
  QLineEdit* m_name = nullptr;
  QLineEdit* m_email = nullptr;
  QSpinBox* m_minutes = nullptr;
  QCheckBox* m_push = nullptr;
  QCheckBox* m_live = nullptr;
  QLineEdit* m_broker = nullptr;
  QTreeWidget* m_brokers = nullptr;
  QPushButton* m_signOut = nullptr;
  QPushButton* m_forget = nullptr;
};

} // namespace mitcad
