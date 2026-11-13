# GemDigger: notes for agents

Why this file exists: the README is for players and stays short. Build, release and signing details live here.

## Checks

`cargo fmt -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`. CI runs all three on Windows and macOS. `main` is protected: changes land by PR, and the `build`, `macos`, `scripts` and `hooks` checks must pass.

## Every release that changes gameplay

Before tagging:

1. Update the README's Features list to match what the game now does.
2. Re-capture the screenshots and clip, then copy them into `media/`:

   ```bash
   cargo build --release
   ./target/release/gemdigger --capture target/capture   # opens a window for about 25 s
   for f in shop surface digging deep bomb; do sips -Z 960 target/capture/$f.png --out media/$f.png; done
   ffmpeg -y -framerate 15 -i target/capture/frames/%05d.png -vf "scale=960:-2:flags=lanczos,format=yuv420p" -c:v libx264 -crf 26 -an -movflags +faststart media/gameplay.mp4
   # NOTE: -framerate 15 must match CLIP_EVERY in capture.rs, or the clip plays at the wrong speed.
   rm -rf target/webp && mkdir -p target/webp && ffmpeg -y -framerate 15 -i target/capture/frames/%05d.png -vf "fps=8,scale=480:-2:flags=lanczos" target/webp/%05d.png
   img2webp -loop 0 -lossy -q 30 -m 4 -d 125 target/webp/*.png -o media/gameplay.webp   # about 6 MB
   ```

   The scenes are scripted in `src/capture.rs`; add one when a new feature deserves a shot.

## Releasing

Bump `version` in `Cargo.toml` and merge, then push a `v*` tag. `release.yml` builds Windows and macOS, signs both, and publishes them with one `SHA256SUMS`. A tag fails before building if any signing input is missing, so nothing unsigned ships.

## Signing

The identities belong to Terrac Skiens and are shared with signal-agent; its `docs/signing-setup.md` covers how they were created. They live in this repo's `release` environment, which only `v*` tags can deploy to:

- secrets: the seven `APPLE_*` values, `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, and a random `KEYCHAIN_PASSWORD`
- variables: `AZURE_SIGNING_ENDPOINT`, `AZURE_SIGNING_ACCOUNT`, `AZURE_CERT_PROFILE`

The `signal-agent-ci` app registration trusts this repo through a federated credential. NOTE: GitHub's token names the repo by numeric IDs, so the subject is `repo:bluefishforsale@3721396/gemdigger@1456130580:environment:release` (credential `gemdigger-release-ids`). A plain `repo:bluefishforsale/gemdigger:...` subject fails Azure login with AADSTS700213.

Builds run in jobs that have no secrets. The signing jobs only unpack the build artifacts outside the workspace, check that they hold exactly the expected files, then sign them. Keep it that way: no build step and no built binary may run in a job that can see the keys.

## Repo settings

The repo is public so anyone can download releases. Only collaborators can open pull requests (`pull_request_creation_policy`). Issues and comments are limited to collaborators by an interaction limit that expires 2027-05-13 and has to be renewed:

```bash
gh api -X PUT repos/bluefishforsale/gemdigger/interaction-limits -f limit=collaborators_only -f expiry=six_months
```
