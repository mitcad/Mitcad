// SPDX-License-Identifier: MIT
#include "WindowsEndpointSecurity.hpp"

#ifdef Q_OS_WIN
#include <array>
#include <cstddef>
#include <memory>
#include <string>
#include <vector>

#include <QDir>
#include <QFileInfo>

#include <windows.h>
#include <aclapi.h>

namespace mitcad::mcp {
namespace {

struct CloseHandleDeleter {
  void operator()(void* handle) const { CloseHandle(handle); }
};

struct LocalFreeDeleter {
  void operator()(void* allocation) const { LocalFree(allocation); }
};

using OwnedHandle = std::unique_ptr<void, CloseHandleDeleter>;
using LocalAllocation = std::unique_ptr<void, LocalFreeDeleter>;

struct Identity {
  std::vector<BYTE> tokenUser;
  alignas(DWORD) std::array<BYTE, SECURITY_MAX_SID_SIZE> system{};
  alignas(DWORD) std::array<BYTE, SECURITY_MAX_SID_SIZE> administrators{};

  bool load() {
    HANDLE rawToken = nullptr;
    if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &rawToken)) {
      return false;
    }
    const OwnedHandle token(rawToken);
    DWORD bytes = 0;
    if (GetTokenInformation(token.get(), TokenUser, nullptr, 0, &bytes) ||
        GetLastError() != ERROR_INSUFFICIENT_BUFFER || bytes < sizeof(TOKEN_USER)) {
      return false;
    }
    tokenUser.resize(bytes);
    if (!GetTokenInformation(token.get(), TokenUser, tokenUser.data(), bytes, &bytes) || !IsValidSid(user())) {
      return false;
    }
    bytes = static_cast<DWORD>(system.size());
    if (!CreateWellKnownSid(WinLocalSystemSid, nullptr, system.data(), &bytes)) {
      return false;
    }
    bytes = static_cast<DWORD>(administrators.size());
    return CreateWellKnownSid(WinBuiltinAdministratorsSid, nullptr, administrators.data(), &bytes) != FALSE;
  }

  PSID user() const { return reinterpret_cast<const TOKEN_USER*>(tokenUser.data())->User.Sid; }

  bool trusted(PSID sid) {
    return sid != nullptr && IsValidSid(sid) &&
           (EqualSid(sid, user()) || EqualSid(sid, system.data()) || EqualSid(sid, administrators.data()));
  }
};

bool ordinaryPath(const QString& path) {
  const QFileInfo info(path);
  if (!info.exists() || info.isSymLink() || info.isJunction() || (!info.isFile() && !info.isDir())) {
    return false;
  }
  const std::wstring native = QDir::toNativeSeparators(info.absoluteFilePath()).toStdWString();
  const DWORD attributes = GetFileAttributesW(native.c_str());
  // Reject every reparse point, including types QFileInfo cannot classify.
  return attributes != INVALID_FILE_ATTRIBUTES && !(attributes & FILE_ATTRIBUTE_REPARSE_POINT);
}

// Metadata writes, deleting a directory's children or changing its ACL can
// replace a discovery record just as a content write can. SYSTEM and local
// administrators are trusted because they can bypass these ACLs already.
constexpr ACCESS_MASK kPrivateRights =
    FILE_READ_DATA | FILE_WRITE_DATA | FILE_APPEND_DATA | FILE_READ_EA | FILE_WRITE_EA | FILE_EXECUTE |
    FILE_DELETE_CHILD | FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES | DELETE | WRITE_DAC | WRITE_OWNER |
    GENERIC_ALL | GENERIC_READ | GENERIC_WRITE | GENERIC_EXECUTE | MAXIMUM_ALLOWED;

// Full access for the user and SYSTEM only; a directory passes it to its
// children. The caller frees the ACL with LocalFree; null on failure.
PACL privateAcl(Identity& identity, bool directory) {
  EXPLICIT_ACCESSW access[2]{};
  const PSID trustees[] = {identity.user(), identity.system.data()};
  for (std::size_t index = 0; index < 2; ++index) {
    access[index].grfAccessPermissions = FILE_ALL_ACCESS;
    access[index].grfAccessMode = SET_ACCESS;
    access[index].grfInheritance = directory ? SUB_CONTAINERS_AND_OBJECTS_INHERIT : NO_INHERITANCE;
    access[index].Trustee.TrusteeForm = TRUSTEE_IS_SID;
    access[index].Trustee.TrusteeType = TRUSTEE_IS_UNKNOWN;
    access[index].Trustee.ptstrName = static_cast<LPWSTR>(trustees[index]);
  }
  PACL acl = nullptr;
  if (SetEntriesInAclW(2, access, nullptr, &acl) != ERROR_SUCCESS) {
    if (acl) {
      LocalFree(acl);
    }
    return nullptr;
  }
  return acl;
}

} // namespace

