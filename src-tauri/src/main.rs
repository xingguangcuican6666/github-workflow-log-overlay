#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::io;
use std::process::Command;

const MAX_RETURNED_LOG_LINES: usize = 1_200;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GhStatus {
    installed: bool,
    version: Option<String>,
    authenticated: bool,
    auth_message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FetchRequest {
    repo: String,
    workflow: String,
    branch: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RunInfo {
    database_id: u64,
    status: String,
    conclusion: Option<String>,
    display_title: Option<String>,
    head_branch: Option<String>,
    event: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    url: Option<String>,
    workflow_name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowLogResponse {
    run: Option<RunInfo>,
    log: String,
    warning: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JobsResponse {
    jobs: Vec<JobInfo>,
}

#[derive(Debug, Deserialize)]
struct JobInfo {
    id: u64,
    name: String,
    status: String,
    conclusion: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
}

fn command_text(output: &[u8]) -> String {
    String::from_utf8_lossy(output).trim().to_string()
}

fn strip_control_sequences(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == 0x1b {
            index += 1;

            if index >= bytes.len() {
                break;
            }

            match bytes[index] {
                b'[' => {
                    index += 1;
                    while index < bytes.len() {
                        let byte = bytes[index];
                        index += 1;
                        if (0x40..=0x7e).contains(&byte) {
                            break;
                        }
                    }
                }
                b']' => {
                    index += 1;
                    while index < bytes.len() {
                        if bytes[index] == 0x07 {
                            index += 1;
                            break;
                        }

                        if bytes[index] == 0x1b
                            && index + 1 < bytes.len()
                            && bytes[index + 1] == b'\\'
                        {
                            index += 2;
                            break;
                        }

                        index += 1;
                    }
                }
                b'P' | b'^' | b'_' | b'X' => {
                    index += 1;
                    while index + 1 < bytes.len() {
                        if bytes[index] == 0x1b && bytes[index + 1] == b'\\' {
                            index += 2;
                            break;
                        }

                        index += 1;
                    }
                }
                _ => {
                    index += 1;
                }
            }

            continue;
        }

        if let Some(character) = input[index..].chars().next() {
            output.push(character);
            index += character.len_utf8();
        } else {
            break;
        }
    }

    output
}

fn has_github_timestamp_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();

    bytes.len() >= 20
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && bytes[4] == b'-'
        && bytes[5].is_ascii_digit()
        && bytes[6].is_ascii_digit()
        && bytes[7] == b'-'
        && bytes[8].is_ascii_digit()
        && bytes[9].is_ascii_digit()
        && bytes[10] == b'T'
        && bytes[11].is_ascii_digit()
        && bytes[12].is_ascii_digit()
        && bytes[13] == b':'
        && bytes[14].is_ascii_digit()
        && bytes[15].is_ascii_digit()
        && bytes[16] == b':'
        && bytes[17].is_ascii_digit()
        && bytes[18].is_ascii_digit()
}

fn strip_leading_github_timestamp(value: &str) -> Option<&str> {
    let trimmed = value.trim_start();

    if !has_github_timestamp_prefix(trimmed) {
        return None;
    }

    let timestamp_end = trimmed
        .as_bytes()
        .iter()
        .take(40)
        .position(|byte| *byte == b'Z')?;

    if trimmed
        .as_bytes()
        .get(timestamp_end + 1)
        .is_some_and(|byte| !byte.is_ascii_whitespace())
    {
        return None;
    }

    Some(trimmed[timestamp_end + 1..].trim_start())
}

fn is_noise_log_line(line: &str) -> bool {
    const NOISE_NEEDLES: &[&str] = &[
        "/github/runner_temp/git-credentials",
        "/home/runner/work/_temp/git-credentials",
        "git-credentials-",
        "git config --local --unset includeif.gitdir:",
        "git submodule foreach --recursive git config --local --show-origin --name-only --get-regexp remote.origin.url",
        "Removing credentials config",
        "Cleaning up orphan processes",
        "Terminate orphan process:",
        "Current runner version:",
        "Runner name:",
        "Runner group name:",
        "Machine name:",
        "Runner Image Provisioner",
        "Operating System",
        "Runner Image",
        "Image Version",
        "Included Software",
        "Image Release",
        "GITHUB_TOKEN Permissions",
        "Secret source:",
        "Prepare workflow directory",
        "Prepare all required actions",
        "Getting action download info",
        "Download action repository",
        "Complete job name:",
        "Set up job",
        "Post job cleanup.",
    ];

    let trimmed = line.trim();

    if trimmed == "##[endgroup]" || trimmed == "##[group]" {
        return true;
    }

    if trimmed.starts_with("##[group]Run actions/checkout@")
        || trimmed.starts_with("##[group]Run actions/setup-")
        || trimmed.starts_with("##[group]Runner Image")
    {
        return true;
    }

    NOISE_NEEDLES.iter().any(|needle| trimmed.contains(needle))
}

fn compact_log_line(line: &str) -> String {
    let mut fields = line.splitn(3, '\t');

    if let (Some(_job), Some(step), Some(message)) = (fields.next(), fields.next(), fields.next()) {
        if let Some(message) = strip_leading_github_timestamp(message) {
            let step = step.trim();

            if step.is_empty() || step == "UNKNOWN STEP" {
                return message.to_string();
            }

            return format!("{step} | {message}");
        }
    }

    strip_leading_github_timestamp(line)
        .unwrap_or(line)
        .to_string()
}

fn clean_log_line(line: &str) -> Option<String> {
    let stripped = strip_control_sequences(line)
        .trim_start_matches('\u{feff}')
        .trim_end()
        .to_string();

    if stripped.trim().is_empty() || is_noise_log_line(&stripped) {
        return None;
    }

    let compacted = compact_log_line(&stripped);
    let compacted = compacted.trim().to_string();

    if compacted.is_empty() || is_noise_log_line(&compacted) {
        return None;
    }

    Some(compacted)
}

fn sanitize_log_lines(log: &str) -> Vec<String> {
    log.replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .filter_map(clean_log_line)
        .collect()
}

fn tail_log_lines(mut lines: Vec<String>, max_lines: usize) -> Vec<String> {
    if lines.len() <= max_lines {
        return lines;
    }

    lines.split_off(lines.len() - max_lines)
}

fn sanitize_log(log: &str) -> String {
    tail_log_lines(sanitize_log_lines(log), MAX_RETURNED_LOG_LINES).join("\n")
}

fn run_gh(args: &[String]) -> Result<String, String> {
    let output = Command::new("gh").args(args).output().map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            "GitHub CLI (gh) was not found in PATH.".to_string()
        } else {
            format!("Failed to run gh: {error}")
        }
    })?;

    let stdout = command_text(&output.stdout);
    let stderr = command_text(&output.stderr);

    if output.status.success() {
        Ok(stdout)
    } else if stderr.is_empty() {
        Err(format!("gh exited with status {}", output.status))
    } else {
        Err(stderr)
    }
}

