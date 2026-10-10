// SPDX-License-Identifier: MIT
#include "CloudAddress.hpp"

#include <QCoreApplication>
#include <QDir>
#include <QUrl>
#include <QUrlQuery>

namespace mitcad {
namespace {

QString tr(const char* text) { return QCoreApplication::translate("CloudAddress", text); }

// The host, port and path of an SSH or web address: [user@]host:path (git's
// short form for SSH) or scheme://[user@]host[:port]/path; nothing for a
// folder or a file:// address.
struct HostPath {
  QString host;
  QString path;
  bool ssh = false;
};

std::optional<HostPath> hostPath(const QString& url) {
  const QString text = url.trimmed();
  if (text.contains(QStringLiteral("://"))) {
    const QUrl parsed(text);
    const QString scheme = parsed.scheme().toLower();
    if (!parsed.isValid() || parsed.host().isEmpty() ||
        (scheme != QLatin1String("ssh") && scheme != QLatin1String("https") && scheme != QLatin1String("http"))) {
      return std::nullopt;
    }
    return HostPath{parsed.host().toLower(), parsed.path(), scheme == QLatin1String("ssh")};
  }
  // git@host:owner/repo.git; not C:\folder (a drive letter) nor a path with
  // a colon further on.
  const qsizetype colon = text.indexOf(QLatin1Char(':'));
  if (colon <= 1) {
    return std::nullopt;
  }
  const QString before = text.left(colon);
  if (before.contains(QLatin1Char('/')) || before.contains(QLatin1Char('\\')) || before.contains(QLatin1Char(' '))) {
    return std::nullopt;
  }
  return HostPath{before.section(QLatin1Char('@'), -1).toLower(), text.mid(colon + 1), true};
}

// A path without its slashes around it and its ".git".
QString repositoryPath(QString path) {
  path.replace(QLatin1Char('\\'), QLatin1Char('/'));
  while (path.startsWith(QLatin1Char('/'))) {
    path.remove(0, 1);
  }
  while (path.endsWith(QLatin1Char('/'))) {
    path.chop(1);
  }
  if (path.endsWith(QLatin1String(".git"))) {
    path.chop(4);
  }
  return path;
}

bool isLocalPath(const QString& text) {
  if (text.startsWith(QLatin1String("file://"))) {
    return true;
  }
  if (text.startsWith(QLatin1Char('/')) || text.startsWith(QLatin1Char('\\')) || text.startsWith(QLatin1Char('~')) ||
      text.startsWith(QLatin1Char('.'))) {
    return true;
  }
  // C:\folder or C:/folder.
  return text.size() >= 3 && text.at(0).isLetter() && text.at(1) == QLatin1Char(':') &&
         (text.at(2) == QLatin1Char('\\') || text.at(2) == QLatin1Char('/'));
}

QString localPath(const QString& text) {
  if (text.startsWith(QLatin1String("file://"))) {
    return QUrl(text).toLocalFile();
  }
  return text;
}

} // namespace

QString cloudServiceName(CloudService service) {
  switch (service) {
  case CloudService::GitHub:
    return QStringLiteral("GitHub");
  case CloudService::GitLab:
    return QStringLiteral("GitLab");
  case CloudService::Forgejo:
    return QStringLiteral("Forgejo / Gitea");
  case CloudService::SharedFolder:
    return tr("Shared folder");
  case CloudService::Other:
    break;
  }
  return tr("Other address");
}

QString cloudServiceHost(CloudService service, const QString& server) {
  switch (service) {
  case CloudService::GitHub:
    return QStringLiteral("github.com");
  case CloudService::GitLab:
    return QStringLiteral("gitlab.com");
  case CloudService::Forgejo:
    return server.trimmed().toLower();
  case CloudService::SharedFolder:
  case CloudService::Other:
    break;
  }
  return {};
}

QString CloudAddress::url() const {
  switch (service) {
  case CloudService::GitHub:
  case CloudService::GitLab:
  case CloudService::Forgejo: {
    const QString host = cloudServiceHost(service, server);
    const QString owner = account.trimmed();
    const QString name = repository.trimmed();
    if (host.isEmpty() || owner.isEmpty() || name.isEmpty()) {
      return {};
    }
    return connect == CloudConnect::Ssh ? QStringLiteral("git@%1:%2/%3.git").arg(host, owner, name)
                                        : QStringLiteral("https://%1/%2/%3.git").arg(host, owner, name);
  }
  case CloudService::SharedFolder:
    // A Windows path typed anywhere ('\'), as git takes it ('/').
    return folder.trimmed().isEmpty() ? QString() : QDir::cleanPath(QString(folder.trimmed()).replace(QLatin1Char('\\'), QLatin1Char('/')));
  case CloudService::Other:
    break;
  }
  return other.trimmed();
}

QString CloudAddress::webPage() const {
  switch (service) {
  case CloudService::GitHub:
  case CloudService::GitLab:
  case CloudService::Forgejo: {
    const QString host = cloudServiceHost(service, server);
    if (host.isEmpty() || account.trimmed().isEmpty() || repository.trimmed().isEmpty()) {
      return {};
    }
    return QStringLiteral("https://%1/%2/%3").arg(host, account.trimmed(), repository.trimmed());
  }
  case CloudService::SharedFolder:
    return {};
  case CloudService::Other:
    break;
  }
  return addressWebPage(other);
}

QString CloudAddress::newRepositoryPage() const {
  switch (service) {
  case CloudService::GitHub: {
    // GitHub's form takes the owner, the name and the visibility.
    QUrl page(QStringLiteral("https://github.com/new"));
    QUrlQuery query;
    if (!account.trimmed().isEmpty()) {
      query.addQueryItem(QStringLiteral("owner"), account.trimmed());
    }
    if (!repository.trimmed().isEmpty()) {
      query.addQueryItem(QStringLiteral("name"), repository.trimmed());
    }
    query.addQueryItem(QStringLiteral("visibility"), QStringLiteral("private"));
    page.setQuery(query);
    return page.toString();
  }
  case CloudService::GitLab:
    return QStringLiteral("https://gitlab.com/projects/new#blank_project");
  case CloudService::Forgejo: {
    const QString host = cloudServiceHost(service, server);
    return host.isEmpty() ? QString() : QStringLiteral("https://%1/repo/create").arg(host);
  }
  case CloudService::SharedFolder:
  case CloudService::Other:
    break;
  }
  return {};
}

QString CloudAddress::sshKeysPage() const {
  switch (service) {
  case CloudService::GitHub:
    return QStringLiteral("https://github.com/settings/keys");
  case CloudService::GitLab:
    return QStringLiteral("https://gitlab.com/-/user_settings/ssh_keys");
  case CloudService::Forgejo: {
    const QString host = cloudServiceHost(service, server);
    if (!host.isEmpty()) {
      return QStringLiteral("https://%1/user/settings/keys").arg(host);
    }
    break;
  }
  case CloudService::SharedFolder:
  case CloudService::Other:
    break;
  }
  return QStringLiteral("https://git-scm.com/book/en/v2/Git-on-the-Server-Generating-Your-SSH-Public-Key");
}

CloudAddress parseCloudAddress(const QString& url, const QString& forgejoServer) {
  CloudAddress address;
  const QString text = url.trimmed();
  if (text.isEmpty()) {
    return address;
  }
  if (isLocalPath(text)) {
    address.service = CloudService::SharedFolder;
    address.folder = QDir::toNativeSeparators(localPath(text));
    return address;
  }
  address.service = CloudService::Other;
  address.other = text;
  const std::optional<HostPath> parsed = hostPath(text);
  if (!parsed) {
    return address;
  }
  const QString path = repositoryPath(parsed->path);
  const QStringList parts = path.split(QLatin1Char('/'));
  // Known services' addresses are <account>/<repository> (GitLab's groups
  // can nest: those stay another address).
  if (parts.size() != 2 || parts.at(0).isEmpty() || parts.at(1).isEmpty() ||
      parts.at(0).startsWith(QLatin1Char('~'))) {
    return address;
  }
  // An https:// address with a port or a user, or ssh:// with a port, is
  // not one the fields compose.
  if (text.contains(QStringLiteral("://"))) {
    const QUrl parsedUrl(text);
    if (parsedUrl.port() != -1 || (!parsed->ssh && !parsedUrl.userName().isEmpty())) {
      return address;
    }
  } else if (!text.startsWith(QLatin1String("git@"))) {
    return address;
  }
  const QString host = parsed->host;
  if (host == QLatin1String("github.com")) {
    address.service = CloudService::GitHub;
  } else if (host == QLatin1String("gitlab.com")) {
    address.service = CloudService::GitLab;
  } else if (host == QLatin1String("codeberg.org") ||
             (!forgejoServer.trimmed().isEmpty() && host == forgejoServer.trimmed().toLower())) {
    address.service = CloudService::Forgejo;
    address.server = host;
  } else {
    return address;
  }
  address.other.clear();
  address.connect = parsed->ssh ? CloudConnect::Ssh : CloudConnect::Https;
  address.account = parts.at(0);
  address.repository = parts.at(1);
  return address;
}

QString repositoryNameFor(const QString& folderName) {
  QString name;
  bool dash = false;
  for (const QChar c : folderName.trimmed().toLower()) {
    if ((c >= QLatin1Char('a') && c <= QLatin1Char('z')) || (c >= QLatin1Char('0') && c <= QLatin1Char('9')) ||
        c == QLatin1Char('_') || c == QLatin1Char('.')) {
      name += c;
      dash = false;
    } else if (!dash && !name.isEmpty()) {
      name += QLatin1Char('-');
      dash = true;
    }
  }
  while (name.endsWith(QLatin1Char('-')) || name.endsWith(QLatin1Char('.'))) {
    name.chop(1);
  }
  return name;
}

QString repositoryNameOf(const QString& url) {
  QString path = url.trimmed();
  if (const std::optional<HostPath> parsed = hostPath(path)) {
    path = parsed->path;
  } else if (path.startsWith(QLatin1String("file://"))) {
    path = QUrl(path).toLocalFile();
  }
  return repositoryPath(path).section(QLatin1Char('/'), -1);
}

QString addressHost(const QString& url) {
  const std::optional<HostPath> parsed = hostPath(url);
  return parsed ? parsed->host : QString();
}

QString addressWebPage(const QString& url) {
  const std::optional<HostPath> parsed = hostPath(url);
  if (!parsed) {
    return {};
  }
  const QString path = repositoryPath(parsed->path);
  if (path.isEmpty() || path.startsWith(QLatin1Char('~'))) {
    return {};
  }
  return QStringLiteral("https://%1/%2").arg(parsed->host, path);
}

bool isSshAddress(const QString& url) {
  const std::optional<HostPath> parsed = hostPath(url);
  return parsed && parsed->ssh;
}

bool addressHasPassword(const QString& url) {
  const QString text = url.trimmed();
  return text.contains(QStringLiteral("://")) && !QUrl(text).password().isEmpty();
}

bool sameRepository(const QString& a, const QString& b) {
  const std::optional<HostPath> first = hostPath(a);
  const std::optional<HostPath> second = hostPath(b);
  if (first && second) {
    return first->host == second->host && repositoryPath(first->path) == repositoryPath(second->path);
  }
  if (first || second) {
    return false;
  }
  const auto folder = [](const QString& text) {
    return QDir::cleanPath(localPath(text.trimmed()).replace(QLatin1Char('\\'), QLatin1Char('/')));
  };
#ifdef _WIN32
  return folder(a).compare(folder(b), Qt::CaseInsensitive) == 0;
#else
  return folder(a) == folder(b);
#endif
}

} // namespace mitcad
