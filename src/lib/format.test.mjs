process.env.TZ = "Asia/Kathmandu";

import assert from "node:assert/strict";
import test from "node:test";
import { formatDate } from "./format.ts";

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
