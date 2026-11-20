use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn run(args: crate::cli::PlanArgs) -> Result<()> {
    let plan = load_plan(&args.plan)?;
    validate_plan(&plan)?;

    if args.dry_run {
        dry_run(&plan, &args.plan)?;
        return Ok(());
    }

    if !args.force {
        confirmation_prompt(&plan)?;
    }

    execute_plan(&plan, &args.plan, args.receipt.as_deref())?;
    Ok(())
}

fn load_plan(path: &Path) -> Result<Plan> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read plan file at {path:?}"))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse plan file at {path:?}"))
}

fn validate_plan(plan: &Plan) -> Result<()> {
    if plan.version != "1" {
        bail!(
            "unsupported plan version `{version}`: only version `1` is supported",
            version = plan.version
        );
    }
    if plan.network.trim().is_empty() {
        bail!("plan.network must be a non-empty string");
    }
    if plan.source.trim().is_empty() {
        bail!("plan.source must be a non-empty source account");
    }
    if plan.calls.is_empty() {
        bail!("plan.calls must contain at least one call");
    }
    for (idx, call) in plan.calls.iter().enumerate() {
        validate_call(idx, call)?;
    }
    Ok(())
}

fn validate_call(idx: usize, call: &Call) -> Result<()> {
    if call.contract.trim().is_empty() {
        bail!("call[{idx}].contract must be a non-empty contract address");
    }
    if call.function.trim().is_empty() {
        bail!("call[{idx}].function must be a non-empty function name");
    }
    for (jdx, arg) in call.args.iter().enumerate() {
        if arg.contains('=') {
            let (key, value) = arg
                .split_once('=')
                .expect("split_once failed for arg with `=`");
            if key.trim().is_empty() {
                bail!("call[{idx}].args[{jdx}] has an empty key");
            }
        } else {
            bail!("call[{idx}].args[{jdx}] must be in `key=value` form");
        }
    }
    Ok(())
}

fn dry_run(plan: &Plan, path: &Path) -> Result<()> {
    let report = DryRunReport::from_plan(plan);
    let serialized = serde_json::to_string_pretty(&report)?;
    fs::write(path.with_extension("dryrun.json"), serialized).with_context(|| {
        format!(
            "failed to write dry-run report at {:?}",
            path.with_extension("dryrun.json")
        )
    })?;
    println!("dry-run report written to {path:?}.dryrun.json");
    Ok(())
}

fn confirmation_prompt(plan: &Plan) -> Result<()> {
    let call_summary: Vec<String> = plan
        .calls
        .iter()
        .enumerate()
        .map(|(idx, call)| format!("  {idx}: {}({})", call.function, call.contract))
        .collect();
    println!("Transaction plan preview:");
    println!("  network: {}", plan.network);
    println!("  source:  {}", plan.source);
    println!("  calls:   {}", plan.calls.len());
    for line in &call_summary {
        println!("{line}");
    }
    println!("This will execute the plan through the existing CLI transport.");
    println!("Type `yes` to continue: ");
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .context("failed to read confirmation from stdin")?;
    if answer.trim().to_ascii_lowercase() != "yes" {
        bail!("plan execution aborted by user");
    }
    Ok(())
}

fn execute_plan(plan: &Plan, plan_path: &Path, receipt_path: Option<&Path>) -> Result<()> {
    let receipt_path = receipt_path
        .map(PathBuf::from)
        .unwrap_or_else(|| plan_path.with_extension("receipt.json"));
    let mut receipt = PlanReceipt {
        plan_file: plan_path.to_string_lossy().into_owned(),
        network: plan.network.clone(),
        started_at: chrono_now_iso(),
        steps: Vec::new(),
        completed_transaction_hashes: Vec::new(),
        errors: Vec::new(),
    };

    for (idx, call) in plan.calls.iter().enumerate() {
        let step_result = execute_call(idx, call, &plan.source, &plan.network);
        let step = PlanStep {
            step_index: idx,
            contract: call.contract.clone(),
            function: call.function.clone(),
            args: call.args.clone(),
            status: step_result.status.clone(),
            detail: step_result.detail.clone(),
            transaction_hash: step_result.transaction_hash.clone(),
        };
        receipt.steps.push(step);
        if let Some(hash) = &step_result.transaction_hash {
            receipt.completed_transaction_hashes.push(hash.clone());
        }
        if let Err(ref detail) = step_result.status {
            receipt.errors.push(format!("step {idx}: {detail}"));
        }
        if step_result.status.is_err() {
            let _ = fs::write(&receipt_path, serde_json::to_string_pretty(&receipt)?);
            let error_message = step_result.status.as_ref().unwrap_err().clone();
            bail!(
                "plan failed at step {idx}: {detail}",
                idx = idx,
                detail = error_message,
            );
        }
        if let Some(assertion) = &call.assertion {
            evaluate_assertion(&assertion, &step, &receipt_path)?;
        }
    }

    receipt.succeeded = true;
    receipt.finished_at = Some(chrono_now_iso());
    fs::write(&receipt_path, serde_json::to_string_pretty(&receipt)?)
        .with_context(|| format!("failed to write receipt at {:?}", receipt_path))?;
    println!("plan completed; receipt written to {receipt_path:?}");
    Ok(())
}

