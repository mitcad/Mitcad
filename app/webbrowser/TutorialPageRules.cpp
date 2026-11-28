// SPDX-License-Identifier: MIT
// What a tutorial page may make MitCad do (mitcad#2615): see
// TutorialPageRules.hpp.
#include "TutorialPageRules.hpp"

#include <QJsonArray>
#include <QJsonDocument>
#include <QRandomGenerator>
#include <QStringList>

namespace mitcad::tutorialrules {
namespace {

// Characters that turn the text's direction around or mark it.
bool bidiControl(char16_t c) {
  return c == 0x061C || c == 0x200E || c == 0x200F || (c >= 0x202A && c <= 0x202E) || (c >= 0x2066 && c <= 0x2069);
}

} // namespace

QString logSafeText(const QString& text, int maximum) {
  const int limit = maximum < 1 ? 1 : maximum;
  QString cut = text;
  bool shortened = false;
  if (cut.size() > limit) {
    cut.truncate(limit);
    // Not half of a character outside the Basic Multilingual Plane.
    if (!cut.isEmpty() && cut.back().isHighSurrogate()) {
      cut.chop(1);
    }
    shortened = true;
  }
  QString safe;
  safe.reserve(cut.size() + 16);
  for (const QChar character : cut) {
    const char16_t c = character.unicode();
    switch (c) {
    case u'\\': safe += QStringLiteral("\\\\"); continue;
    case u'\n': safe += QStringLiteral("\\n"); continue;
    case u'\r': safe += QStringLiteral("\\r"); continue;
    case u'\t': safe += QStringLiteral("\\t"); continue;
    default: break;
    }
    if (c < 0x20 || (c >= 0x7F && c <= 0x9F) || c == 0x2028 || c == 0x2029 || bidiControl(c)) {
      safe += QStringLiteral("\\u{%1}").arg(static_cast<unsigned>(c), 0, 16);
      continue;
    }
    safe += character;
  }
  if (shortened) {
    safe += QStringLiteral("%1 (%2 characters)").arg(QChar(0x2026)).arg(text.size());
  }
  return safe;
}

bool BridgeLimiter::admit(const QString& name, qint64 bytes, qint64 nowMs, QString* reason) {
  if (bytes > kBridgeRequestBytes) {
    *reason = QStringLiteral("The call is larger than %1 MB").arg(kBridgeRequestBytes / (1024 * 1024));
    return false;
  }
  while (!m_recent.empty() && nowMs - m_recent.front() >= 1000) {
    m_recent.pop_front();
  }
  if (static_cast<int>(m_recent.size()) >= kBridgeCallsPerSecond) {
    *reason = QStringLiteral("Too many calls: at most %1 a second").arg(kBridgeCallsPerSecond);
    return false;
  }
  if (m_flying[name] >= kBridgeCallsWaiting) {
    *reason = QStringLiteral("%1 calls of the same kind are still being answered; call again once they are")
                  .arg(kBridgeCallsWaiting);
    return false;
  }
  m_recent.push_back(nowMs);
  ++m_flying[name];
  return true;
}

void BridgeLimiter::finished(const QString& name) {
  const auto it = m_flying.find(name);
  if (it == m_flying.end()) {
    return;
  }
  if (--it->second <= 0) {
    m_flying.erase(it);
  }
}

bool scriptedClicksAllowed() { return !qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT"); }

QString newClickKey() {
  QRandomGenerator* random = QRandomGenerator::system();
  return QStringLiteral("%1%2")
      .arg(random->generate64(), 16, 16, QLatin1Char('0'))
      .arg(random->generate64(), 16, 16, QLatin1Char('0'));
}

QString linkAtScript(const QString& text, const QString& key, bool buttons) {
  // The text as a JSON string inside an array, so that any character in it
  // stays a string; the answer comes back through the page's log.
  const QString quoted = QString::fromUtf8(QJsonDocument(QJsonArray{text}).toJson(QJsonDocument::Compact));
  const QString found = buttons ? QStringLiteral("document.querySelectorAll('a[href],button')")
                                : QStringLiteral("document.links");
  return QStringLiteral("(function(t){var a=Array.prototype.find.call(%3,function(l){"
                        "return l.textContent.indexOf(t)>=0||(l.href||'').indexOf(t)>=0;});"
                        "if(!a){console.log('mitcad:nolink:'+t);return;}"
                        "a.scrollIntoView({block:'center'});var r=a.getBoundingClientRect();"
                        "console.log('mitcad:linkat:%2:'+Math.round(r.left+r.width/2)+','+"
                        "Math.round(r.top+r.height/2)+','+(a.href||a.textContent));})(%1[0])")
      .arg(quoted, key, found);
}

bool parseLinkAt(const QString& message, const QString& key, int* x, int* y, QString* href) {
  if (key.isEmpty()) {
    return false;
  }
  const QString prefix = QStringLiteral("mitcad:linkat:") + key + QLatin1Char(':');
  if (!message.startsWith(prefix)) {
    return false;
  }
  const QStringList parts = message.mid(prefix.size()).split(QLatin1Char(','));
  if (parts.size() < 3) {
    return false;
  }
  bool xOk = false;
  bool yOk = false;
  *x = parts.at(0).toInt(&xOk);
  *y = parts.at(1).toInt(&yOk);
  *href = parts.mid(2).join(QLatin1Char(','));
  return xOk && yOk;
}

bool webLinkAddress(const QUrl& url) {
  const QString scheme = url.scheme().toLower();
  return url.isValid() && (scheme == QLatin1String("http") || scheme == QLatin1String("https")) &&
         !url.host().isEmpty();
}

} // namespace mitcad::tutorialrules
