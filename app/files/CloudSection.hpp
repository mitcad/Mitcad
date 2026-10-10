// SPDX-License-Identifier: MIT
#pragma once

// The Cloud section of New Project, Open from Cloud and Project Settings
// (mitcad#89): the service (GitHub, GitLab, Forgejo / Gitea with its
// server, a shared folder, another address), the account and the
// repository with the way to connect, or a folder, or an address; the
// address they make, which can also be typed (it fills the fields back);
// Create Repository in Browser; and the check of the address, while typing
// after a short pause and again when the dialog gets the focus back, on a
// thread of its own (RemoteTask). What the check found is the owner's to
// say (`describe`); failures of signing in and of the server's identity get
// their fixes here: Copy Public Key and the SSH keys page, how to set up a
// credential helper, Trust This Server (section 10 of mitcad#89). Mitcad
// never asks for, keeps or passes git passwords or tokens.

#include <functional>

#include <QDateTime>
#include <QJsonObject>
#include <QPointer>
#include <QString>
#include <QWidget>

#include "CloudAddress.hpp"

class QButtonGroup;
class QComboBox;
class QFormLayout;
class QHBoxLayout;
class QLabel;
class QLineEdit;
class QPushButton;
class QRadioButton;
class QTimer;

namespace mitcad {

class RemoteTask;

class CloudSection : public QWidget {
  Q_OBJECT

public:
  // Makes the task that checks an address: check_remote, or a project's
  // remote_check (which also says whether the remote shares its history).
  using Checker = std::function<RemoteTask*(const QString& url, QObject* parent)>;
  // What a check that did not fail found, for the status line; `ok` false
  // makes it a problem (red). Logged as "Cloud check <url>: <text>".
  using Describe = std::function<QString(const QJsonObject& answer, bool& ok)>;

  CloudSection(Checker checker, Describe describe, QWidget* parent = nullptr);
  ~CloudSection() override;

  // The fields from an address (a project's, or one given to the dialog).
  void setAddress(const QString& url);
  // The repository's name follows the folder's ("Robot arm" -> robot-arm)
  // until it is typed.
  void setFolderName(const QString& folderName);
  // The way to connect a known service by default: HTTPS when git has a
  // credential helper, else SSH.
  void setDefaultConnect(CloudConnect connect);

  QString url() const;
  CloudAddress address() const { return m_address; }
  // The latest check's answer for url() (empty before its first answer, or
  // after the address changed); a check of the same address again keeps
  // the earlier answer until its own comes.
  const QJsonObject& answer() const { return m_answer; }
  bool hasAnswer() const { return !m_answer.isEmpty(); }
  // The answer had no error and the owner's description was not a problem.
  bool answerOk() const { return m_answerOk; }
  // The address names a shared folder that is new or empty: Mitcad makes a
  // repository there (init_bare) before it uses it.
  bool needsBareRepository() const { return m_needsBare; }
  // Checks the address again (the dialog got the focus back), unless it was
  // checked a moment ago.
  void recheck();
  // Checks the address now.
  void checkNow();
  // The status line's text (for the owner's log).
  QString statusText() const;
  // Sets the status line (the owner's own message), `problem` in red.
  void setStatus(const QString& text, bool problem);
  // Ends a running check (the dialog closes or creates).
  void stopCheck();
  // The first field to type into.
  QWidget* firstField() const;

signals:
  // The address changed (typed, or a field of it).
  void addressChanged(const QString& url);
  // A check's answer came (also a failure), after the status line shows it.
  void checked();

private:
  void build();
  void fieldsChanged();
  void addressEdited(const QString& text);
  void showFields();
  void scheduleCheck();
  void startCheck();
  void finishCheck(const QString& url, const QJsonObject& answer);
  void showFixes(const QString& cls);
  void copyPublicKey();
  void trustServer();
  void openPage(const QString& page, const QString& what);

  Checker m_checker;
  Describe m_describe;
  CloudAddress m_address;
  bool m_repositoryTyped = false;
  bool m_updating = false;
  QComboBox* m_service = nullptr;
  QLabel* m_serverLabel = nullptr;
  QLineEdit* m_server = nullptr;
  QLabel* m_accountLabel = nullptr;
  QWidget* m_accountRow = nullptr;
  QLineEdit* m_account = nullptr;
  QLineEdit* m_repository = nullptr;
  QLabel* m_connectLabel = nullptr;
  QWidget* m_connectRow = nullptr;
  QRadioButton* m_https = nullptr;
  QRadioButton* m_ssh = nullptr;
  QLabel* m_folderLabel = nullptr;
  QWidget* m_folderRow = nullptr;
  QLineEdit* m_folder = nullptr;
  QLineEdit* m_url = nullptr;
  QPushButton* m_create = nullptr;
  QLabel* m_status = nullptr;
  QWidget* m_fixes = nullptr;
  QLabel* m_fixText = nullptr;
  QPushButton* m_copyKey = nullptr;
  QPushButton* m_keysPage = nullptr;
  QPushButton* m_trust = nullptr;
  QFormLayout* m_form = nullptr;
  QTimer* m_delay = nullptr;
  QPointer<RemoteTask> m_task;
  QString m_taskUrl;
  QJsonObject m_answer;
  QString m_answerUrl;
  bool m_answerOk = false;
  bool m_needsBare = false;
  QDateTime m_checkedAt;
};

// The server's identity (mitcad#89, section 10): its SSH host keys fetched
// (host_keys, a thread of its own while the dialog waits), their
// fingerprints, compared with those GitHub, GitLab and Codeberg publish;
// Trust This Server adds them to the user's known_hosts (trust_host_key).
// True when the server is trusted now.
bool askTrustServer(QWidget* parent, const QString& host, int port);

} // namespace mitcad