fn execute_call(idx: usize, call: &Call, source: &str, network: &str) -> StepResult {
    match call.kind.as_deref().unwrap_or("invoke") {
        "invoke" => execute_invoke(idx, call, source, network),
        "deploy" => execute_deploy(idx, call, source, network),
        other => {
            let msg = format!("unsupported call kind `{other}`");
            StepResult {
                status: Err(msg.clone()),
                detail: None,
                transaction_hash: None,
            }
        }
    }
}

fn execute_invoke(idx: usize, call: &Call, source: &str, network: &str) -> StepResult {
    let mut cmd = Command::new("stellar");
    cmd.arg("contract")
        .arg("invoke")
        .arg("--id")
        .arg(&call.contract);
    if !source.is_empty() {
        cmd.arg("--source-account").arg(source);
    }
    cmd.arg("--network").arg(network);
    cmd.arg("--").arg(&call.function);
    for arg in &call.args {
        let (key, value) = arg
            .split_once('=')
            .expect("args are validated as key=value");
        cmd.arg(format!("--{key}")).arg(value);
    }
    match cmd.output() {
        Ok(output) => {
            let ok = output.status.success();
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            let combined: String = vec![stdout, stderr]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if ok {
                StepResult {
                    status: Ok(()),
                    detail: Some(combined),
                    transaction_hash: first_transaction_hash(&stdout),
                }
            } else {
                StepResult {
                    status: Err(combined),
                    detail: Some(combined),
                    transaction_hash: None,
                }
            }
        }
        Err(error) => {
            let msg = error.to_string();
            StepResult {
                status: Err(msg.clone()),
                detail: None,
                transaction_hash: None,
            }
        }
    }
}

fn execute_deploy(idx: usize, call: &Call, source: &str, network: &str) -> StepResult {
    let wasm = call
        .args
        .iter()
        .find_map(|arg| {
            let (key, value) = arg
                .split_once('=')
                .expect("args are validated as key=value");
            if key == "wasm" {
                Some(value)
            } else {
                None
            }
        })
        .ok_or_else(|| "deploy call must include wasm=WASM_PATH".to_string())?;
    let mut cmd = Command::new("stellar");
    cmd.arg("contract").arg("deploy").arg("--wasm").arg(wasm);
    if !source.is_empty() {
        cmd.arg("--source-account").arg(source);
    }
    cmd.arg("--network").arg(network);
    match cmd.output() {
        Ok(output) => {
            let ok = output.status.success();
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            let combined: String = vec![stdout, stderr]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if ok {
                StepResult {
                    status: Ok(()),
                    detail: Some(combined),
                    transaction_hash: first_transaction_hash(&stdout),
                }
            } else {
                StepResult {
                    status: Err(combined),
                    detail: Some(combined),
                    transaction_hash: None,
                }
            }
        }
        Err(error) => {
            let msg = error.to_string();
            StepResult {
                status: Err(msg.clone()),
                detail: None,
                transaction_hash: None,
            }
        }
    }
}

fn first_transaction_hash(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let trimmed = line.trim();
        let lowered = trimmed.to_ascii_lowercase();
        if lowered.starts_with("transaction:") {
            return Some(lowered["transaction:".len()..].trim().to_string());
        }
        let segments: Vec<&str> = trimmed.split_whitespace().collect();
        if segments.len() >= 3 {
            let candidate = segments[segments.len() - 1];
            if candidate.chars().all(|c| c.is_ascii_hexdigit()) && candidate.len() == 64 {
                return Some(candidate.to_ascii_lowercase());
            }
        }
        None
    })
}

fn evaluate_assertion(assertion: &Assertion, step: &PlanStep, _receipt_path: &Path) -> Result<()> {
    match assertion.kind.as_deref().unwrap_or("result") {
        "result" => {
            let found = step
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains(&assertion.expect);
            if !found {
                bail!(
                    "step {step_index} assertion failed: expected output to contain {expect:?}",
                    step_index = step.step_index,
                    expect = assertion.expect
                );
            }
        }
        "event" => {
            let found = step
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains(&assertion.expect);
            if !found {
                bail!(
                    "step {step_index} event assertion failed: expected event data to contain {expect:?}",
                    step_index = step.step_index,
                    expect = assertion.expect
                );
            }
        }
        other => bail!("unsupported assertion kind `{other}`"),
    }
    Ok(())
}

