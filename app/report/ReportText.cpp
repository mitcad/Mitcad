// SPDX-License-Identifier: MIT
#include "ReportText.hpp"

#include <algorithm>
#include <cstdlib>
#include <memory>

#include <QByteArray>
#include <QCryptographicHash>
#include <QDir>
#include <QRegularExpression>
#include <QSysInfo>
#include <QtGlobal>

#if __has_include(<cxxabi.h>)
#include <cxxabi.h>
#define MITCAD_HAS_CXXABI 1
#endif

namespace mitcad::report {
namespace {

// Characters that end a path in running text.
const QString kPathChars = QStringLiteral(R"([^\s"'<>|*?()\[\],;:])");
const QString kUnixPathChars = QStringLiteral(R"([^\s/"'<>|*?()\[\],;:])");

// Names that stay after <path>/: programs, libraries and source files.
const QRegularExpression kKeptName(
    QStringLiteral(R"(^(mitcad(-render|-cli|-updater)?(\.exe)?|[\w.+-]+\.(so(\.[\d.]+)?|dylib|dll|exe|cpp|cxx|cc|c|)"
                   R"(hpp|hxx|h|mm|rs|py|sh))$)"),
    QRegularExpression::CaseInsensitiveOption);
const QRegularExpression kExtension(QStringLiteral(R"(\.([A-Za-z0-9]{1,8})$)"));

QString maskedPath(const QString& path) {
  const QStringList parts = path.split(QRegularExpression(QStringLiteral(R"([\\/])")), Qt::SkipEmptyParts);
  const QString name = parts.isEmpty() ? QString() : parts.last();
  if (path.endsWith(QLatin1Char('/')) || path.endsWith(QLatin1Char('\\')) || parts.size() < 2 ||
      name == QStringLiteral("<home>")) {
    return QStringLiteral("<path>");
  }
  if (kKeptName.match(name).hasMatch()) {
    return QStringLiteral("<path>/") + name;
  }
  const QRegularExpressionMatch extension = kExtension.match(name);
  if (extension.hasMatch() && extension.capturedStart() > 0) {
    return QStringLiteral("<path>/<file>.") + extension.captured(1);
  }
  return QStringLiteral("<path>");
}

QRegularExpression wordPattern(const QString& word) {
  return QRegularExpression(QStringLiteral(R"((?<![\w-]))") + QRegularExpression::escape(word) +
                                QStringLiteral(R"((?![\w-]))"),
                            QRegularExpression::CaseInsensitiveOption);
}

bool maskableName(const QString& name) {
  static const QStringList kept{QStringLiteral("mitcad"), QStringLiteral("localhost"), QStringLiteral("user"),
                                QStringLiteral("home"), QStringLiteral("path"), QStringLiteral("file")};
  return name.size() >= 3 && !kept.contains(name, Qt::CaseInsensitive);
}

QString demangled(const QString& symbol) {
#ifdef MITCAD_HAS_CXXABI
  if (symbol.startsWith(QStringLiteral("_Z"))) {
    int status = 0;
    const QByteArray mangled = symbol.toUtf8();
    std::unique_ptr<char, void (*)(void*)> name(abi::__cxa_demangle(mangled.constData(), nullptr, nullptr, &status),
                                                std::free);
    if (status == 0 && name) {
      QString text = QString::fromUtf8(name.get());
      // Rust's legacy names end with a hash of the crate: not the same
      // from one build to the next.
      static const QRegularExpression rustHash(QStringLiteral(R"(::h[0-9a-f]{16}$)"));
      text.remove(rustHash);
      return text;
    }
  }
#endif
  return symbol;
}

QString baseName(const QString& path) {
  const qsizetype slash = std::max(path.lastIndexOf(QLatin1Char('/')), path.lastIndexOf(QLatin1Char('\\')));
  return path.mid(slash + 1).trimmed();
}

// The qualified name at the start of a mangled (Itanium C++ ABI) symbol,
// without its parameters: "_ZN6mitcad5crash6detail8onSignalEiP9siginfo_tPv"
// is "mitcad::crash::detail::onSignal", "_ZSt9terminatev" "std::terminate".
// The symbol itself when it does not start with one.
QString qualifiedName(const QString& symbol) {
  qsizetype at = 2; // after "_Z"
  const bool nested = symbol.mid(at, 1) == QStringLiteral("N");
  if (nested) {
    ++at;
    // A member function's qualifiers.
    while (at < symbol.size() && QStringLiteral("rVKRO").contains(symbol[at])) {
      ++at;
    }
  }
  QStringList parts;
  if (symbol.mid(at, 2) == QStringLiteral("St")) {
    parts << QStringLiteral("std");
    at += 2;
  }
  while (at < symbol.size() && symbol[at].isDigit()) {
    qsizetype end = at;
    while (end < symbol.size() && symbol[end].isDigit()) {
      ++end;
    }
    const qsizetype length = symbol.mid(at, end - at).toInt();
    if (length <= 0 || end + length > symbol.size()) {
      break;
    }
    parts << symbol.mid(end, length);
    at = end + length;
    if (!nested) {
      break;
    }
  }
  return parts.isEmpty() ? symbol : parts.join(QStringLiteral("::"));
}

// Frames of the crash's machinery at the top of a stack: the handlers,
// the signal's delivery, abort(), the exception and panic runtimes.
bool machinery(const Frame& frame) {
  static const QStringList names{
      QStringLiteral("raise"), QStringLiteral("gsignal"), QStringLiteral("abort"), QStringLiteral("pthread_kill"),
      QStringLiteral("__pthread_kill_implementation"), QStringLiteral("__pthread_kill_internal"),
      QStringLiteral("__restore_rt"), QStringLiteral("_sigtramp"), QStringLiteral("__cxa_throw"),
      QStringLiteral("__cxa_rethrow"), QStringLiteral("rust_panic"), QStringLiteral("KiUserExceptionDispatcher"),
      QStringLiteral("UnhandledExceptionFilter"), QStringLiteral("RtlCaptureStackBackTrace"),
      QStringLiteral("CaptureStackBackTrace")};
  static const QStringList prefixes{
      QStringLiteral("mitcad::crash::detail::"), QStringLiteral("mitcad::geometry::detail::dispatch_occt_signal"),
      QStringLiteral("mitcad::geometry::detail::call_signal_handler"), QStringLiteral("std::terminate"),
      QStringLiteral("__cxxabiv1::"), QStringLiteral("_Unwind_"), QStringLiteral("std::panicking::"),
      QStringLiteral("core::panicking::"), QStringLiteral("std::sys::"), QStringLiteral("std::process::abort"),
      QStringLiteral("std::rt::"), QStringLiteral("__rust_"), QStringLiteral("cxx::unwind::"),
      QStringLiteral("<cxx::unwind::"), QStringLiteral("panic_abort::")};
  // A name still mangled (without a demangler, as with MSVC for another
  // platform's stack): its qualified name, as the names and prefixes are.
  const QString symbol = frame.symbol.startsWith(QStringLiteral("_Z")) ? qualifiedName(frame.symbol) : frame.symbol;
  const QString name = symbol.section(QLatin1Char('('), 0, 0);
  if (names.contains(name)) {
    return true;
  }
  for (const QString& prefix : prefixes) {
    if (symbol.startsWith(prefix)) {
      return true;
    }
  }
  if (!frame.symbol.isEmpty()) {
    return false;
  }
  // Frames without a name in the system's libraries (the signal's
  // trampoline, the C library's insides) and, on Windows, its exception
  // dispatch.
  static const QRegularExpression system(
      QStringLiteral(R"(^(libc\.so.*|libpthread\.so.*|libsystem_.*\.dylib|ntdll\.dll|kernelbase\.dll|kernel32\.dll|)"
                     R"(ucrtbased?\.dll|vcruntime140d?\.dll|msvcp140d?\.dll)$)"),
      QRegularExpression::CaseInsensitiveOption);
  return system.match(frame.module).hasMatch();
}

QString shortKey(const QByteArray& text) {
  return QString::fromLatin1(QCryptographicHash::hash(text, QCryptographicHash::Sha256).toHex().left(12));
}

} // namespace

// ---------------------------------------------------------------------------
// Masking

MaskContext currentMaskContext() {
  MaskContext context;
  context.home = QDir::homePath();
  context.user = qEnvironmentVariable("USER");
  if (context.user.isEmpty()) {
    context.user = qEnvironmentVariable("USERNAME");
  }
  if (context.user.isEmpty()) {
    context.user = QDir(context.home).dirName();
  }
  context.host = QSysInfo::machineHostName();
  return context;
}

QString mask(const QString& text, const MaskContext& context) {
  QString result = text;
  // The home folder, with either kind of slash.
  const QStringList homeParts =
      context.home.split(QRegularExpression(QStringLiteral(R"([\\/])")), Qt::SkipEmptyParts);
  if (homeParts.size() >= 1 && context.home.size() >= 3) {
    QStringList escaped;
    for (const QString& part : homeParts) {
      escaped << QRegularExpression::escape(part);
    }
    const bool rooted = context.home.startsWith(QLatin1Char('/')) || context.home.startsWith(QLatin1Char('\\'));
    const QRegularExpression home((rooted ? QStringLiteral(R"([\\/])") : QString()) +
                                      escaped.join(QStringLiteral(R"([\\/])")) +
                                      QStringLiteral(R"((?=[\\/\s"'<>()\[\],;:]|$))"),
                                  QRegularExpression::CaseInsensitiveOption);
    result.replace(home, QStringLiteral("<home>"));
  }
  // Absolute paths.
  static const QRegularExpression paths(
      QStringLiteral(R"(<home>(?:[\\/]%1*)*)"
                     R"(|(?<![\w])[A-Za-z]:[\\/]%1*)"
                     R"(|\\\\%1+)"
                     R"(|(?<![\w.~:/\\<>-])~(?:/%2+)+/?)"
                     R"(|(?<![\w.~:/\\<>-])(?:/%2+){2,}/?)")
          .arg(kPathChars, kUnixPathChars));
  QString masked;
  qsizetype last = 0;
  for (QRegularExpressionMatchIterator it = paths.globalMatch(result); it.hasNext();) {
    const QRegularExpressionMatch match = it.next();
    masked += result.mid(last, match.capturedStart() - last);
    // A full stop after a path ends the sentence.
    QString path = match.captured();
    qsizetype end = match.capturedEnd();
    while (path.endsWith(QLatin1Char('.'))) {
      path.chop(1);
      --end;
    }
    // The home folder itself stays "<home>".
    masked += path == QStringLiteral("<home>") ? path : maskedPath(path);
    last = end;
  }
  masked += result.mid(last);
  result = masked;
  // The user's and the computer's names.
  if (maskableName(context.user)) {
    result.replace(wordPattern(context.user), QStringLiteral("<user>"));
  }
  if (maskableName(context.host)) {
    result.replace(wordPattern(context.host), QStringLiteral("<host>"));
  }
  const QString shortHost = context.host.section(QLatin1Char('.'), 0, 0);
  if (shortHost != context.host && maskableName(shortHost)) {
    result.replace(wordPattern(shortHost), QStringLiteral("<host>"));
  }
  return result;
}

// ---------------------------------------------------------------------------
// Crash reports

CrashReport parseCrashReport(const QByteArray& text) {
  CrashReport report;
  const QStringList lines = QString::fromUtf8(text).split(QLatin1Char('\n'));
  if (lines.isEmpty() || !lines.first().startsWith(QStringLiteral("Mitcad crash report"))) {
    return report;
  }
  enum class Part { Header, Stack, Actions } part = Part::Header;
  bool ended = false;
  for (qsizetype i = 1; i < lines.size(); ++i) {
    const QString line = lines[i];
    if (line == QStringLiteral("stack:")) {
      part = Part::Stack;
      continue;
    }
    if (line == QStringLiteral("actions:")) {
      part = Part::Actions;
      continue;
    }
    if (line == QStringLiteral("end")) {
      ended = true;
      break;
    }
    if (part == Part::Stack) {
      if (!line.trimmed().isEmpty()) {
        report.stack << line.trimmed();
      }
      continue;
    }
    if (part == Part::Actions) {
      if (!line.isEmpty()) {
        report.actions << line;
      }
      continue;
    }
    const qsizetype colon = line.indexOf(QStringLiteral(": "));
    const QString key = colon < 0 ? line.chopped(line.endsWith(QLatin1Char(':')) ? 1 : 0) : line.left(colon);
    const QString value = colon < 0 ? QString() : line.mid(colon + 2);
    if (key == QStringLiteral("process")) {
      report.process = value;
    } else if (key == QStringLiteral("version")) {
      report.version = value;
    } else if (key == QStringLiteral("platform")) {
      report.platform = value;
    } else if (key == QStringLiteral("pid")) {
      report.pid = value.toLongLong();
    } else if (key == QStringLiteral("parent")) {
      report.parent = value.toLongLong();
    } else if (key == QStringLiteral("time")) {
      report.time = value.toLongLong();
    } else if (key == QStringLiteral("signal")) {
      report.signal = value;
    } else if (key == QStringLiteral("address")) {
      report.address = value;
    } else if (key == QStringLiteral("message")) {
      report.message = value;
    }
  }
  // A report the handler could not finish still says what crashed.
  Q_UNUSED(ended);
  report.valid = !report.signal.isEmpty() && !report.process.isEmpty();
  return report;
}

Frame parseFrame(const QString& line) {
  Frame frame;
  static const QRegularExpression glibc(QStringLiteral(R"(^(.*)\(([^()]*)\)\s*\[(0x[0-9a-fA-F]+)\]$)"));
  static const QRegularExpression inner(QStringLiteral(R"(^([^+]*)\+(0x[0-9a-fA-F]+)$)"));
  static const QRegularExpression mac(
      QStringLiteral(R"(^\s*\d+\s+(\S+)\s+(0x[0-9a-fA-F]+)\s+(.+?)\s+\+\s+(\d+)$)"));
  static const QRegularExpression windows(QStringLiteral(R"(^(.*?)(?:!(.*))?\+(0x[0-9a-fA-F]+)$)"));
  if (const QRegularExpressionMatch match = glibc.match(line); match.hasMatch()) {
    frame.module = baseName(match.captured(1));
    const QRegularExpressionMatch symbol = inner.match(match.captured(2));
    if (symbol.hasMatch()) {
      frame.symbol = demangled(symbol.captured(1).trimmed());
      frame.offset = symbol.captured(2);
    } else {
      frame.symbol = demangled(match.captured(2).trimmed());
    }
    return frame;
  }
  if (const QRegularExpressionMatch match = mac.match(line); match.hasMatch()) {
    frame.module = match.captured(1);
    const QString symbol = match.captured(3);
    if (!symbol.startsWith(QStringLiteral("0x"))) {
      frame.symbol = demangled(symbol);
    }
    frame.offset = QStringLiteral("0x") + QString::number(match.captured(4).toULongLong(), 16);
    return frame;
  }
  if (const QRegularExpressionMatch match = windows.match(line); match.hasMatch()) {
    frame.module = baseName(match.captured(1));
    frame.symbol = match.captured(2);
    frame.offset = match.captured(3);
    return frame;
  }
  frame.module = line.trimmed();
  return frame;
}

QString describeFrame(const Frame& frame) {
  QString text = frame.module;
  if (!frame.symbol.isEmpty()) {
    text += QStringLiteral(": ") + frame.symbol;
  }
  if (!frame.offset.isEmpty()) {
    text += QStringLiteral(" + ") + frame.offset;
  }
  return text;
}

QStringList normalizedFrames(const QStringList& stack, int count) {
  QList<Frame> frames;
  for (const QString& line : stack) {
    frames << parseFrame(line);
  }
  qsizetype start = 0;
  while (start < frames.size() && machinery(frames[start])) {
    ++start;
  }
  QStringList keys;
  for (qsizetype i = start; i < frames.size() && keys.size() < count; ++i) {
    const Frame& frame = frames[i];
    keys << frame.module + QLatin1Char('!') +
                (frame.symbol.isEmpty() ? QStringLiteral("+") + frame.offset : frame.symbol);
  }
  return keys;
}

QStringList trimmedStack(const QStringList& stack) {
  qsizetype start = 0;
  while (start < stack.size() && machinery(parseFrame(stack[start]))) {
    ++start;
  }
  return stack.mid(start);
}

QString crashKey(const QString& signal, const QStringList& stack) {
  return shortKey((QStringLiteral("crash\n") + signal + QLatin1Char('\n') +
                   normalizedFrames(stack).join(QLatin1Char('\n')))
                      .toUtf8());
}

QString errorKey(const QString& context, const QString& message) {
  MaskContext none;
  QString normalized = mask(message, none);
  static const QRegularExpression hex(QStringLiteral(R"(0x[0-9a-fA-F]+)"));
  static const QRegularExpression numbers(QStringLiteral(R"(\d+(\.\d+)?)"));
  normalized.replace(hex, QStringLiteral("0x#"));
  normalized.replace(numbers, QStringLiteral("#"));
  return shortKey((QStringLiteral("error\n") + context + QLatin1Char('\n') + normalized.simplified()).toUtf8());
}

bool isKernelCrashMessage(const QString& message) {
  static const QRegularExpression crash(
      QStringLiteral(R"(SIG(SEGV|BUS|ILL|FPE|SYS) '[^']*' detected|SIGFPE Arithmetic exception|)"
                     R"(ACCESS VIOLATION at address|ILLEGAL INSTRUCTION|INTEGER DIVISION BY ZERO|STACK OVERFLOW|)"
                     R"(PRIVILEGED INSTRUCTION|IN_PAGE ERROR)"));
  return crash.match(message).hasMatch();
}

// ---------------------------------------------------------------------------
// The report and its delivery

QString reportBody(const Report& report, const QString& version) {
  QStringList parts;
  for (const Section& section : report.sections) {
    const QString text = section.text.trimmed();
    if (!section.included || text.isEmpty()) {
      continue;
    }
    QString part = QStringLiteral("### ") + section.title + QStringLiteral("\n\n");
    if (section.code) {
      // A fence the text does not contain.
      const QString fence = text.contains(QStringLiteral("```")) ? QStringLiteral("~~~~") : QStringLiteral("```");
      part += fence + QLatin1Char('\n') + text + QLatin1Char('\n') + fence;
    } else {
      part += text;
    }
    parts << part;
  }
  QString footer = QStringLiteral("<sub>Sent from Mitcad %1").arg(version);
  if (!report.key.isEmpty()) {
    footer += QStringLiteral(" · duplicate key `mitcad-%1`").arg(report.key);
  }
  footer += QStringLiteral("</sub>");
  parts << footer;
  return parts.join(QStringLiteral("\n\n")) + QLatin1Char('\n');
}

namespace {

QUrl formUrl(const QString& tracker, const QString& title, const QString& body) {
  if (tracker.contains(QStringLiteral("{title}")) || tracker.contains(QStringLiteral("{body}"))) {
    QString address = tracker;
    address.replace(QStringLiteral("{title}"), QString::fromLatin1(QUrl::toPercentEncoding(title)));
    address.replace(QStringLiteral("{body}"), QString::fromLatin1(QUrl::toPercentEncoding(body)));
    return QUrl(address, QUrl::StrictMode);
  }
  QUrl url(tracker.trimmed());
  QString path = url.path();
  while (path.endsWith(QLatin1Char('/'))) {
    path.chop(1);
  }
  if (!path.endsWith(QStringLiteral("/new"))) {
    path += QStringLiteral("/new");
  }
  url.setPath(path);
  // Encoded here: spaces as %20, '+' and '&' too, which some trackers read
  // otherwise.
  QString query = QStringLiteral("title=") + QString::fromLatin1(QUrl::toPercentEncoding(title)) +
                  QStringLiteral("&body=") + QString::fromLatin1(QUrl::toPercentEncoding(body));
  if (url.hasQuery()) {
    query = url.query(QUrl::FullyEncoded) + QLatin1Char('&') + query;
  }
  url.setQuery(query, QUrl::StrictMode);
  return url;
}

int encodedLength(const QUrl& url) { return static_cast<int>(url.toEncoded().size()); }

} // namespace

IssueLink issueLink(const QString& tracker, const QString& title, const QString& body, int maxLength) {
  IssueLink link;
  link.url = formUrl(tracker, title, body);
  if (encodedLength(link.url) <= maxLength) {
    return link;
  }
  link.shortened = true;
  link.clipboard = body;
  const QString note = QStringLiteral(
      "> **The whole report is on the clipboard:** it was too long for the link. Select this text and paste "
      "the report over it.\n\n");
  const QString cut = QStringLiteral("\n\n[... cut: the rest is on the clipboard]\n");
  // The longest beginning of the body, at a line's end, that fits.
  QStringList lines = body.split(QLatin1Char('\n'));
  qsizetype low = 0;
  qsizetype high = lines.size();
  while (low < high) {
    const qsizetype middle = (low + high + 1) / 2;
    const QString text = note + QStringList(lines.mid(0, middle)).join(QLatin1Char('\n')) + cut;
    if (encodedLength(formUrl(tracker, title, text)) <= maxLength) {
      low = middle;
    } else {
      high = middle - 1;
    }
  }
  link.url = formUrl(tracker, title, note + QStringList(lines.mid(0, low)).join(QLatin1Char('\n')) + cut);
  return link;
}

} // namespace mitcad::report
