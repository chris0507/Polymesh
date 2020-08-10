// Set options as a parameter, environment variable, or rc file.
require = require("esm")(module /*, options*/);
const assert = require("assert");
module.exports = require("../util/init.js");

let { reqImports } = require("../util/init.js");

// Sets the default exit code to fail unless the script runs successfully
process.exitCode = 1;

async function main() {

  const api = await reqImports.createApi();

  const testEntities = await reqImports.initMain(api);

  let primary_keys = await reqImports.generateKeys( api, 1, "primary7" );

  let issuer_dids = await reqImports.createIdentities( api, primary_keys, testEntities[0] );

  await reqImports.distributePolyBatch( api, primary_keys, reqImports.transfer_amount, testEntities[0] );

  await reqImports.issueTokenPerDid( api, primary_keys, "DEMOCR" );

  await createClaimRules( api, primary_keys, issuer_dids, "DEMOCR" );

  if (reqImports.fail_count > 0) {
    console.log("Failed");
  } else {
    console.log("Passed");
    process.exitCode = 0;
  }

  process.exit();
}

async function createClaimRules(api, accounts, dids, prepend) {

    const ticker = `token${prepend}0`.toUpperCase();
    assert( ticker.length <= 12, "Ticker cannot be longer than 12 characters");
    let senderRules = reqImports.senderRules1(accounts[0].address);
    let receiverRules = reqImports.receiverRules1(accounts[0].address);
    let nonceObj = {nonce: reqImports.nonces.get(accounts[0].address)};
    const transaction = await api.tx.complianceManager.addActiveRule(ticker, senderRules, receiverRules);
    const result = await reqImports.sendTransaction(transaction, accounts[0], nonceObj);
    const passed = result.findRecord('system', 'ExtrinsicSuccess');
    if (passed) reqImports.fail_count--;

    reqImports.nonces.set( accounts[0].address, reqImports.nonces.get(accounts[0].address).addn(1));

}

main().catch(console.error);
