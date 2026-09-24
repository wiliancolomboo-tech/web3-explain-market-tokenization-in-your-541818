# Step 1: Clone and install dependencies
git clone https://github.com/sovereign-rwa/solana-market-tokenization.git
cd solana-market-tokenization
yarn install

# Step 2: Build the Anchor program to generate keypairs and IDL
anchor build

# Step 3: Synchronize Program IDs
# Verify that the declare_id! matches your compiled program keypair:
solana address -k target/deploy/sovereign_rwa-keypair.json
anchor keys sync

# Step 4: Configure local validator with Token-2022 support
solana config set --url localhost

# Step 5: Run the complete test suite against localnet
anchor test