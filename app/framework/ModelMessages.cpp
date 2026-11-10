// SPDX-License-Identifier: MIT
#include "ModelMessages.hpp"

#include <algorithm>
#include <utility>

#include <QCoreApplication>
#include <QMutex>
#include <QMutexLocker>
#include <QDebug>
#include <QSet>

#include "Language.hpp"

namespace mitcad {
namespace {

bool asciiDigit(QChar c) { return c.unicode() >= '0' && c.unicode() <= '9'; }
bool asciiLetter(QChar c) {
  const char16_t u = c.unicode();
  return (u >= 'a' && u <= 'z') || (u >= 'A' && u <= 'Z');
}

// The length of the placeholder at `i` (%1 to %99), or 0.
int placeholderAt(const QString& text, int i) {
  if (text.at(i) != QLatin1Char('%') || i + 1 >= text.size() || !asciiDigit(text.at(i + 1)) ||
      text.at(i + 1) == QLatin1Char('0')) {
    return 0;
  }
  return i + 2 < text.size() && asciiDigit(text.at(i + 2)) ? 3 : 2;
}

// A template's fixed pieces: the text before, between and after its
// placeholders.
QStringList piecesOf(const QString& text) {
  QStringList pieces;
  QString piece;
  for (int i = 0; i < text.size();) {
    const int length = placeholderAt(text, i);
    if (length > 0) {
      pieces << piece;
      piece.clear();
      i += length;
      continue;
    }
    piece += text.at(i++);
  }
  pieces << piece;
  return pieces;
}

// The words (runs of ASCII letters, lower case, two letters or more) of a
// text. With `pieces`, of a template's fixed text, only the words that end
// where the text has a letter-free neighbour, not a placeholder ("%1ed").
QStringList wordsOf(const QString& text, bool openStart = false, bool openEnd = false) {
  QStringList words;
  for (int i = 0; i < text.size();) {
    if (!asciiLetter(text.at(i))) {
      ++i;
      continue;
    }
    int j = i;
    while (j < text.size() && asciiLetter(text.at(j))) {
      ++j;
    }
    const bool touches = (i == 0 && openStart) || (j == text.size() && openEnd);
    if (j - i >= 2 && !touches) {
      words << text.mid(i, j - i).toLower();
    }
    i = j;
  }
  return words;
}

} // namespace

ModelMessageCatalogue::ModelMessageCatalogue(const char* const* templates, int count) {
  m_templates.reserve(static_cast<std::size_t>(std::max(count, 0)));
  std::vector<QStringList> words;
  QHash<QString, int> frequency;
  for (int i = 0; i < count; ++i) {
    const QString text = QString::fromUtf8(templates[i]);
    const QStringList pieces = piecesOf(text);
    Template entry;
    entry.source = templates[i];
    QString pattern = QStringLiteral("\\A");
    QStringList own;
    for (int p = 0; p < pieces.size(); ++p) {
      if (p > 0) {
        pattern += QStringLiteral("(.*?)");
      }
      pattern += QRegularExpression::escape(pieces.at(p));
      entry.fixed += static_cast<int>(pieces.at(p).size());
      own += wordsOf(pieces.at(p), p > 0, p + 1 < pieces.size());
    }
    pattern += QStringLiteral("\\z");
    // A value is on one line: a message of several lines is matched line by
    // line (translate) unless its template has the line breaks.
    entry.pattern = QRegularExpression(pattern);
    own.removeDuplicates();
    for (const QString& word : own) {
      ++frequency[word];
    }
    words.push_back(own);
    m_templates.push_back(std::move(entry));
  }
  // Each template under its rarest word: few templates to try per word of a
  // message.
  for (int i = 0; i < count; ++i) {
    const QStringList& own = words[static_cast<std::size_t>(i)];
    if (own.isEmpty()) {
      m_everywhere.push_back(i);
      continue;
    }
    QString best = own.front();
    for (const QString& word : own) {
      const int f = frequency.value(word);
      const int b = frequency.value(best);
      if (f < b || (f == b && word.size() > best.size())) {
        best = word;
      }
    }
    m_byWord[best].push_back(i);
  }
}

ModelMessageMatch ModelMessageCatalogue::match(const QString& message) const {
  ModelMessageMatch result;
  if (message.isEmpty()) {
    return result;
  }
  std::vector<int> candidates = m_everywhere;
  QStringList words = wordsOf(message);
  words.removeDuplicates();
  for (const QString& word : words) {
    const auto found = m_byWord.constFind(word);
    if (found != m_byWord.cend()) {
      candidates.insert(candidates.end(), found->begin(), found->end());
    }
  }
  std::sort(candidates.begin(), candidates.end());
  candidates.erase(std::unique(candidates.begin(), candidates.end()), candidates.end());
  int bestFixed = -1;
  for (const int index : candidates) {
    const Template& entry = m_templates[static_cast<std::size_t>(index)];
    if (entry.fixed <= bestFixed) {
      continue;
    }
    const QRegularExpressionMatch found = entry.pattern.match(message);
    if (!found.hasMatch()) {
      continue;
    }
    bestFixed = entry.fixed;
    result.index = index;
    result.values.clear();
    for (int g = 1; g <= found.lastCapturedIndex(); ++g) {
      result.values << found.captured(g);
    }
  }
  return result;
}

QString ModelMessageCatalogue::translateWhole(const QString& message, const Translate& translated, int depth,
                                              bool* matched) const {
  const ModelMessageMatch found = match(message);
  *matched = found.found();
  if (!found.found()) {
    return message;
  }
  const char* english = source(found.index);
  const QString target = translated(english);
  if (target.isEmpty() || (target == QString::fromUtf8(english) && !wordsOf(target).isEmpty())) {
    // A template without its translation: the whole message stays English
    // rather than mixing languages. (One without words, "%1: %2", reads
    // the same in every language.)
    *matched = false;
    return message;
  }
  QStringList values = found.values;
  if (depth < 3) {
    for (QString& value : values) {
      // A value that is a sentence may be a message of its own ("%1: %2");
      // names and numbers are not.
      if (value.contains(QLatin1Char(' '))) {
        bool inner = false;
        const QString text = translateWhole(value, translated, depth + 1, &inner);
        if (inner) {
          value = text;
        }
      }
    }
  }
  return fillTemplate(target, values);
}

QString ModelMessageCatalogue::translate(const QString& message, const Translate& translated,
                                         bool* untranslated) const {
  if (message.trimmed().isEmpty()) {
    return message;
  }
  bool matched = false;
  const QString whole = translateWhole(message, translated, 0, &matched);
  if (matched) {
    return whole;
  }
  if (match(message).found()) {
    // Its template is not translated.
    if (untranslated != nullptr) {
      *untranslated = true;
    }
    return message;
  }
  if (message.contains(QLatin1Char('\n'))) {
    QStringList lines = message.split(QLatin1Char('\n'));
    for (QString& line : lines) {
      line = translate(line, translated, untranslated);
    }
    return lines.join(QLatin1Char('\n'));
  }
  const qsizetype colon = message.indexOf(QStringLiteral(": "));
  if (colon > 0) {
    return message.left(colon + 2) + translate(message.mid(colon + 2), translated, untranslated);
  }
  if (untranslated != nullptr) {
    *untranslated = true;
  }
  return message;
}

QString fillTemplate(const QString& text, const QStringList& values) {
  QString out;
  out.reserve(text.size());
  for (int i = 0; i < text.size();) {
    int length = placeholderAt(text, i);
    if (length > 0) {
      int number = text.mid(i + 1, length - 1).toInt();
      if (length == 3 && number > values.size()) {
        // "%15" with one value: %1 and a digit.
        number = text.mid(i + 1, 1).toInt();
        length = 2;
      }
      if (number >= 1 && number <= values.size()) {
        out += values.at(number - 1);
        i += length;
        continue;
      }
    }
    out += text.at(i++);
  }
  return out;
}

QString modelMessage(const QString& english) {
  if (english.isEmpty() || interfaceLanguage() == QLatin1String("en")) {
    return english;
  }
  // A text the interface's catalogue translated is the application's own,
  // not the model's.
  if (englishText(english) != english) {
    return english;
  }
  static QMutex mutex;
  static QHash<QString, QString> cache;
  // What this gave: a message shown again (a status line that shows a
  // panel's message) is not translated twice.
  static QSet<QString> shownTexts;
  static QSet<QString> logged;
  {
    QMutexLocker lock(&mutex);
    const auto found = cache.constFind(english);
    if (found != cache.cend()) {
      return *found;
    }
    if (shownTexts.contains(english)) {
      return english;
    }
  }
  static const ModelMessageCatalogue catalogue(kModelMessageTemplates, kModelMessageTemplateCount);
  bool untranslated = false;
  const QString shown = catalogue.translate(
      english, [](const char* source) { return QCoreApplication::translate("ModelMessage", source); }, &untranslated);
  bool log = false;
  {
    QMutexLocker lock(&mutex);
    if (cache.size() > 4096) {
      cache.clear();
      shownTexts.clear();
    }
    cache.insert(english, shown);
    if (shown != english) {
      shownTexts.insert(shown);
    }
    if (untranslated && logUntranslatedTexts() && !logged.contains(english)) {
      logged.insert(english);
      log = true;
    }
  }
  // Outside the lock: the log's handlers may show messages too.
  if (log) {
    qDebug().noquote() << QStringLiteral("Untranslated model message: %1").arg(english);
  }
  return shown;
}

QString modelMessage(const char* english) { return modelMessage(QString::fromUtf8(english ? english : "")); }

QStringList modelMessages(const QStringList& english) {
  QStringList shown;
  shown.reserve(english.size());
  for (const QString& text : english) {
    shown << modelMessage(text);
  }
  return shown;
}

} // namespace mitcad
