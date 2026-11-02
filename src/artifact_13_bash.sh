# Build verifiable BPF bytecode
anchor build

# Deploy program to cluster
solana program deploy \
  --program-id target/deploy/rwa_compliance_engine-keypair.json \
  target/deploy/rwa_compliance_engine.so \
  --url https://api.devnet.solana.com