//! Discord process control: detect the desktop client and close it so its
//! cookie store can be read.

const DISCORD_EXES: [&str; 6] = [
    "discord.exe",
    "discordptb.exe",
    "discordcanary.exe",
    "discorddevelopment.exe",
    "vesktop.exe",
    "lightcord.exe",
];

#[cfg(target_os = "windows")]
#[repr(C)]
#[allow(non_snake_case)]
struct ProcessEntry32W {
    dwSize: u32,
    cntUsage: u32,
    th32ProcessID: u32,
    th32DefaultHeapID: usize,
    th32ModuleID: u32,
    cntThreads: u32,
    th32ParentProcessID: u32,
    pcPriClassBase: i32,
    dwFlags: u32,
    szExeFile: [u16; 260],
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> isize;
    fn Process32FirstW(snapshot: isize, entry: *mut ProcessEntry32W) -> i32;
    fn Process32NextW(snapshot: isize, entry: *mut ProcessEntry32W) -> i32;
    fn CloseHandle(handle: isize) -> i32;
    fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> isize;
    fn TerminateProcess(handle: isize, exit_code: u32) -> i32;
}

#[cfg(target_os = "windows")]
pub fn discord_processes() -> Vec<u32> {
    const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
    const INVALID_HANDLE_VALUE: isize = -1;

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }

    let mut entry: ProcessEntry32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<ProcessEntry32W>() as u32;

    let mut pids = Vec::new();
    let mut has_entry = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while has_entry {
        let length = entry
            .szExeFile
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..length]).to_ascii_lowercase();
        if DISCORD_EXES.contains(&name.as_str()) {
            pids.push(entry.th32ProcessID);
        }
        has_entry = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }

    unsafe { CloseHandle(snapshot) };
    pids
}

#[cfg(not(target_os = "windows"))]
pub fn discord_processes() -> Vec<u32> {
    Vec::new()
}

pub fn discord_running() -> bool {
    !discord_processes().is_empty()
}

/// Terminate every Discord client process and wait for the cookie store lock
/// to be released.
#[tauri::command]
pub async fn quit_discord() -> Result<usize, String> {
    #[cfg(target_os = "windows")]
    {
        const PROCESS_TERMINATE: u32 = 0x0001;
        const MAX_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

        let pids = discord_processes();
        if pids.is_empty() {
            return Ok(0);
        }

        let mut closed = 0usize;
        for pid in pids {
            let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
            if handle != 0 && handle != -1 {
                if unsafe { TerminateProcess(handle, 1) } != 0 {
                    closed += 1;
                }
                unsafe { CloseHandle(handle) };
            }
        }
        if closed == 0 {
            return Err("could not close Discord — quit it from the tray and try again".into());
        }

        let deadline = std::time::Instant::now() + MAX_WAIT;
        while discord_running() && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
        if discord_running() {
            return Err("Discord is still running — quit it from the tray and try again".into());
        }
        Ok(closed)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("quitting Discord is only supported on Windows".into())
    }
}
