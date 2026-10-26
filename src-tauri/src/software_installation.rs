use sha2::{Digest, Sha256};
use std::{io::Write, time::Duration};

pub(crate) struct VerifiedDownloadPolicy<'a> {
    pub id: &'a str,
    pub url: &'a str,
    pub sha256: &'a str,
    pub max_bytes: usize,
    pub supported_os: &'a [&'a str],
    pub supported_arches: &'a [&'a str],
}

fn audit(id: &str, event: &str) {
    crate::logs::append_ai_os_diagnostic(&format!("software-installation id={id} event={event}"));
}

fn validate_policy(policy: &VerifiedDownloadPolicy<'_>, user_approved: bool) -> Result<(), String> {
    if !user_approved {
        return Err("Software installation requires explicit user approval.".to_string());
    }
    if !policy.url.starts_with("https://") {
        return Err("Software installation sources must use HTTPS.".to_string());
    }
    if policy.sha256.len() != 64
        || !policy
            .sha256
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err("Software installation requires a pinned SHA-256 digest.".to_string());
    }
    if !policy.supported_os.contains(&std::env::consts::OS)
        || !policy.supported_arches.contains(&std::env::consts::ARCH)
    {
        return Err(format!(
            "{} is not available for {}/{}.",
            policy.id,
            std::env::consts::OS,
            std::env::consts::ARCH,
        ));
    }
    Ok(())
}

pub(crate) fn download_verified(
    policy: &VerifiedDownloadPolicy<'_>,
    user_approved: bool,
) -> Result<tempfile::NamedTempFile, String> {
    validate_policy(policy, user_approved)?;
    audit(policy.id, "download-started");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent("AI-OS/1.0.1-beta.6 Software Installation Service")
        .build()
        .map_err(|_| "AI-OS could not initialize the software download client.".to_string())?;
    let response = client
        .get(policy.url)
        .send()
        .map_err(|_| "AI-OS could not download the approved software artifact.".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Software download returned HTTP {}.",
            response.status()
        ));
    }
    let bytes = response
        .bytes()
        .map_err(|_| "AI-OS could not read the downloaded software artifact.".to_string())?;
    if bytes.is_empty() || bytes.len() > policy.max_bytes {
        return Err("The downloaded software artifact had an unexpected size.".to_string());
    }
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if !actual.eq_ignore_ascii_case(policy.sha256) {
        audit(policy.id, "integrity-rejected");
        return Err(
            "The downloaded software artifact failed SHA-256 verification. Nothing was executed."
                .to_string(),
        );
    }
    let mut file = tempfile::NamedTempFile::new()
        .map_err(|_| "AI-OS could not create a temporary software artifact.".to_string())?;
    file.write_all(&bytes)
        .map_err(|_| "AI-OS could not save the verified software artifact.".to_string())?;
    audit(policy.id, "integrity-verified");
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::{validate_policy, VerifiedDownloadPolicy};

    fn policy<'a>() -> VerifiedDownloadPolicy<'a> {
        VerifiedDownloadPolicy {
            id: "test",
            url: "https://example.com/release",
            sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            max_bytes: 1024,
            supported_os: &[std::env::consts::OS],
            supported_arches: &[std::env::consts::ARCH],
        }
    }

    #[test]
    fn requires_explicit_approval_and_verified_https_policy() {
        assert!(validate_policy(&policy(), false).is_err());
        assert!(validate_policy(&policy(), true).is_ok());
        let mut invalid = policy();
        invalid.url = "http://example.com/release";
        assert!(validate_policy(&invalid, true).is_err());
        invalid.url = "https://example.com/release";
        invalid.sha256 = "unknown";
        assert!(validate_policy(&invalid, true).is_err());
    }
}
