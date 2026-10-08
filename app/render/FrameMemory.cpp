// SPDX-License-Identifier: MIT
#include "render/FrameMemory.hpp"

#include <cstring>
#include <random>

#ifdef _WIN32
#include <QCoreApplication>
#include <QNativeIpcKey>
#include <QSharedMemory>
#include <QString>
#else
#include <cerrno>
#include <fcntl.h>
#include <sys/mman.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/uio.h>
#include <unistd.h>
#endif

namespace mitcad::render {

namespace {

#if defined(_WIN32) || !defined(__linux__)
// A random 64-bit number in hex: with the process id it makes a name no
// other process or run of the application uses.
std::string randomHex() {
  std::random_device device;
  const unsigned long long value = (static_cast<unsigned long long>(device()) << 32) ^ device();
  char text[17];
  static const char digits[] = "0123456789abcdef";
  for (int i = 0; i < 16; ++i) {
    text[i] = digits[(value >> (60 - 4 * i)) & 0xf];
  }
  text[16] = '\0';
  return text;
}
#endif

#ifndef _WIN32
std::string systemError(const char* what) { return std::string(what) + ": " + std::strerror(errno); }
#endif

} // namespace

#ifdef _WIN32

struct FrameMemory::Impl {
  std::unique_ptr<QSharedMemory> memory;
};

FrameMemory::FrameMemory() : m_impl(std::make_unique<Impl>()) {}
FrameMemory::~FrameMemory() = default;

bool FrameMemory::create(std::size_t bytes, std::string& error) {
  const QString name = QStringLiteral("mitcad-render-%1-%2")
                           .arg(QCoreApplication::applicationPid())
                           .arg(QString::fromStdString(randomHex()));
  auto memory = std::make_unique<QSharedMemory>(QSharedMemory::platformSafeKey(name));
  if (!memory->create(static_cast<qsizetype>(bytes))) {
    error = memory->errorString().toStdString();
    return false;
  }
  std::memset(memory->data(), 0, bytes);
  m_impl->memory = std::move(memory);
  return true;
}

std::string FrameMemory::key() const {
  return m_impl->memory ? m_impl->memory->nativeIpcKey().toString().toStdString() : std::string();
}

bool FrameMemory::attach(const std::string& key, std::string& error) {
  auto memory = std::make_unique<QSharedMemory>(QNativeIpcKey::fromString(QString::fromStdString(key)));
  if (!memory->attach()) {
    error = memory->errorString().toStdString();
    return false;
  }
  m_impl->memory = std::move(memory);
  return true;
}

void* FrameMemory::data() const { return m_impl->memory ? m_impl->memory->data() : nullptr; }

std::size_t FrameMemory::size() const {
  return m_impl->memory ? static_cast<std::size_t>(m_impl->memory->size()) : 0;
}

#else

struct FrameMemory::Impl {
  int descriptor = -1; // the application's, until the memory goes
  void* data = nullptr;
  std::size_t size = 0;

