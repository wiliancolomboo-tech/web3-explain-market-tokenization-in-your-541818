use anchor_lang::prelude::*;

pub mod error;
pub mod instructions;
pub mod state;

use instructions::initialize_asset::*;
use instructions::register_investor::*;
use instructions::transfer_hook::*;

declare_id!("RWA1111111111111111111111111111111111111111");

#[program]
pub mod solrwa_compliance {
    use super::*;

    pub fn initialize_asset(
        ctx: Context<InitializeAsset>,
        max_investors: u32,
        reg_d_lockup_seconds: i64,
        country_code: u16,
    ) -> Result<()> {
        instructions::initialize_asset::handler(ctx, max_investors, reg_d_lockup_seconds, country_code)
    }

    pub fn register_investor(
        ctx: Context<RegisterInvestor>,
        country_code: u16,
        is_accredited: bool,
        validity_duration_seconds: i64,
    ) -> Result<()> {
        instructions::register_investor::handler(ctx, country_code, is_accredited, validity_duration_seconds)
    }

    #[interface(spl_transfer_hook_interface::execute)]
    pub fn execute_transfer_hook(ctx: Context<ExecuteTransferHook>, amount: u64) -> Result<()> {
        instructions::transfer_hook::handler(ctx, amount)
    }
}