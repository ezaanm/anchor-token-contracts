use crate::contract::{handle, init, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::state::{
    bank_read, bank_store, config_read, poll_store, poll_voter_read, poll_voter_store, state_read,
    Config, Poll, State, TokenManager,
};

use crate::querier::load_token_balance;
use anchor_token::common::OrderBy;
use anchor_token::gov::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, HandleMsg, InitMsg, PollResponse, PollStatus,
    PollsResponse, QueryMsg, StakerResponse, VoteOption, VoterInfo, VotersResponse,
    VotersResponseItem,
};
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    coins, from_binary, log, to_binary, Api, CanonicalAddr, Coin, CosmosMsg, Decimal, Env, Extern,
    HandleResponse, HumanAddr, StdError, Uint128, WasmMsg,
};
use cw20::{Cw20HandleMsg, Cw20ReceiveMsg};

const VOTING_TOKEN: &str = "voting_token";
const TEST_CREATOR: &str = "creator";
const TEST_VOTER: &str = "voter1";
const TEST_VOTER_2: &str = "voter2";
const TEST_VOTER_3: &str = "voter3";
const DEFAULT_QUORUM: u64 = 30u64;
const DEFAULT_THRESHOLD: u64 = 50u64;
const DEFAULT_VOTING_PERIOD: u64 = 10000u64;
const DEFAULT_FIX_PERIOD: u64 = 10u64;
const DEFAULT_TIMELOCK_PERIOD: u64 = 10000u64;
const DEFAULT_EXPIRATION_PERIOD: u64 = 20000u64;
const DEFAULT_PROPOSAL_DEPOSIT: u128 = 10000000000u128;

fn mock_init(mut deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>) {
    let msg = InitMsg {
        quorum: Decimal::percent(DEFAULT_QUORUM),
        threshold: Decimal::percent(DEFAULT_THRESHOLD),
        voting_period: DEFAULT_VOTING_PERIOD,
        timelock_period: DEFAULT_TIMELOCK_PERIOD,
        expiration_period: DEFAULT_EXPIRATION_PERIOD,
        proposal_deposit: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
        snapshot_period: DEFAULT_FIX_PERIOD,
    };

    let env = mock_env(TEST_CREATOR, &[]);
    let _res = init(&mut deps, env.clone(), msg).expect("contract successfully handles InitMsg");

    let msg = HandleMsg::RegisterContracts {
        anchor_token: HumanAddr::from(VOTING_TOKEN),
    };
    let _res =
        handle(&mut deps, env, msg).expect("contract successfully handles RegisterContracts");
}

fn mock_env_height(sender: &str, sent: &[Coin], height: u64, time: u64) -> Env {
    let mut env = mock_env(sender, sent);
    env.block.height = height;
    env.block.time = time;
    env
}

fn init_msg() -> InitMsg {
    InitMsg {
        quorum: Decimal::percent(DEFAULT_QUORUM),
        threshold: Decimal::percent(DEFAULT_THRESHOLD),
        voting_period: DEFAULT_VOTING_PERIOD,
        timelock_period: DEFAULT_TIMELOCK_PERIOD,
        expiration_period: DEFAULT_EXPIRATION_PERIOD,
        proposal_deposit: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
        snapshot_period: DEFAULT_FIX_PERIOD,
    }
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = init_msg();
    let env = mock_env(TEST_CREATOR, &coins(2, VOTING_TOKEN));
    let res = init(&mut deps, env.clone(), msg).unwrap();
    assert_eq!(0, res.messages.len());

    let config: Config = config_read(&mut deps.storage).load().unwrap();
    assert_eq!(
        config,
        Config {
            anchor_token: CanonicalAddr::default(),
            owner: deps
                .api
                .canonical_address(&HumanAddr::from(TEST_CREATOR))
                .unwrap(),
            quorum: Decimal::percent(DEFAULT_QUORUM),
            threshold: Decimal::percent(DEFAULT_THRESHOLD),
            voting_period: DEFAULT_VOTING_PERIOD,
            timelock_period: DEFAULT_TIMELOCK_PERIOD,
            expiration_period: DEFAULT_EXPIRATION_PERIOD,
            proposal_deposit: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
            snapshot_period: DEFAULT_FIX_PERIOD
        }
    );

    let msg = HandleMsg::RegisterContracts {
        anchor_token: HumanAddr::from(VOTING_TOKEN),
    };
    let _res = handle(&mut deps, env, msg).unwrap();
    let config: Config = config_read(&mut deps.storage).load().unwrap();
    assert_eq!(
        config.anchor_token,
        deps.api
            .canonical_address(&HumanAddr::from(VOTING_TOKEN))
            .unwrap()
    );

    let state: State = state_read(&mut deps.storage).load().unwrap();
    assert_eq!(
        state,
        State {
            contract_addr: deps
                .api
                .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
                .unwrap(),
            poll_count: 0,
            total_share: Uint128::zero(),
            total_deposit: Uint128::zero(),
        }
    );
}

#[test]
fn poll_not_found() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);

    let res = query(&deps, QueryMsg::Poll { poll_id: 1 });

    match res {
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Poll does not exist"),
        Err(e) => panic!("Unexpected error: {:?}", e),
        _ => panic!("Must return error"),
    }
}

#[test]
fn fails_init_invalid_quorum() {
    let mut deps = mock_dependencies(20, &[]);
    let env = mock_env("voter", &coins(11, VOTING_TOKEN));
    let msg = InitMsg {
        quorum: Decimal::percent(101),
        threshold: Decimal::percent(DEFAULT_THRESHOLD),
        voting_period: DEFAULT_VOTING_PERIOD,
        timelock_period: DEFAULT_TIMELOCK_PERIOD,
        expiration_period: DEFAULT_EXPIRATION_PERIOD,
        proposal_deposit: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
        snapshot_period: DEFAULT_FIX_PERIOD,
    };

    let res = init(&mut deps, env, msg);

    match res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "quorum must be 0 to 1"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn fails_init_invalid_threshold() {
    let mut deps = mock_dependencies(20, &[]);
    let env = mock_env("voter", &coins(11, VOTING_TOKEN));
    let msg = InitMsg {
        quorum: Decimal::percent(DEFAULT_QUORUM),
        threshold: Decimal::percent(101),
        voting_period: DEFAULT_VOTING_PERIOD,
        timelock_period: DEFAULT_TIMELOCK_PERIOD,
        expiration_period: DEFAULT_EXPIRATION_PERIOD,
        proposal_deposit: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
        snapshot_period: DEFAULT_FIX_PERIOD,
    };

    let res = init(&mut deps, env, msg);

    match res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "threshold must be 0 to 1"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn fails_contract_already_registered() {
    let mut deps = mock_dependencies(20, &[]);
    let env = mock_env("voter", &coins(11, VOTING_TOKEN));
    let msg = InitMsg {
        quorum: Decimal::percent(DEFAULT_QUORUM),
        threshold: Decimal::percent(DEFAULT_THRESHOLD),
        voting_period: DEFAULT_VOTING_PERIOD,
        timelock_period: DEFAULT_TIMELOCK_PERIOD,
        expiration_period: DEFAULT_EXPIRATION_PERIOD,
        proposal_deposit: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
        snapshot_period: DEFAULT_FIX_PERIOD,
    };

    let _res = init(&mut deps, env.clone(), msg).unwrap();

    let msg = HandleMsg::RegisterContracts {
        anchor_token: HumanAddr::from(VOTING_TOKEN),
    };
    let _res = handle(&mut deps, env.clone(), msg.clone()).unwrap();
    let res = handle(&mut deps, env.clone(), msg.clone());
    match res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::Unauthorized { .. }) => {}
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn fails_create_poll_invalid_title() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);

    let msg = create_poll_msg("a".to_string(), "test".to_string(), None, None);
    let env = mock_env(VOTING_TOKEN, &vec![]);
    match handle(&mut deps, env.clone(), msg) {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Title too short"),
        Err(_) => panic!("Unknown error"),
    }

    let msg = create_poll_msg(
            "0123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234012345678901234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234".to_string(),
            "test".to_string(),
            None,
            None,
        );

    match handle(&mut deps, env.clone(), msg) {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Title too long"),
        Err(_) => panic!("Unknown error"),
    }
}

#[test]
fn fails_create_poll_invalid_description() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);

    let msg = create_poll_msg("test".to_string(), "a".to_string(), None, None);
    let env = mock_env(VOTING_TOKEN, &vec![]);
    match handle(&mut deps, env.clone(), msg) {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Description too short"),
        Err(_) => panic!("Unknown error"),
    }

    let msg = create_poll_msg(
            "test".to_string(),
            "012345678901234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234012345678900123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234012345678901234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234123456789012340123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234012345678901234567890123456789012345678901234567890123456789012340123456789001234567890123456789012345678901234567890123456789012345678901234012345678901234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234012345678901234567890123456789012345678901234567890123456789012341234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123456".to_string(),
            None,
            None,
        );

    match handle(&mut deps, env.clone(), msg) {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Description too long"),
        Err(_) => panic!("Unknown error"),
    }
}

#[test]
fn fails_create_poll_invalid_link() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);

    let msg = create_poll_msg(
        "test".to_string(),
        "test".to_string(),
        Some("http://hih".to_string()),
        None,
    );
    let env = mock_env(VOTING_TOKEN, &vec![]);
    match handle(&mut deps, env.clone(), msg) {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Link too short"),
        Err(_) => panic!("Unknown error"),
    }

    let msg = create_poll_msg(
            "test".to_string(),
            "test".to_string(),
            Some("0123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234012345678901234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234".to_string()),
            None,
        );

    match handle(&mut deps, env.clone(), msg) {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Link too long"),
        Err(_) => panic!("Unknown error"),
    }
}

