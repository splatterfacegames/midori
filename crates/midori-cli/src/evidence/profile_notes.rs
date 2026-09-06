use super::report::{Check, CheckStatus, EvidenceReport};
use regex::Regex;
use std::sync::LazyLock;

const TERMS: &[&str] = &[
    "Unity",
    "Unreal",
    "Unity Editor version",
    "Unreal Editor version",
    "mobile",
    "console",
    "LOD",
    "Frame Debugger",
    "RenderDoc",
    "instanced",
    "scale",
    "density",
    "cull",
    "material slot",
    "wind",
    "strict verifier",
    "forest_floor_unity_import_report.json",
    "forest_floor_unreal_editor_report.json",
    "unity_forest_floor_import.png",
    "unity_forest_floor_density.png",
    "unreal_forest_floor_import.png",
    "unreal_forest_floor_foliage_settings.png",
    "detailPrototypesCreated",
    "detailPrototypesGeneratedFromGlb",
    "detailPrototypeFailures",
    "detailPrototypeFallbackErrors",
    "scatterBinaryRecordsRead",
    "scatterChunkReports",
    "foliage_type_count",
    "foliage_type_assets",
    "3500",
    "7000",
];
const SECTIONS: &[&str] = &[
    "## Evidence Artifacts",
    "## Unity",
    "## Unreal",
    "## Verdict",
];
static MARKERS: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    let bounded = |pattern: &str| {
        Regex::new(&format!(
            r"(?i)(^|[^\p{{L}}\p{{N}}_])(?:{pattern})($|[^\p{{L}}\p{{N}}_])"
        ))
        .expect("constant profile pattern")
    };
    vec![
        ("TODO", bounded("TODO")),
        ("TBD", bounded("TBD")),
        ("placeholder", bounded("placeholder")),
        ("pending", bounded("pending")),
        ("not run", bounded(r"not[\s\x1c-\x1f]+run")),
        ("not captured", bounded(r"not[\s\x1c-\x1f]+captured")),
        (
            "missing evidence",
            bounded(r"missing[\s\x1c-\x1f]+(?:report|screenshot|notes?|evidence|artifact)s?"),
        ),
        (
            "fill marker",
            Regex::new(r"(?i)\[(?:fill|replace|todo)[^\]\n]*\]").expect("constant fill pattern"),
        ),
    ]
});

fn finding(name: &str, ok: bool, pass: String, fail: String) -> Check {
    Check {
        name: name.into(),
        status: if ok {
            CheckStatus::Passed
        } else {
            CheckStatus::Failed
        },
        detail: if ok { pass } else { fail },
    }
}

fn python_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

pub fn verify_profile_notes_text(label: &str, text: &str) -> EvidenceReport {
    let text = text
        .strip_prefix('\u{feff}')
        .unwrap_or(text)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let stripped = text.trim_matches(python_space);
    let lower = stripped.to_lowercase();
    // Python IGNORECASE additionally matches these characters against ASCII I/S/K.
    let marker_text: String = text
        .chars()
        .map(|c| match c {
            '\u{130}' | '\u{131}' => 'i',
            '\u{17f}' => 's',
            '\u{212a}' => 'k',
            _ => c,
        })
        .collect();
    let forbidden: Vec<&str> = MARKERS
        .iter()
        .filter_map(|(label, re)| re.is_match(&marker_text).then_some(*label))
        .collect();
    let missing_sections: Vec<&str> = SECTIONS
        .iter()
        .copied()
        .filter(|s| !lower.contains(&s.to_lowercase()))
        .collect();
    let missing_terms: Vec<&str> = TERMS
        .iter()
        .copied()
        .filter(|s| !lower.contains(&s.to_lowercase()))
        .collect();
    let mut checks = vec![
        Check {
            name: "profile.notes.file".into(),
            status: CheckStatus::Passed,
            detail: format!("{label} loaded"),
        },
        finding(
            "profile.notes.length",
            stripped.chars().count() >= 1000,
            format!("{label} is substantial enough for profile evidence"),
            format!("{label} is too short to be credible profile evidence"),
        ),
        finding(
            "profile.notes.unresolved_markers",
            forbidden.is_empty(),
            format!("{label} contains no unresolved capture markers"),
            format!(
                "{label} contains unresolved markers: {}",
                forbidden.join(", ")
            ),
        ),
        finding(
            "profile.notes.sections",
            missing_sections.is_empty(),
            format!("{label} contains required evidence sections"),
            format!(
                "{label} is missing sections: {}",
                missing_sections.join(", ")
            ),
        ),
        finding(
            "profile.notes.required_terms",
            missing_terms.is_empty(),
            format!("{label} contains required engine evidence terms"),
            format!(
                "{label} is missing required terms: {}",
                missing_terms.join(", ")
            ),
        ),
    ];
    if checks.iter().all(|c| c.status == CheckStatus::Passed) {
        checks.push(Check {
            name: "profile.notes".into(),
            status: CheckStatus::Passed,
            detail: format!("{label} contains Unity/Unreal profiling evidence"),
        });
    }
    EvidenceReport::from_checks(checks)
}
