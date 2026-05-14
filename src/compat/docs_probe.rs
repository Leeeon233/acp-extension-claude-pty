use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::compat::claude_probe::ClaudeCli;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LiveProbe<T> {
    Available { value: T },
    Unavailable { reason: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NpmPackageInfo {
    pub package: String,
    pub latest: Option<String>,
    pub stable: Option<String>,
    pub next: Option<String>,
    pub modified: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LiveDocsReport {
    pub npm: LiveProbe<NpmPackageInfo>,
    pub docs: LiveProbe<DocsProbeInfo>,
    pub docs_urls: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocsProbeInfo {
    pub checked_urls: Vec<String>,
    pub missing_required_flags: Vec<String>,
    pub removed_flags_still_documented: Vec<String>,
}

impl LiveDocsReport {
    pub fn summary(&self) -> String {
        let npm = match &self.npm {
            LiveProbe::Available { value } => format!(
                "npm latest={}, stable={}",
                value.latest.as_deref().unwrap_or("unknown"),
                value.stable.as_deref().unwrap_or("unknown")
            ),
            LiveProbe::Unavailable { reason } => format!("npm unavailable: {reason}"),
        };
        let docs = match &self.docs {
            LiveProbe::Available { value } => {
                if value.missing_required_flags.is_empty() {
                    "docs required flags ok".to_string()
                } else {
                    format!("docs drift: missing={:?}", value.missing_required_flags)
                }
            }
            LiveProbe::Unavailable { reason } => format!("docs unavailable: {reason}"),
        };
        format!("{npm}; {docs}")
    }
}

pub async fn probe_live() -> anyhow::Result<LiveDocsReport> {
    let docs_urls = official_docs_urls()
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    Ok(LiveDocsReport {
        npm: npm_claude_code_metadata(),
        docs: official_docs_probe(&docs_urls),
        docs_urls,
    })
}

pub fn npm_claude_code_metadata() -> LiveProbe<NpmPackageInfo> {
    npm_package_metadata("@anthropic-ai/claude-code")
}

pub fn npm_package_metadata(package: &str) -> LiveProbe<NpmPackageInfo> {
    let output = match Command::new("npm")
        .args(["view", package, "--json"])
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            return LiveProbe::Unavailable {
                reason: format!("npm unavailable: {error}"),
            };
        }
    };

    if !output.status.success() {
        return LiveProbe::Unavailable {
            reason: format!(
                "npm view failed with status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        };
    }

    let value: Value = match serde_json::from_slice(&output.stdout) {
        Ok(value) => value,
        Err(error) => {
            return LiveProbe::Unavailable {
                reason: format!("npm returned invalid json: {error}"),
            };
        }
    };

    let tags = value.get("dist-tags").and_then(Value::as_object);
    let time = value.get("time").and_then(Value::as_object);
    LiveProbe::Available {
        value: NpmPackageInfo {
            package: package.to_string(),
            latest: tags.and_then(|tags| string_field(tags.get("latest"))),
            stable: tags.and_then(|tags| string_field(tags.get("stable"))),
            next: tags.and_then(|tags| string_field(tags.get("next"))),
            modified: time.and_then(|time| string_field(time.get("modified"))),
        },
    }
}

pub fn official_docs_urls() -> Vec<&'static str> {
    vec![
        "https://code.claude.com/docs/en/cli-reference",
        "https://code.claude.com/docs/en/interactive-mode",
        "https://code.claude.com/docs/en/claude-directory",
    ]
}

fn official_docs_probe(urls: &[String]) -> LiveProbe<DocsProbeInfo> {
    let mut combined = String::new();
    for url in urls {
        let output = match Command::new("curl").args(["-fsSL", url]).output() {
            Ok(output) => output,
            Err(error) => {
                return LiveProbe::Unavailable {
                    reason: format!("curl unavailable: {error}"),
                };
            }
        };
        if !output.status.success() {
            return LiveProbe::Unavailable {
                reason: format!(
                    "curl failed for {url} with status {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            };
        }
        combined.push_str(&String::from_utf8_lossy(&output.stdout));
        combined.push('\n');
    }

    let missing_required_flags = ClaudeCli::required_flags()
        .into_iter()
        .filter_map(|flag| {
            if combined.contains(&flag.name) {
                None
            } else {
                Some(flag.name)
            }
        })
        .collect();
    let removed_flags_still_documented = ClaudeCli::removed_flags()
        .into_iter()
        .filter_map(|flag| {
            if combined.contains(&flag.name) {
                Some(flag.name)
            } else {
                None
            }
        })
        .collect();

    LiveProbe::Available {
        value: DocsProbeInfo {
            checked_urls: urls.to_vec(),
            missing_required_flags,
            removed_flags_still_documented,
        },
    }
}

fn string_field(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(str::to_string)
}