#[test]
fn fails_create_poll_invalid_deposit() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_CREATOR),
        amount: Uint128(DEFAULT_PROPOSAL_DEPOSIT - 1),
        msg: Some(
            to_binary(&Cw20HookMsg::CreatePoll {
                title: "TESTTEST".to_string(),
                description: "TESTTEST".to_string(),
                link: None,
                execute_msgs: None,
            })
            .unwrap(),
        ),
    });
    let env = mock_env(VOTING_TOKEN, &vec![]);
    match handle(&mut deps, env.clone(), msg) {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(
            msg,
            format!("Must deposit more than {} token", DEFAULT_PROPOSAL_DEPOSIT)
        ),
        Err(_) => panic!("Unknown error"),
    }
}

fn create_poll_msg(
    title: String,
    description: String,
    link: Option<String>,
    execute_msg: Option<Vec<ExecuteMsg>>,
) -> HandleMsg {
    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_CREATOR),
        amount: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
        msg: Some(
            to_binary(&Cw20HookMsg::CreatePoll {
                title,
                description,
                link,
                execute_msgs: execute_msg,
            })
            .unwrap(),
        ),
    });
    msg
}

#[test]
fn happy_days_create_poll() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);
    let env = mock_env_height(VOTING_TOKEN, &vec![], 0, 10000);

    let msg = create_poll_msg("test".to_string(), "test".to_string(), None, None);

    let handle_res = handle(&mut deps, env.clone(), msg.clone()).unwrap();
    assert_create_poll_result(
        1,
        env.block.height + DEFAULT_VOTING_PERIOD,
        TEST_CREATOR,
        handle_res,
        &mut deps,
    );
}

