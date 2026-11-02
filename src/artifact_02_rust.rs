use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    mint_to, set_authority, transfer_checked, Mint, MintTo, SetAuthority, Token2022, TokenAccount,
    TransferChecked,
};
use spl_token_2022::instruction::AuthorityType;

declare_id!("TokenSG111111111111111111111111111111111111");

#[program]
pub mod soltokenize_sg {
    use super::*;

    /// Initialize the tokenized market asset (e.g. MAS-regulated SGD Commercial Paper or Private Fund)
    pub fn initialize_market(
        ctx: Context<InitializeMarket>,
        asset_symbol: String,
        isin: String,
        initial_nav_bps: u64, // NAV per unit in basis points (e.g., 10000 = $1.0000 SGD)
        decimals: u8,
    ) -> Result<()> {
        require!(asset_symbol.len() <= 16, ErrorCode::StringTooLong);
        require!(isin.len() == 12, ErrorCode::InvalidISIN); // Standard ISO 6166 ISIN length
        require!(initial_nav_bps > 0, ErrorCode::InvalidNAV);

        let market = &mut ctx.accounts.market;
        market.authority = ctx.accounts.authority.key();
        market.compliance_officer = ctx.accounts.compliance_officer.key();
        market.mint = ctx.accounts.mint.key();
        market.asset_symbol = asset_symbol;
        market.isin = isin;
        market.nav_bps = initial_nav_bps;
        market.last_nav_update = Clock::get()?.unix_timestamp;
        market.is_paused = false;
        market.bump = ctx.bumps.market;

        emit!(MarketInitializedEvent {
            market: market.key(),
            mint: ctx.accounts.mint.key(),
            isin: market.isin.clone(),
            initial_nav_bps,
        });

        Ok(())
    }

    /// Whitelist an Accredited Investor (AI) under MAS SFA regulations
    pub fn set_investor_status(
        ctx: Context<SetInvestorStatus>,
        investor: Pubkey,
        is_accredited: bool,
        jurisdiction: [u8; 2], // ISO 3166-1 alpha-2, e.g., b"SG"
        expiration_timestamp: i64,
    ) -> Result<()> {
        require!(
            ctx.accounts.authority.key() == ctx.accounts.market.compliance_officer
                || ctx.accounts.authority.key() == ctx.accounts.market.authority,
            ErrorCode::UnauthorizedComplianceAction
        );

        let record = &mut ctx.accounts.investor_record;
        record.investor = investor;
        record.market = ctx.accounts.market.key();
        record.is_accredited = is_accredited;
        record.jurisdiction = jurisdiction;
        record.expiration_timestamp = expiration_timestamp;
        record.bump = ctx.bumps.investor_record;

        emit!(InvestorStatusUpdatedEvent {
            market: ctx.accounts.market.key(),
            investor,
            is_accredited,
            jurisdiction,
            expiration_timestamp,
        });

        Ok(())
    }

    /// Update dynamic NAV per unit based on off-chain independent trustee valuation
    pub fn update_nav(ctx: Context<UpdateNAV>, new_nav_bps: u64) -> Result<()> {
        require!(new_nav_bps > 0, ErrorCode::InvalidNAV);
        let market = &mut ctx.accounts.market;
        let old_nav = market.nav_bps;
        market.nav_bps = new_nav_bps;
        market.last_nav_update = Clock::get()?.unix_timestamp;

        emit!(NAVUpdatedEvent {
            market: market.key(),
            old_nav_bps: old_nav,
            new_nav_bps,
            timestamp: market.last_nav_update,
        });

        Ok(())
    }

    /// Mint tokenized shares to a fully whitelisted accredited investor
    pub fn issue_shares(ctx: Context<IssueShares>, amount: u64) -> Result<()> {
        let market = &ctx.accounts.market;
        require!(!market.is_paused, ErrorCode::MarketPaused);

        // Enforce compliance on the destination account
        let recipient_record = &ctx.accounts.recipient_record;
        let current_time = Clock::get()?.unix_timestamp;
        require!(recipient_record.is_accredited, ErrorCode::InvestorNotAccredited);
        require!(
            recipient_record.expiration_timestamp > current_time,
            ErrorCode::AccreditationExpired
        );

        let seeds = &[
            b"market".as_ref(),
            market.mint.as_ref(),
            &[market.bump],
        ];
        let signer = &[&seeds[..]];

        mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.mint.to_account_info(),
                    to: ctx.accounts.recipient_token_account.to_account_info(),
                    authority: market.to_account_info(),
                },
                signer,
            ),
            amount,
        )?;

        emit!(SharesIssuedEvent {
            market: market.key(),
            recipient: recipient_record.investor,
            amount,
            nav_bps: market.nav_bps,
        });

        Ok(())
    }

    /// Enforce Transfer Hook rule: Synchronous KYC/Accreditation check on on-chain transfers
    pub fn execute_transfer_hook(
        ctx: Context<ExecuteTransferHook>,
        amount: u64,
    ) -> Result<()> {
        let market = &ctx.accounts.market;
        require!(!market.is_paused, ErrorCode::MarketPaused);

        let sender_record = &ctx.accounts.sender_record;
        let recipient_record = &ctx.accounts.recipient_record;
        let current_time = Clock::get()?.unix_timestamp;

        // Verify Source Investor Compliance
        require!(sender_record.is_accredited, ErrorCode::InvestorNotAccredited);
        require!(
            sender_record.expiration_timestamp > current_time,
            ErrorCode::AccreditationExpired
        );

        // Verify Recipient Investor Compliance
        require!(
            recipient_record.is_accredited,
            ErrorCode::InvestorNotAccredited
        );
        require!(
            recipient_record.expiration_timestamp > current_time,
            ErrorCode::AccreditationExpired
        );

        emit!(TransferHookValidatedEvent {
            market: market.key(),
            sender: sender_record.investor,
            recipient: recipient_record.investor,
            amount,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT VALIDATION STRUCTS
// -----------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(asset_symbol: String, isin: String)]
pub儲
pub struct InitializeMarket<'info> {
    #[account(
        init,
        payer = authority,
        space = Market::LEN,
        seeds = [b"market", mint.key().as_ref()],
        bump
    )]
    pub market: Account<'info, Market>,

    #[account(mut)]
    pub mint: InterfaceAccount<'info, Mint>,

    /// CHECK: Authority for administrative controls
    #[account(mut)]
    pub authority: Signer<'info>,

    /// CHECK: Registered compliance officer key
    pub compliance_officer: AccountInfo<'info>,

    pub token_program: Interface<'info, Token2022>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct SetInvestorStatus<'info> {
    #[account(
        mut,
        has_one = authority,
    )]
    pub market: Account<'info, Market>,

    #[account(
        init_if_needed,
        payer = authority,
        space = InvestorRecord::LEN,
        seeds = [b"investor", market.key().as_ref(), investor.key().as_ref()],
        bump
    )]
    pub investor_record: Account<'info, InvestorRecord>,

    /// CHECK: Investor wallet address
    pub investor: AccountInfo<'info>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateNAV<'info> {
    #[account(
        mut,
        has_one = authority,
    )]
    pub market: Account<'info, Market>,
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct IssueShares<'info> {
    #[account(
        mut,
        has_one = authority,
        has_one = mint,
    )]
    pub market: Account<'info, Market>,

    #[account(mut)]
    pub mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        seeds = [b"investor", market.key().as_ref(), recipient_record.investor.key().as_ref()],
        bump = recipient_record.bump
    )]
    pub recipient_record: Account<'info, InvestorRecord>,

    #[account(mut)]
    pub recipient_token_account: InterfaceAccount<'info, TokenAccount>,

    pub authority: Signer<'info>,
    pub token_program: Interface<'info, Token2022>,
}

