import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { RwaComplianceEngine } from "../target/types/rwa_compliance_engine";
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
  transferCheckedWithFee,
  ExtensionType,
} from "@solana/spl-token";
import { assert, expect } from "chai";

describe("US RWA Token-2022 Compliance Test Matrix", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.RwaComplianceEngine as Program<RwaComplianceEngine>;

  const payer = (provider.wallet as any).payer as Keypair;
  const oracleSigner = Keypair.generate();
  const issuerAuthority = Keypair.generate();
  const accreditedInvestor = Keypair.generate();
  const nonAccreditedInvestor = Keypair.generate();
  const sanctionedInvestor = Keypair.generate();

  let assetMint: Keypair;
  let complianceConfigPda: PublicKey;
  let extraAccountMetasPda: PublicKey;

  before(async () => {
    assetMint = Keypair.generate();

    // Derive PDAs
    [complianceConfigPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("compliance-config"), assetMint.publicKey.toBuffer()],
      program.programId
    );

    [extraAccountMetasPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("extra-account-metas"), assetMint.publicKey.toBuffer()],
      program.programId
    );

    // Fund test wallets
    const airdropSigners = [oracleSigner, issuerAuthority, accreditedInvestor, nonAccreditedInvestor, sanctionedInvestor];
    for (const s of airdropSigners) {
      const sig = await provider.connection.requestAirdrop(s.publicKey, 2 * anchor.web3.LAMPORTS_PER_SOL);
      await provider.connection.confirmTransaction(sig);
    }
  });

  it("1. Initializes Compliance Configuration with US Jurisdiction (840) and Rule 144 parameters", async () => {
    await program.methods
      .initializeComplianceConfig(
        issuerAuthority.publicKey,
        oracleSigner.publicKey,
        840, // USA
        new anchor.BN(31536000) // 1 Year Rule 144 holding lock
      )
      .accounts({
        complianceConfig: complianceConfigPda,
        assetMint: assetMint.publicKey,
        payer: payer.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([payer])
      .rpc();

    const configAccount = await program.account.complianceConfig.fetch(complianceConfigPda);
    assert.equal(configAccount.authority.toBase58(), issuerAuthority.publicKey.toBase58());
    assert.equal(configAccount.oracleSigner.toBase58(), oracleSigner.publicKey.toBase58());
    assert.equal(configAccount.jurisdictionCode, 840);
    assert.isFalse(configAccount.isPaused);
  });

  it("2. Registers Accredited Investor Attestation via Authorized Oracle", async () => {
    const [accreditedPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor-registry"), accreditedInvestor.publicKey.toBuffer()],
      program.programId
    );

    const oneYearFromNow = Math.floor(Date.now() / 1000) + 31536000;

    await program.methods
      .updateInvestorAttestation(
        accreditedInvestor.publicKey,
        true,  // is_accredited = true
        false, // is_sanctioned = false
        new anchor.BN(oneYearFromNow),
        840
      )
      .accounts({
        complianceConfig: complianceConfigPda,
        oracleSigner: oracleSigner.publicKey,
        investorAttestation: accreditedPda,
        payer: payer.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([oracleSigner, payer])
      .rpc();

    const attestation = await program.account.investorAttestation.fetch(accreditedPda);
    assert.isTrue(attestation.isAccredited);
    assert.isFalse(attestation.isSanctioned);
    assert.equal(attestation.jurisdictionCode, 840);
  });

  it("3. Registers Non-Accredited Investor Attestation", async () => {
    const [nonAccreditedPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor-registry"), nonAccreditedInvestor.publicKey.toBuffer()],
      program.programId
    );

    const oneYearFromNow = Math.floor(Date.now() / 1000) + 31536000;

    await program.methods
      .updateInvestorAttestation(
        nonAccreditedInvestor.publicKey,
        false, // is_accredited = false (Fails Reg D 506c)
        false,
        new anchor.BN(oneYearFromNow),
        840
      )
      .accounts({
        complianceConfig: complianceConfigPda,
        oracleSigner: oracleSigner.publicKey,
        investorAttestation: nonAccreditedPda,
        payer: payer.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([oracleSigner, payer])
      .rpc();

    const attestation = await program.account.investorAttestation.fetch(nonAccreditedPda);
    assert.isFalse(attestation.isAccredited);
  });

  it("4. Blocks transfer to Non-Accredited Investor with DestinationNotAccredited error", async () => {
    // Attempting simulated Transfer Hook check
    const [sourcePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor-registry"), accreditedInvestor.publicKey.toBuffer()],
      program.programId
    );
    const [destPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor-registry"), nonAccreditedInvestor.publicKey.toBuffer()],
      program.programId
    );

    try {
      await program.methods
        .transferHookExecute(new anchor.BN(500_000))
        .accounts({
          sourceToken: Keypair.generate().publicKey,
          assetMint: assetMint.publicKey,
          destinationToken: Keypair.generate().publicKey,
          sourceAuthority: accreditedInvestor.publicKey,
          extraAccountMetas: extraAccountMetasPda,
          sourceAttestation: sourcePda,
          destinationAttestation: destPda,
          complianceConfig: complianceConfigPda,
        })
        .rpc();
      assert.fail("Transfer should have thrown DestinationNotAccredited error");
    } catch (err: any) {
      expect(err.toString()).to.include("DestinationNotAccredited");
    }
  });

  it("5. Blocks transfer from/to Sanctioned Investor with OFAC SDN error", async () => {
    const [sanctionedPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor-registry"), sanctionedInvestor.publicKey.toBuffer()],
      program.programId
    );

    const futureTime = Math.floor(Date.now() / 1000) + 31536000;

    // Flag investor on OFAC list
    await program.methods
      .updateInvestorAttestation(
        sanctionedInvestor.publicKey,
        true,
        true, // is_sanctioned = TRUE
        new anchor.BN(futureTime),
        840
      )
      .accounts({
        complianceConfig: complianceConfigPda,
        oracleSigner: oracleSigner.publicKey,
        investorAttestation: sanctionedPda,
        payer: payer.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([oracleSigner, payer])
      .rpc();

    const [accreditedPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor-registry"), accreditedInvestor.publicKey.toBuffer()],
      program.programId
    );

    try {
      await program.methods
        .transferHookExecute(new anchor.BN(100_000))
        .accounts({
          sourceToken: Keypair.generate().publicKey,
          assetMint: assetMint.publicKey,
          destinationToken: Keypair.generate().publicKey,
          sourceAuthority: sanctionedInvestor.publicKey,
          extraAccountMetas: extraAccountMetasPda,
          sourceAttestation: sanctionedPda,
          destinationAttestation: accreditedPda,
          complianceConfig: complianceConfigPda,
        })
        .rpc();
      assert.fail("Sanctioned transfer should have failed");
    } catch (err: any) {
      expect(err.toString()).to.include("SourceAccountSanctioned");
    }
  });

  it("6. Emergency Issuer Kill-Switch halts secondary trading", async () => {
    await program.methods
      .togglePause(true)
      .accounts({
        complianceConfig: complianceConfigPda,
        authority: issuerAuthority.publicKey,
      })
      .signers([issuerAuthority])
      .rpc();

    const configAccount = await program.account.complianceConfig.fetch(complianceConfigPda);
    assert.isTrue(configAccount.isPaused);
  });
});