#[test]
fn query_polls() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);
    let env = mock_env_height(VOTING_TOKEN, &vec![], 0, 10000);

    let exec_msg_bz = to_binary(&Cw20HandleMsg::Burn {
        amount: Uint128(123),
    })
    .unwrap();

    let exec_msg_bz2 = to_binary(&Cw20HandleMsg::Burn {
        amount: Uint128(12),
    })
    .unwrap();

    let exec_msg_bz3 = to_binary(&Cw20HandleMsg::Burn { amount: Uint128(1) }).unwrap();

    let mut execute_msgs: Vec<ExecuteMsg> = vec![];

    execute_msgs.push(ExecuteMsg {
        order: 1u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz.clone(),
    });

    execute_msgs.push(ExecuteMsg {
        order: 3u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz3.clone(),
    });

    execute_msgs.push(ExecuteMsg {
        order: 2u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz2.clone(),
    });

    let msg = create_poll_msg(
        "test".to_string(),
        "test".to_string(),
        Some("http://google.com".to_string()),
        Some(execute_msgs.clone()),
    );

    let _handle_res = handle(&mut deps, env.clone(), msg.clone()).unwrap();
    let msg = create_poll_msg("test2".to_string(), "test2".to_string(), None, None);
    let _handle_res = handle(&mut deps, env.clone(), msg.clone()).unwrap();

    let res = query(
        &deps,
        QueryMsg::Polls {
            filter: None,
            start_after: None,
            limit: None,
            order_by: Some(OrderBy::Asc),
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(
        response.polls,
        vec![
            PollResponse {
                id: 1u64,
                creator: HumanAddr::from(TEST_CREATOR),
                status: PollStatus::InProgress,
                end_height: 10000u64,
                title: "test".to_string(),
                description: "test".to_string(),
                link: Some("http://google.com".to_string()),
                deposit_amount: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
                execute_data: Some(execute_msgs.clone()),
                yes_votes: Uint128::zero(),
                no_votes: Uint128::zero(),
                staked_amount: None,
                total_balance_at_end_poll: None,
            },
            PollResponse {
                id: 2u64,
                creator: HumanAddr::from(TEST_CREATOR),
                status: PollStatus::InProgress,
                end_height: 10000u64,
                title: "test2".to_string(),
                description: "test2".to_string(),
                link: None,
                deposit_amount: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
                execute_data: None,
                yes_votes: Uint128::zero(),
                no_votes: Uint128::zero(),
                staked_amount: None,
                total_balance_at_end_poll: None,
            },
        ]
    );

    let res = query(
        &deps,
        QueryMsg::Polls {
            filter: None,
            start_after: Some(1u64),
            limit: None,
            order_by: Some(OrderBy::Asc),
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(
        response.polls,
        vec![PollResponse {
            id: 2u64,
            creator: HumanAddr::from(TEST_CREATOR),
            status: PollStatus::InProgress,
            end_height: 10000u64,
            title: "test2".to_string(),
            description: "test2".to_string(),
            link: None,
            deposit_amount: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
            execute_data: None,
            yes_votes: Uint128::zero(),
            no_votes: Uint128::zero(),
            staked_amount: None,
            total_balance_at_end_poll: None,
        },]
    );

    let res = query(
        &deps,
        QueryMsg::Polls {
            filter: None,
            start_after: Some(2u64),
            limit: None,
            order_by: Some(OrderBy::Desc),
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(
        response.polls,
        vec![PollResponse {
            id: 1u64,
            creator: HumanAddr::from(TEST_CREATOR),
            status: PollStatus::InProgress,
            end_height: 10000u64,
            title: "test".to_string(),
            description: "test".to_string(),
            link: Some("http://google.com".to_string()),
            deposit_amount: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
            execute_data: Some(execute_msgs),
            yes_votes: Uint128::zero(),
            no_votes: Uint128::zero(),
            staked_amount: None,
            total_balance_at_end_poll: None,
        }]
    );

    let res = query(
        &deps,
        QueryMsg::Polls {
            filter: Some(PollStatus::InProgress),
            start_after: Some(1u64),
            limit: None,
            order_by: Some(OrderBy::Asc),
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(
        response.polls,
        vec![PollResponse {
            id: 2u64,
            creator: HumanAddr::from(TEST_CREATOR),
            status: PollStatus::InProgress,
            end_height: 10000u64,
            title: "test2".to_string(),
            description: "test2".to_string(),
            link: None,
            deposit_amount: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
            execute_data: None,
            yes_votes: Uint128::zero(),
            no_votes: Uint128::zero(),
            staked_amount: None,
            total_balance_at_end_poll: None,
        },]
    );

    let res = query(
        &deps,
        QueryMsg::Polls {
            filter: Some(PollStatus::Passed),
            start_after: None,
            limit: None,
            order_by: None,
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(response.polls, vec![]);
}

#[test]
fn create_poll_no_quorum() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);
    let env = mock_env_height(VOTING_TOKEN, &vec![], 0, 10000);

    let msg = create_poll_msg("test".to_string(), "test".to_string(), None, None);

    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_create_poll_result(
        1,
        DEFAULT_VOTING_PERIOD,
        TEST_CREATOR,
        handle_res,
        &mut deps,
    );
}

#[test]
fn fails_end_poll_before_end_height() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);
    let env = mock_env_height(VOTING_TOKEN, &vec![], 0, 10000);

    let msg = create_poll_msg("test".to_string(), "test".to_string(), None, None);

    let handle_res = handle(&mut deps, env.clone(), msg.clone()).unwrap();
    assert_create_poll_result(
        1,
        DEFAULT_VOTING_PERIOD,
        TEST_CREATOR,
        handle_res,
        &mut deps,
    );

    let res = query(&deps, QueryMsg::Poll { poll_id: 1 }).unwrap();
    let value: PollResponse = from_binary(&res).unwrap();
    assert_eq!(DEFAULT_VOTING_PERIOD, value.end_height);

    let msg = HandleMsg::EndPoll { poll_id: 1 };
    let env = mock_env_height(TEST_CREATOR, &vec![], 0, 10000);
    let handle_res = handle(&mut deps, env, msg);

    match handle_res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Voting period has not expired"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn happy_days_end_poll() {
    const POLL_START_HEIGHT: u64 = 1000;
    const POLL_ID: u64 = 1;
    let stake_amount = 1000;

    let mut deps = mock_dependencies(20, &coins(1000, VOTING_TOKEN));
    mock_init(&mut deps);
    let mut creator_env = mock_env_height(
        VOTING_TOKEN,
        &coins(2, VOTING_TOKEN),
        POLL_START_HEIGHT,
        10000,
    );

    let exec_msg_bz = to_binary(&Cw20HandleMsg::Burn {
        amount: Uint128(123),
    })
    .unwrap();

    let exec_msg_bz2 = to_binary(&Cw20HandleMsg::Burn {
        amount: Uint128(12),
    })
    .unwrap();

    let exec_msg_bz3 = to_binary(&Cw20HandleMsg::Burn { amount: Uint128(1) }).unwrap();

    //add three messages with different order
    let mut execute_msgs: Vec<ExecuteMsg> = vec![];

    execute_msgs.push(ExecuteMsg {
        order: 3u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz3.clone(),
    });

    execute_msgs.push(ExecuteMsg {
        order: 2u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz2.clone(),
    });

    execute_msgs.push(ExecuteMsg {
        order: 1u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz.clone(),
    });

    let msg = create_poll_msg(
        "test".to_string(),
        "test".to_string(),
        None,
        Some(execute_msgs),
    );

    let handle_res = handle(&mut deps, creator_env.clone(), msg).unwrap();

    assert_create_poll_result(
        1,
        creator_env.block.height + DEFAULT_VOTING_PERIOD,
        TEST_CREATOR,
        handle_res,
        &mut deps,
    );

    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128((stake_amount + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(stake_amount as u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_stake_tokens_result(
        stake_amount,
        DEFAULT_PROPOSAL_DEPOSIT,
        stake_amount,
        1,
        handle_res,
        &mut deps,
    );

    let msg = HandleMsg::CastVote {
        poll_id: 1,
        vote: VoteOption::Yes,
        amount: Uint128::from(stake_amount),
    };
    let env = mock_env_height(TEST_VOTER, &[], POLL_START_HEIGHT, 10000);
    let handle_res = handle(&mut deps, env, msg).unwrap();

    assert_eq!(
        handle_res.log,
        vec![
            log("action", "cast_vote"),
            log("poll_id", POLL_ID),
            log("amount", "1000"),
            log("voter", TEST_VOTER),
            log("vote_option", "yes"),
        ]
    );

    // not in passed status
    let msg = HandleMsg::ExecutePoll { poll_id: 1 };
    let handle_res = handle(&mut deps, creator_env.clone(), msg).unwrap_err();
    match handle_res {
        StdError::GenericErr { msg, .. } => assert_eq!(msg, "Poll is not in passed status"),
        _ => panic!("DO NOT ENTER HERE"),
    }

    creator_env.message.sender = HumanAddr::from(TEST_CREATOR);
    creator_env.block.height = &creator_env.block.height + DEFAULT_VOTING_PERIOD;

    let msg = HandleMsg::EndPoll { poll_id: 1 };
    let handle_res = handle(&mut deps, creator_env.clone(), msg).unwrap();

    assert_eq!(
        handle_res.log,
        vec![
            log("action", "end_poll"),
            log("poll_id", "1"),
            log("rejected_reason", ""),
            log("passed", "true"),
        ]
    );
    assert_eq!(
        handle_res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from(VOTING_TOKEN),
            msg: to_binary(&Cw20HandleMsg::Transfer {
                recipient: HumanAddr::from(TEST_CREATOR),
                amount: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
            })
            .unwrap(),
            send: vec![],
        })]
    );

    // End poll will withdraw deposit balance
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128(stake_amount as u128),
        )],
    )]);

    // timelock_period has not expired
    let msg = HandleMsg::ExecutePoll { poll_id: 1 };
    let handle_res = handle(&mut deps, creator_env.clone(), msg).unwrap_err();
    match handle_res {
        StdError::GenericErr { msg, .. } => assert_eq!(msg, "Timelock period has not expired"),
        _ => panic!("DO NOT ENTER HERE"),
    }

    creator_env.block.height = &creator_env.block.height + DEFAULT_TIMELOCK_PERIOD;
    let msg = HandleMsg::ExecutePoll { poll_id: 1 };
    let handle_res = handle(&mut deps, creator_env, msg).unwrap();
    assert_eq!(
        handle_res.messages,
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(VOTING_TOKEN),
                msg: exec_msg_bz.clone(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(VOTING_TOKEN),
                msg: exec_msg_bz2,
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(VOTING_TOKEN),
                msg: exec_msg_bz3,
                send: vec![],
            })
        ]
    );
    assert_eq!(
        handle_res.log,
        vec![log("action", "execute_poll"), log("poll_id", "1"),]
    );

    // Query executed polls
    let res = query(
        &deps,
        QueryMsg::Polls {
            filter: Some(PollStatus::Passed),
            start_after: None,
            limit: None,
            order_by: None,
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(response.polls.len(), 0);

    let res = query(
        &deps,
        QueryMsg::Polls {
            filter: Some(PollStatus::InProgress),
            start_after: None,
            limit: None,
            order_by: None,
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(response.polls.len(), 0);

    let res = query(
        &deps,
        QueryMsg::Polls {
            filter: Some(PollStatus::Executed),
            start_after: None,
            limit: None,
            order_by: Some(OrderBy::Desc),
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(response.polls.len(), 1);

    // voter info must be deleted
    let res = query(
        &deps,
        QueryMsg::Voters {
            poll_id: 1u64,
            start_after: None,
            limit: None,
            order_by: None,
        },
    )
    .unwrap();
    let response: VotersResponse = from_binary(&res).unwrap();
    assert_eq!(response.voters.len(), 0);

    // staker locked token must be disappeared
    let res = query(
        &deps,
        QueryMsg::Staker {
            address: HumanAddr::from(TEST_VOTER),
        },
    )
    .unwrap();
    let response: StakerResponse = from_binary(&res).unwrap();
    assert_eq!(
        response,
        StakerResponse {
            balance: Uint128(stake_amount),
            share: Uint128(stake_amount),
            locked_balance: vec![]
        }
    );

    // But the data is still in the store
    let voter_addr_raw = deps
        .api
        .canonical_address(&HumanAddr::from(TEST_VOTER))
        .unwrap();
    let voter = poll_voter_read(&deps.storage, 1u64)
        .load(&voter_addr_raw.as_slice())
        .unwrap();
    assert_eq!(
        voter,
        VoterInfo {
            vote: VoteOption::Yes,
            balance: Uint128(stake_amount),
        }
    );

    let token_manager = bank_read(&deps.storage)
        .load(&voter_addr_raw.as_slice())
        .unwrap();
    assert_eq!(
        token_manager.locked_balance,
        vec![(
            1u64,
            VoterInfo {
                vote: VoteOption::Yes,
                balance: Uint128(stake_amount),
            }
        )]
    );
}

#[test]
fn expire_poll() {
    const POLL_START_HEIGHT: u64 = 1000;
    const POLL_ID: u64 = 1;
    let stake_amount = 1000;

    let mut deps = mock_dependencies(20, &coins(1000, VOTING_TOKEN));
    mock_init(&mut deps);
    let mut creator_env = mock_env_height(
        VOTING_TOKEN,
        &coins(2, VOTING_TOKEN),
        POLL_START_HEIGHT,
        10000,
    );

    let exec_msg_bz = to_binary(&Cw20HandleMsg::Burn {
        amount: Uint128(123),
    })
    .unwrap();
    let mut execute_msgs: Vec<ExecuteMsg> = vec![];
    execute_msgs.push(ExecuteMsg {
        order: 1u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz.clone(),
    });
    let msg = create_poll_msg(
        "test".to_string(),
        "test".to_string(),
        None,
        Some(execute_msgs),
    );

    let handle_res = handle(&mut deps, creator_env.clone(), msg).unwrap();

    assert_create_poll_result(
        1,
        creator_env.block.height + DEFAULT_VOTING_PERIOD,
        TEST_CREATOR,
        handle_res,
        &mut deps,
    );

    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128((stake_amount + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(stake_amount as u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_stake_tokens_result(
        stake_amount,
        DEFAULT_PROPOSAL_DEPOSIT,
        stake_amount,
        1,
        handle_res,
        &mut deps,
    );

    let msg = HandleMsg::CastVote {
        poll_id: 1,
        vote: VoteOption::Yes,
        amount: Uint128::from(stake_amount),
    };
    let env = mock_env_height(TEST_VOTER, &[], POLL_START_HEIGHT, 10000);
    let handle_res = handle(&mut deps, env, msg).unwrap();

    assert_eq!(
        handle_res.log,
        vec![
            log("action", "cast_vote"),
            log("poll_id", POLL_ID),
            log("amount", "1000"),
            log("voter", TEST_VOTER),
            log("vote_option", "yes"),
        ]
    );

    // Poll is not in passed status
    creator_env.block.height = &creator_env.block.height + DEFAULT_TIMELOCK_PERIOD;
    let msg = HandleMsg::ExpirePoll { poll_id: 1 };
    let handle_res = handle(&mut deps, creator_env.clone(), msg);
    match handle_res {
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Poll is not in passed status"),
        _ => panic!("DO NOT ENTER HERE"),
    }

    let msg = HandleMsg::EndPoll { poll_id: 1 };
    let handle_res = handle(&mut deps, creator_env.clone(), msg).unwrap();

    assert_eq!(
        handle_res.log,
        vec![
            log("action", "end_poll"),
            log("poll_id", "1"),
            log("rejected_reason", ""),
            log("passed", "true"),
        ]
    );
    assert_eq!(
        handle_res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from(VOTING_TOKEN),
            msg: to_binary(&Cw20HandleMsg::Transfer {
                recipient: HumanAddr::from(TEST_CREATOR),
                amount: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
            })
            .unwrap(),
            send: vec![],
        })]
    );

    // Expiration period has not been passed
    let msg = HandleMsg::ExpirePoll { poll_id: 1 };
    let handle_res = handle(&mut deps, creator_env.clone(), msg);
    match handle_res {
        Err(StdError::GenericErr { msg, .. }) => {
            assert_eq!(msg, "Expire height has not been reached")
        }
        _ => panic!("DO NOT ENTER HERE"),
    }

    creator_env.block.height = &creator_env.block.height + DEFAULT_EXPIRATION_PERIOD;
    let msg = HandleMsg::ExpirePoll { poll_id: 1 };
    let _handle_res = handle(&mut deps, creator_env.clone(), msg).unwrap();

    let res = query(&deps, QueryMsg::Poll { poll_id: 1 }).unwrap();
    let poll_res: PollResponse = from_binary(&res).unwrap();
    assert_eq!(poll_res.status, PollStatus::Expired);

    let res = query(
        &deps,
        QueryMsg::Polls {
            filter: Some(PollStatus::Expired),
            start_after: None,
            limit: None,
            order_by: Some(OrderBy::Desc),
        },
    )
    .unwrap();
    let polls_res: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(polls_res.polls[0], poll_res);
}

#[test]
fn end_poll_zero_quorum() {
    let mut deps = mock_dependencies(20, &coins(1000, VOTING_TOKEN));
    mock_init(&mut deps);
    let mut creator_env = mock_env_height(VOTING_TOKEN, &vec![], 1000, 10000);

    let mut execute_msgs: Vec<ExecuteMsg> = vec![];
    execute_msgs.push(ExecuteMsg {
        order: 1u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: to_binary(&Cw20HandleMsg::Burn {
            amount: Uint128(123),
        })
        .unwrap(),
    });

    let msg = create_poll_msg(
        "test".to_string(),
        "test".to_string(),
        None,
        Some(execute_msgs),
    );

    let handle_res = handle(&mut deps, creator_env.clone(), msg).unwrap();
    assert_create_poll_result(
        1,
        creator_env.block.height + DEFAULT_VOTING_PERIOD,
        TEST_CREATOR,
        handle_res,
        &mut deps,
    );
    let stake_amount = 100;
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128(100u128 + DEFAULT_PROPOSAL_DEPOSIT),
        )],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(stake_amount as u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    handle(&mut deps, env, msg.clone()).unwrap();

    let msg = HandleMsg::EndPoll { poll_id: 1 };
    creator_env.message.sender = HumanAddr::from(TEST_CREATOR);
    creator_env.block.height = &creator_env.block.height + DEFAULT_VOTING_PERIOD;

    let handle_res = handle(&mut deps, creator_env.clone(), msg).unwrap();

    assert_eq!(
        handle_res.log,
        vec![
            log("action", "end_poll"),
            log("poll_id", "1"),
            log("rejected_reason", "Quorum not reached"),
            log("passed", "false"),
        ]
    );

    assert_eq!(handle_res.messages.len(), 0usize);

    // Query rejected polls
    let res = query(
        &deps,
        QueryMsg::Polls {
            filter: Some(PollStatus::Rejected),
            start_after: None,
            limit: None,
            order_by: Some(OrderBy::Desc),
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(response.polls.len(), 1);

    let res = query(
        &deps,
        QueryMsg::Polls {
            filter: Some(PollStatus::InProgress),
            start_after: None,
            limit: None,
            order_by: None,
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(response.polls.len(), 0);

    let res = query(
        &deps,
        QueryMsg::Polls {
            filter: Some(PollStatus::Passed),
            start_after: None,
            limit: None,
            order_by: None,
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(response.polls.len(), 0);
}

#[test]
fn end_poll_quorum_rejected() {
    let mut deps = mock_dependencies(20, &coins(100, VOTING_TOKEN));
    mock_init(&mut deps);

    let msg = create_poll_msg("test".to_string(), "test".to_string(), None, None);
    let mut creator_env = mock_env(VOTING_TOKEN, &vec![]);
    let handle_res = handle(&mut deps, creator_env.clone(), msg.clone()).unwrap();
    assert_eq!(
        handle_res.log,
        vec![
            log("action", "create_poll"),
            log("creator", TEST_CREATOR),
            log("poll_id", "1"),
            log("end_height", "22345"),
        ]
    );

    let stake_amount = 100;
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128(100u128 + DEFAULT_PROPOSAL_DEPOSIT),
        )],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(stake_amount as u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_stake_tokens_result(
        stake_amount,
        DEFAULT_PROPOSAL_DEPOSIT,
        stake_amount,
        1,
        handle_res,
        &mut deps,
    );

    let msg = HandleMsg::CastVote {
        poll_id: 1,
        vote: VoteOption::Yes,
        amount: Uint128::from(10u128),
    };
    let env = mock_env(TEST_VOTER, &[]);
    let handle_res = handle(&mut deps, env, msg).unwrap();

    assert_eq!(
        handle_res.log,
        vec![
            log("action", "cast_vote"),
            log("poll_id", "1"),
            log("amount", "10"),
            log("voter", TEST_VOTER),
            log("vote_option", "yes"),
        ]
    );

    let msg = HandleMsg::EndPoll { poll_id: 1 };

    creator_env.message.sender = HumanAddr::from(TEST_CREATOR);
    creator_env.block.height = &creator_env.block.height + DEFAULT_VOTING_PERIOD;

    let handle_res = handle(&mut deps, creator_env.clone(), msg.clone()).unwrap();
    assert_eq!(
        handle_res.log,
        vec![
            log("action", "end_poll"),
            log("poll_id", "1"),
            log("rejected_reason", "Quorum not reached"),
            log("passed", "false"),
        ]
    );
}

#[test]
fn end_poll_quorum_rejected_noting_staked() {
    let mut deps = mock_dependencies(20, &coins(100, VOTING_TOKEN));
    mock_init(&mut deps);

    let msg = create_poll_msg("test".to_string(), "test".to_string(), None, None);
    let mut creator_env = mock_env(VOTING_TOKEN, &vec![]);
    let handle_res = handle(&mut deps, creator_env.clone(), msg.clone()).unwrap();
    assert_eq!(
        handle_res.log,
        vec![
            log("action", "create_poll"),
            log("creator", TEST_CREATOR),
            log("poll_id", "1"),
            log("end_height", "22345"),
        ]
    );

    let msg = HandleMsg::EndPoll { poll_id: 1 };

    creator_env.message.sender = HumanAddr::from(TEST_CREATOR);
    creator_env.block.height = &creator_env.block.height + DEFAULT_VOTING_PERIOD;

    let handle_res = handle(&mut deps, creator_env.clone(), msg.clone()).unwrap();
    assert_eq!(
        handle_res.log,
        vec![
            log("action", "end_poll"),
            log("poll_id", "1"),
            log("rejected_reason", "Quorum not reached"),
            log("passed", "false"),
        ]
    );
}

#[test]
fn end_poll_nay_rejected() {
    let voter1_stake = 100;
    let voter2_stake = 1000;
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);
    let mut creator_env = mock_env(VOTING_TOKEN, &coins(2, VOTING_TOKEN));

    let msg = create_poll_msg("test".to_string(), "test".to_string(), None, None);

    let handle_res = handle(&mut deps, creator_env.clone(), msg.clone()).unwrap();
    assert_eq!(
        handle_res.log,
        vec![
            log("action", "create_poll"),
            log("creator", TEST_CREATOR),
            log("poll_id", "1"),
            log("end_height", "22345"),
        ]
    );

    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128((voter1_stake + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(voter1_stake as u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let handle_res = handle(&mut deps, env, msg).unwrap();
    assert_stake_tokens_result(
        voter1_stake,
        DEFAULT_PROPOSAL_DEPOSIT,
        voter1_stake,
        1,
        handle_res,
        &mut deps,
    );

    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128((voter1_stake + voter2_stake + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER_2),
        amount: Uint128::from(voter2_stake as u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let handle_res = handle(&mut deps, env, msg).unwrap();
    assert_stake_tokens_result(
        voter1_stake + voter2_stake,
        DEFAULT_PROPOSAL_DEPOSIT,
        voter2_stake,
        1,
        handle_res,
        &mut deps,
    );

    let env = mock_env(TEST_VOTER_2, &[]);
    let msg = HandleMsg::CastVote {
        poll_id: 1,
        vote: VoteOption::No,
        amount: Uint128::from(voter2_stake),
    };
    let handle_res = handle(&mut deps, env, msg).unwrap();
    assert_cast_vote_success(TEST_VOTER_2, voter2_stake, 1, VoteOption::No, handle_res);

    let msg = HandleMsg::EndPoll { poll_id: 1 };

    creator_env.message.sender = HumanAddr::from(TEST_CREATOR);
    creator_env.block.height = &creator_env.block.height + DEFAULT_VOTING_PERIOD;
    let handle_res = handle(&mut deps, creator_env.clone(), msg.clone()).unwrap();
    assert_eq!(
        handle_res.log,
        vec![
            log("action", "end_poll"),
            log("poll_id", "1"),
            log("rejected_reason", "Threshold not reached"),
            log("passed", "false"),
        ]
    );
}

#[test]
fn fails_cast_vote_not_enough_staked() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);
    let env = mock_env_height(VOTING_TOKEN, &vec![], 0, 10000);

    let msg = create_poll_msg("test".to_string(), "test".to_string(), None, None);

    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_create_poll_result(
        1,
        DEFAULT_VOTING_PERIOD,
        TEST_CREATOR,
        handle_res,
        &mut deps,
    );

    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128(10u128 + DEFAULT_PROPOSAL_DEPOSIT),
        )],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(10u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_stake_tokens_result(10, DEFAULT_PROPOSAL_DEPOSIT, 10, 1, handle_res, &mut deps);

    let env = mock_env_height(TEST_VOTER, &coins(11, VOTING_TOKEN), 0, 10000);
    let msg = HandleMsg::CastVote {
        poll_id: 1,
        vote: VoteOption::Yes,
        amount: Uint128::from(11u128),
    };

    let res = handle(&mut deps, env, msg);

    match res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => {
            assert_eq!(msg, "User does not have enough staked tokens.")
        }
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn happy_days_cast_vote() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);

    let env = mock_env_height(VOTING_TOKEN, &vec![], 0, 10000);
    let msg = create_poll_msg("test".to_string(), "test".to_string(), None, None);

    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_create_poll_result(
        1,
        DEFAULT_VOTING_PERIOD,
        TEST_CREATOR,
        handle_res,
        &mut deps,
    );

    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128(11u128 + DEFAULT_PROPOSAL_DEPOSIT),
        )],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(11u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_stake_tokens_result(11, DEFAULT_PROPOSAL_DEPOSIT, 11, 1, handle_res, &mut deps);

    let env = mock_env_height(TEST_VOTER, &coins(11, VOTING_TOKEN), 0, 10000);
    let amount = 10u128;
    let msg = HandleMsg::CastVote {
        poll_id: 1,
        vote: VoteOption::Yes,
        amount: Uint128::from(amount),
    };

    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_cast_vote_success(TEST_VOTER, amount, 1, VoteOption::Yes, handle_res);

    // balance be double
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128(22u128 + DEFAULT_PROPOSAL_DEPOSIT),
        )],
    )]);

    // Query staker
    let res = query(
        &deps,
        QueryMsg::Staker {
            address: HumanAddr::from(TEST_VOTER),
        },
    )
    .unwrap();
    let response: StakerResponse = from_binary(&res).unwrap();
    assert_eq!(
        response,
        StakerResponse {
            balance: Uint128(22u128),
            share: Uint128(11u128),
            locked_balance: vec![(
                1u64,
                VoterInfo {
                    vote: VoteOption::Yes,
                    balance: Uint128::from(amount),
                }
            )]
        }
    );

    // Query voters
    let res = query(
        &deps,
        QueryMsg::Voters {
            poll_id: 1u64,
            start_after: None,
            limit: None,
            order_by: Some(OrderBy::Desc),
        },
    )
    .unwrap();
    let response: VotersResponse = from_binary(&res).unwrap();
    assert_eq!(
        response.voters,
        vec![VotersResponseItem {
            voter: HumanAddr::from(TEST_VOTER),
            vote: VoteOption::Yes,
            balance: Uint128::from(amount),
        }]
    );

    let res = query(
        &deps,
        QueryMsg::Voters {
            poll_id: 1u64,
            start_after: Some(HumanAddr::from(TEST_VOTER)),
            limit: None,
            order_by: None,
        },
    )
    .unwrap();
    let response: VotersResponse = from_binary(&res).unwrap();
    assert_eq!(response.voters.len(), 0);
}

#[test]
fn happy_days_withdraw_voting_tokens() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);

    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(11u128))],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(11u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_stake_tokens_result(11, 0, 11, 0, handle_res, &mut deps);

    let state: State = state_read(&mut deps.storage).load().unwrap();
    assert_eq!(
        state,
        State {
            contract_addr: deps
                .api
                .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
                .unwrap(),
            poll_count: 0,
            total_share: Uint128::from(11u128),
            total_deposit: Uint128::zero(),
        }
    );

    // double the balance, only half will be withdrawn
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(22u128))],
    )]);

    let env = mock_env(TEST_VOTER, &[]);
    let msg = HandleMsg::WithdrawVotingTokens {
        amount: Some(Uint128::from(11u128)),
    };

    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    let msg = handle_res.messages.get(0).expect("no message");

    assert_eq!(
        msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from(VOTING_TOKEN),
            msg: to_binary(&Cw20HandleMsg::Transfer {
                recipient: HumanAddr::from(TEST_VOTER),
                amount: Uint128::from(11u128),
            })
            .unwrap(),
            send: vec![],
        })
    );

    let state: State = state_read(&mut deps.storage).load().unwrap();
    assert_eq!(
        state,
        State {
            contract_addr: deps
                .api
                .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
                .unwrap(),
            poll_count: 0,
            total_share: Uint128::from(6u128),
            total_deposit: Uint128::zero(),
        }
    );
}

