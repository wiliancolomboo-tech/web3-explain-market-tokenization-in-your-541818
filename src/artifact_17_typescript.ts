import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SovereignRwa } from "../target/types/sovereign_rwa";
import {
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  ExtensionType,
  TOKEN_2022_PROGRAM_ID,
  createInitializeMintInstruction,
  createInitializeTransferHookInstruction,
  getMintLen,
  createAccount,
  mintTo,
  transferChecked,
} from "@solana/spl-token";
import { assert, expect } from "chai";

describe("Sovereign-RWA: Token-2022 US Reg-D Compliance Engine Tests", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.SovereignRwa as Program<SovereignRwa>;
  const authority = provider.wallet as anchor.Wallet;

  let mintKeypair: Keypair;
  let mintPubkey: PublicKey;
  let complianceRegistryPda: PublicKey;
  let registryBump: number;

  const investorAlice = Keypair.generate(); // Accredited US Investor
  const investorBob = Keypair.generate();   // Unaccredited Non-Compliant Account
  let aliceRecordPda: PublicKey;
  let bobRecordPda: PublicKey;

  const ASSET_SYMBOL = "US-CRE-MIA-001";
  const SEC_144_LOCKUP_SECONDS = new anchor.BN(0); // Set to 0 for immediate unit testing of flow

  before(async () => {
    // Airdrop SOL to test actors
    for (const actor of [investorAlice, investorBob]) {
      const sig = await provider.connection.requestAirdrop(actor.publicKey, 2 * anchor.web3.LAMPORTS_PER_SOL);
      await provider.connection.confirmTransaction(sig);
    }

    mintKeypair = Keypair.generate();
    mintPubkey = mintKeypair.publicKey;

    [complianceRegistryPda, registryBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("compliance_registry"), mintPubkey.toBuffer()],
      program.programId
    );

    [aliceRecordPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor_record"), complianceRegistryPda.toBuffer(), investorAlice.publicKey.toBuffer()],
      program.programId
    );

    [bobRecordPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor_record"), complianceRegistryPda.toBuffer(), investorBob.publicKey.toBuffer()],
      program.programId
    );
  });

  it("1. Initializes SEC Compliance Registry for Commercial Real Estate Asset", async () => {
    // 1a. Build Token-2022 Mint with TransferHook Extension
    const extensions = [ExtensionType.TransferHook];
    const mintLen = getMintLen(extensions);
    const lamports = await provider.connection.getMinimumBalanceForRentExemption(mintLen);

    const tx = new Transaction().add(
      SystemProgram.createAccount({
        fromPubkey: authority.publicKey,
        newAccountPubkey: mintPubkey,
        space: mintLen,
        lamports,
        programId: TOKEN_2022_PROGRAM_ID,
      }),
      createInitializeTransferHookInstruction(
        mintPubkey,
        authority.publicKey,
        program.programId,
        TOKEN_2022_PROGRAM_ID
      ),
      createInitializeMintInstruction(
        mintPubkey,
        6,
        authority.publicKey,
        null,
        TOKEN_2022_PROGRAM_ID
      )
    );

    await provider.sendAndConfirm(tx, [mintKeypair]);

    // 1b. Initialize on-chain Compliance Registry
    await program.methods
      .initializeRegistry(ASSET_SYMBOL, SEC_144_LOCKUP_SECONDS)
      .accounts({
        registry: complianceRegistryPda,
        mint: mintPubkey,
        authority: authority.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    const registryAccount = await program.account.complianceRegistry.fetch(complianceRegistryPda);
    expect(registryAccount.assetSymbol).to.equal(ASSET_SYMBOL);
    expect(registryAccount.authority.toBase58()).to.equal(authority.publicKey.toBase58());
    expect(registryAccount.isPaused).to.be.false;
  });

  it("2. Attests Investor Alice under Reg D Rule 506(c) (Accredited US Investor)", async () => {
    await program.methods
      .setInvestorStatus(investorAlice.publicKey, true, 840, false)
      .accounts({
        registry: complianceRegistryPda,
        investorRecord: aliceRecordPda,
        authority: authority.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    const aliceRecord = await program.account.investorComplianceRecord.fetch(aliceRecordPda);
    expect(aliceRecord.isAccredited).to.be.true;
    expect(aliceRecord.jurisdiction).to.equal(840);
    expect(aliceRecord.isRevoked).to.be.false;
  });

  it("3. Registers Investor Bob as Revoked / Unaccredited", async () => {
    await program.methods
      .setInvestorStatus(investorBob.publicKey, false, 840, true)
      .accounts({
        registry: complianceRegistryPda,
        investorRecord: bobRecordPda,
        authority: authority.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    const bobRecord = await program.account.investorComplianceRecord.fetch(bobRecordPda);
    expect(bobRecord.isAccredited).to.be.false;
    expect(bobRecord.isRevoked).to.be.true;
  });

  it("4. Blocks transfer to unaccredited investor Bob via Transfer Hook Interface", async () => {
    try {
      // Simulate transfer check invocation directly
      await program.methods
        .executeTransferHook(new anchor.BN(100_000_000))
        .accounts({
          registry: complianceRegistryPda,
          mint: mintPubkey,
          senderRecord: aliceRecordPda,
          recipientRecord: bobRecordPda,
          sourceAccount: aliceRecordPda, // mock account key
          destinationAccount: bobRecordPda,
          ownerOrDelegate: authority.publicKey,
        })
        .rpc();
      assert.fail("Transfer should have been blocked by Transfer Hook compliance check.");
    } catch (err: any) {
      expect(err.toString()).to.include("UnaccreditedInvestor");
    }
  });

  it("5. Allows transfer between fully verified accredited institutional investors", async () => {
    const investorCharlie = Keypair.generate();
    const [charlieRecordPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor_record"), complianceRegistryPda.toBuffer(), investorCharlie.publicKey.toBuffer()],
      program.programId
    );

    // Whitelist Charlie
    await program.methods
      .setInvestorStatus(investorCharlie.publicKey, true, 840, false)
      .accounts({
        registry: complianceRegistryPda,
        investorRecord: charlieRecordPda,
        authority: authority.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    // Verify transfer check passes without error
    const tx = await program.methods
      .executeTransferHook(new anchor.BN(50_000_000))
      .accounts({
        registry: complianceRegistryPda,
        mint: mintPubkey,
        senderRecord: aliceRecordPda,
        recipientRecord: charlieRecordPda,
        sourceAccount: aliceRecordPda,
        destinationAccount: charlieRecordPda,
        ownerOrDelegate: authority.publicKey,
      })
      .rpc();

    expect(tx).to.be.a("string");
  });

  it("6. Enforces Emergency Pause Circuit Breaker", async () => {
    // Trigger pause
    await program.methods
      .setEmergencyPause(true)
      .accounts({
        registry: complianceRegistryPda,
        authority: authority.publicKey,
      })
      .rpc();

    const registry = await program.account.complianceRegistry.fetch(complianceRegistryPda);
    expect(registry.isPaused).to.be.true;

    // Verify all executions are halted
    try {
      await program.methods
        .executeTransferHook(new anchor.BN(1_000_000))
        .accounts({
          registry: complianceRegistryPda,
          mint: mintPubkey,
          senderRecord: aliceRecordPda,
          recipientRecord: aliceRecordPda,
          sourceAccount: aliceRecordPda,
          destinationAccount: aliceRecordPda,
          ownerOrDelegate: authority.publicKey,
        })
        .rpc();
      assert.fail("Should have halted under Emergency Freeze.");
    } catch (err: any) {
      expect(err.toString()).to.include("RegistryPaused");
    }
  });
});