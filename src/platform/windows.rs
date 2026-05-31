// garlemald-client — cross-platform launcher for FINAL FANTASY XIV 1.x private servers
// Copyright (C) 2026  Samuel Stegall
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Windows platform backend. Uses the registry to locate the install and
//! `CreateProcessA` + `WriteProcessMemory` to launch + patch the game
//! directly (no on-disk modification needed).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::launcher::{
    ENCRYPTION_TIME_PATCH_RVA, GameLaunchRequest, LOBBY_HOST_NAME_RVA, encryption_time_patch,
    lobby_host_patch,
};
use crate::platform::Platform;

pub struct WindowsPlatform;

impl WindowsPlatform {
    pub fn new() -> Self {
        Self
    }
}

impl Platform for WindowsPlatform {
    fn detect_game_install(&self) -> Option<PathBuf> {
        detect_from_registry().ok().flatten()
    }

    fn launch_game(&self, request: &GameLaunchRequest) -> Result<()> {
        launch_and_patch(&request.game_dir, &request.lobby_host, &request.session_id)
    }
}

fn detect_from_registry() -> Result<Option<PathBuf>> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE};

    const KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{F2C4E6E0-EB78-4824-A212-6DF6AF0E8E82}";

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = match hklm.open_subkey_with_flags(KEY, KEY_QUERY_VALUE) {
        Ok(k) => k,
        Err(_) => return Ok(None),
    };
    let install_location: String = key.get_value("InstallLocation").unwrap_or_default();
    let display_name: String = key.get_value("DisplayName").unwrap_or_default();
    if install_location.is_empty() || display_name.is_empty() {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(install_location).join(display_name)))
}

fn launch_and_patch(game_dir: &Path, lobby_host: &str, session_id: &str) -> Result<()> {
    use std::ffi::CString;

    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::Debug::{
        CONTEXT, CONTEXT_FULL_X86, GetThreadContext, ReadProcessMemory,
    };
    use windows::Win32::System::Threading::{
        CREATE_SUSPENDED, CreateProcessA, NORMAL_PRIORITY_CLASS, PROCESS_INFORMATION, ResumeThread,
        STARTUPINFOA, TerminateProcess,
    };
    use windows::core::PCSTR;

    let ticks = unsafe { windows::Win32::System::SystemInformation::GetTickCount() };
    let launch_args = crate::crypto::build_launch_arguments(session_id, ticks)?;
    let command_line = format!(
        "{}\\ffxivgame.exe {}",
        game_dir.display(),
        launch_args.encoded_argument
    );
    let command_line_c = CString::new(command_line)?;
    let working_dir_c = CString::new(game_dir.as_os_str().to_string_lossy().into_owned())?;

    let mut startup: STARTUPINFOA = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOA>() as u32;
    let mut proc_info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    let created = unsafe {
        CreateProcessA(
            PCSTR::null(),
            // SAFETY: CreateProcessA takes the command line as a mutable buffer,
            // but our CString is owned and kept alive for the duration of the call.
            windows::core::PSTR(command_line_c.as_ptr() as *mut _),
            None,
            None,
            false,
            CREATE_SUSPENDED | NORMAL_PRIORITY_CLASS,
            None,
            PCSTR(working_dir_c.as_ptr() as *const _),
            &startup,
            &mut proc_info,
        )
    };
    if created.is_err() {
        return Err(anyhow!("CreateProcess failed: {:?}", created.err()));
    }

    let patch_result = (|| -> Result<()> {
        // Read the image base from [Ebx + 8] of the suspended main thread.
        let mut ctx: CONTEXT = unsafe { std::mem::zeroed() };
        ctx.ContextFlags = CONTEXT_FULL_X86;
        unsafe { GetThreadContext(proc_info.hThread, &mut ctx) }
            .context("GetThreadContext failed")?;

        #[cfg(target_arch = "x86")]
        let ebx = ctx.Ebx as usize;
        #[cfg(not(target_arch = "x86"))]
        let ebx: usize = {
            // On x64 hosts WOW64 contexts aren't directly exposed via `CONTEXT`;
            // the 32-bit launcher that originally shipped with FFXIV 1.x was
            // built as x86 only, so garlemald-client on Windows should also
            // target i686-pc-windows-msvc. If we're compiled as x86_64 this is
            // unreachable in practice.
            return Err(anyhow!(
                "garlemald-client on Windows must be built as i686 to patch ffxivgame.exe"
            ));
        };

        let mut image_base: u32 = 0;
        let mut read_bytes: usize = 0;
        unsafe {
            ReadProcessMemory(
                proc_info.hProcess,
                (ebx + 8) as *const _,
                &mut image_base as *mut _ as *mut _,
                4,
                Some(&mut read_bytes),
            )
        }
        .context("ReadProcessMemory(image_base) failed")?;

        let enc_patch = encryption_time_patch();
        let host_patch = lobby_host_patch(lobby_host)?;
        write_remote(
            proc_info.hProcess,
            image_base as usize + ENCRYPTION_TIME_PATCH_RVA as usize,
            &enc_patch.bytes,
        )?;
        write_remote(
            proc_info.hProcess,
            image_base as usize + LOBBY_HOST_NAME_RVA as usize,
            &host_patch.bytes,
        )?;
        Ok(())
    })();

    if let Err(e) = patch_result {
        unsafe {
            let _ = TerminateProcess(proc_info.hProcess, 1);
        }
        return Err(e);
    }

    if let Err(e) = cap_cpu_affinity(proc_info.hProcess) {
        unsafe {
            let _ = TerminateProcess(proc_info.hProcess, 1);
        }
        return Err(e);
    }

    unsafe {
        ResumeThread(proc_info.hThread);
        let _ = CloseHandle(proc_info.hProcess);
        let _ = CloseHandle(proc_info.hThread);
    }
    Ok(())
}

