# GhostMac

Minimal Rust overlay that stays on top and follows every macOS Space.

## Download & Install

1. Download `GhostMac-dmg` from [GitHub Actions](../../actions) → latest run → Artifacts
2. Open the DMG and drag `GhostMac.app` to Applications
3. **Important:** Before first launch, open Terminal and run:
   ```bash
   xattr -cr /Applications/GhostMac.app
   ```
4. Double-click `GhostMac.app` to run

> **Why the Terminal command?** The app is ad-hoc signed (not notarized with Apple). macOS quarantines downloaded apps, and `xattr -cr` removes that quarantine flag.

## Quit the App

Press **Cmd+Q** while the app is focused, or right-click the Dock icon → Quit.

## Build from Source

Requires **macOS** + **Xcode command-line tools**.

```sh
cargo build --release
```

Or just `git push` – GitHub Actions delivers a ready `.dmg` in ~3 min.






