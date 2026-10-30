// SPDX-License-Identifier: MIT
// Updates (mitcad#603): the download goes to a private, unpredictable
// folder on Windows as on macOS, the same code path on every platform; the
// Windows updater's command line carries the SHA-256 of the verified
// manifest and refuses one without; the updater's check of a file through
// a reader (hashAll, digestMatches) tells a changed file apart.
#include "update/UpdateInstaller.hpp"
#include "update/UpdaterChecks.hpp"

#include <cstddef>
#include <cstdio>
#include <initializer_list>
#include <string>
#include <vector>

#include <QByteArray>
#include <QByteArrayView>
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QFileDevice>
#include <QFileInfo>
#include <QIODevice>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>
#include <QtGlobal>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "update_installer.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

const QString kFolders = QStringLiteral("updates/downloadFolders");

QString text(const char* value) { return QString::fromLatin1(value); }

QString sha256Hex(const QByteArray& data) {
  return QString::fromLatin1(QCryptographicHash::hash(data, QCryptographicHash::Sha256).toHex());
}

std::vector<std::wstring> commandLine(const QStringList& arguments) {
  std::vector<std::wstring> line{L"mitcad-updater.exe"};
  for (const QString& argument : arguments) {
    line.push_back(argument.toStdWString());
  }
  return line;
}

// The command line with the value after `name` replaced, or with `name`
// and its value left out when `value` is null.
std::vector<std::wstring> withOption(std::vector<std::wstring> line, const wchar_t* name, const wchar_t* value) {
  for (std::size_t i = 1; i + 1 < line.size(); ++i) {
    if (line[i] == name) {
      if (value != nullptr) {
        line[i + 1] = value;
      } else {
        line.erase(line.begin() + static_cast<std::ptrdiff_t>(i), line.begin() + static_cast<std::ptrdiff_t>(i) + 2);
      }
      break;
    }
  }
  return line;
}

bool writeFile(const QString& path, const QByteArray& content) {
  QFile file(path);
  return file.open(QIODevice::WriteOnly | QIODevice::Truncate) && file.write(content) == content.size();
}

// The updater's check of a file, with Qt's reader and SHA-256 in place of
// the Windows handle and CNG.
bool fileMatches(const QString& path, const std::wstring& expected) {
  QFile file(path);
  if (!file.open(QIODevice::ReadOnly)) {
    return false;
  }
  QCryptographicHash hash(QCryptographicHash::Sha256);
  const bool read = mitcad::updater::hashAll(
      [&file](unsigned char* buffer, std::size_t capacity) -> long long {
        return static_cast<long long>(file.read(reinterpret_cast<char*>(buffer), static_cast<qint64>(capacity)));
      },
      [&hash](const unsigned char* data, std::size_t size) {
        hash.addData(QByteArrayView(reinterpret_cast<const char*>(data), static_cast<qsizetype>(size)));
        return true;
      });
  const QByteArray digest = hash.result();
  return read && mitcad::updater::digestMatches(reinterpret_cast<const unsigned char*>(digest.constData()),
                                                static_cast<std::size_t>(digest.size()), expected);
}

