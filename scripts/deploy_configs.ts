export const testnet: Config = {
  initialAssets: [],
  insuranceFundInitMsg: {},
  priceFeedInitMsg: {
    decimals: 6,
    oracle_hub_contract: '',
  },
  engineInitMsg: {
    decimals: 6,
    insurance_fund: '',
    fee_pool: '',
    eligible_collateral: undefined,
    initial_margin_ratio: '50000',
    maintenance_margin_ratio: '50000',
    liquidation_fee: '50000',
  },
  vammInitMsg: {
    decimals: 6,
    pricefeed: undefined,
    quote_asset: 'USDC',
    base_asset: 'ETH',
    quote_asset_reserve: '1200000000000',
    base_asset_reserve: '1000000000',
    funding_period: 3_600, // 1 hour in seconds
    toll_ratio: '0',
    spread_ratio: '0',
    fluctuation_limit_ratio: '0',
  },
  cw20InitMsg: {
    name: 'margined_usd',
    symbol: 'musd',
    decimals: 6,
    initial_balances: [],
    mint: undefined,
    marketing: undefined,
  },
}

export const local: Config = {
  initialAssets: [],
  insuranceFundInitMsg: {},
  priceFeedInitMsg: {
    decimals: 6,
    oracle_hub_contract: '',
  },
  engineInitMsg: {
    decimals: 6,
    insurance_fund: '',
    fee_pool: '',
    eligible_collateral: undefined,
    initial_margin_ratio: '50000',
    maintenance_margin_ratio: '50000',
    liquidation_fee: '50000',
  },
  vammInitMsg: {
    decimals: 6,
    pricefeed: undefined,
    quote_asset: 'USDC',
    base_asset: 'ETH',
    quote_asset_reserve: '1200000000000',
    base_asset_reserve: '1000000000',
    funding_period: 3_600, // 1 hour in seconds
    toll_ratio: '0',
    spread_ratio: '0',
    fluctuation_limit_ratio: '0',
  },
  cw20InitMsg: {
    name: 'margined_usd',
    symbol: 'musd',
    decimals: 6,
    initial_balances: [],
    mint: undefined,
    marketing: undefined,
  },
}
