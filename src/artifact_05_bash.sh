# 1. Clone repository and install dependencies
git clone https://github.com/soltokenize-sg/soltokenize-core.git
cd soltokenize-core
npm install
anchor build

# 2. Configure Solana devnet/mainnet cluster
solana config set --url https://api.devnet.solana.com
solana airdrop 2

# 3. Deploy Anchor Program
anchor deploy --program-name soltokenize_sg

# 4. Run full deterministic test suite
anchor test --skip-local-validator