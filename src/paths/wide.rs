use std::path::Path;

use widestring::{U16CStr, U16CString};
use windows_sys::core::PCWSTR;

use super::normalized::NormalizedPath;

/// Convert a raw PCWSTR to a normalized path.
pub fn pcwstr_to_path(pcwstr: PCWSTR) -> NormalizedPath {
    let as_string = unsafe { U16CStr::from_ptr_str(pcwstr) }
        .to_string()
        .unwrap_or_default();

    NormalizedPath::new(as_string)
}

/// Convert a path to a wide string for Windows APIs.
pub fn path_to_widestring(path: &Path) -> U16CString {
    let path_str = path.as_os_str().to_string_lossy();
    U16CString::from_str(&path_str).unwrap_or_else(|_| U16CString::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_to_widestring_roundtrip() {
        let original = Path::new("C:\\Game\\Mods\\test.lua");
        let wide = path_to_widestring(original);
        let back = wide.to_string().unwrap();
        assert_eq!(back, "C:\\Game\\Mods\\test.lua");
    }
}
