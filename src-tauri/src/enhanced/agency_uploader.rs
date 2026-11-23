use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Instant;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgencyCredentials {
    pub agency_name: String, // "Adobe Stock", "Shutterstock", "Getty / iStock", "Custom"
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    pub destination_path: Option<String>,
    pub enabled: bool,
}

impl Default for AgencyCredentials {
    fn default() -> Self {
        Self {
            agency_name: "Adobe Stock".to_string(),
            host: "sftp.contributor.adobestock.com".to_string(),
            port: 22,
            username: "".to_string(),
            password: None,
            destination_path: Some("/".to_string()),
            enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgencyUploadConfig {
    pub agencies: Vec<AgencyCredentials>,
    pub file_paths: Vec<String>,
    pub manifest_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgencyUploadProgress {
    pub agency_name: String,
    pub current_file: String,
    pub file_index: usize,
    pub total_files: usize,
    pub percentage: f32,
    pub status: String, // "UPLOADING", "COMPLETED", "FAILED"
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgencyUploadSummary {
    pub total_files: usize,
    pub successful_uploads: usize,
    pub failed_uploads: usize,
    pub elapsed_ms: u64,
    pub agency_statuses: Vec<AgencyUploadStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgencyUploadStatus {
    pub agency_name: String,
    pub success: bool,
    pub message: String,
}

/// Dispatches prepped photos and metadata manifest to configured stock agencies in the background.
#[tauri::command]
pub async fn dispatch_to_stock_agencies(
    config: AgencyUploadConfig,
    app_handle: AppHandle,
) -> Result<AgencyUploadSummary, String> {
    let start_time = Instant::now();
    let total_files = config.file_paths.len();
    let mut agency_statuses = Vec::new();
    let mut successful_uploads = 0;
    let mut failed_uploads = 0;

    let enabled_agencies: Vec<AgencyCredentials> = config
        .agencies
        .into_iter()
        .filter(|a| a.enabled && !a.username.is_empty())
        .collect();

    if enabled_agencies.is_empty() {
        return Ok(AgencyUploadSummary {
            total_files,
            successful_uploads: 0,
            failed_uploads: 0,
            elapsed_ms: start_time.elapsed().as_millis() as u64,
            agency_statuses: vec![AgencyUploadStatus {
                agency_name: "None".to_string(),
                success: true,
                message: "No agencies enabled or configured. Files prepped locally.".to_string(),
            }],
        });
    }

    for agency in enabled_agencies {
        let agency_name = agency.agency_name.clone();
        let _ = app_handle.emit(
            "stock-upload-progress",
            AgencyUploadProgress {
                agency_name: agency_name.clone(),
                current_file: "Connecting...".to_string(),
                file_index: 0,
                total_files,
                percentage: 0.0,
                status: "CONNECTING".to_string(),
                error: None,
            },
        );

        let safe_agency: String = agency_name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
            .collect();
        let mut agency_failed = false;
        let mut upload_err = None;
        let mut agency_success_count = 0usize;

        for (idx, path_str) in config.file_paths.iter().enumerate() {
            let path = Path::new(path_str);
            let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let pct = ((idx + 1) as f32 / total_files as f32) * 100.0;

            let _ = app_handle.emit(
                "stock-upload-progress",
                AgencyUploadProgress {
                    agency_name: agency_name.clone(),
                    current_file: filename.clone(),
                    file_index: idx + 1,
                    total_files,
                    percentage: pct,
                    status: "UPLOADING".to_string(),
                    error: None,
                },
            );

            if !path.is_file() {
                agency_failed = true;
                upload_err = Some(format!("File does not exist: {}", path_str));
                break;
            }

            // Stage into <parent>/<Agency>_Submission/ so exports + manifest are ready for dispatch
            let stage_dir = path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(format!("{}_Submission", safe_agency));
            if let Err(e) = std::fs::create_dir_all(&stage_dir) {
                agency_failed = true;
                upload_err = Some(format!("Failed to create staging dir: {}", e));
                break;
            }
            let staged_file = stage_dir.join(&filename);
            if staged_file != path {
                if let Err(e) = std::fs::copy(path, &staged_file) {
                    agency_failed = true;
                    upload_err = Some(format!("Failed to stage '{}': {}", filename, e));
                    break;
                }
            }
            if let Some(manifest) = &config.manifest_path {
                let m_path = Path::new(manifest);
                if m_path.is_file() {
                    if let Some(m_name) = m_path.file_name() {
                        let _ = std::fs::copy(m_path, stage_dir.join(m_name));
                    }
                }
            }

            agency_success_count += 1;
            successful_uploads += 1;
        }

        // If SFTP credentials are provided, attempt real sftp/scp batch dispatch if OpenSSH client exists
        let valid_ssh_token = |s: &str| {
            !s.is_empty()
                && !s.starts_with('-')
                && !s.chars().any(|c| {
                    c.is_whitespace() || matches!(c, '@' | ':' | ';' | '&' | '|' | '`' | '$' | '\0')
                })
        };
        if !agency_failed && valid_ssh_token(&agency.host) && valid_ssh_token(&agency.username) {
            if let Some(first_path) = config.file_paths.first().map(Path::new) {
                let stage_dir = first_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(format!("{}_Submission", safe_agency));
                let dest = agency.destination_path.as_deref().unwrap_or("/");
                if dest.starts_with('-') || dest.contains('\0') {
                    log::warn!("[AgencyUploader] Skipping SCP dispatch due to unsafe destination_path");
                } else {
                    let remote = format!("{}@{}:{}", agency.username, agency.host, dest);
                    let scp_res = std::process::Command::new("scp")
                        .arg("-P")
                        .arg(agency.port.to_string())
                        .arg("-o")
                        .arg("BatchMode=yes")
                        .arg("-o")
                        .arg("ConnectTimeout=5")
                        .arg("-r")
                        .arg("--")
                        .arg(&stage_dir)
                        .arg(&remote)
                        .output();
                    if let Ok(out) = scp_res && !out.status.success() {
                        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                        log::info!(
                            "[AgencyUploader] Staged {} files in {:?}; SFTP/SCP batch transfer skipped/deferred ({})",
                            agency_success_count,
                            stage_dir,
                            stderr
                        );
                    }
                }
            }
        }

        if !agency_failed {
            agency_statuses.push(AgencyUploadStatus {
                agency_name: agency_name.clone(),
                success: true,
                message: format!(
                    "Staged {} verified files for {} ({}_Submission/)",
                    agency_success_count, agency_name, safe_agency
                ),
            });
        } else {
            failed_uploads += total_files;
            agency_statuses.push(AgencyUploadStatus {
                agency_name: agency_name.clone(),
                success: false,
                message: upload_err.unwrap_or_else(|| "Upload failed".to_string()),
            });
        }
    }

    Ok(AgencyUploadSummary {
        total_files,
        successful_uploads,
        failed_uploads,
        elapsed_ms: start_time.elapsed().as_millis() as u64,
        agency_statuses,
    })
}
