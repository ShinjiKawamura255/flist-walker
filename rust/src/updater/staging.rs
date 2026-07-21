use crate::update_security;
use crate::updater::{current_version_string, StagedUpdatePaths, UpdateCandidate};
use anyhow::{anyhow, bail, Context, Result};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[cfg(all(unix, not(target_os = "macos")))]
use std::os::unix::fs::PermissionsExt;

const UPDATE_TEMP_DIR_RANDOM_BYTES: usize = 16;
const UPDATE_TEMP_DIR_MAX_ATTEMPTS: usize = 32;
const MAX_REDIRECTS: usize = 3;

#[derive(Clone, Copy, Debug)]
pub(super) struct StagingLimits {
    pub(super) release_json_bytes: u64,
    manifest_bytes: u64,
    signature_bytes: u64,
    binary_bytes: u64,
    sidecar_bytes: u64,
    connect_timeout: Duration,
    inactivity_timeout: Duration,
    request_timeout: Duration,
    total_timeout: Duration,
}

impl Default for StagingLimits {
    fn default() -> Self {
        Self {
            release_json_bytes: 2 * 1024 * 1024,
            manifest_bytes: 1024 * 1024,
            signature_bytes: 64 * 1024,
            binary_bytes: 512 * 1024 * 1024,
            sidecar_bytes: 16 * 1024 * 1024,
            connect_timeout: Duration::from_secs(10),
            inactivity_timeout: Duration::from_secs(30),
            request_timeout: Duration::from_secs(5 * 60),
            total_timeout: Duration::from_secs(10 * 60),
        }
    }
}

struct FetchResponse {
    content_length: Option<u64>,
    reader: Box<dyn Read + Send + Sync>,
}

pub(super) fn stage_update_assets(candidate: &UpdateCandidate) -> Result<StagedUpdatePaths> {
    let limits = StagingLimits::default();
    let allow_loopback = update_feed_override_is_set();
    let mut fetch =
        |url: &str, timeout: Duration| fetch_with_ureq(url, allow_loopback, timeout, limits);
    stage_update_assets_with(
        candidate,
        &std::env::temp_dir(),
        limits,
        allow_loopback,
        &mut fetch,
        |message, signature| {
            update_security::verify_embedded_signature(message, signature)
                .context("failed to verify update checksum signature")
        },
    )
}

fn stage_update_assets_with<F, V>(
    candidate: &UpdateCandidate,
    base_dir: &Path,
    limits: StagingLimits,
    allow_loopback: bool,
    fetch: &mut F,
    verify_signature: V,
) -> Result<StagedUpdatePaths>
where
    F: FnMut(&str, Duration) -> Result<FetchResponse>,
    V: Fn(&[u8], &[u8]) -> Result<()>,
{
    let deadline = Instant::now()
        .checked_add(limits.total_timeout)
        .context("update staging deadline overflow")?;
    let temp_dir = unique_update_temp_dir_in(base_dir, random_update_suffix)?;
    let paths = StagedUpdatePaths::new(temp_dir, candidate);

    let manifest = fetch_small_body(
        &candidate.checksum_url,
        limits.manifest_bytes,
        deadline,
        limits.request_timeout,
        allow_loopback,
        fetch,
    )?;
    let signature = fetch_small_body(
        &candidate.checksum_signature_url,
        limits.signature_bytes,
        deadline,
        limits.request_timeout,
        allow_loopback,
        fetch,
    )?;
    write_new_staged_bytes(&paths.checksum_path, &manifest)?;
    write_new_staged_bytes(&paths.signature_path, &signature)?;
    verify_signature(&manifest, &signature)?;

    let checksums = super::manifest::parse_sha256sums_bytes(&manifest)?;
    let required = [
        (
            candidate.asset_name.as_str(),
            candidate.asset_url.as_str(),
            paths.staged_path.as_path(),
            limits.binary_bytes,
        ),
        (
            candidate.readme_asset_name.as_str(),
            candidate.readme_asset_url.as_str(),
            paths.staged_readme_path.as_path(),
            limits.sidecar_bytes,
        ),
        (
            candidate.license_asset_name.as_str(),
            candidate.license_asset_url.as_str(),
            paths.staged_license_path.as_path(),
            limits.sidecar_bytes,
        ),
        (
            candidate.notices_asset_name.as_str(),
            candidate.notices_asset_url.as_str(),
            paths.staged_notices_path.as_path(),
            limits.sidecar_bytes,
        ),
    ];
    let verified_required = required
        .into_iter()
        .map(|(name, url, path, byte_limit)| {
            let expected = checksums
                .get(name)
                .with_context(|| format!("missing checksum for {name}"))?
                .clone();
            Ok((name, url, path, byte_limit, expected))
        })
        .collect::<Result<Vec<_>>>()?;
    for (name, url, path, byte_limit, expected) in verified_required {
        download_verified_asset(
            name,
            url,
            path,
            &expected,
            byte_limit,
            deadline,
            limits.request_timeout,
            allow_loopback,
            fetch,
        )?;
    }
    Ok(paths)
}

