// SPDX-License-Identifier: MIT
#pragma once

// The addresses of Cloud projects (mitcad#89): the git services the Cloud
// section of New Project, Open from Cloud and Project Settings knows, how
// an address is composed of an account, a repository and the way to
// connect, and read back from one typed; the services' pages for a new
// repository and for SSH keys. Plain Qt Core, without the network (the
// application's unit tests check it).

#include <optional>

#include <QString>

namespace mitcad {

// The services of the Cloud section, in the order the list shows them.
enum class CloudService { GitHub, GitLab, Forgejo, SharedFolder, Other };

// How a known service is reached.
enum class CloudConnect { Https, Ssh };

// What the Cloud section's fields hold.
struct CloudAddress {
  CloudService service = CloudService::GitHub;
  CloudConnect connect = CloudConnect::Https;
  QString server;     // Forgejo / Gitea: the host (codeberg.org)
  QString account;    // the user or organisation
  QString repository; // without ".git"
  QString folder;     // Shared folder
  QString other;      // Other address: as typed

  // The address git is given: composed for a known service, the folder or
  // the typed address otherwise; empty while a part is missing.
  QString url() const;
  // The repository's web page (https://host/account/repository), empty
  // when there is none.
  QString webPage() const;
  // The service's page for a new repository, filled in where the service
  // takes parameters (GitHub), else empty.
  QString newRepositoryPage() const;
  // The service's page for the account's SSH keys (or how to make one).
  QString sshKeysPage() const;
};

// The service's name in the list ("Forgejo / Gitea").
QString cloudServiceName(CloudService service);
// The host of a known service ("github.com"; Forgejo: the server given).
QString cloudServiceHost(CloudService service, const QString& server = QString());

// An address typed or read from a project, as the fields: a known host's
// https:// or SSH address as that service (the account and repository from
// its path), a local path or file:// address as a shared folder, anything
// else as another address. `forgejoServer`: the server the Forgejo fields
// hold, whose addresses are then read as Forgejo's.
CloudAddress parseCloudAddress(const QString& url, const QString& forgejoServer = QString());

// The repository's name a folder's name suggests: lowercase, spaces and
// other characters a repository's name cannot have as '-' ("Robot arm" ->
// "robot-arm").
QString repositoryNameFor(const QString& folderName);

// The repository's name in an address: its last part without ".git"
// ("bracket" of git@github.com:me/bracket.git), for a folder's name.
QString repositoryNameOf(const QString& url);

// The host of an SSH or web address (git@host:owner/repo.git,
// ssh://git@host/..., https://host/...); empty for a folder.
QString addressHost(const QString& url);

// The repository's web page of an SSH or web address
// (https://host/owner/repo), else empty.
QString addressWebPage(const QString& url);

// Whether an address uses SSH (git@host:path, ssh://).
bool isSshAddress(const QString& url);

// Whether an address holds a password or a token (https://user:token@host),
// which Mitcad never gives to git.
bool addressHasPassword(const QString& url);

// Whether two addresses name the same repository: the same host and path
// (without ".git", a trailing '/', case of the host), or the same folder.
bool sameRepository(const QString& a, const QString& b);

} // namespace mitcad
