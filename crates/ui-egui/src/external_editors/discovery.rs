//! Bounded macOS application discovery. Reading bundle metadata never launches an application.
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub name: String,
    pub application: String,
}

#[derive(Clone, Default)]
pub struct Report {
    pub editors: Vec<Candidate>,
    pub unreadable: usize,
    pub limited: bool,
}

/// Recognise known photo suites, or apps explicitly declaring photo documents and an editor role.
/// A TIFF reader alone is insufficient (browsers, office tools and video players also read TIFF).
fn photo_editor(name: &str, info: &Value) -> bool {
    let id = info["CFBundleIdentifier"].as_str().unwrap_or("").to_lowercase();
    let title = name.to_lowercase();
    if title.contains("lightcraft")
        || id.contains("lightcraft")
        || title.contains("installer")
        || title.contains("uninstall")
        || id.ends_with(".helper")
        || info["LSBackgroundOnly"].as_bool() == Some(true)
    {
        return false;
    }
    let known = [
        "affinity photo",
        "affinityphoto",
        "silver efex",
        "silverefex",
        "silver fx",
        "color efex",
        "colorefex",
        "viveza",
        "dfine",
        "sharpener",
        "hdr efex",
        "analog efex",
        "analogefex",
        "hdrefex",
        "perspectiveefex",
        "perspective efex",
        "photocraft",
        "photoshop",
        "photolab",
        "pure raw",
        "pureraw",
        "capture one",
        "captureone",
        "lightroom",
        "nx studio",
        "nxstudio",
        "capture nx",
        "viewnx",
        "digital photo professional",
        "imaging edge",
        "om workspace",
        "phocus",
        "darktable",
        "rawtherapee",
        "raw therapee",
        "art raw",
        "luminar",
        "on1 photo",
        "pixelmator",
        "acorn",
        "gimp",
        "photomator",
        "exposure x",
        "topaz photo",
        "topaz gigapixel",
        "topaz sharpen",
        "topaz denoise",
        "silkypix",
        "raw power",
    ];
    if known.iter().any(|needle| title.contains(needle)) {
        return true;
    }
    info["CFBundleDocumentTypes"].as_array().is_some_and(|types| {
        types.iter().any(|doc| {
            let editor = doc["CFBundleTypeRole"].as_str() == Some("Editor");
            let photo_type = doc["LSItemContentTypes"]
                .as_array()
                .is_some_and(|utis| utis.iter().any(|u| matches!(u.as_str(), Some("public.camera-raw-image" | "public.tiff" | "public.jpeg"))));
            let photography = info["LSApplicationCategoryType"].as_str() == Some("public.app-category.photography");
            editor && photo_type && photography
        })
    })
}

fn is_bundle(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("app"))
}

fn inspect_bundle(path: &Path, read_info: &impl Fn(&Path) -> Option<Value>, seen: &mut BTreeSet<String>, report: &mut Report) {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let Some(application) = canonical.to_str() else {
        report.unreadable += 1;
        return;
    };
    if !seen.insert(application.to_owned()) {
        return;
    }
    let Some(info) = read_info(&path.join("Contents/Info.plist")) else {
        report.unreadable += 1;
        return;
    };
    let fallback = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let name = info["CFBundleDisplayName"].as_str().or_else(|| info["CFBundleName"].as_str()).unwrap_or(fallback);
    let name = if photo_editor(fallback, &info) { fallback } else { name };
    if !photo_editor(name, &info) {
        return;
    }
    report.editors.push(Candidate { name: name.to_owned(), application: application.to_owned() });
}

/// No symlinks, no bundle internals, maximum depth 8 and 30,000 entries per scan.
/// Missing roots are normal. Permission failures and truncation are reported to the user.
fn scan(roots: Vec<PathBuf>, read_info: impl Fn(&Path) -> Option<Value>) -> Report {
    let mut report = Report::default();
    let mut seen = BTreeSet::new();
    let mut pending: Vec<_> = roots.into_iter().rev().map(|p| (p, 0usize)).collect();
    let mut visited = 0usize;
    while let Some((dir, depth)) = pending.pop() {
        if !dir.exists() {
            continue;
        }
        if std::fs::symlink_metadata(&dir).is_ok_and(|m| m.file_type().is_symlink()) {
            continue;
        }
        if is_bundle(&dir) {
            inspect_bundle(&dir, &read_info, &mut seen, &mut report);
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => {
                report.unreadable += 1;
                continue;
            }
        };
        for entry in entries {
            visited += 1;
            if visited > 30_000 {
                report.limited = true;
                break;
            }
            let Ok(entry) = entry else {
                report.unreadable += 1;
                continue;
            };
            let Ok(kind) = entry.file_type() else {
                report.unreadable += 1;
                continue;
            };
            if !kind.is_dir() || kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            if is_bundle(&path) {
                inspect_bundle(&path, &read_info, &mut seen, &mut report);
            } else if depth < 8 && pending.len() < 30_000 {
                pending.push((path, depth + 1));
            } else {
                report.limited = true;
            }
        }
        if visited > 30_000 {
            break;
        }
    }
    report.editors.sort_by_key(|e| e.name.to_lowercase());
    report
}

