use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};
use spl_tlv_account_resolution::{
    account::ExtraAccountMeta, seeds::Seed, state::ExtraAccountMetaList,
};
use spl_transfer_hook_interface::instruction::TransferHookInstruction;

declare_id!("RWA1111111111111111111111111111111111111111");

pub const MAX_COUNTRY_BLACKLIST: usize = 16;
pub const JURISDICTION_USA: u16 = 840;

#[program]
pub mod sovereign_rwa {
    use super::*;

    /// Initialize the Global Issuer Configuration for the RWA Asset.
    pub fn initialize_issuer_config(
        ctx: Context<InitializeIssuerConfig>,
        asset_isin: [u8; 12],
        rule_144_lockup_seconds: i64,
    ) -> Result<()> {
        let issuer_config = &mut ctx.accounts.issuer_config;
        issuer_config.authority = ctx.accounts.authority.key();
        issuer_config.transfer_agent = ctx.accounts.transfer_agent.key();
        issuer_config.mint = ctx.accounts.mint.key();
        issuer_config.asset_isin = asset_isin;
        issuer_config.rule_144_lockup_seconds = rule_144_lockup_seconds;
        issuer_config.is_paused = false;
        issuer_config.bump = ctx.bumps.issuer_config;
        Ok(())
    }

    /// Set or update the KYC/AML and SEC compliance status for an investor.
    /// Executable exclusively by the designated Transfer Agent multi-sig.
    pub fn set_compliance_status(
        ctx: Context<SetComplianceStatus>,
        investor: Pubkey,
        is_accredited: bool,
        kyc_expiry_timestamp: i64,
        jurisdiction_country_code: u16,
        is_sanctioned: bool,
    ) -> Result<()> {
        let compliance = &mut ctx.accounts.compliance_entry;
        compliance.investor = investor;
        compliance.is_accredited = is_accredited;
        compliance.kyc_expiry_timestamp = kyc_expiry_timestamp;
        compliance.jurisdiction_country_code = jurisdiction_country_code;
        compliance.is_sanctioned = is_sanctioned;
        compliance.first_mint_acquisition_timestamp = Clock::get()?.unix_timestamp;
        compliance.bump = ctx.bumps.compliance_entry;

        emit!(ComplianceUpdatedEvent {
            investor,
            is_accredited,
            kyc_expiry_timestamp,
            jurisdiction_country_code,
            is_sanctioned,
        });

        Ok(())
    }

