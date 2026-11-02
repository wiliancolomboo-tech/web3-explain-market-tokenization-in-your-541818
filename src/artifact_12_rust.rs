use anchor_lang::prelude::*;

pub mod error;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("EWPG111111111111111111111111111111111111111");

#[program]
pub mod ewpg_solana {
    use super::*;

    pub fn initialize_register(
        ctx: Context<InitializeRegister>,
        isin: [u8; 12],
        total_nominal_volume: u64,
        coupon_rate_bps: u16,
        maturity_date: i64,
    ) -> Result<()> {
        instructions::initialize_register::handler(
            ctx,
            isin,
            total_nominal_volume,
            coupon_rate_bps,
            maturity_date,
        )
    }

    pub fn set_investor_whitelist(
        ctx: Context<ManageInvestorWhitelist>,
        mifid_category: u8,
        jurisdiction_code: [u8; 2],
        kyc_expiry: i64,
        is_sanctioned: bool,
    ) -> Result<()> {
        instructions::investor_whitelist::set_whitelist_status(
            ctx,
            mifid_category,
            jurisdiction_code,
            kyc_expiry,
            is_sanctioned,
        )
    }

    pub fn verify_transfer_hook(
        ctx: Context<VerifyTransferCompliance>,
        amount: u64,
    ) -> Result<()> {
        instructions::transfer_hook::execute_transfer_verification(ctx, amount)
    }

    pub fn execute_statutory_clawback(
        ctx: Context<ExecuteClawback>,
        amount: u64,
    ) -> Result<()> {
        instructions::corporate_actions::clawback_handler(ctx, amount)
    }
}