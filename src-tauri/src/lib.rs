use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{Manager, State};

struct AppState(Mutex<Connection>);

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Product {
    id: i64,
    product_code: String,
    name: String,
    unit: String,
    current_stock: i64,
    reorder_threshold: Option<i64>,
    is_active: bool,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Movement {
    id: i64,
    product_id: i64,
    product_name: String,
    movement_type: String,
    quantity_delta: i64,
    occurred_at: String,
    created_at: String,
    note: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardSummary {
    active_products: i64,
    total_units: i64,
    low_stock: i64,
    recent_movements: Vec<Movement>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StockReportRow {
    product_id: i64,
    product_name: String,
    unit: String,
    opening_balance: i64,
    purchases: i64,
    sales: i64,
    adjustments: i64,
    closing_balance: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MovementInput {
    product_id: i64,
    quantity: i64,
    occurred_at: Option<String>,
    note: Option<String>,
}

fn initialize_database(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;
         CREATE TABLE IF NOT EXISTS schema_versions (version INTEGER PRIMARY KEY);
         CREATE TABLE IF NOT EXISTS products (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           product_code TEXT NOT NULL UNIQUE,
           name TEXT NOT NULL COLLATE NOCASE,
           unit TEXT NOT NULL,
           reorder_threshold INTEGER CHECK (reorder_threshold IS NULL OR reorder_threshold >= 0),
           is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
           created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );
         CREATE TABLE IF NOT EXISTS movements (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           product_id INTEGER NOT NULL REFERENCES products(id),
           movement_type TEXT NOT NULL CHECK (movement_type IN ('opening', 'purchase', 'sale', 'adjustment')),
           quantity_delta INTEGER NOT NULL CHECK (quantity_delta <> 0),
           occurred_at TEXT NOT NULL,
           created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
           note TEXT
         );
         CREATE INDEX IF NOT EXISTS idx_movements_product ON movements(product_id);
         CREATE INDEX IF NOT EXISTS idx_movements_occurred ON movements(occurred_at);",
    )?;
    let has_version = connection
        .query_row("SELECT 1 FROM schema_versions WHERE version = 1", [], |_| Ok(()))
        .optional()?
        .is_some();
    if !has_version {
        connection.execute("INSERT INTO schema_versions (version) VALUES (1)", [])?;
    }
    let existing_products: Vec<(i64, String, String)> = {
        let mut statement = connection.prepare("SELECT id, name, unit FROM products")?;
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?
    };
    for (id, name, unit) in existing_products {
        let (normalized_name, normalized_unit) = normalize_product_fields(&name, &unit);
        if normalized_name != name || normalized_unit != unit {
            connection.execute(
                "UPDATE products SET name = ?1, unit = ?2 WHERE id = ?3",
                params![normalized_name, normalized_unit, id],
            )?;
        }
    }
    Ok(())
}

fn migrate_legacy_database(target_dir: &Path, legacy_dir: &Path) -> Result<(), String> {
    let target_path = target_dir.join("stocklite.db");
    let legacy_path = legacy_dir.join("stocklite.db");
    if target_path.exists() || !legacy_path.exists() {
        return Ok(());
    }
    fs::create_dir_all(target_dir).map_err(|error| error.to_string())?;
    let legacy = Connection::open(&legacy_path).map_err(|error| format!("Could not open previous database: {error}"))?;
    let integrity: String = legacy
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|error| format!("Could not verify previous database: {error}"))?;
    if integrity != "ok" {
        return Err(format!("Previous database failed integrity check: {integrity}"));
    }
    let mut target = Connection::open(&target_path).map_err(|error| error.to_string())?;
    let backup = rusqlite::backup::Backup::new(&legacy, &mut target).map_err(|error| error.to_string())?;
    backup
        .run_to_completion(100, std::time::Duration::from_millis(10), None)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn product_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Product> {
    Ok(Product {
        id: row.get(0)?,
        product_code: row.get(1)?,
        name: row.get(2)?,
        unit: row.get(3)?,
        current_stock: row.get(4)?,
        reorder_threshold: row.get(5)?,
        is_active: row.get::<_, i64>(6)? == 1,
    })
}

fn title_case(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_product_fields(name: &str, unit: &str) -> (String, String) {
    (title_case(name.trim()), unit.trim().to_lowercase())
}

fn product_query(include_inactive: bool) -> &'static str {
    if include_inactive {
        "SELECT p.id, p.product_code, p.name, p.unit, COALESCE(SUM(m.quantity_delta), 0),
                p.reorder_threshold, p.is_active
         FROM products p LEFT JOIN movements m ON m.product_id = p.id
         GROUP BY p.id"
    } else {
        "SELECT p.id, p.product_code, p.name, p.unit, COALESCE(SUM(m.quantity_delta), 0),
                p.reorder_threshold, p.is_active
         FROM products p LEFT JOIN movements m ON m.product_id = p.id
         WHERE p.is_active = 1 GROUP BY p.id"
    }
}

#[tauri::command]
fn list_products(state: State<'_, AppState>, search: Option<String>, include_inactive: Option<bool>) -> Result<Vec<Product>, String> {
    let connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    let pattern = format!("%{}%", search.unwrap_or_default().trim());
    let mut statement = connection
        .prepare(&format!(
            "{} HAVING (p.name LIKE ?1 OR p.product_code LIKE ?1) ORDER BY p.is_active DESC, p.name",
            product_query(include_inactive.unwrap_or(false))
        ))
        .map_err(|error| error.to_string())?;
    statement
        .query_map([pattern], product_from_row)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn create_product(
    state: State<'_, AppState>,
    name: String,
    unit: String,
    opening_quantity: i64,
    reorder_threshold: Option<i64>,
) -> Result<Product, String> {
    let mut connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    let transaction = connection.transaction().map_err(|error| error.to_string())?;
    let product_id = insert_product(&transaction, &name, &unit, opening_quantity, reorder_threshold)?;
    transaction.commit().map_err(|error| error.to_string())?;
    get_product(&connection, product_id)
}

fn insert_product(
    transaction: &Connection,
    name: &str,
    unit: &str,
    opening_quantity: i64,
    reorder_threshold: Option<i64>,
) -> Result<i64, String> {
    let (name, unit) = normalize_product_fields(name, unit);
    if name.is_empty() || unit.is_empty() {
        return Err("Product name and unit are required.".to_string());
    }
    if opening_quantity < 0 || reorder_threshold.is_some_and(|value| value < 0) {
        return Err("Quantities cannot be negative.".to_string());
    }
    ensure_unique_name(transaction, &name, None)?;
    let next_id: i64 = transaction
        .query_row("SELECT COALESCE(MAX(id), 0) + 1 FROM products", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    let code = format!("STK-{next_id:04}");
    transaction
        .execute(
            "INSERT INTO products (product_code, name, unit, reorder_threshold) VALUES (?1, ?2, ?3, ?4)",
            params![code, name, unit, reorder_threshold],
        )
        .map_err(|error| match error {
            rusqlite::Error::SqliteFailure(_, _) => "A product with this name or code already exists.".to_string(),
            other => other.to_string(),
        })?;
    let product_id = transaction.last_insert_rowid();
    if opening_quantity > 0 {
        transaction
            .execute(
                "INSERT INTO movements (product_id, movement_type, quantity_delta, occurred_at)
                 VALUES (?1, 'opening', ?2, datetime('now'))",
                params![product_id, opening_quantity],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(product_id)
}

#[tauri::command]
fn update_product(
    state: State<'_, AppState>,
    product_id: i64,
    name: String,
    unit: String,
    reorder_threshold: Option<i64>,
) -> Result<Product, String> {
    let (name, unit) = normalize_product_fields(&name, &unit);
    if name.is_empty() || unit.is_empty() {
        return Err("Product name and unit are required.".to_string());
    }
    if reorder_threshold.is_some_and(|value| value < 0) {
        return Err("Reorder threshold cannot be negative.".to_string());
    }
    let connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    ensure_unique_name(&connection, &name, Some(product_id))?;
    let updated = connection
        .execute(
            "UPDATE products SET name = ?1, unit = ?2, reorder_threshold = ?3 WHERE id = ?4 AND is_active = 1",
            params![name, unit, reorder_threshold, product_id],
        )
        .map_err(|error| error.to_string())?;
    if updated == 0 {
        return Err("Product was not found or is archived.".to_string());
    }
    get_product(&connection, product_id)
}

fn ensure_unique_name(connection: &Connection, name: &str, except_id: Option<i64>) -> Result<(), String> {
    let existing: Option<bool> = connection
        .query_row(
            "SELECT is_active FROM products WHERE name = ?1 COLLATE NOCASE AND id IS NOT ?2",
            params![name, except_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    match existing {
        Some(true) => Err("A product with this name already exists.".to_string()),
        Some(false) => Err("An archived product with this name already exists. Restore it from Products > Archived.".to_string()),
        None => Ok(()),
    }
}

#[tauri::command]
fn archive_product(state: State<'_, AppState>, product_id: i64) -> Result<(), String> {
    let connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    set_archived(&connection, product_id, true)
}

#[tauri::command]
fn unarchive_product(state: State<'_, AppState>, product_id: i64) -> Result<(), String> {
    let connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    set_archived(&connection, product_id, false)
}

fn set_archived(connection: &Connection, product_id: i64, archived: bool) -> Result<(), String> {
    let changed = connection
        .execute(
            "UPDATE products SET is_active = ?1 WHERE id = ?2 AND is_active = ?3",
            params![!archived, product_id, archived],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(if archived { "Product was not found or already archived." } else { "Product was not found or is not archived." }.to_string());
    }
    Ok(())
}

#[tauri::command]
fn delete_product(state: State<'_, AppState>, product_id: i64) -> Result<(), String> {
    let mut connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    delete_product_and_history(&mut connection, product_id)
}

fn delete_product_and_history(connection: &mut Connection, product_id: i64) -> Result<(), String> {
    let transaction = connection.transaction().map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM movements WHERE product_id = ?1", [product_id])
        .map_err(|error| error.to_string())?;
    let deleted = transaction
        .execute("DELETE FROM products WHERE id = ?1", [product_id])
        .map_err(|error| error.to_string())?;
    if deleted == 0 {
        return Err("Product was not found.".to_string());
    }
    transaction.commit().map_err(|error| error.to_string())
}

fn get_product(connection: &Connection, product_id: i64) -> Result<Product, String> {
    connection
        .query_row(
            &format!("{} HAVING p.id = ?1", product_query(true)),
            [product_id],
            product_from_row,
        )
        .map_err(|error| error.to_string())
}

fn record_movement(connection: &mut Connection, input: MovementInput, movement_type: &str) -> Result<Product, String> {
    let transaction = connection.transaction().map_err(|error| error.to_string())?;
    let product_id = input.product_id;
    insert_movement(&transaction, input, movement_type)?;
    transaction.commit().map_err(|error| error.to_string())?;
    get_product(connection, product_id)
}

fn insert_movement(transaction: &Connection, input: MovementInput, movement_type: &str) -> Result<(), String> {
    if (movement_type != "adjustment" && input.quantity <= 0) || (movement_type == "adjustment" && input.quantity == 0) {
        return Err(if movement_type == "adjustment" {
            "Adjustment cannot be zero.".to_string()
        } else {
            "Quantity must be greater than zero.".to_string()
        });
    }
    let active: Option<i64> = transaction
        .query_row("SELECT id FROM products WHERE id = ?1 AND is_active = 1", [input.product_id], |row| row.get(0))
        .optional()
        .map_err(|error| error.to_string())?;
    if active.is_none() {
        return Err("Product was not found or is archived.".to_string());
    }
    let current: i64 = transaction
        .query_row(
            "SELECT COALESCE(SUM(m.quantity_delta), 0) FROM movements m
             JOIN products p ON p.id = m.product_id
             WHERE m.product_id = ?1 AND p.is_active = 1",
            [input.product_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let delta = if movement_type == "sale" { -input.quantity } else { input.quantity };
    if current + delta < 0 {
        return Err(format!("Not enough stock. Available balance is {current}."));
    }
    let occurred_at = input.occurred_at.filter(|value| !value.trim().is_empty()).unwrap_or_else(|| "now".to_string());
    let result = if occurred_at == "now" {
        transaction.execute(
            "INSERT INTO movements (product_id, movement_type, quantity_delta, occurred_at, note)
             VALUES (?1, ?2, ?3, datetime('now'), ?4)",
            params![input.product_id, movement_type, delta, input.note],
        )
    } else {
        transaction.execute(
            "INSERT INTO movements (product_id, movement_type, quantity_delta, occurred_at, note)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![input.product_id, movement_type, delta, occurred_at, input.note],
        )
    };
    result.map(|_| ()).map_err(|error| error.to_string())
}

#[tauri::command]
fn record_purchase(state: State<'_, AppState>, input: MovementInput) -> Result<Product, String> {
    let mut connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    record_movement(&mut connection, input, "purchase")
}

#[tauri::command]
fn record_sale(state: State<'_, AppState>, input: MovementInput) -> Result<Product, String> {
    let mut connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    record_movement(&mut connection, input, "sale")
}

#[tauri::command]
fn record_adjustment(state: State<'_, AppState>, input: MovementInput) -> Result<Product, String> {
    let mut connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    record_movement(&mut connection, input, "adjustment")
}

fn movement_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Movement> {
    Ok(Movement {
        id: row.get(0)?,
        product_id: row.get(1)?,
        product_name: row.get(2)?,
        movement_type: row.get(3)?,
        quantity_delta: row.get(4)?,
        occurred_at: row.get(5)?,
        created_at: row.get(6)?,
        note: row.get(7)?,
    })
}

#[tauri::command]
fn list_movements(
    state: State<'_, AppState>,
    product_id: Option<i64>,
    movement_type: Option<String>,
    from_date: Option<String>,
    to_date: Option<String>,
) -> Result<Vec<Movement>, String> {
    let connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT m.id, m.product_id, p.name, m.movement_type, m.quantity_delta,
                    m.occurred_at, m.created_at, m.note
             FROM movements m JOIN products p ON p.id = m.product_id
             WHERE (?1 IS NULL OR m.product_id = ?1)
               AND (?2 IS NULL OR m.movement_type = ?2)
               AND (?3 IS NULL OR date(m.occurred_at) >= date(?3))
               AND (?4 IS NULL OR date(m.occurred_at) <= date(?4))
             ORDER BY date(m.occurred_at) DESC, m.id DESC LIMIT 500",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map(params![product_id, movement_type, from_date, to_date], movement_from_row)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_dashboard_summary(state: State<'_, AppState>) -> Result<DashboardSummary, String> {
    let connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    let active_products = connection
        .query_row("SELECT COUNT(*) FROM products WHERE is_active = 1", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    let total_units = connection
        .query_row(
            "SELECT COALESCE(SUM(balance), 0) FROM
             (SELECT COALESCE(SUM(m.quantity_delta), 0) AS balance
              FROM movements m JOIN products p ON p.id = m.product_id
              WHERE p.is_active = 1 GROUP BY m.product_id)",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let low_stock = connection
        .query_row(
            "SELECT COUNT(*) FROM products p WHERE p.is_active = 1 AND p.reorder_threshold IS NOT NULL
             AND (SELECT COALESCE(SUM(quantity_delta), 0) FROM movements WHERE product_id = p.id) <= p.reorder_threshold",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let recent_movements = query_movements(&connection, 5)?;
    Ok(DashboardSummary { active_products, total_units, low_stock, recent_movements })
}

fn query_movements(connection: &Connection, limit: i64) -> Result<Vec<Movement>, String> {
    let mut statement = connection
        .prepare(
            "SELECT m.id, m.product_id, p.name, m.movement_type, m.quantity_delta,
                    m.occurred_at, m.created_at, m.note
             FROM movements m JOIN products p ON p.id = m.product_id
             ORDER BY date(m.occurred_at) DESC, m.id DESC LIMIT ?1",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map([limit], movement_from_row)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_stock_report(
    state: State<'_, AppState>,
    from_date: String,
    to_date: String,
) -> Result<Vec<StockReportRow>, String> {
    let connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    stock_report(&connection, &from_date, &to_date)
}

fn stock_report(connection: &Connection, from_date: &str, to_date: &str) -> Result<Vec<StockReportRow>, String> {
    if from_date > to_date {
        return Err("The report start date must be before its end date.".to_string());
    }
    let mut statement = connection
        .prepare(
            "SELECT p.id, p.name, p.unit,
             COALESCE((SELECT SUM(quantity_delta) FROM movements WHERE product_id = p.id AND (date(occurred_at) < date(?1)
                       OR (movement_type = 'opening' AND date(occurred_at) <= date(?2)))), 0),
             COALESCE((SELECT SUM(quantity_delta) FROM movements WHERE product_id = p.id AND movement_type = 'purchase' AND date(occurred_at) BETWEEN date(?1) AND date(?2)), 0),
             COALESCE((SELECT SUM(-quantity_delta) FROM movements WHERE product_id = p.id AND movement_type = 'sale' AND date(occurred_at) BETWEEN date(?1) AND date(?2)), 0),
             COALESCE((SELECT SUM(quantity_delta) FROM movements WHERE product_id = p.id AND movement_type = 'adjustment' AND date(occurred_at) BETWEEN date(?1) AND date(?2)), 0),
             COALESCE((SELECT SUM(quantity_delta) FROM movements WHERE product_id = p.id AND date(occurred_at) <= date(?2)), 0)
             FROM products p WHERE p.is_active = 1 ORDER BY p.name",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map(params![from_date, to_date], |row| {
            Ok(StockReportRow {
                product_id: row.get(0)?,
                product_name: row.get(1)?,
                unit: row.get(2)?,
                opening_balance: row.get(3)?,
                purchases: row.get(4)?,
                sales: row.get(5)?,
                adjustments: row.get(6)?,
                closing_balance: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn export_stock_report(
    state: State<'_, AppState>,
    from_date: String,
    to_date: String,
    destination: String,
) -> Result<String, String> {
    let rows = {
        let connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
        stock_report(&connection, &from_date, &to_date)?
    };
    write_report(&rows, Path::new(&destination))?;
    Ok(destination)
}

const REPORT_HEADERS: [&str; 7] = ["Product", "Unit", "Opening", "Purchases", "Sales", "Adjustments", "Closing"];

fn write_report(rows: &[StockReportRow], destination: &Path) -> Result<(), String> {
    let numbers = |row: &StockReportRow| [row.opening_balance, row.purchases, row.sales, row.adjustments, row.closing_balance];
    match destination.extension().and_then(|value| value.to_str()) {
        Some("csv") => {
            let mut writer = csv::Writer::from_path(destination).map_err(|error| error.to_string())?;
            writer.write_record(REPORT_HEADERS).map_err(|error| error.to_string())?;
            for row in rows {
                let mut record = vec![row.product_name.clone(), row.unit.clone()];
                record.extend(numbers(row).map(|value| value.to_string()));
                writer.write_record(record).map_err(|error| error.to_string())?;
            }
            writer.flush().map_err(|error| error.to_string())
        }
        Some("xlsx") => {
            let mut workbook = rust_xlsxwriter::Workbook::new();
            let sheet = workbook.add_worksheet().set_name("Stock report").map_err(|error| error.to_string())?;
            sheet
                .write_row_with_format(0, 0, REPORT_HEADERS, &rust_xlsxwriter::Format::new().set_bold())
                .map_err(|error| error.to_string())?;
            for (index, row) in rows.iter().enumerate() {
                let line = index as u32 + 1;
                sheet.write_row(line, 0, [&row.product_name, &row.unit]).map_err(|error| error.to_string())?;
                sheet.write_row(line, 2, numbers(row).map(|value| value as f64)).map_err(|error| error.to_string())?;
            }
            sheet.autofit();
            workbook.save(destination).map_err(|error| error.to_string())
        }
        _ => Err("Export files must use the .csv or .xlsx extension.".to_string()),
    }
}

/// First sheet (or the CSV) as text cells; Excel dates become YYYY-MM-DD.
fn read_spreadsheet(source: &Path) -> Result<Vec<Vec<String>>, String> {
    if source.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case("csv")) {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_path(source)
            .map_err(|error| format!("Could not open file: {error}"))?;
        // Byte records so files saved in a legacy Excel encoding still import instead of failing outright.
        return reader
            .byte_records()
            .map(|record| {
                record
                    .map(|record| record.iter().map(|cell| String::from_utf8_lossy(cell).into_owned()).collect())
                    .map_err(|error| error.to_string())
            })
            .collect();
    }
    use calamine::{Data, Reader};
    let mut workbook = calamine::open_workbook_auto(source).map_err(|error| format!("Could not open spreadsheet: {error}"))?;
    let range = workbook
        .worksheet_range_at(0)
        .ok_or("The spreadsheet has no sheets.")?
        .map_err(|error| error.to_string())?;
    Ok(range
        .rows()
        .map(|row| {
            row.iter()
                .map(|cell| match cell {
                    Data::DateTime(value) => value.as_datetime().map(|date| date.date().to_string()).unwrap_or_default(),
                    Data::DateTimeIso(value) => value.chars().take(10).collect(),
                    other => other.to_string(),
                })
                .collect()
        })
        .collect())
}

type ImportRecord = HashMap<String, String>;

/// Non-blank data rows keyed by lowercased header, with their spreadsheet row number for error messages.
fn read_import_records(source: &Path, required: &[&str]) -> Result<Vec<(usize, ImportRecord)>, String> {
    let mut rows = read_spreadsheet(source)?.into_iter();
    let headers: Vec<String> = rows
        .next()
        .unwrap_or_default()
        .iter()
        .map(|header| header.trim_start_matches('\u{feff}').trim().to_lowercase().replace('_', " "))
        .collect();
    if let Some(missing) = required.iter().find(|column| !headers.iter().any(|header| header == *column)) {
        return Err(format!("The first row must contain a \"{missing}\" column header."));
    }
    Ok(rows
        .enumerate()
        .filter(|(_, row)| row.iter().any(|cell| !cell.trim().is_empty()))
        .map(|(index, row)| (index + 2, headers.iter().cloned().zip(row.into_iter().map(|cell| cell.trim().to_string())).collect()))
        .collect())
}

fn field<'a>(record: &'a ImportRecord, column: &str) -> &'a str {
    record.get(column).map_or("", String::as_str)
}

fn whole_number(record: &ImportRecord, column: &str) -> Result<Option<i64>, String> {
    let value = field(record, column);
    if value.is_empty() {
        return Ok(None);
    }
    value.parse().map(Some).map_err(|_| format!("\"{column}\" must be a whole number, not \"{value}\"."))
}

/// All rows commit together, so a bad row never leaves a half-imported file behind.
fn import_records(
    connection: &mut Connection,
    records: &[(usize, ImportRecord)],
    import_row: fn(&Connection, &ImportRecord) -> Result<(), String>,
) -> Result<usize, String> {
    if records.is_empty() {
        return Err("The file has no rows to import.".to_string());
    }
    let transaction = connection.transaction().map_err(|error| error.to_string())?;
    for (row, record) in records {
        import_row(&transaction, record).map_err(|error| format!("Row {row}: {error}"))?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(records.len())
}

fn import_product_row(transaction: &Connection, record: &ImportRecord) -> Result<(), String> {
    let opening_quantity = whole_number(record, "opening stock")?.unwrap_or(0);
    let reorder_threshold = whole_number(record, "reorder threshold")?;
    insert_product(transaction, field(record, "name"), field(record, "unit"), opening_quantity, reorder_threshold).map(|_| ())
}

fn import_movement_row(transaction: &Connection, record: &ImportRecord) -> Result<(), String> {
    let movement_type = field(record, "type").to_lowercase();
    if !["purchase", "sale", "adjustment"].contains(&movement_type.as_str()) {
        return Err("\"type\" must be purchase, sale, or adjustment.".to_string());
    }
    let product = field(record, "product");
    let product_id: i64 = transaction
        .query_row(
            "SELECT id FROM products WHERE product_code = ?1 COLLATE NOCASE OR name = ?1
             ORDER BY product_code = ?1 COLLATE NOCASE DESC LIMIT 1",
            [product],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Product \"{product}\" was not found."))?;
    let quantity = whole_number(record, "quantity")?.ok_or("\"quantity\" is required.")?;
    let occurred_at = field(record, "date");
    if !occurred_at.is_empty() {
        let valid: bool = transaction
            .query_row("SELECT date(?1) IS ?1", [occurred_at], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        if !valid {
            return Err(format!("\"date\" must look like 2026-10-25, not \"{occurred_at}\"."));
        }
    }
    let note = Some(field(record, "note").to_string()).filter(|value| !value.is_empty());
    insert_movement(
        transaction,
        MovementInput { product_id, quantity, occurred_at: Some(occurred_at.to_string()), note },
        &movement_type,
    )
}

fn import_products_file(connection: &mut Connection, source: &Path) -> Result<usize, String> {
    let records = read_import_records(source, &["name", "unit"])?;
    import_records(connection, &records, import_product_row)
}

fn import_movements_file(connection: &mut Connection, source: &Path) -> Result<usize, String> {
    let records = read_import_records(source, &["product", "type", "quantity"])?;
    import_records(connection, &records, import_movement_row)
}

#[tauri::command]
fn import_products(state: State<'_, AppState>, source: String) -> Result<usize, String> {
    let mut connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    import_products_file(&mut connection, Path::new(&source))
}

#[tauri::command]
fn import_movements(state: State<'_, AppState>, source: String) -> Result<usize, String> {
    let mut connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    import_movements_file(&mut connection, Path::new(&source))
}

#[tauri::command]
fn create_backup(state: State<'_, AppState>, destination: String) -> Result<String, String> {
    let connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    let destination = PathBuf::from(destination);
    if destination.extension().and_then(|value| value.to_str()) != Some("db") {
        return Err("Backup files must use the .db extension.".to_string());
    }
    let current_path: String = connection
        .query_row("PRAGMA database_list", [], |row| row.get(2))
        .map_err(|error| error.to_string())?;
    if Path::new(&current_path) == destination {
        return Err("Choose a backup path different from the active database.".to_string());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut backup = Connection::open(&destination).map_err(|error| error.to_string())?;
    let backup = rusqlite::backup::Backup::new(&connection, &mut backup).map_err(|error| error.to_string())?;
    backup.run_to_completion(100, std::time::Duration::from_millis(10), None).map_err(|error| error.to_string())?;
    Ok(destination.to_string_lossy().into_owned())
}

#[tauri::command]
fn restore_backup(state: State<'_, AppState>, source: String) -> Result<(), String> {
    let source = Path::new(&source);
    if source.extension().and_then(|value| value.to_str()) != Some("db") {
        return Err("Backup files must use the .db extension.".to_string());
    }
    let backup = Connection::open(source).map_err(|error| error.to_string())?;
    backup
        .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())
        .and_then(|result| if result == "ok" { Ok(()) } else { Err(format!("Backup failed integrity check: {result}")) })?;
    let has_schema: i64 = backup
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('products', 'movements')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if has_schema != 2 {
        return Err("Backup does not contain a compatible StockLite database.".to_string());
    }
    let mut connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    let current_path: String = connection
        .query_row("PRAGMA database_list", [], |row| row.get(2))
        .map_err(|error| error.to_string())?;
    if Path::new(&current_path) == source {
        return Err("Choose a backup file different from the active database.".to_string());
    }
    connection.execute_batch("PRAGMA foreign_keys = OFF;").map_err(|error| error.to_string())?;
    let restore = rusqlite::backup::Backup::new(&backup, &mut connection).map_err(|error| error.to_string())?;
    restore.run_to_completion(100, std::time::Duration::from_millis(10), None).map_err(|error| error.to_string())?;
    drop(restore);
    connection.execute_batch("PRAGMA foreign_keys = ON;").map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_database_path(state: State<'_, AppState>) -> Result<String, String> {
    let connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    connection
        .query_row("PRAGMA database_list", [], |row| row.get::<_, String>(2))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        connection
    }

    #[test]
    fn stock_operations_are_atomic_and_reject_overselling() {
        let mut connection = database();
        connection.execute("INSERT INTO products (product_code, name, unit) VALUES ('STK-0001', 'Coffee', 'bags')", []).unwrap();
        let product_id = connection.last_insert_rowid();
        record_movement(&mut connection, MovementInput { product_id, quantity: 100, occurred_at: None, note: None }, "opening").unwrap();
        record_movement(&mut connection, MovementInput { product_id, quantity: 40, occurred_at: None, note: None }, "purchase").unwrap();
        record_movement(&mut connection, MovementInput { product_id, quantity: 25, occurred_at: None, note: None }, "sale").unwrap();
        let error = record_movement(&mut connection, MovementInput { product_id, quantity: 116, occurred_at: None, note: None }, "sale").unwrap_err();
        assert!(error.contains("Not enough stock"));
        let balance: i64 = connection.query_row("SELECT SUM(quantity_delta) FROM movements", [], |row| row.get(0)).unwrap();
        assert_eq!(balance, 115);
    }

    #[test]
    fn adjustments_support_signed_corrections_without_negative_stock() {
        let mut connection = database();
        connection.execute("INSERT INTO products (product_code, name, unit) VALUES ('STK-0001', 'Filters', 'boxes')", []).unwrap();
        let product_id = connection.last_insert_rowid();
        record_movement(&mut connection, MovementInput { product_id, quantity: 12, occurred_at: None, note: None }, "opening").unwrap();
        record_movement(&mut connection, MovementInput { product_id, quantity: -3, occurred_at: None, note: Some("Damaged".into()) }, "adjustment").unwrap();
        let balance: i64 = connection.query_row("SELECT SUM(quantity_delta) FROM movements", [], |row| row.get(0)).unwrap();
        assert_eq!(balance, 9);
        assert!(record_movement(&mut connection, MovementInput { product_id, quantity: -10, occurred_at: None, note: None }, "adjustment").is_err());
    }

    #[test]
    fn product_and_opening_stock_commit_together() {
        let connection = database();
        connection.execute("INSERT INTO products (product_code, name, unit) VALUES ('STK-0001', 'Coffee', 'bags')", []).unwrap();
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM movements", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
    }

    #[test]
    fn legacy_database_is_copied_only_when_new_location_is_empty() {
        let root = std::env::temp_dir().join(format!("stocklite-migration-{}", std::process::id()));
        let legacy_dir = root.join("com.stocklite.desktop");
        let current_dir = root.join("com.stocklite.app");
        fs::create_dir_all(&legacy_dir).unwrap();
        let legacy = Connection::open(legacy_dir.join("stocklite.db")).unwrap();
        initialize_database(&legacy).unwrap();
        legacy.execute("INSERT INTO products (product_code, name, unit) VALUES ('STK-0001', 'Legacy item', 'pieces')", []).unwrap();
        drop(legacy);
        migrate_legacy_database(&current_dir, &legacy_dir).unwrap();
        let migrated = Connection::open(current_dir.join("stocklite.db")).unwrap();
        assert_eq!(migrated.query_row("SELECT name FROM products", [], |row| row.get::<_, String>(0)).unwrap(), "Legacy item");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn product_fields_are_normalized_and_history_is_recorded() {
        let (name, unit) = normalize_product_fields("  arabica COFFEE beans ", " BAGS ");
        assert_eq!(name, "Arabica Coffee Beans");
        assert_eq!(unit, "bags");
        let mut connection = database();
        connection.execute("INSERT INTO products (product_code, name, unit) VALUES ('STK-0001', ?1, ?2)", params![name, unit]).unwrap();
        let product_id = connection.last_insert_rowid();
        record_movement(&mut connection, MovementInput { product_id, quantity: 1, occurred_at: None, note: None }, "opening").unwrap();
        let history_count: i64 = connection.query_row("SELECT COUNT(*) FROM movements WHERE product_id = ?1", [product_id], |row| row.get(0)).unwrap();
        assert_eq!(history_count, 1);
    }

    fn product_with_timestamped_opening_and_dated_purchase(connection: &mut Connection) {
        connection.execute("INSERT INTO products (product_code, name, unit) VALUES ('STK-0001', 'Rod', 'pieces')", []).unwrap();
        let product_id = connection.last_insert_rowid();
        connection
            .execute(
                "INSERT INTO movements (product_id, movement_type, quantity_delta, occurred_at) VALUES (?1, 'opening', 1000, '2026-10-25 10:18:23')",
                [product_id],
            )
            .unwrap();
        record_movement(connection, MovementInput { product_id, quantity: 7, occurred_at: Some("2026-10-25".into()), note: None }, "purchase").unwrap();
    }

    #[test]
    fn deleting_a_product_removes_its_history_only() {
        let mut connection = database();
        for (code, name) in [("STK-0001", "Rod"), ("STK-0002", "Pipe")] {
            connection.execute("INSERT INTO products (product_code, name, unit) VALUES (?1, ?2, 'pieces')", [code, name]).unwrap();
            let product_id = connection.last_insert_rowid();
            record_movement(&mut connection, MovementInput { product_id, quantity: 10, occurred_at: None, note: None }, "opening").unwrap();
            record_movement(&mut connection, MovementInput { product_id, quantity: 4, occurred_at: None, note: None }, "sale").unwrap();
        }
        delete_product_and_history(&mut connection, 1).unwrap();
        let count = |sql: &str| connection.query_row(sql, [], |row| row.get::<_, i64>(0)).unwrap();
        assert_eq!(count("SELECT COUNT(*) FROM products WHERE id = 1"), 0);
        assert_eq!(count("SELECT COUNT(*) FROM movements WHERE product_id = 1"), 0);
        assert_eq!(count("SELECT COUNT(*) FROM movements WHERE product_id = 2"), 2);
        assert!(delete_product_and_history(&mut connection, 1).is_err());
    }

    #[test]
    fn newest_movement_comes_first_across_timestamp_formats() {
        let mut connection = database();
        product_with_timestamped_opening_and_dated_purchase(&mut connection);
        assert_eq!(query_movements(&connection, 5).unwrap()[0].movement_type, "purchase");
    }

    #[test]
    fn report_includes_timestamped_movements_on_boundary_days() {
        let mut connection = database();
        product_with_timestamped_opening_and_dated_purchase(&mut connection);
        let row = &stock_report(&connection, "2026-10-01", "2026-10-25").unwrap()[0];
        assert_eq!((row.opening_balance, row.purchases, row.closing_balance), (1000, 7, 1007));
        assert_eq!(stock_report(&connection, "2026-10-26", "2026-10-31").unwrap()[0].opening_balance, 1007);
    }

    #[test]
    fn report_rows_add_up_when_a_product_starts_inside_the_period() {
        let mut connection = database();
        product_with_timestamped_opening_and_dated_purchase(&mut connection);
        connection.execute("INSERT INTO movements (product_id, movement_type, quantity_delta, occurred_at) VALUES (1, 'sale', -50, '2026-10-26')", []).unwrap();
        record_movement(&mut connection, MovementInput { product_id: 1, quantity: -3, occurred_at: Some("2026-10-27".into()), note: None }, "adjustment").unwrap();
        for (from, to) in [("2026-10-01", "2026-10-31"), ("2026-10-26", "2026-10-31"), ("2026-10-25", "2026-10-25")] {
            let row = &stock_report(&connection, from, to).unwrap()[0];
            assert_eq!(row.opening_balance + row.purchases - row.sales + row.adjustments, row.closing_balance, "{from}..{to}");
        }
    }

    #[test]
    fn archived_products_can_be_restored() {
        let connection = database();
        connection.execute("INSERT INTO products (product_code, name, unit) VALUES ('STK-0001', 'Rod', 'pieces')", []).unwrap();
        set_archived(&connection, 1, true).unwrap();
        assert!(set_archived(&connection, 1, true).is_err());
        set_archived(&connection, 1, false).unwrap();
        assert!(set_archived(&connection, 1, false).is_err());
        assert!(get_product(&connection, 1).unwrap().is_active);
    }

    #[test]
    fn product_names_stay_unique_and_point_to_archived_duplicates() {
        let connection = database();
        connection.execute("INSERT INTO products (product_code, name, unit) VALUES ('STK-0001', 'Rod', 'pieces')", []).unwrap();
        connection.execute("INSERT INTO products (product_code, name, unit, is_active) VALUES ('STK-0002', 'Pipe', 'pieces', 0)", []).unwrap();
        assert_eq!(ensure_unique_name(&connection, "ROD", None).unwrap_err(), "A product with this name already exists.");
        assert!(ensure_unique_name(&connection, "Pipe", None).unwrap_err().contains("archived"));
        assert!(ensure_unique_name(&connection, "Rod", Some(1)).is_ok());
        assert!(ensure_unique_name(&connection, "Rod", Some(2)).is_err());
        assert!(ensure_unique_name(&connection, "Bolt", None).is_ok());
    }

    fn temp_file(name: &str, contents: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("stocklite-{}-{name}", std::process::id()));
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn product_import_creates_all_rows_or_none() {
        let mut connection = database();
        let good = temp_file("products.csv", "\u{feff}Name,Unit,Opening_Stock,Reorder Threshold\n\"Coffee, dark\",BAGS,12,3\n\nfilters,boxes,,\n");
        assert_eq!(import_products_file(&mut connection, &good).unwrap(), 2);
        let coffee = get_product(&connection, 1).unwrap();
        assert_eq!((coffee.name.as_str(), coffee.unit.as_str(), coffee.current_stock, coffee.reorder_threshold), ("Coffee, Dark", "bags", 12, Some(3)));
        let bad = temp_file("products-bad.csv", "name,unit,opening stock\nRod,pieces,5\nfilters,boxes,1\n");
        assert_eq!(import_products_file(&mut connection, &bad).unwrap_err(), "Row 3: A product with this name already exists.");
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM products", [], |row| row.get::<_, i64>(0)).unwrap(), 2);
        let missing = temp_file("products-missing.csv", "name\nRod\n");
        assert!(import_products_file(&mut connection, &missing).unwrap_err().contains("\"unit\""));
        for path in [good, bad, missing] {
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn movement_import_matches_products_and_rolls_back_on_errors() {
        let mut connection = database();
        connection.execute("INSERT INTO products (product_code, name, unit) VALUES ('STK-0001', 'Rod', 'pieces')", []).unwrap();
        let good = temp_file("movements.csv", "date,product,type,quantity,note\n2026-10-01,stk-0001,Purchase,10,Supplier\n,rod,sale,4,\n2026-10-03,Rod,adjustment,-1,Damaged\n");
        assert_eq!(import_movements_file(&mut connection, &good).unwrap(), 3);
        assert_eq!(get_product(&connection, 1).unwrap().current_stock, 5);
        for (contents, error) in [
            ("product,type,quantity\nRod,purchase,1\nRod,sale,7\n", "Row 3: Not enough stock. Available balance is 6."),
            ("product,type,quantity\nPipe,purchase,1\n", "Row 2: Product \"Pipe\" was not found."),
            ("product,type,quantity,date\nRod,purchase,1,25/10/2026\n", "Row 2: \"date\" must look like 2026-10-25, not \"25/10/2026\"."),
            ("product,type,quantity\nRod,opening,1\n", "Row 2: \"type\" must be purchase, sale, or adjustment."),
            ("product,type,quantity\nRod,purchase,1.5\n", "Row 2: \"quantity\" must be a whole number, not \"1.5\"."),
        ] {
            let bad = temp_file("movements-bad.csv", contents);
            assert_eq!(import_movements_file(&mut connection, &bad).unwrap_err(), error);
            fs::remove_file(bad).unwrap();
        }
        assert_eq!(get_product(&connection, 1).unwrap().current_stock, 5);
        fs::remove_file(good).unwrap();
    }

    #[test]
    fn report_exports_read_back_as_the_same_table() {
        let mut connection = database();
        product_with_timestamped_opening_and_dated_purchase(&mut connection);
        let rows = stock_report(&connection, "2026-10-01", "2026-10-31").unwrap();
        for extension in ["csv", "xlsx"] {
            let path = std::env::temp_dir().join(format!("stocklite-{}-report.{extension}", std::process::id()));
            write_report(&rows, &path).unwrap();
            assert_eq!(
                read_spreadsheet(&path).unwrap(),
                vec![REPORT_HEADERS.map(String::from).to_vec(), ["Rod", "pieces", "1000", "7", "0", "0", "1007"].map(String::from).to_vec()],
                "{extension}"
            );
            fs::remove_file(path).unwrap();
        }
        assert!(write_report(&rows, Path::new("report.pdf")).is_err());
    }

    #[test]
    fn excel_date_cells_import_as_calendar_days() {
        let mut connection = database();
        connection.execute("INSERT INTO products (product_code, name, unit) VALUES ('STK-0001', 'Rod', 'pieces')", []).unwrap();
        let path = std::env::temp_dir().join(format!("stocklite-{}-movements.xlsx", std::process::id()));
        let mut workbook = rust_xlsxwriter::Workbook::new();
        let sheet = workbook.add_worksheet();
        sheet.write_row(0, 0, ["Product", "Type", "Quantity", "Date"]).unwrap();
        sheet.write_row(1, 0, ["Rod", "purchase"]).unwrap();
        sheet.write(1, 2, 3).unwrap();
        let date = rust_xlsxwriter::ExcelDateTime::from_ymd(2026, 10, 25).unwrap();
        sheet.write_datetime_with_format(1, 3, &date, &rust_xlsxwriter::Format::new().set_num_format("yyyy-mm-dd")).unwrap();
        workbook.save(&path).unwrap();
        assert_eq!(import_movements_file(&mut connection, &path).unwrap(), 1);
        assert_eq!(query_movements(&connection, 1).unwrap()[0].occurred_at, "2026-10-25");
        fs::remove_file(path).unwrap();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
            fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
            if let Some(parent) = data_dir.parent() {
                let legacy_dir = parent.join("com.stocklite.desktop");
                migrate_legacy_database(&data_dir, &legacy_dir)?;
            }
            let database_path = data_dir.join("stocklite.db");
            let connection = Connection::open(database_path).map_err(|error| error.to_string())?;
            initialize_database(&connection).map_err(|error| error.to_string())?;
            app.manage(AppState(Mutex::new(connection)));
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_products,
            create_product,
            update_product,
            archive_product,
            unarchive_product,
            delete_product,
            record_purchase,
            record_sale,
            record_adjustment,
            list_movements,
            get_dashboard_summary,
            get_stock_report,
            export_stock_report,
            import_products,
            import_movements,
            create_backup,
            restore_backup,
            get_database_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running StockLite");
}
