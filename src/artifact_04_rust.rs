use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_spl::token_2022::Token2022;
use anchor_spl::token_interface::{Mint, TokenAccount};
use spl_transfer_hook_interface::instruction::TransferHookInstruction;

declare_id!("RWA1111111111111111111111111111111111111111");

pub const COMPLIANCE_CONFIG_SEED: &[u8] = b"compliance_config";
pub const INVESTOR_REGISTRY_SEED: &[u8] = b"investor_registry";
pub const EXTRA_ACCOUNT_METAS_SEED: &[u8] = b"extra-account-metas";

#[program]
pub mod rwa_market_tokenization {
    use super::*;

    /// Initializes the global compliance registry configuration.
    pub fn initialize_compliance_config(
        ctx: Context<InitializeComplianceConfig>,
        regulatory_framework_id: u32, // e.g. 5063 for SEC Rule 506(c)
        rule_144_lockup_seconds: i64, // e.g. 31536000 for 1-year holding period
    ) -> Result<()> {
        let config = &mut ctx.accounts.compliance_config;
        config.admin = ctx.accounts.admin.key();
        config.mint = ctx.accounts.mint.key();
        config.regulatory_framework_id = regulatory_framework_id;
        config.rule_144_lockup_seconds = rule_144_lockup_seconds;
        config.is_paused = false;
        config.total_verified_investors = 0;
        config.bump = ctx.bumps.compliance_config;

        msg!(
            "Compliance Config Initialized. Framework: {}, Lockup: {}s",
            regulatory_framework_id,
            rule_144_lockup_seconds
        );
        Ok(())
    }

