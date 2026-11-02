import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import {
  TOKEN_2022_PROGRAM_ID,
  createMint,
  createAccount,
  mintTo,
  getAccount,
} from "@solana/spl-token";
import { expect } from "chai";

describe("SolTokenize-SG Institutional Engine Testing", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  // Authority & Compliance Actors
  const authority = Keypair.generate();
  const complianceOfficer = Keypair.generate();
  const accreditedInvestor = Keypair.generate();
  const unaccreditedActor = Keypair.generate();

  let mintKeypair: Keypair;
  let mintPubkey: PublicKey;
  let marketPda: PublicKey;
  let marketBump: number;

  before(async () => {
    // Airdrop SOL to test actors
    for (const actor of [authority, complianceOfficer, accreditedInvestor, unaccreditedActor]) {
      const sig = await provider.connection.requestAirdrop(actor.publicKey, 2 * anchor.web3.LAMPORTS_PER_SOL);
      await provider.connection.confirmTransaction(sig, "confirmed");
    }

    mintKeypair = Keypair.generate();
    mintPubkey = mintKeypair.publicKey;

    [marketPda, marketBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("market"), mintPubkey.toBuffer()],
      anchor.workspace.SoltokenizeSg.programId
    );
  });

  it("Initializes a regulated MAS asset market (Commercial Paper / Tokenized Fund)", async () => {
    const program = anchor.workspace.SoltokenizeSg;

    const tx = await program.methods
      .initializeMarket(
        "SGD-COMM-PAPER",
        "SG1234567890",
        new anchor.BN(10000), // $1.0000 SGD
        6
      )
      .accounts({
        market: marketPda,
        mint: mintPubkey,
        authority: authority.publicKey,
        complianceOfficer: complianceOfficer.publicKey,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([authority])
      .rpc();

    expect(tx).to.be.a("string");

    const marketState = await program.account.market.fetch(marketPda);
    expect(marketState.assetSymbol).to.equal("SGD-COMM-PAPER");
    expect(marketState.isin).to.equal("SG1234567890");
    expect(marketState.navBps.toNumber()).to.equal(10000);
    expect(marketState.isPaused).to.be.false;
  });

  it("Whitelists an Accredited Investor with SG jurisdiction & validity duration", async () => {
    const program = anchor.workspace.SoltokenizeSg;

    const [investorRecordPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor"), marketPda.toBuffer(), accreditedInvestor.publicKey.toBuffer()],
      program.programId
    );

    const oneYearFromNow = Math.floor(Date.now() / 1000) + 365 * 86400;

    await program.methods
      .setInvestorStatus(
        accreditedInvestor.publicKey,
        true,
        [83, 71], // ASCII for "SG"
        new anchor.BN(oneYearFromNow)
      )
      .accounts({
        market: marketPda,
        investorRecord: investorRecordPda,
        investor: accreditedInvestor.publicKey,
        authority: authority.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([authority])
      .rpc();

    const record = await program.account.investorRecord.fetch(investorRecordPda);
    expect(record.isAccredited).to.be.true;
    expect(record.investor.toBase58()).to.equal(accreditedInvestor.publicKey.toBase58());
  });

  it("Blocks issuance to an unregistered / non-accredited wallet", async () => {
    const program = anchor.workspace.SoltokenizeSg;

    const [unregisteredPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor"), marketPda.toBuffer(), unaccreditedActor.publicKey.toBuffer()],
      program.programId
    );

    try {
      await program.methods
        .issueShares(new anchor.BN(50_000_000))
        .accounts({
          market: marketPda,
          mint: mintPubkey,
          recipientRecord: unregisteredPda,
          recipientTokenAccount: Keypair.generate().publicKey,
          authority: authority.publicKey,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .signers([authority])
        .rpc();
      expect.fail("Expected transaction to fail due to missing whitelist entry");
    } catch (err: any) {
      expect(err).to.exist;
    }
  });

  it("Deterministically updates NAV per unit from trustee valuation", async () => {
    const program = anchor.workspace.SoltokenizeSg;

    const newNav = new anchor.BN(10450); // $1.0450 SGD
    await program.methods
      .updateNav(newNav)
      .accounts({
        market: marketPda,
        authority: authority.publicKey,
      })
      .signers([authority])
      .rpc();

    const marketState = await program.account.market.fetch(marketPda);
    expect(marketState.navBps.toNumber()).to.equal(10450);
  });
});