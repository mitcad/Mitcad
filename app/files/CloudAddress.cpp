// SPDX-License-Identifier: MIT
#include "CloudAddress.hpp"

#include <QCoreApplication>
#include <QDir>
#include <QStringList>
#include <QUrl>
#include <QUrlQuery>

namespace mitcad {
namespace {

// The file's texts in the context CloudAddress, where lupdate records them
// too (mitcad#1472).
struct Text {
  Q_DECLARE_TR_FUNCTIONS(CloudAddress)
};

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

// The owner as the address's path holds it: without spaces and slashes
// around it ("group/sub" of " /group/sub/ ").
QString ownerPath(const QString& owner) {
  QString path = owner.trimmed();
  while (path.startsWith(QLatin1Char('/'))) {
    path.remove(0, 1);
  }
  while (path.endsWith(QLatin1Char('/'))) {
    path.chop(1);
  }
  return path.trimmed();
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

// "git clone <address> [folder]" as copied from a page: the address.
// Surrounding quotes or <> are dropped.
QString withoutCommand(const QString& pasted) {
  QString text = pasted.trimmed();
  if (text.startsWith(QLatin1String("git clone "), Qt::CaseInsensitive)) {
    text.clear();
    for (const QString& word : pasted.trimmed().mid(10).split(QLatin1Char(' '), Qt::SkipEmptyParts)) {
      if (!word.startsWith(QLatin1Char('-'))) {
        text = word;
        break;
      }
    }
  }
  if (text.size() >= 2) {
    const QChar first = text.front();
    const QChar last = text.back();
    if ((first == QLatin1Char('"') && last == QLatin1Char('"')) || (first == QLatin1Char('\'') && last == QLatin1Char('\'')) ||
        (first == QLatin1Char('<') && last == QLatin1Char('>'))) {
      text = text.mid(1, text.size() - 2).trimmed();
    }
  }
  return text;
}

// host/owner/repository without a scheme (git.example.org/team/parts.git):
// a host name with a dot (or localhost), then a path; no user, port or
// colon, which would make it another form.
bool isBareHostPath(const QString& text) {
  if (text.contains(QStringLiteral("://")) || text.contains(QLatin1Char(' ')) || text.contains(QLatin1Char('\\')) ||
      text.contains(QLatin1Char('@')) || text.contains(QLatin1Char(':'))) {
    return false;
  }
  const qsizetype slash = text.indexOf(QLatin1Char('/'));
  if (slash <= 0) {
    return false;
  }
  const QString host = text.left(slash);
  if (host != QLatin1String("localhost") && (!host.contains(QLatin1Char('.')) || host.startsWith(QLatin1Char('.')) ||
                                             host.endsWith(QLatin1Char('.')))) {
    return false;
  }
  for (const QChar c : host) {
    if (!(c.isLetterOrNumber() || c == QLatin1Char('.') || c == QLatin1Char('-') || c == QLatin1Char('_'))) {
      return false;
    }
  }
  return true;
}

// What was typed or pasted as an address, ready to be read: the command
// and quotes taken off, a bare host/path given https://.
QString pastedAddress(const QString& typed) {
  const QString text = withoutCommand(typed);
  if (!isLocalPath(text) && isBareHostPath(text)) {
    return QStringLiteral("https://") + text;
  }
  return text;
}

// The words after owner/repository in the address of a repository's page
// in a browser (https://host/owner/repo/src/branch/main).
bool isPageWord(const QString& word) {
  static const char* const words[] = {"src",     "tree",     "blob",    "raw",  "commits",       "commit",  "issues",
                                      "pulls",   "pull",     "releases", "wiki", "actions",       "branches", "tags",
                                      "compare", "activity", "graphs",  "settings", "merge_requests"};
  for (const char* candidate : words) {
    if (word == QLatin1String(candidate)) {
      return true;
    }
  }
  return false;
}

bool validHostName(const QString& host) {
  if (host.isEmpty()) {
    return false;
  }
  for (const QChar c : host) {
    if (!(c.isLetterOrNumber() || c == QLatin1Char('.') || c == QLatin1Char('-') || c == QLatin1Char('_') ||
          c == QLatin1Char('[') || c == QLatin1Char(']') || c == QLatin1Char(':'))) {
      return false;
    }
  }
  return true;
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
    return Text::tr("Shared folder");
  case CloudService::Other:
    break;
  }
  return Text::tr("Other address");
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
    const QString path = ownerPath(owner);
    const QString name = repository.trimmed();
    if (host.isEmpty() || path.isEmpty() || name.isEmpty()) {
      return {};
    }
    return connect == CloudConnect::Ssh ? QStringLiteral("git@%1:%2/%3.git").arg(host, path, name)
                                        : QStringLiteral("https://%1/%2/%3.git").arg(host, path, name);
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
    const QString path = ownerPath(owner);
    if (host.isEmpty() || path.isEmpty() || repository.trimmed().isEmpty()) {
      return {};
    }
    return QStringLiteral("https://%1/%2/%3").arg(host, path, repository.trimmed());
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
    // GitHub's form takes the owner (a user or an organisation), the name
    // and the visibility.
    QUrl page(QStringLiteral("https://github.com/new"));
    QUrlQuery query;
    if (!ownerPath(owner).isEmpty()) {
      query.addQueryItem(QStringLiteral("owner"), ownerPath(owner));
    }
    if (!repository.trimmed().isEmpty()) {
      query.addQueryItem(QStringLiteral("name"), repository.trimmed());
    }
    query.addQueryItem(QStringLiteral("visibility"), QStringLiteral("private"));
    page.setQuery(query);
    return page.toString();
  }
  case CloudService::GitLab:
    // GitLab's and Forgejo's forms take the owner as a numeric namespace id:
    // it is chosen on the page.
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
  QString text = pastedAddress(url);
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
  const QString host = parsed->host;
  QStringList parts = repositoryPath(parsed->path).split(QLatin1Char('/'));
  // The address of a repository's page in a browser
  // (https://host/owner/repo/src/branch/main, GitLab's .../-/tree/main) is
  // the repository's: the page part is dropped.
  bool gitlabPage = false;
  if (!parsed->ssh && text.contains(QStringLiteral("://"))) {
    qsizetype cut = -1;
    const qsizetype dash = parts.indexOf(QStringLiteral("-"));
    if (dash > 0) {
      cut = dash;
      gitlabPage = host != QLatin1String("gitlab.com");
    } else if (host != QLatin1String("gitlab.com") && parts.size() > 2 && isPageWord(parts.at(2))) {
      cut = 2;
    }
    if (cut > 0) {
      parts = parts.mid(0, cut);
      const QUrl pageUrl(text);
      address.other = QStringLiteral("%1://%2/%3").arg(pageUrl.scheme(), pageUrl.authority(), parts.join(QLatin1Char('/')));
      text = address.other;
    }
  }
  // Known services' addresses are <owner>/<repository>; on GitLab the owner
  // can be a group with subgroups (group/sub/repository). An empty part, or
  // one starting with '~' (a user's home on a server), stays another
  // address.
  if (parts.size() < 2) {
    return address;
  }
  for (const QString& part : parts) {
    if (part.isEmpty() || part.startsWith(QLatin1Char('~'))) {
      return address;
    }
  }
  // An https:// address with a port or a user, or ssh:// with a port or
  // another user than git (the fields compose git@), is not one the fields
  // compose.
  if (text.contains(QStringLiteral("://"))) {
    const QUrl parsedUrl(text);
    if (parsedUrl.port() != -1 || (!parsed->ssh && !parsedUrl.userName().isEmpty()) ||
        (parsed->ssh && parsedUrl.userName() != QLatin1String("git"))) {
      return address;
    }
  } else if (!text.startsWith(QLatin1String("git@"))) {
    return address;
  }
  CloudService service = CloudService::Other;
  if (host == QLatin1String("github.com")) {
    service = CloudService::GitHub;
  } else if (host == QLatin1String("gitlab.com")) {
    service = CloudService::GitLab;
  } else if (host == QLatin1String("codeberg.org") ||
             (!forgejoServer.trimmed().isEmpty() && host == forgejoServer.trimmed().toLower())) {
    service = CloudService::Forgejo;
  } else if (parts.size() == 2 && !gitlabPage) {
    // Any other server (a self-hosted one): the address is owner/repository
    // on it, as Forgejo and Gitea (and GitLab) have it; the fields then
    // make the same address. Nothing is asked of the Server field first.
    service = CloudService::Forgejo;
  } else {
    return address;
  }
  // GitHub and Forgejo have one owner level (no nested organisations).
  if (service != CloudService::GitLab && parts.size() != 2) {
    return address;
  }
  address.service = service;
  if (service == CloudService::Forgejo) {
    address.server = host;
  }
  address.other.clear();
  address.connect = parsed->ssh ? CloudConnect::Ssh : CloudConnect::Https;
  address.owner = parts.mid(0, parts.size() - 1).join(QLatin1Char('/'));
  address.repository = parts.constLast();
  return address;
}

QString repositoryAddressProblem(const QString& typed) {
  const QString text = pastedAddress(typed);
  if (text.isEmpty() || isLocalPath(text)) {
    return {};
  }
  const QString bad = Text::tr("This is not a valid repository address: %1").arg(typed.trimmed());
  if (text.contains(QStringLiteral("://"))) {
    const QUrl parsed(text);
    const QString scheme = parsed.scheme().toLower();
    const bool known = scheme == QLatin1String("https") || scheme == QLatin1String("http") ||
                       scheme == QLatin1String("ssh") || scheme == QLatin1String("git") ||
                       scheme == QLatin1String("git+ssh") || scheme == QLatin1String("ssh+git");
    if (!parsed.isValid() || !known || !validHostName(parsed.host()) || repositoryPath(parsed.path()).isEmpty()) {
      return bad;
    }
    return {};
  }
  // [user@]host:path, as git reads it; a '/' before the first ':' makes it
  // a path.
  const std::optional<HostPath> parsed = hostPath(text);
  if (!parsed || !validHostName(parsed->host) || parsed->path.trimmed().isEmpty()) {
    return bad;
  }
  return {};
}

bool looksLikeRepositoryAddress(const QString& typed, bool bareHost) {
  const QString text = withoutCommand(typed);
  if (isLocalPath(text)) {
    return false;
  }
  if (text.contains(QStringLiteral("://")) || text.contains(QLatin1Char('@'))) {
    const std::optional<HostPath> parsed = hostPath(text);
    return parsed && repositoryPath(parsed->path).contains(QLatin1Char('/'));
  }
  if (bareHost && isBareHostPath(text)) {
    return repositoryPath(text.mid(text.indexOf(QLatin1Char('/')))).contains(QLatin1Char('/'));
  }
  return false;
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

QString repositoryAddressOf(const QString& url) {
  const CloudAddress address = parseCloudAddress(url);
  switch (address.service) {
  case CloudService::GitHub:
  case CloudService::GitLab:
  case CloudService::Forgejo: {
    const QString host = cloudServiceHost(address.service, address.server);
    if (!host.isEmpty() && !address.owner.isEmpty() && !address.repository.isEmpty()) {
      return QStringLiteral("%1/%2/%3").arg(host, address.owner, address.repository);
    }
    break;
  }
  case CloudService::SharedFolder:
    // The folder's path as this computer writes it.
    return QDir::toNativeSeparators(QDir::cleanPath(QDir::fromNativeSeparators(address.folder)));
  case CloudService::Other:
    break;
  }
  // Another server: its host and the repository's path, without the user,
  // the port and ".git".
  if (const std::optional<HostPath> parsed = hostPath(url)) {
    const QString path = repositoryPath(parsed->path);
    return path.isEmpty() ? parsed->host : QStringLiteral("%1/%2").arg(parsed->host, path);
  }
  // Anything else: as given, without its scheme.
  QString text = url.trimmed();
  const qsizetype scheme = text.indexOf(QStringLiteral("://"));
  if (scheme >= 0) {
    text = text.mid(scheme + 3);
  }
  return repositoryPath(text);
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
