use std::env;
use std::fmt::{Debug, Formatter};
use std::path::{Component, Path, PathBuf};
use once_cell::unsync::Lazy;
use widestring::{U16CStr, U16CString};
use windows_sys::core::PCWSTR;
use crate::{BP_MODS, UE4SS_MODS};

/// Quick and dirty debug println macro. Shamelessly stolen.
#[macro_export]
macro_rules! debug_println {
    ($($arg:tt)*) => (if ::std::cfg!(debug_assertions) { ::std::println!($($arg)*); })
}

/// Typed normalized paths. How nice.
pub struct NormalizedPath(pub PathBuf);

impl NormalizedPath {
    pub fn new(weird_path: &Path) -> Self {
        let lower = path_clean::clean(weird_path);

        NormalizedPath(lower)
    }
}

impl Debug for NormalizedPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

/// Convert a raw PCWSTR *const u16 ptr to a normalized PathBuf.
pub fn pcwstr_to_path(pcwstr: PCWSTR) -> NormalizedPath {
    let as_string = unsafe {
        U16CStr::from_ptr_str(pcwstr)
    }.to_string().unwrap();

    let path = PathBuf::from(as_string);
    NormalizedPath::new(&path)
}

pub fn canonicalize_but_no_prefix(path: &Path) -> PathBuf {
    let can = path.canonicalize().unwrap();
    let as_str = can.to_str().unwrap().to_string().replace(r#"\\?\"#, "");

    PathBuf::from(as_str)
}

/// Re-root the origin path onto the virtualized path, if applicable.
/// If the origin path will ONLY BE re-rooted if is a member of the following dirs:
/// - VotV/Binaries/Win64/Mods/
/// - VotV/Content/Paks/LogicMods/
pub fn reroot_path(origin: &NormalizedPath) -> PathBuf {
    let origin = &origin.0;

    let game_root = NormalizedPath::new(&env::current_exe().unwrap().join("../../../../")).0;

    if !origin.starts_with(game_root) {
        return PathBuf::from(origin);
    }

    let ue4ss_mods = {
        let exe_path = env::current_exe().unwrap();
        NormalizedPath::new(&exe_path.join("../Mods")).0
    };

    let bp_mods = {
        let exe_path = env::current_exe().unwrap();
        NormalizedPath::new(&exe_path.join("../../../Content/Paks/LogicMods")).0
    };

    // If the given path is a member of EITHER of these directories, re-root onto the target.
    if origin.starts_with(&ue4ss_mods) {
        let mut stripped = PathBuf::from("Mods").join(origin.strip_prefix(&ue4ss_mods).unwrap());

        NormalizedPath::new(&UE4SS_MODS.get().unwrap().join(stripped)).0
    } else if origin.starts_with(&bp_mods) {
        let mut stripped = PathBuf::from("LogicMods").join(origin.strip_prefix(&bp_mods).unwrap());

        NormalizedPath::new(&BP_MODS.get().unwrap().join(stripped)).0
    } else {
        origin.clone()
    }
}

/// Convert a path ref into a widestring which contains a nul-terminated list of
/// u16 unicode chars.
pub fn path_to_widestring(path: &Path) -> U16CString {
    let path_str = path.as_os_str().to_str().unwrap();
    let wide_string = U16CString::from_str(path_str).unwrap();

    wide_string
}
