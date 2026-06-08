use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const MODULE_MANIFEST_FILE: &str = "trispr-module.json";
const SUPPORTED_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ModulePackageManifest {
    pub schema_version: u16,
    pub id: String,
    pub name: String,
    pub version: String,
    pub host_capability: String,
    pub required_assets: Vec<ModulePackageAsset>,
    pub sub_capabilities: Vec<ModulePackageSubCapability>,
    pub executable_hooks: Vec<ModulePackageExecutableHook>,
}

impl Default for ModulePackageManifest {
    fn default() -> Self {
        Self {
            schema_version: SUPPORTED_SCHEMA_VERSION,
            id: String::new(),
            name: String::new(),
            version: String::new(),
            host_capability: String::new(),
            required_assets: Vec::new(),
            sub_capabilities: Vec::new(),
            executable_hooks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModulePackageAsset {
    pub id: String,
    pub path: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModulePackageSubCapability {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub requires_setup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModulePackageExecutableHook {
    pub id: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ValidatedModulePackage {
    pub package_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest: ModulePackageManifest,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ModulePackageScanError {
    pub package_dir: PathBuf,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ModulePackageScanReport {
    pub packages: Vec<ValidatedModulePackage>,
    pub errors: Vec<ModulePackageScanError>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ModulePackageInstallResult {
    pub module_id: String,
    pub source_dir: PathBuf,
    pub target_dir: PathBuf,
    pub installed: bool,
    pub package: ValidatedModulePackage,
}

pub fn scan_modules_dir(modules_dir: &Path) -> Result<ModulePackageScanReport, String> {
    if !modules_dir.exists() {
        return Ok(ModulePackageScanReport {
            packages: Vec::new(),
            errors: Vec::new(),
        });
    }
    if !modules_dir.is_dir() {
        return Err(format!(
            "Module directory '{}' is not a directory.",
            modules_dir.display()
        ));
    }

    let mut packages = Vec::new();
    let mut errors = Vec::new();
    let entries = fs::read_dir(modules_dir).map_err(|error| {
        format!(
            "Failed to read module directory '{}': {}",
            modules_dir.display(),
            error
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "Failed to read entry in module directory '{}': {}",
                modules_dir.display(),
                error
            )
        })?;
        let package_dir = entry.path();
        if !package_dir.is_dir() {
            continue;
        }

        match scan_package_dir(&package_dir) {
            Ok(package) => packages.push(package),
            Err(error) => errors.push(ModulePackageScanError { package_dir, error }),
        }
    }

    packages.sort_by(|left, right| left.manifest.id.cmp(&right.manifest.id));
    errors.sort_by(|left, right| left.package_dir.cmp(&right.package_dir));
    Ok(ModulePackageScanReport { packages, errors })
}

pub fn installed_module_ids(report: &ModulePackageScanReport) -> HashSet<String> {
    report
        .packages
        .iter()
        .map(|package| package.manifest.id.clone())
        .collect()
}

pub fn scan_installed_module_ids(modules_dir: &Path) -> Result<HashSet<String>, String> {
    scan_modules_dir(modules_dir).map(|report| installed_module_ids(&report))
}

pub fn install_package_from_dir(
    source_dir: &Path,
    modules_dir: &Path,
) -> Result<ModulePackageInstallResult, String> {
    let source_package = scan_package_dir(source_dir)?;
    let module_id = source_package.manifest.id.clone();
    let target_dir = modules_dir.join(&module_id);

    if target_dir.exists() {
        let target_package = scan_package_dir(&target_dir).map_err(|error| {
            format!(
                "Module package target '{}' already exists but is not valid: {}",
                target_dir.display(),
                error
            )
        })?;
        if target_package.manifest.id != module_id {
            return Err(format!(
                "Module package target '{}' contains module '{}', expected '{}'.",
                target_dir.display(),
                target_package.manifest.id,
                module_id
            ));
        }
        return Ok(ModulePackageInstallResult {
            module_id,
            source_dir: source_dir.to_path_buf(),
            target_dir,
            installed: false,
            package: target_package,
        });
    }

    fs::create_dir_all(modules_dir).map_err(|error| {
        format!(
            "Failed to create module directory '{}': {}",
            modules_dir.display(),
            error
        )
    })?;

    let staging_dir = modules_dir.join(format!(".{}.installing", module_id));
    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir).map_err(|error| {
            format!(
                "Failed to remove stale module install staging directory '{}': {}",
                staging_dir.display(),
                error
            )
        })?;
    }

    copy_dir_recursive(source_dir, &staging_dir)?;
    scan_package_dir(&staging_dir)?;
    fs::rename(&staging_dir, &target_dir).map_err(|error| {
        let _ = fs::remove_dir_all(&staging_dir);
        format!(
            "Failed to install module package '{}' to '{}': {}",
            module_id,
            target_dir.display(),
            error
        )
    })?;

    let installed_package = scan_package_dir(&target_dir)?;
    Ok(ModulePackageInstallResult {
        module_id,
        source_dir: source_dir.to_path_buf(),
        target_dir,
        installed: true,
        package: installed_package,
    })
}

fn copy_dir_recursive(source_dir: &Path, target_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(target_dir).map_err(|error| {
        format!(
            "Failed to create directory '{}': {}",
            target_dir.display(),
            error
        )
    })?;

    for entry in fs::read_dir(source_dir).map_err(|error| {
        format!(
            "Failed to read source directory '{}': {}",
            source_dir.display(),
            error
        )
    })? {
        let entry = entry.map_err(|error| {
            format!(
                "Failed to read entry in source directory '{}': {}",
                source_dir.display(),
                error
            )
        })?;
        let file_type = entry.file_type().map_err(|error| {
            format!(
                "Failed to read file type for '{}': {}",
                entry.path().display(),
                error
            )
        })?;
        let source_path = entry.path();
        let target_path = target_dir.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &target_path).map_err(|error| {
                format!(
                    "Failed to copy '{}' to '{}': {}",
                    source_path.display(),
                    target_path.display(),
                    error
                )
            })?;
        } else {
            return Err(format!(
                "Module package source contains unsupported entry '{}'.",
                source_path.display()
            ));
        }
    }
    Ok(())
}

pub fn scan_package_dir(package_dir: &Path) -> Result<ValidatedModulePackage, String> {
    if !package_dir.is_dir() {
        return Err(format!(
            "Module package directory '{}' does not exist.",
            package_dir.display()
        ));
    }

    let manifest_path = package_dir.join(MODULE_MANIFEST_FILE);
    let raw_manifest = fs::read_to_string(&manifest_path).map_err(|error| {
        format!(
            "Failed to read module manifest '{}': {}",
            manifest_path.display(),
            error
        )
    })?;
    let manifest =
        serde_json::from_str::<ModulePackageManifest>(&raw_manifest).map_err(|error| {
            format!(
                "Failed to parse module manifest '{}': {}",
                manifest_path.display(),
                error
            )
        })?;

    validate_manifest(package_dir, &manifest)?;

    Ok(ValidatedModulePackage {
        package_dir: package_dir.to_path_buf(),
        manifest_path,
        manifest,
    })
}

fn validate_manifest(package_dir: &Path, manifest: &ModulePackageManifest) -> Result<(), String> {
    require_non_empty("module id", &manifest.id)?;
    require_non_empty("module name", &manifest.name)?;
    require_non_empty("module version", &manifest.version)?;
    require_non_empty("host capability", &manifest.host_capability)?;

    if manifest.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(format!(
            "Module '{}' uses unsupported schema version {}. Supported schema version is {}.",
            manifest.id, manifest.schema_version, SUPPORTED_SCHEMA_VERSION
        ));
    }

    if !known_module_id(&manifest.id) {
        return Err(format!("Unknown module id '{}'.", manifest.id));
    }

    if !known_host_capability(&manifest.host_capability) {
        return Err(format!(
            "Unknown host capability '{}'.",
            manifest.host_capability
        ));
    }

    if manifest.id != manifest.host_capability {
        return Err(format!(
            "Module id '{}' must match host capability '{}' for the first package model.",
            manifest.id, manifest.host_capability
        ));
    }

    if !manifest.executable_hooks.is_empty() {
        return Err(format!(
            "Module '{}' declares executable hooks, which are not supported by the first package model.",
            manifest.id
        ));
    }

    for asset in &manifest.required_assets {
        validate_required_asset(package_dir, asset)?;
    }

    Ok(())
}

fn require_non_empty(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("Module manifest is missing {}.", label));
    }
    Ok(())
}

fn known_module_id(module_id: &str) -> bool {
    super::registry::find_manifest(module_id).is_some()
}

fn known_host_capability(host_capability: &str) -> bool {
    matches!(host_capability, "gdd")
}

fn validate_required_asset(package_dir: &Path, asset: &ModulePackageAsset) -> Result<(), String> {
    require_non_empty("asset id", &asset.id)?;
    require_non_empty("asset path", &asset.path)?;
    require_non_empty("asset kind", &asset.kind)?;

    if !matches!(asset.kind.as_str(), "file" | "directory") {
        return Err(format!(
            "Required asset '{}' uses unsupported kind '{}'.",
            asset.id, asset.kind
        ));
    }

    if !is_safe_relative_path(&asset.path) {
        return Err(format!(
            "Required asset '{}' must use a safe relative path inside the package.",
            asset.id
        ));
    }

    let asset_path = package_dir.join(&asset.path);
    match asset.kind.as_str() {
        "file" if !asset_path.is_file() => Err(format!(
            "Required asset '{}' is missing file '{}'.",
            asset.id, asset.path
        )),
        "directory" if !asset_path.is_dir() => Err(format!(
            "Required asset '{}' is missing directory '{}'.",
            asset.id, asset.path
        )),
        _ => Ok(()),
    }
}

fn is_safe_relative_path(path: &str) -> bool {
    let candidate = Path::new(path);
    !candidate.is_absolute()
        && candidate
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_package_dir(test_name: &str) -> PathBuf {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_millis();
        std::env::temp_dir().join(format!("trispr-module-package-{}-{}", test_name, millis))
    }

    fn write_valid_package(test_name: &str) -> PathBuf {
        let package_dir = unique_package_dir(test_name);
        fs::create_dir_all(package_dir.join("templates")).expect("create templates dir");
        fs::write(package_dir.join("templates/universal-strict.md"), "# GDD\n")
            .expect("write template");
        fs::write(
            package_dir.join(MODULE_MANIFEST_FILE),
            r#"{
  "schema_version": 1,
  "id": "gdd",
  "name": "GDD Automation",
  "version": "0.1.0",
  "host_capability": "gdd",
  "required_assets": [
    { "id": "universal_template", "path": "templates/universal-strict.md", "kind": "file" }
  ],
  "sub_capabilities": [
    { "id": "draft_generation", "label": "Draft Generation" },
    { "id": "confluence_publishing", "label": "Confluence Publishing", "requires_setup": true }
  ]
}"#,
        )
        .expect("write manifest");
        package_dir
    }

