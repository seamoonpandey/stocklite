# StockLite

StockLite is an offline-first desktop inventory application built with Tauri 2, Rust, Svelte, TypeScript, and SQLite. It stores data on the local machine and does not require a server or internet connection.

## Run locally

Requirements:

- Node.js 20+
- Rust stable
- Tauri 2 platform prerequisites

```bash
npm install
npm run tauri dev
```

Use `npm run dev` for a browser preview. The browser preview uses demo data because SQLite commands are available only inside the Tauri desktop runtime.

## Build

```bash
npm run check
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build
```

Installers are written to `src-tauri/target/release/bundle/`.

## Features

- Product creation with atomic opening stock
- Product editing and archival without deleting history
- Purchases, sales, and signed stock adjustments
- Sale and adjustment validation inside SQLite transactions
- Backdated movement dates with separate recorded timestamps
- Searchable transaction history and date/type filters
- Opening, purchase, sale, adjustment, and closing stock reports
- Consistent SQLite backups and validated restores
- Four core workspace screens plus reports and data settings

Stock quantities are integers in version one. Movement history is append-only; corrections are made with adjustments rather than editing or deleting transactions.

## Data and backups

The active database is stored in the operating system application-data directory. Open **Settings & data** in the app to see the exact path, create a `.db` backup, or restore a compatible backup. Always create a backup before restoring another database.