#[derive(Accounts)]
pub struct ExecuteTransferHook<'info> {
    #[account(has_one = mint)]
    pub market: Account<'info, Market>,

    pub mint: InterfaceAccount<'info, Mint>,

    #[account(
        seeds = [b"investor", market.key().as_ref(), sender_record.investor.key().as_ref()],
        bump = sender_record.bump
    )]
    pub sender_record: Account<'info, InvestorRecord>,

    #[account(
        seeds = [b"investor", market.key().as_ref(), recipient_record.investor.key().as_ref()],
        bump = recipient_record.bump
    )]
    pub recipient_record: Account<'info, InvestorRecord>,
}

// -----------------------------------------------------------------------------
// STATE STRUCTS
// -----------------------------------------------------------------------------

#[account]
pub struct Market {
    pub authority: Pubkey,          // 32
    pub compliance_officer: Pubkey, // 32
    pub mint: Pubkey,                // 32
    pub asset_symbol: String,        // 4 + 16 = 20
    pub isin: String,                // 4 + 12 = 16
    pub nav_bps: u64,                // 8
    pub last_nav_update: i64,        // 8
    pub is_paused: bool,             // 1
    pub bump: u8,                    // 1
}

impl Market {
    pub const LEN: usize = 8 + 32 + 32 + 32 + 20 + 16 + 8 + 8 + 1 + 1 + 64;
}

#[account]
pub struct InvestorRecord {
    pub investor: Pubkey,           // 32
    pub market: Pubkey,             // 32
    pub is_accredited: bool,        // 1
    pub jurisdiction: [u8; 2],      // 2
    pub expiration_timestamp: i64,  // 8
    pub bump: u8,                   // 1
}

impl InvestorRecord {
    pub const LEN: usize = 8 + 32 + 32 + 1 + 2 + 8 + 1 + 32;
}

// -----------------------------------------------------------------------------
// EVENTS & ERRORS
// -----------------------------------------------------------------------------

#[event]
pub struct MarketInitializedEvent {
    pub market: Pubkey,
    pub mint: Pubkey,
    pub isin: String,
    pub initial_nav_bps: u64,
}

#[event]
pub struct InvestorStatusUpdatedEvent {
    pub market: Pubkey,
    pub investor: Pubkey,
    pub is_accredited: bool,
    pub jurisdiction: [u8; 2],
    pub expiration_timestamp: i64,
}

#[event]
pub struct NAVUpdatedEvent {
    pub market: Pubkey,
    pub old_nav_bps: u64,
    pub new_nav_bps: u64,
    pub timestamp: i64,
}

#[event]
pub struct SharesIssuedEvent {
    pub market: Pubkey,
    pub recipient: Pubkey,
    pub amount: u64,
    pub nav_bps: u64,
}

#[event]
pub struct TransferHookValidatedEvent {
    pub market: Pubkey,
    pub sender: Pubkey,
    pub recipient: Pubkey,
    pub amount: u64,
}

#[error_code]
pub enum ErrorCode {
    #[msg("String exceeds maximum allowed capacity")]
    StringTooLong,
    #[msg("Provided ISIN is invalid; must be exactly 12 characters conforming to ISO 6166")]
    InvalidISIN,
    #[msg("NAV basis points must be strictly positive")]
    InvalidNAV,
    #[msg("Unauthorized compliance operation")]
    UnauthorizedComplianceAction,
    #[msg("Investor fails MAS accreditation requirements")]
    InvestorNotAccredited,
    #[msg("Investor MAS accreditation validity period has expired")]
    AccreditationExpired,
    #[msg("Trading in this tokenized market has been administratively paused")]
    MarketPaused,
}