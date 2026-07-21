use crate::updater::transaction::{self, TransactionSources};
use crate::updater::VerifiedUpdateBundle;
use anyhow::{bail, Context, Result};
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
pub(crate) const INTERNAL_HELPER_FLAG: &str = "--flistwalker-internal-update-helper";

pub(super) fn spawn_update_helper(
    current_exe: &Path,
    bundle: &mut VerifiedUpdateBundle,
) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let _ = (current_exe, bundle);
        bail!("macOS auto-update is unsupported");
    }
    #[cfg(not(target_os = "macos"))]
    {
        let sources = TransactionSources {
            binary: &bundle.staged_path,
            readme: &bundle.staged_readme_path,
            license: &bundle.staged_license_path,
            notices: &bundle.staged_notices_path,
        };
        let mut prepared = transaction::prepare_transaction(current_exe, sources)?;
        bundle.cleanup_staging()?;
        let start_token = transaction::new_start_token();
        let arguments = helper_arguments(
            prepared.marker_path().as_os_str(),
            OsStr::new(prepared.transaction_id()),
            OsStr::new(&start_token),
        );
        let mut command = Command::new(prepared.helper_path());
        command.args(&arguments).current_dir(prepared.install_dir());
        #[cfg(target_os = "windows")]
        command.creation_flags(CREATE_NO_WINDOW);
        let mut child = command.spawn().with_context(|| {
            format!(
                "failed to spawn updater helper {}",
                prepared.helper_path().display()
            )
        })?;
        if let Err(err) = prepared.register_helper(child.id(), &start_token) {
            stop_unregistered_helper(&mut child);
            return Err(err).context("failed to durably register updater helper");
        }
        if let Err(err) = wait_for_acknowledgement(&prepared, &start_token, &mut child) {
            stop_unregistered_helper(&mut child);
            return Err(err);
        }
        prepared.disarm();
        Ok(())
    }
}

fn helper_arguments(marker: &OsStr, transaction_id: &OsStr, start_token: &OsStr) -> [OsString; 4] {
    [
        INTERNAL_HELPER_FLAG.into(),
        marker.to_os_string(),
        transaction_id.to_os_string(),
        start_token.to_os_string(),
    ]
}

fn wait_for_acknowledgement(
    prepared: &transaction::PreparedTransaction,
    start_token: &str,
    child: &mut Child,
) -> Result<()> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(10))
        .context("helper acknowledgement deadline overflow")?;
    loop {
        if prepared.acknowledgement_matches(start_token) {
            return Ok(());
        }
        if let Some(status) = child
            .try_wait()
            .context("failed to query updater helper status")?
        {
            bail!("updater helper exited before acknowledgement: {status}");
        }
        if Instant::now() >= deadline {
            bail!("timed out waiting for updater helper acknowledgement");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn stop_unregistered_helper(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc159_internal_helper_arguments_are_exact_and_positional() {
        let args = helper_arguments(
            OsStr::new("marker"),
            OsStr::new("00112233445566778899aabbccddeeff"),
            OsStr::new("start-token-0123456789"),
        );

        assert_eq!(
            args,
            [
                OsString::from(INTERNAL_HELPER_FLAG),
                OsString::from("marker"),
                OsString::from("00112233445566778899aabbccddeeff"),
                OsString::from("start-token-0123456789")
            ]
        );
    }
}