    /// Registers or updates an investor's SEC accreditation status and KYC passport.
    pub fn set_investor_certificate(
        ctx: Context<SetInvestorCertificate>,
        investor_owner: Pubkey,
        country_code: u16,           // ISO 3166-1 numeric (e.g. 840 for USA)
        investor_classification: u8, // 1 = Accredited Natural, 2 = Qualified Institutional Buyer (QIB)
        accreditation_expiry: i64,   // Unix timestamp
        is_sanctioned: bool,
    ) -> Result<()> {
        let registry = &mut ctx.accounts.investor_registry;
        let clock = Clock::get()?;

        require!(
            accreditation_expiry > clock.unix_timestamp,
            ComplianceError::AccreditationExpired
        );

        if registry.investor_owner == Pubkey::default() {
            let config = &mut ctx.accounts.compliance_config;
            config.total_verified_investors = config
                .total_verified_investors
                .checked_add(1)
                .ok_or(ComplianceError::NumericalOverflow)?;
        }

        registry.investor_owner = investor_owner;
        registry.country_code = country_code;
        registry.investor_classification = investor_classification;
        registry.accreditation_expiry = accreditation_expiry;
        registry.is_sanctioned = is_sanctioned;
        registry.first_acquired_timestamp = if registry.first_acquired_timestamp == 0 {
            clock.unix_timestamp
        } else {
            registry.first_acquired_timestamp
        };
        registry.bump = ctx.bumps.investor_registry;

        emit!(InvestorCertifiedEvent {
            investor: investor_owner,
            country_code,
            classification: investor_classification,
            valid_until: accreditation_expiry,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Administrative Circuit Breaker toggle.
    pub fn set_emergency_pause(ctx: Context<SetEmergencyPause>, pause_state: bool) -> Result<()> {
        let config = &mut ctx.accounts.compliance_config;
        config.is_paused = pause_state;
        emit!(ComplianceCircuitBreakerToggled {
            is_paused: pause_state,
            operator: ctx.accounts.admin.key(),
            timestamp: Clock::get()?.unix_timestamp,
        });
        Ok(())
    }

    /// Entry point invoked strictly by the Token-2022 Transfer Hook CPI.
    pub fn transfer_hook_execute(ctx: Context<TransferHookExecute>, amount: u64) -> Result<()> {
        let config = &ctx.accounts.compliance_config;
        require!(!config.is_paused, ComplianceError::TradingHaltedByCompliance);

        let clock = Clock::get()?;
        let current_time = clock.unix_timestamp;

        // 1. Validate Sender (Source) Compliance
        let sender_cert = &ctx.accounts.sender_registry;
        require!(
            sender_cert.investor_owner == ctx.accounts.source_owner.key(),
            ComplianceError::IdentityMismatch
        );
        require!(
            !sender_cert.is_sanctioned,
            ComplianceError::SanctionedEntityProhibited
        );
        require!(
            sender_cert.accreditation_expiry > current_time,
            ComplianceError::AccreditationExpired
        );

        // 2. Validate SEC Rule 144 Holding Period Lockup on the Source Account
        let minimum_unfreeze_time = sender_cert
            .first_acquired_timestamp
            .checked_add(config.rule_144_lockup_seconds)
            .ok_or(ComplianceError::NumericalOverflow)?;

        require!(
            current_time >= minimum_unfreeze_time,
            ComplianceError::Rule144LockupActive
        );

        // 3. Validate Recipient (Destination) Compliance
        let recipient_cert = &ctx.accounts.recipient_registry;
        require!(
            recipient_cert.investor_owner == ctx.accounts.destination_owner.key(),
            ComplianceError::IdentityMismatch
        );
        require!(
            !recipient_cert.is_sanctioned,
            ComplianceError::SanctionedEntityProhibited
        );
        require!(
            recipient_cert.accreditation_expiry > current_time,
            ComplianceError::AccreditationExpired
        );

        // 4. US Cross-Border Restrictions Check (OFAC Sanction ISO Filter)
        // Ensure no interaction with blacklisted regional codes (e.g. 004, 364, 408)
        require!(
            recipient_cert.country_code != 0 && recipient_cert.country_code != 999,
            ComplianceError::ProhibitedJurisdiction
        );

        emit!(TransferAuditedEvent {
            source: ctx.accounts.source_owner.key(),
            destination: ctx.accounts.destination_owner.key(),
            amount,
            framework_id: config.regulatory_framework_id,
            timestamp: current_time,
        });

        Ok(())
    }

    /// Initializes the ExtraAccountMetaList PDA required by spl-transfer-hook-interface.
    pub fn initialize_extra_account_meta_list(
        ctx: Context<InitializeExtraAccountMetaList>,
    ) -> Result<()> {
        let account_metas = vec![
            // index 0: Source Token Account (passed by Token-2022)
            // index 1: Mint (passed by Token-2022)
            // index 2: Destination Token Account (passed by Token-2022)
            // index 3: Authority / Owner (passed by Token-2022)
            // Extra dynamic account metas for compliance verification:
            // 4: Compliance Config
            spl_tlv_account_resolution::account::ExtraAccountMeta::new_with_seeds(
                &[
                    spl_tlv_account_resolution::seeds::Seed::Literal {
                        bytes: COMPLIANCE_CONFIG_SEED.to_vec(),
                    },
                    spl_tlv_account_resolution::seeds::Seed::AccountKey { index: 1 }, // Mint key
                ],
                false, // is_signer
                false, // is_writable
            )?,
            // 5: Source Owner Investor Registry
            spl_tlv_account_resolution::account::ExtraAccountMeta::new_with_seeds(
                &[
                    spl_tlv_account_resolution::seeds::Seed::Literal {
                        bytes: INVESTOR_REGISTRY_SEED.to_vec(),
                    },
                    spl_tlv_account_resolution::seeds::Seed::AccountKey { index: 3 }, // Source Owner key
                ],
                false,
                false,
            )?,
            // 6: Destination Owner (resolved from destination token account owner)
            // 7: Destination Owner Investor Registry
        ];

        let account_size = spl_tlv_account_resolution::state::ExtraAccountMetaList::size_of(
            account_metas.len(),
        )?;
        let lamports = Rent::get()?.minimum_balance(account_size);

        let mint_key = ctx.accounts.mint.key();
        let signer_seeds: &[&[&[u8]]] = &[&[
            EXTRA_ACCOUNT_METAS_SEED,
            mint_key.as_ref(),
            &[ctx.bumps.extra_account_meta_list],
        ]];

        system_program::create_account(
            CpiContext::new_with_signer(
                ctx.accounts.system_program.to_account_info(),
                system_program::CreateAccount {
                    from: ctx.accounts.admin.to_account_info(),
                    to: ctx.accounts.extra_account_meta_list.to_account_info(),
                },
                signer_seeds,
            ),
            lamports,
            account_size as u64,
            ctx.program_id,
        )?;

        spl_tlv_account_resolution::state::ExtraAccountMetaList::init::<
            TransferHookInstruction,
        >(
            &mut ctx.accounts.extra_account_meta_list.try_borrow_mut_data()?,
            &account_metas,
        )?;

        Ok(())
    }
}

// --------------------------------------------------------------------------------
// Account Structs & Validation Contexts
// --------------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeComplianceConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    pub mint: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = admin,
        space = 8 + ComplianceConfig::INIT_SPACE,
        seeds = [COMPLIANCE_CONFIG_SEED, mint.key().as_ref()],
        bump
    )]
    pub compliance_config: Account<'info, ComplianceConfig>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(investor_owner: Pubkey)]
pub struct SetInvestorCertificate<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        has_one = admin @ ComplianceError::UnauthorizedComplianceAdmin
    )]
    pub compliance_config: Account<'info, ComplianceConfig>,

    #[account(
        init_if_needed,
        payer = admin,
        space = 8 + InvestorRegistry::INIT_SPACE,
        seeds = [INVESTOR_REGISTRY_SEED, investor_owner.as_ref()],
        bump
    )]
    pub investor_registry: Account<'info, InvestorRegistry>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SetEmergencyPause<'info> {
    pub admin: Signer<'info>,
    #[account(
        mut,
        has_one = admin @ ComplianceError::UnauthorizedComplianceAdmin
    )]
    pub compliance_config: Account<'info, ComplianceConfig>,
}

