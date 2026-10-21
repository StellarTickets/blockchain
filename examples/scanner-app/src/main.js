/**
 * Gate-scanner example for the ticketing contract (issue #56).
 *
 * Verifies a ticket by simulating a read-only `get_ticket` (or the
 * deprecated `verify_ticket` alias) against the deployed contract on
 * Stellar testnet. No signature or fee is needed for the read.
 *
 * A production scanner would pair this with a QR/camera input and, when
 * the gate device holds the organizer's or a delegated check-in signer's
 * key, submit a signed `check_in` — see the README for that flow.
 */

import {
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

const contractInput = document.getElementById("contract-id");
const ticketInput = document.getElementById("ticket-id");
const btnVerify = document.getElementById("btn-verify");
const resultEl = document.getElementById("result");
const verdictEl = document.getElementById("verdict");
const detailEl = document.getElementById("detail");

let server = null;

function log(message, cls = "") {
  verdictEl.textContent = message;
  detailEl.textContent = "";
  resultEl.className = cls || "unknown";
}

function showDetail(text) {
  detailEl.textContent = text;
}

function contractId() {
  const id = contractInput.value.trim();
  if (!id) throw new Error("Enter the contract ID first.");
  return id;
}

function ticketId() {
  const raw = ticketInput.value.trim();
  if (!raw) throw new Error("Enter a ticket ID first.");
  return BigInt(raw);
}

/**
 * Simulate a read-only `get_ticket` and decode the returned Ticket struct.
 * `verify_ticket` is a deprecated alias with identical behaviour; new code
 * should call `get_ticket`.
 */
async function verifyTicket(id) {
  if (!server) server = new rpc.Server(RPC_URL);

  // Any funded or unfunded account address can source a simulation; use the
  // contract's own admin placeholder is unnecessary — RPC only needs a valid
  // public key to build the envelope. For simplicity use a well-known
  // testnet account or ask the operator for a source. Here we reuse a
  // constant fee-bump-free source derived from the ticket id space.
  // In production, source with the scanner's own funded key.
  const source = process_source();
  const account = await server.getAccount(source);

  const contract = new Contract(contractId());
  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(contract.call("get_ticket", nativeToScVal(id, { type: "u64" })))
    .setTimeout(30)
    .build();

  const sim = await server.simulateTransaction(tx);
  if (sim.error) throw new Error(sim.error);
  if (!sim.result?.[0]?.retval) throw new Error("Simulation returned no value.");
  return scValToNative(sim.result[0].retval);
}

/**
 * The source account for the simulation envelope. A read-only simulation
 * does not charge fees or require auth, but the RPC still needs a valid
 * public key to build the envelope. Set one once via localStorage, or the
 * operator can paste their scanner key below. Any funded or unfunded
 * ed25519 public key in the G... format works for a pure read.
 */
function process_source() {
  const fromStorage = localStorage.getItem("scanner_source");
  if (fromStorage) return fromStorage;
  throw new Error(
    "No scanner source account set. Call setSource('G...') in the console " +
      "with any valid Stellar public key (the scanner's own account), then retry.",
  );
}

// Expose a one-liner for the operator to set the source once.
window.setSource = (gKey) => {
  localStorage.setItem("scanner_source", gKey);
  console.log(`Scanner source set to ${gKey}`);
};

/** Map a decoded Ticket to an admit / deny verdict for the gate UI. */
function verdictFor(ticket) {
  switch (ticket.status) {
    case "Valid":
      return { verdict: "ADMIT", cls: "valid" };
    case "Used":
      return { verdict: "ALREADY USED", cls: "used" };
    case "Resale":
      return { verdict: "LISTED FOR RESALE", cls: "used" };
    case "Revoked":
      return { verdict: "REVOKED — DENY", cls: "denied" };
    case "Refunded":
      return { verdict: "REFUNDED — DENY", cls: "denied" };
    default:
      return { verdict: `UNKNOWN STATUS: ${ticket.status}`, cls: "denied" };
  }
}

async function verify() {
  btnVerify.disabled = true;
  log("Verifying…");
  try {
    const ticket = await verifyTicket(ticketId());
    const { verdict, cls } = verdictFor(ticket);
    log(verdict, cls);
    showDetail(
      [
        `ticket_id: ${ticketId()}`,
        `event_id:  ${ticket.event_id}`,
        `owner:     ${ticket.owner}`,
        `tier:      ${ticket.tier}`,
        `seat:      ${ticket.seat}`,
        `status:    ${ticket.status}`,
        `transfers: ${ticket.transfers}`,
        `price:     ${ticket.original_price}`,
      ].join("\n"),
    );
  } catch (err) {
    const message = err.message ?? String(err);
    if (message.includes("TicketNotFound") || message.includes("Error(Storage")) {
      log("TICKET NOT FOUND OR ARCHIVED", "denied");
      showDetail(
        "The id does not exist, or its persistent entry has lapsed its TTL " +
          "and been archived. See docs/STORAGE_TTL.md.",
      );
    } else {
      log("VERIFY FAILED", "denied");
      showDetail(message);
    }
  } finally {
    btnVerify.disabled = false;
  }
}

btnVerify.addEventListener("click", verify);
ticketInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") verify();
});