bool privateWindowsEndpointPath(const QString& path) {
  if (!ordinaryPath(path)) {
    return false;
  }
  const bool directory = QFileInfo(path).isDir();
  Identity identity;
  if (!identity.load()) {
    return false;
  }
  const std::wstring native = QDir::toNativeSeparators(QFileInfo(path).absoluteFilePath()).toStdWString();
  PSID owner = nullptr;
  PACL dacl = nullptr;
  PSECURITY_DESCRIPTOR rawDescriptor = nullptr;
  const DWORD status = GetNamedSecurityInfoW(native.c_str(), SE_FILE_OBJECT,
                                            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                                            &owner, nullptr, &dacl, nullptr, &rawDescriptor);
  const LocalAllocation descriptor(rawDescriptor);
  if (status != ERROR_SUCCESS || owner == nullptr || !IsValidSid(owner) || !EqualSid(owner, identity.user()) ||
      dacl == nullptr || !IsValidAcl(dacl)) {
    // A null DACL grants everyone access; ownership alone proves nothing.
    return false;
  }
  for (DWORD index = 0; index < dacl->AceCount; ++index) {
    void* rawAce = nullptr;
    if (!GetAce(dacl, index, &rawAce)) {
      return false;
    }
    const auto* header = static_cast<const ACE_HEADER*>(rawAce);
    if ((header->AceFlags & INHERIT_ONLY_ACE) && !directory) {
      continue; // Files cannot pass this grant to a token-bearing child.
    }
    // A directory's inherit-only grants can expose QSaveFile's temporary
    // token file even when the directory itself has a restrictive ACL.
    switch (header->AceType) {
    case ACCESS_DENIED_ACE_TYPE:
    case ACCESS_DENIED_OBJECT_ACE_TYPE:
    case ACCESS_DENIED_CALLBACK_ACE_TYPE:
    case ACCESS_DENIED_CALLBACK_OBJECT_ACE_TYPE:
      continue; // Denials cannot grant another account access.
    case ACCESS_ALLOWED_ACE_TYPE: {
      constexpr std::size_t sidOffset = offsetof(ACCESS_ALLOWED_ACE, SidStart);
      if (header->AceSize < sidOffset + offsetof(SID, SubAuthority)) {
        return false;
      }
      auto* allowed = static_cast<ACCESS_ALLOWED_ACE*>(rawAce);
      PSID trustee = &allowed->SidStart;
      if (!IsValidSid(trustee) || GetLengthSid(trustee) > header->AceSize - sidOffset) {
        return false;
      }
      if ((allowed->Mask & kPrivateRights) && !identity.trusted(trustee)) {
        return false;
      }
      break;
    }
    default:
      // Object-specific or conditional grants cannot be proved private by
      // this simple file ACL policy. Refuse them instead of overlooking one.
      return false;
    }
  }
  return true;
}

bool secureWindowsEndpointPath(const QString& path, bool directory) {
  const QFileInfo info(path);
  if (!ordinaryPath(path) || info.isDir() != directory) {
    return false;
  }
  Identity identity;
  if (!identity.load()) {
    return false;
  }
  std::wstring native = QDir::toNativeSeparators(info.absoluteFilePath()).toStdWString();
  PSID owner = nullptr;
  PSECURITY_DESCRIPTOR rawDescriptor = nullptr;
  const DWORD status = GetNamedSecurityInfoW(native.c_str(), SE_FILE_OBJECT, OWNER_SECURITY_INFORMATION,
                                            &owner, nullptr, nullptr, nullptr, &rawDescriptor);
  const LocalAllocation descriptor(rawDescriptor);
  if (status != ERROR_SUCCESS || !identity.trusted(owner)) {
    return false;
  }
  const LocalAllocation acl(privateAcl(identity, directory));
  if (!acl) {
    return false;
  }
  // Replace inherited permissions, set a protected DACL and ensure that an
  // elevated process's default group owner does not own the discovery data.
  if (SetNamedSecurityInfoW(native.data(), SE_FILE_OBJECT,
                           OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION |
                               PROTECTED_DACL_SECURITY_INFORMATION,
                           identity.user(), nullptr, static_cast<PACL>(acl.get()), nullptr) != ERROR_SUCCESS) {
    return false;
  }
  return privateWindowsEndpointPath(path);
}

bool createPrivateWindowsDirectory(const QString& path) {
  Identity identity;
  if (!identity.load()) {
    return false;
  }
  const LocalAllocation acl(privateAcl(identity, true));
  SECURITY_DESCRIPTOR descriptor{};
  if (!acl || !InitializeSecurityDescriptor(&descriptor, SECURITY_DESCRIPTOR_REVISION) ||
      !SetSecurityDescriptorOwner(&descriptor, identity.user(), FALSE) ||
      !SetSecurityDescriptorDacl(&descriptor, TRUE, static_cast<PACL>(acl.get()), FALSE) ||
      !SetSecurityDescriptorControl(&descriptor, SE_DACL_PROTECTED, SE_DACL_PROTECTED)) {
    return false;
  }
  SECURITY_ATTRIBUTES attributes{};
  attributes.nLength = sizeof(attributes);
  attributes.lpSecurityDescriptor = &descriptor;
  attributes.bInheritHandle = FALSE;
  const std::wstring native = QDir::toNativeSeparators(QFileInfo(path).absoluteFilePath()).toStdWString();
  return CreateDirectoryW(native.c_str(), &attributes) != FALSE;
}

bool windowsProcessRunning(qint64 pid) {
  if (pid <= 0 || pid > static_cast<qint64>(MAXDWORD)) {
    return false;
  }
  HANDLE raw = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, FALSE, static_cast<DWORD>(pid));
  if (!raw) {
    // No process has the id; access denied (another user's or a protected
    // process) and other errors cannot tell.
    return GetLastError() != ERROR_INVALID_PARAMETER;
  }
  const OwnedHandle process(raw);
  // An exited process stays signalled while a handle keeps it open.
  return WaitForSingleObject(process.get(), 0) != WAIT_OBJECT_0;
}

} // namespace mitcad::mcp
#endif
