// "YYYY-MM-DD" for the local calendar day; toISOString() would give the UTC day instead.
export function localDate(date = new Date()) {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

// SQLite timestamps look like "YYYY-MM-DD HH:MM:SS" (UTC); bare dates are local calendar days.
export function formatDate(value: string) {
  const date = new Date(value.includes(" ") ? `${value.replace(" ", "T")}Z` : value.includes("T") ? value : `${value}T00:00:00`);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat("en", { month: "short", day: "numeric", year: "numeric" }).format(date);
}
