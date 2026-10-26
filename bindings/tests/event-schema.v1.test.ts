/**
 * EVENT_SCHEMA v1 golden fixture integration test (Issue #716).
 *
 * Loads `bindings/tests/fixtures/event_schema_v1.json` — the checked-in golden
 * representation of `docs/EVENT_SCHEMA.md` — and asserts that the TypeScript
 * bindings expose it in a well-formed, consumer-safe shape. The fixture is the
 * machine-checkable contract indexers pin against, so structural drift in the
 * fixture itself (duplicate topics, non-contiguous positions, missing types)
 * fails here rather than silently reaching downstream consumers.
 *
 * This is a pure source-level integration load: it needs no WASM artifact and
 * always runs, mirroring the other fixture-free source checks in this suite.
 */

import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  EVENT_SCHEMA_VERSION,
  type EventSchemaFixture,
} from "../src/index.js";

const __dirname = fileURLToPath(new URL(".", import.meta.url));
const FIXTURE_PATH = resolve(
  __dirname,
  "fixtures/event_schema_v1.json",
);

function loadFixture(): EventSchemaFixture {
  const raw = readFileSync(FIXTURE_PATH, "utf8");
  return JSON.parse(raw) as EventSchemaFixture;
}

describe("EVENT_SCHEMA v1 golden fixture", () => {
  it("fixture file exists and parses as JSON", () => {
    const fixture = loadFixture();
    expect(fixture).toBeTypeOf("object");
  });

  it("declares event_schema_version matching the bindings constant", () => {
    const fixture = loadFixture();
    expect(fixture.event_schema_version).toBe(EVENT_SCHEMA_VERSION);
    expect(fixture.event_schema_version).toBe(1);
  });

  it("declares a non-empty, uniquely-keyed event list", () => {
    const fixture = loadFixture();
    expect(fixture.events.length).toBeGreaterThan(0);

    const keys = fixture.events.map((e) => `${e.namespace}.${e.action}`);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("every event has exactly two non-empty Symbol topics and a description", () => {
    for (const event of loadFixture().events) {
      expect(event.namespace.length).toBeGreaterThan(0);
      expect(event.action.length).toBeGreaterThan(0);
      // Soroban `Symbol` values are capped at 32 characters. Some short-form
      // topics fit in `symbol_short!` (≤9 chars) while others such as
      // "transition" are built with `Symbol::new`; both are valid symbols.
      expect(event.namespace.length).toBeLessThanOrEqual(32);
      expect(event.action.length).toBeLessThanOrEqual(32);
      expect(event.description.length).toBeGreaterThan(0);
    }
  });

  it("every event has at least one field with contiguous 0-indexed positions", () => {
    for (const event of loadFixture().events) {
      const label = `${event.namespace}.${event.action}`;
      expect(event.fields.length, label).toBeGreaterThan(0);

      const positions = event.fields.map((f) => f.position);
      expect(positions, label).toEqual(
        Array.from({ length: positions.length }, (_, i) => i),
      );
    }
  });

  it("every field has a name, a type, and a description", () => {
    for (const event of loadFixture().events) {
      for (const field of event.fields) {
        const label = `${event.namespace}.${event.action}[${field.position}]`;
        expect(field.name.length, label).toBeGreaterThan(0);
        expect(field.type.length, label).toBeGreaterThan(0);
        expect(field.description.length, label).toBeGreaterThan(0);
      }
    }
  });

  it("pins the core round.created event shape", () => {
    const created = loadFixture().events.find(
      (e) => e.namespace === "round" && e.action === "created",
    );
    expect(created).toBeDefined();
    expect(created!.fields.map((f) => f.name)).toEqual([
      "round_id",
      "start_price",
      "start_ledger",
      "bet_end_ledger",
      "end_ledger",
      "mode",
    ]);
    expect(created!.fields.map((f) => f.type)).toEqual([
      "u64",
      "u128",
      "u32",
      "u32",
      "u32",
      "u32",
    ]);
  });

  it("pins the canonical round.summary terminal event shape", () => {
    const summary = loadFixture().events.find(
      (e) => e.namespace === "round" && e.action === "summary",
    );
    expect(summary).toBeDefined();
    expect(summary!.fields).toHaveLength(10);
    expect(summary!.fields.map((f) => f.name)).toEqual([
      "round_id",
      "status",
      "mode",
      "price_final",
      "participant_count",
      "total_pot",
      "fee_amount",
      "settled_at_ledger",
      "confidence",
      "fee_model",
    ]);
  });

  it("the fixture is consumable through the exported EventSchemaFixture type", () => {
    const fixture: EventSchemaFixture = loadFixture();
    const roundEvents = fixture.events.filter((e) => e.namespace === "round");
    expect(roundEvents.length).toBeGreaterThanOrEqual(5);
  });
});
