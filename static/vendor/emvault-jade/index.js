// emvault-jade — public entry point.
//
// A dependency-free Blockstream Jade driver for the browser, speaking Jade's
// CBOR-RPC protocol directly over the Web Serial API (USB). It covers Bitcoin
// PSBT signing and full device onboarding (xpub, master fingerprint, multisig
// registration).
//
//   import { JadeRpc } from "@emvault/jade";
//
//   const jade = await JadeRpc.fromSerial();       // inside a click handler
//   await jade.unlock("testnet");
//   const xpub = await jade.getXpub("testnet", "m/84'/1'/0'");
//   const signedPsbt = await jade.signPsbt("testnet", psbtBytes);
//   await jade.close();

export {
  JadeRpc,
  NETWORKS,
  pathToU32Array,
  base58CheckDecode,
  bytesToHex,
  hexToBytes,
  base64ToBytes,
  bytesToBase64,
} from "./jade-rpc.js";
