use anchor_lang::prelude::*;

pub mod error;
pub mod instructions;
pub mod state;

use instructions::initialize::*;
use instructions::manage_investor::*;
use instructions::transfer_hook::*;
use instructions::regulatory_clawback::*;

declare_id!("Hg2yF532xQZp8Qv2M2aX8f9w1R1x5T8zK3yvJ6p7XmE4");

#[program]
pub mod aegis_usa_core {
    use super::*;

    pub fn initialize(
        ctx: Context<InitializeProtocol>,
        compliance_authority: Pubkey,
        emergency_clawback_admin: Pubkey,
    ) -> Result<()> {
        handle_initialize(ctx, compliance_authority, emergency_clawback_admin)
    }

    pub fn set_investor_status(
        ctx: Context<ManageInvestor>,
        investor_wallet: Pubkey,
        jurisdiction: u8,
        kyc_expiry_ts: i64,
        is_sanctioned: bool,
        is_frozen: bool,
    ) -> Result<()> {
        handle_set_investor_status(
            ctx,
            investor_wallet,
            jurisdiction,
            kyc_expiry_ts,
            is_sanctioned,
            is_frozen,
        )
    }

    pub fn execute_transfer_hook(
        ctx: Context<ExecuteTransferHook>,
        amount: u64,
    ) -> Result<()> {
        handle_execute_transfer_hook(ctx, amount)
    }

    pub fn execute_clawback(
        ctx: Context<RegulatoryClawback>,
        amount: u64,
        decimals: u8,
        legal_docket_reference: String,
    ) -> Result<()> {
        handle_regulatory_clawback(ctx, amount, decimals, legal_docket_reference)
    }
}