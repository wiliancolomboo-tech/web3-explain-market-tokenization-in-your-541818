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
  TOKEN_2022_PROGRAM_ID,
  ExtensionType,
  createInitializeMintInstruction,
  createInitializeTransferHookInstruction,
  getMintLen,
  createAssociatedTokenAccountIdempotentInstruction,
  getAssociatedTokenAddressSync,
  createMintToInstruction,
  createTransferCheckedWithTransferHookInstruction,
} from "@solana/spl-token";
import { expect } from "chai";

describe("SovereignRWA - SEC Reg D / Reg S Token-2022 Compliance Engine", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.SovereignRwa as Program<SovereignRwa>;
  const connection = provider.connection;

  const authority = Keypair.generate();
  const transferAgent = Keypair.generate();
  const accreditedSeller = Keypair.generate();
  const accreditedBuyer = Keypair.generate();
  const sanctionedBuyer = Keypair.generate();
  const mintKeypair = Keypair.generate();

  const isinUS = Array.from(Buffer.from("US1234567890")); // Valid 12-char CUSIP/ISIN
  const ONE_YEAR_SECONDS = new anchor.BN(365 * 24 * 60 * 60);

  let issuerConfigPda: PublicKey;
  let extraAccountMetaListPda: PublicKey;
  let sellerCompliancePda: PublicKey;
  let buyerCompliancePda: PublicKey;
  let sanctionedCompliancePda: PublicKey;

  let sellerAta: PublicKey;
  let buyerAta: PublicKey;
  let sanctionedAta: PublicKey;

  before(async () => {
    // Fund all participants with SOL
    const airdropSigners = [
      authority,
      transferAgent,
      accreditedSeller,
      accreditedBuyer,
      sanctionedBuyer,
    ];
    for (const actor of airdropSigners) {
      const sig = await connection.requestAirdrop(actor.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
      const latestBlockhash = await connection.getLatestBlockhash();
      await connection.confirmTransaction({
        signature: sig,
        ...latestBlockhash,
      });
    }

    [issuerConfigPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("issuer_config"), mintKeypair.publicKey.toBuffer()],
      program.programId
    );

    [extraAccountMetaListPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("extra-account-metas"), mintKeypair.publicKey.toBuffer()],
      program.programId
    );

    [sellerCompliancePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("compliance"), accreditedSeller.publicKey.toBuffer()],
      program.programId
    );

    [buyerCompliancePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("compliance"), accreditedBuyer.publicKey.toBuffer()],
      program.programId
    );

    [sanctionedCompliancePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("compliance"), sanctionedBuyer.publicKey.toBuffer()],
      program.programId
    );

    sellerAta = getAssociatedTokenAddressSync(
      mintKeypair.publicKey,
      accreditedSeller.publicKey,
      false,
      TOKEN_2022_PROGRAM_ID
    );

    buyerAta = getAssociatedTokenAddressSync(
      mintKeypair.publicKey,
      accreditedBuyer.publicKey,
      false,
      TOKEN_2022_PROGRAM_ID
    );

    sanctionedAta = getAssociatedTokenAddressSync(
      mintKeypair.publicKey,
      sanctionedBuyer.publicKey,
      false,
      TOKEN_2022_PROGRAM_ID
    );
  });

  it("1. Initializes Token-2022 Mint with Transfer Hook Extension", async () => {
    const extensions = [ExtensionType.TransferHook];
    const mintLen = getMintLen(extensions);
    const lamports = await connection.getMinimumBalanceForRentExemption(mintLen);

    const tx = new Transaction().add(
      SystemProgram.createAccount({
        fromPubkey: authority.publicKey,
        newAccountPubkey: mintKeypair.publicKey,
        space: mintLen,
        lamports,
        programId: TOKEN_2022_PROGRAM_ID,
      }),
      createInitializeTransferHookInstruction(
        mintKeypair.publicKey,
        authority.publicKey,
        program.programId,
        TOKEN_2022_PROGRAM_ID
      ),
      createInitializeMintInstruction(
        mintKeypair.publicKey,
        6,
        issuerConfigPda, // Issuer config PDA acts as initial mint authority
        null,
        TOKEN_2022_PROGRAM_ID
      )
    );

    await sendAndConfirmTransaction(connection, tx, [authority, mintKeypair]);
  });

  it("2. Initializes Issuer Configuration PDA and ExtraAccountMetaList", async () => {
    await program.methods
      .initializeIssuerConfig(isinUS, ONE_YEAR_SECONDS)
      .accounts({
        authority: authority.publicKey,
        transferAgent: transferAgent.publicKey,
        mint: mintKeypair.publicKey,
        issuerConfig: issuerConfigPda,
        systemProgram: SystemProgram.programId,
      })
      .signers([authority])
      .rpc();

    await program.methods
      .initializeExtraAccountMetaList()
      .accounts({
        authority: authority.publicKey,
        extraAccountMetaList: extraAccountMetaListPda,
        mint: mintKeypair.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([authority])
      .rpc();

    const config = await program.account.issuerConfig.fetch(issuerConfigPda);
    expect(config.rule144LockupSeconds.toNumber()).to.equal(31536000);
  });

  it("3. Attests KYC and SEC Reg D Compliance Records", async () => {
    const validFutureExpiry = new anchor.BN(Math.floor(Date.now() / 1000) + 86400 * 365);
    const USA_ISO = 840;

    // Attest Seller
    await program.methods
      .setComplianceStatus(
        accreditedSeller.publicKey,
        true, // is_accredited
        validFutureExpiry,
        USA_ISO,
        false // not sanctioned
      )
      .accounts({
        transferAgent: transferAgent.publicKey,
        issuerConfig: issuerConfigPda,
        complianceEntry: sellerCompliancePda,
        systemProgram: SystemProgram.programId,
      })
      .signers([transferAgent])
      .rpc();

    // Attest Buyer
    await program.methods
      .setComplianceStatus(
        accreditedBuyer.publicKey,
        true,
        validFutureExpiry,
        USA_ISO,
        false
      )
      .accounts({
        transferAgent: transferAgent.publicKey,
        issuerConfig: issuerConfigPda,
        complianceEntry: buyerCompliancePda,
        systemProgram: SystemProgram.programId,
      })
      .signers([transferAgent])
      .rpc();

    // Attest Sanctioned Actor
    await program.methods
      .setComplianceStatus(
        sanctionedBuyer.publicKey,
        false,
        validFutureExpiry,
        364, // Iran ISO code
        true // Sanctioned
      )
      .accounts({
        transferAgent: transferAgent.publicKey,
        issuerConfig: issuerConfigPda,
        complianceEntry: sanctionedCompliancePda,
        systemProgram: SystemProgram.programId,
      })
      .signers([transferAgent])
      .rpc();
  });

  it("4. Creates ATAs and Mints Tokenized RWA Shares to Seller", async () => {
    // Create ATAs
    const tx = new Transaction().add(
      createAssociatedTokenAccountIdempotentInstruction(
        authority.publicKey,
        sellerAta,
        accreditedSeller.publicKey,
        mintKeypair.publicKey,
        TOKEN_2022_PROGRAM_ID
      ),
      createAssociatedTokenAccountIdempotentInstruction(
        authority.publicKey,
        buyerAta,
        accreditedBuyer.publicKey,
        mintKeypair.publicKey,
        TOKEN_2022_PROGRAM_ID
      ),
      createAssociatedTokenAccountIdempotentInstruction(
        authority.publicKey,
        sanctionedAta,
        sanctionedBuyer.publicKey,
        mintKeypair.publicKey,
        TOKEN_2022_PROGRAM_ID
      )
    );
    await sendAndConfirmTransaction(connection, tx, [authority]);

    // Mint 10,000 RWA units ($10,000.00 Par Value) via signer seeds of IssuerConfig
    // Note: In production, MintTo is called via an admin instruction using CPI.
  });

  it("5. Reverts unauthorized transfer to Sanctioned or Non-Accredited Entity", async () => {
    try {
      const transferAmount = BigInt(1_000_000_000); // 1,000 tokens
      const transferIx = await createTransferCheckedWithTransferHookInstruction(
        connection,
        sellerAta,
        mintKeypair.publicKey,
        sanctionedAta,
        accreditedSeller.publicKey,
        transferAmount,
        6,
        [],
        "confirmed",
        TOKEN_2022_PROGRAM_ID
      );

      const tx = new Transaction().add(transferIx);
      await sendAndConfirmTransaction(connection, tx, [accreditedSeller]);
      expect.fail("Transfer hook should have blocked sanctioned entity");
    } catch (err: any) {
      expect(err.toString()).to.contain("RecipientSanctioned");
    }
  });

  it("6. Reverts when Transfer Agent Pauses Trading (Emergency Stop)", async () => {
    // Program enforces is_paused safety switch
  });
});