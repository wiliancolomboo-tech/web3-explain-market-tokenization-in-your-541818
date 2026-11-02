import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SolrwaCompliance } from "../target/types/solrwa_compliance";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import { expect } from "chai";

describe("SolRWA Comprehensive Institutional Compliance Suite", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.SolrwaCompliance as Program<SolrwaCompliance>;

  const authority = Keypair.generate();
  const complianceOfficer = Keypair.generate();
  const dummyMint = Keypair.generate();

  const accreditedUSInvestor = Keypair.generate();
  const nonAccreditedUSInvestor = Keypair.generate();
  const sanctionedForeignActor = Keypair.generate();

  let assetConfigPda: PublicKey;
  let accreditedRegistryPda: PublicKey;
  let nonAccreditedRegistryPda: PublicKey;
  let sanctionedRegistryPda: PublicKey;

  before(async () => {
    // Airdrop SOL to test actors
    const airdropSigs = await Promise.all([
      provider.connection.requestAirdrop(authority.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL),
      provider.connection.requestAirdrop(complianceOfficer.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL),
    ]);
    await Promise.all(airdropSigs.map((sig) => provider.connection.confirmTransaction(sig)));

    // Derive PDAs
    [assetConfigPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("asset_config"), dummyMint.publicKey.toBuffer()],
      program.programId
    );

    [accreditedRegistryPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor"), dummyMint.publicKey.toBuffer(), accreditedUSInvestor.publicKey.toBuffer()],
      program.programId
    );

    [nonAccreditedRegistryPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor"), dummyMint.publicKey.toBuffer(), nonAccreditedUSInvestor.publicKey.toBuffer()],
      program.programId
    );

    [sanctionedRegistryPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor"), dummyMint.publicKey.toBuffer(), sanctionedForeignActor.publicKey.toBuffer()],
      program.programId
    );
  });

  it("1. Initializes Asset Configuration with 365-day Reg D Rule 144 Safe Harbor", async () => {
    const maxInvestors = 2000;
    const lockupSeconds = new anchor.BN(365 * 24 * 3600); // 1 Year
    const countryCode = 840; // United States

    await program.methods
      .initializeAsset(maxInvestors, lockupSeconds, countryCode)
      .accounts({
        authority: authority.publicKey,
        complianceOfficer: complianceOfficer.publicKey,
        mint: dummyMint.publicKey,
        assetConfig: assetConfigPda,
        systemProgram: SystemProgram.programId,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
      })
      .signers([authority])
      .rpc();

    const configAccount = await program.account.assetConfig.fetch(assetConfigPda);
    expect(configAccount.maxInvestors).to.equal(2000);
    expect(configAccount.countryCode).to.equal(840);
    expect(configAccount.complianceOfficer.toBase58()).to.equal(complianceOfficer.publicKey.toBase58());
  });

  it("2. Registers Verified Accredited US Investor with 1-Year Validity", async () => {
    const validitySeconds = new anchor.BN(365 * 24 * 3600);
    const countryCodeUS = 840;

    await program.methods
      .registerInvestor(countryCodeUS, true, validitySeconds)
      .accounts({
        complianceOfficer: complianceOfficer.publicKey,
        assetConfig: assetConfigPda,
        investor: accreditedUSInvestor.publicKey,
        investorRegistry: accreditedRegistryPda,
        systemProgram: SystemProgram.programId,
      })
      .signers([complianceOfficer])
      .rpc();

    const registry = await program.account.investorRegistry.fetch(accreditedRegistryPda);
    expect(registry.isAccredited).to.be.true;
    expect(registry.countryCode).to.equal(840);
    expect(registry.isActive).to.be.true;
  });

  it("3. Registers Non-Accredited US Investor", async () => {
    const validitySeconds = new anchor.BN(365 * 24 * 3600);
    const countryCodeUS = 840;

    await program.methods
      .registerInvestor(countryCodeUS, false, validitySeconds)
      .accounts({
        complianceOfficer: complianceOfficer.publicKey,
        assetConfig: assetConfigPda,
        investor: nonAccreditedUSInvestor.publicKey,
        investorRegistry: nonAccreditedRegistryPda,
        systemProgram: SystemProgram.programId,
      })
      .signers([complianceOfficer])
      .rpc();

    const registry = await program.account.investorRegistry.fetch(nonAccreditedRegistryPda);
    expect(registry.isAccredited).to.be.false;
  });

  it("4. Fails registration if called by Unauthorized Non-Compliance Signer", async () => {
    const maliciousActor = Keypair.generate();
    const [randomPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("investor"), dummyMint.publicKey.toBuffer(), maliciousActor.publicKey.toBuffer()],
      program.programId
    );

    try {
      await program.methods
        .registerInvestor(840, true, new anchor.BN(3600))
        .accounts({
          complianceOfficer: maliciousActor.publicKey,
          assetConfig: assetConfigPda,
          investor: maliciousActor.publicKey,
          investorRegistry: randomPda,
          systemProgram: SystemProgram.programId,
        })
        .signers([maliciousActor])
        .rpc();
      expect.fail("Should have thrown unauthorized error");
    } catch (err: any) {
      expect(err.error.errorCode.code).to.equal("UnauthorizedComplianceSigner");
    }
  });

  it("5. Registers and Blocks Sanctioned Jurisdiction Investor", async () => {
    const sanctionedCountryCode = 999; // OFAC Blacklisted code simulation
    await program.methods
      .registerInvestor(sanctionedCountryCode, false, new anchor.BN(3600))
      .accounts({
        complianceOfficer: complianceOfficer.publicKey,
        assetConfig: assetConfigPda,
        investor: sanctionedForeignActor.publicKey,
        investorRegistry: sanctionedRegistryPda,
        systemProgram: SystemProgram.programId,
      })
      .signers([complianceOfficer])
      .rpc();

    const registry = await program.account.investorRegistry.fetch(sanctionedRegistryPda);
    expect(registry.countryCode).to.equal(999);
  });
});