#[test]
fn happy_days_withdraw_voting_tokens_all() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);

    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(11u128))],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(11u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_stake_tokens_result(11, 0, 11, 0, handle_res, &mut deps);

    let state: State = state_read(&mut deps.storage).load().unwrap();
    assert_eq!(
        state,
        State {
            contract_addr: deps
                .api
                .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
                .unwrap(),
            poll_count: 0,
            total_share: Uint128::from(11u128),
            total_deposit: Uint128::zero(),
        }
    );

    // double the balance, all balance withdrawn
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(22u128))],
    )]);

    let env = mock_env(TEST_VOTER, &[]);
    let msg = HandleMsg::WithdrawVotingTokens { amount: None };

    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    let msg = handle_res.messages.get(0).expect("no message");

    assert_eq!(
        msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from(VOTING_TOKEN),
            msg: to_binary(&Cw20HandleMsg::Transfer {
                recipient: HumanAddr::from(TEST_VOTER),
                amount: Uint128::from(22u128),
            })
            .unwrap(),
            send: vec![],
        })
    );

    let state: State = state_read(&mut deps.storage).load().unwrap();
    assert_eq!(
        state,
        State {
            contract_addr: deps
                .api
                .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
                .unwrap(),
            poll_count: 0,
            total_share: Uint128::zero(),
            total_deposit: Uint128::zero(),
        }
    );
}

