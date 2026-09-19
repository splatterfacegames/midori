// Ported evidence-verifier code; keep the upstream structure over local lint
// style so future syncs against the PowerShell verifier stay diffable.
#![allow(
    clippy::collapsible_if,
    clippy::single_match,
    clippy::type_complexity,
    clippy::chunks_exact_to_as_chunks,
    clippy::no_effect_replace
)]

pub mod full;
mod profile_notes;
mod report;

pub use full::{FullEvidenceOptions, verify_engine_evidence};
pub use profile_notes::{verify_profile_notes, verify_profile_notes_text};
pub use report::{Check, CheckStatus, EvidenceReport, ReportStatus};
