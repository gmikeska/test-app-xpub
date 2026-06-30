// emvault-jade — public entry point.
//
// A dependency-free Blockstream Jade driver for the browser, speaking Jade's
// CBOR-RPC protocol directly over the Web Serial API (USB). Covers Bitcoin
// (PSBT) and Liquid/Elements (PSET) workflows that `lwk_wasm` does not expose.
//
//   import { JadeRpc } from "@emvault/jade";
//
//   const jade = await JadeRpc.fromSerial();
//   await jade.unlock("testnet-liquid");
//   const xpub = await jade.getXpub("testnet-liquid", "m/84'/1'/0'");
//   const signed = await jade.signPset("testnet-liquid", psetBytes);
//   await jade.close();

export {
  JadeRpc,
  pathToU32Array,
  base58CheckDecode,
  bytesToHex,
  hexToBytes,
  base64ToBytes,
  bytesToBase64,
} from "./jade-rpc.js";
