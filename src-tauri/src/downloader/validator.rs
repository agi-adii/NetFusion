use std::path::{Path, PathBuf};
use url::Url;

/// Validates an HTTP/HTTPS download URL and guards against malicious schemes or invalid hosts.
pub fn validate_download_url(raw_url: &str) -> Result<Url, String> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return Err("Download URL cannot be empty".into());
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err("URL contains invalid control characters".into());
    }

    let parsed = Url::parse(trimmed).map_err(|e| format!("Invalid URL format: {e}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(format!(
                "Unsupported or unsafe URL scheme '{other}://'. Only HTTP and HTTPS are permitted."
            ))
        }
    }

    let Some(host) = parsed.host_str() else {
        return Err("URL is missing a valid hostname".into());
    };
    if host.is_empty() {
        return Err("URL hostname cannot be empty".into());
    }

    Ok(parsed)
}

/// Sanitizes a filename for safe storage on Windows NTFS/ReFS filesystems.
pub fn sanitize_windows_filename(raw_name: &str) -> String {
    let base = raw_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(raw_name)
        .trim();

    let mut cleaned: String = base
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
                || c.is_control()
            {
                '_'
            } else {
                c
            }
        })
        .collect();

    cleaned = cleaned.trim_matches(['.', ' ']).to_string();
    if cleaned.is_empty() {
        cleaned = "download.bin".to_string();
    }

    // Guard Windows reserved device names (CON, PRN, AUX, NUL, COM1..9, LPT1..9)
    let stem_upper = cleaned
        .split('.')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    if matches!(
        stem_upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        cleaned = format!("_{cleaned}");
    }

    if cleaned.len() > 200 {
        cleaned.truncate(200);
    }
    cleaned
}

/// Extracts suggested filename from HTTP `Content-Disposition` header or URL path.
pub fn extract_filename(url: &Url, content_disposition: Option<&str>) -> String {
    if let Some(cd) = content_disposition {
        // Check filename*=UTF-8''... or filename="..."
        if let Some(idx) = cd.to_ascii_lowercase().find("filename*=") {
            let rest = &cd[idx + 10..];
            let token = rest.split(';').next().unwrap_or(rest).trim();
            let stripped = token
                .strip_prefix("UTF-8''")
                .or_else(|| token.strip_prefix("utf-8''"))
                .unwrap_or(token)
                .trim_matches('"');
            let decoded = percent_decode_simple(stripped);
            if !decoded.is_empty() {
                return sanitize_windows_filename(&decoded);
            }
        }
        if let Some(idx) = cd.to_ascii_lowercase().find("filename=") {
            let rest = &cd[idx + 9..];
            let token = rest
                .split(';')
                .next()
                .unwrap_or(rest)
                .trim()
                .trim_matches(['"', '\'']);
            if !token.is_empty() {
                return sanitize_windows_filename(token);
            }
        }
    }

    if let Some(segments) = url.path_segments() {
        if let Some(last) = segments.rev().find(|s| !s.is_empty()) {
            let decoded = percent_decode_simple(last);
            if !decoded.is_empty() {
                return sanitize_windows_filename(&decoded);
            }
        }
    }

    format!(
        "{}_download.bin",
        url.host_str().unwrap_or("netfusion").replace('.', "_")
    )
}

/// Deduplicates target filepath (`file.iso` -> `file (1).iso`) if file already exists on disk.
pub fn unique_destination_path(dir: &Path, file_name: &str) -> (String, PathBuf) {
    let safe_name = sanitize_windows_filename(file_name);
    let candidate = dir.join(&safe_name);
    if !candidate.exists() && !PathBuf::from(format!("{}.nfdl", candidate.display())).exists() {
        return (safe_name, candidate);
    }

    let (stem, ext) = match safe_name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (safe_name.clone(), String::new()),
    };

    for n in 1..10_000 {
        let numbered = format!("{stem} ({n}){ext}");
        let p = dir.join(&numbered);
        if !p.exists() && !PathBuf::from(format!("{}.nfdl", p.display())).exists() {
            return (numbered, p);
        }
    }
    (safe_name, candidate)
}

fn percent_decode_simple(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex_str) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(val) = u8::from_str_radix(hex_str, 16) {
                    out.push(val);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_validation_and_filename_sanitization() {
        assert!(validate_download_url("https://example.com/archive.zip").is_ok());
        assert!(validate_download_url("file:///C:/Windows/System32/cmd.exe").is_err());
        assert!(validate_download_url("javascript:alert(1)").is_err());

        assert_eq!(sanitize_windows_filename("..//evil:file?.exe"), "evil_file_.exe");
        assert_eq!(sanitize_windows_filename("CON.txt"), "_CON.txt");
    }
}

