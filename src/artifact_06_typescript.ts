import { Connection, Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import { Program, AnchorProvider, Wallet, Idl, BN } from "@coral-xyz/anchor";
import { Resend } from "resend";
import * as winston from "winston";

const logger = winston.createLogger({
  level: "info",
  format: winston.format.combine(
    winston.format.timestamp(),
    winston.format.json()
  ),
  transports: [new winston.transports.Console()],
});

export interface CertificationParams {
  investorPublicKey: PublicKey;
  countryCode: number; // 840 = US
  investorClassification: number; // 1 = Accredited, 2 = QIB
  validityDays: number;
  isSanctioned: boolean;
  investorEmail: string;
}

export class EnterpriseComplianceService {
  private connection: Connection;
  private provider: AnchorProvider;
  private program: Program;
  private resend: Resend;
  private adminKeypair: Keypair;

  constructor(
    rpcUrl: string,
    adminKeypair: Keypair,
    programId: PublicKey,
    idl: Idl,
    resendApiKey: string
  ) {
    this.connection = new Connection(rpcUrl, "confirmed");
    this.adminKeypair = adminKeypair;
    const wallet = new Wallet(this.adminKeypair);
    this.provider = new AnchorProvider(this.connection, wallet, {
      commitment: "confirmed",
    });
    this.program = new Program(idl, this.provider);
    this.resend = new Resend(resendApiKey);
  }

  public async certifyInvestor(params: CertificationParams): Promise<string> {
    logger.info("Executing investor compliance verification", {
      investor: params.investorPublicKey.toBase58(),
      country: params.countryCode,
    });

    const [complianceConfigPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("compliance_config"), this.adminKeypair.publicKey.toBuffer()],
      this.program.programId
    );

    const [investorRegistryPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor_registry"), params.investorPublicKey.toBuffer()],
      this.program.programId
    );

    const now = Math.floor(Date.now() / 1000);
    const expiryTimestamp = new BN(now + params.validityDays * 86400);

    const txSig = await this.program.methods
      .setInvestorCertificate(
        params.investorPublicKey,
        params.countryCode,
        params.investorClassification,
        expiryTimestamp,
        params.isSanctioned
      )
      .accounts({
        admin: this.adminKeypair.publicKey,
        complianceConfig: complianceConfigPDA,
        investorRegistry: investorRegistryPDA,
        systemProgram: SystemProgram.programId,
      })
      .signers([this.adminKeypair])
      .rpc();

    logger.info("Attestation confirmed on-chain", { txSignature: txSig });

    // Enterprise Integration: Send Legal Notice through Resend
    await this.dispatchComplianceNotice(params, txSig);

    return txSig;
  }

  private async dispatchComplianceNotice(
    params: CertificationParams,
    txSignature: string
  ): Promise<void> {
    try {
      const emailPayload = {
        from: "compliance@tokenized-assets.us",
        to: params.investorEmail,
        subject: "SEC Rule 506(c) On-Chain Compliance Certificate Issued",
        html: `
          <div style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto; border: 1px solid #e2e8f0; border-radius: 8px; padding: 24px;">
            <h2 style="color: #0f172a;">SEC Rule 506(c) Digital Asset Certification</h2>
            <p style="color: #334155; line-height: 1.5;">
              Your Solana public identity has been cryptographically certified under SEC Regulation D 506(c) and Regulation S provisions.
            </p>
            <div style="background-color: #f8fafc; padding: 16px; border-radius: 6px; margin: 20px 0;">
              <p style="margin: 4px 0;"><strong>Investor Public Key:</strong> <code>${params.investorPublicKey.toBase58()}</code></p>
              <p style="margin: 4px 0;"><strong>ISO Country Origin:</strong> ${params.countryCode} (United States)</p>
              <p style="margin: 4px 0;"><strong>Investor Class:</strong> ${params.investorClassification === 2 ? "Qualified Institutional Buyer (QIB)" : "Accredited Investor"}</p>
              <p style="margin: 4px 0;"><strong>Settlement Tx Hash:</strong> <code>${txSignature}</code></p>
            </div>
            <p style="color: #64748b; font-size: 12px;">
              This notification fulfills the electronic notice requirement under Delaware General Corporation Law § 224 and SEC transfer guidelines.
            </p>
          </div>
        `,
      };

      const dispatchResult = await this.resend.emails.send(emailPayload);
      logger.info("Enterprise confirmation receipt sent via Resend", {
        id: dispatchResult.data?.id,
      });
    } catch (err) {
      logger.error("Failed to send Resend email dispatch", { error: err });
      // Non-blocking for on-chain state, but surfaced in enterprise telemetry
    }
  }

  public async listenToSettlementEvents(): Promise<void> {
    logger.info("Initializing WebSocket listener for TransferAuditedEvent...");

    this.program.addEventListener("TransferAuditedEvent", async (event, slot) => {
      logger.info("TransferAuditedEvent Intercepted!", {
        source: event.source.toBase58(),
        destination: event.destination.toBase58(),
        amount: event.amount.toString(),
        slot,
      });

      // Post trade receipt to enterprise audit stream
      await this.resend.emails.send({
        from: "audit-desk@tokenized-assets.us",
        to: "compliance-archive@tokenized-assets.us",
        subject: `SEC Audit Ledger: Settlement Recorded at Slot ${slot}`,
        html: `
          <h3>Secondary Market Transfer Executed</h3>
          <ul>
            <li><strong>Source:</strong> ${event.source.toBase58()}</li>
            <li><strong>Destination:</strong> ${event.destination.toBase58()}</li>
            <li><strong>Token Units:</strong> ${event.amount.toString()}</li>
            <li><strong>Framework ID:</strong> ${event.frameworkId}</li>
            <li><strong>Timestamp:</strong> ${new Date(event.timestamp.toNumber() * 1000).toISOString()}</li>
          </ul>
        `,
      });
    });
  }
}