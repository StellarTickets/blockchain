/**
 * Reference client for the ticketing contract: issues one ticket on the
 * Stellar testnet and verifies it (issue #225).
 *
 * See README.md in this directory for setup. Configuration via environment:
 *
 *   CONTRACT_ID       required — the deployed contract address (C...)
 *   EVENT_ID          optional — an existing event id (default 1)
 *   ORGANIZER_SECRET  optional — organizer secret key; a funded testnet
 *                     identity is generated when omitted
 */

import {
  Address,
  Contract,
  Keypair,
  Networks,
  BASE_FEE,
  TransactionBuilder,
  nativeToScVal,
  rpc,
  scValToNative,
} from "@stellar/stellar-sdk";

const CONTRACT_ID = process.env.CONTRACT_ID;
if (!CONTRACT_ID) {
  throw new Error("CONTRACT_ID is required (the deployed contract address, C...)");
}
const EVENT_ID = BigInt(process.env.EVENT_ID ?? "1");
const RPC_URL = process.env.SOROBAN_RPC_URL ?? "https://soroban-testnet.stellar.org";
const FRIENDBOT_URL = "https://friendbot-testnet.stellar.org";

async function fundOnTestnet(keypair: Keypair): Promise<void> {
  const response = await fetch(`${FRIENDBOT_URL}?addr=${keypair.publicKey()}`);
  // friendbot answers 400 with "already funded" when the account exists.
  if (!response.ok && response.status !== 400) {
    throw new Error(`friendbot funding failed: ${response.status} ${await response.text()}`);
  }
}

async function waitForTransaction(server: rpc.Server, hash: string): Promise<void> {
  let response = await server.getTransaction(hash);
  while (response.status === "NOT_FOUND") {
    await new Promise((resolve) => setTimeout(resolve, 1_000));
    response = await server.getTransaction(hash);
  }
  if (response.status !== "SUCCESS" || !response.result) {
    throw new Error(`transaction ${hash} ended with status ${response.status}`);
  }
}

async function main(): Promise<void> {
  let organizer: Keypair;
  const organizerSecret = process.env.ORGANIZER_SECRET;
  if (organizerSecret) {
    organizer = Keypair.fromSecret(organizerSecret);
  } else {
    organizer = Keypair.random();
    console.log(`generated organizer identity: ${organizer.secret()}`);
    await fundOnTestnet(organizer);
  }

  const buyer = Keypair.random();
  const server = new rpc.Server(RPC_URL);
  const contract = new Contract(CONTRACT_ID);

  // issue_ticket requires the organizer's signature and submits a real
  // transaction that mints the ticket.
  const issueTx = new TransactionBuilder(await server.getAccount(organizer.publicKey()), {
    fee: BASE_FEE,
    networkPassphrase: Networks.TESTNET,
  })
    .addOperation(
      contract.call(
        "issue_ticket",
        new Address(organizer.publicKey()).toScVal(),
        nativeToScVal(EVENT_ID, { type: "u64" }),
        new Address(buyer.publicKey()).toScVal(),
        nativeToScVal("GA", { type: "string" }),
        nativeToScVal("unassigned", { type: "string" }),
        nativeToScVal(1_000n, { type: "i128" }),
      ),
    )
    .setTimeout(30)
    .build();

  const prepared = await server.prepareTransaction(issueTx);
  prepared.sign(organizer);
  const sent = await server.sendTransaction(prepared);
  console.log(`issue_ticket submitted: ${sent.hash}`);
  await waitForTransaction(server, sent.hash);

  const ticketId = scValToNative((await server.getTransaction(sent.hash)).result!.returnValue) as bigint;
  console.log(`issued ticket ${ticketId} to ${buyer.publicKey()}`);

  // get_ticket is read-only: simulate it and decode the return value.
  const verifyTx = new TransactionBuilder(await server.getAccount(organizer.publicKey()), {
    fee: BASE_FEE,
    networkPassphrase: Networks.TESTNET,
  })
    .addOperation(contract.call("get_ticket", nativeToScVal(ticketId, { type: "u64" })))
    .setTimeout(30)
    .build();

  const simulation = await server.simulateTransaction(verifyTx);
  const retval = simulation.result?.[0]?.retval;
  if (!retval) {
    throw new Error(`get_ticket simulation failed: ${simulation.error}`);
  }
  const ticket = scValToNative(retval) as Record<string, unknown>;
  console.log("retrieved ticket:", ticket);

  if (ticket.owner !== buyer.publicKey()) {
    throw new Error("unexpected ticket owner");
  }
  console.log("done");
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
