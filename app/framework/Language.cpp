// SPDX-License-Identifier: MIT
#include "Language.hpp"

#include <cstring>
#include <memory>

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QHash>
#include <QLocale>
#include <QMutex>
#include <QMutexLocker>
#include <QSet>
#include <QSettings>
#include <QTranslator>
#include <QtLogging>

namespace mitcad {
namespace {

const QString kLanguageKey = QStringLiteral("interface/language");
const QString kEnglish = QStringLiteral("en");
// The message whose translation is the language's own name.
constexpr const char* kNameContext = "LanguageName";
//: The name of this language in itself, as Preferences › General › Language lists it ("Suomi", "Deutsch").
constexpr const char* kOwnName = QT_TRANSLATE_NOOP("LanguageName", "English");
// Qt's own texts whose translation in Qt's catalogue differs from MitCad's
// terms (Finnish: Qt's "Peru" for Cancel, MitCad's "Peruuta"). MitCad's
// catalogue is asked before Qt's (startInterfaceLanguage), so its entries
// in Qt's contexts win; marked here so that lupdate keeps them.
[[maybe_unused]] constexpr const char* kQtTextsInMitCadTerms[] = {
    QT_TRANSLATE_NOOP("QPlatformTheme", "Cancel"),  // the Cancel of every standard button box
    QT_TRANSLATE_NOOP("QProgressDialog", "Cancel"),
    QT_TRANSLATE_NOOP("QWizard", "Cancel"),
};
// The context-free copy of every translated message that the build adds to
// a catalogue (tools/translations, "prepare"): a text whose context at run
// time is not the one lupdate saw in the source (a class without Q_OBJECT
// whose tr() is its base class's, a file's own tr() helper) is still found.
constexpr const char* kAnyContext = "MitCad.AnyContext";

// The language in use and the English source of each text the catalogue
// translated (englishText).
struct LanguageState {
  QString code = kEnglish;
  QMutex mutex;
  QHash<QString, QString> sources;
  QSet<QString> missed;
  bool logMisses = false;
};

LanguageState& state() {
  static LanguageState current;
  return current;
}

// A catalogue of MitCad's: `primary` looks a text up in its own context;
// the other, installed before Qt's own catalogue so that Qt's texts take
// Qt's translation first, in the context-free part. Both note the English
// source of what they translate.
class Catalogue : public QTranslator {
 public:
  explicit Catalogue(bool primary) : m_primary(primary) {}

  QString translate(const char* context, const char* source, const char* disambiguation = nullptr,
                    int n = -1) const override {
    if (source == nullptr || *source == '\0') {
      return QString();
    }
    QString text;
    if (m_primary) {
      text = QTranslator::translate(context, source, disambiguation, n);
    } else if (context == nullptr ||
               (std::strcmp(context, "QShortcut") != 0 && std::strcmp(context, "QKeySequence") != 0 &&
                std::strcmp(context, kNameContext) != 0)) {
      // Not the names of keys: a shortcut's text is Qt's, never a word of
      // MitCad's that happens to be the same.
      text = QTranslator::translate(kAnyContext, source, disambiguation, n);
    }
    LanguageState& current = state();
    if (!m_primary && current.logMisses) {
      // Asked only when the catalogue in its contexts had no translation: a
      // text found here was found without its context (mitcad#1472).
      const QString key = QStringLiteral("%1: %2: %3")
                              .arg(text.isEmpty() ? QStringLiteral("Untranslated") : QStringLiteral("Fallback"),
                                   QString::fromUtf8(context ? context : ""), QString::fromUtf8(source));
      bool first = false;
      {
        QMutexLocker lock(&current.mutex);
        first = !current.missed.contains(key);
        current.missed.insert(key);
      }
      // Outside the lock: the log's handlers may translate texts too.
      if (first) {
        qDebug().noquote() << key;
      }
    }
    if (text.isEmpty()) {
      return text;
    }
    const QString english = QString::fromUtf8(source);
    if (text != english) {
      QMutexLocker lock(&current.mutex);
      current.sources.insert(text, english);
    }
    return text;
  }

