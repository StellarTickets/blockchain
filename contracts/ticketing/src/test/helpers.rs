use super::*;

pub struct TestContext<'a> {
    pub env: Env,
    pub client: TicketingContractClient<'a>,
    pub token: TokenClient<'a>,
    pub token_asset: StellarAssetClient<'a>,
    pub admin: Address,
    pub organizer: Address,
}

pub fn setup<'a>() -> (
    Env,
    TicketingContractClient<'a>,
    TokenClient<'a>,
    StellarAssetClient<'a>,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let organizer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = TokenClient::new(&env, &token_contract.address());
    let token_asset = StellarAssetClient::new(&env, &token_contract.address());

    let contract_id = env.register(TicketingContract, ());
    let client = TicketingContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token_contract.address());
    // Event creation is restricted to allowlisted organizers (issue #135).
    client.approve_organizer(&admin, &organizer);

    (env, client, token, token_asset, admin, organizer)
}

pub fn setup_uninitialized<'a>() -> (Env, TicketingContractClient<'a>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let organizer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);

    let contract_id = env.register(TicketingContract, ());
    let client = TicketingContractClient::new(&env, &contract_id);

    (env, client, token_contract.address(), admin, organizer)
}

pub fn make_event(env: &Env, client: &TicketingContractClient, organizer: &Address, event_id: u64) {
    client.create_event(
        organizer,
        &event_id,
        &EventParams {
            name: String::from_str(env, "Radiohead Live"),
            category: String::from_str(env, "concert"),
            max_resale_multiplier_bps: 12_000, // max 120% of face value on resale
            royalty_bps: 500,                  // 5% organizer royalty
            starts_at: 10_000,
            transfer_freeze_seconds: 100,
            resale_cutoff_seconds: 200,
            max_per_wallet: None,
            refund_window_seconds: None,
            presale_starts_at: None,
            presale_ends_at: None,
        },
    );
}

pub fn make_custom_event(
    env: &Env,
    client: &TicketingContractClient,
    organizer: &Address,
    event_id: u64,
    name: &str,
    category: &str,
    max_resale_multiplier_bps: u32,
    royalty_bps: u32,
    starts_at: u64,
) {
    client.create_event(
        organizer,
        &event_id,
        &EventParams {
            name: String::from_str(env, name),
            category: String::from_str(env, category),
            max_resale_multiplier_bps,
            royalty_bps,
            starts_at,
            transfer_freeze_seconds: 100,
            resale_cutoff_seconds: 200,
            max_per_wallet: None,
            refund_window_seconds: None,
            presale_starts_at: None,
            presale_ends_at: None,
        },
    );
}

pub fn mint_tokens(token_asset: &StellarAssetClient, recipient: &Address, amount: i128) {
    token_asset.mint(recipient, &amount);
}

pub fn issue_sample_ticket(
    env: &Env,
    client: &TicketingContractClient,
    organizer: &Address,
    event_id: u64,
    buyer: &Address,
    price: i128,
) -> u64 {
    client.issue_ticket(
        organizer,
        &event_id,
        buyer,
        &String::from_str(env, "GA"),
        &String::from_str(env, "unassigned"),
        &price,
    )
}
