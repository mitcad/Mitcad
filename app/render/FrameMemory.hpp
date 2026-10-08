// SPDX-License-Identifier: MIT
#pragma once

// The shared memory the render worker's frames arrive in (RenderProtocol.hpp,
// docs/rendering.md), made so that it cannot outlive both processes:
//
// - POSIX (Linux, macOS): an anonymous segment (Linux: memfd_create; other
//   systems: shm_open with a random name that is unlinked at once). It has
//   no name in the file system: the application passes its file descriptor
//   to the worker over a Unix socket (FrameChannel, SCM_RIGHTS), and the
//   segment is gone when both have unmapped it or ended, also after a
//   crash or a kill.
// - Windows: a named QSharedMemory with a name unique to the application's
//   process and run; Windows frees it with the last handle.
//
// Plain C++17 but for Windows' QSharedMemory.

#include <cstddef>
#include <memory>
#include <string>

namespace mitcad::render {

class FrameMemory {
public:
  FrameMemory();
  ~FrameMemory();
  FrameMemory(const FrameMemory&) = delete;
  FrameMemory& operator=(const FrameMemory&) = delete;

  // The application: a new segment of `bytes`, zeroed.
  bool create(std::size_t bytes, std::string& error);
#ifdef _WIN32
  // The segment's name for the worker's "memory" command.
  std::string key() const;
  // The worker: the application's segment.
  bool attach(const std::string& key, std::string& error);
#else
  // The segment's file descriptor (for FrameChannel::send), -1 when the
  // memory is not created or was attached.
  int descriptor() const;
  // The worker: maps a segment received from FrameChannel::receive and
  // closes the descriptor.
  bool attach(int descriptor, std::string& error);
#endif

  void* data() const;
  std::size_t size() const;

private:
  struct Impl;
  std::unique_ptr<Impl> m_impl;
};

#ifndef _WIN32
// A Unix socket pair between the application and the worker that carries
// FrameMemory's file descriptors.
class FrameChannel {
public:
  FrameChannel() = default;
  ~FrameChannel();
  FrameChannel(const FrameChannel&) = delete;
  FrameChannel& operator=(const FrameChannel&) = delete;

  // The application: a new pair; both ends close on exec. The worker's end
  // is made inheritable in the child (makeInheritable, called between fork
  // and exec) and closed here once the worker started (closeWorkerEnd).
  bool open(std::string& error);
  int workerEnd() const { return m_worker; }
  void closeWorkerEnd();
  // In the child process after fork (async-signal-safe).
  static void makeInheritable(int descriptor);

  // The worker: its end, from the command line.
  void adopt(int descriptor);

  // Sends a descriptor to the other end; it stays open here.
  bool send(int descriptor, std::string& error);
  // Receives one (blocks until it comes); -1 on an error.
  int receive(std::string& error);

private:
  int m_own = -1;
  int m_worker = -1;
};
#endif

} // namespace mitcad::render
