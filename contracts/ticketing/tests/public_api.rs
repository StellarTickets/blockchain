use soroban_sdk::{testutils::Address as _, Address, Env, IntoVal, String, TryFromVal, Val};
use stellar_tickets_ticketing::{
    Category, DataKey, Error, Event, EventParams, GiftClaim, PendingPaymentToken, StoredTicket,
    Ticket, TicketStatus,
};

// Compile as an external consumer: moving definitions between private modules
// must preserve their crate-root exports and Soroban conversion traits.
fn contract_type<T: IntoVal<Env, Val> + TryFromVal<Env, Val>>() {}

#[test]
fn public_types_keep_their_crate_root_paths_and_ticket_encoding() {
    contract_type::<Category>();
    contract_type::<DataKey>();
    contract_type::<Event>();
    contract_type::<EventParams>();
    contract_type::<GiftClaim>();
    contract_type::<PendingPaymentToken>();
    contract_type::<StoredTicket>();
    contract_type::<Ticket>();
    contract_type::<TicketStatus>();

    let env = Env::default();
    let ticket = Ticket {
        event_id: 1,
        owner: Address::generate(&env),
        tier: String::from_str(&env, "GA"),
        seat: String::from_str(&env, "A1"),
        status: TicketStatus::Valid,
        original_price: 1_000,
        resale_price: 0,
        transfers: 0,
        purchased_at: 0,
    };
    let encoded: Val = ticket.clone().into_val(&env);
    assert_eq!(Ticket::try_from_val(&env, &encoded).unwrap(), ticket);
    assert_eq!(Error::NotOrganizer as u32, 6);
    assert_eq!(Error::AlreadyUsed as u32, 8);
    assert_eq!(Error::Revoked as u32, 9);
    assert_eq!(Error::MathOverflow as u32, 45);
    assert_eq!(Error::NotTransferable as u32, 46);
    assert_eq!(Error::MetadataUriNotSet as u32, 47);
    assert_eq!(Error::TicketNotUsed as u32, 48);
    assert_eq!(Error::RetentionPeriodNotElapsed as u32, 49);
}