pub fn discover() -> Result<Report, String> {
    if !cfg!(target_os = "macos") {
        return Err("Automatic detection is currently available on macOS only.".into());
    }
    let mut roots = vec![PathBuf::from("/Applications"), PathBuf::from("/System/Applications")];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(&home).join("Applications"));
        roots.push(PathBuf::from(home).join("Library/Application Support"));
    }
    // Spotlight includes indexed apps in nonstandard folders and on mounted disks.
    // A missing/disabled index falls back to the bounded folder search above.
    let mut index_limited = false;
    if let Ok(output) = std::process::Command::new("/usr/bin/mdfind").args(["-0", "kMDItemContentType == 'com.apple.application-bundle'"]).output()
        && output.status.success()
    {
        if output.stdout.len() <= 4_194_304 {
            roots.extend(
                String::from_utf8_lossy(&output.stdout).split('\0').take(10_000).map(PathBuf::from).filter(|p| p.is_absolute() && is_bundle(p)),
            );
        } else {
            index_limited = true;
        }
    }
    roots.push(PathBuf::from("/Library/Application Support"));
    roots.sort_by_key(|p| {
        if is_bundle(p) {
            0
        } else if p.ends_with("Applications") {
            1
        } else {
            2
        }
    });
    let mut report = scan(roots, |path| {
        // plutil supports XML and binary plists; no shell interpolation or executable launch.
        if std::fs::metadata(path).ok()?.len() > 1_048_576 {
            return None;
        }
        let output = std::process::Command::new("/usr/bin/plutil").args(["-convert", "json", "-o", "-"]).arg(path).output().ok()?;
        if !output.status.success() || output.stdout.len() > 4_194_304 {
            return None;
        }
        serde_json::from_slice(&output.stdout).ok()
    });
    report.limited |= index_limited;
    Ok(report)
}

/// Merge by canonical application path, preserving existing ids and TIFF preferences.
pub fn merge(settings: &mut crate::state::AppSettings, candidates: &[Candidate]) -> Result<usize, String> {
    let mut next = settings.clone();
    let mut added = 0;
    for candidate in candidates {
        let duplicate = next.external_editors.iter().any(|editor| {
            let path = std::fs::canonicalize(&editor.application).ok();
            let application = path.as_ref().and_then(|p| p.to_str()).unwrap_or(&editor.application);
            application == candidate.application
                || (!editor.application.contains('/') && editor.application.trim_end_matches(".app").eq_ignore_ascii_case(&candidate.name))
        });
        if duplicate {
            continue;
        }
        super::command(&mut next, &serde_json::json!({"action":"add", "name":candidate.name, "application":candidate.application}))?;
        added += 1;
    }
    *settings = next;
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn recognises_legacy_nik_affinity_and_photo_metadata_not_generic_tiff_readers() {
        assert!(photo_editor("Silver Efex Pro 2", &json!({})));
        assert!(photo_editor("Affinity Photo 2", &json!({})));
        let mut metadata = json!({"LSApplicationCategoryType":"public.app-category.photography", "CFBundleDocumentTypes":[{"CFBundleTypeRole":"Editor", "LSItemContentTypes":["public.tiff"]}]});
        assert!(photo_editor("New photo app", &metadata));
        metadata["LSApplicationCategoryType"] = json!("public.app-category.video");
        assert!(!photo_editor("Video player", &metadata));
        assert!(!photo_editor("LightCraft EdZ Lab", &metadata));
    }
    #[test]
    fn rescanning_preserves_preferences_and_deleted_entries_can_be_detected_again() {
        let mut settings = crate::state::AppSettings::default();
        let candidates = vec![Candidate { name: "Affinity Photo 2".into(), application: "/Applications/Affinity Photo 2.app".into() }];
        assert_eq!(merge(&mut settings, &candidates).unwrap(), 1);
        let id = settings.external_editors[0].id;
        settings.external_editors[0].file.bit_depth = 8;
        assert_eq!(merge(&mut settings, &candidates).unwrap(), 0);
        assert_eq!(settings.external_editors[0].file.bit_depth, 8);
        super::super::command(&mut settings, &json!({"action":"remove", "id":id})).unwrap();
        assert_eq!(merge(&mut settings, &candidates).unwrap(), 1);
        assert_ne!(settings.external_editors[0].id, id);
    }
    #[test]
    fn nested_bundles_are_found_once_without_entering_application_contents() {
        let root = std::env::temp_dir().join(format!("lightcraft-discovery-{}", std::process::id()));
        let nik = root.join("Nik Collection/Silver Efex Pro 2.app");
        std::fs::create_dir_all(nik.join("Contents/Nested.app")).unwrap();
        std::fs::create_dir_all(root.join("Browser.app")).unwrap();
        let report = scan(vec![root.clone(), root.clone(), nik.clone()], |_| Some(json!({})));
        assert_eq!(report.editors.len(), 1);
        assert!(report.editors[0].application.ends_with("Silver Efex Pro 2.app"));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_merge_is_atomic_and_name_only_configuration_is_not_duplicated() {
        let mut settings = crate::state::AppSettings::default();
        super::super::command(&mut settings, &json!({"action":"add", "name":"Custom label", "application":"Affinity Photo 2"})).unwrap();
        let candidates = [Candidate { name: "Affinity Photo 2".into(), application: "/Applications/Affinity Photo 2.app".into() }];
        assert_eq!(merge(&mut settings, &candidates).unwrap(), 0);
        let before = settings.clone();
        let bad = [
            Candidate { name: "Silver Efex".into(), application: "/Applications/Silver Efex.app".into() },
            Candidate { name: "Bad\nName".into(), application: "/Applications/Bad.app".into() },
        ];
        assert!(merge(&mut settings, &bad).is_err());
        assert_eq!(settings, before);
    }
    #[cfg(unix)]
    #[test]
    fn symlinked_directories_are_not_followed() {
        let root = std::env::temp_dir().join(format!("lightcraft-discovery-links-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::os::unix::fs::symlink(&root, root.join("loop")).unwrap();
        let report = scan(vec![root.clone()], |_| panic!("no bundles expected"));
        assert!(report.editors.is_empty());
        assert!(!report.limited);
        std::fs::remove_dir_all(root).unwrap();
    }
}