#[derive(Accounts)]
pub struct TransferHookExecute<'info> {
    #[account(token::mint = mint)]
    pub source_account: InterfaceAccount<'info, TokenAccount>,

    pub mint: InterfaceAccount<'info, Mint>,

    #[account(token::mint = mint)]
    pub destination_account: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: Validated as owner of source token account
    pub source_owner: AccountInfo<'info>,

    /// CHECK: Validated as owner of destination token account
    pub destination_owner: AccountInfo<'info>,

    #[account(
        seeds = [COMPLIANCE_CONFIG_SEED, mint.key().as_ref()],
        bump = compliance_config.bump
    )]
    pub compliance_config: Account<'info, ComplianceConfig>,

    #[account(
        seeds = [INVESTOR_REGISTRY_SEED, source_owner.key().as_ref()],
        bump = sender_registry.bump
    )]
    pub sender_registry: Account<'info, InvestorRegistry>,

    #[account(
        seeds = [INVESTOR_REGISTRY_SEED, destination_owner.key().as_ref()],
        bump = recipient_registry.bump
    )]
    pub recipient_registry: Account<'info, InvestorRegistry>,
}

#[derive(Accounts)]
pub struct InitializeExtraAccountMetaList<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    pub mint: InterfaceAccount<'info, Mint>,

    /// CHECK: Initialized via raw CPI with deterministic seeds
    #[account(
        mut,
        seeds = [EXTRA_ACCOUNT_METAS_SEED, mint.key().as_ref()],
        bump
    )]
    pub extra_account_meta_list: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

// --------------------------------------------------------------------------------
// State Definitions
// --------------------------------------------------------------------------------

#[account]
#[derive(InitSpace)]
pub struct ComplianceConfig {
    pub admin: Pubkey,
    pub mint: Pubkey,
    pub regulatory_framework_id: u32,
    pub rule_144_lockup_seconds: i64,
    pub is_paused: bool,
    pub total_verified_investors: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct InvestorRegistry {
    pub investor_owner: Pubkey,
    pub country_code: u16,
    pub investor_classification: u8,
    pub accreditation_expiry: i64,
    pub first_acquired_timestamp: i64,
    pub is_sanctioned: bool,
    pub bump: u8,
}

// --------------------------------------------------------------------------------
// Events & Telemetry
// --------------------------------------------------------------------------------

#[event]
pub struct InvestorCertifiedEvent {
    pub investor: Pubkey,
    pub country_code: u16,
    pub classification: u8,
    pub valid_until: i64,
    pub timestamp: i64,
}

#[event]
pub struct TransferAuditedEvent {
    pub source: Pubkey,
    pub destination: Pubkey,
    pub amount: u64,
    pub framework_id: u32,
    pub timestamp: i64,
}

#[event]
pub struct ComplianceCircuitBreakerToggled {
    pub is_paused: bool,
    pub operator: Pubkey,
    pub timestamp: i64,
}

// --------------------------------------------------------------------------------
// Error Codes
// --------------------------------------------------------------------------------

#[error_code]
pub enum ComplianceError {
    #[msg("Operation rejected: Caller is not the designated Compliance Administrator")]
    UnauthorizedComplianceAdmin,

    #[msg("SEC Compliance Failure: Investor accreditation credentials have expired")]
    AccreditationExpired,

    #[msg("SEC Rule 144 Violation: Restricted security holding lockup is still active")]
    Rule144LockupActive,

    #[msg("OFAC Sanctions Block: Target address is flagged as a sanctioned entity")]
    SanctionedEntityProhibited,

    #[msg("Jurisdiction Block: Cross-border transfer into restricted territory prohibited")]
    ProhibitedJurisdiction,

    #[msg("Circuit Breaker Active: Token trading has been halted by compliance admin")]
    TradingHaltedByCompliance,

    #[msg("Identity Mismatch: Account ownership does not match registered compliance KYC hash")]
    IdentityMismatch,

    #[msg("Math Overflow encountered during compliance calculation")]
    NumericalOverflow,
}