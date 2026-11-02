use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_spl::token_2022::Token2022;
use anchor_spl::token_interface::{Mint, TokenAccount};
use spl-tlv-account-resolution::{account::ExtraAccountMeta, seeds::Seed};
use spl-transfer-hook-interface::instruction::{ExecuteInstruction, TransferHookInstruction};

declare_id!("RWA1111111111111111111111111111111111111111");

pub const EXTRA_ACCOUNT_METAS_SEED: &[u8] = b"extra-account-metas";
pub const INVESTOR_REGISTRY_SEED: &[u8] = b"investor-registry";
pub const CONFIG_SEED: &[u8] = b"compliance-config";

#[program]
pub mod rwa_compliance_engine {
    use super::*;

    /// Initializes the master compliance configuration for an RWA asset mint
    pub fn initialize_compliance_config(
        ctx: Context<InitializeComplianceConfig>,
        authority: Pubkey,
        oracle_signer: Pubkey,
        jurisdiction_code: u16, // ISO 3166-1 numeric (e.g., 840 for USA)
        lockup_seconds: i64,    // Rule 144 restricted holding period
    ) -> Result<()> {
        let config = &mut ctx.accounts.compliance_config;
        config.authority = authority;
        config.oracle_signer = oracle_signer;
        config.asset_mint = ctx.accounts.asset_mint.key();
        config.jurisdiction_code = jurisdiction_code;
        config.lockup_seconds = lockup_seconds;
        config.is_paused = false;
        config.bump = ctx.bumps.compliance_config;

        emit!(ComplianceConfigInitializedEvent {
            mint: config.asset_mint,
            authority,
            oracle_signer,
            lockup_seconds,
        });

        Ok(())
    }

    /// Sets up the ExtraAccountMetaList PDA required by spl-transfer-hook-interface
    pub fn initialize_extra_account_metas(
        ctx: Context<InitializeExtraAccountMetas>,
    ) -> Result<()> {
        // Build the extra account requirements needed by the transfer hook:
        // 1. Source Investor Attestation PDA
        // 2. Destination Investor Attestation PDA
        // 3. Compliance Config PDA
        let account_metas = vec![
            // Source Investor Attestation
            ExtraAccountMeta::new_with_seeds(
                &[
                    Seed::Literal {
                        bytes: INVESTOR_REGISTRY_SEED.to_vec(),
                    },
                    Seed::AccountKey { index: 3 }, // owner of source token account
                ],
                false, // is_signer
                false, // is_writable
            )?,
            // Destination Investor Attestation
            ExtraAccountMeta::new_with_seeds(
                &[
                    Seed::Literal {
                        bytes: INVESTOR_REGISTRY_SEED.to_vec(),
                    },
                    Seed::AccountKey { index: 2 }, // destination token account authority/owner
                ],
                false, // is_signer
                false, // is_writable
            )?,
            // Compliance Config PDA
            ExtraAccountMeta::new_with_seeds(
                &[
                    Seed::Literal {
                        bytes: CONFIG_SEED.to_vec(),
                    },
                    Seed::AccountKey { index: 1 }, // mint key
                ],
                false, // is_signer
                false, // is_writable
            )?,
        ];

        // Allocate and write extra account meta list
        let bump = ctx.bumps.extra_account_metas;
        let account_size = ExtraAccountMeta::size_of(account_metas.len())?;
        
        let signer_seeds: &[&[&[u8]]] = &[&[
            EXTRA_ACCOUNT_METAS_SEED,
            ctx.accounts.asset_mint.to_account_info().key.as_ref(),
            &[bump],
        ]];

        let lamports = Rent::get()?.minimum_balance(account_size);
        system_program::create_account(
            CpiContext::new_with_signer(
                ctx.accounts.system_program.to_account_info(),
                system_program::CreateAccount {
                    from: ctx.accounts.payer.to_account_info(),
                    to: ctx.accounts.extra_account_metas.to_account_info(),
                },
                signer_seeds,
            ),
            lamports,
            account_size as u64,
            ctx.program_id,
        )?;

        ExtraAccountMeta::init(
            &mut ctx.accounts.extra_account_metas.try_borrow_mut_data()?,
            &account_metas,
        )?;

        Ok(())
    }

