use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke;
use anchor_spl::token_interface::{
    approve, initialize_mint2, mint_close_authority_initialize, spl_token_2022,
    transfer_checked, transfer_fee_initialize, Approve, InitializeMint2, Mint,
    MintCloseAuthorityInitialize, TokenInterface, TransferChecked, TransferFeeInitialize,
};
use spl_token_2022::{
    extension::{
        confidential_transfer::{instruction as confidential_instruction, DecryptableBalance},
        confidential_transfer_fee::instruction as confidential_fee_instruction,
        transfer_fee::TransferFeeConfig,
        BaseStateWithExtensions, ExtensionType, StateWithExtensions,
    },
    state::Mint as MintState,
};

/// Length of the AES ciphertext used for Token-2022 decryptable balances.
pub const AE_CIPHERTEXT_LEN: usize = 36;

declare_id!("6sC5C8VFoTpEZQVn3YK9EUSd5g3Cs6zTT3HCDBGQkyo4");

/// Extensions this program understands when validating an arbitrary mint.
const SUPPORTED_EXTENSIONS: &[ExtensionType] = &[
    ExtensionType::MintCloseAuthority,
    ExtensionType::MetadataPointer,
    ExtensionType::TransferFeeConfig,
];

#[program]
pub mod t22 {
    use super::*;

    /// Create a Token-2022 mint using Anchor's declarative extension constraints.
    pub fn create_mint_declarative(
        ctx: Context<CreateMintDeclarative>,
        decimals: u8,
    ) -> Result<()> {
        msg!(
            "mint {} created with {} decimals",
            ctx.accounts.mint.key(),
            decimals
        );
        Ok(())
    }

    /// Create a Token-2022 mint carrying MintCloseAuthority
    /// and TransferFeeConfig.
    pub fn create_mint_with_fee(
        ctx: Context<CreateMintWithFee>,
        decimals: u8,
        basis_points: u16,
        maximum_fee: u64,
    ) -> Result<()> {
        let extensions = [
            ExtensionType::MintCloseAuthority,
            ExtensionType::TransferFeeConfig,
        ];

        let space =
            ExtensionType::try_calculate_account_len::<MintState>(&extensions)?;

        let lamports = Rent::get()?.minimum_balance(space);

        anchor_lang::system_program::create_account(
            CpiContext::new(
                ctx.accounts.system_program.key(),
                anchor_lang::system_program::CreateAccount {
                    from: ctx.accounts.payer.to_account_info(),
                    to: ctx.accounts.mint.to_account_info(),
                },
            ),
            lamports,
            space as u64,
            &ctx.accounts.token_program.key(),
        )?;

        mint_close_authority_initialize(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                MintCloseAuthorityInitialize {
                    token_program_id: ctx.accounts.token_program.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                },
            ),
            Some(&ctx.accounts.payer.key()),
        )?;

