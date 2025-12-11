use std::path::PathBuf;
use std::sync::OnceLock;

use log::debug;

use super::normalized::NormalizedPath;
use super::splice::splice_path;

pub static PATH_REGISTRY: OnceLock<PathRegistry> = OnceLock::new();

pub struct PathMapping {
    source: NormalizedPath,
    target: NormalizedPath,
}

impl PathMapping {
    pub fn new(source: impl Into<NormalizedPath>, target: impl Into<NormalizedPath>) -> Self {
        PathMapping {
            source: source.into(),
            target: target.into(),
        }
    }
}

/// Registry of virtual path mappings.
pub struct PathRegistry {
    mappings: Vec<PathMapping>,
}

impl PathRegistry {
    pub fn new() -> Self {
        PathRegistry {
            mappings: Vec::new(),
        }
    }

    pub fn register(&mut self, source: impl Into<NormalizedPath>, target: impl Into<NormalizedPath>) {
        let mapping = PathMapping::new(source, target);
        debug!(
            "[PathRegistry] Registered mapping: {:?} -> {:?}",
            mapping.source, mapping.target
        );
        self.mappings.push(mapping);
    }

    pub fn try_remap(&self, path: &NormalizedPath) -> Option<PathBuf> {
        for mapping in &self.mappings {
            if let Some(remapped) = splice_path(path, &mapping.source, &mapping.target) {
                return Some(remapped);
            }
        }
        None
    }

    pub fn would_remap(&self, path: &NormalizedPath) -> bool {
        self.try_remap(path).is_some()
    }

    pub fn len(&self) -> usize {
        self.mappings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mappings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_basic() {
        let mut registry = PathRegistry::new();
        registry.register("C:\\Game\\Mods", "D:\\MyMods");
        registry.register("C:\\Game\\Content\\Paks\\LogicMods", "D:\\MyPaks");

        let path = NormalizedPath::new("C:\\Game\\Mods\\test.lua");
        let result = registry.try_remap(&path);
        assert_eq!(result, Some(PathBuf::from("D:\\MyMods\\test.lua")));

        let path = NormalizedPath::new("C:\\Game\\Content\\Paks\\LogicMods\\mod.pak");
        let result = registry.try_remap(&path);
        assert_eq!(result, Some(PathBuf::from("D:\\MyPaks\\mod.pak")));
    }

    #[test]
    fn test_registry_no_match() {
        let mut registry = PathRegistry::new();
        registry.register("C:\\Game\\Mods", "D:\\MyMods");

        let path = NormalizedPath::new("C:\\Other\\file.txt");
        let result = registry.try_remap(&path);
        assert_eq!(result, None);
    }

    #[test]
    fn test_registry_first_match_wins() {
        let mut registry = PathRegistry::new();
        // More specific mapping first
        registry.register("C:\\Game\\Mods\\Special", "D:\\SpecialMods");
        registry.register("C:\\Game\\Mods", "D:\\MyMods");

        // Path under Special should use first mapping
        let path = NormalizedPath::new("C:\\Game\\Mods\\Special\\test.lua");
        let result = registry.try_remap(&path);
        assert_eq!(result, Some(PathBuf::from("D:\\SpecialMods\\test.lua")));

        // Path under Mods (not Special) should use second mapping
        // Note: result path is lowercase due to NormalizedPath normalization
        let path = NormalizedPath::new("C:\\Game\\Mods\\Other\\test.lua");
        let result = registry.try_remap(&path);
        assert_eq!(result, Some(PathBuf::from("D:\\MyMods\\other\\test.lua")));
    }
}