#[test]
fn withdraw_voting_tokens_remove_not_in_progress_poll_voter_info() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);

    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(11u128))],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(11u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_stake_tokens_result(11, 0, 11, 0, handle_res, &mut deps);

    // make fake polls; one in progress & one in passed
    poll_store(&mut deps.storage)
        .save(
            &1u64.to_be_bytes(),
            &Poll {
                id: 1u64,
                creator: CanonicalAddr::default(),
                status: PollStatus::InProgress,
                yes_votes: Uint128::zero(),
                no_votes: Uint128::zero(),
                end_height: 0u64,
                title: "title".to_string(),
                description: "description".to_string(),
                deposit_amount: Uint128::zero(),
                link: None,
                execute_data: None,
                total_balance_at_end_poll: None,
                staked_amount: None,
            },
        )
        .unwrap();

    poll_store(&mut deps.storage)
        .save(
            &2u64.to_be_bytes(),
            &Poll {
                id: 1u64,
                creator: CanonicalAddr::default(),
                status: PollStatus::Passed,
                yes_votes: Uint128::zero(),
                no_votes: Uint128::zero(),
                end_height: 0u64,
                title: "title".to_string(),
                description: "description".to_string(),
                deposit_amount: Uint128::zero(),
                link: None,
                execute_data: None,
                total_balance_at_end_poll: None,
                staked_amount: None,
            },
        )
        .unwrap();

    let voter_addr_raw = deps
        .api
        .canonical_address(&HumanAddr::from(TEST_VOTER))
        .unwrap();
    poll_voter_store(&mut deps.storage, 1u64)
        .save(
            &voter_addr_raw.as_slice(),
            &VoterInfo {
                vote: VoteOption::Yes,
                balance: Uint128(5u128),
            },
        )
        .unwrap();
    poll_voter_store(&mut deps.storage, 2u64)
        .save(
            &voter_addr_raw.as_slice(),
            &VoterInfo {
                vote: VoteOption::Yes,
                balance: Uint128(5u128),
            },
        )
        .unwrap();
    bank_store(&mut deps.storage)
        .save(
            &voter_addr_raw.as_slice(),
            &TokenManager {
                share: Uint128(11u128),
                locked_balance: vec![
                    (
                        1u64,
                        VoterInfo {
                            vote: VoteOption::Yes,
                            balance: Uint128(5u128),
                        },
                    ),
                    (
                        2u64,
                        VoterInfo {
                            vote: VoteOption::Yes,
                            balance: Uint128(5u128),
                        },
                    ),
                ],
            },
        )
        .unwrap();

    // withdraw voting token must remove not in-progress votes infos from the store
    let env = mock_env(TEST_VOTER, &[]);
    let msg = HandleMsg::WithdrawVotingTokens {
        amount: Some(Uint128::from(5u128)),
    };

    let _ = handle(&mut deps, env, msg).unwrap();
    let voter = poll_voter_read(&deps.storage, 1u64)
        .load(&voter_addr_raw.as_slice())
        .unwrap();
    assert_eq!(
        voter,
        VoterInfo {
            vote: VoteOption::Yes,
            balance: Uint128(5u128),
        }
    );
    assert_eq!(
        poll_voter_read(&deps.storage, 2u64)
            .load(&voter_addr_raw.as_slice())
            .is_err(),
        true
    );

    let token_manager = bank_read(&deps.storage)
        .load(&voter_addr_raw.as_slice())
        .unwrap();
    assert_eq!(
        token_manager.locked_balance,
        vec![(
            1u64,
            VoterInfo {
                vote: VoteOption::Yes,
                balance: Uint128(5u128),
            }
        )]
    );
}

