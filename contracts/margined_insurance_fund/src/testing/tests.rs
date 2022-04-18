use crate::contract::{execute, instantiate, query};
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{from_binary, Addr};
use margined_perp::margined_insurance_fund::{
    AllVammResponse, ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, VammResponse,
};

#[test]
fn test_instantiation() {
    let mut deps = mock_dependencies(&[]);
    let msg = InstantiateMsg {};
    let info = mock_info("addr0000", &[]);
    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    let info = mock_info("addr0000", &[]);
    assert_eq!(config, ConfigResponse { owner: info.sender });
}

#[test]
fn test_update_config() {
    let mut deps = mock_dependencies(&[]);
    let msg = InstantiateMsg {};
    let info = mock_info("addr0000", &[]);
    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    // Update the config
    let msg = ExecuteMsg::UpdateConfig {
        owner: Some("addr0001".to_string()),
    };

    let info = mock_info("addr0000", &[]);
    execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!(
        config,
        ConfigResponse {
            owner: Addr::unchecked("addr0001".to_string()),
        }
    );
}
#[test]
fn test_query_vamm() {
    //instantiate contract here
    let mut deps = mock_dependencies(&[]);
    let msg = InstantiateMsg {};
    let info = mock_info("addr0000", &[]);

    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    //add an vAMM
    let addr1 = "addr0001".to_string();

    let info = mock_info("addr0000", &[]);
    let msg = ExecuteMsg::AddVamm { vamm: addr1 };

    execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    //check for the added vAMM
    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::IsVamm {
            vamm: "addr0001".to_string(),
        },
    )
    .unwrap();

    let res: VammResponse = from_binary(&res).unwrap();
    let is_vamm = res.is_vamm;

    assert_eq!(is_vamm, true);
}

#[test]
fn test_query_all_vamm() {
    //instantiate contract here
    let mut deps = mock_dependencies(&[]);
    let msg = InstantiateMsg {};
    let info = mock_info("addr0000", &[]);

    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    //check to see that there are no vAMMs
    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::IsVamm {
            vamm: "addr0001".to_string(),
        },
    )
    .unwrap();

    let res: VammResponse = from_binary(&res).unwrap();
    let is_vamm = res.is_vamm;

    assert_eq!(is_vamm, false);

    //add an vAMM
    let addr1 = "addr0001".to_string();

    let info = mock_info("addr0000", &[]);
    let msg = ExecuteMsg::AddVamm { vamm: addr1 };

    execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    //add another vAMM
    let addr2 = "addr0002".to_string();

    let info = mock_info("addr0000", &[]);
    let msg = ExecuteMsg::AddVamm { vamm: addr2 };

    execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    //check for the added vAMMs
    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::GetAllVamm {},
    )
    .unwrap();

    let res: AllVammResponse = from_binary(&res).unwrap();
    let list = res.vamm_list;

    assert_eq!(list, vec![Addr::unchecked("addr0001".to_string()), Addr::unchecked("addr0002".to_string())]);

    //////////////////
    //query all here//
    //////////////////
}

#[test]
fn test_add_vamm() {
    //instantiate contract here
    let mut deps = mock_dependencies(&[]);
    let msg = InstantiateMsg {};
    let info = mock_info("addr0000", &[]);

    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    //query the vAMM we want to add
    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::IsVamm {
            vamm: "addr0001".to_string(),
        },
    )
    .unwrap();

    let res: VammResponse = from_binary(&res).unwrap();
    let is_vamm = res.is_vamm;

    assert_eq!(is_vamm, false);

    //add an vAMM
    let addr1 = "addr0001".to_string();

    let info = mock_info("addr0000", &[]);
    let msg = ExecuteMsg::AddVamm { vamm: addr1 };

    execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    //check for the added vAMM
    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::IsVamm {
            vamm: "addr0001".to_string(),
        },
    )
    .unwrap();

    let res: VammResponse = from_binary(&res).unwrap();
    let is_vamm = res.is_vamm;

    assert_eq!(is_vamm, true);
}

#[test]
fn test_add_second_vamm() {
    // this tests for adding a second vAMM, to ensure the 'push' match arm of save_vamm is used

    //instantiate contract here
    let mut deps = mock_dependencies(&[]);
    let msg = InstantiateMsg {};
    let info = mock_info("addr0000", &[]);

    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    //add first vAMM
    let addr1 = "addr0001".to_string();

    let info = mock_info("addr0000", &[]);
    let msg = ExecuteMsg::AddVamm { vamm: addr1 };

    execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    //add second vAMM
    let addr2 = "addr0002".to_string();

    let info = mock_info("addr0000", &[]);
    let msg = ExecuteMsg::AddVamm { vamm: addr2 };

    execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    //check for the second added vAMM
    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::IsVamm {
            vamm: "addr0002".to_string(),
        },
    )
    .unwrap();

    let res: VammResponse = from_binary(&res).unwrap();
    let is_vamm = res.is_vamm;

    assert_eq!(is_vamm, true);
}

#[test]
fn test_remove_vamm() {
    //instantiate contract here
    let mut deps = mock_dependencies(&[]);
    let msg = InstantiateMsg {};
    let info = mock_info("addr0000", &[]);

    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    //add an vAMM
    let addr1 = "addr0001".to_string();

    let info = mock_info("addr0000", &[]);
    let msg = ExecuteMsg::AddVamm { vamm: addr1 };

    execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    //check to see that there is one vAMM
    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::IsVamm {
            vamm: "addr0001".to_string(),
        },
    )
    .unwrap();

    let res: VammResponse = from_binary(&res).unwrap();
    let is_vamm = res.is_vamm;

    assert_eq!(is_vamm, true);

    //remove an AMM
    let addr1 = "addr0001".to_string();

    let info = mock_info("addr0000", &[]);
    let msg = ExecuteMsg::RemoveVamm { vamm: addr1 };

    execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    //check that there are zero AMMs
    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::IsVamm {
            vamm: "addr0001".to_string(),
        },
    )
    .unwrap();

    let res: VammResponse = from_binary(&res).unwrap();
    let is_vamm = res.is_vamm;

    assert_eq!(is_vamm, false);
}

#[test]
fn test_not_owner() {
    //instantiate contract here
    let mut deps = mock_dependencies(&[]);
    let msg = InstantiateMsg {};
    let info = mock_info("addr0000", &[]);

    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    // try to update the config
    let msg = ExecuteMsg::UpdateConfig {
        owner: Some("addr0001".to_string()),
    };

    let info = mock_info("not_the_owner", &[]);

    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();

    assert_eq!(res.to_string(), "Unauthorized");

    // try to add a vAMM
    let addr1 = "addr0001".to_string();

    let info = mock_info("not_the_owner", &[]);
    let msg = ExecuteMsg::AddVamm { vamm: addr1 };

    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();

    assert_eq!(res.to_string(), "Unauthorized");

    //try to remove an vAMM
    let addr1 = "addr0001".to_string();

    let info = mock_info("not_the_owner", &[]);
    let msg = ExecuteMsg::RemoveVamm { vamm: addr1 };

    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();

    assert_eq!(res.to_string(), "Unauthorized");
}
