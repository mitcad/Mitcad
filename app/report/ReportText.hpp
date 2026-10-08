// SPDX-License-Identifier: MIT
#pragma once

// The text of feedback and error reports (mitcad#61, mitcad#62), shared by
// Help > Send Feedback and the reports offered after a crash or an internal
// error: masking of personal data, crash report files, the duplicate key
// of a stack trace, the report's Markdown and the prefilled issue form's
// address. Qt Core only, no windows (app.unit tests it).

#include <QList>
#include <QString>
#include <QStringList>
#include <QUrl>

namespace mitcad::report {

// ---------------------------------------------------------------------------
// Masking

// What masking hides: the home folder, the user's and the computer's
// names.
struct MaskContext {
  QString home; // the home folder, as the system writes it
  QString user; // the login name
  QString host; // the computer's name
};

// This process's user, home folder and computer.
MaskContext currentMaskContext();

// The text with personal data masked:
// - the home folder becomes `<home>` wherever it occurs (also with spaces
//   in it, or the other kind of slash);
// - then every absolute path (`/a/b`, `C:\a`, `\\server\share`, `~/a`,
//   `<home>/a`) becomes `<path>`, followed by the file's name when it is a
//   program, library or source file (`<path>/libTKernel.so.8`,
//   `<path>/guard.cpp`; a stack frame stays readable) and otherwise by its
//   extension only (`<path>/<file>.f3d`): a design's name is the user's;
// - the user's and the computer's names, as whole words, become `<user>`
//   and `<host>` (not when they are "mitcad" or "localhost", or shorter
//   than three characters, where they would mask ordinary words; paths
//   with them are masked anyway).
QString mask(const QString& text, const MaskContext& context);

// ---------------------------------------------------------------------------
// Crash reports (written by CrashHandler.cpp)

struct CrashReport {
  bool valid = false;
  QString file;     // where it was read from
  QString process;  // "app", "import-worker", "render-worker"
  QString version;
  QString platform;
  qint64 pid = 0;
  qint64 parent = 0; // the application whose worker it was; 0: none
  qint64 time = 0;   // seconds since 1970
  QString signal;    // "SIGSEGV", "EXCEPTION_ACCESS_VIOLATION", ...
  QString address;
  QString message;   // a panic's or an uncaught exception's
  QStringList stack; // as written: a frame per line
  QStringList actions;
};

CrashReport parseCrashReport(const QByteArray& text);

// One frame of a stack as the handlers write it: glibc's backtrace_symbols
// (`/p/libX.so(symbol+0x1a) [0x7f..]`, `/p/mitcad(+0x1234) [0x55..]`),
// macOS's (`3   mitcad   0x0000000100003f40 symbol + 52`) or Windows'
// (`C:\p\mitcad.exe+0x1234`).
struct Frame {
  QString module; // the file's name, without its folder
  QString symbol; // demangled where the platform can; empty: unknown
  QString offset; // the offset in the symbol or, without one, the module
};

Frame parseFrame(const QString& line);

// A frame for people: "mitcad: mitcad::MainWindow::save() + 0x1a".
QString describeFrame(const Frame& frame);

// The frames that identify a crash: the handlers' own frames, the signal's
// delivery and abort() at the top left out, at most `count` after them,
// each as "module!symbol" (or "module!+offset" without a symbol), so that
// addresses that change from run to run do not matter.
QStringList normalizedFrames(const QStringList& stack, int count = 8);

// The stack without the crash's machinery at its top (as normalizedFrames
// leaves it out): what a report shows.
QStringList trimmedStack(const QStringList& stack);

// The duplicate key of a crash: 12 hex digits of the SHA-256 of the
// signal and the normalised frames.
QString crashKey(const QString& signal, const QStringList& stack);

// The duplicate key of an internal error without a stack: of its context
// (the operation) and its message with numbers, addresses and paths
// replaced, so that the same error in another design or place matches.
QString errorKey(const QString& context, const QString& message);

// Whether an error message is a crash inside the geometry kernel that
// OCCT's handlers turned into an error ("fillet: SIGSEGV 'segmentation
// violation' detected", on Windows "ACCESS VIOLATION at address ..."): an
// internal error, as the operation failed for no fault of the input.
bool isKernelCrashMessage(const QString& message);

// ---------------------------------------------------------------------------
// The report and its delivery

struct Section {
  QString id;      // "description", "diagnostics", "stack", ...
  QString title;   // its heading in the report
  QString text;
  bool included = true;
  bool code = false;   // shown as a code block (logs, stacks)
  bool masked = true;  // masked before the preview (not the contact address)
};

struct Report {
  QString title;
  QList<Section> sections;
  QString key; // the duplicate key, empty for feedback
};

// The report as Markdown: each included section with text under its
// heading, code sections fenced, the duplicate key last.
QString reportBody(const Report& report, const QString& version);

// The longest prefilled address sent to a browser: issue trackers refuse
// longer ones (414), and so do some browsers.
constexpr int kMaxUrlLength = 8000;

// The prefilled issue form for `tracker`: an issue list's address
// (`https://host/owner/repo/issues`, `.../issues/new`: the form gets
// `title` and `body` as query parameters, as GitHub, Forgejo and Gitea
// read them) or a template with `{title}` and `{body}` where they go.
// A body too long for kMaxUrlLength is cut at a line, with a note at the
// top that the whole report is on the clipboard (`clipboard`, empty when
// it all fit).
struct IssueLink {
  QUrl url;
  bool shortened = false;
  QString clipboard;
};

IssueLink issueLink(const QString& tracker, const QString& title, const QString& body,
                    int maxLength = kMaxUrlLength);

} // namespace mitcad::report