fn gh_status_blocking() -> GhStatus {
    let version_output = Command::new("gh").arg("--version").output();

    let Ok(version_output) = version_output else {
        return GhStatus {
            installed: false,
            version: None,
            authenticated: false,
            auth_message: Some("GitHub CLI (gh) was not found in PATH.".to_string()),
        };
    };

    if !version_output.status.success() {
        return GhStatus {
            installed: false,
            version: None,
            authenticated: false,
            auth_message: Some(command_text(&version_output.stderr)),
        };
    }

    let version = command_text(&version_output.stdout)
        .lines()
        .next()
        .map(str::to_string);

    let auth_output = Command::new("gh").args(["auth", "status"]).output();
    let (authenticated, auth_message) = match auth_output {
        Ok(output) => {
            let message = if output.stderr.is_empty() {
                command_text(&output.stdout)
            } else {
                command_text(&output.stderr)
            };
            (output.status.success(), Some(message))
        }
        Err(error) => (
            false,
            Some(format!("Failed to check gh auth status: {error}")),
        ),
    };

    GhStatus {
        installed: true,
        version,
        authenticated,
        auth_message,
    }
}

fn fetch_job_logs(repo: &str, run_id: u64) -> Result<WorkflowLogResponse, String> {
    let jobs_endpoint = format!("repos/{repo}/actions/runs/{run_id}/jobs?per_page=100");
    let jobs_args = vec!["api".to_string(), jobs_endpoint];
    let jobs_json = run_gh(&jobs_args)?;
    let jobs_response: JobsResponse = serde_json::from_str(&jobs_json)
        .map_err(|error| format!("Failed to parse workflow jobs output: {error}"))?;

    if jobs_response.jobs.is_empty() {
        return Ok(WorkflowLogResponse {
            run: None,
            log: String::new(),
            warning: Some("Run found, but no jobs are available yet.".to_string()),
        });
    }

    let mut combined_log = String::new();
    let mut unavailable = Vec::new();

    for job in jobs_response.jobs {
        let state = job.conclusion.as_deref().unwrap_or(&job.status);
        let job_name = job.name;
        combined_log.push_str(&format!("\n===== Job: {} ({state}) =====\n", job_name));

        if let Some(started_at) = &job.started_at {
            combined_log.push_str(&format!("started_at: {started_at}\n"));
        }

        if let Some(completed_at) = &job.completed_at {
            combined_log.push_str(&format!("completed_at: {completed_at}\n"));
        }

        let log_endpoint = format!("repos/{repo}/actions/jobs/{}/logs", job.id);
        let log_args = vec!["api".to_string(), log_endpoint];

        match run_gh(&log_args) {
            Ok(job_log) if !job_log.trim().is_empty() => {
                let job_log = sanitize_log(&job_log);
                combined_log.push_str(&job_log);
                combined_log.push('\n');
            }
            Ok(_) => {
                unavailable.push(job_name.clone());
                combined_log.push_str("Log is not available for this job yet.\n");
            }
            Err(error) => {
                unavailable.push(job_name.clone());
                combined_log.push_str(&format!("Log is not available for this job yet: {error}\n"));
            }
        }
    }

    let warning = if unavailable.is_empty() {
        None
    } else {
        Some(format!(
            "Live logs are partial; unavailable jobs: {}.",
            unavailable.join(", ")
        ))
    };

    Ok(WorkflowLogResponse {
        run: None,
        log: combined_log.trim_start().trim_end().to_string(),
        warning,
    })
}

