/**
 * Sample indexer for the ticketing contract (issue #58): polls Soroban RPC
 * `getEvents` and prints every contract event as one JSON line.
 *
 * Events are decoded with the contract spec published in each release
 * (`stellar_tickets_ticketing.spec.json`, see scripts/export-spec.sh), so the
 * indexer follows new events without code changes. See README.md in this
 * directory. Configuration via environment:
 *
 *   CONTRACT_ID      required — the deployed contract address (C...)
 *   SPEC_PATH        optional — path to the spec JSON
 *                    (default ../../target/wasm32v1-none/release/stellar_tickets_ticketing.spec.json)
 *   START_LEDGER     optional — first ledger to read; defaults to the oldest
 *                    ledger the RPC still retains
 *   SOROBAN_RPC_URL  optional — defaults to the public testnet RPC
 *   POLL_MS          optional — delay between polls (default 5000)
 */

import { readFileSync } from "node:fs";
import { rpc, scValToNative, xdr } from "@stellar/stellar-sdk";

const CONTRACT_ID = process.env.CONTRACT_ID ?? "";
if (!CONTRACT_ID) {
  throw new Error("CONTRACT_ID is required (the deployed contract address, C...)");
}
const SPEC_PATH =
  process.env.SPEC_PATH ??
  new URL(
    "../../target/wasm32v1-none/release/stellar_tickets_ticketing.spec.json",
    import.meta.url,
  ).pathname;
const RPC_URL = process.env.SOROBAN_RPC_URL ?? "https://soroban-testnet.stellar.org";
const POLL_MS = Number(process.env.POLL_MS ?? "5000");

interface EventParam {
  name: string;
  location: "topic_list" | "data";
}

interface EventSpec {
  name: string;
  prefix_topics: string[];
  params: EventParam[];
  data_format: "map" | "vec" | "single_value";
}

/** Indexes the spec's `event_v0` entries by their first topic symbol. */
function loadEventSpecs(path: string): Map<string, EventSpec> {
  const entries: Array<Record<string, unknown>> = JSON.parse(readFileSync(path, "utf8"));
  const specs = new Map<string, EventSpec>();
  for (const entry of entries) {
    const spec = entry.event_v0 as EventSpec | undefined;
    if (spec) {
      specs.set(spec.prefix_topics[0], spec);
    }
  }
  if (specs.size === 0) {
    throw new Error(`no event_v0 entries in ${path}; is this the contract spec JSON?`);
  }
  return specs;
}

/** Turns raw topics and data into `{ field: value }` using the event's spec. */
function decode(spec: EventSpec, topics: xdr.ScVal[], value: xdr.ScVal): Record<string, unknown> {
  const fields: Record<string, unknown> = {};
  const topicParams = spec.params.filter((p) => p.location === "topic_list");
  const dataParams = spec.params.filter((p) => p.location === "data");

  // Topics are the prefix symbol(s) followed by the #[topic] fields in order.
  topicParams.forEach((param, i) => {
    fields[param.name] = scValToNative(topics[spec.prefix_topics.length + i]);
  });

  const data = scValToNative(value);
  if (spec.data_format === "map") {
    for (const param of dataParams) {
      fields[param.name] = data[param.name];
    }
  } else if (spec.data_format === "vec") {
    dataParams.forEach((param, i) => {
      fields[param.name] = data[i];
    });
  } else if (dataParams.length === 1) {
    fields[dataParams[0].name] = data;
  }
  return fields;
}

/** JSON.stringify cannot encode the bigints that u64/i128 decode to. */
function toJson(record: unknown): string {
  return JSON.stringify(record, (_key, v) => (typeof v === "bigint" ? v.toString() : v));
}

async function main(): Promise<void> {
  const specs = loadEventSpecs(SPEC_PATH);
  // Plain http is only allowed for a local RPC such as `stellar container start`.
  const server = new rpc.Server(RPC_URL, { allowHttp: RPC_URL.startsWith("http://") });

  let startLedger = process.env.START_LEDGER ? Number(process.env.START_LEDGER) : undefined;
  if (startLedger === undefined) {
    // The RPC only retains a recent window of events; start at its oldest.
    startLedger = (await server.getHealth()).oldestLedger;
  }
  let cursor: string | undefined;

  for (;;) {
    const filters: rpc.Api.EventFilter[] = [{ type: "contract", contractIds: [CONTRACT_ID] }];
    const page = cursor
      ? await server.getEvents({ filters, cursor, limit: 200 })
      : await server.getEvents({ filters, startLedger, limit: 200 });

    for (const event of page.events) {
      const name = scValToNative(event.topic[0]) as string;
      const spec = specs.get(name);
      console.log(
        toJson({
          id: event.id,
          ledger: event.ledger,
          ledgerClosedAt: event.ledgerClosedAt,
          txHash: event.txHash,
          event: spec?.name ?? name,
          fields: spec ? decode(spec, event.topic, event.value) : scValToNative(event.value),
        }),
      );
    }

    cursor = page.cursor;
    if (page.events.length === 0) {
      await new Promise((resolve) => setTimeout(resolve, POLL_MS));
    }
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
