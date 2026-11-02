# 1. Install dependencies
npm install -g @coral-xyz/anchor-cli @solana/web3.js @solana/spl-token
cargo install --git https://github.com/coral-xyz/anchor avm --locked --force

# 2. Build smart contract binary
anchor build

# 3. Synchronize program keys
solana address -k target/deploy/us_rwa_transfer_hook-keypair.json
# Replace declare_id!() in lib.rs with output if fresh key generated

# 4. Deploy to Solana Devnet
solana config set --url https://api.devnet.solana.com
anchor deploy --provider.cluster devnet

# 5. Run end-to-end deterministic integration test suite
anchor test