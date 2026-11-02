use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount};
use spl_transfer_hook_interface::instruction::TransferHookInstruction;

declare_id!("RWAHook1111111111111111111111111111111111111");

#[program]
pub mod us_rwa_transfer_hook {
    use super::*;

    /// Initializes the compliance registry configuration for a specific RWA token mint.
    pub fn initialize_asset_config(
        ctx: Context<InitializeAssetConfig>,
        lockup_duration_seconds: i64, // e.g., 31,536,000s for Rule 144 1-year holding period
        jurisdiction_code: u16,       // 840 for United States (ISO 3166-1 numeric)
    ) -> Result<()> {
        let asset_config = &mut ctx.accounts.asset_config;
        asset_config.authority = ctx.accounts.authority.key();
        asset_config.mint = ctx.accounts.mint.key();
        asset_config.lockup_duration_seconds = lockup_duration_seconds;
        asset_config.jurisdiction_code = jurisdiction_code;
        asset_config.is_paused = false;
        asset_config.bump = ctx.bumps.asset_config;

        msg!(
            "RWA Asset Config initialized for Mint: {} with Lockup: {}s",
            asset_config.mint,
            lockup_duration_seconds
        );
        Ok(())
    }

    /// Whitelists or updates an investor's SEC compliance status (KYC/AML, Reg D/S status).
    pub fn upsert_investor_registry(
        ctx: Context<UpsertInvestorRegistry>,
        investor_wallet: Pubkey,
        accreditation_expiry: i64,
        is_sanctioned: bool,
        jurisdiction_code: u16,
    ) -> Result<()> {
        let registry = &mut ctx.accounts.investor_registry;
        registry.investor = investor_wallet;
        registry.accreditation_expiry = accreditation_expiry;
        registry.is_sanctioned = is_sanctioned;
        registry.jurisdiction_code = jurisdiction_code;
        registry.updated_at = Clock::get()?.unix_timestamp;
        registry.bump = ctx.bumps.investor_registry;

        msg!(
            "Investor Registry Updated: {} | Sanctioned: {} | ValidUntil: {}",
            investor_wallet,
            is_sanctioned,
            accreditation_expiry
        );
        Ok(())
    }

    /// Sets individual token holding lockups for Reg D Rule 144 compliance.
    pub fn register_holding_lockup(
        ctx: Context<RegisterHoldingLockup>,
        token_account: Pubkey,
        custom_unlock_timestamp: Option<i64>,
    ) -> Result<()> {
        let lockup_account = &mut ctx.accounts.holding_lockup;
        let clock = Clock::get()?;
        let unlock_timestamp = match custom_unlock_timestamp {
            Some(ts) => ts,
            None => clock.unix_timestamp + ctx.accounts.asset_config.lockup_duration_seconds,
        };

        lockup_account.token_account = token_account;
        lockup_account.locked_until = unlock_timestamp;
        lockup_account.bump = ctx.bumps.holding_lockup;

        msg!(
            "Token Account: {} locked under Rule 144 until: {}",
            token_account,
            unlock_timestamp
        );
        Ok(())
    }

    /// SPL Transfer Hook fallback entrypoint executing on every atomic transfer
    pub fn execute_transfer_hook(
        ctx: Context<ExecuteTransferHook>,
        amount: u64,
    ) -> Result<()> {
        let asset_config = &ctx.accounts.asset_config;
        require!(!asset_config.is_paused, ComplianceError::TradingHalted);

        let clock = Clock::get()?;
        let current_timestamp = clock.unix_timestamp;

        // 1. Verify Source Token Account Lockup (Rule 144 check)
        if let Some(source_lockup) = &ctx.accounts.source_lockup {
            require!(
                current_timestamp >= source_lockup.locked_until,
                ComplianceError::Rule144LockupActive
            );
        }

        // 2. Verify Destination Investor Accreditation & Sanctions Check
        let destination_registry = &ctx.accounts.destination_registry;
        require!(
            !destination_registry.is_sanctioned,
            ComplianceError::SanctionedEntityForbidden
        );
        require!(
            current_timestamp <= destination_registry.accreditation_expiry,
            ComplianceError::AccreditationExpired
        );

        // 3. Prevent zero or negative amounts
        require!(amount > 0, ComplianceError::InvalidTransferAmount);

        msg!(
            "Transfer Hook Cleared: Transferred {} units to compliant wallet {}",
            amount,
            ctx.accounts.destination_registry.investor
        );
        Ok(())
    }

