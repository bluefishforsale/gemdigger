# windows-exe-template

A starting point for Rust programs that ship as a Windows `.exe`. The program itself is a placeholder that prints a greeting and its version. The point of the repo is everything around it: a pinned toolchain, a build that runs on Windows in CI, tagged releases that publish the `.exe` with a checksum, and an installer that verifies that checksum before anything touches your `PATH`.

The habits come from [claude-signal](https://github.com/bluefishforsale/claude-signal), where every version is pinned, every download is checked, and there's an uninstall script that backs all of it out. Pins and checksums cost very little up front, and they're how you avoid running something you didn't choose.

## Starting a new project from it

Click "Use this template" on GitHub (or `gh repo create NAME --private --template bluefishforsale/windows-exe-template`), then rename `windows-exe-template` in three places: `name` in `Cargo.toml`, the `$Repo` and `$Name` defaults at the top of `install-windows.ps1`, and `$Name` in `uninstall-windows.ps1`. The workflows read the name from `cargo metadata`, so they need no edits. Run `cargo build` once so `Cargo.lock` picks up the new name, then replace `run` in `src/main.rs` with the real program.

After cloning, turn on the pre-commit hook once, which runs rustfmt on staged files so formatting never shows up in review:

```bash
git config core.hooksPath .githooks
```

## Building

You can write and test on macOS or Linux, since `cargo test` and `cargo clippy` run anywhere. The real `.exe` is built on Windows, though: CI uses a `windows-latest` runner with the MSVC toolchain rather than cross-compiling, because that's the build users actually run, and clippy and the tests should see the same `cfg` it does.

`rust-toolchain.toml` pins the compiler (currently 1.96.0), and `Cargo.lock` is committed, so CI, releases, and every clone build with the same inputs. Bump the toolchain in its own commit and read the release notes first.

`.cargo/config.toml` links the C runtime statically. Without that, the `.exe` depends on `vcruntime140.dll` and won't start on a machine that's missing the Visual C++ redistributable, which is a confusing first impression for whoever you hand it to.

## CI

`ci.yml` runs on every push to `main` and every PR. On Windows it runs `cargo fmt -- --check`, `cargo clippy -- -D warnings` (warnings fail the build), `cargo test`, a release build, and then runs the built `.exe` with `--version` to check it starts and reports the version in `Cargo.toml`. A second Windows job parses every `.ps1` and runs `Invoke-ScriptAnalyzer` on them, and a small Linux job runs shellcheck on the git hook.

## Releasing

Bump `version` in `Cargo.toml`, commit, then tag and push:

```bash
git tag v0.2.0
git push origin v0.2.0
```

`release.yml` refuses a tag that doesn't match `Cargo.toml`, since otherwise the `.exe` would report a different version than the release it came from. Then it runs the tests, builds, and publishes a GitHub Release with the `.exe` and a `SHA256SUMS` file.

## Installing on Windows

```powershell
powershell -ExecutionPolicy Bypass -File install-windows.ps1
```

This downloads the latest release (or the one given with `-Version v0.1.0`), checks the `.exe` against `SHA256SUMS`, copies it to `%LOCALAPPDATA%\Programs\windows-exe-template`, and adds that folder to your user `PATH`. It's safe to rerun, and rerunning upgrades in place. No admin rights are needed.

The repo is private, so the installer downloads through the [GitHub CLI](https://cli.github.com/) rather than a plain URL, which would 404 without credentials. Install it with `winget install GitHub.cli` and run `gh auth login` first.

It edits `PATH` through the registry directly instead of `[Environment]::SetEnvironmentVariable`, because that call expands every `%VAR%` already in your `PATH` and saves the result as a plain string, quietly freezing entries like `%USERPROFILE%\bin`.

## Uninstall

```powershell
powershell -ExecutionPolicy Bypass -File uninstall-windows.ps1
```

This removes the install folder and its `PATH` entry, and leaves the rest of your `PATH` alone.

## Caveat

The install and uninstall scripts pass the PowerShell parser and ScriptAnalyzer in CI, and the build and `--version` smoke test run on a real Windows runner, but the scripts haven't been run end to end on a desktop Windows machine yet.