#[test]
fn fails_withdraw_voting_tokens_no_stake() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);

    let env = mock_env(TEST_VOTER, &coins(11, VOTING_TOKEN));
    let msg = HandleMsg::WithdrawVotingTokens {
        amount: Some(Uint128::from(11u128)),
    };

    let res = handle(&mut deps, env, msg);

    match res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Nothing staked"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn fails_withdraw_too_many_tokens() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);

    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(10u128))],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(10u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_stake_tokens_result(10, 0, 10, 0, handle_res, &mut deps);

    let env = mock_env(TEST_VOTER, &[]);
    let msg = HandleMsg::WithdrawVotingTokens {
        amount: Some(Uint128::from(11u128)),
    };

    let res = handle(&mut deps, env, msg);

    match res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => {
            assert_eq!(msg, "User is trying to withdraw too many tokens.")
        }
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn fails_cast_vote_twice() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);

    let env = mock_env_height(VOTING_TOKEN, &coins(2, VOTING_TOKEN), 0, 10000);

    let msg = create_poll_msg("test".to_string(), "test".to_string(), None, None);
    let handle_res = handle(&mut deps, env.clone(), msg.clone()).unwrap();

    assert_create_poll_result(
        1,
        env.block.height + DEFAULT_VOTING_PERIOD,
        TEST_CREATOR,
        handle_res,
        &mut deps,
    );

    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128(11u128 + DEFAULT_PROPOSAL_DEPOSIT),
        )],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(11u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_stake_tokens_result(11, DEFAULT_PROPOSAL_DEPOSIT, 11, 1, handle_res, &mut deps);

    let amount = 1u128;
    let msg = HandleMsg::CastVote {
        poll_id: 1,
        vote: VoteOption::Yes,
        amount: Uint128::from(amount),
    };
    let env = mock_env_height(TEST_VOTER, &[], 0, 10000);
    let handle_res = handle(&mut deps, env.clone(), msg).unwrap();
    assert_cast_vote_success(TEST_VOTER, amount, 1, VoteOption::Yes, handle_res);

    let msg = HandleMsg::CastVote {
        poll_id: 1,
        vote: VoteOption::Yes,
        amount: Uint128::from(amount),
    };
    let res = handle(&mut deps, env, msg);

    match res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "User has already voted."),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn fails_cast_vote_without_poll() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);

    let msg = HandleMsg::CastVote {
        poll_id: 0,
        vote: VoteOption::Yes,
        amount: Uint128::from(1u128),
    };
    let env = mock_env(TEST_VOTER, &coins(11, VOTING_TOKEN));

    let res = handle(&mut deps, env, msg);

    match res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Poll does not exist"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn happy_days_stake_voting_tokens() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);

    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(11u128))],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(11u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_stake_tokens_result(11, 0, 11, 0, handle_res, &mut deps);
}

#[test]
fn fails_insufficient_funds() {
    let mut deps = mock_dependencies(20, &[]);

    // initialize the store
    mock_init(&mut deps);

    // insufficient token
    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(0u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let res = handle(&mut deps, env, msg);

    match res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Insufficient funds sent"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn fails_staking_wrong_token() {
    let mut deps = mock_dependencies(20, &[]);

    // initialize the store
    mock_init(&mut deps);

    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(11u128))],
    )]);

    // wrong token
    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(11u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN.to_string() + "2", &[]);
    let res = handle(&mut deps, env, msg);

    match res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::Unauthorized { .. }) => {}
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn share_calculation() {
    let mut deps = mock_dependencies(20, &[]);

    // initialize the store
    mock_init(&mut deps);

    // create 100 share
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(100u128))],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(100u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN.to_string(), &[]);
    let _res = handle(&mut deps, env, msg);

    // add more balance(100) to make share:balance = 1:2
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128(200u128 + 100u128),
        )],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(100u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN.to_string(), &[]);
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(
        res.log,
        vec![
            log("action", "staking"),
            log("sender", TEST_VOTER),
            log("share", "50"),
            log("amount", "100"),
        ]
    );

    let msg = HandleMsg::WithdrawVotingTokens {
        amount: Some(Uint128(100u128)),
    };
    let env = mock_env(TEST_VOTER.to_string(), &[]);
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(
        res.log,
        vec![
            log("action", "withdraw"),
            log("recipient", TEST_VOTER),
            log("amount", "100"),
        ]
    );

    // 100 tokens withdrawn
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(200u128))],
    )]);

    let res = query(
        &mut deps,
        QueryMsg::Staker {
            address: HumanAddr::from(TEST_VOTER),
        },
    )
    .unwrap();
    let stake_info: StakerResponse = from_binary(&res).unwrap();
    assert_eq!(stake_info.share, Uint128(100));
    assert_eq!(stake_info.balance, Uint128(200));
    assert_eq!(stake_info.locked_balance, vec![]);
}

// helper to confirm the expected create_poll response
fn assert_create_poll_result(
    poll_id: u64,
    end_height: u64,
    creator: &str,
    handle_res: HandleResponse,
    deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>,
) {
    assert_eq!(
        handle_res.log,
        vec![
            log("action", "create_poll"),
            log("creator", creator),
            log("poll_id", poll_id.to_string()),
            log("end_height", end_height.to_string()),
        ]
    );

    //confirm poll count
    let state: State = state_read(&mut deps.storage).load().unwrap();
    assert_eq!(
        state,
        State {
            contract_addr: deps
                .api
                .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
                .unwrap(),
            poll_count: 1,
            total_share: Uint128::zero(),
            total_deposit: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
        }
    );
}

fn assert_stake_tokens_result(
    total_share: u128,
    total_deposit: u128,
    new_share: u128,
    poll_count: u64,
    handle_res: HandleResponse,
    deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>,
) {
    assert_eq!(
        handle_res.log.get(2).expect("no log"),
        &log("share", new_share.to_string())
    );

    let state: State = state_read(&mut deps.storage).load().unwrap();
    assert_eq!(
        state,
        State {
            contract_addr: deps
                .api
                .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
                .unwrap(),
            poll_count,
            total_share: Uint128(total_share),
            total_deposit: Uint128(total_deposit),
        }
    );
}

fn assert_cast_vote_success(
    voter: &str,
    amount: u128,
    poll_id: u64,
    vote_option: VoteOption,
    handle_res: HandleResponse,
) {
    assert_eq!(
        handle_res.log,
        vec![
            log("action", "cast_vote"),
            log("poll_id", poll_id.to_string()),
            log("amount", amount.to_string()),
            log("voter", voter),
            log("vote_option", vote_option.to_string()),
        ]
    );
}