fn random_update_suffix() -> [u8; UPDATE_TEMP_DIR_RANDOM_BYTES] {
    let mut bytes = [0u8; UPDATE_TEMP_DIR_RANDOM_BYTES];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

fn update_feed_override_is_set() -> bool {
    std::env::var("FLISTWALKER_UPDATE_FEED_URL")
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
}

fn remaining_request_timeout(deadline: Instant, request_timeout: Duration) -> Result<Duration> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .context("update staging total deadline exceeded")?;
    Ok(remaining.min(request_timeout))
}

fn fetch_small_body<F>(
    url: &str,
    byte_limit: u64,
    deadline: Instant,
    request_timeout: Duration,
    allow_loopback: bool,
    fetch: &mut F,
) -> Result<Vec<u8>>
where
    F: FnMut(&str, Duration) -> Result<FetchResponse>,
{
    validate_update_url(url, allow_loopback)?;
    let timeout = remaining_request_timeout(deadline, request_timeout)?;
    let mut response = fetch(url, timeout)?;
    reject_oversized_content_length(response.content_length, byte_limit, url)?;
    let mut body = Vec::new();
    copy_bounded_and_hash(&mut response.reader, &mut body, byte_limit, deadline)
        .with_context(|| format!("failed to read update body {url}"))?;
    Ok(body)
}

#[allow(clippy::too_many_arguments)]
fn download_verified_asset<F>(
    asset_name: &str,
    url: &str,
    out: &Path,
    expected_digest: &str,
    byte_limit: u64,
    deadline: Instant,
    request_timeout: Duration,
    allow_loopback: bool,
    fetch: &mut F,
) -> Result<()>
where
    F: FnMut(&str, Duration) -> Result<FetchResponse>,
{
    validate_update_url(url, allow_loopback)?;
    let timeout = remaining_request_timeout(deadline, request_timeout)?;
    let mut response = fetch(url, timeout)?;
    reject_oversized_content_length(response.content_length, byte_limit, asset_name)?;
    let mut file = open_new_staged_file(out)?;
    let actual = copy_bounded_and_hash(&mut response.reader, &mut file, byte_limit, deadline)
        .with_context(|| format!("failed to write {}", out.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync {}", out.display()))?;
    if !actual.eq_ignore_ascii_case(expected_digest) {
        bail!("checksum mismatch for {asset_name}: expected {expected_digest}, got {actual}");
    }
    Ok(())
}

fn reject_oversized_content_length(
    content_length: Option<u64>,
    byte_limit: u64,
    label: &str,
) -> Result<()> {
    if content_length.is_some_and(|length| length > byte_limit) {
        bail!("update body {label} exceeds {byte_limit} byte limit");
    }
    Ok(())
}

fn copy_bounded_and_hash(
    reader: &mut dyn Read,
    writer: &mut dyn Write,
    byte_limit: u64,
    deadline: Instant,
) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        if Instant::now() >= deadline {
            bail!("update staging total deadline exceeded");
        }
        let read = reader
            .read(&mut buffer)
            .context("failed to read update body")?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .context("update body byte count overflow")?;
        if total > byte_limit {
            bail!("update body exceeds {byte_limit} byte limit");
        }
        writer
            .write_all(&buffer[..read])
            .context("failed to write update body")?;
        hasher.update(&buffer[..read]);
    }
    writer.flush().context("failed to flush update body")?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn write_new_staged_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = open_new_staged_file(path)?;
    file.write_all(bytes)
        .with_context(|| format!("failed to write {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync {}", path.display()))
}

