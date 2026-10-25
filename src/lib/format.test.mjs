process.env.TZ = "Asia/Kathmandu";

import assert from "node:assert/strict";
import test from "node:test";
import { formatDate, localDate } from "./format.ts";

test("localDate uses the local calendar day, not the UTC one", () => {
  assert.equal(localDate(new Date("2026-10-25T20:00:00Z")), "2026-10-26");
  assert.equal(localDate(new Date(2026, 9, 1)), "2026-10-01");
});

test("formats bare dates as local calendar days", () => {
  assert.equal(formatDate("2026-10-25"), "Oct 25, 2026");
});

test("formats SQLite UTC timestamps without throwing", () => {
  assert.equal(formatDate("2026-10-25 10:18:23"), "Oct 25, 2026");
  assert.equal(formatDate("2026-10-25 23:30:00"), "Oct 26, 2026");
});

test("formats ISO timestamps", () => {
  assert.equal(formatDate("2026-10-25T10:18:23.000Z"), "Oct 25, 2026");
});

test("returns unparseable values unchanged instead of throwing", () => {
  assert.equal(formatDate("not a date"), "not a date");
});
