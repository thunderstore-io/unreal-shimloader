use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::path::{Component, Path, PathBuf};

fn clean_path(path: impl AsRef<Path>) -> PathBuf {
    let mut out: Vec<Component> = Vec::new();

    for comp in path.as_ref().components() {
        match comp {
            Component::CurDir => (),
            Component::ParentDir => match out.last() {
                Some(Component::RootDir) => (),
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                None
                | Some(Component::CurDir)
                | Some(Component::ParentDir)
                | Some(Component::Prefix(_)) => out.push(comp),
            },
            comp => out.push(comp),
        }
    }

    if out.is_empty() {
        PathBuf::from(".")
    } else {
        out.iter().collect()
    }
}

#[derive(Clone)]
pub struct NormalizedPath {
    inner: PathBuf,
    original: PathBuf,
}

impl NormalizedPath {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let cleaned = clean_path(path);
        let lowered = PathBuf::from(cleaned.to_string_lossy().to_lowercase());

        NormalizedPath {
            inner: lowered,
            original: cleaned,
        }
    }

    pub fn inner(&self) -> &Path {
        &self.inner
    }

    pub fn original(&self) -> &Path {
        &self.original
    }

    pub fn to_path_buf(&self) -> PathBuf {
        self.original.clone()
    }

    pub fn components(&self) -> std::path::Components<'_> {
        self.inner.components()
    }

    pub fn component_count(&self) -> usize {
        self.inner.components().count()
    }

    pub fn starts_with(&self, base: &NormalizedPath) -> bool {
        self.inner.starts_with(&base.inner)
    }

    pub fn strip_prefix(&self, prefix: &NormalizedPath) -> Option<PathBuf> {
        self.inner.strip_prefix(&prefix.inner).ok().map(Path::to_path_buf)
    }

    pub fn join(&self, path: impl AsRef<Path>) -> NormalizedPath {
        NormalizedPath::new(self.original.join(path))
    }
}

impl Debug for NormalizedPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.original)
    }
}

impl PartialEq for NormalizedPath {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for NormalizedPath {}

impl Hash for NormalizedPath {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

impl AsRef<Path> for NormalizedPath {
    fn as_ref(&self) -> &Path {
        &self.original
    }
}

impl From<&Path> for NormalizedPath {
    fn from(path: &Path) -> Self {
        NormalizedPath::new(path)
    }
}

impl From<PathBuf> for NormalizedPath {
    fn from(path: PathBuf) -> Self {
        NormalizedPath::new(path)
    }
}

impl From<&str> for NormalizedPath {
    fn from(path: &str) -> Self {
        NormalizedPath::new(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_case_insensitive_equality() {
        let path1 = NormalizedPath::new("C:\\Game\\MODS");
        let path2 = NormalizedPath::new("c:\\game\\mods");
        assert_eq!(path1, path2);
    }

    #[test]
    fn test_cleans_dot_components() {
        let path = NormalizedPath::new("C:\\Game\\..\\Game\\Mods\\.\\test.lua");
        assert_eq!(path.inner().to_string_lossy(), "c:\\game\\mods\\test.lua");
    }

    #[test]
    fn test_starts_with() {
        let parent = NormalizedPath::new("C:\\Game\\Mods");
        let child = NormalizedPath::new("C:\\Game\\Mods\\test.lua");
        let other = NormalizedPath::new("C:\\Other\\Mods");

        assert!(child.starts_with(&parent));
        assert!(!other.starts_with(&parent));
        assert!(!parent.starts_with(&child)); // parent is not child of child
    }

    #[test]
    fn test_strip_prefix() {
        let parent = NormalizedPath::new("C:\\Game\\Mods");
        let child = NormalizedPath::new("C:\\Game\\Mods\\test.lua");

        let remainder = child.strip_prefix(&parent);
        assert_eq!(remainder, Some(PathBuf::from("test.lua")));
    }

    // Edge case tests for Windows path handling

    #[test]
    fn test_forward_slashes_normalized() {
        let path = NormalizedPath::new("C:/Game/Mods/test.lua");
        assert!(path.inner().to_string_lossy().contains("\\"));
    }

    #[test]
    fn test_unc_path_preserved() {
        let path = NormalizedPath::new("\\\\server\\share\\file.txt");
        // UNC paths should be preserved (lowercased but structure intact)
        assert!(path.inner().to_string_lossy().starts_with("\\\\"));
    }

    #[test]
    fn test_extended_path_prefix() {
        // Extended paths \\?\ are passed through (clean_path handles them)
        let path = NormalizedPath::new("\\\\?\\C:\\Game\\Mods\\test.lua");
        // Verify it doesn't crash and produces something usable
        assert!(!path.inner().to_string_lossy().is_empty());
    }

    #[test]
    fn test_device_path_prefix() {
        // Device paths \\.\ 
        let path = NormalizedPath::new("\\\\.\\C:\\Game\\Mods\\test.lua");
        assert!(!path.inner().to_string_lossy().is_empty());
    }

    #[test]
    fn test_trailing_spaces_and_dots() {
        // Windows strips trailing dots/spaces - our normalization should handle this
        let path1 = NormalizedPath::new("C:\\Game\\Mods\\test.txt");
        let path2 = NormalizedPath::new("C:\\Game\\Mods\\test.txt.");
        // clean_path may or may not strip trailing dots - verify no crash
        assert!(!path1.inner().to_string_lossy().is_empty());
        assert!(!path2.inner().to_string_lossy().is_empty());
    }

    #[test]
    fn test_empty_path() {
        let path = NormalizedPath::new("");
        // Should handle empty path without panic
        assert!(path.inner().to_string_lossy().is_empty() || path.inner().to_string_lossy() == ".");
    }

    #[test]
    fn test_root_path() {
        let path = NormalizedPath::new("C:\\");
        assert!(!path.inner().to_string_lossy().is_empty());
    }
}
