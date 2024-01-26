use crate::{BP_MODS, CONFIG_DIR, EXE_DIR, GAME_ROOT, UE4SS_MODS};
use once_cell::unsync::Lazy;
use std::env;
use std::fmt::{Debug, Formatter};
use std::path::{Component, Path, PathBuf};
use widestring::{U16CStr, U16CString};
use windows_sys::core::PCWSTR;

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

/// Convert a raw PCWSTR *const u16 ptr to a normalized `PathBuf`.
pub fn pcwstr_to_path(pcwstr: PCWSTR) -> NormalizedPath {
    let as_string = unsafe { U16CStr::from_ptr_str(pcwstr) }
        .to_string()
        .unwrap();

    let path = PathBuf::from(as_string);
    NormalizedPath::new(&path)
}

pub fn normalize_path(path: &Path) -> PathBuf {
    path_clean::clean(path)
}

/// Logically splice the orig path relative to some root onto another root.
/// Returns `Option::None` if relative is not a parent of orig.
pub fn logical_splice(base: &Path, relative: &Path, onto: &Path) -> Option<PathBuf> {
    let base_comps = base.components();
    let rela_comps = relative.components(); 

    let mut count = 0;
    let is_invalid = base_comps
        .clone()
        .zip(rela_comps)
        .inspect(|_| count += 1)
        .any(|(b, r)| !b.as_os_str().eq_ignore_ascii_case(r.as_os_str()));

    if is_invalid {
        return None;
    }

    let chopped = base_comps.skip(count);
    let mut onto = onto.to_path_buf();
    onto.extend(chopped);

    Some(onto)
}

/// Re-root the origin path onto the virtualized path, if applicable.
/// If the origin path will ONLY BE re-rooted if is a member of the following dirs:
/// - GAME/Binaries/Win64/Mods/
/// - GAME/Content/Paks/LogicMods/
pub fn reroot_path(origin: &NormalizedPath) -> Option<PathBuf> {
    let origin = &origin.0;
    
    let lua_mods: Lazy<PathBuf> = Lazy::new(|| EXE_DIR.join("Mods").to_path_buf());
    let bp_mods: Lazy<PathBuf> = Lazy::new(|| {
        path_clean::clean(EXE_DIR
            .join("..")
            .join("..")
            .join("Content")
            .join("Paks")
            .join("LogicMods")
            .to_path_buf())
    });
    let config_dir: Lazy<PathBuf> = Lazy::new(|| {
        path_clean::clean(EXE_DIR
            .join("..")
            .join("..")
            .join("Config"))
            .to_path_buf()
    });

    let bindings = vec![
        (lua_mods.as_ref(), UE4SS_MODS.get().unwrap()),
        (bp_mods.as_ref(), BP_MODS.get().unwrap()),
        (config_dir.as_ref(), CONFIG_DIR.get().unwrap()),
    ];

    for (relative_to, onto) in bindings {
        if let Some(output) = logical_splice(&origin, relative_to, onto) {
            return Some(output);
        }
    }

    None
}

/// Convert a path ref into a widestring which contains a nul-terminated list of
/// u16 unicode chars.
pub fn path_to_widestring(path: &Path) -> U16CString {
    let path_str = path.as_os_str().to_str().unwrap();
    U16CString::from_str(path_str).unwrap()
}
