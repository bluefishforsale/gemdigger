# GemDigger: notes for agents

Why this file exists: the README is for players and stays short. Build, release and signing details live here.

## Checks

`cargo fmt -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`. CI runs all three on Windows and macOS. `main` is protected: changes land by PR, and the `build`, `macos`, `scripts` and `hooks` checks must pass.

## Versioning

Versions are `x.y.z`, set in `Cargo.toml` and tagged `vx.y.z`.

- **x, major:** bumping it means the y series has landed on all of its core principles. It's stable, every planned feature has landed, and its known bugs are fixed. A major bump requires a written roadmap and spec first.
- **y, gameplay:** a significant gameplay change within the current x paradigm, such as new mechanics, items, monsters or zones. Reset z to 0.
- **z, fixes:** bug fixes for the current y release. Also use z for changes that don't alter gameplay, such as packaging, signing, platform or graphics-backend work.

When a change does both, the larger bump wins. While x is 0, the game is still finding its core principles.

## Every y release

A y release changes gameplay, so before tagging:

1. Update the README's Features list to match what the game now does.
2. Re-capture the screenshots and clip, then copy them into `media/`:

   ```bash
   cargo build --release
   ./target/release/gemdigger --capture target/capture   # opens a window for about 25 s
   for f in shop surface digging deep bomb ion outpost base; do sips -Z 960 target/capture/$f.png --out media/$f.png; done
   ffmpeg -y -framerate 15 -i target/capture/frames/%05d.png -vf "scale=960:-2:flags=lanczos,format=yuv420p" -c:v libx264 -crf 26 -an -movflags +faststart media/gameplay.mp4
   # NOTE: -framerate 15 must match CLIP_EVERY in capture.rs, or the clip plays at the wrong speed.
   rm -rf target/webp && mkdir -p target/webp && ffmpeg -y -framerate 15 -i target/capture/frames/%05d.png -vf "fps=8,scale=480:-2:flags=lanczos" target/webp/%05d.png
   img2webp -loop 0 -lossy -q 30 -m 4 -d 125 target/webp/*.png -o media/gameplay.webp   # about 6 MB
   ```

   The scenes are scripted in `src/capture.rs`; add one when a new feature deserves a shot.

## Playtest

`./target/release/gemdigger --playtest <dir>` plays the real game with an autopilot through every feature (shop, digging, ping, laser, rockets, bombs, monster fights, relics, bases, teleport, death and respawn, save round trip), with no audio and no real save, printing PASS or FAIL per step and saving a screenshot after each. Starting money, one bomb, some cargo and the test monsters are granted, and shop purchases call `apply` directly. Run it before tagging, and look at the screenshots, not only the PASS lines.

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
