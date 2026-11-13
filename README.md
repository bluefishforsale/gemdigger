# GemDigger

A cozy-to-creepy digging game. You drive a little drill pod down into the ground, dig out gems, fly back up to sell them at the shop, and spend the money on a better drill, a bigger tank, a roomier cargo hold, a tougher hull and a brighter lamp. The mine goes down forever. Near the surface it's sunny and cheerful, but the deeper you go the darker it gets, the music turns ominous, and things start living in the rock.

It's meant to be gentle. Lives are infinite: when the pod is lost (out of fuel, a bad fall, or something with teeth) it comes back at the shop with your money and upgrades intact. Only the gems you were carrying are lost, and they aren't really lost either, since they stay where you fell as a glowing crate, and an arrow at the edge of the screen points the way back to every one of them. Progress saves itself every 30 seconds, whenever you sell, buy or die, and when you close the window.

## Playing

Arrow keys or WASD drive left and right and dig in the direction you push; up (or space) fires the thruster. You can dig sideways and down, but not up, so plan a way back. At the shop, click a row or press 1 to 0 to sell, refuel, repair, craft, or upgrade. ESC pauses and opens settings (music, sound effects, screen shake, full screen); F11 also toggles full screen, which shows more of the mine rather than blowing the art up.

Monsters show up below about 60 m, and you always get a warning: the ground rumbles and the screen shakes for a couple of seconds before one breaks out of the rock. They only ever move side to side, so the escape is always the same, fly straight up. Small ones nibble the hull, bigger ones bite harder, and the biggest one, the Deep Thing, which only lives far down, takes the pod in one go.

Bombs are the hard-won tool. Coal turns up from about 10 m, Sulfur from 90 m, and Potassium only from 180 m and rarely, and the shop crafts one of each into a bomb. They ride in their own pouch of three rather than the cargo hold, and B drops one (underground only). After a two-and-a-half-second fuse it clears the 3x3 around it, boulders included, scoops what gems and minerals it can into the hold, kills anything short of the Deep Thing (which it only stuns for a few seconds), and hurts the pod too if she hasn't backed off. "Sell gems" never sells minerals; a separate row sells spares for a few dollars.

## Installing on Windows

```powershell
powershell -ExecutionPolicy Bypass -File install-windows.ps1
```

That downloads the latest release through the [GitHub CLI](https://cli.github.com/) (the repo is private, so `winget install GitHub.cli` and `gh auth login` first), checks it against the release's `SHA256SUMS`, and installs it to `%LOCALAPPDATA%\Programs\gemdigger`, on your `PATH`. The save lives at `%APPDATA%\GemDigger\save.json`, and `uninstall-windows.ps1` removes the game but leaves the save alone.

## Installing on macOS

Each release also has `gemdigger-macos.zip`, a universal `GemDigger.app` for Apple Silicon and Intel Macs, signed with a Developer ID and notarized by Apple, so it opens like any other app.

## How it's built

Rust with [macroquad](https://macroquad.rs), forked from `windows-exe-template`, so it inherits the pinned toolchain, Windows CI with a `--version` smoke test, tag-driven releases and the checksummed installer; the template's README has the details on each.

The map is never stored: every tile comes from a hash of the world seed and its position, and the save only records the tiles you've changed, which is what lets it go down forever without the save growing past what you've actually dug. All of the music and sound effects are synthesized from code at startup, five music loops (one per depth band, crossfaded as you descend) plus every effect, so there's no audio to license or ship.

The art comes from an image-model sprite atlas in `art/atlas.png`, sliced into named sprites under `assets/sprites/` by `tools/slice_atlas.py` and baked into the exe at build time. To swap art without rebuilding, drop a PNG with the same name into an `assets/sprites` folder next to the exe; sounds work the same way from `assets/sounds` (`.ogg` or `.wav`, named like `pickup_3` or `music_4`). If the atlas is regenerated, its sprite boxes need re-measuring, and the script's docstring says how.

For testing without touching a real save, point `GEMDIGGER_SAVE` at another file.

## Signing

Every release is signed, and a tag with any signing input missing refuses to build at all rather than ship something unsigned. Windows builds get an Authenticode signature from Azure Artifact Signing; the Mac app gets a Developer ID signature, the hardened runtime, and Apple notarization stapled to it. Both identities are Terrac Skiens's and are shared with signal-agent, whose `docs/signing-setup.md` covers creating them.

The inputs live in this repo's `release` environment, which only `v*` tags can deploy to, so pull requests never see them. It holds the seven `APPLE_*` secrets and `AZURE_TENANT_ID`/`AZURE_CLIENT_ID` (all the same values as signal-agent), a random `KEYCHAIN_PASSWORD` for the throwaway CI keychain, and three variables naming the Azure signing setup: `AZURE_SIGNING_ENDPOINT`, `AZURE_SIGNING_ACCOUNT` and `AZURE_CERT_PROFILE`. The `signal-agent-ci` app registration also needs a second federated credential trusting this repo, subject `repo:bluefishforsale/gemdigger:environment:release`, or Azure login fails from here.

## Checks

`cargo fmt -- --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test`. The tests cover the rules headless: digging, death dropping cargo as a stash while keeping money, stash recovery against a full hold, monsters never moving vertically, the Deep Thing killing a maxed hull, flying up escaping it, save round trips, and the corrupt-save backup.
