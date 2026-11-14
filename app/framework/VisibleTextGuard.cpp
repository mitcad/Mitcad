// SPDX-License-Identifier: MIT
#include "framework/VisibleTextGuard.hpp"

#include <QAbstractButton>
#include <QAbstractItemModel>
#include <QAbstractItemView>
#include <QAction>
#include <QApplication>
#include <QComboBox>
#include <QFile>
#include <QFileDialog>
#include <QGroupBox>
#include <QHash>
#include <QHeaderView>
#include <QLabel>
#include <QLineEdit>
#include <QList>
#include <QMenu>
#include <QMenuBar>
#include <QPair>
#include <QPlainTextEdit>
#include <QPointer>
#include <QSet>
#include <QTabBar>
#include <QTextEdit>
#include <QTreeView>
#include <QWidget>
#include <QtLogging>

#include <algorithm>
#include <utility>

namespace mitcad {
namespace visible_text {
namespace {

// Not a letter, digit or underscore before or after: a whole word.
const QString kBefore = QStringLiteral("(?<![A-Za-z0-9_])");
const QString kAfter = QStringLiteral("(?![A-Za-z0-9_])");

const QRegularExpression& topologicalPattern() {
  // E{…}, V{…}, r{…}, and F3:side(…), F1:top, F2:end(…).
  static const QRegularExpression pattern(kBefore + QStringLiteral("(?:[EVr]\\{|F\\d+:(?:side|top|bottom|end|start)") +
                                          kAfter + QStringLiteral(")"));
  return pattern;
}

const QRegularExpression& idPattern() {
  // F57 (and F2 of F2.b0), O3, k12, p15, c4.
  static const QRegularExpression pattern(kBefore + QStringLiteral("(?:F|O|k|p|c)\\d{1,5}") + kAfter);
  return pattern;
}

// A text that is only keys: "F2", "Ctrl+F6", "F6, Shift+F6", also with a
// note after them, "F2 (reserved)" (Keyboard Shortcuts).
const QRegularExpression& keysOnlyPattern() {
  static const QRegularExpression pattern(
      QStringLiteral("^\\s*(?:(?:Ctrl|Shift|Alt|Meta|Cmd|Option|Command|Num|⌘|⇧|⌥|⌃)\\+?)*F\\d{1,2}"
                     "(?:\\s*[,/]\\s*(?:(?:Ctrl|Shift|Alt|Meta|Cmd|Option|Command|Num|⌘|⇧|⌥|⌃)\\+?)*[^\\s,/]+)*"
                     "(?:\\s+\\([^()]*\\))?\\s*$"));
  return pattern;
}

// Whether the F<n> at [start, end) of `text` names a function key.
bool functionKey(const QString& text, qsizetype start, qsizetype end) {
  bool number = false;
  const int n = text.mid(start + 1, end - start - 1).toInt(&number);
  if (!number || n < 1 || n > 35) {
    return false;
  }
  const QChar before = start > 0 ? text.at(start - 1) : QChar();
  const QChar after = end < text.size() ? text.at(end) : QChar();
  if (before == QLatin1Char('+')) {
    return true; // Ctrl+F6
  }
  if (before == QLatin1Char('(') && after == QLatin1Char(')')) {
    return true; // "Rename (F2)"
  }
  // "Press F2", "the F6 key", "F3 is used by both … and …" (Keyboard
  // Shortcuts' conflict).
  static const QRegularExpression keyAfter(QStringLiteral("^\\s*\\+|^\\s+(?:key|is used|is reserved|is taken)\\b"),
                                           QRegularExpression::CaseInsensitiveOption);
  if (text.left(start).trimmed().endsWith(QStringLiteral("press"), Qt::CaseInsensitive) ||
      keyAfter.match(text.mid(end)).hasMatch()) {
    return true;
  }
  return keysOnlyPattern().match(text).hasMatch();
}

} // namespace

QStringList identifiersIn(const QString& text) {
  QStringList found;
  QVector<QPair<qsizetype, qsizetype>> spans; // the topological names' places
  for (auto it = topologicalPattern().globalMatch(text); it.hasNext();) {
    const QRegularExpressionMatch match = it.next();
    spans.append({match.capturedStart(), match.capturedEnd()});
    if (!found.contains(match.captured())) {
      found << match.captured();
    }
  }
  for (auto it = idPattern().globalMatch(text); it.hasNext();) {
    const QRegularExpressionMatch match = it.next();
    const qsizetype start = match.capturedStart();
    const qsizetype end = match.capturedEnd();
    if (std::any_of(spans.cbegin(), spans.cend(),
                    [start](const QPair<qsizetype, qsizetype>& span) { return start >= span.first && start < span.second; })) {
      continue; // F3 of F3:side
    }
    if (text.at(start) == QLatin1Char('F') && functionKey(text, start, end)) {
      continue;
    }
    if (!found.contains(match.captured())) {
      found << match.captured();
    }
  }
  return found;
}

QString plainText(const QString& text) {
  if (!Qt::mightBeRichText(text)) {
    return text;
  }
  static const QRegularExpression breaks(QStringLiteral("<br\\s*/?>|</p>|</li>|</tr>"),
                                         QRegularExpression::CaseInsensitiveOption);
  static const QRegularExpression tags(QStringLiteral("<[^>]*>"));
  static const QRegularExpression numeric(QStringLiteral("&#(x?)([0-9A-Fa-f]+);"));
  QString plain = text;
  plain.replace(breaks, QStringLiteral("\n"));
  plain.replace(tags, QString());
  QString decoded;
  qsizetype last = 0;
  for (auto it = numeric.globalMatch(plain); it.hasNext();) {
    const QRegularExpressionMatch match = it.next();
    decoded += plain.mid(last, match.capturedStart() - last);
    bool ok = false;
    const uint code = match.captured(2).toUInt(&ok, match.captured(1).isEmpty() ? 10 : 16);
    decoded += ok ? QString::fromUcs4(reinterpret_cast<const char32_t*>(&code), 1) : match.captured();
    last = match.capturedEnd();
  }
  decoded += plain.mid(last);
  decoded.replace(QStringLiteral("&nbsp;"), QStringLiteral(" "))
      .replace(QStringLiteral("&lt;"), QStringLiteral("<"))
      .replace(QStringLiteral("&gt;"), QStringLiteral(">"))
      .replace(QStringLiteral("&quot;"), QStringLiteral("\""))
      .replace(QStringLiteral("&amp;"), QStringLiteral("&"));
  return decoded;
}

QVector<AllowEntry> parseAllowList(const QString& contents, QStringList* errors) {
  QVector<AllowEntry> entries;
  int number = 0;
  for (const QString& raw : contents.split(QLatin1Char('\n'))) {
    ++number;
    const QString line = raw.trimmed();
    if (line.isEmpty() || line.startsWith(QLatin1Char('#'))) {
      continue;
    }
    const QStringList fields = line.split(QStringLiteral(" | "));
    const auto fail = [&](const QString& why) {
      if (errors != nullptr) {
        *errors << QStringLiteral("line %1: %2").arg(number).arg(why);
      }
    };
    if (fields.size() < 3) {
      fail(QStringLiteral("not <token> | <text regex> | <reason>"));
      continue;
    }
    const QString token = fields.at(0).trimmed();
    AllowEntry entry{QRegularExpression(QRegularExpression::anchoredPattern(token)),
                     QRegularExpression(fields.at(1).trimmed()), fields.mid(2).join(QStringLiteral(" | ")).trimmed()};
    if (token.isEmpty() || fields.at(1).trimmed().isEmpty() || entry.reason.isEmpty()) {
      fail(QStringLiteral("an empty token, text or reason"));
      continue;
    }
    if (!entry.token.isValid() || !entry.text.isValid()) {
      fail(QStringLiteral("invalid regex: %1")
               .arg(entry.token.isValid() ? entry.text.errorString() : entry.token.errorString()));
      continue;
    }
    entries.append(entry);
  }
  return entries;
}

bool allowed(const QVector<AllowEntry>& entries, const QString& token, const QString& text) {
  return std::any_of(entries.cbegin(), entries.cend(), [&](const AllowEntry& entry) {
    return entry.token.match(token).hasMatch() && entry.text.match(text).hasMatch();
  });
}

QString objectPath(const QWidget* widget) {
  QStringList parts;
  for (const QObject* at = widget; at != nullptr; at = at->parent()) {
    parts.prepend(at->objectName().isEmpty() ? QString::fromLatin1(at->metaObject()->className()) : at->objectName());
  }
  return parts.join(QLatin1Char('/'));
}

} // namespace visible_text

namespace {

using Sources = QList<QPair<QPointer<QWidget>, std::function<QStringList()>>>;

Sources& sources() {
  static Sources all;
  return all;
}

// The allow-list named by MITCAD_TEST_TEXT_GUARD, read once.
const QVector<visible_text::AllowEntry>& allowList() {
  static const QVector<visible_text::AllowEntry> entries = [] {
    const QString path = qEnvironmentVariable("MITCAD_TEST_TEXT_GUARD");
    QFile file(path);
    if (path.isEmpty() || path == QStringLiteral("1") || !file.open(QIODevice::ReadOnly)) {
      return QVector<visible_text::AllowEntry>();
    }
    QStringList errors;
    QVector<visible_text::AllowEntry> read = visible_text::parseAllowList(QString::fromUtf8(file.readAll()), &errors);
    for (const QString& error : errors) {
      qDebug().noquote() << QStringLiteral("Identifier in visible text: the allow-list %1: %2").arg(path, error);
    }
    return read;
  }();
  return entries;
}

// The texts already logged (a widget's path, the identifier and the text).
QSet<QString>& reported() {
  static QSet<QString> seen;
  return seen;
}

// Whether a widget (or one above it) shows the application's log for
// people who read logs (the Debug Log window): its own property says so.
bool logView(const QWidget* widget) {
  for (const QWidget* at = widget; at != nullptr; at = at->parentWidget()) {
    if (at->property("mitcadLogText").toBool()) {
      return true;
    }
  }
  return false;
}

// Whether a widget is in a file dialog (Qt's own, on Linux): its folders'
// and files' names are the user's own, whatever they look like (a file
// named "part.b01149df-c335-….f3d" is no internal id).
bool inFileDialog(const QWidget* widget) {
  for (const QWidget* at = widget; at != nullptr; at = at->parentWidget()) {
    if (qobject_cast<const QFileDialog*>(at) != nullptr) {
      return true;
    }
  }
  return false;
}

class Reader {
public:
  int found = 0;