void privateFolderTests() {
  using namespace mitcad;
  Installation windows;
  windows.kind = Installation::Kind::WindowsInstaller;
  windows.location = text("C:/Program Files/Mitcad");
  windows.allUsers = true;
  Installation mac;
  mac.kind = Installation::Kind::MacBundle;
  mac.location = text("/Applications/Mitcad.app");

  // Windows: a folder of its own in the temp folder, with a random name,
  // not the fixed mitcad-update of earlier releases.
  const QFileInfo download(windows.downloadPath(text("1.2.3")));
  CHECK(download.fileName() == text("mitcad-1.2.3-windows-x64.exe"));
  const QString folder = download.absolutePath();
  const QString name = QFileInfo(folder).fileName();
  CHECK(QFileInfo(folder).isDir());
  CHECK(name.startsWith(text("mitcad-update-")) && name.size() > 14);
  CHECK(QDir::cleanPath(QFileInfo(folder).absolutePath()) == QDir::cleanPath(QDir::temp().absolutePath()));
#ifndef _WIN32
  // Owner-only (mkdtemp): nobody else can list it or put a file in it.
  const QFileDevice::Permissions others = QFileDevice::ReadGroup | QFileDevice::WriteGroup | QFileDevice::ExeGroup |
                                          QFileDevice::ReadOther | QFileDevice::WriteOther | QFileDevice::ExeOther;
  CHECK(!(QFile::permissions(folder) & others));
#endif
  // The same folder, the same code path, for macOS's disk image; one
  // folder per run.
  const QFileInfo image(mac.downloadPath(text("1.2.3")));
  CHECK(image.fileName() == text("Mitcad-1.2.3-macos-arm64.dmg"));
  CHECK(image.absolutePath() == folder);
  CHECK(QFileInfo(windows.downloadPath(text("1.2.4"))).absolutePath() == folder);
  // Windows remembers it, once, for a later start to remove.
  CHECK(QSettings().value(kFolders).toStringList() == QStringList{folder});

  // The updater copy left in it goes with it; a remembered path that is
  // not such a folder directly in the temp folder is never removed.
  CHECK(writeFile(QDir(folder).filePath(text("mitcad-updater.exe")), QByteArray("updater")));
  QTemporaryDir other(QDir::temp().filePath(text("mitcad-other-XXXXXX")));
  QTemporaryDir parent;
  CHECK(other.isValid() && parent.isValid());
  const QString nested = QDir(parent.path()).filePath(text("mitcad-update-nested"));
  CHECK(QDir().mkpath(nested));
  QSettings().setValue(kFolders, QStringList{folder, other.path(), nested, QString()});
  windows.removeLeftovers();
  CHECK(!QFileInfo::exists(folder));
  CHECK(QFileInfo::exists(other.path()));
  CHECK(QFileInfo::exists(nested));
  CHECK(!QSettings().contains(kFolders));

  // The next download gets a new folder: its name is never known before.
  const QString again = QFileInfo(windows.downloadPath(text("1.2.3"))).absolutePath();
  CHECK(again != folder);
  CHECK(QFileInfo(again).isDir());
  CHECK(QFileInfo(again).fileName().startsWith(text("mitcad-update-")));
  // An earlier run's folder (one this process did not make) is removed at
  // a later start, as this run's is.
  QTemporaryDir earlier(QDir::temp().filePath(text("mitcad-update-XXXXXX")));
  earlier.setAutoRemove(false);
  CHECK(earlier.isValid());
  CHECK(writeFile(QDir(earlier.path()).filePath(text("updater.log")), QByteArray("Starting mitcad.exe\r\n")));
  QSettings().setValue(kFolders, QStringList{earlier.path(), again});
  windows.removeLeftovers();
  CHECK(!QFileInfo::exists(earlier.path()));
  CHECK(!QFileInfo::exists(again));
  CHECK(!QSettings().contains(kFolders));

  // macOS keeps its folder to itself: no settings.
  const QString macFolder = QFileInfo(mac.downloadPath(text("1.2.3"))).absolutePath();
  CHECK(QFileInfo(macFolder).isDir());
  CHECK(!QSettings().contains(kFolders));
  mac.removeLeftovers();
  CHECK(!QFileInfo::exists(macFolder));

  // A Linux AppImage: the hidden file next to it, as before.
  QTemporaryDir apps;
  CHECK(apps.isValid());
  Installation appImage;
  appImage.kind = Installation::Kind::AppImage;
  appImage.location = QDir(apps.path()).filePath(text("Mitcad.AppImage"));
  CHECK(appImage.downloadPath(text("1.2.3")) == QDir(apps.path()).filePath(text(".Mitcad.AppImage.update")));
  // Announce only: no download.
  CHECK(Installation().downloadPath(text("1.2.3")).isEmpty());
}

