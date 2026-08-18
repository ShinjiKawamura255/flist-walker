use crate::fs_atomic::{acquire_sidecar_lock, write_text_atomic};
use crate::path_utils::normalize_windows_path_buf;
use crate::runtime_config::settings_base_dir;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const SEARCH_CATALOG_FILE_NAME: &str = ".flistwalker_search_catalog.json";
const SEARCH_CATALOG_VERSION: u32 = 1;
const CATALOG_LOCK_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PresetEntryType {
    #[default]
    All,
    File,
    Folder,
}

impl PresetEntryType {
    pub fn include_flags(self) -> (bool, bool) {
        match self {
            Self::All => (true, true),
            Self::File => (true, false),
            Self::Folder => (false, true),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PresetSource {
    #[default]
    Auto,
    Filelist,
    Walker,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PresetSortMode {
    #[default]
    Score,
    NameAsc,
    NameDesc,
    ModifiedDesc,
    ModifiedAsc,
    CreatedDesc,
    CreatedAsc,
    SizeDesc,
    SizeAsc,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NamedRoot {
    pub name: String,
    pub path: PathBuf,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchPreset {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_name: Option<String>,
    pub root_path: PathBuf,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub entry_type: PresetEntryType,
    #[serde(default)]
    pub source: PresetSource,
    #[serde(default)]
    pub regex: bool,
    #[serde(default = "default_true")]
    pub ignore_case: bool,
    #[serde(default = "default_true")]
    pub ignore_enabled: bool,
    #[serde(default)]
    pub sort: PresetSortMode,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchCatalog {
    #[serde(default = "catalog_version")]
    pub version: u32,
    #[serde(default)]
    pub named_roots: Vec<NamedRoot>,
    #[serde(default)]
    pub presets: Vec<SearchPreset>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for SearchCatalog {
    fn default() -> Self {
        Self {
            version: SEARCH_CATALOG_VERSION,
            named_roots: Vec::new(),
            presets: Vec::new(),
            extra: BTreeMap::new(),
        }
    }
}

impl SearchCatalog {
    pub fn named_root(&self, name: &str) -> Option<&NamedRoot> {
        self.named_roots
            .iter()
            .find(|root| names_equal(&root.name, name))
    }

    pub fn preset(&self, name: &str) -> Option<&SearchPreset> {
        self.presets
            .iter()
            .find(|preset| names_equal(&preset.name, name))
    }

    pub fn add_named_root(&mut self, name: &str, path: PathBuf) -> Result<()> {
        let name = validate_catalog_name(name)?;
        if self.named_root(&name).is_some() {
            return Err(anyhow!("named root already exists: {name}"));
        }
        self.named_roots.push(NamedRoot {
            name,
            path: normalize_windows_path_buf(path),
            extra: BTreeMap::new(),
        });
        Ok(())
    }

    pub fn replace_named_root(
        &mut self,
        original_name: &str,
        name: &str,
        path: PathBuf,
    ) -> Result<()> {
        let Some(original_index) = self
            .named_roots
            .iter()
            .position(|root| names_equal(&root.name, original_name))
        else {
            return Err(anyhow!("named root is not configured: {original_name}"));
        };
        let name = validate_catalog_name(name)?;
        if self
            .named_roots
            .iter()
            .enumerate()
            .any(|(index, root)| index != original_index && names_equal(&root.name, &name))
        {
            return Err(anyhow!("named root already exists: {name}"));
        }
        let previous_name = self.named_roots[original_index].name.clone();
        self.named_roots[original_index].name = name.clone();
        self.named_roots[original_index].path = normalize_windows_path_buf(path);
        for preset in &mut self.presets {
            if preset
                .root_name
                .as_deref()
                .is_some_and(|candidate| names_equal(candidate, &previous_name))
            {
                preset.root_name = Some(name.clone());
            }
        }
        Ok(())
    }

    pub fn remove_named_root(&mut self, name: &str) -> Result<()> {
        let Some(index) = self
            .named_roots
            .iter()
            .position(|root| names_equal(&root.name, name))
        else {
            return Err(anyhow!("named root is not configured: {name}"));
        };
        let removed_name = self.named_roots.remove(index).name;
        for preset in &mut self.presets {
            if preset
                .root_name
                .as_deref()
                .is_some_and(|candidate| names_equal(candidate, &removed_name))
            {
                preset.root_name = None;
            }
        }
        Ok(())
    }

    pub fn save_preset(&mut self, mut preset: SearchPreset) -> Result<()> {
        preset.name = validate_catalog_name(&preset.name)?;
        preset.root_path = normalize_windows_path_buf(preset.root_path);
        if let Some(root_name) = preset.root_name.as_deref() {
            preset.root_name = Some(validate_catalog_name(root_name)?);
        }
        if let Some(existing) = self
            .presets
            .iter_mut()
            .find(|candidate| names_equal(&candidate.name, &preset.name))
        {
            *existing = preset;
        } else {
            self.presets.push(preset);
        }
        Ok(())
    }

    pub fn add_preset(&mut self, mut preset: SearchPreset) -> Result<()> {
        preset.name = validate_catalog_name(&preset.name)?;
        preset.root_path = normalize_windows_path_buf(preset.root_path);
        if let Some(root_name) = preset.root_name.as_deref() {
            preset.root_name = Some(validate_catalog_name(root_name)?);
        }
        if self
            .presets
            .iter()
            .any(|candidate| names_equal(&candidate.name, &preset.name))
        {
            return Err(anyhow!("search preset already exists: {}", preset.name));
        }
        self.presets.push(preset);
        Ok(())
    }

    pub fn replace_preset(&mut self, original_name: &str, mut preset: SearchPreset) -> Result<()> {
        let Some(original_index) = self
            .presets
            .iter()
            .position(|candidate| names_equal(&candidate.name, original_name))
        else {
            return Err(anyhow!("search preset is not configured: {original_name}"));
        };
        preset.name = validate_catalog_name(&preset.name)?;
        preset.root_path = normalize_windows_path_buf(preset.root_path);
        if let Some(root_name) = preset.root_name.as_deref() {
            preset.root_name = Some(validate_catalog_name(root_name)?);
        }
        if self.presets.iter().enumerate().any(|(index, candidate)| {
            index != original_index && names_equal(&candidate.name, &preset.name)
        }) {
            return Err(anyhow!("search preset already exists: {}", preset.name));
        }
        preset.extra = self.presets[original_index].extra.clone();
        self.presets[original_index] = preset;
        Ok(())
    }

    pub fn remove_preset(&mut self, name: &str) -> Result<()> {
        let Some(index) = self
            .presets
            .iter()
            .position(|preset| names_equal(&preset.name, name))
        else {
            return Err(anyhow!("search preset is not configured: {name}"));
        };
        self.presets.remove(index);
        Ok(())
    }

    pub fn resolve_preset_root(&self, preset: &SearchPreset) -> PathBuf {
        preset
            .root_name
            .as_deref()
            .and_then(|name| self.named_root(name))
            .map(|root| root.path.clone())
            .unwrap_or_else(|| preset.root_path.clone())
    }
}

fn default_true() -> bool {
    true
}

fn catalog_version() -> u32 {
    SEARCH_CATALOG_VERSION
}

fn names_equal(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

pub fn validate_catalog_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(anyhow!("catalog name must not be empty"));
    }
    if name.contains('=') {
        return Err(anyhow!("catalog name must not contain '='"));
    }
    if name.chars().any(char::is_control) {
        return Err(anyhow!("catalog name must not contain control characters"));
    }
    Ok(name.to_string())
}

pub fn search_catalog_file_path() -> Result<PathBuf> {
    settings_base_dir()
        .map(|base| base.join(SEARCH_CATALOG_FILE_NAME))
        .ok_or_else(|| anyhow!("settings directory is unavailable"))
}

pub fn load_search_catalog() -> Result<SearchCatalog> {
    load_search_catalog_from_path(&search_catalog_file_path()?)
}

pub fn load_search_catalog_from_path(path: &Path) -> Result<SearchCatalog> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SearchCatalog::default());
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to read search catalog: {}", path.display()));
        }
    };
    let catalog: SearchCatalog = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse search catalog: {}", path.display()))?;
    validate_catalog(&catalog)?;
    Ok(catalog)
}

pub fn update_search_catalog<F>(path: &Path, mutate: F) -> Result<SearchCatalog>
where
    F: FnOnce(&mut SearchCatalog) -> Result<()>,
{
    let _lock = acquire_sidecar_lock(path, CATALOG_LOCK_TIMEOUT)
        .with_context(|| format!("failed to lock search catalog: {}", path.display()))?;
    let mut catalog = load_search_catalog_from_path(path)?;
    mutate(&mut catalog)?;
    validate_catalog(&catalog)?;
    let text =
        serde_json::to_string_pretty(&catalog).context("failed to serialize search catalog")?;
    write_text_atomic(path, &text)
        .with_context(|| format!("failed to write search catalog: {}", path.display()))?;
    Ok(catalog)
}

fn validate_catalog(catalog: &SearchCatalog) -> Result<()> {
    if catalog.version != SEARCH_CATALOG_VERSION {
        return Err(anyhow!(
            "unsupported search catalog version {} (expected {})",
            catalog.version,
            SEARCH_CATALOG_VERSION
        ));
    }
    let mut seen_roots = Vec::<String>::new();
    for root in &catalog.named_roots {
        validate_catalog_name(&root.name)?;
        let key = root.name.to_ascii_lowercase();
        if seen_roots.contains(&key) {
            return Err(anyhow!("duplicate named root: {}", root.name));
        }
        seen_roots.push(key);
    }
    let mut seen_presets = Vec::<String>::new();
    for preset in &catalog.presets {
        validate_catalog_name(&preset.name)?;
        let key = preset.name.to_ascii_lowercase();
        if seen_presets.contains(&key) {
            return Err(anyhow!("duplicate search preset: {}", preset.name));
        }
        seen_presets.push(key);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("flistwalker-search-catalog-{name}-{nonce}"))
    }

    fn preset(name: &str, root: &Path) -> SearchPreset {
        SearchPreset {
            name: name.to_string(),
            root_name: None,
            root_path: root.to_path_buf(),
            query: "ext:rs".to_string(),
            entry_type: PresetEntryType::File,
            source: PresetSource::Auto,
            regex: false,
            ignore_case: true,
            ignore_enabled: true,
            sort: PresetSortMode::Score,
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn catalog_names_are_trimmed_and_reject_empty_control_and_separator() {
        assert_eq!(validate_catalog_name("  work  ").unwrap(), "work");
        assert!(validate_catalog_name("   ").is_err());
        assert!(validate_catalog_name("bad=name").is_err());
        assert!(validate_catalog_name("bad\nname").is_err());
    }

    #[test]
    fn catalog_update_round_trips_unknown_fields_and_keeps_legacy_roots_untouched() {
        let root = test_root("roundtrip");
        fs::create_dir_all(&root).expect("create root");
        let catalog_path = root.join(SEARCH_CATALOG_FILE_NAME);
        let legacy_path = root.join(".flistwalker_roots.txt");
        fs::write(&legacy_path, "legacy-root\n").expect("write legacy roots");
        fs::write(
            &catalog_path,
            r#"{"version":1,"named_roots":[],"presets":[],"future":{"kept":true}}"#,
        )
        .expect("write catalog");

        update_search_catalog(&catalog_path, |catalog| {
            catalog.add_named_root("work", root.join("repo"))?;
            catalog.save_preset(preset("rust", &root))
        })
        .expect("update catalog");

        let loaded = load_search_catalog_from_path(&catalog_path).expect("load catalog");
        assert!(loaded.named_root("WORK").is_some());
        assert!(loaded.preset("rust").is_some());
        assert_eq!(loaded.extra["future"]["kept"], Value::Bool(true));
        assert_eq!(fs::read_to_string(&legacy_path).unwrap(), "legacy-root\n");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn newer_or_malformed_catalog_is_not_overwritten() {
        let root = test_root("readonly-failure");
        fs::create_dir_all(&root).expect("create root");
        let path = root.join(SEARCH_CATALOG_FILE_NAME);
        for text in [r#"{"version":2}"#, "not json"] {
            fs::write(&path, text).expect("write invalid catalog");
            assert!(update_search_catalog(&path, |_| Ok(())).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn removing_named_root_preserves_preset_snapshot() {
        let root = test_root("snapshot");
        let mut catalog = SearchCatalog::default();
        catalog
            .add_named_root("repo", root.join("current"))
            .expect("add root");
        let mut saved = preset("search", &root.join("snapshot"));
        saved.root_name = Some("repo".to_string());
        catalog.save_preset(saved).expect("save preset");
        assert_eq!(
            catalog.resolve_preset_root(catalog.preset("search").unwrap()),
            root.join("current")
        );
        catalog.remove_named_root("repo").expect("remove root");
        assert_eq!(
            catalog.resolve_preset_root(catalog.preset("search").unwrap()),
            root.join("snapshot")
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn catalog_mutations_strip_windows_verbatim_prefixes_from_stored_paths() {
        let extended_path = PathBuf::from(r"\\?\C:\Users\tester\Documents");
        let expected_path = PathBuf::from(r"C:\Users\tester\Documents");
        let mut catalog = SearchCatalog::default();

        catalog
            .add_named_root("docs", extended_path.clone())
            .expect("add named root");
        assert_eq!(catalog.named_root("docs").unwrap().path, expected_path);

        catalog
            .replace_named_root("docs", "documents", extended_path.clone())
            .expect("replace named root");
        assert_eq!(catalog.named_root("documents").unwrap().path, expected_path);

        catalog
            .save_preset(preset("saved", &extended_path))
            .expect("save preset");
        assert_eq!(catalog.preset("saved").unwrap().root_path, expected_path);

        catalog
            .replace_preset("saved", preset("edited", &extended_path))
            .expect("replace preset");
        assert_eq!(catalog.preset("edited").unwrap().root_path, expected_path);
    }

    #[test]
    fn replacing_preset_supports_rename_in_place_and_rejects_name_collisions() {
        let root = test_root("replace-preset");
        let mut catalog = SearchCatalog::default();
        let mut rust = preset("Rust", &root);
        rust.extra
            .insert("future".to_string(), Value::String("keep".to_string()));
        catalog.save_preset(rust).expect("save rust preset");
        catalog
            .save_preset(preset("Docs", &root))
            .expect("save docs preset");

        let mut edited = catalog.preset("Rust").cloned().expect("rust preset");
        edited.name = "Rust source".to_string();
        edited.query = "ext:rs dir:src".to_string();
        catalog
            .replace_preset("Rust", edited)
            .expect("rename preset");

        assert_eq!(
            catalog
                .presets
                .iter()
                .map(|preset| preset.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Rust source", "Docs"]
        );
        assert_eq!(
            catalog.preset("Rust source").expect("renamed preset").query,
            "ext:rs dir:src"
        );
        assert_eq!(
            catalog.preset("Rust source").expect("renamed preset").extra["future"],
            Value::String("keep".to_string())
        );

        let before_collision = catalog.clone();
        let mut collision = catalog
            .preset("Rust source")
            .cloned()
            .expect("renamed preset");
        collision.name = "docs".to_string();
        assert!(catalog.replace_preset("Rust source", collision).is_err());
        assert_eq!(catalog, before_collision);
    }

    #[test]
    fn adding_preset_rejects_name_collisions_without_overwriting_existing_entries() {
        let root = test_root("add-preset");
        let mut catalog = SearchCatalog::default();
        catalog
            .add_preset(preset("Rust", &root))
            .expect("add rust preset");
        let before_collision = catalog.clone();

        assert!(catalog.add_preset(preset("rust", &root)).is_err());
        assert_eq!(catalog, before_collision);
    }

    #[test]
    fn atomic_preset_replace_preserves_latest_unknown_fields() {
        let root = test_root("atomic-replace-preset");
        fs::create_dir_all(&root).expect("create root");
        let catalog_path = root.join(SEARCH_CATALOG_FILE_NAME);
        fs::write(
            &catalog_path,
            format!(
                r#"{{"version":1,"named_roots":[],"presets":[{{"name":"Rust","root_path":{},"query":"old","future_preset":{{"keep":true}}}}],"future_catalog":{{"keep":true}}}}"#,
                serde_json::to_string(&root).expect("serialize root")
            ),
        )
        .expect("write catalog");
        let edited = preset("Rust source", &root);

        update_search_catalog(&catalog_path, |catalog| {
            catalog.replace_preset("Rust", edited)
        })
        .expect("replace preset atomically");

        let loaded = load_search_catalog_from_path(&catalog_path).expect("load updated catalog");
        assert!(loaded.preset("Rust").is_none());
        assert_eq!(
            loaded.preset("Rust source").expect("renamed preset").extra["future_preset"]["keep"],
            Value::Bool(true)
        );
        assert_eq!(loaded.extra["future_catalog"]["keep"], Value::Bool(true));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn replacing_named_root_renames_preset_references_and_preserves_unknown_fields() {
        let root = test_root("replace-named-root");
        let mut catalog = SearchCatalog::default();
        catalog
            .add_named_root("work", root.join("old"))
            .expect("add work root");
        catalog.named_roots[0]
            .extra
            .insert("future".to_string(), Value::String("keep".to_string()));
        catalog
            .add_named_root("docs", root.join("docs"))
            .expect("add docs root");
        let mut linked = preset("Rust", &root.join("snapshot"));
        linked.root_name = Some("work".to_string());
        catalog.save_preset(linked).expect("save linked preset");

        catalog
            .replace_named_root("work", "workspace", root.join("new"))
            .expect("replace named root");

        assert_eq!(
            catalog
                .named_roots
                .iter()
                .map(|root| root.name.as_str())
                .collect::<Vec<_>>(),
            vec!["workspace", "docs"]
        );
        assert_eq!(
            catalog.named_root("workspace").expect("renamed root").path,
            root.join("new")
        );
        assert_eq!(
            catalog.named_root("workspace").expect("renamed root").extra["future"],
            Value::String("keep".to_string())
        );
        assert_eq!(
            catalog
                .preset("Rust")
                .expect("linked preset")
                .root_name
                .as_deref(),
            Some("workspace")
        );

        let before_collision = catalog.clone();
        assert!(catalog
            .replace_named_root("workspace", "DOCS", root.join("collision"))
            .is_err());
        assert_eq!(catalog, before_collision);
    }

    #[test]
    fn atomic_named_root_replace_preserves_latest_unknown_fields() {
        let root = test_root("atomic-replace-named-root");
        fs::create_dir_all(&root).expect("create root");
        let catalog_path = root.join(SEARCH_CATALOG_FILE_NAME);
        fs::write(
            &catalog_path,
            format!(
                r#"{{"version":1,"named_roots":[{{"name":"work","path":{},"future_root":{{"keep":true}}}}],"presets":[{{"name":"Rust","root_name":"work","root_path":{},"query":"","future_preset":{{"keep":true}}}}],"future_catalog":{{"keep":true}}}}"#,
                serde_json::to_string(&root.join("old")).expect("serialize old root"),
                serde_json::to_string(&root.join("snapshot")).expect("serialize snapshot")
            ),
        )
        .expect("write catalog");

        update_search_catalog(&catalog_path, |catalog| {
            catalog.replace_named_root("work", "workspace", root.join("new"))
        })
        .expect("replace named root atomically");

        let loaded = load_search_catalog_from_path(&catalog_path).expect("load updated catalog");
        assert_eq!(
            loaded.named_root("workspace").expect("renamed root").extra["future_root"]["keep"],
            Value::Bool(true)
        );
        assert_eq!(
            loaded.preset("Rust").expect("preset").root_name.as_deref(),
            Some("workspace")
        );
        assert_eq!(
            loaded.preset("Rust").expect("preset").extra["future_preset"]["keep"],
            Value::Bool(true)
        );
        assert_eq!(loaded.extra["future_catalog"]["keep"], Value::Bool(true));
        let _ = fs::remove_dir_all(root);
    }
}
