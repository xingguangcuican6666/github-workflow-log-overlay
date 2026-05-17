#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::io;
use std::process::Command;

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
        log: combined_log.trim_start().to_string(),
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
            log,
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