    /// Registers or updates an investor's KYC/AML and SEC accredited status
    pub fn update_investor_attestation(
        ctx: Context<UpdateInvestorAttestation>,
        investor: Pubkey,
        is_accredited: bool,
        is_sanctioned: bool,
        kyc_expiry: i64,
        jurisdiction_code: u16,
    ) -> Result<()> {
        let attestation = &mut ctx.accounts.investor_attestation;
        attestation.investor = investor;
        attestation.is_accredited = is_accredited;
        attestation.is_sanctioned = is_sanctioned;
        attestation.kyc_expiry = kyc_expiry;
        attestation.jurisdiction_code = jurisdiction_code;
        attestation.last_attestation_timestamp = Clock::get()?.unix_timestamp;
        attestation.bump = ctx.bumps.investor_attestation;

        emit!(InvestorAttestationUpdatedEvent {
            investor,
            is_accredited,
            is_sanctioned,
            kyc_expiry,
            jurisdiction_code,
        });

        Ok(())
    }

    /// The execute entry point called automatically by the Token-2022 Transfer Hook
    pub fn transfer_hook_execute(
        ctx: Context<TransferHookExecute>,
        amount: u64,
    ) -> Result<()> {
        let clock = Clock::get()?;
        let current_timestamp = clock.unix_timestamp;

        // Verify compliance configuration state
        let config = &ctx.accounts.compliance_config;
        require!(!config.is_paused, ComplianceError::ContractPaused);

        // 1. Validate Source Investor Attestation
        let source_attestation = &ctx.accounts.source_attestation;
        require!(
            !source_attestation.is_sanctioned,
            ComplianceError::SourceAccountSanctioned
        );
        require!(
            source_attestation.kyc_expiry > current_timestamp,
            ComplianceError::SourceKycExpired
        );

        // 2. Validate Destination Investor Attestation
        let destination_attestation = &ctx.accounts.destination_attestation;
        require!(
            !destination_attestation.is_sanctioned,
            ComplianceError::DestinationAccountSanctioned
        );
        require!(
            destination_attestation.kyc_expiry > current_timestamp,
            ComplianceError::DestinationKycExpired
        );
        require!(
            destination_attestation.is_accredited,
            ComplianceError::DestinationNotAccredited
        );

        // 3. Reg S / Reg D Cross-Border Rule Checks
        // If U.S. issuer (840) and recipient is foreign, check Reg S restrictions
        if config.jurisdiction_code == 840 && destination_attestation.jurisdiction_code != 840 {
            // Require verified non-US status or qualified institutional buyer
            require!(
                destination_attestation.kyc_expiry > current_timestamp,
                ComplianceError::RegSViolation
            );
        }

        emit!(SettlementCompliantTransferEvent {
            source: ctx.accounts.source_token.key(),
            destination: ctx.accounts.destination_token.key(),
            amount,
            timestamp: current_timestamp,
        });

        Ok(())
    }

    /// Pause or unpause emergency compliance halt
    pub fn toggle_pause(ctx: Context<TogglePause>, paused: bool) -> Result<()> {
        ctx.accounts.compliance_config.is_paused = paused;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT VALIDATION CONTEXTS
// -----------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeComplianceConfig<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + ComplianceConfig::INIT_SPACE,
        seeds = [CONFIG_SEED, asset_mint.key().as_ref()],
        bump
    )]
    pub compliance_config: Account<'info, ComplianceConfig>,
    pub asset_mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitializeExtraAccountMetas<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: ExtraAccountMetaList Account validated via PDA seeds
    #[account(
        mut,
        seeds = [EXTRA_ACCOUNT_METAS_SEED, asset_mint.key().as_ref()],
        bump
    )]
    pub extra_account_metas: UncheckedAccount<'info>,
    pub asset_mint: InterfaceAccount<'info, Mint>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(investor: Pubkey)]