    /// Emergency Circuit Breaker (SEC Compliance Freeze / Asset Custody)
    pub fn set_circuit_breaker(ctx: Context<SetCircuitBreaker>, is_paused: bool) -> Result<()> {
        ctx.accounts.asset_config.is_paused = is_paused;
        msg!("Asset Config Circuit Breaker status set to: {}", is_paused);
        Ok(())
    }
}

// -------------------------------------------------------------------------
// Account Contexts
// -------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeAssetConfig<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + AssetConfig::INIT_SPACE,
        seeds = [b"asset_config", mint.key().as_ref()],
        bump
    )]
    pub asset_config: Account<'info, AssetConfig>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(investor_wallet: Pubkey)]
pub struct UpsertInvestorRegistry<'info> {
    #[account(
        init_or_update,
        payer = authority,
        space = 8 + InvestorRegistry::INIT_SPACE,
        seeds = [b"investor_registry", asset_config.key().as_ref(), investor_wallet.as_ref()],
        bump
    )]
    pub investor_registry: Account<'info, InvestorRegistry>,
    #[account(
        has_one = authority @ ComplianceError::UnauthorizedComplianceOfficer
    )]
    pub asset_config: Account<'info, AssetConfig>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(token_account: Pubkey)]
pub struct RegisterHoldingLockup<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + HoldingLockup::INIT_SPACE,
        seeds = [b"holding_lockup", token_account.as_ref()],
        bump
    )]
    pub holding_lockup: Account<'info, HoldingLockup>,
    #[account(
        has_one = authority @ ComplianceError::UnauthorizedComplianceOfficer
    )]
    pub asset_config: Account<'info, AssetConfig>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ExecuteTransferHook<'info> {
    #[account(
        seeds = [b"asset_config", source_token.mint.as_ref()],
        bump = asset_config.bump
    )]
    pub asset_config: Account<'info, AssetConfig>,
    pub source_token: InterfaceAccount<'info, TokenAccount>,
    pub destination_token: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: Transfer Hook authority
    pub authority: UncheckedAccount<'info>,
    #[account(
        seeds = [b"holding_lockup", source_token.key().as_ref()],
        bump = source_lockup.bump
    )]
    pub source_lockup: Option<Account<'info, HoldingLockup>>,
    #[account(
        seeds = [b"investor_registry", asset_config.key().as_ref(), destination_token.owner.as_ref()],
        bump = destination_registry.bump
    )]
    pub destination_registry: Account<'info, InvestorRegistry>,
}

#[derive(Accounts)]
pub struct SetCircuitBreaker<'info> {
    #[account(
        mut,
        has_one = authority @ ComplianceError::UnauthorizedComplianceOfficer
    )]
    pub asset_config: Account<'info, AssetConfig>,
    pub authority: Signer<'info>,
}

// -------------------------------------------------------------------------
// Storage State Structs
// -------------------------------------------------------------------------

#[account]
#[derive(InitSpace)]
pub struct AssetConfig {
    pub authority: Pubkey,
    pub mint: Pubkey,
    pub lockup_duration_seconds: i64,
    pub jurisdiction_code: u16,
    pub is_paused: bool,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct InvestorRegistry {
    pub investor: Pubkey,
    pub accreditation_expiry: i64,
    pub is_sanctioned: bool,
    pub jurisdiction_code: u16,
    pub updated_at: i64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct HoldingLockup {
    pub token_account: Pubkey,
    pub locked_until: i64,
    pub bump: u8,
}

// -------------------------------------------------------------------------
// Error Codes
// -------------------------------------------------------------------------

#[error_code]
pub enum ComplianceError {
    #[msg("Trading on this security has been halted by compliance or legal authority.")]
    TradingHalted,
    #[msg("SEC Rule 144 1-year holding lockup period has not elapsed for this asset lot.")]
    Rule144LockupActive,
    #[msg("Transfer rejected: Target wallet address is flagged on OFAC/Sanctions list.")]
    SanctionedEntityForbidden,
    #[msg("Transfer rejected: Investor accreditation has expired under SEC Reg D guidelines.")]
    AccreditationExpired,
    #[msg("Action unauthorized: Caller is not the approved compliance officer.")]
    UnauthorizedComplianceOfficer,
    #[msg("Invalid transfer amount specified.")]
    InvalidTransferAmount,
}