/**
 * Freighter wallet integration example for the ticketing contract (issue #54).
 *
 * Demonstrates the three-step wallet flow:
 *   1. requestAccess()  — get the user's public key from the extension
 *   2. signTransaction  — hand an unsigned XDR to Freighter for approval
 *   3. sendTransaction  — submit the signed envelope and poll for inclusion
 *
 * Requires the Freighter browser extension and a testnet account. Configure
 * CONTRACT_ID and EVENT_ID in the page before connecting.
 */

import { requestAccess, signTransaction, isConnected } from "@stellar/freighter-api";
import {
  Address,
  Contract,
  Networks,
  BASE_FEE,
  TransactionBuilder,
  nativeToScVal,
  scValToNative,
  rpc,
} from "@stellar/stellar-sdk";

const RPC_URL = "https://soroban-testnet.stellar.org";
const NETWORK_PASSPHRASE = Networks.TESTNET;

const logEl = document.getElementById("log");
const contractInput = document.getElementById("contract-id");
const eventInput = document.getElementById("event-id");
const btnConnect = document.getElementById("btn-connect");
const btnRead = document.getElementById("btn-read");
const btnBuy = document.getElementById("btn-buy");

let address = null;
let server = null;

function log(message, cls = "") {
  const line = document.createElement("div");
  if (cls) line.className = cls;
  line.textContent = message;
  logEl.appendChild(line);
  logEl.scrollTop = logEl.scrollHeight;
}

function contractId() {
  const id = contractInput.value.trim();
  if (!id) throw new Error("Set the contract ID first.");
  return id;
}

function eventId() {
  return BigInt(eventInput.value || "1");
}

function contract() {
  return new Contract(contractId());
}

async function connect() {
  btnConnect.disabled = true;
  try {
    const ok = await isConnected();
    if (!ok) {
      throw new Error("Freighter is not installed or not reachable. Install it from freighter.app.");
    }
    const access = await requestAccess();
    if (access.error) {
      throw new Error(access.error);
    }
    address = access.address;
    server = new rpc.Server(RPC_URL);
    log(`Connected: ${address}`, "ok");
    btnRead.disabled = false;
    btnBuy.disabled = false;
  } catch (err) {
    log(`Connect failed: ${err.message ?? err}`, "error");
  } finally {
    btnConnect.disabled = false;
  }
}

/** Read-only call: simulate `get_event` and decode the return value. */
async function readEvent() {
  try {
    const account = await server.getAccount(address);
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: NETWORK_PASSPHRASE,
    })
      .addOperation(contract().call("get_event", nativeToScVal(eventId(), { type: "u64" })))
      .setTimeout(60)
      .build();

    const sim = await server.simulateTransaction(tx);
    if (sim.error) {
      throw new Error(sim.error);
    }
    const event = scValToNative(sim.result[0].retval);
    log(
      `Event ${eventId()}: name=${event.name}, organizer=${event.organizer}, ` +
        `starts_at=${event.starts_at}, cancelled=${event.cancelled}`,
      "ok",
    );
  } catch (err) {
    log(`Read failed: ${err.message ?? err}`, "error");
  }
}

/**
 * Write call: build `purchase_primary`, ask Freighter to sign it, submit.
 * The user sees Freighter's approval popup during signTransaction.
 */
async function buyTicket() {
  try {
    const account = await server.getAccount(address);
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: NETWORK_PASSPHRASE,
    })
      .addOperation(
        contract().call(
          "purchase_primary",
          new Address(address).toScVal(),
          nativeToScVal(eventId(), { type: "u64" }),
          nativeToScVal("GA", { type: "string" }),
          nativeToScVal("unassigned", { type: "string" }),
        ),
      )
      .setTimeout(60)
      .build();

    // Simulate first so resource fees are accurate and failures surface
    // before the Freighter popup.
    const prepared = await server.prepareTransaction(tx);
    log("Requesting Freighter signature…");

    const signedResult = await signTransaction(prepared.toXDR(), {
      networkPassphrase: NETWORK_PASSPHRASE,
    });
    if (signedResult.error) {
      throw new Error(signedResult.error);
    }

    const signed = TransactionBuilder.fromXDR(signedResult.signedTxXdr, NETWORK_PASSPHRASE);
    const sent = await server.sendTransaction(signed);
    if (sent.status === "ERROR") {
      throw new Error(`sendTransaction error: ${JSON.stringify(sent.errorResult)}`);
    }
    log(`Submitted: ${sent.hash}`);

    let response = await server.getTransaction(sent.hash);
    while (response.status === "NOT_FOUND") {
      await new Promise((resolve) => setTimeout(resolve, 1000));
      response = await server.getTransaction(sent.hash);
    }
    if (response.status !== "SUCCESS") {
      throw new Error(`Transaction ${sent.hash} ended with status ${response.status}`);
    }
    const ticketId = scValToNative(response.result.returnValue);
    log(`Success — ticket id ${ticketId}`, "ok");
  } catch (err) {
    log(`Buy failed: ${err.message ?? err}`, "error");
  }
}

btnConnect.addEventListener("click", connect);
btnRead.addEventListener("click", readEvent);
btnBuy.addEventListener("click", buyTicket);