    #[test]
    fn accepts_valid_gdd_package() {
        let package_dir = write_valid_package("valid");
        let package = scan_package_dir(&package_dir).expect("valid package");

        assert_eq!(package.manifest.id, "gdd");
        assert_eq!(package.manifest.host_capability, "gdd");
        assert_eq!(package.manifest.required_assets.len(), 1);
        assert_eq!(package.manifest.sub_capabilities.len(), 2);

        let _ = fs::remove_dir_all(package_dir);
    }

    #[test]
    fn rejects_unknown_module_id() {
        let package_dir = write_valid_package("unknown-id");
        let manifest_path = package_dir.join(MODULE_MANIFEST_FILE);
        let manifest = fs::read_to_string(&manifest_path)
            .expect("read manifest")
            .replace("\"id\": \"gdd\"", "\"id\": \"unknown\"");
        fs::write(&manifest_path, manifest).expect("write manifest");

        let error = scan_package_dir(&package_dir).expect_err("unknown id should fail");

        assert!(error.contains("Unknown module id 'unknown'"));
        let _ = fs::remove_dir_all(package_dir);
    }

    #[test]
    fn rejects_missing_required_asset() {
        let package_dir = write_valid_package("missing-asset");
        fs::remove_file(package_dir.join("templates/universal-strict.md")).expect("remove asset");

        let error = scan_package_dir(&package_dir).expect_err("missing asset should fail");

        assert!(error.contains("Required asset 'universal_template' is missing file"));
        let _ = fs::remove_dir_all(package_dir);
    }

