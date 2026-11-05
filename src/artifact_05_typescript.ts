import { Connection, Keypair, PublicKey, clusterApiUrl } from "@solana/web3.js";
import { Program, AnchorProvider, Wallet, Idl } from "@coral-xyz/anchor";
import axios from "axios";
import * as dotenv from "dotenv";

dotenv.config();

// Enterprise Configuration
const SOLANA_RPC_URL = process.env.SOLANA_RPC_URL || clusterApiUrl("devnet");
const RESEND_API_KEY = process.env.RESEND_API_KEY || "re_test_000000000000000000000000";
const RESEND_ENDPOINT = "https://api.resend.com/emails";
const COMPLIANCE_WEBHOOK_URL = process.env.COMPLIANCE_WEBHOOK_URL || "https://httpbin.org/post";

export interface ComplianceNotificationPayload {
  investor: string;
  isAccredited: boolean;
  isSanctioned: boolean;
  lockupExpiry: number;
  isin: string;
  transactionSignature: string;
}

export class VeriUSEnterpriseAgent {
  private connection: Connection;
  private provider: AnchorProvider;
  private programId: PublicKey;

  constructor(signerKeypair: Keypair, programId: PublicKey) {
    this.connection = new Connection(SOLANA_RPC_URL, "confirmed");
    const wallet = new Wallet(signerKeypair);
    this.provider = new AnchorProvider(this.connection, wallet, {
      commitment: "confirmed",
      preflightCommitment: "confirmed",
    });
    this.programId = programId;
  }

  /**
   * Dispatches regulatory alert to Enterprise SIEM/Slack via Webhook
   */
  public async dispatchWebhookAlert(payload: ComplianceNotificationPayload): Promise<void> {
    try {
      const response = await axios.post(
        COMPLIANCE_WEBHOOK_URL,
        {
          service: "VeriUS-Compliance-Daemon",
          environment: process.env.NODE_ENV || "production",
          timestamp: new Date().toISOString(),
          compliance_event: payload,
        },
        { timeout: 5000 }
      );
      console.log(`[TELEMETRY] Compliance webhook published successfully. Status: ${response.status}`);
    } catch (error: any) {
      console.error(`[ERROR] Failed to dispatch webhook alert: ${error.message}`);
    }
  }

  /**
   * Sends transactional email alert via Resend API
   */
  public async sendRegulatoryEmail(
    recipientEmail: string,
    payload: ComplianceNotificationPayload
  ): Promise<void> {
    try {
      const emailBody = {
        from: "compliance@verius-capital.com",
        to: [recipientEmail],
        subject: `[AUDIT ALERT] Tokenized Asset Compliance State Updated - ${payload.isin}`,
        html: `
          <h2>SEC Compliance Attestation Recorded</h2>
          <p><strong>Investor Public Key:</strong> ${payload.investor}</p>
          <p><strong>Asset ISIN:</strong> ${payload.isin}</p>
          <p><strong>SEC Rule 506(c) Accredited:</strong> ${payload.isAccredited ? "VERIFIED" : "UNVERIFIED"}</p>
          <p><strong>OFAC Sanctions Status:</strong> ${payload.isSanctioned ? "FLAGGED / BLOCKED" : "CLEARED"}</p>
          <p><strong>Rule 144 Lockup Expiry:</strong> ${new Date(payload.lockupExpiry * 1000).toUTCString()}</p>
          <p><strong>On-Chain Transaction:</strong> <code>${payload.transactionSignature}</code></p>
        `,
      };

      const response = await axios.post(RESEND_ENDPOINT, emailBody, {
        headers: {
          Authorization: `Bearer ${RESEND_API_KEY}`,
          "Content-Type": "application/json",
        },
      });

      console.log(`[RESEND] Regulatory notification email sent. ID: ${response.data.id}`);
    } catch (error: any) {
      console.error(`[ERROR] Resend dispatch failure: ${error.response?.data || error.message}`);
    }
  }

  /**
   * Core Daemon process monitoring and validating off-chain ingest
   */
  public async runHealthCheck(): Promise<boolean> {
    try {
      const version = await this.connection.getVersion();
      const slot = await this.connection.getSlot();
      console.log(`[HEALTH] Solana Node Connected: Node v${version["solana-core"]}, Current Slot: ${slot}`);
      return true;
    } catch (error: any) {
      console.error(`[CRITICAL] RPC Node Unhealthy: ${error.message}`);
      return false;
    }
  }
}

// Executable entry point for automation CLI
if (require.main === module) {
  (async () => {
    console.log("Starting VeriUS Enterprise Compliance Ingestion Worker...");
    const dummyKeypair = Keypair.generate();
    const programId = new PublicKey("VeriUS1111111111111111111111111111111111111");
    const agent = new VeriUSEnterpriseAgent(dummyKeypair, programId);

    const isHealthy = await agent.runHealthCheck();
    if (!isHealthy) process.exit(1);

    // Mock compliance attestation event dispatch
    const mockEvent: ComplianceNotificationPayload = {
      investor: "4vMdpvuvUM1HSmH28dBvDkysoYkh9NTYhq9642111111",
      isAccredited: true,
      isSanctioned: false,
      lockupExpiry: Math.floor(Date.now() / 1000) + 86400 * 365,
      isin: "US912828ZD12", // 10-Year U.S. Treasury Note
      transactionSignature: "5KtPn...sampleSignatureVerificationOnDevnet...7xL",
    };

    await agent.dispatchWebhookAlert(mockEvent);
    if (process.env.RESEND_API_KEY) {
      await agent.sendRegulatoryEmail("auditor@sec-oversight-vault.com", mockEvent);
    }
    console.log("VeriUS Ingestion Worker cycle completed.");
  })();
}