fn chrono_now_iso() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = now.as_secs();
    let days = total_secs / 86_400;
    let remaining = total_secs % 86_400;
    let hours = remaining / 3_600;
    let minutes = (remaining % 3_600) / 60;
    let seconds = remaining % 60;
    let (y, m, d) = days_to_ymd(days as i64);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d, hours, minutes, seconds
    )
}

fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    let total = days + 719_468;
    let year = if total >= 0 {
        (total as f64 / 365.2425).floor() as i64
    } else {
        (total as f64 / 365.2425).ceil() as i64 - 1
    };
    let mut year = year.max(1);
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        let next_start = cumulative_days_before_year(year + 1);
        if total < next_start {
            break;
        }
        year += 1;
    }
    let start = cumulative_days_before_year(year);
    let doy = (total - start) as u32;
    let month = month_from_doy(year, doy);
    let day = doy - cumulative_days_before_month(year, month) + 1;
    (year, month, day)
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0) && (year % 100 != 0 || year % 400 == 0)
}

fn cumulative_days_before_year(year: i64) -> i64 {
    let y = year - 1;
    y * 365 + y / 4 - y / 100 + y / 400
}

fn month_from_doy(year: i64, doy: u32) -> u32 {
    let cumulative: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let leap_adj = if is_leap(year) && doy >= 59 { 1 } else { 0 };
    let mut month = 0;
    for (i, &c) in cumulative.iter().enumerate() {
        if doy >= c + leap_adj {
            month = i as u32;
        }
    }
    if month >= 11 {
        11
    } else {
        month + 1
    }
}

fn cumulative_days_before_month(year: i64, month: u32) -> u32 {
    let cumulative: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let leap_adj = if is_leap(year) && month > 2 { 1 } else { 0 };
    cumulative[(month - 1) as usize] + leap_adj
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Plan {
    pub version: String,
    pub network: String,
    pub source: String,
    pub calls: Vec<Call>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Call {
    #[serde(default)]
    pub kind: Option<String>,
    pub contract: String,
    pub function: String,
    pub args: Vec<String>,
    pub assertion: Option<Assertion>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Assertion {
    #[serde(default = "default_assertion_kind")]
    pub kind: String,
    pub expect: String,
}

fn default_assertion_kind() -> String {
    "result".to_string()
}

#[derive(Debug, Serialize, Clone)]
pub struct PlanReceipt {
    pub plan_file: String,
    pub network: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub succeeded: bool,
    pub steps: Vec<PlanStep>,
    pub completed_transaction_hashes: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct PlanStep {
    pub step_index: usize,
    pub contract: String,
    pub function: String,
    pub args: Vec<String>,
    pub status: ResultStatus,
    pub detail: Option<String>,
    pub transaction_hash: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ResultStatus {
    pub ok: bool,
}

impl From<anyhow::Result<()>> for ResultStatus {
    fn from(result: anyhow::Result<()>) -> Self {
        match result {
            Ok(()) => ResultStatus { ok: true },
            Err(_) => ResultStatus { ok: false },
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct DryRunReport {
    pub version: String,
    pub network: String,
    pub source: String,
    pub calls: Vec<DryRunCall>,
    pub authorization_summary: Vec<DryRunAuthorization>,
}

#[derive(Debug, Serialize, Clone)]
pub struct DryRunCall {
    pub step_index: usize,
    pub kind: String,
    pub contract: String,
    pub function: String,
    pub args: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct DryRunAuthorization {
    pub call_index: usize,
    pub note: String,
}

impl DryRunReport {
    fn from_plan(plan: &Plan) -> Self {
        let calls = plan
            .calls
            .iter()
            .enumerate()
            .map(|(idx, call)| DryRunCall {
                step_index: idx,
                kind: call.kind.as_deref().unwrap_or("invoke").to_string(),
                contract: call.contract.clone(),
                function: call.function.clone(),
                args: call.args.clone(),
            })
            .collect();
        let authorization_summary = plan
            .calls
            .iter()
            .enumerate()
            .map(|(idx, call)| DryRunAuthorization {
                call_index: idx,
                note: authorization_note(call),
            })
            .collect();
        DryRunReport {
            version: plan.version.clone(),
            network: plan.network.clone(),
            source: plan.source.clone(),
            calls,
            authorization_summary,
        }
    }
}

fn authorization_note(call: &Call) -> String {
    if call.function == "create_escrow" {
        return "buyer authorizes token transfer and contract invocation".to_string();
    }
    if matches!(
        call.function.as_str(),
        "deposit" | "release" | "refund" | "dispute" | "resolve" | "cancel"
    ) || call.function.starts_with("release_partial")
    {
        return "caller authorizes the nested SEP-41 token transfer and contract call".to_string();
    }
    "caller authorizes the contract invocation".to_string()
}