pub(super) fn make_staged_binary_executable(staged_path: &Path) -> Result<()> {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut perms = fs::metadata(staged_path)
            .with_context(|| format!("failed to read metadata {}", staged_path.display()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(staged_path, perms)
            .with_context(|| format!("failed to chmod {}", staged_path.display()))?;
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        let _ = staged_path;
    }
    Ok(())
}

#[cfg(test)]
fn unique_update_temp_dir() -> Result<PathBuf> {
    let mut rng = OsRng;
    unique_update_temp_dir_in(&std::env::temp_dir(), || {
        let mut bytes = [0u8; UPDATE_TEMP_DIR_RANDOM_BYTES];
        rng.fill_bytes(&mut bytes);
        bytes
    })
}

#[cfg(test)]
pub(super) fn test_unique_update_temp_dir() -> Result<PathBuf> {
    unique_update_temp_dir()
}

fn unique_update_temp_dir_in(
    base_dir: &Path,
    mut random_bytes: impl FnMut() -> [u8; UPDATE_TEMP_DIR_RANDOM_BYTES],
) -> Result<PathBuf> {
    for _ in 0..UPDATE_TEMP_DIR_MAX_ATTEMPTS {
        let suffix = hex_bytes(&random_bytes());
        let dir = base_dir.join(format!("flistwalker-update-{suffix}"));
        match fs::create_dir(&dir) {
            Ok(()) => {
                if let Err(err) = set_private_dir_permissions(&dir) {
                    let _ = fs::remove_dir(&dir);
                    return Err(err);
                }
                return Ok(dir);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => {
                return Err(err).with_context(|| format!("failed to create {}", dir.display()));
            }
        }
    }
    bail!(
        "failed to create unique update temp directory after {} attempts",
        UPDATE_TEMP_DIR_MAX_ATTEMPTS
    )
}

#[cfg(all(unix, not(target_os = "macos")))]
fn set_private_dir_permissions(path: &Path) -> Result<()> {
    let mut perms = fs::metadata(path)
        .with_context(|| format!("failed to read metadata {}", path.display()))?
        .permissions();
    perms.set_mode(0o700);
    fs::set_permissions(path, perms).with_context(|| format!("failed to chmod {}", path.display()))
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
fn set_private_dir_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

pub(super) fn open_new_staged_file(path: &Path) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("failed to create new staged file {}", path.display()))
}

#[cfg(any(test, not(target_os = "macos")))]
pub(super) fn write_new_staged_file(path: &Path, contents: &str) -> Result<()> {
    use std::io::Write as _;
    let mut file = open_new_staged_file(path)?;
    file.write_all(contents.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))
}

pub(super) fn fetch_release_json(url: &str, allow_loopback: bool) -> Result<Vec<u8>> {
    let limits = StagingLimits::default();
    let deadline = Instant::now()
        .checked_add(limits.total_timeout)
        .context("update check deadline overflow")?;
    let mut fetch =
        |target: &str, timeout: Duration| fetch_with_ureq(target, allow_loopback, timeout, limits);
    fetch_small_body(
        url,
        limits.release_json_bytes,
        deadline,
        limits.request_timeout,
        allow_loopback,
        &mut fetch,
    )
}

