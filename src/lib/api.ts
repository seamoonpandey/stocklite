import { invoke } from "@tauri-apps/api/core";
import type { DashboardSummary, Movement, Product, StockReportRow } from "./types";

const demoProducts: Product[] = [
  { id: 1, productCode: "STK-0001", name: "Arabica Coffee Beans", unit: "bags", currentStock: 42, reorderThreshold: 10, isActive: true },
  { id: 2, productCode: "STK-0002", name: "Ceramic Mugs", unit: "pieces", currentStock: 128, reorderThreshold: null, isActive: true },
  { id: 3, productCode: "STK-0003", name: "Paper Filters", unit: "boxes", currentStock: 16, reorderThreshold: 20, isActive: true },
];

const demoMovements: Movement[] = [
  { id: 1, productId: 2, productName: "Ceramic Mugs", movementType: "purchase", quantityDelta: 48, occurredAt: new Date().toISOString(), createdAt: new Date().toISOString(), note: "Weekly supplier delivery" },
  { id: 2, productId: 1, productName: "Arabica Coffee Beans", movementType: "sale", quantityDelta: -8, occurredAt: new Date(Date.now() - 86400000).toISOString(), createdAt: new Date().toISOString() },
];

function isTauri() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function call<T>(command: string, args: Record<string, unknown>, fallback: T): Promise<T> {
  if (!isTauri()) return fallback;
  return invoke<T>(command, args);
}

export const listProducts = (search = "", includeInactive = false) =>
  call<Product[]>("list_products", { search, includeInactive }, demoProducts.filter((product) =>
    `${product.name} ${product.productCode}`.toLowerCase().includes(search.toLowerCase()),
  ));

export const listMovements = (filters: {
  productId?: number;
  movementType?: string;
  fromDate?: string;
  toDate?: string;
} = {}) => call<Movement[]>("list_movements", {
productId: filters.productId ?? null,
movementType: filters.movementType || null,
fromDate: filters.fromDate || null,
toDate: filters.toDate || null,
}, demoMovements);

export const getDashboardSummary = () =>
  call<DashboardSummary>("get_dashboard_summary", {}, {
    activeProducts: demoProducts.length,
    totalUnits: demoProducts.reduce((total, product) => total + product.currentStock, 0),
    lowStock: 1,
    recentMovements: demoMovements,
  });

export const createProduct = (input: {
  name: string;
  unit: string;
  openingQuantity: number;
  reorderThreshold: number | null;
}) =>
  call<Product>("create_product", {
    name: input.name,
    unit: input.unit,
    openingQuantity: input.openingQuantity,
    reorderThreshold: input.reorderThreshold,
  }, {
    id: Date.now(), productCode: `STK-${String(Date.now()).slice(-4)}`, name: input.name, unit: input.unit,
    currentStock: input.openingQuantity, reorderThreshold: input.reorderThreshold, isActive: true,
  });

export const updateProduct = (input: {
  productId: number;
  name: string;
  unit: string;
  reorderThreshold: number | null;
}) =>
  call<Product>("update_product", {
    productId: input.productId,
    name: input.name,
    unit: input.unit,
    reorderThreshold: input.reorderThreshold,
  }, {
    ...demoProducts.find((product) => product.id === input.productId) ?? demoProducts[0],
    name: input.name,
    unit: input.unit,
    reorderThreshold: input.reorderThreshold,
  });

export const archiveProduct = (productId: number) => call<void>("archive_product", { productId }, undefined);
export const unarchiveProduct = (productId: number) => call<void>("unarchive_product", { productId }, undefined);
export const deleteProduct =(productId: number) => call<void>("delete_product", { productId }, undefined);

export const recordMovement = (type: "purchase" | "sale" | "adjustment", input: {
  productId: number;
  quantity: number;
  occurredAt: string;
  note: string;
}) =>
  call<Product>(`record_${type}`, {
    input: {
      productId: input.productId,
      quantity: input.quantity,
      occurredAt: input.occurredAt,
      note: input.note || null,
    },
  }, demoProducts.find((product) => product.id === input.productId) ?? demoProducts[0]);

export const getStockReport = (fromDate: string, toDate: string) =>
  call<StockReportRow[]>("get_stock_report", { fromDate, toDate }, []);

export const createBackup = (destination: string) => call<string>("create_backup", { destination }, destination);
export const restoreBackup = (source: string) => call<void>("restore_backup", { source }, undefined);
export const getDatabasePath = () => call<string>("get_database_path", {}, "Browser preview (no local database)");
