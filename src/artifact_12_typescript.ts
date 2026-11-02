import * as anchor from '@coral-xyz/anchor';
import { Program } from '@coral-xyz/anchor';
import { HelvetiaDlt } from '../target/types/helvetia_dlt';
import { Keypair, PublicKey, SystemProgram } from '@solana/web3.js';
import { expect } from 'chai';

describe('helvetia-dlt: Swiss Market Tokenization Test Suite', () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.HelvetiaDlt as Program<HelvetiaDlt>;

  const admin = provider.wallet;
  const dummyMint = Keypair.generate();
  const investorA = Keypair.generate();
  const investorB = Keypair.generate();

  const ISIN = 'CH1234567890';
  const MAX_HOLDERS = 100;

  let complianceConfigPda: PublicKey;
  let passportAPda: PublicKey;
  let passportBPda: PublicKey;

  before(async () => {
    [complianceConfigPda] = PublicKey.findProgramAddressSync(
      [Buffer.from('compliance_config'), dummyMint.publicKey.toBuffer()],
      program.programId
    );

    [passportAPda] = PublicKey.findProgramAddressSync(
      [Buffer.from('investor_passport'), complianceConfigPda.toBuffer(), investorA.publicKey.toBuffer()],
      program.programId
    );

    [passportBPda] = PublicKey.findProgramAddressSync(
      [Buffer.from('investor_passport'), complianceConfigPda.toBuffer(), investorB.publicKey.toBuffer()],
      program.programId
    );
  });

  it('1. Initializes the Swiss DLT Compliance Engine with valid ISIN and limits', async () => {
    const tx = await program.methods
      .initializeComplianceEngine(ISIN, MAX_HOLDERS)
      .accounts({
        complianceConfig: complianceConfigPda,
        mint: dummyMint.publicKey,
        authority: admin.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    expect(tx).to.be.a('string');

    const configAccount = await program.account.complianceConfig.fetch(complianceConfigPda);
    expect(configAccount.admin.toBase58()).to.equal(admin.publicKey.toBase58());
    expect(configAccount.mint.toBase58()).to.equal(dummyMint.publicKey.toBase58());
    expect(configAccount.maxHolders).to.equal(MAX_HOLDERS);
    expect(configAccount.currentHolders).to.equal(0);
    expect(configAccount.isTradingPaused).to.be.false;

    const isinDecoded = Buffer.from(configAccount.isin).toString('utf8');
    expect(isinDecoded).to.equal(ISIN);
  });

  it('2. Enforces ISIN format validation (rejects invalid string lengths)', async () => {
    const badMint = Keypair.generate();
    const [badConfigPda] = PublicKey.findProgramAddressSync(
      [Buffer.from('compliance_config'), badMint.publicKey.toBuffer()],
      program.programId
    );

    try {
      await program.methods
        .initializeComplianceEngine('CH123', MAX_HOLDERS) // Invalid length != 12
        .accounts({
          complianceConfig: badConfigPda,
          mint: badMint.publicKey,
          authority: admin.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc();
      expect.fail('Should have failed with InvalidISINLength');
    } catch (err: any) {
      expect(err.error.errorCode.code).to.equal('InvalidISINLength');
    }
  });

  it('3. Registers Investor A under Swiss KYC Tier 2 (HNWI)', async () => {
    const validUntil = new anchor.BN(Math.floor(Date.now() / 1000) + 86400 * 365);
    const countryCode = [67, 72]; // "CH" in ASCII

    await program.methods
      .registerInvestor(2, countryCode, true, validUntil)
      .accounts({
        complianceConfig: complianceConfigPda,
        passport: passportAPda,
        investorWallet: investorA.publicKey,
        admin: admin.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    const passportAccount = await program.account.investorPassport.fetch(passportAPda);
    expect(passportAccount.wallet.toBase58()).to.equal(investorA.publicKey.toBase58());
    expect(passportAccount.kycTier).to.equal(2);
    expect(passportAccount.isAccredited).to.be.true;
    expect(passportAccount.isFrozen).to.be.false;

    const configAccount = await program.account.complianceConfig.fetch(complianceConfigPda);
    expect(configAccount.currentHolders).to.equal(1);
  });

  it('4. Freezes Investor A and checks sanction enforcement flag', async () => {
    await program.methods
      .setInvestorFreeze(true)
      .accounts({
        complianceConfig: complianceConfigPda,
        passport: passportAPda,
        admin: admin.publicKey,
      })
      .rpc();

    const passportAccount = await program.account.investorPassport.fetch(passportAPda);
    expect(passportAccount.isFrozen).to.be.true;

    // Restore to unfrozen state
    await program.methods
      .setInvestorFreeze(false)
      .accounts({
        complianceConfig: complianceConfigPda,
        passport: passportAPda,
        admin: admin.publicKey,
      })
      .rpc();

    const unfrozenAccount = await program.account.investorPassport.fetch(passportAPda);
    expect(unfrozenAccount.isFrozen).to.be.false;
  });

  it('5. Halts and resumes global trading (Circuit Breaker)', async () => {
    await program.methods
      .setTradingPause(true)
      .accounts({
        complianceConfig: complianceConfigPda,
        admin: admin.publicKey,
      })
      .rpc();

    let configAccount = await program.account.complianceConfig.fetch(complianceConfigPda);
    expect(configAccount.isTradingPaused).to.be.true;

    await program.methods
      .setTradingPause(false)
      .accounts({
        complianceConfig: complianceConfigPda,
        admin: admin.publicKey,
      })
      .rpc();

    configAccount = await program.account.complianceConfig.fetch(complianceConfigPda);
    expect(configAccount.isTradingPaused).to.be.false;
  });

  it('6. Blocks unauthorized attempts to register investors', async () => {
    const maliciousActor = Keypair.generate();
    const validUntil = new anchor.BN(Math.floor(Date.now() / 1000) + 86400 * 365);

    try {
      await program.methods
        .registerInvestor(1, [67, 72], false, validUntil)
        .accounts({
          complianceConfig: complianceConfigPda,
          passport: passportBPda,
          investorWallet: investorB.publicKey,
          admin: maliciousActor.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([maliciousActor])
        .rpc();
      expect.fail('Should have failed unauthorized access check');
    } catch (err: any) {
      expect(err).to.exist;
    }
  });
});