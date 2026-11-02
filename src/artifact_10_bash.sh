# 1. Build and verify the Anchor program deterministically
anchor build

# 2. Configure Solana CLI to target desired cluster
solana config set --url https://api.devnet.solana.com
solana airdrop 2

# 3. Deploy the smart contract program
anchor deploy --program-name sol_juris_rwa

# 4. Initialize the orchestration container environment
cp .env.example .env
# Edit .env with your RESEND_API_KEY and SLACK_WEBHOOK_URL

# 5. Launch turnkey enterprise daemon
docker-compose up -d --build

# 6. Verify operational telemetry
curl -s http://localhost:8080/health | jq .