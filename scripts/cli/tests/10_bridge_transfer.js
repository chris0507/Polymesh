// Set options as a parameter, environment variable, or rc file.
require = require("esm")(module /*, options*/);
module.exports = require("../util/init.js");

let { reqImports } = require("../util/init.js");

// Sets the default exit code to fail unless the script runs successfully
process.exitCode = 1;

async function main() {
  const api = await reqImports.createApi();

  const testEntities = await reqImports.initMain(api);

  let alice = testEntities[0];
  let relay = testEntities[4];

  let alice_did = JSON.parse(
    await reqImports.keyToIdentityIds(api, alice.publicKey)
  );

  let did_balance = 1000 * 10 ** 6;

  await reqImports.topUpIdentityBalance(
    api,
    alice,
    alice_did.Unique,
    did_balance
  );

  await acceptMultisigSignerAsKey(api, relay, 9);
 
  await reqImports.distributePolyBatch( api, [relay], reqImports.transfer_amount, alice );
  
  await bridgeTransfer(api, relay, alice);
  
  await freezeTransaction(api, alice);
  
  await sleep(50000).then(async() => { await unfreezeTransaction(api, alice); });
  
  if (reqImports.fail_count > 0) {
    console.log("Failed");
  } else {
    console.log("Passed");
    process.exitCode = 0;
  }

  process.exit();
}

async function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function acceptMultisigSignerAsKey(api, signer, authId) {
  // 1. Change Controller
  let nonceObj = { nonce: reqImports.nonces.get(signer.address) };

  const transaction = api.tx.multiSig.acceptMultisigSignerAsKey(authId);

  const result = await reqImports.sendTransaction(
    transaction,
    signer,
    nonceObj
  );

  const passed = result.findRecord("system", "ExtrinsicSuccess");
  if (passed) reqImports.fail_count--;

  reqImports.nonces.set(
    signer.address,
    reqImports.nonces.get(signer.address).addn(1)
  );
}

//  Propose Bridge Transaction
async function bridgeTransfer(api, signer, alice) {
  let amount = 10;
  let bridge_tx = {
    nonce: 2,
    recipient: alice.publicKey,
    amount,
    tx_hash: reqImports.u8aToHex(1, 256),
  };

  let nonceObj = { nonce: reqImports.nonces.get(signer.address) };
  const transaction = api.tx.bridge.proposeBridgeTx(bridge_tx);

  const result = await reqImports.sendTransaction(
    transaction,
    signer,
    nonceObj
  );

  const passed = result.findRecord("system", "ExtrinsicSuccess");
  if (passed) reqImports.fail_count--;

  reqImports.nonces.set(
    signer.address,
    reqImports.nonces.get(signer.address).addn(1)
  );
}

async function freezeTransaction(api, signer, alice) {

  let nonceObj = { nonce: reqImports.nonces.get(signer.address) };
  const transaction = api.tx.bridge.freeze();

  const result = await reqImports.sendTransaction(
    transaction,
    signer,
    nonceObj
  );

  const passed = result.findRecord("system", "ExtrinsicSuccess");
  if (passed) reqImports.fail_count--;

  reqImports.nonces.set(
    signer.address,
    reqImports.nonces.get(signer.address).addn(1)
  );

}

async function unfreezeTransaction(api, signer) {

  let nonceObj = { nonce: reqImports.nonces.get(signer.address) };
  const transaction = api.tx.bridge.unfreeze();

  const result = await reqImports.sendTransaction(
    transaction,
    signer,
    nonceObj
  );

  const passed = result.findRecord("system", "ExtrinsicSuccess");
  if (passed) reqImports.fail_count--;

  reqImports.nonces.set(
    signer.address,
    reqImports.nonces.get(signer.address).addn(1)
  );

}

main().catch(console.error);
