import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { RwaMarketTokenization } from "../target/types/rwa_market_tokenization";
import {
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  TOKEN_2022_PROGRAM_ID,
  createMint,
  createAccount,
  mintTo,
  transferChecked,
  ExtensionType,
  createInitializeTransferHookInstruction,
} from "@solana/spl-token";
import { assert, expect } from "chai";

describe("SEC Reg D 506(c) RWA Token-2022 Compliance Engine", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.RwaMarketTokenization as Program<RwaMarketTokenization>;

  const admin = Keypair.generate();
  const accreditedSender = Keypair.generate();
  const accreditedReceiver = Keypair.generate();
  const unaccreditedParty = Keypair.generate();

  let mintKeypair: Keypair;
  let complianceConfigPDA: PublicKey;
  let senderRegistryPDA: PublicKey;
  let receiverRegistryPDA: PublicKey;

  let senderATA: PublicKey;
  let receiverATA: PublicKey;
  let unaccreditedATA: PublicKey;

  before(async () => {
    // 1. Fund all participants
    for (const kp of [admin, accreditedSender, accreditedReceiver, unaccreditedParty]) {
      const sig = await provider.connection.requestAirdrop(kp.publicKey, 5 * anchor.web3.LAMPORTS_PER_SOL);
      const latestBlockhash = await provider.connection.getLatestBlockhash();
      await provider.connection.confirmTransaction({
        signature: sig,
        ...latestBlockhash,
      });
    }

    // 2. Derive PDAs
    mintKeypair = Keypair.generate();

    [complianceConfigPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("compliance_config"), mintKeypair.publicKey.toBuffer()],
      program.programId
    );

    [senderRegistryPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor_registry"), accreditedSender.publicKey.toBuffer()],
      program.programId
    );

    [receiverRegistryPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor_registry"), accreditedReceiver.publicKey.toBuffer()],
      program.programId
    );
  });

  it("1. Successfully initializes Compliance Config for SEC Rule 506(c) (Zero Lockup for Test)", async () => {
    // 5063 = Reg D 506(c), 0 seconds lockup for initial positive assertion test
    await program.methods
      .initializeComplianceConfig(5063, new anchor.BN(0))
      .accounts({
        admin: admin.publicKey,
        mint: mintKeypair.publicKey,
        complianceConfig: complianceConfigPDA,
        systemProgram: SystemProgram.programId,
      })
      .signers([admin])
      .rpc();

    const configAccount = await program.account.complianceConfig.fetch(complianceConfigPDA);
    assert.equal(configAccount.regulatoryFrameworkId, 5063);
    assert.isFalse(configAccount.isPaused);
    assert.isTrue(configAccount.admin.equals(admin.publicKey));
  });

  it("2. Certifies Accredited Institutional Sender and Receiver", async () => {
    const futureExpiry = new anchor.BN(Math.floor(Date.now() / 1000) + 86400 * 365);

    // Certify Sender (US Accredited Natural Person)
    await program.methods
      .setInvestorCertificate(
        accreditedSender.publicKey,
        840, // United States
        1,   // Accredited
        futureExpiry,
        false // Not sanctioned
      )
      .accounts({
        admin: admin.publicKey,
        complianceConfig: complianceConfigPDA,
        investorRegistry: senderRegistryPDA,
        systemProgram: SystemProgram.programId,
      })
      .signers([admin])
      .rpc();

    // Certify Receiver (Qualified Institutional Buyer)
    await program.methods
      .setInvestorCertificate(
        accreditedReceiver.publicKey,
        840,
        2, // QIB
        futureExpiry,
        false
      )
      .accounts({
        admin: admin.publicKey,
        complianceConfig: complianceConfigPDA,
        investorRegistry: receiverRegistryPDA,
        systemProgram: SystemProgram.programId,
      })
      .signers([admin])
      .rpc();

    const senderCert = await program.account.investorRegistry.fetch(senderRegistryPDA);
    assert.equal(senderCert.countryCode, 840);
    assert.isFalse(senderCert.isSanctioned);
  });

  it("3. Executes Transfer Hook verification between Accredited Wallets", async () => {
    // Direct invocation test of the transfer hook logic
    await program.methods
      .transferHookExecute(new anchor.BN(50_000_000))
      .accounts({
        sourceAccount: accreditedSender.publicKey, // Simulated ATA
        mint: mintKeypair.publicKey,
        destinationAccount: accreditedReceiver.publicKey, // Simulated ATA
        sourceOwner: accreditedSender.publicKey,
        destinationOwner: accreditedReceiver.publicKey,
        complianceConfig: complianceConfigPDA,
        senderRegistry: senderRegistryPDA,
        recipientRegistry: receiverRegistryPDA,
      })
      .rpc();
  });

  it("4. Blocks transfer when recipient is NOT verified in Compliance Registry", async () => {
    const [unaccreditedRegistryPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor_registry"), unaccreditedParty.publicKey.toBuffer()],
      program.programId
    );

    try {
      await program.methods
        .transferHookExecute(new anchor.BN(10_000_000))
        .accounts({
          sourceAccount: accreditedSender.publicKey,
          mint: mintKeypair.publicKey,
          destinationAccount: unaccreditedParty.publicKey,
          sourceOwner: accreditedSender.publicKey,
          destinationOwner: unaccreditedParty.publicKey,
          complianceConfig: complianceConfigPDA,
          senderRegistry: senderRegistryPDA,
          recipientRegistry: unaccreditedRegistryPDA,
        })
        .rpc();
      assert.fail("Should have thrown error due to missing compliance record");
    } catch (err: any) {
      expect(err.toString()).to.include("AccountNotInitialized");
    }
  });

  it("5. Halts all transactions instantly when Circuit Breaker is triggered", async () => {
    // Admin toggles pause
    await program.methods
      .setEmergencyPause(true)
      .accounts({
        admin: admin.publicKey,
        complianceConfig: complianceConfigPDA,
      })
      .signers([admin])
      .rpc();

    try {
      await program.methods
        .transferHookExecute(new anchor.BN(1_000_000))
        .accounts({
          sourceAccount: accreditedSender.publicKey,
          mint: mintKeypair.publicKey,
          destinationAccount: accreditedReceiver.publicKey,
          sourceOwner: accreditedSender.publicKey,
          destinationOwner: accreditedReceiver.publicKey,
          complianceConfig: complianceConfigPDA,
          senderRegistry: senderRegistryPDA,
          recipientRegistry: receiverRegistryPDA,
        })
        .rpc();
      assert.fail("Should have failed due to emergency pause");
    } catch (err: any) {
      expect(err.toString()).to.include("TradingHaltedByCompliance");
    }

    // Reset circuit breaker
    await program.methods
      .setEmergencyPause(false)
      .accounts({
        admin: admin.publicKey,
        complianceConfig: complianceConfigPDA,
      })
      .signers([admin])
      .rpc();
  });
});