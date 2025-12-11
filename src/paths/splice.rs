use std::path::PathBuf;

use super::normalized::NormalizedPath;
use super::registry::PATH_REGISTRY;

/// Re-map a path through the global path registry.
pub fn remap_path(path: &NormalizedPath) -> Option<PathBuf> {
    PATH_REGISTRY.get().and_then(|registry| registry.try_remap(path))
}

/// Splice a path from one root onto another.
/// Returns the remapped path if `path` starts with `source_root`, otherwise None.
pub fn splice_path(
    path: &NormalizedPath,
    source_root: &NormalizedPath,
    target_root: &NormalizedPath,
) -> Option<PathBuf> {
    if !path.starts_with(source_root) {
        return None;
    }

    let relative = path.strip_prefix(source_root)?;
    let result = target_root.to_path_buf().join(relative);

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_splice_path_basic() {
        let path = NormalizedPath::new("C:\\Game\\Mods\\test.lua");
        let source = NormalizedPath::new("C:\\Game\\Mods");
        let target = NormalizedPath::new("D:\\MyMods");

        let result = splice_path(&path, &source, &target);
        assert_eq!(result, Some(PathBuf::from("D:\\MyMods\\test.lua")));
    }

    #[test]
    fn test_splice_path_nested() {
        let path = NormalizedPath::new("C:\\Game\\Mods\\subdir\\another\\test.lua");
        let source = NormalizedPath::new("C:\\Game\\Mods");
        let target = NormalizedPath::new("D:\\MyMods");

        let result = splice_path(&path, &source, &target);
        assert_eq!(result, Some(PathBuf::from("D:\\MyMods\\subdir\\another\\test.lua")));
    }

    #[test]
    fn test_splice_path_not_under_source() {
        let path = NormalizedPath::new("C:\\Other\\test.lua");
        let source = NormalizedPath::new("C:\\Game\\Mods");
        let target = NormalizedPath::new("D:\\MyMods");

        let result = splice_path(&path, &source, &target);
        assert_eq!(result, None);
    }

    #[test]
    fn test_splice_path_parent_does_not_match() {
        // This is the critical bug fix test!
        // A path that is a PARENT of source should NOT match.
        let path = NormalizedPath::new("C:\\Game");
        let source = NormalizedPath::new("C:\\Game\\Mods");
        let target = NormalizedPath::new("D:\\MyMods");

        let result = splice_path(&path, &source, &target);
        assert_eq!(result, None);
    }

    #[test]
    fn test_splice_path_exact_match() {
        // The exact directory itself should match, returning just the target.
        let path = NormalizedPath::new("C:\\Game\\Mods");
        let source = NormalizedPath::new("C:\\Game\\Mods");
        let target = NormalizedPath::new("D:\\MyMods");

        let result = splice_path(&path, &source, &target);
        assert_eq!(result, Some(PathBuf::from("D:\\MyMods")));
    }

    #[test]
    fn test_splice_path_case_insensitive() {
        let path = NormalizedPath::new("C:\\GAME\\MODS\\test.lua");
        let source = NormalizedPath::new("c:\\game\\mods");
        let target = NormalizedPath::new("D:\\MyMods");

        let result = splice_path(&path, &source, &target);
        assert_eq!(result, Some(PathBuf::from("D:\\MyMods\\test.lua")));
    }

    // Edge case: UNC paths should NOT match local paths
    #[test]
    fn test_unc_path_not_remapped() {
        let path = NormalizedPath::new("\\\\server\\share\\Mods\\test.lua");
        let source = NormalizedPath::new("C:\\Game\\Mods");
        let target = NormalizedPath::new("D:\\MyMods");

        let result = splice_path(&path, &source, &target);
        assert_eq!(result, None);
    }

    // Edge case: Different drives should NOT match
    #[test]
    fn test_different_drive_not_remapped() {
        let path = NormalizedPath::new("E:\\Game\\Mods\\test.lua");
        let source = NormalizedPath::new("C:\\Game\\Mods");
        let target = NormalizedPath::new("D:\\MyMods");

        let result = splice_path(&path, &source, &target);
        assert_eq!(result, None);
    }

    // Edge case: Similar prefix but different directory
    #[test]
    fn test_similar_prefix_not_remapped() {
        let path = NormalizedPath::new("C:\\Game\\ModsBackup\\test.lua");
        let source = NormalizedPath::new("C:\\Game\\Mods");
        let target = NormalizedPath::new("D:\\MyMods");

        let result = splice_path(&path, &source, &target);
        assert_eq!(result, None);
    }
}
