#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <winspool.h>
#include <winsplp.h>
#include <wtsapi32.h>
#include <userenv.h>
#include <aclapi.h>
#include <sddl.h>

#include <atomic>
#include <cstring>
#include <cwchar>
#include <cstddef>
#include <new>
#include <string>
#include <vector>

#pragma comment(lib, "Advapi32.lib")
#pragma comment(lib, "Userenv.lib")
#pragma comment(lib, "Wtsapi32.lib")

namespace {
constexpr wchar_t kPortName[] = L"StampaCP:";
constexpr wchar_t kMonitorName[] = L"StampaCP Monitor";
std::atomic_bool g_port_enabled{true};
MONITOR2 g_monitor{};

struct PortContext {
    HANDLE file = INVALID_HANDLE_VALUE;
    std::wstring spool_path;
};

struct XcvContext {
    ACCESS_MASK granted_access = 0;
};

bool IsOurPort(LPCWSTR name) {
    return name && _wcsicmp(name, kPortName) == 0;
}

bool SetJobFileAcl(const std::wstring& path, HANDLE user_token) {
    DWORD bytes = 0;
    GetTokenInformation(user_token, TokenUser, nullptr, 0, &bytes);
    if (GetLastError() != ERROR_INSUFFICIENT_BUFFER || bytes == 0) return false;

    std::vector<BYTE> token_data(bytes);
    if (!GetTokenInformation(user_token, TokenUser, token_data.data(), bytes, &bytes)) return false;
    auto* token_user = reinterpret_cast<TOKEN_USER*>(token_data.data());

    PSID system_sid = nullptr;
    PSID admins_sid = nullptr;
    if (!ConvertStringSidToSidW(L"S-1-5-18", &system_sid) ||
        !ConvertStringSidToSidW(L"S-1-5-32-544", &admins_sid)) {
        if (system_sid) LocalFree(system_sid);
        if (admins_sid) LocalFree(admins_sid);
        return false;
    }

    EXPLICIT_ACCESSW entries[3]{};
    entries[0].grfAccessPermissions = FILE_GENERIC_READ;
    entries[0].grfAccessMode = SET_ACCESS;
    entries[0].grfInheritance = NO_INHERITANCE;
    BuildTrusteeWithSidW(&entries[0].Trustee, token_user->User.Sid);

    entries[1].grfAccessPermissions = GENERIC_ALL;
    entries[1].grfAccessMode = SET_ACCESS;
    entries[1].grfInheritance = NO_INHERITANCE;
    BuildTrusteeWithSidW(&entries[1].Trustee, system_sid);

    entries[2].grfAccessPermissions = GENERIC_ALL;
    entries[2].grfAccessMode = SET_ACCESS;
    entries[2].grfInheritance = NO_INHERITANCE;
    BuildTrusteeWithSidW(&entries[2].Trustee, admins_sid);

    PACL acl = nullptr;
    const DWORD acl_result = SetEntriesInAclW(3, entries, nullptr, &acl);
    LocalFree(system_sid);
    LocalFree(admins_sid);
    if (acl_result != ERROR_SUCCESS) return false;

    const DWORD result = SetNamedSecurityInfoW(
        const_cast<LPWSTR>(path.c_str()), SE_FILE_OBJECT,
        DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
        nullptr, nullptr, acl, nullptr);
    LocalFree(acl);
    return result == ERROR_SUCCESS;
}

std::wstring GetMonitorDirectory() {
    HMODULE module = nullptr;
    if (!GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            reinterpret_cast<LPCWSTR>(&GetMonitorDirectory), &module)) {
        return {};
    }

    std::vector<wchar_t> buffer(32768);
    const DWORD length = GetModuleFileNameW(module, buffer.data(), static_cast<DWORD>(buffer.size()));
    if (length == 0 || length >= buffer.size()) return {};
    std::wstring path(buffer.data(), length);
    const size_t slash = path.find_last_of(L"\\/");
    if (slash == std::wstring::npos) return {};
    path.resize(slash);
    return path;
}

