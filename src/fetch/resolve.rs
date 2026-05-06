use std::path::{Path, PathBuf};

use super::url::ParsedUrl;
use super::FetchError;

pub fn resolve_relative_url(base: &str, relative: &str) -> Result<String, FetchError> {
    if let Some(base_path) = base.strip_prefix("file://") {
        return resolve_relative_file_url(base_path, relative);
    }

    if relative.starts_with("http://") || relative.starts_with("https://") {
        return Ok(relative.to_string());
    }

    let base_parsed = ParsedUrl::parse(base)?;

    if relative.starts_with("//") {
        return Ok(format!(
            "{}{}",
            base_parsed.scheme_prefix(),
            relative.trim_start_matches("//")
        ));
    }

    if relative.starts_with('/') {
        return Ok(format!(
            "{}{}{}",
            base_parsed.scheme_prefix(),
            base_parsed.authority(),
            relative
        ));
    }

    let base_dir = match base_parsed.path_and_query.rsplit_once('/') {
        Some((prefix, _)) if !prefix.is_empty() => prefix,
        _ => "",
    };

    Ok(format!(
        "{}{}{}/{}",
        base_parsed.scheme_prefix(),
        base_parsed.authority(),
        base_dir,
        relative
    ))
}

fn resolve_relative_file_url(base_path: &str, relative: &str) -> Result<String, FetchError> {
    if relative.starts_with("file://") {
        return Ok(relative.to_string());
    }

    let base = Path::new(base_path);
    let resolved = if relative.starts_with('/') {
        PathBuf::from(relative)
    } else {
        let parent = if base.is_dir() {
            base
        } else {
            base.parent().unwrap_or_else(|| Path::new("/"))
        };
        parent.join(relative)
    };

    let normalized = normalize_path(resolved);
    let absolute = if normalized.is_absolute() {
        normalized
    } else {
        std::env::current_dir()
            .map_err(FetchError::Io)?
            .join(normalized)
    };

    Ok(format!("file://{}", absolute.display()))
}

fn normalize_path(path: PathBuf) -> PathBuf {
    use std::path::Component;

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}
