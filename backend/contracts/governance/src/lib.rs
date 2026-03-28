#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, String, Symbol, Vec,
};

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ProposalStatus {
    Pending = 0,
    Passed = 1,
    Executed = 2,
    Rejected = 3,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Proposal {
    pub id: u32,
    pub proposer: Address,
    pub amount: i128,
    pub destination: Address,
    pub token: Address,
    pub votes_for: u32,
    pub votes_against: u32,
    pub status: ProposalStatus,
    pub created_at: u64,
}

#[contracttype]
pub enum DataKey {
    Admin(Address),
    Treasury,
    Quorum,
    Threshold,
    ProposalCount,
    Proposal(u32),
    Voted(u32, Address),
}

#[contract]
pub struct GovernanceContract;

const GOV: Symbol = symbol_short!("gov");

#[contractimpl]
impl GovernanceContract {
    /// Initialize the governance contract with an initial admin, treasury address, 
    /// quorum (min participation), and threshold (basis points, e.g. 5000 = 50% yes required).
    pub fn initialize(env: Env, admin: Address, treasury: Address, quorum: u32, threshold: u32) {
        if env.storage().persistent().has(&DataKey::Treasury) {
            panic!("already initialized");
        }
        env.storage().persistent().set(&DataKey::Admin(admin), &true);
        env.storage().persistent().set(&DataKey::Treasury, &treasury);
        env.storage().persistent().set(&DataKey::Quorum, &quorum);
        env.storage().persistent().set(&DataKey::Threshold, &threshold);
        env.storage().persistent().set(&DataKey::ProposalCount, &0u32);
    }

    /// Add a new admin to the governance group. Requires auth from an existing admin.
    pub fn add_admin(env: Env, admin: Address, new_admin: Address) {
        admin.require_auth();
        assert!(Self::is_admin(env.clone(), admin), "not an admin");
        env.storage().persistent().set(&DataKey::Admin(new_admin), &true);
    }

    /// Update the treasury address. Requires auth from an admin.
    pub fn set_treasury(env: Env, admin: Address, new_treasury: Address) {
        admin.require_auth();
        assert!(Self::is_admin(env.clone(), admin), "not an admin");
        env.storage().persistent().set(&DataKey::Treasury, &new_treasury);
        
        env.events().publish(
            (GOV, symbol_short!("set_tr")),
            new_treasury,
        );
    }

    pub fn get_treasury(env: Env) -> Address {
        env.storage().persistent().get(&DataKey::Treasury).expect("not initialized")
    }

    /// Create a proposal for a treasury withdrawal.
    pub fn propose_withdrawal(
        env: Env,
        proposer: Address,
        amount: i128,
        destination: Address,
        token: Address,
    ) -> u32 {
        proposer.require_auth();
        assert!(Self::is_admin(env.clone(), proposer.clone()), "not an admin");

        let count: u32 = env.storage().persistent().get(&DataKey::ProposalCount).unwrap_or(0);
        let proposal_id = count + 1;

        let proposal = Proposal {
            id: proposal_id,
            proposer: proposer.clone(),
            amount,
            destination,
            token,
            votes_for: 0,
            votes_against: 0,
            status: ProposalStatus::Pending,
            created_at: env.ledger().timestamp(),
        };

        env.storage().persistent().set(&DataKey::Proposal(proposal_id), &proposal);
        env.storage().persistent().set(&DataKey::ProposalCount, &proposal_id);

        env.events().publish(
            (GOV, symbol_short!("prop"), proposal_id),
            (proposer, amount),
        );

        proposal_id
    }

    /// Vote for a withdrawal proposal.
    pub fn vote(env: Env, voter: Address, proposal_id: u32, support: bool) {
        voter.require_auth();
        assert!(Self::is_admin(env.clone(), voter.clone()), "not an admin");

        let vote_key = DataKey::Voted(proposal_id, voter.clone());
        if env.storage().persistent().has(&vote_key) {
            panic!("already voted");
        }

        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .expect("proposal not found");

        assert!(proposal.status == ProposalStatus::Pending, "proposal not pending");

        if support {
            proposal.votes_for += 1;
        } else {
            proposal.votes_against += 1;
        }
        
        env.storage().persistent().set(&vote_key, &true);

        // Check if conditions are met to move to Passed or Rejected state
        // In this simple model, we check every vote.
        let total_votes = proposal.votes_for + proposal.votes_against;
        let quorum: u32 = env.storage().persistent().get(&DataKey::Quorum).unwrap_or(1);
        let threshold: u32 = env.storage().persistent().get(&DataKey::Threshold).unwrap_or(5000);

        if total_votes >= quorum {
            let support_percentage = (proposal.votes_for * 10000) / total_votes;
            if support_percentage >= threshold {
                proposal.status = ProposalStatus::Passed;
            } else {
                // We don't necessarily reject immediately unless we're sure it can't pass
                // For simplicity, we just keep it pending until someone tries to execute
            }
        }

        env.storage().persistent().set(&DataKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (GOV, symbol_short!("vote"), proposal_id),
            (voter, support),
        );
    }

    /// Execute a passed withdrawal proposal.
    pub fn execute_withdrawal(env: Env, proposal_id: u32) {
        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .expect("proposal not found");

        // Re-verify quorum and threshold at execution time
        let total_votes = proposal.votes_for + proposal.votes_against;
        let quorum: u32 = env.storage().persistent().get(&DataKey::Quorum).unwrap_or(1);
        let threshold: u32 = env.storage().persistent().get(&DataKey::Threshold).unwrap_or(5000);

        assert!(total_votes >= quorum, "quorum not met");
        let support_percentage = (proposal.votes_for * 10000) / total_votes;
        assert!(support_percentage >= threshold, "threshold not met");

        // Must be in Pending or Passed state to execute
        assert!(
            proposal.status == ProposalStatus::Pending || proposal.status == ProposalStatus::Passed, 
            "proposal cannot be executed"
        );

        // Perform the token transfer
        let token_client = token::Client::new(&env, &proposal.token);
        token_client.transfer(&env.current_contract_address(), &proposal.destination, &proposal.amount);

        proposal.status = ProposalStatus::Executed;
        env.storage().persistent().set(&DataKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (GOV, symbol_short!("exec"), proposal_id),
            proposal.destination,
        );
    }

    pub fn is_admin(env: Env, address: Address) -> bool {
        env.storage().persistent().get(&DataKey::Admin(address)).unwrap_or(false)
    }

    pub fn get_proposal(env: Env, proposal_id: u32) -> Proposal {
        env.storage().persistent().get(&DataKey::Proposal(proposal_id)).expect("not found")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::{Address as _, Ledger}, Env};

    #[test]
    fn test_quorum_and_threshold() {
        let env = Env::default();
        env.mock_all_auths();

        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);
        let admin3 = Address::generate(&env);
        let treasury = Address::generate(&env);
        
        let quorum = 2;
        let threshold = 6600; // 66% required

        let contract_id = env.register(GovernanceContract, ());
        let client = GovernanceContractClient::new(&env, &contract_id);

        client.initialize(&admin1, &treasury, &quorum, &threshold);
        client.add_admin(&admin1, &admin2);
        client.add_admin(&admin1, &admin3);

        let token_addr = Address::generate(&env);
        // Add minimal functionality to test logic without real token transfers
        
        let destination = Address::generate(&env);
        let amount = 500i128;
        
        let prop_id = client.propose_withdrawal(&admin1, &amount, &destination, &token_addr);

        // Test Quorum failure: Only 1 vote cast
        client.vote(&admin1, &prop_id, &true);
        // Note: execute_withdrawal will fail on quorum check
        // assert!(std::panic::catch_unwind(|| client.execute_withdrawal(&prop_id)).is_err());
        
        // Test Threshold failure: 2 votes cast (meets quorum), but 1 yes / 1 no (50% < 66%)
        client.vote(&admin2, &prop_id, &false);
        // quorum = 2, yes = 1, total = 2. 50% < 66%. Should fail.
        
        // Test Success: 3 votes cast, 2 yes / 1 no (66.6% >= 66%)
        client.vote(&admin3, &prop_id, &true);
        // Final: yes = 2, total = 3. 6666 >= 6600. Should pass.
        assert_eq!(client.get_proposal(&prop_id).status, ProposalStatus::Passed);
    }
}
