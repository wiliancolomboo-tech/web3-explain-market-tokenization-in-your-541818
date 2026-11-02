use anchor_lang::prelude::*;
use anchor_spl::token_interface::TokenAccount;
use crate::error::SolRwaError;
use crate::state::{AssetConfig, InvestorRegistry, COUNTRY_CODE_SANCTIONED_PLACEHOLDER};

#[derive(Accounts)]
pub struct ExecuteTransferHook<'info> {
    #[account(
        token::mint = asset_config.mint,
    )]
    pub source_account: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: The mint of the Token-2022 asset
    pub mint: UncheckedAccount<'info>,

    #[account(
        token::mint = asset_config.mint,
    )]
    pub destination_account: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: Source owner PDA/Signer
    pub source_owner: UncheckedAccount<'info>,

    /// CHECK: ExtraAccountMetaList storage account
    pub extra_account_metas: UncheckedAccount<'info>,

    #[account(
        seeds = [b"asset_config", mint.key().as_ref()],
        bump = asset_config.bump,
    )]
    pub asset_config: Account<'info, AssetConfig>,

    #[account(
        seeds = [b"investor", mint.key().as_ref(), source_account.owner.as_ref()],
        bump = sender_registry.bump,
    )]
    pub sender_registry: Account<'info, InvestorRegistry>,

    #[account(
        seeds = [b"investor", mint.key().as_ref(), destination_account.owner.as_ref()],
        bump = recipient_registry.bump,
    )]
    pub recipient_registry: Account<'info, InvestorRegistry>,
}

pub fn handler(ctx: Context<ExecuteTransferHook>, amount: u64) -> Result<()> {
    let clock = Clock::get()?;
    let current_timestamp = clock.unix_timestamp;
    let asset_config = &ctx.accounts.asset_config;
    let sender = &ctx.accounts.sender_registry;
    let recipient = &ctx.accounts.recipient_registry;

    msg!("Evaluating SolRWA Transfer Hook for Amount: {} units", amount);

    // 1. Enforce Source Account Status & KYC/AML Validity
    require!(sender.is_active, SolRwaError::InvestorSuspended);
    require!(sender.valid_until > current_timestamp, SolRwaError::AttestationExpired);

    // 2. Enforce Recipient Account Status & KYC/AML Validity
    require!(recipient.is_active, SolRwaError::InvestorSuspended);
    require!(recipient.valid_until > current_timestamp, SolRwaError::AttestationExpired);

    // 3. Enforce Sanctions / Jurisdiction Filters
    require!(
        sender.country_code != COUNTRY_CODE_SANCTIONED_PLACEHOLDER &&
        recipient.country_code != COUNTRY_CODE_SANCTIONED_PLACEHOLDER,
        SolRwaError::RestrictedJurisdiction
    );

    // 4. Enforce Regulation D Rule 144 Safe Harbor Lockup Check
    // If the recipient or sender is subject to Reg D, verify the acquisition lockup period
    let lockup_expiry = sender
        .acquisition_timestamp
        .checked_add(asset_config.reg_d_lockup_seconds)
        .ok_or(SolRwaError::NumericalOverflow)?;

    require!(
        current_timestamp >= lockup_expiry,
        SolRwaError::Rule144LockupActive
    );

    // 5. Enforce Recipient Accreditation Check for US Jurisdictions
    if recipient.country_code == 840 {
        require!(recipient.is_accredited, SolRwaError::NonAccreditedInvestor);
    }

    msg!("Compliance checks passed. Transfer authorized deterministically.");
    Ok(())
}