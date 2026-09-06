use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Passed,
    Failed,
    Missing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportStatus {
    Passed,
    Failed,
    Pending,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EvidenceReport {
    pub status: ReportStatus,
    pub checks: Vec<Check>,
}

impl EvidenceReport {
    pub fn from_checks(checks: Vec<Check>) -> Self {
        let status = if checks.iter().any(|c| c.status == CheckStatus::Failed) {
            ReportStatus::Failed
        } else if checks.iter().any(|c| c.status == CheckStatus::Missing) {
            ReportStatus::Pending
        } else {
            ReportStatus::Passed
        };
        Self { status, checks }
    }

    pub fn exit_code(&self, allow_pending: bool) -> i32 {
        match self.status {
            ReportStatus::Passed => 0,
            ReportStatus::Pending if allow_pending => 0,
            _ => 1,
        }
    }
}