fn validate_update_url(url: &str, allow_loopback: bool) -> Result<()> {
    let request = ureq::get(url);
    let parsed = request
        .request_url()
        .map_err(|err| anyhow!("invalid update URL: {err}"))?;
    let scheme = parsed.scheme();
    let host = parsed.host().to_ascii_lowercase();
    let loopback = matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1" | "[::1]");
    if allow_loopback && loopback && scheme == "http" {
        return Ok(());
    }
    let trusted_host = host == "api.github.com"
        || host == "github.com"
        || host.ends_with(".githubusercontent.com");
    if scheme != "https" || !trusted_host {
        bail!("update URL must use an approved HTTPS origin");
    }
    Ok(())
}

fn fetch_with_ureq(
    url: &str,
    allow_loopback: bool,
    timeout: Duration,
    limits: StagingLimits,
) -> Result<FetchResponse> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .context("update request deadline overflow")?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(limits.connect_timeout)
        .timeout_read(limits.inactivity_timeout)
        .redirects(0)
        .build();
    let mut current = url.to_string();
    for redirect_count in 0..=MAX_REDIRECTS {
        validate_update_url(&current, allow_loopback)?;
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .context("update request deadline exceeded")?;
        let request = agent.get(&current).timeout(remaining).set(
            "User-Agent",
            &format!("flistwalker/{}", current_version_string()),
        );
        let request_url = request
            .request_url()
            .map_err(|err| anyhow!("invalid update URL: {err}"))?;
        let response = match request.call() {
            Ok(response) => response,
            Err(ureq::Error::Status(_, response)) => response,
            Err(err) => return Err(anyhow!("failed to download {current}: {err}")),
        };
        if matches!(response.status(), 301 | 302 | 303 | 307 | 308) {
            if redirect_count == MAX_REDIRECTS {
                bail!("update redirect limit exceeded");
            }
            let location = response
                .header("Location")
                .context("update redirect missing Location header")?;
            current = request_url
                .as_url()
                .join(location)
                .context("invalid update redirect URL")?
                .to_string();
            continue;
        }
        if !(200..300).contains(&response.status()) {
            bail!("failed to download {current}: HTTP {}", response.status());
        }
        let content_length = response
            .header("Content-Length")
            .map(|value| {
                value
                    .parse::<u64>()
                    .context("invalid update Content-Length header")
            })
            .transpose()?;
        return Ok(FetchResponse {
            content_length,
            reader: response.into_reader(),
        });
    }
    unreachable!("redirect loop always returns or fails")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::updater::UpdateSupport;
    use sha2::{Digest, Sha256};
    use std::collections::HashMap;
    use std::io::Cursor;

    #[test]
    fn unique_update_temp_dir_in_retries_collisions_and_never_reuses_existing_dir() {
        let base = unique_update_temp_dir().expect("base temp dir");
        let first = [0x11; UPDATE_TEMP_DIR_RANDOM_BYTES];
        let second = [0x22; UPDATE_TEMP_DIR_RANDOM_BYTES];
        let first_path = base.join(format!("flistwalker-update-{}", hex_bytes(&first)));
        let second_path = base.join(format!("flistwalker-update-{}", hex_bytes(&second)));
        fs::create_dir(&first_path).expect("precreate collision dir");
        let mut attempts = [first, second].into_iter();

        let actual = unique_update_temp_dir_in(&base, || attempts.next().expect("random bytes"))
            .expect("create after collision");

        assert_eq!(actual, second_path);
        assert!(first_path.is_dir(), "existing collision dir must remain");
        assert!(second_path.is_dir(), "second attempt should create dir");

        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let mode = fs::metadata(&second_path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o700);
        }

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn unique_update_temp_dir_in_fails_after_repeated_collisions() {
        let base = unique_update_temp_dir().expect("base temp dir");
        let repeated = [0x33; UPDATE_TEMP_DIR_RANDOM_BYTES];
        let collision_path = base.join(format!("flistwalker-update-{}", hex_bytes(&repeated)));
        fs::create_dir(&collision_path).expect("precreate collision dir");

        let err = unique_update_temp_dir_in(&base, || repeated)
            .expect_err("repeated collisions should fail");

        assert!(
            err.to_string().contains("after 32 attempts"),
            "unexpected error: {err}"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn open_new_staged_file_refuses_existing_file() {
        let dir = unique_update_temp_dir().expect("temp dir");
        let path = dir.join("existing.bin");
        fs::write(&path, b"existing").expect("write existing file");

        let err = open_new_staged_file(&path).expect_err("existing file must not be overwritten");

        assert!(
            err.to_string().contains("failed to create new staged file"),
            "unexpected error: {err}"
        );
        assert_eq!(fs::read(&path).expect("read existing"), b"existing");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_new_staged_file_refuses_existing_file() {
        let dir = unique_update_temp_dir().expect("temp dir");
        let path = dir.join("apply-update.sh");
        fs::write(&path, "existing").expect("write existing script");

        let err = write_new_staged_file(&path, "replacement")
            .expect_err("existing helper script must not be overwritten");

        assert!(
            err.to_string().contains("failed to create new staged file"),
            "unexpected error: {err}"
        );
        assert_eq!(
            fs::read_to_string(&path).expect("read existing"),
            "existing"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    fn tc157_candidate() -> UpdateCandidate {
        UpdateCandidate {
            current_version: "1.0.0".to_string(),
            target_version: "1.0.1".to_string(),
            release_url: "http://127.0.0.1/release".to_string(),
            asset_name: "FlistWalker-1.0.1-linux-x86_64".to_string(),
            asset_url: "http://127.0.0.1/binary".to_string(),
            readme_asset_name: "FlistWalker-1.0.1-linux-x86_64.README.txt".to_string(),
            readme_asset_url: "http://127.0.0.1/readme".to_string(),
            license_asset_name: "FlistWalker-1.0.1-linux-x86_64.LICENSE.txt".to_string(),
            license_asset_url: "http://127.0.0.1/license".to_string(),
            notices_asset_name: "FlistWalker-1.0.1-linux-x86_64.THIRD_PARTY_NOTICES.txt"
                .to_string(),
            notices_asset_url: "http://127.0.0.1/notices".to_string(),
            checksum_url: "http://127.0.0.1/SHA256SUMS".to_string(),
            checksum_signature_url: "http://127.0.0.1/SHA256SUMS.sig".to_string(),
            support: UpdateSupport::Auto,
        }
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn tc157_bodies(candidate: &UpdateCandidate) -> HashMap<String, Vec<u8>> {
        let assets = [
            (
                &candidate.asset_name,
                &candidate.asset_url,
                b"binary".as_slice(),
            ),
            (
                &candidate.readme_asset_name,
                &candidate.readme_asset_url,
                b"readme".as_slice(),
            ),
            (
                &candidate.license_asset_name,
                &candidate.license_asset_url,
                b"license".as_slice(),
            ),
            (
                &candidate.notices_asset_name,
                &candidate.notices_asset_url,
                b"notices".as_slice(),
            ),
        ];
        let manifest = assets
            .iter()
            .map(|(name, _, body)| format!("{}  {name}\n", sha256_hex(body)))
            .collect::<String>();
        let mut bodies = assets
            .into_iter()
            .map(|(_, url, body)| (url.clone(), body.to_vec()))
            .collect::<HashMap<_, _>>();
        bodies.insert(candidate.checksum_url.clone(), manifest.into_bytes());
        bodies.insert(
            candidate.checksum_signature_url.clone(),
            b"signature".to_vec(),
        );
        bodies
    }

    #[test]
    fn tc157_streaming_copy_enforces_decoded_byte_limit() {
        let mut input = Cursor::new(vec![0x5a; 9]);
        let mut output = Vec::new();
        let err = copy_bounded_and_hash(
            &mut input,
            &mut output,
            8,
            std::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .expect_err("decoded body over the limit must fail");

        assert!(
            err.to_string().contains("exceeds"),
            "unexpected error: {err}"
        );
        assert!(
            output.len() <= 8,
            "writer must not receive bytes beyond the limit"
        );
    }

    #[test]
    fn tc157_limits_and_deadline_are_fail_closed() {
        let limits = StagingLimits::default();
        assert_eq!(limits.release_json_bytes, 2 * 1024 * 1024);
        assert_eq!(limits.manifest_bytes, 1024 * 1024);
        assert_eq!(limits.signature_bytes, 64 * 1024);
        assert_eq!(limits.binary_bytes, 512 * 1024 * 1024);
        assert_eq!(limits.sidecar_bytes, 16 * 1024 * 1024);
        assert_eq!(limits.connect_timeout, Duration::from_secs(10));
        assert_eq!(limits.inactivity_timeout, Duration::from_secs(30));
        assert_eq!(limits.request_timeout, Duration::from_secs(5 * 60));
        assert_eq!(limits.total_timeout, Duration::from_secs(10 * 60));
        assert!(reject_oversized_content_length(Some(9), 8, "fixture").is_err());
        assert!(reject_oversized_content_length(None, 8, "fixture").is_ok());

        let mut input = Cursor::new(b"body");
        let mut output = Vec::new();
        let err = copy_bounded_and_hash(
            &mut input,
            &mut output,
            8,
            Instant::now() - Duration::from_millis(1),
        )
        .expect_err("expired deadline must fail before reading");
        assert!(err.to_string().contains("deadline"));
        assert!(output.is_empty());
    }

    #[test]
    fn tc157_url_policy_rejects_untrusted_or_insecure_origins() {
        assert!(validate_update_url("https://api.github.com/repos/x", false).is_ok());
        assert!(validate_update_url("https://objects.githubusercontent.com/x", false).is_ok());
        assert!(validate_update_url("http://api.github.com/repos/x", false).is_err());
        assert!(validate_update_url("https://example.invalid/asset", false).is_err());
        assert!(validate_update_url("http://127.0.0.1:8080/asset", true).is_ok());
        assert!(validate_update_url("http://127.0.0.1:8080/asset", false).is_err());
    }

    #[test]
    fn tc157_staging_fetches_trust_material_before_assets() {
        let candidate = tc157_candidate();
        let bodies = tc157_bodies(&candidate);
        let base = unique_update_temp_dir().expect("base");
        let mut order = Vec::new();
        let mut fetch = |url: &str, _timeout: std::time::Duration| {
            order.push(url.to_string());
            let body = bodies.get(url).expect("scripted body").clone();
            Ok(FetchResponse {
                content_length: Some(body.len() as u64),
                reader: Box::new(Cursor::new(body)),
            })
        };

        let staged = stage_update_assets_with(
            &candidate,
            &base,
            StagingLimits::default(),
            true,
            &mut fetch,
            |_, _| Ok(()),
        )
        .expect("staging");

        assert_eq!(
            &order[..2],
            [
                candidate.checksum_url.clone(),
                candidate.checksum_signature_url.clone()
            ]
        );
        assert_eq!(order[2], candidate.asset_url);
        drop(staged);
        assert_eq!(fs::read_dir(&base).expect("read base").count(), 0);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn tc157_signature_failure_prevents_asset_requests_and_cleans_staging() {
        let candidate = tc157_candidate();
        let bodies = tc157_bodies(&candidate);
        let base = unique_update_temp_dir().expect("base");
        let mut order = Vec::new();
        let mut fetch = |url: &str, _timeout: Duration| {
            order.push(url.to_string());
            let body = bodies.get(url).expect("scripted body").clone();
            Ok(FetchResponse {
                content_length: Some(body.len() as u64),
                reader: Box::new(Cursor::new(body)),
            })
        };

        let err = stage_update_assets_with(
            &candidate,
            &base,
            StagingLimits::default(),
            true,
            &mut fetch,
            |_, _| bail!("invalid signature"),
        )
        .expect_err("signature failure must stop staging");

        assert!(err.to_string().contains("invalid signature"));
        assert_eq!(
            order,
            [
                candidate.checksum_url.clone(),
                candidate.checksum_signature_url.clone()
            ]
        );
        assert_eq!(fs::read_dir(&base).expect("read base").count(), 0);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn tc157_all_required_digests_are_validated_before_any_asset_request() {
        let candidate = tc157_candidate();
        let mut bodies = tc157_bodies(&candidate);
        bodies.insert(
            candidate.checksum_url.clone(),
            format!("{}  {}\n", sha256_hex(b"binary"), candidate.asset_name).into_bytes(),
        );
        let base = unique_update_temp_dir().expect("base");
        let mut order = Vec::new();
        let mut fetch = |url: &str, _timeout: Duration| {
            order.push(url.to_string());
            let body = bodies.get(url).expect("scripted body").clone();
            Ok(FetchResponse {
                content_length: Some(body.len() as u64),
                reader: Box::new(Cursor::new(body)),
            })
        };

        let err = stage_update_assets_with(
            &candidate,
            &base,
            StagingLimits::default(),
            true,
            &mut fetch,
            |_, _| Ok(()),
        )
        .expect_err("missing required digest must fail before assets");

        assert!(err.to_string().contains("missing checksum"));
        assert_eq!(
            order,
            [
                candidate.checksum_url.clone(),
                candidate.checksum_signature_url.clone()
            ]
        );
        assert_eq!(fs::read_dir(&base).expect("read base").count(), 0);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn tc157_failed_staging_cleans_only_owned_partial_directory() {
        let candidate = tc157_candidate();
        let mut bodies = tc157_bodies(&candidate);
        bodies.insert(candidate.asset_url.clone(), b"tampered".to_vec());
        let base = unique_update_temp_dir().expect("base");
        let unrelated = base.join("unrelated");
        fs::write(&unrelated, b"keep").expect("write unrelated");
        let mut fetch = |url: &str, _timeout: std::time::Duration| {
            let body = bodies.get(url).expect("scripted body").clone();
            Ok(FetchResponse {
                content_length: None,
                reader: Box::new(Cursor::new(body)),
            })
        };

        let err = stage_update_assets_with(
            &candidate,
            &base,
            StagingLimits::default(),
            true,
            &mut fetch,
            |_, _| Ok(()),
        )
        .expect_err("checksum mismatch must fail");

        assert!(err.to_string().contains("checksum mismatch"));
        assert_eq!(fs::read(&unrelated).expect("read unrelated"), b"keep");
        assert_eq!(fs::read_dir(&base).expect("read base").count(), 1);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn tc157_partial_reader_failure_removes_owned_partial_file() {
        struct FailingReader {
            first: Option<Vec<u8>>,
        }

        impl Read for FailingReader {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if let Some(bytes) = self.first.take() {
                    let count = bytes.len().min(buffer.len());
                    buffer[..count].copy_from_slice(&bytes[..count]);
                    return Ok(count);
                }
                Err(std::io::Error::other("injected reader failure"))
            }
        }

        let candidate = tc157_candidate();
        let bodies = tc157_bodies(&candidate);
        let base = unique_update_temp_dir().expect("base");
        let mut fetch = |url: &str, _timeout: Duration| {
            let body = bodies.get(url).expect("scripted body").clone();
            let reader: Box<dyn Read + Send + Sync> = if url == candidate.asset_url {
                Box::new(FailingReader {
                    first: Some(body[..3].to_vec()),
                })
            } else {
                Box::new(Cursor::new(body))
            };
            Ok(FetchResponse {
                content_length: None,
                reader,
            })
        };

        let err = stage_update_assets_with(
            &candidate,
            &base,
            StagingLimits::default(),
            true,
            &mut fetch,
            |_, _| Ok(()),
        )
        .expect_err("partial reader failure must fail staging");

        assert!(err.to_string().contains("failed to write"));
        assert_eq!(fs::read_dir(&base).expect("read base").count(), 0);
        let _ = fs::remove_dir_all(&base);
    }
}