  void look(const QWidget* widget, const QString& raw) {
    if (raw.isEmpty()) {
      return;
    }
    const QString text = visible_text::plainText(raw);
    const QStringList tokens = visible_text::identifiersIn(text);
    if (tokens.isEmpty()) {
      return;
    }
    QString shown = text.simplified();
    if (shown.size() > 300) {
      shown = shown.left(300) + QStringLiteral("…");
    }
    for (const QString& token : tokens) {
      if (visible_text::allowed(allowList(), token, text)) {
        continue;
      }
      const QString path = visible_text::objectPath(widget);
      if (reported().contains(path + QLatin1Char('\n') + token + QLatin1Char('\n') + text)) {
        continue;
      }
      reported().insert(path + QLatin1Char('\n') + token + QLatin1Char('\n') + text);
      ++found;
      qDebug().noquote() << QStringLiteral("Identifier in visible text: %1: %2: %3").arg(path, token, shown);
    }
  }

  void itemView(const QAbstractItemView* view) {
    const QAbstractItemModel* model = view->model();
    if (model == nullptr) {
      return;
    }
    // A header shows its sections' titles; the rows are its view's.
    if (const auto* header = qobject_cast<const QHeaderView*>(view)) {
      for (int section = 0; section < header->count(); ++section) {
        if (!header->isSectionHidden(section)) {
          look(view, model->headerData(section, header->orientation(), Qt::DisplayRole).toString());
          look(view, model->headerData(section, header->orientation(), Qt::ToolTipRole).toString());
        }
      }
      return;
    }
    const auto* tree = qobject_cast<const QTreeView*>(view);
    int budget = 4000; // cells read in one view at the most
    std::function<void(const QModelIndex&)> rows = [&](const QModelIndex& parent) {
      const int count = model->rowCount(parent);
      const int columns = model->columnCount(parent);
      for (int row = 0; row < count && budget > 0; ++row) {
        if (tree != nullptr && tree->isRowHidden(row, parent)) {
          continue;
        }
        for (int column = 0; column < columns && budget > 0; ++column, --budget) {
          const QModelIndex index = model->index(row, column, parent);
          look(view, model->data(index, Qt::DisplayRole).toString());
          look(view, model->data(index, Qt::ToolTipRole).toString());
        }
        const QModelIndex first = model->index(row, 0, parent);
        if (tree != nullptr && tree->isExpanded(first)) {
          rows(first);
        }
      }
    };
    rows(view->rootIndex());
  }

