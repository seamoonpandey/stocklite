# StockLite

StockLite is a simple offline inventory app for products, purchases, sales,
stock adjustments, reports, and backups. It stores data on your computer and
does not need an account, server, or internet connection.

<p align="center">
  <a href="https://github.com/seamoonpandey/stocklite/releases/latest">
    <strong>⬇ Download StockLite</strong>
  </a>
</p>

## Install StockLite

Open the [latest release page](https://github.com/seamoonpandey/stocklite/releases/latest)
and download the file for your computer:

| Computer | File to download | What to do |
| --- | --- | --- |
| macOS Apple Silicon | `.dmg` | Open it, then drag StockLite to Applications |
| Windows | `.exe` | Open it and follow the installer |
| Debian/Ubuntu Linux | `.deb` | Open it with the system software installer |
| Fedora/RHEL Linux | `.rpm` | Open it with the system software installer |

The release page contains the installer files. A non-technical user should
only need to download the file for their operating system and open it.

## First use

1. Open StockLite.
2. Select **Add product**.
3. Enter the product name, unit, and optional opening stock.
4. Use **Purchase**, **Sale**, or **Adjustment** whenever stock changes.
5. Create regular backups from **Settings & data**.

StockLite works offline. The browser preview is for development only and uses
demo data; use the installed desktop application for real inventory.

## Build locally

See [development documentation](docs/DEVELOPMENT.md) for setup, testing, and
local builds. See [release documentation](docs/RELEASING.md) for publishing
the macOS, Windows, Debian/Ubuntu, and Fedora/RHEL installers.

## Features

- Product creation, editing, and archival without deleting history
- Permanent deletion of a product together with its transaction history
- Atomic opening stock, purchases, sales, and signed adjustments
- Protection against negative inventory and overselling
- Backdated movements with separate recorded timestamps
- Searchable transaction history and stock reports
- Consistent SQLite backups and validated restores
- Offline-first local database

## Data and backups

The active database is stored in the operating system application-data
directory. Open **Settings & data** to see its exact location, create a `.db`
backup, or restore a compatible backup. Always create a backup before restoring.
