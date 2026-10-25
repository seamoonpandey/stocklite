use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
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
    let name = name.trim();
    let unit = unit.trim();
    if name.is_empty() || unit.is_empty() {
        return Err("Product name and unit are required.".to_string());
    }
    if opening_quantity < 0 || reorder_threshold.is_some_and(|value| value < 0) {
        return Err("Quantities cannot be negative.".to_string());
    }
    let mut connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    let transaction = connection.transaction().map_err(|error| error.to_string())?;
    let duplicate: Option<i64> = transaction
        .query_row("SELECT id FROM products WHERE name = ?1 COLLATE NOCASE", [name], |row| row.get(0))
        .optional()
        .map_err(|error| error.to_string())?;
    if duplicate.is_some() {
        return Err("A product with this name already exists.".to_string());
    }
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
    transaction.commit().map_err(|error| error.to_string())?;
    get_product(&connection, product_id)
}

#[tauri::command]
fn update_product(
    state: State<'_, AppState>,
    product_id: i64,
    name: String,
    unit: String,
    reorder_threshold: Option<i64>,
) -> Result<Product, String> {
    let name = name.trim();
    let unit = unit.trim();
    if name.is_empty() || unit.is_empty() {
        return Err("Product name and unit are required.".to_string());
    }
    if reorder_threshold.is_some_and(|value| value < 0) {
        return Err("Reorder threshold cannot be negative.".to_string());
    }
    let connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
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

#[tauri::command]
fn archive_product(state: State<'_, AppState>, product_id: i64) -> Result<(), String> {
    let connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    let changed = connection
        .execute("UPDATE products SET is_active = 0 WHERE id = ?1 AND is_active = 1", [product_id])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("Product was not found or already archived.".to_string());
    }
    Ok(())
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
    if (movement_type != "adjustment" && input.quantity <= 0) || (movement_type == "adjustment" && input.quantity == 0) {
        return Err(if movement_type == "adjustment" {
            "Adjustment cannot be zero.".to_string()
        } else {
            "Quantity must be greater than zero.".to_string()
        });
    }
    let transaction = connection.transaction().map_err(|error| error.to_string())?;
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
    result.map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    get_product(connection, input.product_id)
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
             ORDER BY m.occurred_at DESC, m.id DESC LIMIT 500",
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
             ORDER BY m.occurred_at DESC, m.id DESC LIMIT ?1",
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
    if from_date > to_date {
        return Err("The report start date must be before its end date.".to_string());
    }
    let connection = state.0.lock().map_err(|_| "Database is unavailable".to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT p.id, p.name, p.unit,
             COALESCE((SELECT SUM(quantity_delta) FROM movements WHERE product_id = p.id AND occurred_at < ?1), 0),
             COALESCE((SELECT SUM(quantity_delta) FROM movements WHERE product_id = p.id AND movement_type = 'purchase' AND date(occurred_at) BETWEEN date(?1) AND date(?2)), 0),
             COALESCE((SELECT SUM(-quantity_delta) FROM movements WHERE product_id = p.id AND movement_type = 'sale' AND date(occurred_at) BETWEEN date(?1) AND date(?2)), 0),
             COALESCE((SELECT SUM(quantity_delta) FROM movements WHERE product_id = p.id AND movement_type = 'adjustment' AND date(occurred_at) BETWEEN date(?1) AND date(?2)), 0),
             COALESCE((SELECT SUM(quantity_delta) FROM movements WHERE product_id = p.id AND occurred_at <= ?2), 0)
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
        .invoke_handler(tauri::generate_handler![
            list_products,
            create_product,
            update_product,
            archive_product,
            record_purchase,
            record_sale,
            record_adjustment,
            list_movements,
            get_dashboard_summary,
            get_stock_report,
            create_backup,
            restore_backup,
            get_database_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running StockLite");
}