        transfer_fee_initialize(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                TransferFeeInitialize {
                    token_program_id: ctx.accounts.token_program.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                },
            ),
            Some(&ctx.accounts.payer.key()),
            Some(&ctx.accounts.payer.key()),
            basis_points,
            maximum_fee,
        )?;

        initialize_mint2(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                InitializeMint2 {
                    mint: ctx.accounts.mint.to_account_info(),
                },
            ),
            decimals,
            &ctx.accounts.payer.key(),
            None,
        )?;

        msg!(
            "fee mint {} created with {} bytes",
            ctx.accounts.mint.key(),
            space
        );

        Ok(())
    }

    /// Create a mint with ConfidentialTransferMint enabled.
    pub fn create_confidential_mint(
        ctx: Context<CreateConfidentialMint>,
        decimals: u8,
        auto_approve_new_accounts: bool,
    ) -> Result<()> {
        let space = ExtensionType::try_calculate_account_len::<MintState>(&[
            ExtensionType::ConfidentialTransferMint,
        ])?;

        let lamports = Rent::get()?.minimum_balance(space);

        anchor_lang::system_program::create_account(
            CpiContext::new(
                ctx.accounts.system_program.key(),
                anchor_lang::system_program::CreateAccount {
                    from: ctx.accounts.payer.to_account_info(),
                    to: ctx.accounts.mint.to_account_info(),
                },
            ),
            lamports,
            space as u64,
            &ctx.accounts.token_program.key(),
        )?;

        let initialize_confidential_ix =
            confidential_instruction::initialize_mint(
                &ctx.accounts.token_program.key(),
                &ctx.accounts.mint.key(),
                Some(ctx.accounts.payer.key()),
                auto_approve_new_accounts,
                None,
            )?;

        invoke(
            &initialize_confidential_ix,
            &[
                ctx.accounts.mint.to_account_info(),
                ctx.accounts.token_program.to_account_info(),
            ],
        )?;

        initialize_mint2(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                InitializeMint2 {
                    mint: ctx.accounts.mint.to_account_info(),
                },
            ),
            decimals,
            &ctx.accounts.payer.key(),
            None,
        )?;

        msg!(
            "confidential mint {} created with {} bytes",
            ctx.accounts.mint.key(),
            space
        );

        Ok(())
    }

    /// Create a confidential-transfer mint that also charges
    /// confidential transfer fees.
    pub fn create_confidential_fee_mint(
        ctx: Context<CreateConfidentialFeeMint>,
        decimals: u8,
        basis_points: u16,
        maximum_fee: u64,
        withdraw_withheld_authority_elgamal_pubkey: [u8; 32],
    ) -> Result<()> {
        let extensions = [
            ExtensionType::TransferFeeConfig,
            ExtensionType::ConfidentialTransferMint,
            ExtensionType::ConfidentialTransferFeeConfig,
        ];

        let space =
            ExtensionType::try_calculate_account_len::<MintState>(&extensions)?;

        let lamports = Rent::get()?.minimum_balance(space);

        anchor_lang::system_program::create_account(
            CpiContext::new(
                ctx.accounts.system_program.key(),
                anchor_lang::system_program::CreateAccount {
                    from: ctx.accounts.payer.to_account_info(),
                    to: ctx.accounts.mint.to_account_info(),
                },
            ),
            lamports,
            space as u64,
            &ctx.accounts.token_program.key(),
        )?;

        let mint_info = ctx.accounts.mint.to_account_info();
        let token_program_info = ctx.accounts.token_program.to_account_info();

        let invoke_accounts = [
            mint_info.clone(),
            token_program_info.clone(),
        ];

        // Initialize normal transfer fees first.
        transfer_fee_initialize(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                TransferFeeInitialize {
                    token_program_id: token_program_info.clone(),
                    mint: mint_info.clone(),
                },
            ),
            Some(&ctx.accounts.payer.key()),
            Some(&ctx.accounts.payer.key()),
            basis_points,
            maximum_fee,
        )?;

        // Initialize confidential transfers.
        let initialize_confidential_ix =
            confidential_instruction::initialize_mint(
                &ctx.accounts.token_program.key(),
                &ctx.accounts.mint.key(),
                Some(ctx.accounts.payer.key()),
                true,
                None,
            )?;

        invoke(
            &initialize_confidential_ix,
            &invoke_accounts,
        )?;

        // Initialize confidential transfer fees.
        let initialize_confidential_fee_ix =
            confidential_fee_instruction::
                initialize_confidential_transfer_fee_config(
                    &ctx.accounts.token_program.key(),
                    &ctx.accounts.mint.key(),
                    Some(ctx.accounts.payer.key()),
                    &withdraw_withheld_authority_elgamal_pubkey.into(),
                )?;

        invoke(
            &initialize_confidential_fee_ix,
            &invoke_accounts,
        )?;

        // Base mint MUST be initialized after all extensions.
        initialize_mint2(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                InitializeMint2 {
                    mint: mint_info,
                },
            ),
            decimals,
            &ctx.accounts.payer.key(),
            None,
        )?;

        msg!(
            "confidential fee mint {} created with {} bytes",
            ctx.accounts.mint.key(),
            space
        );

        Ok(())
    }

    /// Deposit public tokens into the pending confidential balance.
    pub fn deposit_confidential(
        ctx: Context<DepositConfidential>,
        amount: u64,
        decimals: u8,
    ) -> Result<()> {
        let ix = confidential_instruction::deposit(
            &ctx.accounts.token_program.key(),
            &ctx.accounts.token_account.key(),
            &ctx.accounts.mint.key(),
            amount,
            decimals,
            &ctx.accounts.authority.key(),
            &[],
        )?;

        invoke(
            &ix,
            &[
                ctx.accounts.token_account.to_account_info(),
                ctx.accounts.mint.to_account_info(),
                ctx.accounts.authority.to_account_info(),
                ctx.accounts.token_program.to_account_info(),
            ],
        )?;

        Ok(())
    }

    /// Move pending confidential balance into the available balance.
    pub fn apply_pending_balance(
        ctx: Context<ApplyPendingBalance>,
        expected_pending_balance_credit_counter: u64,
        new_decryptable_available_balance: [u8; AE_CIPHERTEXT_LEN],
    ) -> Result<()> {
        let decryptable_balance =
            DecryptableBalance::from(new_decryptable_available_balance);

        let ix = confidential_instruction::apply_pending_balance(
            &ctx.accounts.token_program.key(),
            &ctx.accounts.token_account.key(),
            expected_pending_balance_credit_counter,
            &decryptable_balance,
            &ctx.accounts.authority.key(),
            &[],
        )?;

        invoke(
            &ix,
            &[
                ctx.accounts.token_account.to_account_info(),
                ctx.accounts.authority.to_account_info(),
                ctx.accounts.token_program.to_account_info(),
            ],
        )?;

        Ok(())
    }

    /// Create a mint with a permanent delegate.
    pub fn create_seizable_mint(
        ctx: Context<CreateSeizableMint>,
        decimals: u8,
    ) -> Result<()> {
        msg!(
            "seizable mint {} created with {} decimals; permanent delegate = {}",
            ctx.accounts.mint.key(),
            decimals,
            ctx.accounts.payer.key()
        );

        Ok(())
    }

    /// Perform Token-2022 Approve through CPI.
    ///
    /// A token account with CPI Guard enabled should reject this CPI.
    pub fn delegate_to_program(
        ctx: Context<DelegateToProgram>,
        amount: u64,
    ) -> Result<()> {
        approve(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Approve {
                    to: ctx.accounts.token_account.to_account_info(),
                    delegate: ctx.accounts.delegate.to_account_info(),
                    authority: ctx.accounts.owner.to_account_info(),
                },
            ),
            amount,
        )?;

        msg!(
            "delegated {} to {}",
            amount,
            ctx.accounts.delegate.key()
        );

        Ok(())
    }

    /// Transfer tokens using the mint's permanent delegate.
    pub fn permanent_delegate_seize(
        ctx: Context<PermanentDelegateSeize>,
        amount: u64,
        decimals: u8,
    ) -> Result<()> {
        transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                TransferChecked {
                    from: ctx.accounts.source.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    to: ctx.accounts.destination.to_account_info(),
                    authority: ctx.accounts.permanent_delegate.to_account_info(),
                },
            ),
            amount,
            decimals,
        )?;

        msg!(
            "permanent delegate moved {} tokens",
            amount
        );

        Ok(())
    }

    /// Verify that the mint contains only supported extensions.
    pub fn assert_supported_mint(
        ctx: Context<AssertSupportedMint>,
    ) -> Result<()> {
        let mint_info = ctx.accounts.mint.to_account_info();

        let data = mint_info.try_borrow_data()?;

        let state =
            StateWithExtensions::<MintState>::unpack(&data)?;

        for extension in state.get_extension_types()? {
            require!(
                SUPPORTED_EXTENSIONS.contains(&extension),
                MintError::UnsupportedExtension
            );
        }

        let basis_points =
            match state.get_extension::<TransferFeeConfig>() {
                Ok(config) => {
                    u16::from(
                        config
                            .get_epoch_fee(Clock::get()?.epoch)
                            .transfer_fee_basis_points,
                    )
                }
                Err(_) => 0,
            };

        msg!(
            "mint accepted: {} decimals, {} bps fee",
            state.base.decimals,
            basis_points
        );

        Ok(())
    }
}

