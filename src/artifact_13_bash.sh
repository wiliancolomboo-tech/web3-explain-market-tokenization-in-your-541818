# Airdrop testnet SOL
solana airdrop 2 -u devnet

# Deploy to Devnet
anchor deploy --provider.cluster devnet

# Verify deployment logs and inspect program accounts
solana program show <PROGRAM_ID> -u devnet