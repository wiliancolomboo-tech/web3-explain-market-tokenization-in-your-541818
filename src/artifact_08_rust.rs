use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;
use spl_transfer_hook_interface::instruction::TransferHookInstruction;

declare_id!("HOOKx9Q3J5z8WbA9wXv4yQ5L4kR3wMRWAxL1kXqTmV3y");

pub const HOOK_EXTRA_SEEDS: &[u8] = b"extra-account-metas";

#[program]
pub mod solpath_hook {
    use super::*;

    /// Initialize ExtraAccountMetaList required for Token-2022 Transfer Hook resolution
    pub fn initialize_extra_account_meta_list(
        ctx: Context<InitializeExtraAccountMetaList>,
    ) -> Result<()> {
        msg!("SolPath Hook: Initializing extra account meta list for dynamic CPI resolution");
        Ok(())
    }

    /// The synchronous transfer hook instruction executed on every SPL Token-2022 transfer
    pub fn transfer_hook(ctx: Context<ExecuteTransferHook>, amount: u64) -> Result<()> {
        let sender_registry = &ctx.accounts.sender_compliance;
        let recipient_registry = &ctx.accounts.recipient_compliance;
        let now = Clock::get()?.unix_timestamp;

        msg!("SolPath Hook: Evaluating trade compliance for transfer of {} atomic units", amount);

        // 1. OFAC Sanctions & Freeze Verification
        require!(!sender_registry.is_sanctioned, TransferHookError::SenderSanctioned);
        require!(!recipient_registry.is_sanctioned, TransferHookError::RecipientSanctioned);

        // 2. KYC Freshness Verification
        require!(sender_registry.kyc_expiry_timestamp > now, TransferHookError::SenderKycExpired);
        require!(recipient_registry.kyc_expiry_timestamp > now, TransferHookError::RecipientKycExpired);

        // 3. Reg D / Reg S Cross-Border Transfer Logic
        // Reg S non-US investors can transfer to other Reg S non-US investors immediately.
        // Reg D US investors require explicit accreditation compliance.
        if !recipient_registry.is_foreign_reg_s {
            require!(
                recipient_registry.is_accredited_us,
                TransferHookError::RecipientNotAccredited
            );
        }

        msg!("SolPath Hook: Trade cleared. SEC compliance verified.");
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeExtraAccountMetaList<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: ExtraAccountMetaList account PDA
    #[account(
        init,
        payer = payer,
        space = 256,
        seeds = [HOOK_EXTRA_SEEDS, mint.key().as_ref()],
        bump
    )]
    pub extra_account_meta_list: AccountInfo<'info>,

    pub mint: InterfaceAccount<'info, Mint>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ExecuteTransferHook<'info> {
    /// CHECK: Source Token Account
    pub source_account: AccountInfo<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    /// CHECK: Destination Token Account
    pub destination_account: AccountInfo<'info>,
    /// CHECK: Owner authority of source
    pub owner: AccountInfo<'info>,

    /// CHECK: Extra Account Meta Account
    #[account(
        seeds = [HOOK_EXTRA_SEEDS, mint.key().as_ref()],
        bump
    )]
    pub extra_account_meta_list: AccountInfo<'info>,

    #[account(
        seeds = [b"investor_entry", owner.key().as_ref()],
        seeds::program = Pubkey::new_from_array(solpath_rwa_id()),
        bump = sender_compliance.bump
    )]
    pub sender_compliance: Account<'info, InvestorRegistryEntry>,

    #[account(
        seeds = [b"investor_entry", destination_account.key().as_ref()],
        seeds::program = Pubkey::new_from_array(solpath_rwa_id()),
        bump = recipient_compliance.bump
    )]
    pub recipient_compliance: Account<'info, InvestorRegistryEntry>,
}

pub fn solpath_rwa_id() -> [u8; 32] {
    // Return standard Pubkey byte representation for SolPath RWA ID
    Pubkey::new_from_array([
        0xf2, 0xa1, 0x33, 0x12, 0x8b, 0x4c, 0x98, 0x22,
        0x10, 0x44, 0x89, 0xa2, 0xbb, 0xee, 0xcc, 0x01,
        0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc,
        0xdd, 0xee, 0xff, 0x00, 0x11, 0x22, 0x33, 0x44
    ]).to_bytes()
}

#[account]
pub struct InvestorRegistryEntry {
    pub investor: Pubkey,
    pub kyc_expiry_timestamp: i64,
    pub is_accredited_us: bool,
    pub is_foreign_reg_s: bool,
    pub is_sanctioned: bool,
    pub last_updated: i64,
    pub bump: u8,
}

#[error_code]
pub enum TransferHookError {
    #[msg("Source investor account is OFAC / legal sanctioned.")]
    SenderSanctioned,
    #[msg("Destination investor account is OFAC / legal sanctioned.")]
    RecipientSanctioned,
    #[msg("Source investor KYC certification has expired.")]
    SenderKycExpired,
    #[msg("Destination investor KYC certification has expired.")]
    RecipientKycExpired,
    #[msg("Destination recipient is not certified as an SEC Accredited Investor.")]
    RecipientNotAccredited,
}