use crate::cli::ScaffoldValidateArgs;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

const REQUIRED_FILES: [&str; 6] = [
    "Cargo.toml",
    "README.md",
    "src/lib.rs",
    "src/contract.rs",
    "src/types.rs",
    "src/errors.rs",
];
const PACKAGE_PREFIX: &str = "soroban-forge-";

pub fn run(args: ScaffoldValidateArgs) -> Result<()> {
    validate_scaffold(&args.path)?;
    println!("Scaffold at '{}' is valid", args.path.display());
    Ok(())
}

fn validate_scaffold(root: &Path) -> Result<()> {
    if !root.is_dir() {
        bail!("Scaffold path '{}' is not a directory", root.display());
    }

    for relative in REQUIRED_FILES {
        let path = root.join(relative);
        if !path.is_file() {
            bail!(
                "Invalid scaffold at '{}': missing required file '{}'",
                root.display(),
                relative
            );
        }
    }

    let manifest_path = root.join("Cargo.toml");
    let manifest_text = fs::read_to_string(&manifest_path).with_context(|| {
        format!(
            "Could not read scaffold manifest at '{}'",
            manifest_path.display()
        )
    })?;
    let manifest: toml::Value = toml::from_str(&manifest_text).with_context(|| {
        format!(
            "Invalid TOML in scaffold manifest '{}'",
            manifest_path.display()
        )
    })?;

    let package_name = manifest
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| {
            anyhow::anyhow!("Invalid scaffold manifest: [package].name must be a string")
        })?;
    let Some(contract_name) = package_name.strip_prefix(PACKAGE_PREFIX) else {
        bail!("Invalid scaffold manifest: [package].name must start with '{PACKAGE_PREFIX}'");
    };
    if contract_name.is_empty() {
        bail!("Invalid scaffold manifest: [package].name must include a contract name");
    }

    let has_cdylib = manifest
        .get("lib")
        .and_then(|lib| lib.get("crate-type"))
        .and_then(toml::Value::as_array)
        .is_some_and(|crate_types| {
            crate_types
                .iter()
                .any(|crate_type| crate_type.as_str() == Some("cdylib"))
        });
    if !has_cdylib {
        bail!("Invalid scaffold manifest: [lib].crate-type must include 'cdylib'");
    }

    let has_soroban_sdk = manifest
        .get("dependencies")
        .and_then(|dependencies| dependencies.get("soroban-sdk"))
        .is_some();
    if !has_soroban_sdk {
        bail!("Invalid scaffold manifest: [dependencies] must include 'soroban-sdk'");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_scaffold;
    use std::fs;
    use tempfile::tempdir;

    fn create_valid_scaffold(root: &std::path::Path) -> anyhow::Result<()> {
        fs::create_dir_all(root.join("src"))?;
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"soroban-forge-example\"\nversion = \"0.2.0\"\nedition = \"2021\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\nsoroban-sdk = \"=27.0.6\"\n",
        )?;
        for relative in [
            "README.md",
            "src/lib.rs",
            "src/contract.rs",
            "src/types.rs",
            "src/errors.rs",
        ] {
            fs::write(root.join(relative), "")?;
        }
        Ok(())
    }

    #[test]
    fn accepts_a_complete_template_shaped_scaffold() -> anyhow::Result<()> {
        let directory = tempdir()?;
        create_valid_scaffold(directory.path())?;

        validate_scaffold(directory.path())
    }

    #[test]
    fn reports_a_missing_required_template_file() -> anyhow::Result<()> {
        let directory = tempdir()?;
        create_valid_scaffold(directory.path())?;
        fs::remove_file(directory.path().join("src/types.rs"))?;

        let error = validate_scaffold(directory.path()).unwrap_err();
        assert!(error
            .to_string()
            .contains("missing required file 'src/types.rs'"));
        Ok(())
    }

    #[test]
    fn reports_an_incoherent_manifest() -> anyhow::Result<()> {
        let directory = tempdir()?;
        create_valid_scaffold(directory.path())?;
        fs::write(
            directory.path().join("Cargo.toml"),
            "[package]\nname = \"example\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\nsoroban-sdk = \"=27.0.6\"\n",
        )?;

        let error = validate_scaffold(directory.path()).unwrap_err();
        assert!(error.to_string().contains("[package].name must start with"));
        Ok(())
    }

    #[test]
    fn reports_a_missing_contract_dependency() -> anyhow::Result<()> {
        let directory = tempdir()?;
        create_valid_scaffold(directory.path())?;
        fs::write(
            directory.path().join("Cargo.toml"),
            "[package]\nname = \"soroban-forge-example\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n",
        )?;

        let error = validate_scaffold(directory.path()).unwrap_err();
        assert!(error
            .to_string()
            .contains("[dependencies] must include 'soroban-sdk'"));
        Ok(())
    }
}
