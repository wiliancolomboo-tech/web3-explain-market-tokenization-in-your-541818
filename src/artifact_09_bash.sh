# 1. Build Solana Program
anchor build

# 2. Deploy to Devnet / Mainnet-Beta
solana program deploy target/deploy/sol_rwa_engine.so --program-id RWA1111111111111111111111111111111111111111

# 3. Install Client Engine Dependencies
npm install

# 4. Run Complete Regulatory Test Suite
anchor test

# 5. Launch Enterprise Compliance Worker
export RESEND_API_KEY="re_test_000000000000000000000000"
export COMPLIANCE_EMAIL="counsel@capital-management.com"
npm run start:worker