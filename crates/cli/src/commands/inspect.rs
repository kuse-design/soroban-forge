use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cli::InspectArgs;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InspectEntry {
    pub key: String,
    pub value: String,
}

pub fn run(args: InspectArgs) -> Result<()> {
    let output = fetch_state(&args)?;
    let entries = parse_entries(&output)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else {
        print_table(&entries);
    }

    Ok(())
}

fn fetch_state(args: &InspectArgs) -> Result<String> {
    let mut command = std::process::Command::new("stellar");
    command
        .arg("contract")
        .arg("invoke")
        .arg("--id")
        .arg(&args.contract)
        .arg("--network")
        .arg(&args.network)
        .arg("--")
        .arg("inspect")
        .arg("--json");

    if let Some(source) = &args.source {
        command.arg("--source").arg(source);
    }
    if !args.keys.is_empty() {
        command.arg("--keys").arg(args.keys.join(","));
    }

    let output = command
        .output()
        .context("failed to invoke the stellar CLI")?;
    if !output.status.success() {
        anyhow::bail!(
            "stellar contract inspect failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_entries(raw: &str) -> Result<Vec<InspectEntry>> {
    let value: Value = serde_json::from_str(raw).context("failed to parse inspect output")?;
    let object = value
        .as_object()
        .context("inspect output must be a JSON object")?;

    Ok(object
        .iter()
        .map(|(key, value)| InspectEntry {
            key: key.clone(),
            value: value.to_string(),
        })
        .collect())
}

fn print_table(entries: &[InspectEntry]) {
    if entries.is_empty() {
        println!("No contract state found.");
        return;
    }

    let key_width = entries
        .iter()
        .map(|entry| entry.key.len())
        .max()
        .unwrap_or(3);
    println!("{:<key_width$} | VALUE", "KEY");
    println!("{}-+-{}", "-".repeat(key_width), "-".repeat(20));
    for entry in entries {
        println!("{:<key_width$} | {}", entry.key, entry.value);
    }
}