std::wstring QuoteArgument(const std::wstring& value) {
    std::wstring quoted = L"\"";
    size_t slashes = 0;
    for (wchar_t ch : value) {
        if (ch == L'\\') {
            ++slashes;
        } else if (ch == L'"') {
            quoted.append(slashes * 2 + 1, L'\\');
            quoted.push_back(ch);
            slashes = 0;
        } else {
            quoted.append(slashes, L'\\');
            slashes = 0;
            quoted.push_back(ch);
        }
    }
    quoted.append(slashes * 2, L'\\');
    quoted.push_back(L'"');
    return quoted;
}

bool LaunchGuiForActiveUser(const std::wstring& job_path) {
    const std::wstring directory = GetMonitorDirectory();
    if (directory.empty()) {
        SetLastError(ERROR_PATH_NOT_FOUND);
        return false;
    }
    const std::wstring executable = directory + L"\\StampaCP.exe";
    const DWORD session_id = WTSGetActiveConsoleSessionId();
    if (session_id == 0xFFFFFFFF) {
        SetLastError(ERROR_NO_SUCH_LOGON_SESSION);
        return false;
    }

    HANDLE user_token = nullptr;
    if (!WTSQueryUserToken(session_id, &user_token)) return false;

    const bool acl_ok = SetJobFileAcl(job_path, user_token);
    if (!acl_ok) {
        CloseHandle(user_token);
        SetLastError(ERROR_ACCESS_DENIED);
        return false;
    }

    LPVOID environment = nullptr;
    if (!CreateEnvironmentBlock(&environment, user_token, FALSE)) {
        CloseHandle(user_token);
        return false;
    }

    std::wstring command = QuoteArgument(executable) + L" --spool-job " + QuoteArgument(job_path);
    std::vector<wchar_t> mutable_command(command.begin(), command.end());
    mutable_command.push_back(L'\0');

    STARTUPINFOW startup{};
    startup.cb = sizeof(startup);
    startup.lpDesktop = const_cast<LPWSTR>(L"winsta0\\default");
    PROCESS_INFORMATION process{};

    const BOOL created = CreateProcessAsUserW(
        user_token, executable.c_str(), mutable_command.data(),
        nullptr, nullptr, FALSE, CREATE_UNICODE_ENVIRONMENT,
        environment, directory.c_str(), &startup, &process);

    const DWORD error = created ? ERROR_SUCCESS : GetLastError();
    DestroyEnvironmentBlock(environment);
    CloseHandle(user_token);

    if (!created) {
        SetLastError(error);
        return false;
    }
    CloseHandle(process.hThread);
    CloseHandle(process.hProcess);
    return true;
}

