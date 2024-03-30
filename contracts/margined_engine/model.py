



from dataclasses import dataclass

@dataclass
class Position:
    size : int
    margin: int
    notional : int
    last_updated_premium_fraction: int
    updated_at : int
    unrealized_pnl : int
    open_position_notional : int
    def __post_init__(self):
        assert(self.size != 0)
        assert(self.margin > 0)
        assert(self.notional > 0)
        assert(self.updated_at >= 0)
        
@dataclass
class Market:
    initial_margin_ratio: int
    maintenance_ratio: int
    liquidation_fee : int
    funding_period: int
    eligible_collateral : int
    partial_liquidation_ratio: int
    decimals: int
    open_interest_notional: int
    open_notional:int
    position_open_notional: int
    prepaid_bad_debt: int
    unrealized_pnl : int
    insurance_fund: int
    fee_pool: int
    funding_period: int
    fluctuation_limit_ratio: int
    positions : [Position]
    
    def __post_init__(self):
        assert(self.initial_margin_ratio >= self.maintenance_ratio)
        assert(self.initial_margin_ratio > 0)
        assert(self.liquidation_fee >=0)
        assert(self.funding_period > 0)

if __name__ == "__main__":
    market = Market(0.1, 0.05, 0.01, 3600, 1000000, 0.1, 18, 0, 0, 0, 0, 0, 0, 0, 0, 0, [])
    
    market.open_position()
    # deposit margin
    # withdraw margin
    # pay funding
    # liquidate position
    # close position