#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Vec};

#[derive(Clone, Copy, PartialEq)]
#[contracttype]
pub enum BountyStatus {
    Open = 0,
    InProgress = 1,
    Completed = 2,
    Disputed = 3,
    Cancelled = 4,
}

#[contracttype]
pub struct Bounty {
    pub id: u64,
    pub creator: Address,
    pub title: String,
    pub description: String,
    pub budget: i128,
    pub deadline: u64,
    pub status: BountyStatus,
    pub created_at: u64,
}

#[contracttype]
pub struct BountyApplication {
    pub id: u64,
    pub bounty_id: u64,
    pub freelancer: Address,
    pub proposal: String,
    pub proposed_budget: i128,
    pub timeline: u64,
    pub created_at: u64,
}

#[contracttype]
pub enum DataKey {
    BountyCounter,
    AppCounter,
    Bounty(u64),
    Application(u64),
    SelectedFreelancer(u64),
    BountyApplications(u64), // Maps bounty_id -> Vec<application_id>
}

#[contracttype]
#[derive(Clone)]
pub struct ApplicationPage {
    pub application_ids: Vec<u64>,
    pub total: u64,
    pub has_more: bool,
}

#[contract]
pub struct BountyContract;

#[contractimpl]
impl BountyContract {
    pub fn create_bounty(
        env: Env,
        creator: Address,
        title: String,
        description: String,
        budget: i128,
        deadline: u64,
    ) -> u64 {
        creator.require_auth();

        let mut counter: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::BountyCounter)
            .unwrap_or(0);
        counter += 1;

        let bounty = Bounty {
            id: counter,
            creator,
            title,
            description,
            budget,
            deadline,
            status: BountyStatus::Open,
            created_at: env.ledger().timestamp(),
        };

        env.storage().persistent().set(&DataKey::Bounty(counter), &bounty);
        env.storage().persistent().set(&DataKey::BountyCounter, &counter);

