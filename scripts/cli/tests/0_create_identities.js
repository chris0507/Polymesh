// Set options as a parameter, environment variable, or rc file.
require = require("esm")(module /*, options*/);
module.exports = require("../util/init.js");

let { reqImports } = require("../util/init.js");

async function main() {
  // Schema path
  const filePath = reqImports["path"].join(__dirname + "/../../../polymesh_schema.json");
  const customTypes = JSON.parse(reqImports["fs"].readFileSync(filePath, "utf8"));

  // Start node instance
  const ws_provider = new reqImports["WsProvider"]("ws://127.0.0.1:9944/");
  const api = await reqImports["ApiPromise"].create({
    types: customTypes,
    provider: ws_provider
  });

  const testEntities = await reqImports["initMain"](api);

  let keys = await reqImports["generateKeys"](api,5, "master");

  await createIdentities(api, testEntities, testEntities[0]);

  await reqImports["blockTillPoolEmpty"](api);

  await createIdentities(api, keys, testEntities[0]);

  await reqImports["distributePoly"]( api, keys, reqImports["transfer_amount"], testEntities[0] );

  await reqImports["blockTillPoolEmpty"](api);

  await new Promise(resolve => setTimeout(resolve, 3000));

  if (reqImports["fail_count"] > 0) {
    console.log("Failed");
    process.exitCode = 1;
  } else {
    console.log("Passed");
  }

  process.exit();
}

// Create a new DID for each of accounts[]
async function createIdentities(api, accounts, alice) {

    let dids = [];
      for (let i = 0; i < accounts.length; i++) {
        const unsub = await api.tx.identity
          .cddRegisterDid(accounts[i].address, null, [])
          .signAndSend(
            alice,
            { nonce: reqImports["nonces"].get(alice.address) },
            ({ events = [], status }) => {
              if (status.isFinalized) {
                reqImports["fail_count"] = reqImports["callback"](status, events, "identity", "NewDid", reqImports["fail_count"]);
                unsub();
              }
            }
          );

        reqImports["nonces"].set(alice.address, reqImports["nonces"].get(alice.address).addn(1));
      }
      await reqImports["blockTillPoolEmpty"](api);
      for (let i = 0; i < accounts.length; i++) {
        const d = await api.query.identity.keyToIdentityIds(accounts[i].publicKey);
        dids.push(d.raw.asUnique);
      }
      let did_balance = 10 * 10**12;
      for (let i = 0; i < dids.length; i++) {
        await api.tx.balances
          .topUpIdentityBalance(dids[i], did_balance)
          .signAndSend(
            alice,
            { nonce: reqImports["nonces"].get(alice.address) }
          );
        reqImports["nonces"].set(
          alice.address,
          reqImports["nonces"].get(alice.address).addn(1)
        );
      }
      return dids;

}

main().catch(console.error);
