import http from "http";

const server = http.createServer((req, res) => {
  if (req.url === "/health" && req.method === "GET") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(
      JSON.stringify({
        status: "HEALTHY",
        jurisdiction: "SG",
        framework: "MAS Notice 626 / SFA Sec 275",
        uptimeSeconds: process.uptime(),
        timestamp: new Date().toISOString(),
      })
    );
  } else {
    res.writeHead(404);
    res.end();
  }
});

server.listen(process.env.PORT || 8080, () => {
  console.log(`[+] Healthcheck telemetry bound on port ${process.env.PORT || 8080}`);
});