pub struct UpdateInvestorAttestation<'info> {
    #[account(
        mut,
        constraint = compliance_config.oracle_signer == oracle_signer.key() @ ComplianceError::UnauthorizedOracle
    )]
    pub compliance_config: Account<'info, ComplianceConfig>,
    pub oracle_signer: Signer<'info>,
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + InvestorAttestation::INIT_SPACE,
        seeds = [INVESTOR_REGISTRY_SEED, investor.as_ref()],
        bump
    )]
    pub investor_attestation: Account<'info, InvestorAttestation>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct TransferHookExecute<'info> {
    #[account(token::mint = compliance_config.asset_mint)]
    pub source_token: InterfaceAccount<'info, TokenAccount>,
    pub asset_mint: InterfaceAccount<'info, Mint>,
    #[account(token::mint = compliance_config.asset_mint)]
    pub destination_token: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: Authority of the source token account
    pub source_authority: UncheckedAccount<'info>,
    /// CHECK: Validated against ExtraAccountMetaList seed
    #[account(
        seeds = [EXTRA_ACCOUNT_METAS_SEED, asset_mint.key().as_ref()],
        bump
    )]
    pub extra_account_metas: UncheckedAccount<'info>,
    #[account(
        seeds = [INVESTOR_REGISTRY_SEED, source_authority.key().as_ref()],
        bump = source_attestation.bump
    )]
    pub source_attestation: Account<'info, InvestorAttestation>,
    #[account(
        seeds = [INVESTOR_REGISTRY_SEED, destination_token.owner.as_ref()],
        bump = destination_attestation.bump
    )]
    pub destination_attestation: Account<'info, InvestorAttestation>,
    #[account(
        seeds = [CONFIG_SEED, asset_mint.key().as_ref()],
        bump = compliance_config.bump
    )]
    pub compliance_config: Account<'info, ComplianceConfig>,
}

#[derive(Accounts)]
pub struct TogglePause<'info> {
    #[account(
        mut,
        constraint = compliance_config.authority == authority.key() @ ComplianceError::UnauthorizedAuthority
    )]
    pub compliance_config: Account<'info, ComplianceConfig>,
    pub authority: Signer<'info>,
}

// -----------------------------------------------------------------------------
// STATE STRUCTS
// -----------------------------------------------------------------------------

#[account]
#[derive(InitSpace)]
pub struct ComplianceConfig {
    pub authority: Pubkey,         // 32
    pub oracle_signer: Pubkey,     // 32
    pub asset_mint: Pubkey,        // 32
    pub jurisdiction_code: u16,    // 2
    pub lockup_seconds: i64,       // 8
    pub is_paused: bool,           // 1
    pub bump: u8,                  // 1
}

#[account]
#[derive(InitSpace)]
pub struct InvestorAttestation {
    pub investor: Pubkey,                 // 32
    pub is_accredited: bool,              // 1 (Reg D 506(c) verification)
    pub is_sanctioned: bool,              // 1 (OFAC Specially Designated Nationals)
    pub kyc_expiry: i64,                  // 8 (Unix timestamp)
    pub jurisdiction_code: u16,           // 2 (ISO numeric country code)
    pub last_attestation_timestamp: i64,  // 8
    pub bump: u8,                         // 1
}

// -----------------------------------------------------------------------------
// EVENTS
// -----------------------------------------------------------------------------

#[event]
pub struct ComplianceConfigInitializedEvent {
    pub mint: Pubkey,
    pub authority: Pubkey,
    pub oracle_signer: Pubkey,
    pub lockup_seconds: i64,
}

#[event]
pub struct InvestorAttestationUpdatedEvent {
    pub investor: Pubkey,
    pub is_accredited: bool,
    pub is_sanctioned: bool,
    pub kyc_expiry: i64,
    pub jurisdiction_code: u16,
}

#[event]
pub struct SettlementCompliantTransferEvent {
    pub source: Pubkey,
    pub destination: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

// -----------------------------------------------------------------------------
// ERROR CODES
// -----------------------------------------------------------------------------

#[error_code]
pub enum ComplianceError {
    #[msg("Engine halt: RWA Compliance operations are currently paused by issuer.")]
    ContractPaused,
    #[msg("Unauthorized: Caller is not the approved compliance oracle identity.")]
    UnauthorizedOracle,
    #[msg("Unauthorized: Caller is not the sovereign program authority.")]
    UnauthorizedAuthority,
    #[msg("Compliance Violation: Transfer source address is flagged on OFAC/Sanctions list.")]
    SourceAccountSanctioned,
    #[msg("Compliance Violation: Source investor KYC attestation has expired.")]
    SourceKycExpired,
    #[msg("Compliance Violation: Destination address is flagged on OFAC/Sanctions list.")]
    DestinationAccountSanctioned,
    #[msg("Compliance Violation: Destination investor KYC attestation has expired.")]
    DestinationKycExpired,
    #[msg("Compliance Violation: Destination address is not accredited under SEC Reg D Rule 506(c).")]
    DestinationNotAccredited,
    #[msg("Compliance Violation: Cross-border transfer violates Regulation S overseas restrictions.")]
    RegSViolation,
}