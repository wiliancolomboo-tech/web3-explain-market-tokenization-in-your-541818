# Copy and configure production secrets
cp .env.example .env

# Launch self-healing container
docker compose up -d --build

# Inspect real-time DvP log stream
docker compose logs -f rwa-compliance-daemon