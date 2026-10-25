# StockLite development

## Requirements

- Node.js 22 or newer
- Rust stable
- Tauri 2 prerequisites for the current operating system

Install dependencies and start the desktop app:

```bash
npm ci
npm run tauri dev
```

Run the browser preview with `npm run dev`. It intentionally uses demo data
because SQLite commands run inside Tauri, not in a browser tab.

## Validation

Run the same checks used before release:

```bash
npm run check
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

Build the local operating-system installer:

```bash
npm run tauri build
```

Output is written below `src-tauri/target/release/bundle/`.

## Architecture

- `src/routes/+page.svelte`: workspace screens and form interactions
- `src/lib/api.ts`: typed frontend-to-Tauri command boundary
- `src/lib/types.ts`: frontend DTOs
- `src-tauri/src/lib.rs`: SQLite schema, commands, transactions, reports, and backup logic
- `src-tauri/tauri.conf.json`: desktop window and bundling configuration

Frontend code never writes SQLite directly. Stock validation and writes stay in
Rust transactions.
