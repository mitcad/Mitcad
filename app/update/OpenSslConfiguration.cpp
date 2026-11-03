// SPDX-License-Identifier: MIT
#include "update/OpenSslConfiguration.hpp"

#include <QtGlobal>

#ifdef Q_OS_LINUX
#include <QByteArray>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QLibrary>
#include <QString>
#include <QStringList>
#include <QtLogging>

#include <cstddef>
#include <cstdint>
#endif

namespace mitcad {

#ifdef Q_OS_LINUX
namespace {

// libcrypto 3's flags (openssl/conf.h, openssl/crypto.h, openssl/err.h):
// Mitcad has no OpenSSL headers, as it links no OpenSSL; Qt's TLS plugin
// loads it at run time, and so does this.
constexpr unsigned long kConfIgnoreMissingFile = 0x10; // CONF_MFLAGS_IGNORE_MISSING_FILE
constexpr unsigned long kConfDefaultSection = 0x20;    // CONF_MFLAGS_DEFAULT_SECTION
constexpr std::uint64_t kInitLoadConfig = 0x40;        // OPENSSL_INIT_LOAD_CONFIG
constexpr int kErrorTextString = 0x02;                 // ERR_TXT_STRING

using LibraryContextNew = void* (*)();
using LibraryContextFree = void (*)(void*);
using LoadBuiltinModules = void (*)();
using ModulesLoadFile = int (*)(void*, const char*, const char*, unsigned long);
using PeekErrorData = unsigned long (*)(const char**, int*);
using ErrorString = void (*)(unsigned long, char*, std::size_t);
using ClearErrors = void (*)();
using InitSettingsNew = void* (*)();
using InitSetConfigFile = int (*)(void*, const char*);
using InitSettingsFree = void (*)(void*);
using InitCrypto = int (*)(std::uint64_t, const void*);

template <typename Function>
Function resolve(QLibrary& library, const char* name, QStringList& missing) {
  const QFunctionPointer function = library.resolve(name);
  if (function == nullptr) missing << QString::fromLatin1(name);
  return reinterpret_cast<Function>(function);
}

// The executables' folder: an AppImage's file is the application for Qt,
// its executables are in its mount folder (as AssistantSetup's).
QString executableDirectory() {
  const QString self = QFileInfo(QStringLiteral("/proc/self/exe")).symLinkTarget();
  return self.isEmpty() ? QString() : QFileInfo(self).absolutePath();
}

} // namespace

void checkOpenSslConfiguration() {
  const QString bin = executableDirectory();
  if (bin.isEmpty()) return;
  const QString bundled = QDir::cleanPath(bin + QStringLiteral("/../lib/libcrypto.so.3"));
  if (!QFileInfo(bundled).isFile()) return;
  // The same library Qt's TLS plugin loads later by its name (libcrypto.so.3
  // is loaded once per process), so the configuration chosen here is the
  // one its TLS initialises with. QLibrary never unloads it on its own.
  QLibrary crypto(bundled);
  if (!crypto.load()) {
    qInfo().noquote() << QStringLiteral("TLS: cannot check the system's OpenSSL configuration (%1)")
                                .arg(crypto.errorString());
    return;
  }
  QStringList missing;
  const auto contextNew = resolve<LibraryContextNew>(crypto, "OSSL_LIB_CTX_new", missing);
  const auto contextFree = resolve<LibraryContextFree>(crypto, "OSSL_LIB_CTX_free", missing);
  const auto builtinModules = resolve<LoadBuiltinModules>(crypto, "OPENSSL_load_builtin_modules", missing);
  const auto modulesLoadFile = resolve<ModulesLoadFile>(crypto, "CONF_modules_load_file_ex", missing);
  const auto peekErrorData = resolve<PeekErrorData>(crypto, "ERR_peek_error_data", missing);
  const auto errorString = resolve<ErrorString>(crypto, "ERR_error_string_n", missing);
  const auto clearErrors = resolve<ClearErrors>(crypto, "ERR_clear_error", missing);
  const auto settingsNew = resolve<InitSettingsNew>(crypto, "OPENSSL_INIT_new", missing);
  const auto setConfigFile = resolve<InitSetConfigFile>(crypto, "OPENSSL_INIT_set_config_filename", missing);
  const auto settingsFree = resolve<InitSettingsFree>(crypto, "OPENSSL_INIT_free", missing);
  const auto initCrypto = resolve<InitCrypto>(crypto, "OPENSSL_init_crypto", missing);
  if (!missing.isEmpty()) {
    qInfo().noquote() << QStringLiteral("TLS: cannot check the system's OpenSSL configuration (%1 has no %2)")
                                .arg(bundled, missing.join(QStringLiteral(", ")));
    return;
  }

  // The system's configuration (OPENSSL_CONF, else OPENSSLDIR's
  // openssl.cnf), loaded as OpenSSL's own start-up loads it but into a
  // library context of its own, which leaves the process's default one
  // untouched, and with every module's error reported: OpenSSL's start-up
  // stops at the first module that fails, so that a failing module leaves
  // the policy only partly applied, or no TLS at all. A missing file is no
  // error, as for OpenSSL.
  void* context = contextNew();
  if (context == nullptr) {
    qInfo().noquote() << QStringLiteral("TLS: cannot check the system's OpenSSL configuration (no library context)");
    return;
  }
  builtinModules();
  const int loaded = modulesLoadFile(context, nullptr, nullptr, kConfDefaultSection | kConfIgnoreMissingFile);
  QString error;
  if (loaded <= 0) {
    const char* data = nullptr;
    int flags = 0;
    const unsigned long code = peekErrorData(&data, &flags);
    if (code != 0) {
      char text[256] = {};
      errorString(code, text, sizeof text);
      error = QString::fromUtf8(text);
      if (data != nullptr && *data != '\0' && (flags & kErrorTextString) != 0) {
        error += QStringLiteral(": ") + QString::fromUtf8(data);
      }
    }
    if (error.isEmpty()) error = QStringLiteral("no error recorded");
    clearErrors();
  }
  contextFree(context);
  if (loaded > 0) {
    qInfo().noquote() << QStringLiteral("TLS: the system's OpenSSL configuration loads; the AppImage's OpenSSL uses it");
    return;
  }

  // The AppImage's own configuration for the process's default context,
  // before Qt's TLS initialises it: OpenSSL loads its configuration once
  // per process, so Qt's later start-up keeps this one. No OPENSSL_CONF in
  // the environment, which every program Mitcad starts would inherit.
  const QByteArray own = QFile::encodeName(QDir::cleanPath(bin + QStringLiteral("/../share/mitcad/openssl.cnf")));
  // Without settings OpenSSL would load the system's configuration again.
  void* settings = settingsNew();
  const int initialised = settings != nullptr && setConfigFile(settings, own.constData()) > 0
                              ? initCrypto(kInitLoadConfig, settings)
                              : 0;
  if (settings != nullptr) settingsFree(settings);
  qInfo().noquote()
      << QStringLiteral("TLS: the system's OpenSSL configuration failed to load (%1); using the AppImage's own")
             .arg(error);
  if (initialised <= 0) {
    qInfo().noquote() << QStringLiteral("TLS: the AppImage's own OpenSSL configuration %1 did not load either; "
                                        "TLS may be unavailable")
                                .arg(QFile::decodeName(own));
    clearErrors();
  }
}
#else
void checkOpenSslConfiguration() {}
#endif

} // namespace mitcad
