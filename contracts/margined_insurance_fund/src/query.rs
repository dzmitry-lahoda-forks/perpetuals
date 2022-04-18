use cosmwasm_std::{Deps, StdResult};
use margined_perp::margined_insurance_fund::{AllVammResponse, ConfigResponse, VammResponse};

use crate::state::{is_vamm, read_config, read_vammlist, Config};
/// Queries contract config
pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config: Config = read_config(deps.storage)?;

    Ok(ConfigResponse {
        owner: config.owner,
    })
}

/// Queries if the vAMM with given address is already stored
pub fn query_is_vamm(deps: Deps, vamm: String) -> StdResult<VammResponse> {
    // validate address
    let vamm_valid = deps.api.addr_validate(&vamm)?;

    // read the current storage and pull the vamm list
    let vamm_bool = is_vamm(deps.storage, vamm_valid);

    Ok(VammResponse { is_vamm: vamm_bool })
}

//Queries multiple vAMMs TODO: add the option to query a slice
pub fn query_mult_vamm(deps: Deps) -> StdResult<AllVammResponse> {
    let out = read_vammlist(deps, deps.storage)?;
    Ok(AllVammResponse { vamm_list: out })
}
