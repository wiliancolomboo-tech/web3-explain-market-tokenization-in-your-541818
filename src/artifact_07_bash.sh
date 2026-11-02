# 1. Install Solana Tool suite and Anchor
sh -c "$(curl -sSfL https://release.solana.com/v1.18.26/install)"
cargo install --git https://github.com/coral-xyz/anchor avm --locked --force
avm install 0.30.1
avm use 0.30.1

# 2. Configure Local or Devnet Environment
solana config set --url https://api.devnet.solana.com
solana-keygen new --outfile ~/.config/solana/id.json --no-bip39-passphrase --force

# 3. Build & Test Program
anchor build
anchor test

# 4. Deploy Program to Solana Network
anchor deploy --provider.cluster devnet