        counter
    }

    pub fn get_bounty(env: Env, bounty_id: u64) -> Bounty {
        env.storage()
            .persistent()
            .get(&DataKey::Bounty(bounty_id))
            .expect("Bounty not found")
    }

    pub fn apply_for_bounty(
        env: Env,
        bounty_id: u64,
        freelancer: Address,
        proposal: String,
        proposed_budget: i128,
        timeline: u64,
    ) -> u64 {
        freelancer.require_auth();

        let mut counter: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::AppCounter)
            .unwrap_or(0);
        counter += 1;

        let application = BountyApplication {
            id: counter,
            bounty_id,
            freelancer,
            proposal,
            proposed_budget,
            timeline,
            created_at: env.ledger().timestamp(),
        };

        env.storage().persistent().set(&DataKey::Application(counter), &application);
        env.storage().persistent().set(&DataKey::AppCounter, &counter);

        // Track application ID under the bounty
        let mut app_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::BountyApplications(bounty_id))
            .unwrap_or(Vec::new(&env));
        app_ids.push_back(counter);
        env.storage()
            .persistent()
            .set(&DataKey::BountyApplications(bounty_id), &app_ids);

        counter
    }

    pub fn get_application(env: Env, application_id: u64) -> BountyApplication {
        env.storage()
            .persistent()
            .get(&DataKey::Application(application_id))
            .expect("Application not found")
    }

    /// List application IDs for a bounty with pagination.
    /// Returns a page of application IDs, total count, and whether there are more.
    pub fn get_bounty_applications(
        env: Env,
        bounty_id: u64,
        offset: u32,
        limit: u32,
    ) -> ApplicationPage {
        let all_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::BountyApplications(bounty_id))
            .unwrap_or(Vec::new(&env));

        let total: u64 = all_ids.len() as u64;
        let start_u32 = offset.min(total as u32);
        let end_u32 = (offset.saturating_add(limit)).min(total as u32);

        let mut page_ids = Vec::new(&env);
        let mut i = start_u32;
        while i < end_u32 {
            page_ids.push_back(all_ids.get(i).unwrap());
            i += 1;
        }

        ApplicationPage {
            application_ids: page_ids,
            total,
            has_more: (end_u32 as u64) < total,
        }
    }

    pub fn select_freelancer(env: Env, bounty_id: u64, application_id: u64) -> bool {
        let mut bounty: Bounty = env
            .storage()
            .persistent()
            .get(&DataKey::Bounty(bounty_id))
            .expect("Bounty not found");

        bounty.creator.require_auth();

        let application: BountyApplication = env
            .storage()
            .persistent()
            .get(&DataKey::Application(application_id))
            .expect("Application not found");

        assert!(application.bounty_id == bounty_id, "Application does not match bounty");

        env.storage()
            .persistent()
            .set(&DataKey::SelectedFreelancer(bounty_id), &application.freelancer);

        bounty.status = BountyStatus::InProgress;
        env.storage().persistent().set(&DataKey::Bounty(bounty_id), &bounty);

        true
    }

    pub fn complete_bounty(env: Env, bounty_id: u64) -> bool {
        let mut bounty: Bounty = env
            .storage()
            .persistent()
            .get(&DataKey::Bounty(bounty_id))
            .expect("Bounty not found");

        bounty.creator.require_auth();
        assert!(bounty.status == BountyStatus::InProgress, "Bounty not in progress");

        bounty.status = BountyStatus::Completed;
        env.storage().persistent().set(&DataKey::Bounty(bounty_id), &bounty);

        true
    }

    pub fn cancel_bounty(env: Env, bounty_id: u64) -> bool {
        let mut bounty: Bounty = env
            .storage()
            .persistent()
            .get(&DataKey::Bounty(bounty_id))
            .expect("Bounty not found");

        bounty.creator.require_auth();
        assert!(bounty.status == BountyStatus::Open, "Only open bounties can be cancelled");

        bounty.status = BountyStatus::Cancelled;
        env.storage().persistent().set(&DataKey::Bounty(bounty_id), &bounty);

        true
    }

    pub fn get_bounties_count(env: Env) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::BountyCounter)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn test_create_bounty() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(BountyContract, ());
        let client = BountyContractClient::new(&env, &contract_id);

        let creator = Address::generate(&env);
        let bounty_id = client.create_bounty(
            &creator,
            &String::from_str(&env, "Test Bounty"),
            &String::from_str(&env, "Test Description"),
            &5000i128,
            &100u64,
        );

        assert_eq!(bounty_id, 1);
        let bounty = client.get_bounty(&bounty_id);
        assert_eq!(bounty.creator, creator);
        assert_eq!(bounty.budget, 5000i128);
    }

    #[test]
    fn test_apply_for_bounty() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(BountyContract, ());
        let client = BountyContractClient::new(&env, &contract_id);

        let creator = Address::generate(&env);
        let freelancer = Address::generate(&env);

        let bounty_id = client.create_bounty(
            &creator,
            &String::from_str(&env, "Test Bounty"),
            &String::from_str(&env, "Test Description"),
            &5000i128,
            &100u64,
        );

        let app_id = client.apply_for_bounty(
            &bounty_id,
            &freelancer,
            &String::from_str(&env, "I can do this!"),
            &4500i128,
            &30u64,
        );

        assert_eq!(app_id, 1);
        let application = client.get_application(&app_id);
        assert_eq!(application.freelancer, freelancer);
    }

    #[test]
    fn test_get_bounty_applications_pagination() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(BountyContract, ());
        let client = BountyContractClient::new(&env, &contract_id);

        let creator = Address::generate(&env);
        let bounty_id = client.create_bounty(
            &creator,
            &String::from_str(&env, "Test Bounty"),
            &String::from_str(&env, "Test Description"),
            &5000i128,
            &100u64,
        );

        // Apply with 3 different freelancers
        for i in 0..3 {
            let freelancer = Address::generate(&env);
            client.apply_for_bounty(
                &bounty_id,
                &freelancer,
                &String::from_str(&env, "Proposal"),
                &(4000i128 + i),
                &30u64,
            );
        }

        // Get all applications
        let page0 = client.get_bounty_applications(&bounty_id, &0, &10);
        assert_eq!(page0.total, 3);
        assert_eq!(page0.application_ids.len(), 3);
        assert!(!page0.has_more);

        // Get first 2 applications (page 0, limit 2)
        let page = client.get_bounty_applications(&bounty_id, &0, &2);
        assert_eq!(page.total, 3);
        assert_eq!(page.application_ids.len(), 2);
        assert!(page.has_more);

        // Get last application (page 1, limit 2)
        let page2 = client.get_bounty_applications(&bounty_id, &2, &2);
        assert_eq!(page2.total, 3);
        assert_eq!(page2.application_ids.len(), 1);
        assert!(!page2.has_more);

        // Empty bounty returns empty page
        let empty_bounty_id = client.create_bounty(
            &creator,
            &String::from_str(&env, "Empty Bounty"),
            &String::from_str(&env, "No apps"),
            &1000i128,
            &50u64,
        );
        let empty_page = client.get_bounty_applications(&empty_bounty_id, &0, &10);
        assert_eq!(empty_page.total, 0);
        assert_eq!(empty_page.application_ids.len(), 0);
        assert!(!empty_page.has_more);
    }
}