BOOL WINAPI EnumPorts(HANDLE, LPWSTR, DWORD level, LPBYTE ports,
                      DWORD buffer_size, LPDWORD needed, LPDWORD returned) {
    if (!needed || !returned) {
        SetLastError(ERROR_INVALID_PARAMETER);
        return FALSE;
    }
    *needed = 0;
    *returned = 0;
    if (!g_port_enabled.load()) return TRUE;

    const DWORD name_bytes = static_cast<DWORD>(sizeof(kPortName));
    const DWORD monitor_bytes = static_cast<DWORD>(sizeof(kMonitorName));
    DWORD required = 0;
    if (level == 1) {
        required = sizeof(PORT_INFO_1W) + name_bytes;
    } else if (level == 2) {
        required = sizeof(PORT_INFO_2W) + name_bytes + monitor_bytes + monitor_bytes;
    } else {
        SetLastError(ERROR_INVALID_LEVEL);
        return FALSE;
    }
    *needed = required;
    if (!ports || buffer_size < required) {
        SetLastError(ERROR_INSUFFICIENT_BUFFER);
        return FALSE;
    }

    if (level == 1) {
        auto* info = reinterpret_cast<PORT_INFO_1W*>(ports);
        auto* name = reinterpret_cast<LPWSTR>(ports + sizeof(PORT_INFO_1W));
        memcpy(name, kPortName, name_bytes);
        info->pName = name;
    } else {
        auto* info = reinterpret_cast<PORT_INFO_2W*>(ports);
        BYTE* cursor = ports + sizeof(PORT_INFO_2W);
        auto* port_name = reinterpret_cast<LPWSTR>(cursor);
        memcpy(port_name, kPortName, name_bytes);
        cursor += name_bytes;
        auto* monitor_name = reinterpret_cast<LPWSTR>(cursor);
        memcpy(monitor_name, kMonitorName, monitor_bytes);
        cursor += monitor_bytes;
        auto* description = reinterpret_cast<LPWSTR>(cursor);
        memcpy(description, kMonitorName, monitor_bytes);
        info->pServerName = nullptr;
        info->pPrinterName = nullptr;
        info->pPortName = port_name;
        info->pMonitorName = monitor_name;
        info->pDescription = description;
        info->fPortType = PORT_TYPE_WRITE;
        info->Reserved = 0;
    }
    *returned = 1;
    return TRUE;
}

BOOL WINAPI OpenPort(HANDLE, LPWSTR name, PHANDLE handle) {
    if (!handle || !IsOurPort(name) || !g_port_enabled.load()) {
        SetLastError(ERROR_UNKNOWN_PORT);
        return FALSE;
    }
    auto* context = new (std::nothrow) PortContext();
    if (!context) {
        SetLastError(ERROR_NOT_ENOUGH_MEMORY);
        return FALSE;
    }
    *handle = context;
    return TRUE;
}

BOOL WINAPI StartDocPort(HANDLE port, LPWSTR, DWORD, DWORD level, LPBYTE) {
    auto* context = static_cast<PortContext*>(port);
    if (!context || level != 1 || context->file != INVALID_HANDLE_VALUE) {
        SetLastError(ERROR_INVALID_PARAMETER);
        return FALSE;
    }

    wchar_t temp_path[MAX_PATH + 1]{};
    const DWORD length = GetTempPathW(MAX_PATH, temp_path);
    if (length == 0 || length > MAX_PATH) return FALSE;

    wchar_t temp_file[MAX_PATH + 1]{};
    if (!GetTempFileNameW(temp_path, L"SCP", 0, temp_file)) return FALSE;

    context->spool_path = temp_file;
    context->file = CreateFileW(
        context->spool_path.c_str(), GENERIC_WRITE, 0, nullptr,
        CREATE_ALWAYS, FILE_ATTRIBUTE_TEMPORARY, nullptr);
    if (context->file == INVALID_HANDLE_VALUE) {
        DeleteFileW(context->spool_path.c_str());
        context->spool_path.clear();
        return FALSE;
    }
    return TRUE;
}

BOOL WINAPI WritePort(HANDLE port, LPBYTE buffer, DWORD count, LPDWORD written) {
    auto* context = static_cast<PortContext*>(port);
    if (!context || context->file == INVALID_HANDLE_VALUE || !written || (!buffer && count)) {
        SetLastError(ERROR_INVALID_PARAMETER);
        return FALSE;
    }
    return WriteFile(context->file, buffer, count, written, nullptr);
}

BOOL WINAPI EndDocPort(HANDLE port) {
    auto* context = static_cast<PortContext*>(port);
    if (!context || context->file == INVALID_HANDLE_VALUE) {
        SetLastError(ERROR_INVALID_HANDLE);
        return FALSE;
    }
    if (!FlushFileBuffers(context->file)) return FALSE;
    if (!CloseHandle(context->file)) return FALSE;
    context->file = INVALID_HANDLE_VALUE;

    if (!LaunchGuiForActiveUser(context->spool_path)) {
        // Il file viene conservato in caso di errore, così il lavoro non viene perso.
        return FALSE;
    }
    return TRUE;
}

