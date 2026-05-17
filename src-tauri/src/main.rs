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
        Err(error) => (false, Some(format!("Failed to check gh auth status: {error}"))),
    };

    GhStatus {
        installed: true,
        version,
        authenticated,
        auth_message,
    }
}

fn fetch_workflow_log_blocking(request: FetchRequest) -> Result<WorkflowLogResponse, String> {
    let repo = request.repo.trim();
    let workflow = request.workflow.trim();
    let branch = request.branch.as_deref().map(str::trim).filter(|value| !value.is_empty());

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
        Err(error) => Ok(WorkflowLogResponse {
            run: Some(run),
            log: String::new(),
            warning: Some(format!("Run found, but logs are not available yet: {error}")),
        }),
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

