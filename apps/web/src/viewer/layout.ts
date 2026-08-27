export type LayoutKind = "columns" | "rows" | "quad" | "grid";

export function chooseLayout(photoCount: number, width: number, height: number): LayoutKind {
  if (photoCount === 4) return "quad";
  if (photoCount === 2 || photoCount === 3) return width >= height ? "columns" : "rows";
  return "grid";
}

export function gridColumns(photoCount: number): number {
  return Math.max(1, Math.ceil(Math.sqrt(photoCount)));
}
