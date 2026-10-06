// SPDX-License-Identifier: MIT
#pragma once

// The dialogs of remote repositories (P12 remote): connecting a project to
// a remote, opening a project from one, choosing what to keep of files
// changed both here and on the remote when a sync stops for them, and the
// remote's settings; and what they know of git hosts' addresses. Mitcad
// never asks for a password or a token: signing in is git's (SSH keys, a
// credential helper), and a repository is made on the service's web page.

#include <functional>
#include <optional>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>

class QWidget;

namespace mitcad {

// The git hosts Connect gives help for: their pages for an SSH key and a
// new repository differ, nothing else does.
enum class RemoteService { GitHub, Forgejo, GitLab, Other };

// The service an address's host names (github.com, gitlab.com, codeberg.org),
// else nothing.
std::optional<RemoteService> serviceOf(const QString& url);
// The host of an SSH address (git@host:owner/repo.git, ssh://git@host/...)
// or a web one (https://host/...); empty for a folder.
QString remoteHost(const QString& url);
// The repository's web page for an SSH or web address of a host
// (https://host/owner/repo), else empty.
QString remoteWebPage(const QString& url);
// The repository's name, the last part of its address without ".git"
// ("bracket" of git@github.com:me/bracket.git), for a folder's name.
QString repositoryName(const QString& url);
// The service's page for a new repository named `name` (GitHub's form
// filled in, private), at the host of `url` where the service has more
// than one; empty when it is not known.
QString newRepositoryPage(RemoteService service, const QString& url, const QString& name);
// How to add an SSH key to an account of the service.
QString sshKeyHelpPage(RemoteService service);

// File > Connect Project to Remote: the address of the remote repository
// for project `project` (its folder's name), `url` suggested. `author`
// ("Name <email>") goes into every version, which the dialog says. Nothing
// when cancelled.
std::optional<QString> askConnectRemote(QWidget* parent, const QString& project, const QString& author,
                                        const QString& url);

// File > Open Project from Remote: the address and the new folder (in
// `location`, by default Documents/Mitcad, named after the repository).
struct RemoteOpening {
  QString url;
  QString folder;
};
std::optional<RemoteOpening> askOpenRemote(QWidget* parent, const QString& location, const RemoteOpening& previous);

// Resolve Sync Conflicts: for each file changed both here and on the
// remote (the sync's `conflicts`), keep mine, take theirs or save mine as a
// copy (the default, which loses nothing). `compare` gives the comparison
// of the remote's file with the project's for a conflict whose both sides
// have one. Returns {path: "mine" | "theirs" | "copy"}, nothing when
// cancelled (nothing changes then).
std::optional<QJsonObject> askResolveConflicts(QWidget* parent, const QJsonArray& conflicts,
                                               const std::function<QString(const QJsonObject& conflict)>& compare);

// File > Remote Settings: the remote (`remote_info`) with Change Address,
// Disconnect and Open in Browser.
enum class RemoteSettingsChoice { None, ChangeAddress, Disconnect, OpenInBrowser };
RemoteSettingsChoice askRemoteSettings(QWidget* parent, const QString& project, const QJsonObject& info);

} // namespace mitcad
