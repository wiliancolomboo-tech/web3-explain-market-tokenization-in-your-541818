import * as http from "http";

export function startHealthServer(port = 8080) {
  const server = http.createServer((req, res) => {
    if (req.url === "/health" && req.method === "GET") {
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(
        JSON.stringify({
          status: "UP",
          subsystem: "Solana Token-2022 US RWA Transfer Hook Daemon",
          solana_synced: true,
          jurisdiction: "US_SEC_REG_D_506C",
          uptime: process.uptime(),
          timestamp: new Date().toISOString(),
        })
      );
      return;
    }
    res.writeHead(404);
    res.end();
  });

  server.listen(port, () => {
    console.log(`[HEALTH] Observability HTTP server listening on port ${port}`);
  });
}