use crate::error::AppError;
use crate::helper::curve::Curve;
use crate::instruction::AppInstruction;
use crate::interfaces::isplt::ISPLT;
use crate::schema::{
  lpt::LPT,
  network::{Network, NetworkState},
  pool::Pool,
};
use solana_program::{
  account_info::{next_account_info, AccountInfo},
  entrypoint::ProgramResult,
  info,
  program::{invoke, invoke_signed},
  program_pack::{IsInitialized, Pack},
  pubkey::Pubkey,
};

///
/// fee = 2500000/1000000000 = 0.25%
/// earn = 500000/1000000000 = 0.05%
///
const FEE: u64 = 2500000;
const EARN: u64 = 500000;
const FEE_DECIMALS: u64 = 1000000000;

pub struct Processor {}

impl Processor {
  pub fn process(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
  ) -> ProgramResult {
    let instruction = AppInstruction::unpack(instruction_data)?;
    match instruction {
      AppInstruction::InitializeNetwork {} => {
        info!("Calling InitializeNetwork function");
        let accounts_iter = &mut accounts.iter();
        let network_acc = next_account_info(accounts_iter)?;
        if network_acc.owner != program_id {
          return Err(AppError::IncorrectProgramId.into());
        }

        let mut network_data = Network::unpack_unchecked(&network_acc.data.borrow())?;
        if network_data.is_initialized() {
          return Err(AppError::ConstructorOnce.into());
        }
        if !network_acc.is_signer {
          return Err(AppError::InvalidOwner.into());
        }

        network_data.state = NetworkState::Initialized;
        network_data.mints[0] = Network::primary();
        for i in 1..Network::max_mints() {
          let mint_acc = next_account_info(accounts_iter)?;
          network_data.mints[i] = *mint_acc.key;
        }
        Network::pack(network_data, &mut network_acc.data.borrow_mut())?;

        Ok(())
      }

      AppInstruction::InitializePool { reserve, lpt } => {
        info!("Calling InitializePool function");
        let accounts_iter = &mut accounts.iter();
        let owner = next_account_info(accounts_iter)?;
        let network_acc = next_account_info(accounts_iter)?;
        let pool_acc = next_account_info(accounts_iter)?;
        let treasury_acc = next_account_info(accounts_iter)?;
        let lpt_acc = next_account_info(accounts_iter)?;
        let src_acc = next_account_info(accounts_iter)?;
        let mint_acc = next_account_info(accounts_iter)?;
        let treasurer = next_account_info(accounts_iter)?;
        let splt_program = next_account_info(accounts_iter)?;
        let sysvar_rent_acc = next_account_info(accounts_iter)?;
        if network_acc.owner != program_id
          || pool_acc.owner != program_id
          || lpt_acc.owner != program_id
        {
          return Err(AppError::IncorrectProgramId.into());
        }

        let mut network_data = Network::unpack(&network_acc.data.borrow())?;
        let mut pool_data = Pool::unpack_unchecked(&pool_acc.data.borrow())?;
        let mut lpt_data = LPT::unpack_unchecked(&lpt_acc.data.borrow())?;
        if pool_data.is_initialized() || lpt_data.is_initialized() {
          return Err(AppError::ConstructorOnce.into());
        }
        let seed: &[&[_]] = &[&pool_acc.key.to_bytes()[..]];
        let treasurer_key = Pubkey::create_program_address(&seed, program_id)?;
        if !owner.is_signer
          || !pool_acc.is_signer
          || !lpt_acc.is_signer
          || treasurer_key != *treasurer.key
        {
          return Err(AppError::InvalidOwner.into());
        }
        if !network_data.is_approved(mint_acc.key) {
          return Err(AppError::UnmatchedPool.into());
        }
        if *mint_acc.key != Network::primary() && !network_data.is_activated() {
          return Err(AppError::NotInitialized.into());
        }
        if *mint_acc.key == Network::primary() && network_data.is_activated() {
          return Err(AppError::ConstructorOnce.into());
        }
        if reserve == 0 || lpt == 0 {
          return Err(AppError::ZeroValue.into());
        }

        // Account Constructor
        let ix_initialize_account = ISPLT::initialize_account(
          *treasury_acc.key,
          *mint_acc.key,
          *treasurer.key,
          *sysvar_rent_acc.key,
          *splt_program.key,
        )?;
        invoke_signed(
          &ix_initialize_account,
          &[
            treasury_acc.clone(),
            mint_acc.clone(),
            treasurer.clone(),
            sysvar_rent_acc.clone(),
            splt_program.clone(),
          ],
          &[&seed],
        )?;

        // Deposit token
        let ix_transfer = ISPLT::transfer(
          reserve,
          *src_acc.key,
          *treasury_acc.key,
          *owner.key,
          *splt_program.key,
        )?;
        invoke(
          &ix_transfer,
          &[
            src_acc.clone(),
            treasury_acc.clone(),
            owner.clone(),
            splt_program.clone(),
          ],
        )?;

        // Update network data
        if *mint_acc.key == Network::primary() {
          network_data.state = NetworkState::Activated;
          Network::pack(network_data, &mut network_acc.data.borrow_mut())?;
        }
        // Update pool data
        pool_data.owner = *owner.key;
        pool_data.network = *network_acc.key;
        pool_data.mint = *mint_acc.key;
        pool_data.treasury = *treasury_acc.key;
        pool_data.reserve = reserve;
        pool_data.lpt = lpt;
        pool_data.fee = FEE;
        pool_data.is_initialized = true;
        Pool::pack(pool_data, &mut pool_acc.data.borrow_mut())?;
        // Update lpt data
        lpt_data.owner = *owner.key;
        lpt_data.pool = *pool_acc.key;
        lpt_data.lpt = lpt;
        lpt_data.is_initialized = true;
        LPT::pack(lpt_data, &mut lpt_acc.data.borrow_mut())?;

        Ok(())
      }

      AppInstruction::InitializeLPT {} => {
        info!("Calling InitializeLPTfunction");
        let accounts_iter = &mut accounts.iter();
        let owner = next_account_info(accounts_iter)?;
        let pool_acc = next_account_info(accounts_iter)?;
        let lpt_acc = next_account_info(accounts_iter)?;
        if pool_acc.owner != program_id || lpt_acc.owner != program_id {
          return Err(AppError::IncorrectProgramId.into());
        }

        let mut lpt_data = LPT::unpack_unchecked(&lpt_acc.data.borrow())?;
        if lpt_data.is_initialized() {
          return Err(AppError::ConstructorOnce.into());
        }
        if !owner.is_signer || !lpt_acc.is_signer {
          return Err(AppError::InvalidOwner.into());
        }

        lpt_data.owner = *owner.key;
        lpt_data.pool = *pool_acc.key;
        lpt_data.lpt = 0;
        lpt_data.is_initialized = true;
        LPT::pack(lpt_data, &mut lpt_acc.data.borrow_mut())?;

        Ok(())
      }

      AppInstruction::AddLiquidity { reserve } => {
        info!("Calling AddLiquidity function");
        let accounts_iter = &mut accounts.iter();
        let owner = next_account_info(accounts_iter)?;
        let pool_acc = next_account_info(accounts_iter)?;
        let treasury_acc = next_account_info(accounts_iter)?;
        let lpt_acc = next_account_info(accounts_iter)?;
        let src_acc = next_account_info(accounts_iter)?;
        let splt_program = next_account_info(accounts_iter)?;
        if pool_acc.owner != program_id || lpt_acc.owner != program_id {
          return Err(AppError::IncorrectProgramId.into());
        }

        let mut pool_data = Pool::unpack(&pool_acc.data.borrow())?;
        let mut lpt_data = LPT::unpack(&lpt_acc.data.borrow())?;
        if !owner.is_signer
          || pool_data.treasury != *treasury_acc.key
          || lpt_data.owner != *owner.key
        {
          return Err(AppError::InvalidOwner.into());
        }
        if lpt_data.pool != *pool_acc.key {
          return Err(AppError::UnmatchedPool.into());
        }
        if reserve == 0 {
          return Err(AppError::ZeroValue.into());
        }

        // Deposit token
        let ix_transfer = ISPLT::transfer(
          reserve,
          *src_acc.key,
          *treasury_acc.key,
          *owner.key,
          *splt_program.key,
        )?;
        invoke(
          &ix_transfer,
          &[
            src_acc.clone(),
            treasury_acc.clone(),
            owner.clone(),
            splt_program.clone(),
          ],
        )?;

        // Compute corresponding paid-back lpt
        let paid_lpt = (pool_data.lpt)
          .checked_mul(reserve as u128)
          .ok_or(AppError::Overflow)?
          .checked_div(pool_data.reserve as u128)
          .ok_or(AppError::Overflow)?;
        // Update pool
        pool_data.reserve = pool_data
          .reserve
          .checked_add(reserve)
          .ok_or(AppError::Overflow)?;
        pool_data.lpt = pool_data
          .lpt
          .checked_add(paid_lpt)
          .ok_or(AppError::Overflow)?;
        Pool::pack(pool_data, &mut pool_acc.data.borrow_mut())?;
        // Update lpt data
        lpt_data.lpt = lpt_data
          .lpt
          .checked_add(paid_lpt)
          .ok_or(AppError::Overflow)?;
        LPT::pack(lpt_data, &mut lpt_acc.data.borrow_mut())?;

        Ok(())
      }

      AppInstruction::RemoveLiquidity { lpt } => {
        info!("Calling RemoveLiquidity function");
        let accounts_iter = &mut accounts.iter();
        let owner = next_account_info(accounts_iter)?;
        let pool_acc = next_account_info(accounts_iter)?;
        let treasury_acc = next_account_info(accounts_iter)?;
        let lpt_acc = next_account_info(accounts_iter)?;
        let dst_acc = next_account_info(accounts_iter)?;
        let treasurer = next_account_info(accounts_iter)?;
        let splt_program = next_account_info(accounts_iter)?;
        if pool_acc.owner != program_id || lpt_acc.owner != program_id {
          return Err(AppError::IncorrectProgramId.into());
        }

        let mut pool_data = Pool::unpack(&pool_acc.data.borrow())?;
        let mut lpt_data = LPT::unpack(&lpt_acc.data.borrow())?;
        let seed: &[&[_]] = &[&pool_acc.key.to_bytes()[..]];
        let treasurer_key = Pubkey::create_program_address(&seed, program_id)?;
        if !owner.is_signer
          || pool_data.treasury != *treasury_acc.key
          || lpt_data.owner != *owner.key
          || treasurer_key != *treasurer.key
        {
          return Err(AppError::InvalidOwner.into());
        }
        if lpt_data.pool != *pool_acc.key {
          return Err(AppError::UnmatchedPool.into());
        }
        if lpt == 0 {
          return Err(AppError::ZeroValue.into());
        }
        if lpt_data.lpt < lpt {
          return Err(AppError::InsufficientFunds.into());
        }

        // Compute corresponding paid-back reserve
        let paid_reserve = (pool_data.reserve as u128)
          .checked_mul(lpt)
          .ok_or(AppError::Overflow)?
          .checked_div(pool_data.lpt)
          .ok_or(AppError::Overflow)? as u64;

        // Update lpt data
        lpt_data.lpt = lpt_data.lpt.checked_sub(lpt).ok_or(AppError::Overflow)?;
        LPT::pack(lpt_data, &mut lpt_acc.data.borrow_mut())?;
        // Update pool
        pool_data.reserve = pool_data
          .reserve
          .checked_sub(paid_reserve)
          .ok_or(AppError::Overflow)?;
        pool_data.lpt = pool_data.lpt.checked_sub(lpt).ok_or(AppError::Overflow)?;
        Pool::pack(pool_data, &mut pool_acc.data.borrow_mut())?;

        // Withdraw token
        let ix_transfer = ISPLT::transfer(
          paid_reserve,
          *treasury_acc.key,
          *dst_acc.key,
          *treasurer.key,
          *splt_program.key,
        )?;
        invoke_signed(
          &ix_transfer,
          &[
            treasury_acc.clone(),
            dst_acc.clone(),
            treasurer.clone(),
            splt_program.clone(),
          ],
          &[&seed],
        )?;

        Ok(())
      }

      AppInstruction::Swap { amount } => {
        info!("Calling Swap function");
        let accounts_iter = &mut accounts.iter();
        let owner = next_account_info(accounts_iter)?;

        let bid_pool_acc = next_account_info(accounts_iter)?;
        let bid_treasury_acc = next_account_info(accounts_iter)?;
        let src_acc = next_account_info(accounts_iter)?;

        let ask_pool_acc = next_account_info(accounts_iter)?;
        let ask_treasury_acc = next_account_info(accounts_iter)?;
        let dst_acc = next_account_info(accounts_iter)?;
        let ask_treasurer = next_account_info(accounts_iter)?;

        let sen_pool_acc = next_account_info(accounts_iter)?;
        let sen_treasury_acc = next_account_info(accounts_iter)?;
        let vault_acc = next_account_info(accounts_iter)?;
        let sen_treasurer = next_account_info(accounts_iter)?;

        let splt_program = next_account_info(accounts_iter)?;
        if bid_pool_acc.owner != program_id
          || ask_pool_acc.owner != program_id
          || sen_pool_acc.owner != program_id
        {
          return Err(AppError::IncorrectProgramId.into());
        }

        let mut bid_pool_data = Pool::unpack(&bid_pool_acc.data.borrow())?;
        let mut ask_pool_data = Pool::unpack(&ask_pool_acc.data.borrow())?;
        let mut sen_pool_data = Pool::unpack(&sen_pool_acc.data.borrow())?;
        let ask_seed: &[&[_]] = &[&ask_pool_acc.key.to_bytes()[..]];
        let ask_treasurer_key = Pubkey::create_program_address(&ask_seed, program_id)?;
        let sen_seed: &[&[_]] = &[&sen_pool_acc.key.to_bytes()[..]];
        let sen_treasurer_key = Pubkey::create_program_address(&sen_seed, program_id)?;
        if !owner.is_signer
          || bid_pool_data.treasury != *bid_treasury_acc.key
          || ask_pool_data.treasury != *ask_treasury_acc.key
          || ask_treasurer_key != *ask_treasurer.key
          || sen_pool_data.treasury != *sen_treasury_acc.key
          || sen_treasurer_key != *sen_treasurer.key
        {
          return Err(AppError::InvalidOwner.into());
        }
        if sen_pool_data.network != bid_pool_data.network
          || sen_pool_data.network != ask_pool_data.network
        {
          return Err(AppError::IncorrectNetworkId.into());
        }
        if amount == 0 {
          return Err(AppError::ZeroValue.into());
        }
        if *bid_pool_acc.key == *ask_pool_acc.key {
          return Ok(());
        }

        // Compute new state
        let new_bid_reserve = bid_pool_data
          .reserve
          .checked_add(amount)
          .ok_or(AppError::Overflow)?;
        let new_ask_reserve_without_fee = Curve::curve(
          new_bid_reserve,
          bid_pool_data.reserve,
          bid_pool_data.lpt,
          ask_pool_data.reserve,
          ask_pool_data.lpt,
        )
        .ok_or(AppError::Overflow)?;

        // Transfer bid
        let ix_transfer = ISPLT::transfer(
          amount,
          *src_acc.key,
          *bid_treasury_acc.key,
          *owner.key,
          *splt_program.key,
        )?;
        invoke(
          &ix_transfer,
          &[
            src_acc.clone(),
            bid_treasury_acc.clone(),
            owner.clone(),
            splt_program.clone(),
          ],
        )?;
        bid_pool_data.reserve = new_bid_reserve;
        Pool::pack(bid_pool_data, &mut bid_pool_acc.data.borrow_mut())?;

        // Apply fee
        let is_primary = ask_pool_data.mint == Network::primary();
        let (new_ask_reserve_with_fee, paid_amount, _, earn) = Self::apply_fee(
          new_ask_reserve_without_fee,
          ask_pool_data.reserve,
          is_primary,
        )
        .ok_or(AppError::Overflow)?;

        // Transfer ask
        let new_ask_reserve = new_ask_reserve_with_fee
          .checked_add(earn)
          .ok_or(AppError::Overflow)?;
        ask_pool_data.reserve = new_ask_reserve;
        Pool::pack(ask_pool_data, &mut ask_pool_acc.data.borrow_mut())?;
        let ix_transfer = ISPLT::transfer(
          paid_amount,
          *ask_treasury_acc.key,
          *dst_acc.key,
          *ask_treasurer.key,
          *splt_program.key,
        )?;
        invoke_signed(
          &ix_transfer,
          &[
            ask_treasury_acc.clone(),
            dst_acc.clone(),
            ask_treasurer.clone(),
            splt_program.clone(),
          ],
          &[&ask_seed],
        )?;

        // Transfer earn
        if earn != 0 {
          let earn_in_sen = Curve::curve(
            new_ask_reserve,
            new_ask_reserve_with_fee,
            ask_pool_data.lpt,
            sen_pool_data.reserve,
            sen_pool_data.lpt,
          )
          .ok_or(AppError::Overflow)?;
          sen_pool_data.reserve = sen_pool_data
            .reserve
            .checked_sub(earn_in_sen)
            .ok_or(AppError::Overflow)?;
          Pool::pack(sen_pool_data, &mut sen_pool_acc.data.borrow_mut())?;
          let ix_transfer = ISPLT::transfer(
            earn_in_sen,
            *sen_treasury_acc.key,
            *vault_acc.key,
            *sen_treasurer.key,
            *splt_program.key,
          )?;
          invoke_signed(
            &ix_transfer,
            &[
              sen_treasury_acc.clone(),
              vault_acc.clone(),
              sen_treasurer.clone(),
              splt_program.clone(),
            ],
            &[&sen_seed],
          )?;
        }

        Ok(())
      }

      AppInstruction::Transfer { lpt } => {
        let accounts_iter = &mut accounts.iter();
        let owner = next_account_info(accounts_iter)?;
        let src_lpt_acc = next_account_info(accounts_iter)?;
        let dst_lpt_acc = next_account_info(accounts_iter)?;
        if src_lpt_acc.owner != program_id || dst_lpt_acc.owner != program_id {
          return Err(AppError::IncorrectProgramId.into());
        }

        let mut src_lpt_data = LPT::unpack(&src_lpt_acc.data.borrow())?;
        let mut dst_lpt_data = LPT::unpack(&dst_lpt_acc.data.borrow())?;
        if !owner.is_signer || src_lpt_data.owner != *owner.key {
          return Err(AppError::InvalidOwner.into());
        }
        if src_lpt_data.pool != dst_lpt_data.pool {
          return Err(AppError::UnmatchedPool.into());
        }
        if lpt == 0 {
          return Err(AppError::ZeroValue.into());
        }
        if src_lpt_data.lpt < lpt {
          return Err(AppError::InsufficientFunds.into());
        }
        if *src_lpt_acc.key == *dst_lpt_acc.key {
          return Ok(());
        }

        // Update lpt data
        src_lpt_data.lpt = src_lpt_data
          .lpt
          .checked_sub(lpt)
          .ok_or(AppError::Overflow)?;
        LPT::pack(src_lpt_data, &mut src_lpt_acc.data.borrow_mut())?;
        dst_lpt_data.lpt = dst_lpt_data
          .lpt
          .checked_add(lpt)
          .ok_or(AppError::Overflow)?;
        LPT::pack(dst_lpt_data, &mut dst_lpt_acc.data.borrow_mut())?;

        Ok(())
      }

      AppInstruction::CloseLPT {} => {
        info!("Calling CloseLPT function");
        let accounts_iter = &mut accounts.iter();
        let owner = next_account_info(accounts_iter)?;
        let lpt_acc = next_account_info(accounts_iter)?;
        let dst_acc = next_account_info(accounts_iter)?;
        if lpt_acc.owner != program_id {
          return Err(AppError::IncorrectProgramId.into());
        }

        let lpt_data = LPT::unpack(&lpt_acc.data.borrow())?;
        if !owner.is_signer || lpt_data.owner != *owner.key {
          return Err(AppError::InvalidOwner.into());
        }
        if lpt_data.lpt != 0 {
          return Err(AppError::ZeroValue.into());
        }

        let lpt_lamports = lpt_acc.lamports();
        **dst_acc.lamports.borrow_mut() = lpt_lamports
          .checked_add(dst_acc.lamports())
          .ok_or(AppError::Overflow)?;
        **lpt_acc.lamports.borrow_mut() = 0;

        Ok(())
      }

      AppInstruction::ClosePool {} => {
        info!("Calling ClosePool function");
        let accounts_iter = &mut accounts.iter();
        let owner = next_account_info(accounts_iter)?;
        let pool_acc = next_account_info(accounts_iter)?;
        let treasury_acc = next_account_info(accounts_iter)?;
        let dst_acc = next_account_info(accounts_iter)?;
        let treasurer = next_account_info(accounts_iter)?;
        let splt_program = next_account_info(accounts_iter)?;
        if pool_acc.owner != program_id {
          return Err(AppError::IncorrectProgramId.into());
        }

        let pool_data = Pool::unpack(&pool_acc.data.borrow())?;
        let seed: &[&[_]] = &[&pool_acc.key.to_bytes()[..]];
        let treasurer_key = Pubkey::create_program_address(&seed, program_id)?;
        if !owner.is_signer
          || pool_data.owner != *owner.key
          || pool_data.treasury != *treasury_acc.key
          || treasurer_key != *treasurer.key
        {
          return Err(AppError::InvalidOwner.into());
        }
        if pool_data.lpt != 0 || pool_data.reserve != 0 {
          return Err(AppError::ZeroValue.into());
        }

        // Close treasury
        let ix_close_account = ISPLT::close_account(
          *treasury_acc.key,
          *dst_acc.key,
          *treasurer.key,
          *splt_program.key,
        )?;
        invoke_signed(
          &ix_close_account,
          &[
            treasury_acc.clone(),
            dst_acc.clone(),
            treasurer.clone(),
            splt_program.clone(),
          ],
          &[&seed],
        )?;
        // Close pool
        let dst_lamports = dst_acc.lamports();
        **dst_acc.lamports.borrow_mut() = dst_lamports
          .checked_add(pool_acc.lamports())
          .ok_or(AppError::Overflow)?;
        **pool_acc.lamports.borrow_mut() = 0;

        Ok(())
      }
    }
  }

  fn apply_fee(
    new_ask_reserve: u64,
    ask_reserve: u64,
    is_primary: bool,
  ) -> Option<(u64, u64, u64, u64)> {
    let paid_amount_without_fee = ask_reserve.checked_sub(new_ask_reserve)?;
    let fee = (paid_amount_without_fee as u128)
      .checked_mul(FEE as u128)?
      .checked_div(FEE_DECIMALS as u128)? as u64;
    let mut earn = (paid_amount_without_fee as u128)
      .checked_mul(EARN as u128)?
      .checked_div(FEE_DECIMALS as u128)? as u64;
    if is_primary {
      earn = 0;
    }
    let new_ask_reserve_with_fee = new_ask_reserve.checked_add(fee)?;
    let paid_amount_with_fee = paid_amount_without_fee
      .checked_sub(fee)?
      .checked_sub(earn)?;
    Some((new_ask_reserve_with_fee, paid_amount_with_fee, fee, earn))
  }
}