    #[test]
    fn rejects_path_traversal_assets() {
        let package_dir = write_valid_package("path-traversal");
        let manifest_path = package_dir.join(MODULE_MANIFEST_FILE);
        let manifest = fs::read_to_string(&manifest_path)
            .expect("read manifest")
            .replace("templates/universal-strict.md", "../outside.md");
        fs::write(&manifest_path, manifest).expect("write manifest");

        let error = scan_package_dir(&package_dir).expect_err("path traversal should fail");

        assert!(error.contains("safe relative path"));
        let _ = fs::remove_dir_all(package_dir);
    }

    #[test]
    fn rejects_executable_hooks() {
        let package_dir = write_valid_package("executable-hook");
        let manifest_path = package_dir.join(MODULE_MANIFEST_FILE);
        let manifest = fs::read_to_string(&manifest_path)
            .expect("read manifest")
            .replace(
            "\n}",
            ",\n  \"executable_hooks\": [{ \"id\": \"install\", \"command\": \"setup.exe\" }]\n}",
        );
        fs::write(&manifest_path, manifest).expect("write manifest");

        let error = scan_package_dir(&package_dir).expect_err("executable hook should fail");

        assert!(error.contains("executable hooks"));
        let _ = fs::remove_dir_all(package_dir);
    }