// ============================================================
// ACCOUNTS
// ============================================================

#[derive(Accounts)]
#[instruction(decimals: u8)]
pub struct CreateMintDeclarative<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        init,
        payer = payer,
        mint::decimals = decimals,
        mint::authority = payer,
        mint::token_program = token_program,
        extensions::close_authority::authority = payer,
        extensions::metadata_pointer::authority = payer,
        extensions::metadata_pointer::metadata_address = payer,
    )]
    pub mint: InterfaceAccount<'info, Mint>,

    pub token_program: Interface<'info, TokenInterface>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CreateMintWithFee<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK:
    /// This account is created and initialized inside the instruction.
    /// It must sign because System Program creates the account at this key.
    #[account(mut, signer)]
    pub mint: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AssertSupportedMint<'info> {
    /// CHECK:
    /// Ownership is checked here and the Token-2022 mint data is
    /// validated manually inside the instruction.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct CreateConfidentialMint<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK:
    /// Created and initialized manually in the instruction.
    #[account(mut, signer)]
    pub mint: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CreateConfidentialFeeMint<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK:
    /// Created and initialized manually in the instruction.
    #[account(mut, signer)]
    pub mint: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct DepositConfidential<'info> {
    /// CHECK:
    /// Token-2022 validates this token account and its confidential
    /// transfer configuration.
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    /// CHECK:
    /// Token-2022 validates this mint during the deposit.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    pub authority: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct ApplyPendingBalance<'info> {
    /// CHECK:
    /// Token-2022 validates the token account and authority.
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    pub authority: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
#[instruction(decimals: u8)]
pub struct CreateSeizableMint<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        init,
        payer = payer,
        mint::decimals = decimals,
        mint::authority = payer,
        mint::token_program = token_program,
        extensions::permanent_delegate::delegate = payer,
    )]
    pub mint: InterfaceAccount<'info, Mint>,

    pub token_program: Interface<'info, TokenInterface>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct DelegateToProgram<'info> {
    /// CHECK:
    /// Token-2022 validates this account during Approve.
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    /// CHECK:
    /// Any public key may be stored as the delegate.
    pub delegate: UncheckedAccount<'info>,

    pub owner: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct PermanentDelegateSeize<'info> {
    /// CHECK:
    /// Token-2022 validates the source during TransferChecked.
    #[account(mut, owner = token_program.key())]
    pub source: UncheckedAccount<'info>,

    /// CHECK:
    /// Token-2022 validates this mint during TransferChecked.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    /// CHECK:
    /// Token-2022 validates the destination during TransferChecked.
    #[account(mut, owner = token_program.key())]
    pub destination: UncheckedAccount<'info>,

    /// The signer must match the mint's PermanentDelegate extension.
    pub permanent_delegate: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

// ============================================================
// ERRORS
// ============================================================

#[error_code]
pub enum MintError {
    #[msg(
        "mint carries an extension this program has not been written to handle"
    )]
    UnsupportedExtension,
}