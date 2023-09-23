#![allow(unused)]

use std::{env, thread};
use std::ffi::c_void;
use std::fs::{canonicalize, File};
use std::ops::Index;
use std::path::{Path, PathBuf};
use once_cell::sync::OnceCell;
use toml::Value;
use widestring::U16CString;
use windows_sys::w;
use windows_sys::Win32::Foundation::{BOOL, HWND, TRUE};
use windows_sys::Win32::System::Console::AllocConsole;
use windows_sys::Win32::System::LibraryLoader::LoadLibraryW;
use windows_sys::Win32::UI::WindowsAndMessaging::{MESSAGEBOX_STYLE, MessageBoxW};

mod hooks;
mod utils;

static BP_MODS: OnceCell<PathBuf> = OnceCell::new();
static UE4SS_MODS: OnceCell<PathBuf> = OnceCell::new();

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "system" fn DllMain(
    dll_module: u32,
    call_reason: u32,
    reserved: *const c_void
) -> BOOL {
    match call_reason {
        DLL_PROCESS_ATTACH => {
            // Initialize the shim if we haven't yet set the TARGET_DIR static.
            // This ensures that DllMain is not called multiple times with DLL_PROCESS_ATTACH.
            if let None = UE4SS_MODS.get() {
                shim_init();
            }
        },
        // ¯\_(ツ)_/¯
        _ => {}
    }

    TRUE
}

unsafe fn shim_init() {
    #[cfg(debug_assertions)]
    AllocConsole();

    let mut args = env::args().collect::<Vec<_>>();
    let ue4ss_mods = {
        let flag_idx = args.iter().position(|x| x == "--ue4ss-mods")
            .expect("--ue4ss-mods was not set when launching the game, bailing out.");

        utils::canonicalize_but_no_prefix(&PathBuf::from(&args[flag_idx + 1]))
    };
    let bp_mods = {
        let flag_idx = args.iter().position(|x| x == "--bp-mods")
            .expect("--ue4ss-mods was not set when launching the game, bailing out.");

        utils::canonicalize_but_no_prefix(&PathBuf::from(&args[flag_idx + 1]))
    };

    std::panic::set_hook(Box::new(|x| unsafe {
        let message = {
            let message = format!("votv-shimloader has crashed: \n\n{}", x);

            U16CString::from_str(&message)
        }.unwrap();

        MessageBoxW(
            0,
            message.as_ptr(),
            w!("votv-shimloader"),
            0
        );
    }));

    if !Path::new("ue4ss.dll").is_file() {
        panic!("ue4ss.dll could not be found in {:?}", env::current_dir().unwrap());
    }

    BP_MODS.set(bp_mods);
    UE4SS_MODS.set(ue4ss_mods);

    hooks::enable_hooks().unwrap();

    let ue4ss_dll = env::current_exe().unwrap().join("../ue4ss.dll");
    let wide_path = U16CString::from_str(ue4ss_dll.to_str().unwrap()).unwrap();

    LoadLibraryW(wide_path.as_ptr());
}