# 1. Create Token-2022 Mint with Transfer Hook Extension
spl-token --program-2022 create-token \
  --transfer-hook RWA1111111111111111111111111111111111111111

# 2. Initialize Compliance Config via CLI
npm run oracle:attest -- -i <INVESTOR_PUBKEY> -a true -s false -j 840