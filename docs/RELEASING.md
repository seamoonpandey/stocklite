# StockLite releasing

## Create a release

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
git tag v1.0.3
git push origin v1.0.3
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

## Data preservation

The application identifier is part of the operating-system application-data
path and must not be changed after release. StockLite uses the stable
`com.stocklite.app` identifier. The startup migration also imports databases
created by the temporary `com.stocklite.desktop` identifier used by an earlier
build. Installer upgrades replace application binaries only; they do not
replace the database.
