import express, { Request, Response } from "express";

const app = express();
const PORT = process.env.PORT || 8080;

app.get("/health", async (_req: Request, res: Response) => {
  res.status(200).json({
    status: "healthy",
    subsystem: "aegis-solana-compliance-oracle",
    timestamp: new Date().toISOString(),
    uptime: process.uptime(),
  });
});

app.listen(PORT, () => {
  console.log(`[Health Telemetry] Server running on port ${PORT}`);
});