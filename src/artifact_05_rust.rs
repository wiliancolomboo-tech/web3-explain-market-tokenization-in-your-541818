use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, Token2022, TokenAccount};

declare_id!("MerL1onRWA111111111111111111111111111111111");

pub const AIR_SEED: &[u8] = b"accredited_investor";
pub const ASSET_SEED: &[u8] = b"asset_tranche";
pub const ESCROW_SEED: &[u8] = b"yield_escrow";

#[program]
pub mod merlion_rwa {
    use super::*;

    /// Initializes a Singapore Sovereign RWA Debt Tranche (Bond/Commercial Paper)
    pub fn initialize_asset(
        ctx: Context<InitializeAsset>,
        isin: String,
        jurisdiction: [u8; 2], // e.g., *b"SG"
        par_value: u64,        // In base units of payment currency (cents)
        coupon_bps: u16,       // Basis points, e.g., 550 = 5.5%
        maturity_timestamp: i64,
    ) -> Result<()> {
        require!(isin.len() == 12, MerlionError::InvalidISINLength);
        require!(
            maturity_timestamp > Clock::get()?.unix_timestamp,
            MerlionError::InvalidMaturity
        );

        let asset = &mut ctx.accounts.asset;
        asset.issuer = ctx.accounts.authority.key();
        asset.mint = ctx.accounts.mint.key();
        asset.isin = isin;
        asset.jurisdiction = jurisdiction;
        asset.par_value = par_value;
        asset.coupon_bps = coupon_bps;
        asset.maturity_timestamp = maturity_timestamp;
        asset.total_minted = 0;
        asset.is_paused = false;
        asset.bump = ctx.bumps.asset;

        msg!("Merlion RWA Asset Initialized: ISIN {}", asset.isin);
        Ok(())
    }

    /// Registers or updates an Investor's Accreditation Status under MAS SFA Sec 4A
    pub fn register_investor(
        ctx: Context<RegisterInvestor>,
        investor_pubkey: Pubkey,
        kyc_expiry: i64,
        is_accredited: bool,
        jurisdiction: [u8; 2],
        max_allocation: u64,
    ) -> Result<()> {
        let clock = Clock::get()?;
        require!(kyc_expiry > clock.unix_timestamp, MerlionError::ExpiredKYC);

        let investor_record = &mut ctx.accounts.investor_record;
        investor_record.investor = investor_pubkey;
        investor_record.kyc_expiry = kyc_expiry;
        investor_record.is_accredited = is_accredited;
        investor_record.is_sanctioned = false;
        investor_record.jurisdiction = jurisdiction;
        investor_record.max_allocation = max_allocation;
        investor_record.updated_at = clock.unix_timestamp;
        investor_record.bump = ctx.bumps.investor_record;

        msg!(
            "Registered Investor: {} | Accredited: {} | Jurisdiction: {:?}",
            investor_pubkey,
            is_accredited,
            jurisdiction
        );
        Ok(())
    }

    /// Toggles sanctions or operational freeze for regulatory compliance (MAS Notice 626)
    pub fn set_sanctions_status(
        ctx: Context<SetSanctionsStatus>,
        is_sanctioned: bool,
    ) -> Result<()> {
        let record = &mut ctx.accounts.investor_record;
        record.is_sanctioned = is_sanctioned;
        record.updated_at = Clock::get()?.unix_timestamp;
        msg!("Updated Investor {} Sanctions Flag: {}", record.investor, is_sanctioned);
        Ok(())
    }

    /// Transfer Hook execution interface compliant with SPL Token-2022
    pub fn execute_transfer_hook(
        ctx: Context<ExecuteTransferHook>,
        amount: u64,
    ) -> Result<()> {
        let asset = &ctx.accounts.asset;
        require!(!asset.is_paused, MerlionError::AssetTradingPaused);

        let clock = Clock::get()?;

        // Sender Validation
        let source_record = &ctx.accounts.source_investor_record;
        require!(!source_record.is_sanctioned, MerlionError::SourceSanctioned);
        require!(source_record.kyc_expiry > clock.unix_timestamp, MerlionError::SourceKYCExpired);

        // Destination Validation (Securities and Futures Act SFA Section 4A Compliance)
        let dest_record = &ctx.accounts.destination_investor_record;
        require!(!dest_record.is_sanctioned, MerlionError::DestinationSanctioned);
        require!(dest_record.kyc_expiry > clock.unix_timestamp, MerlionError::DestinationKYCExpired);
        require!(dest_record.is_accredited, MerlionError::DestinationNotAccredited);

        // Singaporean Jurisdiction cross-check
        require!(
            dest_record.jurisdiction == *b"SG" || dest_record.jurisdiction == asset.jurisdiction,
            MerlionError::JurisdictionMismatch
        );

        // Cap Table concentration limitation check
        let destination_post_balance = ctx.accounts.destination_token.amount.checked_add(amount)
            .ok_or(MerlionError::MathOverflow)?;
        require!(
            destination_post_balance <= dest_record.max_allocation,
            MerlionError::AllocationExceeded
        );

        msg!(
            "Transfer Hook Verified: {} tokens cleared from {} to {}",
            amount,
            ctx.accounts.source_token.key(),
            ctx.accounts.destination_token.key()
        );
        Ok(())
    }
}

