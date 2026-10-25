// SQLite timestamps look like "YYYY-MM-DD HH:MM:SS" (UTC); bare dates are local calendar days.
export function formatDate(value: string) {
  const date = new Date(value.includes(" ") ? `${value.replace(" ", "T")}Z` : value.includes("T") ? value : `${value}T00:00:00`);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat("en", { month: "short", day: "numeric", year: "numeric" }).format(date);
}