  void widget(const QWidget* w) {
    if (logView(w) || inFileDialog(w)) {
      return;
    }
    if (w->isWindow()) {
      look(w, w->windowTitle());
    }
    look(w, w->toolTip());
    if (const auto* label = qobject_cast<const QLabel*>(w)) {
      look(w, label->text());
    } else if (const auto* button = qobject_cast<const QAbstractButton*>(w)) {
      look(w, button->text());
    } else if (const auto* box = qobject_cast<const QGroupBox*>(w)) {
      look(w, box->title());
    } else if (const auto* tabs = qobject_cast<const QTabBar*>(w)) {
      for (int i = 0; i < tabs->count(); ++i) {
        look(w, tabs->tabText(i));
        look(w, tabs->tabToolTip(i));
      }
    } else if (const auto* line = qobject_cast<const QLineEdit*>(w)) {
      look(w, line->placeholderText());
      if (line->isReadOnly()) {
        look(w, line->text());
      }
    } else if (const auto* edit = qobject_cast<const QTextEdit*>(w)) {
      if (edit->isReadOnly()) {
        look(w, edit->toPlainText());
      }
    } else if (const auto* plain = qobject_cast<const QPlainTextEdit*>(w)) {
      if (plain->isReadOnly()) {
        look(w, plain->toPlainText());
      }
    } else if (const auto* combo = qobject_cast<const QComboBox*>(w)) {
      if (!combo->isEditable()) {
        look(w, combo->currentText());
      }
    } else if (const auto* view = qobject_cast<const QAbstractItemView*>(w)) {
      itemView(view);
    } else if (const auto* menu = qobject_cast<const QMenu*>(w)) {
      for (const QAction* action : menu->actions()) {
        look(w, action->text());
      }
    } else if (const auto* bar = qobject_cast<const QMenuBar*>(w)) {
      for (const QAction* action : bar->actions()) {
        look(w, action->text());
      }
    }
  }
};

} // namespace

bool VisibleTextGuard::enabled() { return qEnvironmentVariableIsSet("MITCAD_TEST_TEXT_GUARD"); }

int VisibleTextGuard::check() {
  if (!enabled()) {
    return 0;
  }
  Reader reader;
  const QWidgetList windows = QApplication::topLevelWidgets();
  for (const QWidget* window : windows) {
    if (!window->isVisible()) {
      continue;
    }
    reader.widget(window);
    const QList<QWidget*> children = window->findChildren<QWidget*>();
    for (const QWidget* child : children) {
      if (child->isVisible()) {
        reader.widget(child);
      }
    }
  }
  Sources& all = sources();
  all.erase(std::remove_if(all.begin(), all.end(), [](const auto& source) { return source.first.isNull(); }),
            all.end());
  for (const auto& source : std::as_const(all)) {
    if (source.first->isVisible() && !logView(source.first)) {
      for (const QString& text : source.second()) {
        reader.look(source.first, text);
      }
    }
  }
  return reader.found;
}

void VisibleTextGuard::addSource(QWidget* widget, std::function<QStringList()> texts) {
  if (enabled() && widget != nullptr) {
    sources().append({QPointer<QWidget>(widget), std::move(texts)});
  }
}

} // namespace mitcad
