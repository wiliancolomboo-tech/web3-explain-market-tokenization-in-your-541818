import { Command } from "commander";
import { Keypair, PublicKey } from "@solana/web3.js";
import * as fs from "fs";
import * as dotenv from "dotenv";
import { EnterpriseComplianceService } from "./compliance-service";

dotenv.config();

const program = new Command();
program.version("1.0.0").description("Institutional Solana RWA CLI Compliance Manager");

function loadKeypair(path: string): Keypair {
  const secretKey = JSON.parse(fs.readFileSync(path, "utf-8"));
  return Keypair.fromSecretKey(new Uint8Array(secretKey));
}

program
  .command("certify")
  .description("Certify an investor for SEC Rule 506(c) Secondary Trading")
  .requiredOption("-k, --keypair <path>", "Path to Compliance Admin Keypair JSON")
  .requiredOption("-i, --investor <pubkey>", "Investor Solana Public Key")
  .requiredOption("-e, --email <email>", "Investor Contact Email for Resend Delivery")
  .option("-c, --country <code", "ISO Numeric Country Code", "840")
  .option("-t, --type <type>", "1 = Accredited, 2 = QIB", "1")
  .option("-d, --days <days>", "Validity in Days", "365")
  .action(async (options) => {
    const adminKey = loadKeypair(options.keypair);
    const dummyIdl = JSON.parse(fs.readFileSync("./target/idl/rwa_market_tokenization.json", "utf-8"));
    const rpcUrl = process.env.SOLANA_RPC_URL || "https://api.devnet.solana.com";
    const resendApiKey = process.env.RESEND_API_KEY || "re_mock_key";

    const service = new EnterpriseComplianceService(
      rpcUrl,
      adminKey,
      new PublicKey(dummyIdl.address),
      dummyIdl,
      resendApiKey
    );

    const sig = await service.certifyInvestor({
      investorPublicKey: new PublicKey(options.investor),
      countryCode: parseInt(options.country),
      investorClassification: parseInt(options.type),
      validityDays: parseInt(options.days),
      isSanctioned: false,
      investorEmail: options.email,
    });

    console.log(`Success! Verification transaction signature: ${sig}`);
  });

program
  .command("listen")
  .description("Stream real-time compliance events and forward to enterprise logging")
  .requiredOption("-k, --keypair <path>", "Path to Admin Keypair JSON")
  .action(async (options) => {
    const adminKey = loadKeypair(options.keypair);
    const dummyIdl = JSON.parse(fs.readFileSync("./target/idl/rwa_market_tokenization.json", "utf-8"));
    const rpcUrl = process.env.SOLANA_RPC_URL || "https://api.devnet.solana.com";
    const resendApiKey = process.env.RESEND_API_KEY || "re_mock_key";

    const service = new EnterpriseComplianceService(
      rpcUrl,
      adminKey,
      new PublicKey(dummyIdl.address),
      dummyIdl,
      resendApiKey
    );

    await service.listenToSettlementEvents();
  });

program.parse(process.argv);