// ----------------------------------------------------------------------------
// Account Structs & Contexts
// ----------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(isin: String)]
pub struct InitializeAsset<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = 8 + AssetTranche::INIT_SPACE,
        seeds = [ASSET_SEED, mint.key().as_ref()],
        bump
    )]
    pub asset: Account<'info, AssetTranche>,

    pub mint: InterfaceAccount<'info, Mint>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(investor_pubkey: Pubkey)]
pub struct RegisterInvestor<'info> {
    #[account(mut)]
    pub compliance_officer: Signer<'info>,

    #[account(
        init_or_reset,
        payer = compliance_officer,
        space = 8 + InvestorRecord::INIT_SPACE,
        seeds = [AIR_SEED, investor_pubkey.as_ref()],
        bump
    )]
    pub investor_record: Account<'info, InvestorRecord>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SetSanctionsStatus<'info> {
    #[account(mut)]
    pub compliance_officer: Signer<'info>,

    #[account(
        mut,
        seeds = [AIR_SEED, investor_record.investor.as_ref()],
        bump = investor_record.bump
    )]
    pub investor_record: Account<'info, InvestorRecord>,
}

#[derive(Accounts)]
pub struct ExecuteTransferHook<'info> {
    #[account(
        seeds = [ASSET_SEED, source_token.mint.as_ref()],
        bump = asset.bump
    )]
    pub asset: Account<'info, AssetTranche>,

    pub source_token: InterfaceAccount<'info, TokenAccount>,
    pub destination_token: InterfaceAccount<'info, TokenAccount>,

    #[account(
        seeds = [AIR_SEED, source_token.owner.as_ref()],
        bump = source_investor_record.bump
    )]
    pub source_investor_record: Account<'info, InvestorRecord>,

    #[account(
        seeds = [AIR_SEED, destination_token.owner.as_ref()],
        bump = destination_investor_record.bump
    )]
    pub destination_investor_record: Account<'info, InvestorRecord>,
}

// ----------------------------------------------------------------------------
// State Schema
// ----------------------------------------------------------------------------

#[account]
#[derive(InitSpace)]
pub struct AssetTranche {
    pub issuer: Pubkey,
    pub mint: Pubkey,
    #[max_len(12)]
    pub isin: String,             // 12-char ISO 6166 ISIN
    pub jurisdiction: [u8; 2],    // "SG"
    pub par_value: u64,
    pub coupon_bps: u16,
    pub maturity_timestamp: i64,
    pub total_minted: u64,
    pub is_paused: bool,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct InvestorRecord {
    pub investor: Pubkey,
    pub kyc_expiry: i64,
    pub is_accredited: bool,
    pub is_sanctioned: bool,
    pub jurisdiction: [u8; 2],
    pub max_allocation: u64,
    pub updated_at: i64,
    pub bump: u8,
}

// ----------------------------------------------------------------------------
// Error Codes
// ----------------------------------------------------------------------------

#[error_code]
pub enum MerlionError {
    #[msg("ISIN must be exactly 12 alphanumeric characters (ISO 6166).")]
    InvalidISINLength,
    #[msg("Maturity timestamp must be in the future.")]
    InvalidMaturity,
    #[msg("Supplied KYC expiration date is already in the past.")]
    ExpiredKYC,
    #[msg("Asset trading is currently paused by compliance authority.")]
    AssetTradingPaused,
    #[msg("Source wallet flagged in Anti-Money Laundering / Sanctions list.")]
    SourceSanctioned,
    #[msg("Source wallet KYC accreditation has expired.")]
    SourceKYCExpired,
    #[msg("Destination wallet flagged in Anti-Money Laundering / Sanctions list.")]
    DestinationSanctioned,
    #[msg("Destination wallet KYC accreditation has expired.")]
    DestinationKYCExpired,
    #[msg("Destination investor is not certified as an SFA Accredited Investor.")]
    DestinationNotAccredited,
    #[msg("Destination jurisdiction violates instrument legal offering parameters.")]
    JurisdictionMismatch,
    #[msg("Transfer exceeds destination investor's statutory cap allocation.")]
    AllocationExceeded,
    #[msg("Arithmetic calculation overflow.")]
    MathOverflow,
}