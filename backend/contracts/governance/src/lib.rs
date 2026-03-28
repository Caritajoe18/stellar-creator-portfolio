#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String};

#[contracttype]
pub struct GovernanceConfig {
    pub platform_fee_percent: u32,
    pub min_bounty_budget: i128,
    pub max_bounty_budget: i128,
    pub dispute_resolution_period: u64,
    pub admin_address: Address,
    pub last_updated: u64,
}

#[contracttype]
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub title: String,
    pub description: String,
    pub yes_votes: u64,
    pub no_votes: u64,
    pub approved: bool,
    pub executed: bool,
    pub created_at: u64,
    pub voting_deadline: u64,
}

#[contracttype]
pub enum DataKey {
    Config,
    ProposalCounter,
    Proposal(u64),
    Vote(u64, Address),
}

#[contract]
pub struct GovernanceContract;

#[contractimpl]
impl GovernanceContract {
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        assert!(
            !env.storage().persistent().has(&DataKey::Config),
            "Already initialized"
        );

        let config = GovernanceConfig {
            platform_fee_percent: 50,
            min_bounty_budget: 100,
            max_bounty_budget: 1_000_000,
            dispute_resolution_period: 7 * 24 * 3600,
            admin_address: admin,
            last_updated: env.ledger().timestamp(),
        };
        env.storage().persistent().set(&DataKey::Config, &config);
    }

    pub fn get_config(env: Env) -> GovernanceConfig {
        env.storage()
            .persistent()
            .get(&DataKey::Config)
            .expect("Not initialized")
    }

    pub fn set_platform_fee(env: Env, admin: Address, fee_percent: u32) -> bool {
        admin.require_auth();
        assert!(fee_percent <= 1000, "Fee cannot exceed 10%");

        let mut config: GovernanceConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .expect("Not initialized");

        assert!(admin == config.admin_address, "Only admin can update fee");

        config.platform_fee_percent = fee_percent;
        config.last_updated = env.ledger().timestamp();
        env.storage().persistent().set(&DataKey::Config, &config);
        true
    }

    pub fn set_bounty_limits(env: Env, admin: Address, min_budget: i128, max_budget: i128) -> bool {
        admin.require_auth();

        let mut config: GovernanceConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .expect("Not initialized");

        assert!(
            admin == config.admin_address,
            "Only admin can update limits"
        );
        assert!(min_budget > 0, "Min budget must be positive");
        assert!(max_budget > min_budget, "Max must be greater than min");

        config.min_bounty_budget = min_budget;
        config.max_bounty_budget = max_budget;
        config.last_updated = env.ledger().timestamp();
        env.storage().persistent().set(&DataKey::Config, &config);
        true
    }

    pub fn create_proposal(
        env: Env,
        proposer: Address,
        title: String,
        description: String,
        voting_period: u64,
    ) -> u64 {
        proposer.require_auth();

        let mut counter: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::ProposalCounter)
            .unwrap_or(0);
        counter += 1;

        let proposal = Proposal {
            id: counter,
            proposer,
            title,
            description,
            yes_votes: 0,
            no_votes: 0,
            approved: false,
            executed: false,
            created_at: env.ledger().timestamp(),
            voting_deadline: env.ledger().timestamp() + voting_period,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Proposal(counter), &proposal);
        env.storage()
            .persistent()
            .set(&DataKey::ProposalCounter, &counter);
        counter
    }

    pub fn vote(env: Env, voter: Address, proposal_id: u64, vote_yes: bool) -> bool {
        voter.require_auth();

        let vote_key = DataKey::Vote(proposal_id, voter.clone());
        assert!(!env.storage().persistent().has(&vote_key), "Already voted");

        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .expect("Proposal not found");

        assert!(!proposal.executed, "Proposal already executed");
        assert!(
            env.ledger().timestamp() < proposal.voting_deadline,
            "Voting period has ended"
        );

        if vote_yes {
            proposal.yes_votes += 1;
        } else {
            proposal.no_votes += 1;
        }

        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        env.storage().persistent().set(&vote_key, &true);
        true
    }

    pub fn get_proposal(env: Env, proposal_id: u64) -> Proposal {
        env.storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .expect("Proposal not found")
    }

    pub fn execute_proposal(env: Env, proposal_id: u64) -> bool {
        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .expect("Proposal not found");

        assert!(
            env.ledger().timestamp() >= proposal.voting_deadline,
            "Voting still in progress"
        );
        assert!(!proposal.executed, "Already executed");

        proposal.approved = proposal.yes_votes > proposal.no_votes;
        proposal.executed = true;
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn test_initialize_and_get_config() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(GovernanceContract, ());
        let client = GovernanceContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let config = client.get_config();
        assert_eq!(config.platform_fee_percent, 50);
        assert_eq!(config.admin_address, admin);
    }

    #[test]
    fn test_create_and_execute_proposal() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(GovernanceContract, ());
        let client = GovernanceContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let proposer = Address::generate(&env);
        let id = client.create_proposal(
            &proposer,
            &String::from_str(&env, "Reduce fees"),
            &String::from_str(&env, "Lower platform fee to 3%"),
            &100u64,
        );
        assert_eq!(id, 1);

        let voter = Address::generate(&env);
        client.vote(&voter, &id, &true);

        env.ledger().with_mut(|l| l.timestamp += 101);
        client.execute_proposal(&id);

        let proposal = client.get_proposal(&id);
        assert!(proposal.approved);
        assert!(proposal.executed);
    }
}
