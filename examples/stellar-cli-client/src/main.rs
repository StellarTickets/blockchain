//! Example client for the ticketing contract (issue #226).
//!
//! A thin wrapper around the stellar CLI (the only dependency: the `stellar`
//! binary must be on `PATH`) that runs on the Stellar testnet:
//!
//! 1. submits `issue_ticket` with the given identity as organizer,
//! 2. reads the ticket back with a simulated `get_ticket` call
//!    (`--send no`).
//!
//! Usage:
//!
//! ```text
//! cargo run --manifest-path examples/stellar-cli-client/Cargo.toml -- \
//!   <CONTRACT_ID> <organizer-identity> [EVENT_ID] [network]
//! ```
//!
//! See `scripts/examples/README.md` for the deployment prerequisites.

use std::process::Command;

/// Runs the stellar CLI and returns its trimmed stdout.
fn run_stellar(args: &[String]) -> String {
    let output = Command::new("stellar")
        .args(args)
        .output()
        .expect("failed to run the stellar CLI; is it installed?");

    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        panic!("stellar command failed: stellar {}", args.join(" "));
    }

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 || args.len() > 4 {
        eprintln!(
            "usage: stellar-cli-client <contract-id> <organizer-identity> [event-id] [network]"
        );
        std::process::exit(2);
    }

    let contract_id = args[0].clone();
    let organizer = args[1].clone();
    let event_id = args.get(2).map(String::as_str).unwrap_or("1").to_string();
    let network = args.get(3).map(String::as_str).unwrap_or("testnet").to_string();

    // The demo tickets itself: resolve the organizer identity to a public key
    // so the ticket has a concrete owner address.
    let buyer = run_stellar(&[
        "keys".to_string(),
        "address".to_string(),
        organizer.clone(),
    ]);
    println!("issuing ticket on {network} for event {event_id} to {buyer}");

    let issue_args = vec![
        "contract".to_string(),
        "invoke".to_string(),
        "--id".to_string(),
        contract_id.clone(),
        "--source".to_string(),
        organizer.clone(),
        "--network".to_string(),
        network.clone(),
        "--".to_string(),
        "issue_ticket".to_string(),
        "--organizer".to_string(),
        organizer.clone(),
        "--event-id".to_string(),
        event_id.clone(),
        "--to".to_string(),
        buyer.clone(),
        "--tier".to_string(),
        "GA".to_string(),
        "--seat".to_string(),
        "unassigned".to_string(),
        "--price".to_string(),
        "1000".to_string(),
    ];

    let issued = run_stellar(&issue_args);
    println!("issue_ticket -> {issued}");

    let verify_args = vec![
        "contract".to_string(),
        "invoke".to_string(),
        "--id".to_string(),
        contract_id,
        "--source".to_string(),
        organizer,
        "--network".to_string(),
        network,
        "--send".to_string(),
        "no".to_string(),
        "--".to_string(),
        "get_ticket".to_string(),
        "--ticket-id".to_string(),
        issued,
    ];

    let ticket = run_stellar(&verify_args);
    println!("get_ticket -> {ticket}");
}