void updaterArgumentTests() {
  using namespace mitcad;
  Installation windows;
  windows.kind = Installation::Kind::WindowsInstaller;
  windows.location = text("C:/Program Files/Mitcad");
  windows.allUsers = true;
  const QString hash = sha256Hex(QByteArray("installer"));
  const QString installer = text("C:/Temp/mitcad-update-a1B2c3/mitcad-1.2.3-windows-x64.exe");
  const QString log = text("C:/Temp/mitcad-update-a1B2c3/updater.log");
  const QStringList arguments = windowsUpdaterArguments(windows, installer, hash, log, 4242);

  // What Mitcad passes is what the updater reads.
  updater::Arguments parsed;
  std::wstring error;
  const std::vector<std::wstring> line = commandLine(arguments);
  CHECK(updater::parseArguments(line, parsed, error));
  CHECK(error.empty());
  CHECK(parsed.sha256 == hash.toStdWString());
  CHECK(parsed.installer == QDir::toNativeSeparators(installer).toStdWString());
  CHECK(parsed.wait == L"4242");
  CHECK(parsed.scope == L"all");
  CHECK(parsed.dir == QDir::toNativeSeparators(windows.location).toStdWString());
  CHECK(parsed.restart == QDir::toNativeSeparators(text("C:/Program Files/Mitcad/bin/mitcad.exe")).toStdWString());
  CHECK(parsed.log == QDir::toNativeSeparators(log).toStdWString());
  windows.allUsers = false;
  CHECK(updater::parseArguments(commandLine(windowsUpdaterArguments(windows, installer, hash, log, 1)), parsed, error));
  CHECK(parsed.scope == L"user");

  // No SHA-256, or one not as the manifest writes it: refused, with the
  // log still named so that the reason is written.
  const std::wstring upper = hash.toUpper().toStdWString();
  const std::wstring shorter = hash.left(63).toStdWString();
  const std::wstring longer = (hash + text("0")).toStdWString();
  const std::wstring notHex = (hash.left(63) + text("g")).toStdWString();
  for (const std::vector<std::wstring>& refused :
       {withOption(line, L"--sha256", nullptr), withOption(line, L"--sha256", L""),
        withOption(line, L"--sha256", upper.c_str()), withOption(line, L"--sha256", shorter.c_str()),
        withOption(line, L"--sha256", longer.c_str()), withOption(line, L"--sha256", notHex.c_str())}) {
    updater::Arguments partial;
    std::wstring reason;
    CHECK(!updater::parseArguments(refused, partial, reason));
    CHECK(reason.find(L"SHA-256") != std::wstring::npos);
    CHECK(partial.log == QDir::toNativeSeparators(log).toStdWString());
  }
  // --sha256 last, without its value.
  std::vector<std::wstring> last = withOption(line, L"--sha256", nullptr);
  last.push_back(L"--sha256");
  CHECK(!updater::parseArguments(last, parsed, error));
  // The other required values and the scope, as before.
  CHECK(!updater::parseArguments(withOption(line, L"--installer", nullptr), parsed, error));
  CHECK(!updater::parseArguments(withOption(line, L"--dir", nullptr), parsed, error));
  CHECK(!updater::parseArguments(withOption(line, L"--restart", nullptr), parsed, error));
  CHECK(!updater::parseArguments(withOption(line, L"--scope", L"everyone"), parsed, error));
  CHECK(updater::parseArguments(withOption(line, L"--scope", nullptr), parsed, error) && parsed.scope.empty());
  CHECK(updater::parseArguments(withOption(line, L"--wait", nullptr), parsed, error) && parsed.wait.empty());

  CHECK(updater::isSha256Hex(hash.toStdWString()));
  CHECK(!updater::isSha256Hex(upper));
  CHECK(!updater::isSha256Hex(std::wstring()));

  // Mitcad stages no Windows update without the hash: the updater would
  // refuse to run the installer.
  QString staged;
  CHECK(!stageUpdate(windows, installer, QString(), text("1.2.2"), text("1.2.3"), staged));
  CHECK(!staged.isEmpty());
  staged.clear();
  CHECK(!stageUpdate(windows, installer, hash.toUpper(), text("1.2.2"), text("1.2.3"), staged));
  CHECK(!staged.isEmpty());
}

