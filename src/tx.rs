use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use solana_client::rpc_client::RpcClient;
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_sdk::{
    hash::Hash,
    signature::{read_keypair_file, Keypair, Signature},
    signer::Signer,
    transaction::Transaction,
};
use solana_system_interface::instruction as system_instruction;
use std::path::Path;

const BASE_SIGNATURE_FEE_LAMPORTS: u64 = 5_000;

#[derive(Debug, Clone)]
pub struct TxBuildConfig {
    pub rpc_url: String,
    pub lamports: u64,
    pub compute_unit_limit: u32,
    pub compute_unit_price_microlamports: u64,
}

#[derive(Debug, Clone)]
pub struct BenchTx {
    pub iteration: usize,
    pub signature: Signature,
    pub raw: Vec<u8>,
    pub base64: String,
    pub estimated_spend_lamports: u64,
}

pub fn load_keypair(path: &Path) -> Result<Keypair> {
    read_keypair_file(path).map_err(|err| anyhow::anyhow!("read keypair {}: {err}", path.display()))
}

pub fn estimate_spend_lamports(
    compute_unit_limit: u32,
    compute_unit_price_microlamports: u64,
    route_tip_lamports: u64,
) -> u64 {
    let priority_fee = (compute_unit_limit as u64)
        .saturating_mul(compute_unit_price_microlamports)
        .saturating_add(999_999)
        / 1_000_000;
    BASE_SIGNATURE_FEE_LAMPORTS
        .saturating_add(priority_fee)
        .saturating_add(route_tip_lamports)
}

pub fn build_transactions(
    config: &TxBuildConfig,
    payer: &Keypair,
    count: usize,
    max_spend_lamports: Option<u64>,
) -> Result<Vec<BenchTx>> {
    let rpc = RpcClient::new(config.rpc_url.clone());
    let blockhash = rpc
        .get_latest_blockhash()
        .context("fetch latest blockhash")?;
    build_transactions_with_blockhash(config, payer, count, max_spend_lamports, blockhash)
}

pub fn build_transactions_with_blockhash(
    config: &TxBuildConfig,
    payer: &Keypair,
    count: usize,
    max_spend_lamports: Option<u64>,
    blockhash: Hash,
) -> Result<Vec<BenchTx>> {
    let mut out = Vec::with_capacity(count);
    let mut estimated_total = 0_u64;
    for iteration in 0..count {
        let estimated_spend = estimated_transaction_spend(config);
        if let Some(max) = max_spend_lamports {
            if estimated_total.saturating_add(estimated_spend) > max {
                bail!(
                    "spend cap exceeded before tx {iteration}: estimated_next_total={} max={max}",
                    estimated_total.saturating_add(estimated_spend)
                );
            }
        }
        out.push(build_transaction_with_blockhash(
            config, payer, iteration, blockhash,
        )?);
        estimated_total = estimated_total.saturating_add(estimated_spend);
    }
    Ok(out)
}

pub fn build_transaction_with_blockhash(
    config: &TxBuildConfig,
    payer: &Keypair,
    iteration: usize,
    blockhash: Hash,
) -> Result<BenchTx> {
    let transfer_lamports = config
        .lamports
        .checked_add(iteration as u64)
        .with_context(|| format!("transfer lamports overflow at iteration {iteration}"))?;
    let mut instructions = Vec::with_capacity(3);
    instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(
        config.compute_unit_limit,
    ));
    if config.compute_unit_price_microlamports > 0 {
        instructions.push(ComputeBudgetInstruction::set_compute_unit_price(
            config.compute_unit_price_microlamports,
        ));
    }
    instructions.push(system_instruction::transfer(
        &payer.pubkey(),
        &payer.pubkey(),
        transfer_lamports,
    ));
    let tx = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[payer],
        blockhash,
    );
    let raw = bincode::serialize(&tx).context("serialize transaction")?;
    let base64 = STANDARD.encode(&raw);
    Ok(BenchTx {
        iteration,
        signature: tx.signatures[0],
        raw,
        base64,
        estimated_spend_lamports: estimated_transaction_spend(config),
    })
}

pub fn estimated_transaction_spend(config: &TxBuildConfig) -> u64 {
    estimate_spend_lamports(
        config.compute_unit_limit,
        config.compute_unit_price_microlamports,
        0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::{hash::Hash, pubkey::Pubkey, transaction::Transaction};
    use std::collections::HashSet;
    use std::str::FromStr;

    #[test]
    fn generated_signatures_are_unique_with_stable_compute_budget() {
        let payer = Keypair::new();
        let config = TxBuildConfig {
            rpc_url: "http://127.0.0.1:8899".to_string(),
            lamports: 1,
            compute_unit_limit: 20_000,
            compute_unit_price_microlamports: 0,
        };
        let txs = build_transactions_with_blockhash(&config, &payer, 64, None, Hash::new_unique())
            .expect("build txs");
        let unique = txs.iter().map(|tx| tx.signature).collect::<HashSet<_>>();
        assert_eq!(unique.len(), txs.len());
    }

    #[test]
    fn generated_transactions_do_not_include_memo_instruction() {
        let payer = Keypair::new();
        let config = TxBuildConfig {
            rpc_url: "http://127.0.0.1:8899".to_string(),
            lamports: 500,
            compute_unit_limit: 20_000,
            compute_unit_price_microlamports: 100,
        };
        let built = build_transaction_with_blockhash(&config, &payer, 7, Hash::new_unique())
            .expect("build tx");
        let tx: Transaction = bincode::deserialize(&built.raw).expect("deserialize tx");
        let memo_program = Pubkey::from_str("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr").unwrap();

        assert!(!tx.message.account_keys.contains(&memo_program));
        assert_eq!(tx.message.instructions.len(), 3);
    }

    #[test]
    fn spend_cap_rejects_before_building_extra_transaction() {
        let payer = Keypair::new();
        let config = TxBuildConfig {
            rpc_url: "http://127.0.0.1:8899".to_string(),
            lamports: 1,
            compute_unit_limit: 20_000,
            compute_unit_price_microlamports: 500_000,
        };
        let one_tx = estimate_spend_lamports(20_000, 500_000, 0);
        let err =
            build_transactions_with_blockhash(&config, &payer, 2, Some(one_tx), Hash::new_unique())
                .expect_err("cap should reject second tx");
        assert!(err.to_string().contains("spend cap exceeded"));
    }
}