#[test]
fn update_config() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);

    // update owner
    let env = mock_env(TEST_CREATOR, &[]);
    let msg = HandleMsg::UpdateConfig {
        owner: Some(HumanAddr("addr0001".to_string())),
        quorum: None,
        threshold: None,
        voting_period: None,
        timelock_period: None,
        expiration_period: None,
        proposal_deposit: None,
        snapshot_period: None,
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // it worked, let's query the state
    let res = query(&deps, QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!("addr0001", config.owner.as_str());
    assert_eq!(Decimal::percent(DEFAULT_QUORUM), config.quorum);
    assert_eq!(Decimal::percent(DEFAULT_THRESHOLD), config.threshold);
    assert_eq!(DEFAULT_VOTING_PERIOD, config.voting_period);
    assert_eq!(DEFAULT_TIMELOCK_PERIOD, config.timelock_period);
    assert_eq!(DEFAULT_PROPOSAL_DEPOSIT, config.proposal_deposit.u128());

    // update left items
    let env = mock_env("addr0001", &[]);
    let msg = HandleMsg::UpdateConfig {
        owner: None,
        quorum: Some(Decimal::percent(20)),
        threshold: Some(Decimal::percent(75)),
        voting_period: Some(20000u64),
        timelock_period: Some(20000u64),
        expiration_period: Some(30000u64),
        proposal_deposit: Some(Uint128(123u128)),
        snapshot_period: Some(11),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // it worked, let's query the state
    let res = query(&deps, QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!("addr0001", config.owner.as_str());
    assert_eq!(Decimal::percent(20), config.quorum);
    assert_eq!(Decimal::percent(75), config.threshold);
    assert_eq!(20000u64, config.voting_period);
    assert_eq!(20000u64, config.timelock_period);
    assert_eq!(30000u64, config.expiration_period);
    assert_eq!(123u128, config.proposal_deposit.u128());
    assert_eq!(11u64, config.snapshot_period);

    // Unauthorzied err
    let env = mock_env(TEST_CREATOR, &[]);
    let msg = HandleMsg::UpdateConfig {
        owner: None,
        quorum: None,
        threshold: None,
        voting_period: None,
        timelock_period: None,
        expiration_period: None,
        proposal_deposit: None,
        snapshot_period: None,
    };

    let res = handle(&mut deps, env, msg);
    match res {
        Err(StdError::Unauthorized { .. }) => {}
        _ => panic!("Must return unauthorized error"),
    }
}

#[test]
fn add_several_execute_msgs() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);
    let env = mock_env_height(VOTING_TOKEN, &vec![], 0, 10000);

    let exec_msg_bz = to_binary(&Cw20HandleMsg::Burn {
        amount: Uint128(123),
    })
    .unwrap();

    let exec_msg_bz2 = to_binary(&Cw20HandleMsg::Burn {
        amount: Uint128(12),
    })
    .unwrap();

    let exec_msg_bz3 = to_binary(&Cw20HandleMsg::Burn { amount: Uint128(1) }).unwrap();

    // push two execute msgs to the list
    let mut execute_msgs: Vec<ExecuteMsg> = vec![];

    execute_msgs.push(ExecuteMsg {
        order: 1u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz.clone(),
    });

    execute_msgs.push(ExecuteMsg {
        order: 3u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz3.clone(),
    });

    execute_msgs.push(ExecuteMsg {
        order: 2u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz2.clone(),
    });

    let msg = create_poll_msg(
        "test".to_string(),
        "test".to_string(),
        None,
        Some(execute_msgs.clone()),
    );

    let handle_res = handle(&mut deps, env.clone(), msg.clone()).unwrap();
    assert_create_poll_result(
        1,
        env.block.height + DEFAULT_VOTING_PERIOD,
        TEST_CREATOR,
        handle_res.clone(),
        &mut deps,
    );

    let res = query(&deps, QueryMsg::Poll { poll_id: 1 }).unwrap();
    let value: PollResponse = from_binary(&res).unwrap();

    let response_execute_data = value.execute_data.unwrap();
    assert_eq!(response_execute_data.len(), 3);
    assert_eq!(response_execute_data, execute_msgs);
}

#[test]
fn execute_poll_with_order() {
    const POLL_START_HEIGHT: u64 = 1000;
    const POLL_ID: u64 = 1;
    let stake_amount = 1000;

    let mut deps = mock_dependencies(20, &coins(1000, VOTING_TOKEN));
    mock_init(&mut deps);
    let mut creator_env = mock_env_height(
        VOTING_TOKEN,
        &coins(2, VOTING_TOKEN),
        POLL_START_HEIGHT,
        10000,
    );

    let exec_msg_bz = to_binary(&Cw20HandleMsg::Burn {
        amount: Uint128(10),
    })
    .unwrap();

    let exec_msg_bz2 = to_binary(&Cw20HandleMsg::Burn {
        amount: Uint128(20),
    })
    .unwrap();

    let exec_msg_bz3 = to_binary(&Cw20HandleMsg::Burn {
        amount: Uint128(30),
    })
    .unwrap();
    let exec_msg_bz4 = to_binary(&Cw20HandleMsg::Burn {
        amount: Uint128(40),
    })
    .unwrap();
    let exec_msg_bz5 = to_binary(&Cw20HandleMsg::Burn {
        amount: Uint128(50),
    })
    .unwrap();

    //add three messages with different order
    let mut execute_msgs: Vec<ExecuteMsg> = vec![];

    execute_msgs.push(ExecuteMsg {
        order: 3u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz3.clone(),
    });

    execute_msgs.push(ExecuteMsg {
        order: 4u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz4.clone(),
    });

    execute_msgs.push(ExecuteMsg {
        order: 2u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz2.clone(),
    });

    execute_msgs.push(ExecuteMsg {
        order: 5u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz5.clone(),
    });

    execute_msgs.push(ExecuteMsg {
        order: 1u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz.clone(),
    });

    let msg = create_poll_msg(
        "test".to_string(),
        "test".to_string(),
        None,
        Some(execute_msgs),
    );

    let handle_res = handle(&mut deps, creator_env.clone(), msg).unwrap();

    assert_create_poll_result(
        1,
        creator_env.block.height + DEFAULT_VOTING_PERIOD,
        TEST_CREATOR,
        handle_res,
        &mut deps,
    );

    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128((stake_amount + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(stake_amount as u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_stake_tokens_result(
        stake_amount,
        DEFAULT_PROPOSAL_DEPOSIT,
        stake_amount,
        1,
        handle_res,
        &mut deps,
    );

    let msg = HandleMsg::CastVote {
        poll_id: 1,
        vote: VoteOption::Yes,
        amount: Uint128::from(stake_amount),
    };
    let env = mock_env_height(TEST_VOTER, &[], POLL_START_HEIGHT, 10000);
    let handle_res = handle(&mut deps, env, msg).unwrap();

    assert_eq!(
        handle_res.log,
        vec![
            log("action", "cast_vote"),
            log("poll_id", POLL_ID),
            log("amount", "1000"),
            log("voter", TEST_VOTER),
            log("vote_option", "yes"),
        ]
    );

    creator_env.message.sender = HumanAddr::from(TEST_CREATOR);
    creator_env.block.height = &creator_env.block.height + DEFAULT_VOTING_PERIOD;

    let msg = HandleMsg::EndPoll { poll_id: 1 };
    let handle_res = handle(&mut deps, creator_env.clone(), msg).unwrap();

    assert_eq!(
        handle_res.log,
        vec![
            log("action", "end_poll"),
            log("poll_id", "1"),
            log("rejected_reason", ""),
            log("passed", "true"),
        ]
    );
    assert_eq!(
        handle_res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from(VOTING_TOKEN),
            msg: to_binary(&Cw20HandleMsg::Transfer {
                recipient: HumanAddr::from(TEST_CREATOR),
                amount: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
            })
            .unwrap(),
            send: vec![],
        })]
    );

    // End poll will withdraw deposit balance
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128(stake_amount as u128),
        )],
    )]);

    creator_env.block.height = &creator_env.block.height + DEFAULT_TIMELOCK_PERIOD;
    let msg = HandleMsg::ExecutePoll { poll_id: 1 };
    let handle_res = handle(&mut deps, creator_env, msg).unwrap();
    assert_eq!(
        handle_res.messages,
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(VOTING_TOKEN),
                msg: exec_msg_bz,
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(VOTING_TOKEN),
                msg: exec_msg_bz2,
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(VOTING_TOKEN),
                msg: exec_msg_bz3,
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(VOTING_TOKEN),
                msg: exec_msg_bz4,
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(VOTING_TOKEN),
                msg: exec_msg_bz5,
                send: vec![],
            }),
        ]
    );
    assert_eq!(
        handle_res.log,
        vec![log("action", "execute_poll"), log("poll_id", "1"),]
    );
}

#[test]
fn snapshot_poll() {
    let stake_amount = 1000;

    let mut deps = mock_dependencies(20, &coins(100, VOTING_TOKEN));
    mock_init(&mut deps);

    let msg = create_poll_msg("test".to_string(), "test".to_string(), None, None);
    let mut creator_env = mock_env(VOTING_TOKEN, &vec![]);
    let handle_res = handle(&mut deps, creator_env.clone(), msg.clone()).unwrap();
    assert_eq!(
        handle_res.log,
        vec![
            log("action", "create_poll"),
            log("creator", TEST_CREATOR),
            log("poll_id", "1"),
            log("end_height", "22345"),
        ]
    );

    //must not be executed
    let snapshot_err = handle(
        &mut deps,
        creator_env.clone(),
        HandleMsg::SnapshotPoll { poll_id: 1 },
    )
    .unwrap_err();
    assert_eq!(
        StdError::generic_err("Cannot snapshot at this height",),
        snapshot_err
    );

    // change time
    creator_env.block.height = 22345 - 10;

    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128((stake_amount + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    let fix_res = handle(
        &mut deps,
        creator_env.clone(),
        HandleMsg::SnapshotPoll { poll_id: 1 },
    )
    .unwrap();

    assert_eq!(
        fix_res.log,
        vec![
            log("action", "snapshot_poll"),
            log("poll_id", "1"),
            log("staked_amount", stake_amount),
        ]
    );

    //must not be executed
    let snapshot_error = handle(
        &mut deps,
        creator_env.clone(),
        HandleMsg::SnapshotPoll { poll_id: 1 },
    )
    .unwrap_err();
    assert_eq!(
        StdError::generic_err("Snapshot has already occurred"),
        snapshot_error
    );
}

#[test]
fn happy_days_cast_vote_with_snapshot() {
    let mut deps = mock_dependencies(20, &[]);
    mock_init(&mut deps);

    let env = mock_env_height(VOTING_TOKEN, &vec![], 0, 10000);
    let msg = create_poll_msg("test".to_string(), "test".to_string(), None, None);

    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_create_poll_result(
        1,
        DEFAULT_VOTING_PERIOD,
        TEST_CREATOR,
        handle_res,
        &mut deps,
    );

    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128(11u128 + DEFAULT_PROPOSAL_DEPOSIT),
        )],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(11u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_stake_tokens_result(11, DEFAULT_PROPOSAL_DEPOSIT, 11, 1, handle_res, &mut deps);

    //cast_vote without snapshot
    let env = mock_env_height(TEST_VOTER, &coins(11, VOTING_TOKEN), 0, 10000);
    let amount = 10u128;

    let msg = HandleMsg::CastVote {
        poll_id: 1,
        vote: VoteOption::Yes,
        amount: Uint128::from(amount),
    };

    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_cast_vote_success(TEST_VOTER, amount, 1, VoteOption::Yes, handle_res);

    // balance be double
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128(22u128 + DEFAULT_PROPOSAL_DEPOSIT),
        )],
    )]);

    let res = query(&deps, QueryMsg::Poll { poll_id: 1 }).unwrap();
    let value: PollResponse = from_binary(&res).unwrap();
    assert_eq!(value.staked_amount, None);
    let end_height = value.end_height;

    //cast another vote
    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER_2),
        amount: Uint128::from(11u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let _handle_res = handle(&mut deps, env, msg.clone()).unwrap();

    // another voter cast a vote
    let msg = HandleMsg::CastVote {
        poll_id: 1,
        vote: VoteOption::Yes,
        amount: Uint128::from(10u128),
    };
    let env = mock_env_height(TEST_VOTER_2, &[], end_height - 9, 10000);
    let handle_res = handle(&mut deps, env.clone(), msg).unwrap();
    assert_cast_vote_success(TEST_VOTER_2, amount, 1, VoteOption::Yes, handle_res);

    let res = query(&deps, QueryMsg::Poll { poll_id: 1 }).unwrap();
    let value: PollResponse = from_binary(&res).unwrap();
    assert_eq!(value.staked_amount, Some(Uint128(22)));

    // snanpshot poll will not go through
    let snap_error = handle(
        &mut deps,
        env.clone(),
        HandleMsg::SnapshotPoll { poll_id: 1 },
    )
    .unwrap_err();
    assert_eq!(
        StdError::generic_err("Snapshot has already occurred"),
        snap_error
    );

    // balance be double
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128(33u128 + DEFAULT_PROPOSAL_DEPOSIT),
        )],
    )]);

    // another voter cast a vote but the snapshot is already occurred
    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER_3),
        amount: Uint128::from(11u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let _handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    let msg = HandleMsg::CastVote {
        poll_id: 1,
        vote: VoteOption::Yes,
        amount: Uint128::from(10u128),
    };
    let env = mock_env_height(TEST_VOTER_3, &[], end_height - 8, 10000);
    let handle_res = handle(&mut deps, env.clone(), msg).unwrap();
    assert_cast_vote_success(TEST_VOTER_3, amount, 1, VoteOption::Yes, handle_res);

    let res = query(&deps, QueryMsg::Poll { poll_id: 1 }).unwrap();
    let value: PollResponse = from_binary(&res).unwrap();
    assert_eq!(value.staked_amount, Some(Uint128(22)));
}