 private:
  bool m_primary;
};

// "fi", "fi_FI", "fi-FI" and the like as the codes to look for, the most
// specific first ("pt_BR", then "pt").
QStringList codesFor(const QString& tag) {
  QString code = tag.trimmed();
  code.replace(QLatin1Char('-'), QLatin1Char('_'));
  QStringList codes;
  if (code.isEmpty()) {
    return codes;
  }
  codes << code.toLower();
  const qsizetype cut = code.indexOf(QLatin1Char('_'));
  if (cut > 0) {
    codes.front() = code.left(cut).toLower() + code.mid(cut);
    codes << code.left(cut).toLower();
  }
  return codes;
}

// The available code a tag names, "en" for English, or empty.
QString matchCode(const QString& tag, const QStringList& available) {
  for (const QString& code : codesFor(tag)) {
    if (code == kEnglish) {
      return kEnglish;
    }
    if (available.contains(code)) {
      return code;
    }
  }
  return QString();
}

QString catalogueName(const QString& code) { return QStringLiteral("mitcad_%1.qm").arg(code); }

} // namespace

QString languagePreference() { return QSettings().value(kLanguageKey).toString(); }

void setLanguagePreference(const QString& code) {
  if (code.isEmpty()) {
    QSettings().remove(kLanguageKey);
  } else {
    QSettings().setValue(kLanguageKey, code);
  }
}

QString translationsFolder() {
  const QDir executable(QCoreApplication::applicationDirPath());
  // Beside the executable (a build tree, the Windows installer), in an
  // installed tree's share folder (the AppImage), in a macOS bundle's
  // resources: where the annotations' lettering is too.
  const QStringList folders{executable.filePath(QStringLiteral("translations")),
                            executable.filePath(QStringLiteral("../share/mitcad/translations")),
                            executable.filePath(QStringLiteral("../Resources/translations"))};
  for (const QString& folder : folders) {
    const QDir dir(folder);
    if (dir.exists() && !dir.entryList({QStringLiteral("mitcad_*.qm")}, QDir::Files).isEmpty()) {
      return QDir::cleanPath(dir.absolutePath());
    }
  }
  return QString();
}

QList<InterfaceLanguage> availableLanguages() {
  QList<InterfaceLanguage> languages{{kEnglish, QString::fromLatin1(kOwnName)}};
  const QString folder = translationsFolder();
  if (folder.isEmpty()) {
    return languages;
  }
  const QStringList files = QDir(folder).entryList({QStringLiteral("mitcad_*.qm")}, QDir::Files, QDir::Name);
  for (const QString& file : files) {
    const QString code = file.mid(7, file.size() - 7 - 3);
    if (code.isEmpty() || code == kEnglish) {
      continue;
    }
    QTranslator catalogue;
    QString name;
    if (catalogue.load(QDir(folder).filePath(file))) {
      name = catalogue.translate(kNameContext, kOwnName);
    }
    if (name.isEmpty()) {
      // A catalogue without its own name: the system's name of the
      // language, else its code.
      const QLocale locale(code);
      name = locale.language() != QLocale::C ? locale.nativeLanguageName() : code;
      if (!name.isEmpty()) {
        name[0] = name.at(0).toUpper();
      }
    }
    languages.append({code, name});
  }
  return languages;
}

LanguageChoice chooseLanguage(const QString& option, const QString& environment, const QString& preference,
                              const QStringList& systemLanguages, const QStringList& available) {
  const auto given = [&available](const QString& tag, const QString& reason) {
    const QString code = matchCode(tag, available);
    if (code.isEmpty()) {
      return LanguageChoice{kEnglish, QStringLiteral("%1; no catalogue for %2").arg(reason, tag)};
    }
    return LanguageChoice{code, reason};
  };
  if (!option.trimmed().isEmpty()) {
    return given(option, QStringLiteral("--language"));
  }
  if (!environment.trimmed().isEmpty()) {
    return given(environment, QStringLiteral("MITCAD_LANGUAGE"));
  }
  const QString stored = preference.trimmed();
  QString note;
  if (!stored.isEmpty() && stored != QLatin1String("auto")) {
    const QString code = matchCode(stored, available);
    if (!code.isEmpty()) {
      return {code, QStringLiteral("preference")};
    }
    note = QStringLiteral("; no catalogue for the preference %1").arg(stored);
  }
  // Automatic: the system's first UI language when MitCad has it, else
  // English (not a later language of the list: a user who reads Swedish
  // first and Finnish second gets English, as the rest of the system
  // falls back to).
  const QString first = systemLanguages.isEmpty() ? QString() : systemLanguages.first();
  const QString code = matchCode(first, available);
  const QString reason = QStringLiteral("automatic: the system's first language is %1%2")
                             .arg(first.isEmpty() ? QStringLiteral("unknown") : first, note);
  return {code.isEmpty() ? kEnglish : code, reason};
}

QString startInterfaceLanguage(const QString& option) {
  LanguageState& current = state();
  current.logMisses = qEnvironmentVariableIntValue("MITCAD_LOG_UNTRANSLATED") != 0;
  QStringList available;
  for (const InterfaceLanguage& language : availableLanguages()) {
    if (language.code != kEnglish) {
      available << language.code;
    }
  }
  const LanguageChoice choice = chooseLanguage(option, qEnvironmentVariable("MITCAD_LANGUAGE"), languagePreference(),
                                               QLocale::system().uiLanguages(), available);
  QString code = choice.code;
  QString reason = choice.reason;
  if (code != kEnglish) {
    const QDir folder(translationsFolder());
    // The context-free part first, then Qt's own catalogue, then the
    // catalogue in its contexts: the last installed is asked first.
    auto anyContext = std::make_unique<Catalogue>(false);
    auto primary = std::make_unique<Catalogue>(true);
    const QString file = folder.filePath(catalogueName(code));
    if (anyContext->load(file) && primary->load(file)) {
      QCoreApplication::installTranslator(anyContext.release());
      auto qt = std::make_unique<QTranslator>();
      const QString qtFile = folder.filePath(QStringLiteral("qtbase_%1.qm").arg(code));
      if (QFileInfo::exists(qtFile) && qt->load(qtFile)) {
        QCoreApplication::installTranslator(qt.release());
      }
      QCoreApplication::installTranslator(primary.release());
    } else {
      reason = QStringLiteral("%1; %2 did not load").arg(reason, file);
      code = kEnglish;
    }
  }
  current.code = code;
  qInfo().noquote() << QStringLiteral("Language: %1 (%2)").arg(code, reason);
  return code;
}

QString interfaceLanguage() { return state().code; }

bool logUntranslatedTexts() { return state().logMisses; }

QString englishText(const QString& shown) {
  LanguageState& current = state();
  QMutexLocker lock(&current.mutex);
  return current.sources.value(shown, shown);
}

QString catalogueText(const QString& file, const char* context, const char* source) {
  Catalogue primary(true);
  Catalogue anyContext(false);
  if (!primary.load(file) || !anyContext.load(file)) {
    return QString();
  }
  const QString text = primary.translate(context, source);
  return text.isEmpty() ? anyContext.translate(context, source) : text;
}

} // namespace mitcad
