//! Asks GitHub whether a newer release exists and, if the player agrees,
//! swaps the installed game for it, so nobody has to come back to the
//! download page for every fix.

use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use anyhow::{Context, anyhow, bail};
use sha2::{Digest, Sha256};

use crate::log;

const REPO: &str = "bluefishforsale/gemdigger";
/// Where to send a player whose update didn't work.
pub const DOWNLOAD_PAGE: &str = "github.com/bluefishforsale/gemdigger/releases";
// NOTE: the Developer ID team every macOS release is signed by. A download
// signed by anyone else is thrown away, so a changed signing team means
// updating this first and shipping that build the old way.
const TEAM_ID: &str = "6U2BDWRU5F";
const EXE: &str = "gemdigger.exe";
const DMG: &str = "gemdigger-macos.dmg";
const APP: &str = "GemDigger.app";
const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;

/// A release number, `x.y.z`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version([u32; 3]);

impl Version {
    /// Reads `0.4.1` or `v0.4.1`; anything else is not a version.
    pub fn parse(s: &str) -> Option<Version> {
        let mut parts = s.strip_prefix('v').unwrap_or(s).split('.');
        let mut next = || parts.next()?.parse().ok();
        let v = Version([next()?, next()?, next()?]);
        parts.next().is_none().then_some(v)
    }

    /// The version this build was compiled as.
    pub fn current() -> Version {
        Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Version([0, 0, 0]))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [x, y, z] = self.0;
        write!(f, "{x}.{y}.{z}")
    }
}

/// A copy of the game this code knows how to replace.
#[derive(Clone, Debug)]
enum Install {
    /// The Windows exe, wherever it was put.
    Exe(PathBuf),
    /// The macOS app bundle the running binary sits inside.
    Bundle(PathBuf),
}

impl Install {
    /// `None` for a copy that isn't an installed release, such as a `cargo run` build.
    fn find() -> Option<Install> {
        let exe = std::env::current_exe().ok()?;
        if cfg!(windows) {
            return Some(Install::Exe(exe));
        }
        if !cfg!(target_os = "macos") {
            return None;
        }
        let bundle = exe.ancestors().nth(3)?;
        (bundle.extension()? == "app").then(|| Install::Bundle(bundle.to_path_buf()))
    }

    fn path(&self) -> &Path {
        match self {
            Install::Exe(p) | Install::Bundle(p) => p,
        }
    }
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent(env!("CARGO_PKG_NAME"))
        .build()
        .into()
}

/// The newest published release.
fn latest() -> anyhow::Result<Version> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let body = agent(Duration::from_secs(15))
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .with_context(|| format!("unable to ask {url} for the latest release"))?
        .body_mut()
        .read_to_string()
        .with_context(|| format!("unable to read the answer from {url}"))?;
    let release: serde_json::Value =
        serde_json::from_str(&body).with_context(|| format!("{url} didn't answer in JSON"))?;
    let tag = release
        .get("tag_name")
        .and_then(|t| t.as_str())
        .ok_or_else(|| anyhow!("{url} named no release tag"))?;
    Version::parse(tag).ok_or_else(|| anyhow!("release tag \"{tag}\" is not a version"))
}

fn download(version: Version, file: &str) -> anyhow::Result<Vec<u8>> {
    let url = format!("https://github.com/{REPO}/releases/download/v{version}/{file}");
    agent(Duration::from_secs(600))
        .get(&url)
        .call()
        .with_context(|| format!("unable to download {url}"))?
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD)
        .read_to_vec()
        .with_context(|| format!("unable to finish downloading {url}"))
}

/// The checksum a release's `SHA256SUMS` lists for one of its files.
fn listed_sum<'a>(sums: &'a str, file: &str) -> Option<&'a str> {
    sums.lines().find_map(|line| {
        let (sum, name) = line.split_once("  ")?;
        (name == file && sum.len() == 64).then_some(sum)
    })
}

