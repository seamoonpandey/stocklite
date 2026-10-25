import { invoke } from "@tauri-apps/api/core";
import type { DashboardSummary, Movement, Product } from "./types";

const demoProducts: Product[] = [
  { id: 1, productCode: "STK-0001", name: "Arabica Coffee Beans", unit: "bags", currentStock: 42, isActive: true },
  { id: 2, productCode: "STK-0002", name: "Ceramic Mugs", unit: "pieces", currentStock: 128, isActive: true },
  { id: 3, productCode: "STK-0003", name: "Paper Filters", unit: "boxes", currentStock: 16, isActive: true },
];

const demoMovements: Movement[] = [
  { id: 1, productId: 2, productName: "Ceramic Mugs", movementType: "purchase", quantityDelta: 48, occurredAt: new Date().toISOString(), note: "Weekly supplier delivery" },
  { id: 2, productId: 1, productName: "Arabica Coffee Beans", movementType: "sale", quantityDelta: -8, occurredAt: new Date(Date.now() - 86400000).toISOString() },
  { id: 3, productId: 3, productName: "Paper Filters", movementType: "adjustment", quantityDelta: -2, occurredAt: new Date(Date.now() - 172800000).toISOString(), note: "Damaged stock" },
];

async function call<T>(command: string, args: Record<string, unknown>, fallback: T): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
    if (isTauri) throw error;
    return fallback;
  }
}

export const listProducts = (search = "") =>
  call("list_products", { search }, demoProducts.filter((product) =>
    `${product.name} ${product.productCode}`.toLowerCase().includes(search.toLowerCase()),
  ));

export const listMovements = () => call("list_movements", {}, demoMovements);

export const getDashboardSummary = () =>
  call<DashboardSummary>("get_dashboard_summary", {}, {
    activeProducts: demoProducts.length,
    totalUnits: demoProducts.reduce((total, product) => total + product.currentStock, 0),
    lowStock: 1,
    recentMovements: demoMovements,
  });

export const createProduct = (name: string, unit: string, openingQuantity: number) =>
  call<Product>("create_product", { name, unit, openingQuantity }, {
    id: Date.now(), productCode: `STK-${String(Date.now()).slice(-4)}`, name, unit, currentStock: openingQuantity, isActive: true,
  });

export const recordMovement = (command: "record_purchase" | "record_sale" | "record_adjustment", productId: number, quantity: number, note: string) =>
  call<Product>(command, { productId, quantity, note }, demoProducts.find((product) => product.id === productId) ?? demoProducts[0]);
