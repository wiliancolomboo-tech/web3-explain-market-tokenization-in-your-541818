use anchor_lang::prelude::*;
use anchor_spl::token_interface::TokenAccount;
use spl_transfer_hook_interface::instruction::TransferHookInstruction;

declare_id!("HOOK1111111111111111111111111111111111111111");

pub const REGISTRY_SEED: &[u8] = b"compliance_registry";
pub const INVESTOR_SEED: &[u8] = b"investor_profile";

#[program]
pub mod rwa_transfer_hook {
    use super::*;

    /// Execute transfer hook interface entry point
    pub fn execute(ctx: Context<ExecuteTransferHook>, amount: u64) -> Result<()> {
        let clock = Clock::get()?;

        // Verify sender lockup expiration (Reg D 1-year Rule 144 requirement)
        let sender_profile = &ctx.accounts.sender_investor_profile;
        require!(!sender_profile.is_frozen, HookError::AccountFrozen);
        require!(
            clock.unix_timestamp >= sender_profile.lockup_expiry,
            HookError::LockupPeriodActive
        );

        // Verify recipient accreditation & KYC status
        let recipient_profile = &ctx.accounts.recipient_investor_profile;
        require!(!recipient_profile.is_frozen, HookError::AccountFrozen);
        require!(recipient_profile.is_accredited, HookError::RecipientNotAccredited);
        require!(
            recipient_profile.kyc_expiry > clock.unix_timestamp,
            HookError::RecipientKycExpired
        );

        msg!(
            "TransferHook Approved: Transfer of {} units permitted from {} to {}",
            amount,
            ctx.accounts.source_token.key(),
            ctx.accounts.destination_token.key()
        );

        Ok(())
    }

    /// Fallback dispatcher conforming to the SPL Transfer Hook interface ID
    pub fn fallback<'info>(
        program_id: &Pubkey,
        accounts: &'info [AccountInfo<'info>],
        data: &[u8],
    ) -> Result<()> {
        let instruction = TransferHookInstruction::unpack(data)
            .map_err(|_| ProgramError::InvalidInstructionData)?;

        match instruction {
            TransferHookInstruction::Execute { amount } => {
                let amount_bytes = amount.to_le_bytes();
                let mut execute_data = vec![105, 37, 101, 197, 75, 251, 102, 26]; // Anchor execute discriminator
                execute_data.extend_from_slice(&amount_bytes);

                crate::__private::__global::execute(program_id, accounts, &execute_data)
            }
            _ => Err(ProgramError::InvalidInstructionData.into()),
        }
    }
}

#[derive(Accounts)]
pub struct ExecuteTransferHook<'info> {
    #[account(
        token::token_program = anchor_spl::token_2022::Token2022::id()
    )]
    pub source_token: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: Mint verified in SPL Transfer Hook validation
    pub mint: AccountInfo<'info>,

    #[account(
        token::token_program = anchor_spl::token_2022::Token2022::id()
    )]
    pub destination_token: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: Owner of source token
    pub owner_authority: AccountInfo<'info>,

    /// CHECK: Extra account metas PDA
    pub extra_account_metas: AccountInfo<'info>,

    /// Sender Investor Profile PDA
    #[account(
        seeds = [INVESTOR_SEED, asset_config.key().as_ref(), source_token.owner.as_ref()],
        seeds::program = rwa_security_engine_id(),
        bump = sender_investor_profile.bump
    )]
    pub sender_investor_profile: Account<'info, InvestorProfileState>,

    /// Recipient Investor Profile PDA
    #[account(
        seeds = [INVESTOR_SEED, asset_config.key().as_ref(), destination_token.owner.as_ref()],
        seeds::program = rwa_security_engine_id(),
        bump = recipient_investor_profile.bump
    )]
    pub recipient_investor_profile: Account<'info, InvestorProfileState>,

    /// CHECK: Main asset config account from security engine
    pub asset_config: AccountInfo<'info>,
}

fn rwa_security_engine_id() -> Pubkey {
    Pubkey::new_from_array(crate::id().to_bytes()) // Resolves dynamically to linked engine
}

#[account]
pub struct InvestorProfileState {
    pub wallet: Pubkey,
    pub jurisdiction: [u8; 2],
    pub is_accredited: bool,
    pub kyc_expiry: i64,
    pub lockup_expiry: i64,
    pub is_frozen: bool,
    pub bump: u8,
}

#[error_code]
pub enum HookError {
    #[msg("Account is frozen by compliance order")]
    AccountFrozen,
    #[msg("SEC Rule 144 / Reg D 12-month lockup is still active for sender")]
    LockupPeriodActive,
    #[msg("Recipient is not an accredited investor")]
    RecipientNotAccredited,
    #[msg("Recipient KYC has expired")]
    RecipientKycExpired,
}