#[test]
fn fails_end_poll_quorum_inflation_without_snapshot_poll() {
    const POLL_START_HEIGHT: u64 = 1000;
    const POLL_ID: u64 = 1;
    let stake_amount = 1000;

    let mut deps = mock_dependencies(20, &coins(1000, VOTING_TOKEN));
    mock_init(&mut deps);

    let mut creator_env = mock_env_height(
        VOTING_TOKEN,
        &coins(2, VOTING_TOKEN),
        POLL_START_HEIGHT,
        10000,
    );

    let exec_msg_bz = to_binary(&Cw20HandleMsg::Burn {
        amount: Uint128(123),
    })
    .unwrap();

    //add two messages
    let mut execute_msgs: Vec<ExecuteMsg> = vec![];
    execute_msgs.push(ExecuteMsg {
        order: 1u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz.clone(),
    });

    execute_msgs.push(ExecuteMsg {
        order: 2u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz.clone(),
    });

    let msg = create_poll_msg(
        "test".to_string(),
        "test".to_string(),
        None,
        Some(execute_msgs),
    );

    let handle_res = handle(&mut deps, creator_env.clone(), msg).unwrap();

    assert_create_poll_result(
        1,
        creator_env.block.height + DEFAULT_VOTING_PERIOD,
        TEST_CREATOR,
        handle_res,
        &mut deps,
    );

    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128((stake_amount + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(stake_amount as u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_stake_tokens_result(
        stake_amount,
        DEFAULT_PROPOSAL_DEPOSIT,
        stake_amount,
        1,
        handle_res,
        &mut deps,
    );

    let msg = HandleMsg::CastVote {
        poll_id: 1,
        vote: VoteOption::Yes,
        amount: Uint128::from(stake_amount),
    };
    let env = mock_env_height(TEST_VOTER, &[], POLL_START_HEIGHT, 10000);
    let handle_res = handle(&mut deps, env, msg).unwrap();

    assert_eq!(
        handle_res.log,
        vec![
            log("action", "cast_vote"),
            log("poll_id", POLL_ID),
            log("amount", "1000"),
            log("voter", TEST_VOTER),
            log("vote_option", "yes"),
        ]
    );

    creator_env.block.height = &creator_env.block.height + DEFAULT_VOTING_PERIOD - 10;

    // did not SnapshotPoll

    // staked amount get increased 10 times
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128(((10 * stake_amount) + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    //cast another vote
    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER_2),
        amount: Uint128::from(8 * stake_amount as u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let _handle_res = handle(&mut deps, env, msg.clone()).unwrap();

    // another voter cast a vote
    let msg = HandleMsg::CastVote {
        poll_id: 1,
        vote: VoteOption::Yes,
        amount: Uint128::from(stake_amount),
    };
    let env = mock_env_height(TEST_VOTER_2, &[], creator_env.block.height, 10000);
    let handle_res = handle(&mut deps, env, msg).unwrap();

    assert_eq!(
        handle_res.log,
        vec![
            log("action", "cast_vote"),
            log("poll_id", POLL_ID),
            log("amount", "1000"),
            log("voter", TEST_VOTER_2),
            log("vote_option", "yes"),
        ]
    );

    creator_env.message.sender = HumanAddr::from(TEST_CREATOR);
    creator_env.block.height += 10;

    // quorum must reach
    let msg = HandleMsg::EndPoll { poll_id: 1 };
    let handle_res = handle(&mut deps, creator_env.clone(), msg).unwrap();

    assert_eq!(
        handle_res.log,
        vec![
            log("action", "end_poll"),
            log("poll_id", "1"),
            log("rejected_reason", "Quorum not reached"),
            log("passed", "false"),
        ]
    );

    let res = query(&deps, QueryMsg::Poll { poll_id: 1 }).unwrap();
    let value: PollResponse = from_binary(&res).unwrap();
    assert_eq!(
        10 * stake_amount,
        value.total_balance_at_end_poll.unwrap().u128()
    );
}

#[test]
fn happy_days_end_poll_with_controlled_quorum() {
    const POLL_START_HEIGHT: u64 = 1000;
    const POLL_ID: u64 = 1;
    let stake_amount = 1000;

    let mut deps = mock_dependencies(20, &coins(1000, VOTING_TOKEN));
    mock_init(&mut deps);

    let mut creator_env = mock_env_height(
        VOTING_TOKEN,
        &coins(2, VOTING_TOKEN),
        POLL_START_HEIGHT,
        10000,
    );

    let exec_msg_bz = to_binary(&Cw20HandleMsg::Burn {
        amount: Uint128(123),
    })
    .unwrap();

    //add two messages
    let mut execute_msgs: Vec<ExecuteMsg> = vec![];
    execute_msgs.push(ExecuteMsg {
        order: 1u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz.clone(),
    });

    execute_msgs.push(ExecuteMsg {
        order: 2u64,
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: exec_msg_bz.clone(),
    });

    let msg = create_poll_msg(
        "test".to_string(),
        "test".to_string(),
        None,
        Some(execute_msgs),
    );

    let handle_res = handle(&mut deps, creator_env.clone(), msg).unwrap();

    assert_create_poll_result(
        1,
        creator_env.block.height + DEFAULT_VOTING_PERIOD,
        TEST_CREATOR,
        handle_res,
        &mut deps,
    );

    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128((stake_amount + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: Uint128::from(stake_amount as u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_stake_tokens_result(
        stake_amount,
        DEFAULT_PROPOSAL_DEPOSIT,
        stake_amount,
        1,
        handle_res,
        &mut deps,
    );

    let msg = HandleMsg::CastVote {
        poll_id: 1,
        vote: VoteOption::Yes,
        amount: Uint128::from(stake_amount),
    };
    let env = mock_env_height(TEST_VOTER, &[], POLL_START_HEIGHT, 10000);
    let handle_res = handle(&mut deps, env, msg).unwrap();

    assert_eq!(
        handle_res.log,
        vec![
            log("action", "cast_vote"),
            log("poll_id", POLL_ID),
            log("amount", "1000"),
            log("voter", TEST_VOTER),
            log("vote_option", "yes"),
        ]
    );

    creator_env.block.height = &creator_env.block.height + DEFAULT_VOTING_PERIOD - 10;

    // send SnapshotPoll
    let fix_res = handle(
        &mut deps,
        creator_env.clone(),
        HandleMsg::SnapshotPoll { poll_id: 1 },
    )
    .unwrap();

    assert_eq!(
        fix_res.log,
        vec![
            log("action", "snapshot_poll"),
            log("poll_id", "1"),
            log("staked_amount", stake_amount),
        ]
    );

    // staked amount get increased 10 times
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(
            &HumanAddr::from(MOCK_CONTRACT_ADDR),
            &Uint128(((10 * stake_amount) + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    //cast another vote
    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER_2),
        amount: Uint128::from(8 * stake_amount as u128),
        msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
    });

    let env = mock_env(VOTING_TOKEN, &[]);
    let _handle_res = handle(&mut deps, env, msg.clone()).unwrap();

    let msg = HandleMsg::CastVote {
        poll_id: 1,
        vote: VoteOption::Yes,
        amount: Uint128::from(8 * stake_amount),
    };
    let env = mock_env_height(TEST_VOTER_2, &[], creator_env.block.height, 10000);
    let handle_res = handle(&mut deps, env, msg).unwrap();

    assert_eq!(
        handle_res.log,
        vec![
            log("action", "cast_vote"),
            log("poll_id", POLL_ID),
            log("amount", "8000"),
            log("voter", TEST_VOTER_2),
            log("vote_option", "yes"),
        ]
    );

    creator_env.message.sender = HumanAddr::from(TEST_CREATOR);
    creator_env.block.height += 10;

    // quorum must reach
    let msg = HandleMsg::EndPoll { poll_id: 1 };
    let handle_res = handle(&mut deps, creator_env.clone(), msg).unwrap();

    assert_eq!(
        handle_res.log,
        vec![
            log("action", "end_poll"),
            log("poll_id", "1"),
            log("rejected_reason", ""),
            log("passed", "true"),
        ]
    );
    assert_eq!(
        handle_res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from(VOTING_TOKEN),
            msg: to_binary(&Cw20HandleMsg::Transfer {
                recipient: HumanAddr::from(TEST_CREATOR),
                amount: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
            })
            .unwrap(),
            send: vec![],
        })]
    );

    let res = query(&deps, QueryMsg::Poll { poll_id: 1 }).unwrap();
    let value: PollResponse = from_binary(&res).unwrap();
    assert_eq!(
        stake_amount,
        value.total_balance_at_end_poll.unwrap().u128()
    );

    assert_eq!(value.yes_votes.u128(), 9 * stake_amount);

    // actual staked amount is 10 times bigger than staked amount
    let actual_staked_weight = (load_token_balance(
        &deps,
        &HumanAddr::from(VOTING_TOKEN),
        &deps
            .api
            .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
            .unwrap(),
    )
    .unwrap()
        - Uint128(DEFAULT_PROPOSAL_DEPOSIT))
    .unwrap();

    assert_eq!(actual_staked_weight.u128(), (10 * stake_amount))
}
