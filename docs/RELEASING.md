# StockLite releasing

## Create the v1 release

The repository workflow builds installers on native GitHub-hosted runners:

- macOS: `.dmg`
- Windows: `.exe` and `.msi`
- Linux: `.deb` and `.rpm`

After validation, create and push a version tag:

```bash
git checkout main
git pull --ff-only
npm run check
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
git tag v1.0.0
git push origin v1.0.0
```

The tag starts `.github/workflows/release.yml`. It builds all three operating
system variants in parallel, then publishes one GitHub Release with the
installer files attached. A workflow can also be started manually from the
GitHub Actions tab for validation without publishing a release.

## End-user distribution

Users should be sent to:

<https://github.com/seamoonpandey/stocklite/releases/latest>

Do not ask end users to install Node.js, Rust, Git, or run terminal commands.
They only need to download and open the installer for their operating system.
