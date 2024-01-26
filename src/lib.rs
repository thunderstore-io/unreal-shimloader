#![allow(unused, clippy::undocumented_unsafe_blocks)]
#![warn(
    clippy::pedantic,
    clippy::unwrap_used,
)]

use std::{env, thread, fs};
use std::ffi::c_void;
use std::fs::{canonicalize, File};
use std::ops::Index;
use std::path::{Path, PathBuf};
use clap::Parser;
use once_cell::sync::{Lazy, OnceCell};
use toml::Value;
use utils::NormalizedPath;
use widestring::U16CString;
use windows_sys::w;
use windows_sys::Win32::Foundation::{BOOL, HWND, TRUE};
use windows_sys::Win32::System::Console::AllocConsole;
use windows_sys::Win32::System::LibraryLoader::LoadLibraryW;
use windows_sys::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows_sys::Win32::UI::WindowsAndMessaging::{MESSAGEBOX_STYLE, MessageBoxW};

mod hooks;
mod utils;

static BP_MODS: OnceCell<PathBuf> = OnceCell::new();
static UE4SS_MODS: OnceCell<PathBuf> = OnceCell::new();
static CONFIG_DIR: OnceCell<PathBuf> = OnceCell::new();

static GAME_ROOT: Lazy<PathBuf> = Lazy::new(|| {
    let current_exe = env::current_exe().unwrap();
    current_exe
        .ancestors()
        .nth(3)
        .unwrap_or_else(|| 
            panic!("The executable at {current_exe:?} is not contained within a valid UE directory structure."))
        .to_path_buf()
});

static EXE_DIR: Lazy<PathBuf> = Lazy::new(|| {
    env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
});

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(hide = true)]
    other: Vec<String>,

    #[arg(long)]
    ue4ss_mods: Option<PathBuf>,

    #[arg(long)]
    bp_mods: Option<PathBuf>,

    #[arg(long)]
    config_dir: Option<PathBuf>,

    #[arg(long, default_value = "false")]
    disable_mods: bool,
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "system" fn DllMain(
    dll_module: u32,
    call_reason: u32,
    reserved: *const c_void
) -> BOOL {
    if call_reason == DLL_PROCESS_ATTACH && UE4SS_MODS.get().is_none() {
        // Initialize the shim if we haven't yet set the TARGET_DIR static.
        // This ensures that DllMain is not called multiple times with DLL_PROCESS_ATTACH.
        shim_init();
    }

    TRUE
}

unsafe fn shim_init() {
    #[cfg(debug_assertions)]
    AllocConsole();

    std::panic::set_hook(Box::new(|x| unsafe {
        let message = U16CString::from_str(format!("unreal-shimloader has crashed: \n\n{}", x));

        MessageBoxW(
            0,
            message.unwrap().as_ptr(),
            w!("unreal-shimloader"),
            0
        );
    }));

    let current_exe = env::current_exe()
        .expect("Failed to get the path of the currently running executable.");
    let exe_dir = current_exe.parent().unwrap();

    // Ensure that UE4SS is not installed via xinput1_3.dll
    let xinput_path = exe_dir.join("xinput1_3.dll");
    if xinput_path.exists() {
        panic!(
            "Shimloader is not compatible with the xinput1_3.dll UE4SS binary.\n
            1. Remove the file at {:?} \n
            2. Ensure that ue4ss.dll exists within {:?} \n
            3. Run the game again.",
            xinput_path, exe_dir
        );
    }

    let args = Args::parse();

    // If no args are specified then the user is NOT running virtualized. Load the game
    // and ue4ss as usual.
    let argc = env::args().collect::<Vec<_>>().len();
    if argc - args.other.len() == 1 {
        load_ue4ss(&current_exe);
        return;
    }

    if args.disable_mods {
        return;
    }

    let toplevel_dir = current_exe
        .ancestors()
        .nth(3)
        .unwrap_or_else(|| 
            panic!("The executable at {current_exe:?} is not contained within a valid UE directory structure."));

    // Validation to ensure that the Content/Paks/LogicMods directory exists in the game directory.
    // This is really janky to do in DllMain. Oh well!
    let logicmods_dir = toplevel_dir
        .join("Content")
        .join("Paks")
        .join("LogicMods");

    if !logicmods_dir.is_dir() {
        fs::create_dir_all(&logicmods_dir);
    }

    // Create the Config directory in the game, if it doesn't already exist.
    let real_config_dir = toplevel_dir.join("Config");

    if !real_config_dir.is_dir() {
        fs::create_dir_all(real_config_dir);
    }

    // Create the ue4ss_mods and bp_mods directories if they don't already exist.
    let ue4ss_mods = utils::normalize_path(&args.ue4ss_mods.unwrap());
    let bp_mods = utils::normalize_path(&args.bp_mods.unwrap());
    let config_dir = utils::normalize_path(&args.config_dir.unwrap());

    for dir in [&ue4ss_mods, &bp_mods, &config_dir] {
        fs::create_dir_all(&dir);
    }
    
    BP_MODS.set(bp_mods);
    UE4SS_MODS.set(ue4ss_mods);
    CONFIG_DIR.set(config_dir);

    if let Err(e) = hooks::enable_hooks() {
        panic!("Failed to enable one or more hooks. {e}")
    }

    load_ue4ss(&current_exe);
}

unsafe fn load_ue4ss(current_exe: &Path) {
    let ue4ss_dll = current_exe.join("../ue4ss.dll");
    assert!(ue4ss_dll.is_file(), "ue4ss.dll could not be found at {ue4ss_dll:?}");

    let wide_path = U16CString::from_str(ue4ss_dll.to_str().unwrap()).unwrap();
    LoadLibraryW(wide_path.as_ptr());
}
