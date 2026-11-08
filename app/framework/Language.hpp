// SPDX-License-Identifier: MIT
#pragma once

#include <QList>
#include <QString>
#include <QStringList>

namespace mitcad {

// The interface language (mitcad#1460, mitcad#1284; docs/translations.md).
// English is the source language of every text and needs no file; another
// language is the Qt Linguist catalogue mitcad_<code>.qm in the
// translations folder (beside the executable in a build tree and the
// Windows installer, share/mitcad/translations in the AppImage,
// Resources/translations in a macOS bundle), with Qt's own qtbase_<code>.qm
// for its standard dialogs and buttons where Qt has one. Adding a language
// is data: its .ts file and its line in app/translations/languages.txt; the
// code lists what was built.

// A language that can be chosen: its code ("fi") and its name in itself
// ("Suomi"), which its catalogue carries (the message "English" of the
// context "LanguageName").
struct InterfaceLanguage {
  QString code;
  QString nativeName;
};

// The setting interface/language: "" (Automatic, also when absent), "en"
// or a language's code.
QString languagePreference();
void setLanguagePreference(const QString& code);

// The folder whose mitcad_<code>.qm files are the languages there are; empty
// when there is none.
QString translationsFolder();

// English first, then every language whose catalogue is in the
// translations folder, by code.
QList<InterfaceLanguage> availableLanguages();

// The language to start in, and why (the start-up's Language: line):
// `option` (--language, "" when not given), else `environment`
// (MITCAD_LANGUAGE), else the preference, else Automatic: the system's
// first UI language (`systemLanguages`, as QLocale::uiLanguages gives them)
// when it is one of `available`, otherwise English. A code that is not
// available is English, with the reason saying so.
struct LanguageChoice {
  QString code;   // "en" or an available language's code
  QString reason; // "--language", "MITCAD_LANGUAGE", "preference", "automatic: ..."
};
LanguageChoice chooseLanguage(const QString& option, const QString& environment, const QString& preference,
                              const QStringList& systemLanguages, const QStringList& available);

// At start, after the QApplication exists and before any window: the
// catalogues of the chosen language installed (English installs none) and
// "Language: <code> (<reason>)" logged. `option` is --language's value.
// Returns the code in use.
QString startInterfaceLanguage(const QString& option);

// The code of the language in use ("en" before startInterfaceLanguage).
QString interfaceLanguage();

// The English source of a text the installed catalogue translated (a
// command's name in the command search, which also finds the English
// names); the text itself when it was not translated.
QString englishText(const QString& shown);

// Loads a catalogue as startInterfaceLanguage does and gives its
// translation of the message `source` of `context` (empty when it has none),
// for tests; also through its context-free part, as the application looks
// a text up.
QString catalogueText(const QString& file, const char* context, const char* source);

} // namespace mitcad