void digestTests() {
  using namespace mitcad;
  QTemporaryDir directory;
  CHECK(directory.isValid());
  // Larger than one read of the updater's loop (64 KiB).
  QByteArray content(200000, '\0');
  for (int i = 0; i < content.size(); ++i) {
    content[i] = static_cast<char>((i * 31) % 251);
  }
  const QString path = QDir(directory.path()).filePath(text("mitcad-1.2.3-windows-x64.exe"));
  CHECK(writeFile(path, content));
  const std::wstring expected = sha256Hex(content).toStdWString();
  CHECK(fileMatches(path, expected));
  // One byte changed (what a replaced installer is): no match.
  QByteArray changed = content;
  changed[100000] = static_cast<char>(changed[100000] ^ 1);
  CHECK(writeFile(path, changed));
  CHECK(!fileMatches(path, expected));
  // One byte more: no match either.
  CHECK(writeFile(path, content + QByteArray(1, 'x')));
  CHECK(!fileMatches(path, expected));
  // No file: no match.
  CHECK(!fileMatches(QDir(directory.path()).filePath(text("missing.exe")), expected));
  // The empty file's known digest; the hash must be written as the
  // manifest writes it.
  const QString empty = QDir(directory.path()).filePath(text("empty"));
  CHECK(writeFile(empty, QByteArray()));
  CHECK(fileMatches(empty, L"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"));
  CHECK(!fileMatches(empty, L"E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"));

  const unsigned char bytes[] = {0x00, 0x0f, 0xa5, 0xff};
  CHECK(updater::toHex(bytes, sizeof(bytes)) == L"000fa5ff");
  CHECK(!updater::digestMatches(bytes, sizeof(bytes), L"000fa5ff")); // not a SHA-256

  // A read error ends the check unmatched; an add error too.
  int calls = 0;
  CHECK(!updater::hashAll([&calls](unsigned char*, std::size_t) -> long long { return ++calls == 1 ? 10 : -1; },
                          [](const unsigned char*, std::size_t) { return true; }));
  CHECK(calls == 2);
  CHECK(!updater::hashAll([](unsigned char*, std::size_t) -> long long { return 10; },
                          [](const unsigned char*, std::size_t) { return false; }));
  // A reader claiming more than it was given is an error too.
  CHECK(!updater::hashAll([](unsigned char*, std::size_t capacity) -> long long {
    return static_cast<long long>(capacity) + 1;
  }, [](const unsigned char*, std::size_t) { return true; }));
}

// mitcad#921: an update from a test's source (an address, key or
// certificate from the environment) never asks for administrator rights.
void elevationTests() {
  using namespace mitcad;
  Installation allUsers;
  allUsers.kind = Installation::Kind::WindowsInstaller;
  allUsers.location = text("C:/Program Files/Mitcad");
  allUsers.allUsers = true;

  // The released source: an installation for all users updates as before.
  Installation same = allUsers;
  CHECK(!restrictToUserRights(same, false, false));
  CHECK(same.kind == Installation::Kind::WindowsInstaller && same.canInstall());

  // A test's source without administrator rights: announce only, with the
  // reason in the log's "announce only: ..." line.
  Installation restricted = allUsers;
  CHECK(restrictToUserRights(restricted, true, false));
  CHECK(restricted.kind == Installation::Kind::NotifyOnly);
  CHECK(!restricted.canInstall());
  CHECK(restricted.downloadPath(text("1.2.3")).isEmpty());
  CHECK(restricted.describe().startsWith(text("announce only: an update from a test's source")));
  CHECK(restricted.describe().contains(text("administrator rights")));

  // With administrator rights already (the installer test in session 0),
  // nothing is elevated: the update installs.
  Installation administrator = allUsers;
  CHECK(!restrictToUserRights(administrator, true, true));
  CHECK(administrator.canInstall());

  // Just for the current user, an AppImage, a macOS bundle: never elevated.
  Installation user = allUsers;
  user.allUsers = false;
  CHECK(!restrictToUserRights(user, true, false));
  CHECK(user.canInstall());
  Installation appImage;
  appImage.kind = Installation::Kind::AppImage;
  appImage.location = text("/home/user/Mitcad.AppImage");
  CHECK(!restrictToUserRights(appImage, true, false));
  CHECK(appImage.canInstall());
  Installation mac;
  mac.kind = Installation::Kind::MacBundle;
  mac.location = text("/Applications/Mitcad.app");
  CHECK(!restrictToUserRights(mac, true, false));
  CHECK(mac.canInstall());

  // Off Windows no process counts as an administrator here.
#ifndef _WIN32
  CHECK(!processHasAdministratorRights());
#endif
}

} // namespace

int updateInstallerTests() {
  // The settings in a folder of the test's own, never the user's.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("update_installer"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  CHECK(QSettings().fileName().startsWith(directory.path()));
  privateFolderTests();
  updaterArgumentTests();
  digestTests();
  elevationTests();
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}
