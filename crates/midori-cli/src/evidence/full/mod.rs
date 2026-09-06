mod common;
mod engines;
mod package;
mod png;

use super::{EvidenceReport, verify_profile_notes};
use common::{JsonLoad, Verifier, load_json};
use engines::{
    check_summary, check_unity_report, check_unreal_dry_run_report, check_unreal_report,
};
use package::check_midori_report;
use png::check_png_artifact;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct FullEvidenceOptions {
    pub validation_root: PathBuf,
    pub midori_report: PathBuf,
    pub summary: PathBuf,
    pub unity_report: PathBuf,
    pub unreal_dry_run_report: PathBuf,
    pub unreal_report: PathBuf,
    pub unity_import_screenshot: PathBuf,
    pub unity_density_screenshot: PathBuf,
    pub unreal_import_screenshot: PathBuf,
    pub unreal_foliage_settings_screenshot: PathBuf,
    pub profile_notes: PathBuf,
}

impl FullEvidenceOptions {
    pub fn from_validation_root(validation_root: PathBuf) -> Self {
        Self {
            midori_report: validation_root.join("forest_floor_midori_validation_report.json"),
            summary: validation_root.join("engine_validation_summary.json"),
            unity_report: validation_root.join("forest_floor_unity_import_report.json"),
            unreal_dry_run_report: validation_root.join("forest_floor_unreal_dry_run_report.json"),
            unreal_report: validation_root.join("forest_floor_unreal_editor_report.json"),
            validation_root,
            unity_import_screenshot: PathBuf::from(
                "docs/validation/screenshots/unity_forest_floor_import.png",
            ),
            unity_density_screenshot: PathBuf::from(
                "docs/validation/screenshots/unity_forest_floor_density.png",
            ),
            unreal_import_screenshot: PathBuf::from(
                "docs/validation/screenshots/unreal_forest_floor_import.png",
            ),
            unreal_foliage_settings_screenshot: PathBuf::from(
                "docs/validation/screenshots/unreal_forest_floor_foliage_settings.png",
            ),
            profile_notes: PathBuf::from("docs/validation/midori-nature-engine-profile-notes.md"),
        }
    }
}

fn load_report(verifier: &mut Verifier, name: &str, path: &Path) -> Option<serde_json::Value> {
    match load_json(path) {
        JsonLoad::Missing => {
            verifier.missing(name, format!("{path:?} is missing"));
            None
        }
        JsonLoad::Invalid(error) => {
            verifier.fail(name, error);
            None
        }
        JsonLoad::Value(value) if value.is_object() => {
            verifier.pass(name, format!("{path:?} loaded"));
            Some(value)
        }
        JsonLoad::Value(_) => {
            verifier.fail(name, format!("{path:?} must contain a JSON object"));
            None
        }
    }
}

fn guarded<T, F>(verifier: &mut Verifier, family: &str, action: F) -> Option<T>
where
    F: FnOnce(&mut Verifier) -> Option<T>,
{
    match catch_unwind(AssertUnwindSafe(|| action(verifier))) {
        Ok(value) => value,
        Err(_) => {
            verifier.fail(
                family,
                "malformed evidence triggered an internal verifier error",
            );
            None
        }
    }
}

fn check_profile_notes(verifier: &mut Verifier, path: &Path) {
    match fs::metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            verifier.extend(verify_profile_notes(path).checks);
        }
        Err(error) => verifier.fail(
            "profile.notes",
            format!("{path:?} cannot be inspected: {error}"),
        ),
        Ok(metadata) if !metadata.is_file() => {
            verifier.fail("profile.notes", format!("{path:?} is not a regular file"));
        }
        Ok(metadata) if metadata.len() == 0 => {
            verifier.extend(verify_profile_notes(path).checks);
        }
        Ok(_) => match catch_unwind(AssertUnwindSafe(|| verify_profile_notes(path))) {
            Ok(report) => verifier.extend(report.checks),
            Err(_) => verifier.fail(
                "profile.notes",
                "malformed profile notes triggered an internal verifier error",
            ),
        },
    }
}

fn check_png_artifact_guarded(verifier: &mut Verifier, name: &str, path: &Path) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        check_png_artifact(verifier, name, path)
    }));
    if result.is_err() {
        verifier.fail(
            name,
            "malformed PNG evidence triggered an internal verifier error",
        );
    }
}

pub fn verify_engine_evidence(options: &FullEvidenceOptions) -> EvidenceReport {
    let mut verifier = Verifier::default();
    let package_dir = options.validation_root.join("forest_floor");

    let manifest = match load_report(&mut verifier, "midori.report", &options.midori_report) {
        Some(report) => guarded(&mut verifier, "midori.report", |verifier| {
            check_midori_report(verifier, &report)
        }),
        None => None,
    };

    if let Some(summary) = load_report(&mut verifier, "summary.report", &options.summary) {
        let manifest_ref = manifest.as_ref();
        let _ = guarded(&mut verifier, "summary.report", |verifier| {
            check_summary(verifier, &summary, &options.validation_root, manifest_ref);
            Some(())
        });
    }

    if let Some(manifest) = manifest.as_ref() {
        if let Some(report) = load_report(
            &mut verifier,
            "unreal_dry_run.report",
            &options.unreal_dry_run_report,
        ) {
            let manifest = manifest.clone();
            let package_dir = package_dir.clone();
            let _ = guarded(&mut verifier, "unreal_dry_run.report", |verifier| {
                check_unreal_dry_run_report(verifier, &report, &manifest, &package_dir);
                Some(())
            });
        }
        if let Some(report) = load_report(&mut verifier, "unity.report", &options.unity_report) {
            let manifest = manifest.clone();
            let package_dir = package_dir.clone();
            let _ = guarded(&mut verifier, "unity.report", |verifier| {
                check_unity_report(verifier, &report, &manifest, &package_dir);
                Some(())
            });
        }
        if let Some(report) = load_report(
            &mut verifier,
            "unreal.editor_report",
            &options.unreal_report,
        ) {
            let manifest = manifest.clone();
            let package_dir = package_dir.clone();
            let _ = guarded(&mut verifier, "unreal.editor_report", |verifier| {
                check_unreal_report(verifier, &report, &manifest, &package_dir);
                Some(())
            });
        }
    }

    check_png_artifact_guarded(
        &mut verifier,
        "unity.import_screenshot",
        &options.unity_import_screenshot,
    );
    check_png_artifact_guarded(
        &mut verifier,
        "unity.density_screenshot",
        &options.unity_density_screenshot,
    );
    check_png_artifact_guarded(
        &mut verifier,
        "unreal.import_screenshot",
        &options.unreal_import_screenshot,
    );
    check_png_artifact_guarded(
        &mut verifier,
        "unreal.foliage_settings_screenshot",
        &options.unreal_foliage_settings_screenshot,
    );

    check_profile_notes(&mut verifier, &options.profile_notes);
    EvidenceReport::from_checks(verifier.into_checks())
}

trait ExtendVerifier {
    fn extend(&mut self, checks: Vec<super::Check>);
}

impl ExtendVerifier for Verifier {
    fn extend(&mut self, checks: Vec<super::Check>) {
        // Verifier's checks remain private to keep mutation of report order
        // centralized. This method is filled by the internal transfer helper.
        for check in checks {
            match check.status {
                super::CheckStatus::Passed => self.pass(check.name, check.detail),
                super::CheckStatus::Failed => self.fail(check.name, check.detail),
                super::CheckStatus::Missing => self.missing(check.name, check.detail),
            }
        }
    }
}