  ~Impl() {
    if (data != nullptr) {
      munmap(data, size);
    }
    if (descriptor >= 0) {
      close(descriptor);
    }
  }
  bool map(int fd, std::size_t bytes, std::string& error) {
    void* mapped = mmap(nullptr, bytes, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (mapped == MAP_FAILED) {
      error = systemError("mmap");
      return false;
    }
    data = mapped;
    size = bytes;
    return true;
  }
};

FrameMemory::FrameMemory() : m_impl(std::make_unique<Impl>()) {}
FrameMemory::~FrameMemory() = default;

bool FrameMemory::create(std::size_t bytes, std::string& error) {
#ifdef __linux__
  const int fd = memfd_create("mitcad-render-frames", MFD_CLOEXEC);
  if (fd < 0) {
    error = systemError("memfd_create");
    return false;
  }
#else
  // No anonymous shared memory to pass on: a name of its own, gone at once.
  const std::string name = "/mitcad-" + std::to_string(getpid()) + "-" + randomHex().substr(0, 12);
  const int fd = shm_open(name.c_str(), O_RDWR | O_CREAT | O_EXCL, 0600);
  if (fd < 0) {
    error = systemError("shm_open");
    return false;
  }
  shm_unlink(name.c_str());
  fcntl(fd, F_SETFD, FD_CLOEXEC);
#endif
  if (ftruncate(fd, static_cast<off_t>(bytes)) != 0) {
    error = systemError("ftruncate");
    close(fd);
    return false;
  }
  auto impl = std::make_unique<Impl>();
  impl->descriptor = fd;
  if (!impl->map(fd, bytes, error)) {
    return false;
  }
  // New pages of both kinds read as zeros.
  m_impl = std::move(impl);
  return true;
}

int FrameMemory::descriptor() const { return m_impl->descriptor; }

bool FrameMemory::attach(int descriptor, std::string& error) {
  auto impl = std::make_unique<Impl>();
  struct stat status {};
  const bool ok = fstat(descriptor, &status) == 0 && status.st_size > 0;
  if (!ok) {
    error = systemError("fstat");
  } else {
    impl->map(descriptor, static_cast<std::size_t>(status.st_size), error);
  }
  close(descriptor);
  if (impl->data == nullptr) {
    return false;
  }
  m_impl = std::move(impl);
  return true;
}

void* FrameMemory::data() const { return m_impl->data; }

std::size_t FrameMemory::size() const { return m_impl->size; }

// --- FrameChannel ---------------------------------------------------------

FrameChannel::~FrameChannel() {
  if (m_own >= 0) {
    close(m_own);
  }
  closeWorkerEnd();
}

bool FrameChannel::open(std::string& error) {
  int ends[2];
  if (socketpair(AF_UNIX, SOCK_STREAM, 0, ends) != 0) {
    error = systemError("socketpair");
    return false;
  }
  for (const int end : ends) {
    fcntl(end, F_SETFD, FD_CLOEXEC);
#ifdef SO_NOSIGPIPE
    // A worker that is gone must not end the application with SIGPIPE.
    const int on = 1;
    setsockopt(end, SOL_SOCKET, SO_NOSIGPIPE, &on, sizeof on);
#endif
  }
  m_own = ends[0];
  m_worker = ends[1];
  return true;
}

void FrameChannel::closeWorkerEnd() {
  if (m_worker >= 0) {
    close(m_worker);
    m_worker = -1;
  }
}

void FrameChannel::makeInheritable(int descriptor) { fcntl(descriptor, F_SETFD, 0); }

void FrameChannel::adopt(int descriptor) {
  m_own = descriptor;
  fcntl(descriptor, F_SETFD, FD_CLOEXEC);
}

bool FrameChannel::send(int descriptor, std::string& error) {
  char byte = 'F';
  iovec data{&byte, 1};
  alignas(cmsghdr) char control[CMSG_SPACE(sizeof(int))];
  std::memset(control, 0, sizeof control);
  msghdr message{};
  message.msg_iov = &data;
  message.msg_iovlen = 1;
  message.msg_control = control;
  message.msg_controllen = sizeof control;
  cmsghdr* header = CMSG_FIRSTHDR(&message);
  header->cmsg_level = SOL_SOCKET;
  header->cmsg_type = SCM_RIGHTS;
  header->cmsg_len = CMSG_LEN(sizeof(int));
  std::memcpy(CMSG_DATA(header), &descriptor, sizeof(int));
#ifdef MSG_NOSIGNAL
  const int flags = MSG_NOSIGNAL;
#else
  const int flags = 0;
#endif
  ssize_t sent = -1;
  do {
    sent = sendmsg(m_own, &message, flags);
  } while (sent < 0 && errno == EINTR);
  if (sent != 1) {
    error = systemError("sendmsg");
    return false;
  }
  return true;
}

int FrameChannel::receive(std::string& error) {
  char byte = 0;
  iovec data{&byte, 1};
  alignas(cmsghdr) char control[CMSG_SPACE(sizeof(int))];
  msghdr message{};
  message.msg_iov = &data;
  message.msg_iovlen = 1;
  message.msg_control = control;
  message.msg_controllen = sizeof control;
#ifdef MSG_CMSG_CLOEXEC
  const int flags = MSG_CMSG_CLOEXEC;
#else
  const int flags = 0;
#endif
  ssize_t received = -1;
  do {
    received = recvmsg(m_own, &message, flags);
  } while (received < 0 && errno == EINTR);
  if (received != 1) {
    error = received == 0 ? std::string("the application closed the frame channel") : systemError("recvmsg");
    return -1;
  }
  const cmsghdr* header = CMSG_FIRSTHDR(&message);
  if (header == nullptr || header->cmsg_level != SOL_SOCKET || header->cmsg_type != SCM_RIGHTS ||
      header->cmsg_len != CMSG_LEN(sizeof(int))) {
    error = "no file descriptor in the frame channel's message";
    return -1;
  }
  int descriptor = -1;
  std::memcpy(&descriptor, CMSG_DATA(header), sizeof(int));
  return descriptor;
}

#endif

} // namespace mitcad::render
