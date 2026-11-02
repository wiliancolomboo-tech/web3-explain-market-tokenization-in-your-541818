# ==============================================================================
# 1. BUILD & DEPLOY ON-CHAIN PROGRAM (Solana Devnet / Testnet / Mainnet)
# ==============================================================================

# Build deterministic Anchor binary
anchor build

# Deploy to Solana Devnet
solana program deploy \
  --program-id target/deploy/rwa_market_tokenization-keypair.json \
  target/deploy/rwa_market_tokenization.so \
  --url devnet

# ==============================================================================
# 2. RUN TEST SUITE & GENERATE VERIFICATION PROOFS
# ==============================================================================

anchor test

# ==============================================================================
# 3. INITIALIZE CLI & DISPATCH INVESTOR ACCREDITATION
# ==============================================================================

# Export enterprise credentials
export RESEND_API_KEY="re_live_enterprise_secret_token"
export SOLANA_RPC_URL="https://api.devnet.solana.com"

# Register an Accredited US Institutional Investor and dispatch confirmation email
npm run cli -- certify \
  -k /path/to/compliance-admin-keypair.json \
  -i 9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM \
  -e "institutional-desk@blackrock-simulated.com" \
  --country 840 \
  --type 2 \
  --days 365

# Launch headless daemon to stream trades & auto-email trade confirmations
docker-compose up -d --build