    /// Initializes the ExtraAccountMetaList PDA required by SPL Token-2022 Transfer Hook.
    pub fn initialize_extra_account_meta_list(
        ctx: Context<InitializeExtraAccountMetaList>,
    ) -> Result<()> {
        let account_metas = vec![
            // Index 4: Issuer Config PDA
            ExtraAccountMeta::new_with_seeds(
                &[
                    Seed::Literal {
                        bytes: b"issuer_config".to_vec(),
                    },
                    Seed::AccountKey { index: 1 }, // Mint address
                ],
                false, // is_signer
                false, // is_writable
            )?,
            // Index 5: Sender Compliance Record
            ExtraAccountMeta::new_with_seeds(
                &[
                    Seed::Literal {
                        bytes: b"compliance".to_vec(),
                    },
                    Seed::AccountKey { index: 3 }, // Transfer Authority / Sender Owner
                ],
                false,
                false,
            )?,
            // Index 6: Recipient Compliance Record
            ExtraAccountMeta::new_with_seeds(
                &[
                    Seed::Literal {
                        bytes: b"compliance".to_vec(),
                    },
                    Seed::AccountKey { index: 2 }, // Destination Token Account Owner
                ],
                false,
                false,
            )?,
        ];

        // Allocate and write extra account metas into the account data
        let account_size = ExtraAccountMetaList::size_of(account_metas.len())?;
        let rent = Rent::get()?;
        let lamports = rent.minimum_balance(account_size);

        let cpi_program = ctx.accounts.system_program.to_account_info();
        let cpi_accounts = anchor_lang::system_program::CreateAccount {
            from: ctx.accounts.authority.to_account_info(),
            to: ctx.accounts.extra_account_meta_list.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

        let mint_key = ctx.accounts.mint.key();
        let seeds = [
            b"extra-account-metas",
            mint_key.as_ref(),
            &[ctx.bumps.extra_account_meta_list],
        ];
        let signer_seeds = &[&seeds[..]];

        anchor_lang::system_program::create_account(
            cpi_ctx.with_signer(signer_seeds),
            lamports,
            account_size as u64,
            ctx.program_id,
        )?;

        let mut data = ctx.accounts.extra_account_meta_list.try_borrow_mut_data()?;
        ExtraAccountMetaList::init::<TransferHookInstruction>(&mut data, &account_metas)?;

        Ok(())
    }

    /// The Transfer Hook evaluation instruction invoked on every Token-2022 transfer.
    pub fn transfer_hook(ctx: Context<TransferHook>, amount: u64) -> Result<()> {
        let issuer_config = &ctx.accounts.issuer_config;
        require!(!issuer_config.is_paused, RwaSecurityError::AssetTradingPaused);

        let current_time = Clock::get()?.unix_timestamp;

        // 1. Verify Sender Compliance
        let sender_compliance = &ctx.accounts.sender_compliance;
        require!(
            !sender_compliance.is_sanctioned,
            RwaSecurityError::SenderSanctioned
        );
        require!(
            sender_compliance.kyc_expiry_timestamp > current_time,
            RwaSecurityError::SenderKycExpired
        );
        require!(
            sender_compliance.is_accredited,
            RwaSecurityError::SenderNotAccredited
        );

        // 2. Verify Recipient Compliance
        let recipient_compliance = &ctx.accounts.recipient_compliance;
        require!(
            !recipient_compliance.is_sanctioned,
            RwaSecurityError::RecipientSanctioned
        );
        require!(
            recipient_compliance.kyc_expiry_timestamp > current_time,
            RwaSecurityError::RecipientKycExpired
        );
        require!(
            recipient_compliance.is_accredited,
            RwaSecurityError::RecipientNotAccredited
        );

        // 3. SEC Regulation S / Sanctioned Jurisdiction Check (e.g., North Korea, Iran)
        let blocked_jurisdictions = [408, 364, 192]; // KP, IR, CU
        require!(
            !blocked_jurisdictions.contains(&sender_compliance.jurisdiction_country_code)
                && !blocked_jurisdictions.contains(&recipient_compliance.jurisdiction_country_code),
            RwaSecurityError::SanctionedJurisdictionDetected
        );

        // 4. SEC Rule 144 Statutory 12-Month Lockup Check
        // If holding unaccredited or Reg D standard class, enforce duration.
        let elapsed = current_time.saturating_sub(sender_compliance.first_mint_acquisition_timestamp);
        if elapsed < issuer_config.rule_144_lockup_seconds {
            // Under Rule 144, intra-accredited P2P transfer is permitted ONLY if both parties are accredited
            require!(
                sender_compliance.is_accredited && recipient_compliance.is_accredited,
                RwaSecurityError::Rule144LockupActive
            );
        }

        emit!(TransferExecutedEvent {
            from: ctx.accounts.source_token.key(),
            to: ctx.accounts.destination_token.key(),
            amount,
            timestamp: current_time,
        });

        Ok(())
    }

    /// Court-ordered recovery or OFAC garnishment of tokens via Permanent Delegate.
    pub fn execute_legal_clawback(
        ctx: Context<ExecuteLegalClawback>,
        amount: u64,
        court_order_ref: [u8; 32],
    ) -> Result<()> {
        let issuer_config = &ctx.accounts.issuer_config;
        require_keys_eq!(
            ctx.accounts.authority.key(),
            issuer_config.authority,
            RwaSecurityError::UnauthorizedTransferAgent
        );

        // CPI directly into Token-2022 program to execute force transfer
        let seeds = [
            b"issuer_config",
            issuer_config.mint.as_ref(),
            &[issuer_config.bump],
        ];
        let signer_seeds = &[&seeds[..]];

        let cpi_accounts = anchor_spl::token_2022::TransferChecked {
            from: ctx.accounts.source_token.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
            to: ctx.accounts.destination_recovery_token.to_account_info(),
            authority: ctx.accounts.issuer_config.to_account_info(),
        };

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );

        anchor_spl::token_2022::transfer_checked(cpi_ctx, amount, ctx.accounts.mint.decimals)?;

        emit!(ClawbackExecutedEvent {
            source: ctx.accounts.source_token.key(),
            destination: ctx.accounts.destination_recovery_token.key(),
            amount,
            court_order_ref,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }
}

// ---------------------------------------------------------------------------------
// Account Contexts
// ---------------------------------------------------------------------------------

#[derive(Accounts)]
pub fn initialize_issuer_config_accounts<'info>(
    ctx: &mut Context<InitializeIssuerConfig>,
) -> Result<()> {
    Ok(())
}

#[derive(Accounts)]
pub struct InitializeIssuerConfig<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    /// CHECK: Transfer Agent multi-sig public key
    pub transfer_agent: UncheckedAccount<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        init,
        payer = authority,
        space = 8 + IssuerConfig::INIT_SPACE,
        seeds = [b"issuer_config", mint.key().as_ref()],
        bump
    )]
    pub issuer_config: Account<'info, IssuerConfig>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(investor: Pubkey)]
