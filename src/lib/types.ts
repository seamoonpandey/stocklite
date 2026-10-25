export type Product = {
  id: number;
  productCode: string;
  name: string;
  unit: string;
  currentStock: number;
  reorderThreshold: number | null;
  isActive: boolean;
};

export type Movement = {
  id: number;
  productId: number;
  productName: string;
  movementType: "opening" | "purchase" | "sale" | "adjustment";
  quantityDelta: number;
  occurredAt: string;
  note?: string;
  createdAt: string;
};

export type StockReportRow = {
  productId: number;
  productName: string;
  unit: string;
  openingBalance: number;
  purchases: number;
  sales: number;
  adjustments: number;
  closingBalance: number;
};

export type DashboardSummary = {
  activeProducts: number;
  totalUnits: number;
  lowStock: number;
  recentMovements: Movement[];
};