fn fetch_workflow_log_blocking(request: FetchRequest) -> Result<WorkflowLogResponse, String> {
    let repo = request.repo.trim();
    let workflow = request.workflow.trim();
    let branch = request
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if repo.is_empty() || !repo.contains('/') {
        return Err("Repository must use owner/repo format.".to_string());
    }

    if workflow.is_empty() {
        return Err("Workflow name or workflow file is required.".to_string());
    }

    let mut list_args = vec![
        "run".to_string(),
        "list".to_string(),
        "-R".to_string(),
        repo.to_string(),
        "-w".to_string(),
        workflow.to_string(),
        "--limit".to_string(),
        "1".to_string(),
        "--json".to_string(),
        "databaseId,status,conclusion,displayTitle,headBranch,event,createdAt,updatedAt,url,workflowName".to_string(),
    ];

    if let Some(branch) = branch {
        list_args.push("-b".to_string());
        list_args.push(branch.to_string());
    }

    let runs_json = run_gh(&list_args)?;
    let runs: Vec<RunInfo> = serde_json::from_str(&runs_json)
        .map_err(|error| format!("Failed to parse gh run list output: {error}"))?;

    let Some(run) = runs.into_iter().next() else {
        return Ok(WorkflowLogResponse {
            run: None,
            log: String::new(),
            warning: Some("No workflow runs were found for this configuration.".to_string()),
        });
    };

    let view_args = vec![
        "run".to_string(),
        "view".to_string(),
        run.database_id.to_string(),
        "-R".to_string(),
        repo.to_string(),
        "--log".to_string(),
    ];

    match run_gh(&view_args) {
        Ok(log) => Ok(WorkflowLogResponse {
            run: Some(run),
            log: sanitize_log(&log),
            warning: None,
        }),
        Err(error) => {
            let live_logs = fetch_job_logs(repo, run.database_id)?;
            Ok(WorkflowLogResponse {
                run: Some(run),
                log: live_logs.log,
                warning: live_logs.warning.or_else(|| {
                    Some(format!(
                        "Using job-level live logs because run-level logs are not available yet: {error}"
                    ))
                }),
            })
        }
    }
}

#[tauri::command]
async fn check_gh() -> Result<GhStatus, String> {
    tauri::async_runtime::spawn_blocking(gh_status_blocking)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn fetch_workflow_log(request: FetchRequest) -> Result<WorkflowLogResponse, String> {
    tauri::async_runtime::spawn_blocking(move || fetch_workflow_log_blocking(request))
        .await
        .map_err(|error| error.to_string())?
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![check_gh, fetch_workflow_log])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