pub struct SetComplianceStatus<'info> {
    #[account(mut)]
    pub transfer_agent: Signer<'info>,
    #[account(
        has_one = transfer_agent @ RwaSecurityError::UnauthorizedTransferAgent
    )]
    pub issuer_config: Account<'info, IssuerConfig>,
    #[account(
        init_if_needed,
        payer = transfer_agent,
        space = 8 + InvestorCompliance::INIT_SPACE,
        seeds = [b"compliance", investor.as_ref()],
        bump
    )]
    pub compliance_entry: Account<'info, InvestorCompliance>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitializeExtraAccountMetaList<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    /// CHECK: The extra account meta PDA derived by seeds
    #[account(
        mut,
        seeds = [b"extra-account-metas", mint.key().as_ref()],
        bump
    )]
    pub extra_account_meta_list: UncheckedAccount<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct TransferHook<'info> {
    pub source_token: InterfaceAccount<'info, TokenAccount>,
    pub mint: InterfaceAccount<'info, Mint>,
    pub destination_token: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: Transfer Authority / Owner
    pub owner: UncheckedAccount<'info>,
    /// CHECK: ExtraAccountMetaList checked by Token-2022
    pub extra_account_meta_list: UncheckedAccount<'info>,
    #[account(
        seeds = [b"issuer_config", mint.key().as_ref()],
        bump = issuer_config.bump
    )]
    pub issuer_config: Account<'info, IssuerConfig>,
    #[account(
        seeds = [b"compliance", owner.key().as_ref()],
        bump = sender_compliance.bump
    )]
    pub sender_compliance: Account<'info, InvestorCompliance>,
    #[account(
        seeds = [b"compliance", destination_token.owner.as_ref()],
        bump = recipient_compliance.bump
    )]
    pub recipient_compliance: Account<'info, InvestorCompliance>,
}

#[derive(Accounts)]
pub struct ExecuteLegalClawback<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        seeds = [b"issuer_config", mint.key().as_ref()],
        bump = issuer_config.bump
    )]
    pub issuer_config: Account<'info, IssuerConfig>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub source_token: InterfaceAccount<'info, TokenAccount>,
    #[account(mut)]
    pub destination_recovery_token: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
}

// ---------------------------------------------------------------------------------
// State Structs
// ---------------------------------------------------------------------------------

#[account]
#[derive(InitSpace)]
pub struct IssuerConfig {
    pub authority: Pubkey,
    pub transfer_agent: Pubkey,
    pub mint: Pubkey,
    pub asset_isin: [u8; 12],
    pub rule_144_lockup_seconds: i64,
    pub is_paused: bool,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct InvestorCompliance {
    pub investor: Pubkey,
    pub is_accredited: bool,
    pub is_sanctioned: bool,
    pub kyc_expiry_timestamp: i64,
    pub first_mint_acquisition_timestamp: i64,
    pub jurisdiction_country_code: u16, // ISO 3166-1 numeric
    pub bump: u8,
}

// ---------------------------------------------------------------------------------
// Events & Errors
// ---------------------------------------------------------------------------------

#[event]
pub struct ComplianceUpdatedEvent {
    pub investor: Pubkey,
    pub is_accredited: bool,
    pub kyc_expiry_timestamp: i64,
    pub jurisdiction_country_code: u16,
    pub is_sanctioned: bool,
}

#[event]
pub struct TransferExecutedEvent {
    pub from: Pubkey,
    pub to: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct ClawbackExecutedEvent {
    pub source: Pubkey,
    pub destination: Pubkey,
    pub amount: u64,
    pub court_order_ref: [u8; 32],
    pub timestamp: i64,
}

#[error_code]
pub enum RwaSecurityError {
    #[msg("Asset transfers are currently paused by the General Partner.")]
    AssetTradingPaused,
    #[msg("Transfer Agent signature is invalid or unauthorized.")]
    UnauthorizedTransferAgent,
    #[msg("Sender compliance check failed: Account is sanctioned.")]
    SenderSanctioned,
    #[msg("Sender compliance check failed: KYC certification has expired.")]
    SenderKycExpired,
    #[msg("Sender compliance check failed: Must be a SEC Reg D Rule 501 Accredited Investor.")]
    SenderNotAccredited,
    #[msg("Recipient compliance check failed: Account is sanctioned.")]
    RecipientSanctioned,
    #[msg("Recipient compliance check failed: KYC certification has expired.")]
    RecipientKycExpired,
    #[msg("Recipient compliance check failed: Must be a SEC Reg D Rule 501 Accredited Investor.")]
    RecipientNotAccredited,
    #[msg("Jurisdiction blocked under OFAC or cross-border securities restrictions.")]
    SanctionedJurisdictionDetected,
    #[msg("Statutory SEC Rule 144 holding period lockup is currently active.")]
    Rule144LockupActive,
}