/// FFXIV 1.x crashes immediately at launch on hosts where 16 or more
/// logical CPUs are visible to the process. Restrict the child to at most
/// 15 of the CPUs it was already allowed to run on.
fn cap_cpu_affinity(process: windows::Win32::Foundation::HANDLE) -> Result<()> {
    use windows::Win32::System::Threading::{GetProcessAffinityMask, SetProcessAffinityMask};

    const MAX_CPUS: u32 = 15;

    let mut process_mask: usize = 0;
    let mut system_mask: usize = 0;
    unsafe { GetProcessAffinityMask(process, &mut process_mask, &mut system_mask) }
        .context("GetProcessAffinityMask failed")?;

    if (process_mask as u64).count_ones() <= MAX_CPUS {
        return Ok(());
    }

    let mut new_mask: usize = 0;
    let mut taken: u32 = 0;
    for bit in 0..(std::mem::size_of::<usize>() * 8) {
        let b: usize = 1 << bit;
        if process_mask & b != 0 {
            new_mask |= b;
            taken += 1;
            if taken == MAX_CPUS {
                break;
            }
        }
    }

    unsafe { SetProcessAffinityMask(process, new_mask) }
        .context("SetProcessAffinityMask failed")?;
    Ok(())
}

fn write_remote(
    process: windows::Win32::Foundation::HANDLE,
    remote_addr: usize,
    bytes: &[u8],
) -> Result<()> {
    use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;
    use windows::Win32::System::Memory::{PAGE_PROTECTION_FLAGS, PAGE_READWRITE, VirtualProtectEx};

    let mut old_protect = PAGE_PROTECTION_FLAGS(0);
    unsafe {
        VirtualProtectEx(
            process,
            remote_addr as *mut _,
            bytes.len(),
            PAGE_READWRITE,
            &mut old_protect,
        )
    }
    .context("VirtualProtectEx RW failed")?;

    let mut written = 0usize;
    unsafe {
        WriteProcessMemory(
            process,
            remote_addr as *const _,
            bytes.as_ptr() as *const _,
            bytes.len(),
            Some(&mut written),
        )
    }
    .context("WriteProcessMemory failed")?;

    if written != bytes.len() {
        return Err(anyhow!(
            "short write: wrote {} of {} bytes",
            written,
            bytes.len()
        ));
    }

    let mut _restore = PAGE_PROTECTION_FLAGS(0);
    unsafe {
        VirtualProtectEx(
            process,
            remote_addr as *mut _,
            bytes.len(),
            old_protect,
            &mut _restore,
        )
    }
    .context("VirtualProtectEx restore failed")?;
    Ok(())
}
