export type Product = {
  id: number;
  productCode: string;
  name: string;
  unit: string;
  currentStock: number;
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
};

export type DashboardSummary = {
  activeProducts: number;
  totalUnits: number;
  lowStock: number;
  recentMovements: Movement[];
};
