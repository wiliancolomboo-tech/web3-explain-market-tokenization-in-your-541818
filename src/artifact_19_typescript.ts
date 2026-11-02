// Express or Node HTTP health probe endpoint
import http from "http";

const server = http.createServer(async (req, res) => {
  if (req.url === "/healthz") {
    try {
      // Confirm connection to Solana cluster
      const slot = await connection.getSlot();
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ status: "HEALTHY", currentSlot: slot, timestamp: Date.now() }));
    } catch (e: any) {
      res.writeHead(503, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ status: "UNHEALTHY", reason: e.message }));
    }
  }
});
server.listen(8080);