    #[test]
    fn scans_valid_packages_and_reports_invalid_packages() {
        let modules_dir = unique_package_dir("modules-root");
        fs::create_dir_all(&modules_dir).expect("create modules dir");
        let valid_package = write_valid_package("scan-valid");
        let invalid_package = write_valid_package("scan-invalid");
        let valid_target = modules_dir.join("gdd");
        let invalid_target = modules_dir.join("broken");
        fs::rename(&valid_package, &valid_target).expect("move valid package");
        fs::rename(&invalid_package, &invalid_target).expect("move invalid package");
        fs::remove_file(invalid_target.join("templates/universal-strict.md"))
            .expect("remove invalid asset");

        let report = scan_modules_dir(&modules_dir).expect("scan modules dir");

        assert_eq!(report.packages.len(), 1);
        assert_eq!(report.packages[0].manifest.id, "gdd");
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].error.contains("missing file"));
        let _ = fs::remove_dir_all(modules_dir);
    }

    #[test]
    fn installs_package_from_source_dir() {
        let source_dir = write_valid_package("install-source");
        let modules_dir = unique_package_dir("install-target");

        let result = install_package_from_dir(&source_dir, &modules_dir).expect("install package");

        assert!(result.installed);
        assert_eq!(result.module_id, "gdd");
        assert!(modules_dir.join("gdd").join(MODULE_MANIFEST_FILE).is_file());
        assert!(modules_dir
            .join("gdd")
            .join("templates/universal-strict.md")
            .is_file());
        let _ = fs::remove_dir_all(source_dir);
        let _ = fs::remove_dir_all(modules_dir);
    }

    #[test]
    fn install_is_idempotent_for_valid_existing_target() {
        let source_dir = write_valid_package("install-idempotent-source");
        let modules_dir = unique_package_dir("install-idempotent-target");

        let first = install_package_from_dir(&source_dir, &modules_dir).expect("first install");
        let second = install_package_from_dir(&source_dir, &modules_dir).expect("second install");

        assert!(first.installed);
        assert!(!second.installed);
        assert_eq!(second.package.manifest.id, "gdd");
        let _ = fs::remove_dir_all(source_dir);
        let _ = fs::remove_dir_all(modules_dir);
    }
}
