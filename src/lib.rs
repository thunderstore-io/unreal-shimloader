#![allow(unused, clippy::undocumented_unsafe_blocks)]
#![warn(
    clippy::pedantic,
    clippy::unwrap_used,
)]

use std::{env, thread, fs};
use std::io::Write;
use std::alloc::GlobalAlloc;
use std::collections::HashMap;
use std::ffi::c_void;
use std::fs::{canonicalize, File};
use std::ops::Index;
use std::path::{Path, PathBuf};

use chrono::Local;
use log::{debug, error, LevelFilter};
use getargs::{Arg, Opt, Options};
use once_cell::sync::{Lazy, OnceCell};
use paths::{NormalizedPath, PathRegistry, PATH_REGISTRY};
use widestring::U16CString;
use windows_sys::w;
use windows_sys::Win32::Foundation::{BOOL, HWND, TRUE};
use windows_sys::Win32::System::Console::AllocConsole;
use windows_sys::Win32::System::Diagnostics::Debug::DebugActiveProcess;
use windows_sys::Win32::System::LibraryLoader::LoadLibraryW;
use windows_sys::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessId};
use windows_sys::Win32::UI::WindowsAndMessaging::{MESSAGEBOX_STYLE, MessageBoxW};

mod hooks;
mod paths;
mod utils;

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

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "system" fn DllMain(
    dll_module: u32,
    call_reason: u32,
    reserved: *const c_void
) -> BOOL {
    if call_reason == DLL_PROCESS_ATTACH && PATH_REGISTRY.get().is_none() {
        // Initialize the shim if we haven't yet set the PATH_REGISTRY static.
        // This ensures that DllMain is not called multiple times with DLL_PROCESS_ATTACH.
        shim_init();
    }

    TRUE
}

unsafe fn shim_init() {
    #[cfg(debug_assertions)]
    AllocConsole();

    std::panic::set_hook(Box::new(|x| unsafe {
        let message = format!("unreal-shimloader has crashed: \n\n{x}");
        error!("{message}");

        let message = U16CString::from_str(message);
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
 
    let mut target = Box::new(File::create(exe_dir.join("shimloader-log.txt")).expect("Failed to create log file."));
    env_logger::Builder::new()
        .target(env_logger::Target::Pipe(target))
        .filter(None, LevelFilter::Debug)
        .format(|buf, record| {
            writeln!(
                buf,
                "[{} {} {}:{}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.level(),
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.args()
            )
        })
        .init();

    debug!("unreal_shimloader -- start");
    debug!("current directory: {exe_dir:?}");
    debug!("current executable: {current_exe:?}");
    debug!("args: {:?}", env::args().collect::<Vec<_>>());

    // Ensure that UE4SS is not installed via xinput1_3.dll
    let xinput_path = exe_dir.join("xinput1_3.dll");
    assert!(
        !xinput_path.exists(), 
        "Shimloader is not compatible with the xinput1_3.dll UE4SS binary.\n
        1. Remove the file at {xinput_path:?} \n
        2. Ensure that ue4ss.dll exists within {exe_dir:?} \n
        3. Run the game again.",
    );

    let args = env::args().skip(1).collect::<Vec<_>>();
    let mut opts = Options::new(args.iter().map(String::as_str));

    let mut lua_dir: Option<PathBuf> = None;
    let mut pak_dir: Option<PathBuf> = None;
    let mut cfg_dir: Option<PathBuf> = None;

    while let Some(opt) = opts.next_arg().expect("Failed to parse arguments") {
        match opt {
            Arg::Long("mod-dir") => lua_dir = Some(PathBuf::from(opts.value().expect("`--mod-dir` argument has no value."))),
            Arg::Long("pak-dir") => pak_dir = Some(PathBuf::from(opts.value().expect("`--pak-dir` argument has no value."))),
            Arg::Long("cfg-dir") => cfg_dir = Some(PathBuf::from(opts.value().expect("`--cfg-dir` argument has no value."))),
            _ => (),
        }
    }

    // If no args are specified then we start the game with ue4ss and mods disabled.
    let run_vanilla = ![&lua_dir, &pak_dir, &cfg_dir].iter().any(|x| x.is_some());
    if run_vanilla {
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
    let ue4ss_mods = paths::NormalizedPath::new(&lua_dir.unwrap());
    let bp_mods = paths::NormalizedPath::new(&pak_dir.unwrap());
    let config_dir = paths::NormalizedPath::new(&cfg_dir.unwrap());

    for dir in [ue4ss_mods.as_ref(), bp_mods.as_ref(), config_dir.as_ref()] {
        let _ = fs::create_dir_all(dir);
    }
    
    // Build the path registry with all virtual directory mappings.
    let mut registry = PathRegistry::new();

    // Lua mods: GAME/Binaries/Win64/Mods/ -> user's mod directory
    registry.register(EXE_DIR.join("Mods"), ue4ss_mods.to_path_buf());
    
    // Blueprint mods: GAME/Content/Paks/LogicMods/ -> user's pak directory
    // NormalizedPath automatically cleans .. components
    let bp_source = EXE_DIR
        .join("..")
        .join("..")
        .join("Content")
        .join("Paks")
        .join("LogicMods");
    registry.register(bp_source, bp_mods.to_path_buf());
    
    // Config: GAME/Config/ -> user's config directory
    let config_source = EXE_DIR
        .join("..")
        .join("..")
        .join("Config");
    registry.register(config_source, config_dir.to_path_buf());

    let _ = PATH_REGISTRY.set(registry);

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