/// Downloads a release file and refuses it unless it matches the release's checksum.
fn fetch(version: Version, file: &str) -> anyhow::Result<Vec<u8>> {
    let sums =
        String::from_utf8(download(version, "SHA256SUMS")?).context("SHA256SUMS is not text")?;
    let expected =
        listed_sum(&sums, file).ok_or_else(|| anyhow!("SHA256SUMS has no entry for {file}"))?;
    let bytes = download(version, file)?;
    let actual: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if actual != expected {
        bail!("checksum mismatch for {file}: expected {expected}, got {actual}");
    }
    Ok(bytes)
}

/// Where a copy waits beside the real one: `.new-gemdigger.exe` for `gemdigger.exe`.
///
/// NOTE: the tag goes in front so the extension survives. Windows picks how to
/// read a file's signature from its extension and calls a `.new` file unsigned.
fn beside(path: &Path, tag: &str) -> PathBuf {
    let mut name = OsString::from(format!(".{tag}-"));
    name.push(path.file_name().unwrap_or_default());
    path.with_file_name(name)
}

fn remove(path: &Path) -> std::io::Result<()> {
    let gone = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    match gone {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Puts `staged` where `current` is, leaving `current` beside it as the old copy.
///
/// NOTE: renames only, never a copy over the top. Windows lets a running exe
/// be renamed but not overwritten, and a rename can't leave half a game behind.
fn swap(current: &Path, staged: &Path) -> anyhow::Result<()> {
    let old = beside(current, "old");
    remove(&old).with_context(|| format!("unable to clear \"{}\"", old.display()))?;
    std::fs::rename(current, &old)
        .with_context(|| format!("unable to move \"{}\" aside", current.display()))?;
    if let Err(e) = std::fs::rename(staged, current) {
        let _ = std::fs::rename(&old, current);
        return Err(e).with_context(|| {
            format!(
                "unable to move \"{}\" into place; kept the old one",
                staged.display()
            )
        });
    }
    Ok(())
}

/// Keeps a helper program from flashing a console window over the game.
#[cfg(windows)]
fn windowless(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn windowless(_: &mut Command) {}

fn finish(mut command: Command) -> anyhow::Result<()> {
    windowless(&mut command);
    let program = command.get_program().to_string_lossy().into_owned();
    let out = command
        .output()
        .with_context(|| format!("unable to run {program}"))?;
    if !out.status.success() {
        bail!(
            "{program} failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

fn run(program: &str, args: &[&std::ffi::OsStr]) -> anyhow::Result<()> {
    let mut command = Command::new(program);
    command.args(args);
    finish(command)
}

/// Refuses a Windows exe unless it carries a valid Authenticode signature
/// from the same publisher as `running`.
///
/// NOTE: the checksum only proves the download matches the release page. This
/// is what stops a tampered release from running. Comparing against the
/// running copy keeps the publisher's name out of the source, and means an
/// unsigned build can't update itself at all.
fn check_authenticode(staged: &Path, running: &Path) -> anyhow::Result<()> {
    // The paths travel as environment variables so nothing in them is ever
    // read as PowerShell.
    const SCRIPT: &str = "$ErrorActionPreference = 'Stop'; \
        $new = Get-AuthenticodeSignature -LiteralPath $env:GEMDIGGER_STAGED; \
        $old = Get-AuthenticodeSignature -LiteralPath $env:GEMDIGGER_RUNNING; \
        if ($new.Status -ne 'Valid') { throw \"the download's signature is $($new.Status)\" }; \
        if ($old.Status -ne 'Valid') { throw \"this copy's signature is $($old.Status)\" }; \
        if ($new.SignerCertificate.Subject -ne $old.SignerCertificate.Subject) { \
            throw \"the download is signed by $($new.SignerCertificate.Subject)\" }";
    let mut command = Command::new("powershell");
    command
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("GEMDIGGER_STAGED", staged)
        .env("GEMDIGGER_RUNNING", running);
    finish(command)
}

/// Refuses an app that isn't ours and notarized, whatever its checksum says.
///
/// NOTE: the checksum only proves the download matches the release page. This
/// is what stops a tampered release from running: Apple's signature, from our team.
fn check_signature(app: &Path) -> anyhow::Result<()> {
    let ours = format!("-R=anchor apple generic and certificate leaf[subject.OU] = \"{TEAM_ID}\"");
    run(
        "codesign",
        &[
            "--verify".as_ref(),
            "--deep".as_ref(),
            "--strict".as_ref(),
            ours.as_ref(),
            app.as_ref(),
        ],
    )?;
    run(
        "spctl",
        &[
            "--assess".as_ref(),
            "--type".as_ref(),
            "execute".as_ref(),
            app.as_ref(),
        ],
    )
}

/// Copies the app out of a release disk image to `staged`.
fn unpack_dmg(dmg: &[u8], staged: &Path) -> anyhow::Result<()> {
    let tmp = std::env::temp_dir().join(format!("gemdigger-update-{}", std::process::id()));
    let mount = tmp.join("mount");
    std::fs::create_dir_all(&mount)
        .with_context(|| format!("unable to create \"{}\"", mount.display()))?;
    let image = tmp.join(DMG);
    let copied = std::fs::write(&image, dmg)
        .with_context(|| format!("unable to write \"{}\"", image.display()))
        .and_then(|()| {
            run(
                "hdiutil",
                &[
                    "attach".as_ref(),
                    "-nobrowse".as_ref(),
                    "-readonly".as_ref(),
                    "-noautoopen".as_ref(),
                    "-mountpoint".as_ref(),
                    mount.as_ref(),
                    image.as_ref(),
                ],
            )
        })
        .and_then(|()| {
            let copied = run("ditto", &[mount.join(APP).as_ref(), staged.as_ref()]);
            let _ = run("hdiutil", &["detach".as_ref(), mount.as_ref()]);
            copied
        });
    let _ = std::fs::remove_dir_all(&tmp);
    copied
}

/// Downloads `version`, checks it, and puts it where the running copy is.
fn install(target: &Install, version: Version) -> anyhow::Result<()> {
    let staged = beside(target.path(), "new");
    remove(&staged).with_context(|| format!("unable to clear \"{}\"", staged.display()))?;
    let ready = match target {
        Install::Exe(running) => fetch(version, EXE)
            .and_then(|bytes| {
                std::fs::write(&staged, bytes)
                    .with_context(|| format!("unable to write \"{}\"", staged.display()))
            })
            .and_then(|()| check_authenticode(&staged, running)),
        Install::Bundle(_) => fetch(version, DMG)
            .and_then(|dmg| unpack_dmg(&dmg, &staged))
            .and_then(|()| check_signature(&staged)),
    };
    let swapped = ready.and_then(|()| swap(target.path(), &staged));
    if swapped.is_err() {
        let _ = remove(&staged);
        return swapped;
    }
    // Windows won't delete an exe that's still running; the next launch does.
    let _ = remove(&beside(target.path(), "old"));
    swapped
}

fn launch(target: &Install) -> anyhow::Result<()> {
    let mut command = match target {
        Install::Exe(exe) => Command::new(exe),
        Install::Bundle(app) => {
            let mut open = Command::new("open");
            open.arg("-n").arg(app);
            open
        }
    };
    command
        .spawn()
        .with_context(|| format!("unable to start \"{}\"", target.path().display()))?;
    Ok(())
}

/// What happened to an update the player agreed to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The new version is in place; save, call `restart`, and quit.
    Installed,
    Failed,
}

enum State {
    Idle,
    Checking(Receiver<Version>),
    Offered(Version),
    Installing(Version, Receiver<bool>),
}

/// The update check and install, run off the main thread so a slow network
/// never stalls a frame.
pub struct Updater {
    target: Option<Install>,
    state: State,
}

impl Updater {
    /// Clears out what the last update left behind and asks GitHub for the
    /// latest release. Does nothing for a copy it couldn't replace anyway.
    pub fn start() -> Updater {
        let target = Install::find();
        let Some(found) = &target else {
            return Updater {
                target,
                state: State::Idle,
            };
        };
        // The previous version can still be exiting right after an update;
        // if so this fails and the next launch clears it.
        let _ = remove(&beside(found.path(), "old"));
        let (tx, rx) = channel();
        std::thread::spawn(move || match latest() {
            Ok(v) if v > Version::current() => {
                log::line(&format!("update available: {v}"));
                let _ = tx.send(v);
            }
            Ok(_) => {}
            Err(e) => log::line(&format!("update check: {e:#}")),
        });
        Updater {
            target,
            state: State::Checking(rx),
        }
    }

    /// Call every frame. Reports once when an install finishes either way.
    pub fn poll(&mut self) -> Option<Outcome> {
        match &self.state {
            State::Checking(rx) => {
                match rx.try_recv() {
                    Ok(v) => self.state = State::Offered(v),
                    Err(TryRecvError::Disconnected) => self.state = State::Idle,
                    Err(TryRecvError::Empty) => {}
                }
                None
            }
            State::Installing(_, rx) => {
                let done = match rx.try_recv() {
                    Ok(true) => Outcome::Installed,
                    Ok(false) | Err(TryRecvError::Disconnected) => Outcome::Failed,
                    Err(TryRecvError::Empty) => return None,
                };
                self.state = State::Idle;
                Some(done)
            }
            State::Idle | State::Offered(_) => None,
        }
    }

    /// The newer version waiting on the player's answer.
    pub fn offered(&self) -> Option<Version> {
        match self.state {
            State::Offered(v) => Some(v),
            _ => None,
        }
    }

    /// The version being downloaded right now.
    pub fn installing(&self) -> Option<Version> {
        match self.state {
            State::Installing(v, _) => Some(v),
            _ => None,
        }
    }

    /// The player said yes: download and install in the background.
    pub fn accept(&mut self) {
        let (State::Offered(version), Some(target)) = (&self.state, self.target.clone()) else {
            return;
        };
        let version = *version;
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let done = install(&target, version);
            if let Err(e) = &done {
                log::line(&format!("update to {version}: {e:#}"));
            }
            let _ = tx.send(done.is_ok());
        });
        self.state = State::Installing(version, rx);
    }

    /// The player said not now; the next launch asks again.
    pub fn decline(&mut self) {
        self.state = State::Idle;
    }

    /// Starts the freshly installed version. The caller quits right after.
    pub fn restart(&self) {
        let Some(target) = &self.target else {
            return;
        };
        if let Err(e) = launch(target) {
            log::line(&format!("{e:#}"));
        }
    }
}

/// `--update`: check and install from the command line, with no window.
pub fn run_cli() -> anyhow::Result<String> {
    let target = Install::find().ok_or_else(|| anyhow!("this copy isn't an installed release"))?;
    let _ = remove(&beside(target.path(), "old"));
    let current = Version::current();
    let newest = latest()?;
    if newest <= current {
        return Ok(format!("{current} is up to date"));
    }
    install(&target, newest)?;
    Ok(format!("updated {current} to {newest}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gemdigger-update-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 0.10.0 is newer than 0.9.0: versions compare as numbers, not as text.
    #[test]
    fn versions_compare_by_number_not_by_text() {
        let v = |s| Version::parse(s).unwrap();
        assert!(v("0.10.0") > v("0.9.0"));
        assert!(v("v1.0.0") > v("0.99.99"));
        assert_eq!(v("v0.4.1"), v("0.4.1"));
    }

    /// The tag goes into a download URL, so only a plain x.y.z may pass.
    #[test]
    fn a_tag_that_is_not_x_y_z_is_no_version() {
        for bad in [
            "",
            "v1",
            "1.2",
            "1.2.3.4",
            "1.2.x",
            "1.2.3/../x",
            "1.2.-3",
            "nightly",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad:?} parsed as a version");
        }
    }

    /// This build's own version has to parse, or it would update to anything.
    #[test]
    fn the_build_knows_its_own_version() {
        assert_eq!(Version::current().to_string(), env!("CARGO_PKG_VERSION"));
    }

    /// The sum has to be the one for this file, not its neighbour in the list.
    #[test]
    fn the_checksum_is_read_for_the_named_file_only() {
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        let sums = format!("{a}  gemdigger-macos.dmg\n{b}  gemdigger.exe\n");
        assert_eq!(listed_sum(&sums, "gemdigger.exe"), Some(b.as_str()));
        assert_eq!(listed_sum(&sums, "gemdigger-macos.dmg"), Some(a.as_str()));
        assert_eq!(listed_sum(&sums, "digger.exe"), None);
        assert_eq!(listed_sum("abc  gemdigger.exe\n", "gemdigger.exe"), None);
    }

    /// After a swap the new copy is in place and the old one is kept beside it.
    #[test]
    fn a_swap_keeps_the_old_copy_beside_the_new_one() {
        let dir = scratch("swap");
        let current = dir.join("gemdigger.exe");
        let staged = beside(&current, "new");
        std::fs::write(&current, "old").unwrap();
        std::fs::write(&staged, "new").unwrap();
        std::fs::write(beside(&current, "old"), "older").unwrap();
        swap(&current, &staged).unwrap();
        assert_eq!(std::fs::read_to_string(&current).unwrap(), "new");
        assert_eq!(
            std::fs::read_to_string(beside(&current, "old")).unwrap(),
            "old"
        );
        assert!(!staged.exists(), "the staged copy was left behind");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A Mac app is a folder; the same swap has to move it whole.
    #[test]
    fn a_swap_moves_a_whole_app_folder() {
        let dir = scratch("bundle");
        let current = dir.join("GemDigger.app");
        let staged = beside(&current, "new");
        for (app, body) in [(&current, "old"), (&staged, "new")] {
            std::fs::create_dir_all(app.join("Contents")).unwrap();
            std::fs::write(app.join("Contents/Info.plist"), body).unwrap();
        }
        swap(&current, &staged).unwrap();
        let plist = |app: &Path| std::fs::read_to_string(app.join("Contents/Info.plist")).unwrap();
        assert_eq!(plist(&current), "new");
        assert_eq!(plist(&beside(&current, "old")), "old");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// An exe nobody signed must never replace the game, even with a matching checksum.
    #[cfg(windows)]
    #[test]
    fn an_unsigned_exe_is_refused() {
        let dir = scratch("unsigned");
        let unsigned = dir.join("gemdigger.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &unsigned).unwrap();
        let refused = check_authenticode(&beside(&unsigned, "new"), &unsigned);
        assert!(refused.is_err(), "a missing download passed the check");
        std::fs::copy(&unsigned, beside(&unsigned, "new")).unwrap();
        let refused = check_authenticode(&beside(&unsigned, "new"), &unsigned).unwrap_err();
        assert!(
            format!("{refused:#}").contains("NotSigned"),
            "refused for the wrong reason: {refused:#}"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The staged copy keeps its extension, which Windows reads the signature by.
    #[test]
    fn a_staged_copy_keeps_its_extension() {
        let staged = beside(Path::new("games/gemdigger.exe"), "new");
        assert_eq!(staged, Path::new("games/.new-gemdigger.exe"));
    }

    /// If the new copy can't be moved in, the game she had must still be there.
    #[test]
    fn a_failed_swap_puts_the_old_copy_back() {
        let dir = scratch("failed");
        let current = dir.join("gemdigger.exe");
        std::fs::write(&current, "old").unwrap();
        let missing = beside(&current, "new");
        assert!(swap(&current, &missing).is_err());
        assert_eq!(std::fs::read_to_string(&current).unwrap(), "old");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