BOOL WINAPI ClosePort(HANDLE port) {
    auto* context = static_cast<PortContext*>(port);
    if (!context) {
        SetLastError(ERROR_INVALID_HANDLE);
        return FALSE;
    }
    if (context->file != INVALID_HANDLE_VALUE) CloseHandle(context->file);
    delete context;
    return TRUE;
}

BOOL WINAPI XcvOpenPort(LPCWSTR, ACCESS_MASK granted, PHANDLE handle) {
    if (!handle) {
        SetLastError(ERROR_INVALID_PARAMETER);
        return FALSE;
    }
    auto* context = new (std::nothrow) XcvContext{granted};
    if (!context) {
        SetLastError(ERROR_NOT_ENOUGH_MEMORY);
        return FALSE;
    }
    *handle = context;
    return TRUE;
}

DWORD WINAPI XcvDataPort(HANDLE xcv, LPCWSTR command, PBYTE input, DWORD input_size,
                         PBYTE, DWORD, PDWORD output_needed) {
    auto* context = static_cast<XcvContext*>(xcv);
    if (!context || !command) return ERROR_INVALID_PARAMETER;

    const bool add = _wcsicmp(command, L"AddPort") == 0;
    const bool remove = _wcsicmp(command, L"DeletePort") == 0;
    if (add || remove) {
        if ((context->granted_access & SERVER_ACCESS_ADMINISTER) == 0) return ERROR_ACCESS_DENIED;
        if (!input || input_size != sizeof(kPortName) ||
            _wcsicmp(reinterpret_cast<LPCWSTR>(input), kPortName) != 0) {
            return ERROR_INVALID_PARAMETER;
        }
        g_port_enabled.store(add);
        if (output_needed) *output_needed = 0;
        return ERROR_SUCCESS;
    }
    if (_wcsicmp(command, L"MonitorUI") == 0) return ERROR_NOT_SUPPORTED;
    return ERROR_INVALID_FUNCTION;
}

BOOL WINAPI XcvClosePort(HANDLE xcv) {
    auto* context = static_cast<XcvContext*>(xcv);
    if (!context) {
        SetLastError(ERROR_INVALID_HANDLE);
        return FALSE;
    }
    delete context;
    return TRUE;
}

VOID WINAPI Shutdown(HANDLE monitor) {
    delete static_cast<int*>(monitor);
}
} // namespace

extern "C" __declspec(dllexport)
LPMONITOR2 WINAPI InitializePrintMonitor2(PMONITORINIT init, PHANDLE monitor) {
    if (!init || !monitor || init->cbSize < sizeof(MONITORINIT)) {
        SetLastError(ERROR_INVALID_PARAMETER);
        return nullptr;
    }

    auto* instance = new (std::nothrow) int(1);
    if (!instance) {
        SetLastError(ERROR_NOT_ENOUGH_MEMORY);
        return nullptr;
    }

    g_monitor = {};
    g_monitor.cbSize = sizeof(MONITOR2);
    g_monitor.pfnEnumPorts = EnumPorts;
    g_monitor.pfnOpenPort = OpenPort;
    g_monitor.pfnStartDocPort = StartDocPort;
    g_monitor.pfnWritePort = WritePort;
    g_monitor.pfnEndDocPort = EndDocPort;
    g_monitor.pfnClosePort = ClosePort;
    g_monitor.pfnXcvOpenPort = XcvOpenPort;
    g_monitor.pfnXcvDataPort = XcvDataPort;
    g_monitor.pfnXcvClosePort = XcvClosePort;
    g_monitor.pfnShutdown = Shutdown;

    *monitor = instance;
    return &g_monitor;
}

BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID) {
    if (reason == DLL_PROCESS_ATTACH) DisableThreadLibraryCalls(instance);
    return TRUE;
}
