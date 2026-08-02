/* tslint:disable */
/* eslint-disable */
export function searchLedgerDevice(): Promise<HIDDevice>;
/**
 * Convert the given string to a QR code image uri
 *
 * The image format is monocromatic bitmap, returned as an encoded in base64 uri.
 *
 * Without `pixel_per_module` the default is no border, and 1 pixel per module, to be used
 * for example in html: `style="image-rendering: pixelated; border: 20px solid white;"`
 */
export function stringToQr(str: string, pixel_per_module?: number | null): string;
/**
 * Wallet chain
 */
export enum Chain {
  /**
   * External address, shown when asked for a payment.
   * Wallet having a single descriptor are considered External
   */
  External = 0,
  /**
   * Internal address, used for the change
   */
  Internal = 1,
}
/**
 * An Elements (Liquid) address
 */
export class Address {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates an `Address`
   *
   * If you know the network, you can use `parse()` to validate that the network is consistent.
   */
  constructor(s: string);
  /**
   * Parses an `Address` ensuring is for the right network
   */
  static parse(s: string, network: Network): Address;
  /**
   * Return the script pubkey of the address.
   */
  scriptPubkey(): Script;
  /**
   * Return true if the address is blinded, in other words, if it has a blinding key.
   */
  isBlinded(): boolean;
  /**
   * Return true if the address is for mainnet.
   */
  isMainnet(): boolean;
  /**
   * Return the unconfidential address, in other words, the address without the blinding key.
   */
  toUnconfidential(): Address;
  /**
   * Return the string representation of the address.
   * This representation can be used to recreate the address via `new()`
   */
  toString(): string;
  /**
   * Returns a string encoding an image in a uri
   *
   * The string can be open in the browser or be used as `src` field in `img` in HTML
   *
   * For max efficiency we suggest to pass `None` to `pixel_per_module`, get a very small image
   * and use styling to scale up the image in the browser. eg
   * `style="image-rendering: pixelated; border: 20px solid white;"`
   */
  QRCodeUri(pixel_per_module?: number | null): string;
  /**
   * Returns a string of the QR code printable in a terminal environment
   */
  QRCodeText(): string;
}
/**
 * Value returned from asking an address to the wallet.
 * Containing the confidential address and its
 * derivation index (the last element in the derivation path)
 */
export class AddressResult {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Return the address.
   */
  address(): Address;
  /**
   * Return the derivation index of the address.
   */
  index(): number;
}
/**
 * Context for actions related to an AMP0 (sub)account
 *
 * <div class="warning">
 * <b>WARNING:</b>
 *
 * AMP0 is based on a legacy system, and some things do not fit precisely the way LWK allows to do
 * things.
 *
 * Callers must be careful with the following:
 * * <b>Addresses: </b>
 *   to get addresses use [`Amp0::address()`]. This ensures
 *   that all addresses used are correctly monitored by the AMP0 server.
 * * <b>Syncing: </b>
 *   to sync the AMP0 [`crate::Wollet`], use [`Amp0::last_index()`] and [`crate::clients::blocking::BlockchainBackend::full_scan_to_index()`]. This ensures that all utxos are synced, even if there are gaps between higher than the GAP LIMIT.
 *
 * <i>
 * Failing to do the above might lead to inconsistent states, where funds are not shown or they
 * cannot be spent!
 * </i>
 * </div>
 */
export class Amp0 {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Create a new AMP0 context for the specified network
   */
  static newWithNetwork(network: Network, username: string, password: string, amp_id: string): Promise<Amp0>;
  /**
   * Create a new AMP0 context for testnet
   */
  static newTestnet(username: string, password: string, amp_id: string): Promise<Amp0>;
  /**
   * Create a new AMP0 context for mainnet
   */
  static newMainnet(username: string, password: string, amp_id: string): Promise<Amp0>;
  /**
   * Index of the last returned address
   *
   * Use this and [`crate::EsploraClient::full_scan_to_index()`] to sync the `Wollet`
   */
  lastIndex(): number;
  /**
   * AMP ID
   */
  ampId(): string;
  /**
   * Get an address
   *
   * If `index` is None, a new address is returned.
   */
  address(index?: number | null): Promise<AddressResult>;
  /**
   * The LWK watch-only wallet corresponding to the AMP0 (sub)account.
   */
  wollet(): Wollet;
  /**
   * Ask AMP0 server to cosign
   */
  sign(amp0pset: Amp0Pset): Promise<Transaction>;
}
/**
 * Session connecting to AMP0
 */
export class Amp0Connected {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Connect and register to AMP0.
   */
  static connect(network: Network, signer_data: Amp0SignerData): Promise<Amp0Connected>;
  /**
   * Obtain a login challenge
   *
   * This must be signed with [`amp0_sign_challenge()`].
   */
  getChallenge(): Promise<string>;
  /**
   * Log in
   *
   * `sig` must be obtained from [`amp0_sign_challenge()`] called with the value returned
   * by [`Amp0Connected::get_challenge()`]
   */
  login(sig: string): Promise<Amp0LoggedIn>;
}
/**
 * Session logged in AMP0
 */
export class Amp0LoggedIn {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * List of AMP IDs.
   */
  getAmpIds(): string[];
  /**
   * Get the next account for AMP0 account creation
   *
   * This must be given to [`amp0_account_xpub()`] to obtain the xpub to pass to
   * [`Amp0LoggedIn::create_amp0_account()`]
   */
  nextAccount(): number;
  /**
   * Create a new AMP0 account
   *
   * `account_xpub` must be obtained from [`amp0_account_xpub()`] called with the value obtained from
   * [`Amp0LoggedIn::next_account()`]
   */
  createAmp0Account(pointer: number, account_xpub: string): Promise<string>;
  /**
   * Create a new Watch-Only entry for this wallet
   */
  createWatchOnly(username: string, password: string): Promise<void>;
}
/**
 * A PSET to use with AMP0
 *
 * When asking AMP0 to cosign, the caller must pass some extra data that does not belong to the
 * PSET. This struct holds and manage the necessary data.
 *
 * If you're not dealing with AMP0, do not use this struct.
 */
export class Amp0Pset {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates a `Amp0Pset`
   */
  constructor(pset: Pset, blinding_nonces: string[]);
  /**
   * Get the PSET
   */
  pset(): Pset;
  /**
   * Get the blinding nonces
   */
  blindingNonces(): string[];
}
export class Amp0SignerData {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
}
/**
 * Context for actions interacting with Asset Management Platform version 2
 */
export class Amp2 {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Create a new AMP2 client
   *
   *  * `server_key` - The keyorigin xpub of the AMP2 server key
   *  * `url` - The URL of the AMP2 server
   */
  static new(server_key: string, url: string): Amp2;
  /**
   * Create a new AMP2 client with the default url and server key for the testnet network.
   */
  static newTestnet(): Amp2;
  /**
   * Get an AMP2 wallet descriptor from the keyorigin xpub string obtained from a signer
   */
  descriptorFromStr(keyorigin_xpub: string, descriptor_blinding_key: string): Amp2Descriptor;
  /**
   * Register an AMP2 wallet with the AMP2 server
   */
  register(desc: Amp2Descriptor): Promise<string>;
  /**
   * Ask the AMP2 server to cosign a PSET
   */
  cosign(pset: Pset): Promise<Pset>;
}
/**
 * An Asset Management Platform version 2 descriptor
 */
export class Amp2Descriptor {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Return the descriptor as a `WolletDescriptor`
   */
  descriptor(): WolletDescriptor;
  /**
   * Return the string representation of the descriptor.
   */
  toString(): string;
  /**
   * Create an `Amp2Descriptor` using any `WolletDescriptor`
   *
   * Warning: AMP2 server only supports a limited subset of descriptors.
   * To make sure this AMP2 descriptor can be used safely,
   * register this with AMP2 as soon as possible.
   */
  static newWithCustomDescriptor(desc: WolletDescriptor): Amp2Descriptor;
}
/**
 * An asset identifier and an amount in satoshi units
 */
export class AssetAmount {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  amount(): bigint;
  asset(): AssetId;
}
/**
 * A blinding factor for asset commitments.
 */
export class AssetBlindingFactor {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates an `AssetBlindingFactor` from a string.
   */
  static fromString(s: string): AssetBlindingFactor;
  /**
   * Creates an `AssetBlindingFactor` from a byte slice.
   */
  static fromBytes(bytes: Uint8Array): AssetBlindingFactor;
  /**
   * Returns a zero asset blinding factor.
   */
  static zero(): AssetBlindingFactor;
  /**
   * Returns the bytes (32 bytes) in little-endian byte order.
   *
   * This is the internal representation used by secp256k1. The byte order is
   * reversed compared to the hex string representation (which uses big-endian,
   * following Bitcoin display conventions).
   */
  toBytes(): Uint8Array;
  /**
   * Returns string representation of the ABF
   */
  toString(): string;
}
/**
 * A valid asset identifier.
 *
 * 32 bytes encoded as hex string.
 */
export class AssetId {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates an `AssetId`
   *
   * Deprecated: use `from_string()` instead
   */
  constructor(asset_id: string);
  /**
   * Creates an `AssetId` from hex string
   */
  static fromString(s: string): AssetId;
  /**
   * Creates an `AssetId` from a bytes.
   */
  static fromBytes(bytes: Uint8Array): AssetId;
  /**
   * Returns the `AssetId` bytes in little-endian byte order.
   */
  toBytes(): Uint8Array;
  /**
   * Return the string representation of the asset identifier (64 hex characters).
   * This representation can be used to recreate the asset identifier via `fromString()`
   */
  toString(): string;
}
/**
 * An ordered collection of asset identifiers.
 */
export class AssetIds {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Return an empty list of asset identifiers.
   */
  static empty(): AssetIds;
  /**
   * Return the string representation of this list of asset identifiers.
   */
  toString(): string;
}
/**
 * Data related to an asset in the registry:
 * - contract: the contract of the asset
 * - tx: the issuance transaction of the asset
 */
export class AssetMeta {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Return the contract of the asset.
   */
  contract(): Contract;
  /**
   * Return the issuance transaction of the asset.
   */
  tx(): Transaction;
}
/**
 * A signed balance of assets, to represent a balance with negative values such
 * as the results of a transaction from the perspective of a wallet.
 */
export class Balance {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Convert the balance to a JsValue for serialization
   *
   * Note: the amounts are strings since `JSON.stringify` cannot handle `BigInt`s.
   * Use `entries()` to get the raw data.
   */
  toJSON(): any;
  /**
   * Returns the balance as a JavaScript `Map` of asset id to amount.
   */
  entries(): any;
  /**
   * Return the string representation of the balance.
   */
  toString(): string;
}
/**
 * The bip variant for a descriptor like specified in the bips (49, 84, 86, 87)
 */
export class Bip {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates a bip49 variant
   */
  static bip49(): Bip;
  /**
   * Creates a bip84 variant
   */
  static bip84(): Bip;
  /**
   * Creates a bip87 variant
   */
  static bip87(): Bip;
  /**
   * Creates a bip86 variant
   */
  static bip86(): Bip;
  /**
   * Return the string representation of the bip variant, such as "bip49", "bip84" or "bip87"
   */
  toString(): string;
}
/**
 * Wrapper over [`lwk_boltz::BoltzSession`]
 */
export class BoltzSession {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Get the rescue file
   */
  rescueFile(): string;
  /**
   * Prepare a lightning invoice payment
   */
  preparePay(lightning_payment: LightningPayment, refund_address: Address): Promise<PreparePayResponse>;
  /**
   * Fetch a BOLT12 invoice without creating or starting a swap
   */
  fetchBolt12Invoice(lightning_payment: LightningPayment): Promise<Invoice>;
  /**
   * Create a lightning invoice for receiving payment
   */
  invoice(amount: bigint, description: string | null | undefined, claim_address: Address): Promise<InvoiceResponse>;
  /**
   * Restore a swap from its serialized data
   */
  restorePreparePay(data: string): Promise<PreparePayResponse>;
  /**
   * Restore a swap from its serialized data
   */
  restoreInvoice(data: string): Promise<InvoiceResponse>;
}
/**
 * Wrapper over [`lwk_boltz::BoltzSessionBuilder`]
 */
export class BoltzSessionBuilder {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Create a new BoltzSessionBuilder with the given network
   *
   * This creates a builder with default Esplora client for the network.
   */
  constructor(network: Network, esplora_client: EsploraClient);
  /**
   * Set the timeout for creating swaps
   *
   * If not set, the default timeout of 10 seconds is used.
   */
  createSwapTimeout(timeout_seconds: bigint): BoltzSessionBuilder;
  /**
   * Set the timeout for the advance call
   *
   * If not set, the default timeout of 3 minutes is used.
   */
  timeoutAdvance(timeout_seconds: bigint): BoltzSessionBuilder;
  /**
   * Set the mnemonic for deriving swap keys
   *
   * If not set, a new random mnemonic will be generated.
   */
  mnemonic(mnemonic: Mnemonic): BoltzSessionBuilder;
  /**
   * Set the polling flag
   *
   * If true, the advance call will not await on the websocket connection returning immediately
   * even if there is no update, thus requiring the caller to poll for updates.
   *
   * If true, the timeout_advance will be ignored even if set.
   */
  polling(polling: boolean): BoltzSessionBuilder;
  /**
   * Set the next index to use for deriving keypairs
   *
   * Avoid a call to the boltz API to recover this information.
   *
   * When the mnemonic is not set, this is ignored.
   */
  nextIndexToUse(next_index_to_use: number): BoltzSessionBuilder;
  /**
   * Set the referral id for the BoltzSession
   */
  referralId(referral_id: string): BoltzSessionBuilder;
  /**
   * Set the Boltz API base URL
   *
   * The caller is responsible for ensuring the provider behind this URL matches the session
   * network passed to the builder.
   *
   * If this is used together with a persistent store on the Rust side, the caller must use a
   * different store per provider. Persisted swap data is not namespaced by provider, so
   * reusing the same store across different `apiUrl` values can mix swaps from different
   * providers.
   */
  apiUrl(api_url: string): BoltzSessionBuilder;
  /**
   * Set the url of the bitcoin electrum client
   */
  bitcoinElectrumClient(bitcoin_electrum_client: string): BoltzSessionBuilder;
  /**
   * Set the random preimages flag
   *
   * The default is false, the preimages will be deterministic and the rescue file will be
   * compatible with the Boltz web app.
   * If true, the preimages will be random potentially allowing concurrent sessions with the same
   * mnemonic, but completing the swap will be possible only with the preimage data. For example
   * the boltz web app will be able only to refund the swap, not to bring it to completion.
   * If true, when serializing the swap data, the preimage will be saved in the data.
   */
  randomPreimages(random_preimages: boolean): BoltzSessionBuilder;
  /**
   * Build the BoltzSession
   */
  build(): Promise<BoltzSession>;
}
/**
 * A contract defining metadata of an asset such the name and the ticker
 */
export class Contract {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates a `Contract`
   */
  constructor(domain: string, issuer_pubkey: string, name: string, precision: number, ticker: string, version: number);
  /**
   * Return the string representation of the contract.
   */
  toString(): string;
  /**
   * Return the domain of the issuer of the contract.
   */
  domain(): string;
  /**
   * Make a copy of the contract.
   *
   * This is needed to pass it to a function that requires a `Contract` (without borrowing)
   * but you need the same contract after that call.
   */
  clone(): Contract;
}
export class CurrencyCode {
  free(): void;
  [Symbol.dispose](): void;
  constructor(code: string);
  name(): string;
  alpha3(): string;
  exp(): number;
}
/**
 * A blockchain backend implementation based on the
 * [esplora HTTP API](https://github.com/blockstream/esplora/blob/master/API.md).
 * But can also use the [waterfalls](https://github.com/RCasatta/waterfalls)
 * endpoint to speed up the scan if supported by the server.
 */
export class EsploraClient {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates an Esplora client with the given options
   */
  constructor(network: Network, url: string, waterfalls: boolean, concurrency: number, utxo_only: boolean);
  /**
   * Scan the blockchain for the scripts generated by a watch-only wallet
   *
   * This method scans both external and internal address chains, stopping after finding
   * 20 consecutive unused addresses (the gap limit) as recommended by
   * [BIP44](https://github.com/bitcoin/bips/blob/master/bip-0044.mediawiki#address-gap-limit).
   *
   * Returns `Some(Update)` if any changes were found during scanning, or `None` if no changes
   * were detected.
   *
   * To scan beyond the gap limit use `full_scan_to_index()` instead.
   */
  fullScan(wollet: Wollet): Promise<Update | undefined>;
  /**
   * Scan the blockchain for the scripts generated by a watch-only wallet up to a specified derivation index
   *
   * While `full_scan()` stops after finding 20 consecutive unused addresses (the gap limit),
   * this method will scan at least up to the given derivation index. This is useful to prevent
   * missing funds in cases where outputs exist beyond the gap limit.
   *
   * Will scan both external and internal address chains up to the given index for maximum safety,
   * even though internal addresses may not need such deep scanning.
   *
   * If transactions are found beyond the gap limit during this scan, subsequent calls to
   * `full_scan()` will automatically scan up to the highest used index, preventing any
   * previously-found transactions from being missed.
   *
   * See `full_scan_to_index()` for a blocking version of this method.
   */
  fullScanToIndex(wollet: Wollet, index: number): Promise<Update | undefined>;
  /**
   * Broadcast a transaction to the network so that a miner can include it in a block.
   */
  broadcastTx(tx: Transaction): Promise<Txid>;
  /**
   * Broadcast a PSET by extracting the transaction from the PSET and broadcasting it.
   */
  broadcast(pset: Pset): Promise<Txid>;
  /**
   * Set the waterfalls server recipient key. This is used to encrypt the descriptor when calling the waterfalls endpoint.
   */
  setWaterfallsServerRecipient(recipient: string): Promise<void>;
  /**
   * Return the descriptor string to use with Waterfalls descriptor endpoints.
   *
   * This is a temporary API exposed to let callers use Waterfalls subscription
   * endpoints directly. It may be removed once subscription support is
   * implemented in LWK.
   *
   * The returned descriptor has key origin information stripped and is encrypted
   * for the Waterfalls server recipient unless descriptor encryption has been
   * explicitly disabled on this client.
   */
  waterfallsDescriptor(descriptor: WolletDescriptor): Promise<string>;
  /**
   * Query the last used derivation index for a wallet's descriptor from the waterfalls server.
   *
   * This method queries the waterfalls `/v1/last_used_index` endpoint to get the last used
   * derivation index for both external and internal chains of the wallet's descriptor.
   *
   * Returns `LastUsedIndexResponse` containing the last used indexes and the tip block hash.
   *
   * # Errors
   *
   * Returns an error if this client was not configured with waterfalls support,
   * if the descriptor does not contain a wildcard,
   * or if the descriptor uses ELIP151 blinding.
   */
  lastUsedIndex(descriptor: WolletDescriptor): Promise<LastUsedIndexResponse>;
}
/**
 * Multiple exchange rates against BTC provided from various sources
 */
export class ExchangeRates {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Get the median exchange rate
   */
  median(): number;
  /**
   * Get the individual exchange rates as a JSON array
   *
   * Each rate contains: rate, currency, source, and timestamp
   */
  results(): any;
  /**
   * Get the number of sources that provided rates
   */
  resultsCount(): number;
  /**
   * Serialize the entire response to JSON string
   */
  serialize(): string;
}
/**
 * An external UTXO, owned by another wallet.
 */
export class ExternalUtxo {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Construct an ExternalUtxo
   */
  constructor(vout: number, tx: Transaction, unblinded: TxOutSecrets, max_weight_to_satisfy: number, is_segwit: boolean);
}
/**
 * The total fee paid by the transaction for each asset type.
 */
export class Fees {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Returns the fees as a JavaScript `Map` of asset id to amount.
   */
  entries(): any;
  /**
   * Note: the amounts are strings since `JSON.stringify` cannot handle `BigInt`s.
   * Use `entries()` to get the raw data.
   */
  toJSON(): any;
  /**
   * Return the string representation of the fee.
   */
  toString(): string;
}
/**
 * Wrapper over [`lwk_boltz::Invoice`]
 */
export class Invoice {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Return a string representation of the invoice
   */
  toString(): string;
  /**
   * Return the invoice amount in whole satoshis
   */
  amountSats(): bigint;
  /**
   * Return true if this is a BOLT11 invoice
   */
  isBolt11(): boolean;
  /**
   * Return true if this is a BOLT12 invoice
   */
  isBolt12(): boolean;
  /**
   * Return the BOLT11 invoice string if this is a BOLT11 invoice
   */
  bolt11Invoice(): string | undefined;
  /**
   * Return the BOLT12 invoice string if this is a BOLT12 invoice
   */
  bolt12Invoice(): string | undefined;
}
/**
 * Wrapper over [`lwk_boltz::InvoiceResponse`]
 */
export class InvoiceResponse {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Serialize the response to JSON string for JS interop
   */
  serialize(): string;
  /**
   * Return the bolt11 invoice string
   */
  bolt11Invoice(): string;
  swapId(): string;
  /**
   * The fee of the swap provider
   *
   * It is equal to the amount of the invoice minus the amount of the onchain transaction.
   * Does not include the fee of the onchain transaction.
   */
  fee(): bigint | undefined;
  /**
   * Complete the payment by advancing through the swap states until completion or failure
   * Consumes self as the inner method does
   */
  completePay(): Promise<boolean>;
}
/**
 * The details of an issuance or reissuance.
 */
export class Issuance {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Return the asset id or None if it's a null issuance
   */
  asset(): AssetId | undefined;
  /**
   * Return the token id or None if it's a null issuance
   */
  token(): AssetId | undefined;
  /**
   * Return the previous output index or None if it's a null issuance
   */
  prevVout(): number | undefined;
  /**
   * Return the previous transaction id or None if it's a null issuance
   */
  prevTxid(): Txid | undefined;
  /**
   * Return true if this is effectively an issuance
   */
  isIssuance(): boolean;
  /**
   * Return true if this is effectively a reissuance
   */
  isReissuance(): boolean;
}
/**
 * A Jade hardware wallet usable via Web Serial
 */
export class Jade {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates a Jade from Web Serial for the given network.
   *
   * When filter is true, it will filter available serial with Blockstream released chips, use
   * false if you don't see your DYI jade
   */
  static fromSerial(network: Network, filter: boolean): Promise<Jade>;
  getVersion(): Promise<any>;
  getMasterXpub(): Promise<Xpub>;
  /**
   * Return a single sig address with the given `variant` and `path` derivation
   */
  getReceiveAddressSingle(variant: Singlesig, path: Uint32Array): Promise<string>;
  /**
   * Return a multisig address of a registered `multisig_name` wallet
   *
   * This method accept `path` and `path_n` in place of a single `Vec<Vec<u32>>` because the
   * latter is not supported by wasm_bindgen (and neither `(u32, Vec<u32>)`). `path` and `path_n`
   * are converted internally to a `Vec<Vec<u32>>` with the caveat all the paths are the same,
   * which is almost always the case.
   */
  getReceiveAddressMulti(multisig_name: string, path: Uint32Array): Promise<string>;
  /**
   * Sign and consume the given PSET, returning the signed one
   */
  sign(pset: Pset): Promise<Pset>;
  wpkh(): Promise<WolletDescriptor>;
  shWpkh(): Promise<WolletDescriptor>;
  multi(name: string): Promise<WolletDescriptor>;
  getRegisteredMultisigs(): Promise<any>;
  keyoriginXpub(bip: Bip): Promise<string>;
  registerDescriptor(name: string, desc: WolletDescriptor): Promise<boolean>;
}
/**
 * WebSocket-based `Jade` useful for testing in the browser with the Jade emulator.
 */
export class JadeWebSocket {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates a Jade from WebSocket for the given network
   *
   * The url should point to your WebSocket bridge that connects to the Docker Jade emulator
   */
  static fromWebSocket(network: Network, url: string): Promise<JadeWebSocket>;
  getVersion(): Promise<any>;
  getMasterXpub(): Promise<Xpub>;
  /**
   * Return a single sig address with the given `variant` and `path` derivation
   */
  getReceiveAddressSingle(variant: Singlesig, path: Uint32Array): Promise<string>;
  /**
   * Return a multisig address of a registered `multisig_name` wallet
   *
   * This method accept `path` and `path_n` in place of a single `Vec<Vec<u32>>` because the
   * latter is not supported by wasm_bindgen (and neither `(u32, Vec<u32>)`). `path` and `path_n`
   * are converted internally to a `Vec<Vec<u32>>` with the caveat all the paths are the same,
   * which is almost always the case.
   */
  getReceiveAddressMulti(multisig_name: string, path: Uint32Array): Promise<string>;
  /**
   * Sign and consume the given PSET, returning the signed one
   */
  sign(pset: Pset): Promise<Pset>;
  wpkh(): Promise<WolletDescriptor>;
  shWpkh(): Promise<WolletDescriptor>;
  multi(name: string): Promise<WolletDescriptor>;
  getRegisteredMultisigs(): Promise<any>;
  keyoriginXpub(bip: Bip): Promise<string>;
  registerDescriptor(name: string, desc: WolletDescriptor): Promise<boolean>;
}
/**
 * A bridge that connects a [`JsStorage`] to [`lwk_common::Store`].
 */
export class JsStoreLink {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Create a new `JsStoreLink` from a JavaScript storage object.
   *
   * The JS object must have `get(key)`, `put(key, value)`, `remove(key)`,
   * and `isPersisted()` methods.
   */
  constructor(storage: any);
}
/**
 * Test helper to verify Rust can read/write through a JS store.
 */
export class JsTestStore {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Create a new test helper wrapping the given JS storage.
   */
  constructor(storage: any);
  /**
   * Write a key-value pair to the store.
   */
  write(key: string, value: Uint8Array): void;
  /**
   * Read a value from the store.
   */
  read(key: string): Uint8Array | undefined;
  /**
   * Remove a key from the store.
   */
  remove(key: string): void;
}
/**
 * Response from the last_used_index endpoint
 *
 * Returns the highest derivation index that has been used (has transaction history)
 * for both external and internal chains. This is useful for quickly determining
 * the next unused address without downloading full transaction history.
 */
export class LastUsedIndexResponse {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Last used index on the external (receive) chain, or undefined if no addresses have been used.
   */
  readonly external: number | undefined;
  /**
   * Last used index on the internal (change) chain, or undefined if no addresses have been used.
   */
  readonly internal: number | undefined;
  /**
   * Current blockchain tip hash for reference.
   */
  readonly tip: string | undefined;
}
export class LedgerWeb {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * hid_device must be already opened
   */
  constructor(hid_device: HIDDevice, network: Network);
  getVersion(): Promise<string>;
  deriveXpub(path: string): Promise<string>;
  slip77MasterBlindingKey(): Promise<string>;
  fingerprint(): Promise<string>;
  /**
   * TODO Should use Signer::wpkh_slip77_descriptor
   */
  wpkhSlip77Descriptor(): Promise<WolletDescriptor>;
  /**
   * Sign and consume the given PSET, returning the signed one
   */
  sign(pset: Pset): Promise<Pset>;
  /**
   * Return a single sig address with the given `variant` and `index`
   */
  getReceiveAddressSingle(index: number): Promise<string>;
}
/**
 * Wrapper over [`lwk_boltz::LightningPayment`]
 */
export class LightningPayment {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Create a LightningPayment from a bolt11 invoice string or a bolt12 offer
   */
  constructor(invoice: string);
  /**
   * Return a string representation of the LightningPayment
   */
  toString(): string;
  /**
   * Return a QR code image uri for the LightningPayment
   */
  toUriQr(pixel_per_module?: number | null): string;
}
/**
 * A struct representing a magic routing hint, with details on how to pay directly without using Boltz
 */
export class MagicRoutingHint {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * The address to pay directly to
   */
  address(): string;
  /**
   * The amount to pay directly to
   */
  amount(): bigint;
  /**
   * The URI to pay directly to
   */
  uri(): string;
}
/**
 * A mnemonic secret code used as a master secret for a bip39 wallet.
 *
 * Supported number of words are 12, 15, 18, 21, and 24.
 */
export class Mnemonic {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates a Mnemonic
   */
  constructor(s: string);
  /**
   * Return the string representation of the Mnemonic.
   * This representation can be used to recreate the Mnemonic via `new()`
   *
   * Note this is secret information, do not log it.
   */
  toString(): string;
  /**
   * Creates a Mnemonic from entropy, at least 16 bytes are needed.
   */
  static fromEntropy(b: Uint8Array): Mnemonic;
  /**
   * Creates a random Mnemonic of given words (12,15,18,21,24)
   */
  static fromRandom(word_count: number): Mnemonic;
}
/**
 * The network of the elements blockchain such as mainnet, testnet or regtest.
 */
export class Network {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates a mainnet `Network``
   */
  static mainnet(): Network;
  /**
   * Creates a testnet `Network``
   */
  static testnet(): Network;
  /**
   * Creates a regtest `Network``
   */
  static regtest(policy_asset: AssetId): Network;
  /**
   * Creates the default regtest `Network` with the policy asset `5ac9f65c0efcc4775e0baec4ec03abdde22473cd3cf33c0419ca290e0751b225`
   */
  static regtestDefault(): Network;
  /**
   * Return the default esplora client for this network
   */
  defaultEsploraClient(): EsploraClient;
  /**
   * Return true if the network is a mainnet network
   */
  isMainnet(): boolean;
  /**
   * Return true if the network is a testnet network
   */
  isTestnet(): boolean;
  /**
   * Return true if the network is a regtest network
   */
  isRegtest(): boolean;
  /**
   * Return a string representation of the network, like "liquid", "liquid-testnet" or "liquid-regtest"
   */
  toString(): string;
  /**
   * Return the policy asset for this network
   */
  policyAsset(): AssetId;
  /**
   * Return the genesis block hash for this network as hex string.
   */
  genesisBlockHash(): string;
  /**
   * Return the transaction builder for this network
   */
  txBuilder(): TxBuilder;
  /**
   * Return the default explorer URL for this network
   */
  defaultExplorerUrl(): string;
}
/**
 * An optional wallet transaction output. Could be None when it's not possible to unblind.
 * It seems required by wasm_bindgen because we can't return `Vec<Option<WalletTxOut>>`
 */
export class OptionWalletTxOut {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Return a copy of the WalletTxOut if it exists, otherwise None
   */
  get(): WalletTxOut | undefined;
}
/**
 * A reference to a transaction output
 */
export class OutPoint {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates an `OutPoint` from a string representation.
   */
  constructor(s: string);
  /**
   * Creates an `OutPoint` from a transaction ID and output index.
   */
  static fromParts(txid: Txid, vout: number): OutPoint;
  /**
   * Return the transaction identifier.
   */
  txid(): Txid;
  /**
   * Return the output index.
   */
  vout(): number;
}
/**
 * POS (Point of Sale) configuration for encoding/decoding
 */
export class PosConfig {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Create a new POS configuration
   */
  constructor(descriptor: WolletDescriptor, currency: CurrencyCode);
  /**
   * Create a POS configuration with all options
   */
  static withOptions(descriptor: WolletDescriptor, currency: CurrencyCode, show_gear?: boolean | null, show_description?: boolean | null): PosConfig;
  /**
   * Decode a POS configuration from a URL-safe base64 encoded string
   */
  static decode(encoded: string): PosConfig;
  /**
   * Encode the POS configuration to a URL-safe base64 string
   */
  encode(): string;
  /**
   * Return a string representation of the POS configuration
   */
  toString(): string;
  /**
   * Get the wallet descriptor
   */
  readonly descriptor: WolletDescriptor;
  /**
   * Get the currency code
   */
  readonly currency: CurrencyCode;
  /**
   * Get whether to show the gear/settings button
   */
  readonly showGear: boolean | undefined;
  /**
   * Get whether to show the description/note field
   */
  readonly showDescription: boolean | undefined;
}
/**
 * Helper to convert satoshi values of an asset to the value with the given precision and viceversa.
 *
 * For example 100 satoshi with precision 2 is "1.00"
 */
export class Precision {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Create a new Precision, useful to encode e decode values for assets with precision.
   * erroring if the given precision is greater than the allowed maximum (8)
   */
  constructor(precision: number);
  /**
   * Convert the given satoshi value to the formatted value according to our precision
   *
   * For example 100 satoshi with precision 2 is "1.00"
   */
  satsToString(sats: bigint): string;
  /**
   * Convert the given string with precision to satoshi units.
   *
   * For example the string "1.00" of an asset with precision 2 is 100 satoshi.
   */
  stringToSats(sats: string): bigint;
}
export class PreparePayResponse {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Serialize the response to JSON string for JS interop
   */
  serialize(): string;
  swapId(): string;
  uri(): string;
  uriAddress(): Address;
  uriAmount(): bigint;
  /**
   * The fee of the swap provider
   *
   * It is equal to the amount requested onchain minus the amount of the bolt11 invoice
   * Does not include the fee of the onchain transaction.
   */
  fee(): bigint | undefined;
  completePay(): Promise<boolean>;
}
/**
 * Wrapper over [`lwk_wollet::PricesFetcher`]
 */
export class PricesFetcher {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Create a new PricesFetcher with default settings
   */
  constructor();
  /**
   * Fetch exchange rates for the given currency (e.g., "USD", "EUR", "CHF")
   *
   * Returns an ExchangeRates object containing rates from multiple sources and the median
   */
  rates(currency: CurrencyCode): Promise<ExchangeRates>;
}
/**
 * Wrapper over [`lwk_wollet::PricesFetcherBuilder`]
 */
export class PricesFetcherBuilder {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
}
/**
 * Partially Signed Elements Transaction
 */
export class Pset {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates a `Pset` from its base64 string representation.
   */
  constructor(base64: string);
  /**
   * Return a base64 string representation of the Pset.
   * The string can be used to re-create the Pset via `new()`
   */
  toString(): string;
  /**
   * Extract the Transaction from a Pset by filling in
   * the available signature information in place.
   */
  extractTx(): Transaction;
  /**
   * Get the unique id of the PSET as defined by [BIP-370](https://github.com/bitcoin/bips/blob/master/bip-0370.mediawiki#unique-identification)
   *
   * The unique id is the txid of the PSET with sequence numbers of inputs set to 0
   */
  uniqueId(): Txid;
  /**
   * Attempt to merge with another `Pset`.
   */
  combine(other: Pset): void;
  /**
   * Return a copy of the inputs of this PSET
   */
  inputs(): PsetInput[];
  /**
   * Return a copy of the outputs of this PSET
   */
  outputs(): PsetOutput[];
  /**
   * Add wallet details to this PSET in place
   */
  addDetails(wollet: Wollet): void;
}
/**
 * The details regarding balance and amounts in a PSET:
 *
 * - The fee of the transaction in the PSET
 * - The net balance of the assets in the PSET from the point of view of the wallet
 * - The outputs going out of the wallet
 */
export class PsetBalance {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Fee paid by this transaction.
   *
   * Warning: if there are multiple assets paying fees this function can return an incorrect value.
   *
   * Deprecated: use `feesIn(assetId)` or `fees()` instead.
   */
  fee(): bigint;
  /**
   * Fees paid by this transaction.
   */
  fees(): Fees;
  /**
   * The amount of fee with given asset id
   */
  feesIn(asset: AssetId): bigint;
  /**
   * The net balance for every asset with respect of the wallet asking the pset details
   */
  balances(): Balance;
  recipients(): Recipient[];
}
/**
 * The details of a Partially Signed Elements Transaction:
 *
 * - the net balance from the point of view of the wallet
 * - the available and missing signatures for each input
 * - for issuances and reissuances transactions contains the issuance or reissuance details
 */
export class PsetDetails {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Return the balance of the PSET from the point of view of the wallet
   * that generated this via `psetDetails()`
   */
  balance(): PsetBalance;
  /**
   * For each input existing or missing signatures
   */
  signatures(): PsetSignatures[];
  /**
   * Set of fingerprints for which the PSET is missing a signature
   */
  fingerprintsMissing(): string[];
  /**
   * List of fingerprints for which the PSET has a signature
   */
  fingerprintsHas(): string[];
  /**
   * Return an element for every input that could possibly be a issuance or a reissuance
   */
  inputsIssuances(): Issuance[];
}
/**
 * PSET input
 */
export class PsetInput {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Prevout TXID of the input
   */
  previousTxid(): Txid;
  /**
   * Prevout vout of the input
   */
  previousVout(): number;
  /**
   * Prevout scriptpubkey of the input
   */
  previousScriptPubkey(): Script | undefined;
  /**
   * Redeem script of the input
   */
  redeemScript(): Script | undefined;
  /**
   * If the input has an issuance, the asset id
   */
  issuanceAsset(): AssetId | undefined;
  /**
   * If the input has an issuance, the token id
   */
  issuanceToken(): AssetId | undefined;
  /**
   * If the input has a (re)issuance, the issuance object
   */
  issuance(): Issuance | undefined;
  /**
   * Input sighash
   */
  sighash(): number;
  /**
   * If the input has an issuance, returns [asset_id, token_id].
   * Returns undefined if the input has no issuance.
   */
  issuanceIds(): AssetId[] | undefined;
}
/**
 * PSET output
 */
export class PsetOutput {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Get the script pubkey
   */
  scriptPubkey(): Script;
  /**
   * Get the explicit amount, if set
   */
  amount(): bigint | undefined;
  /**
   * Get the explicit asset ID, if set
   */
  asset(): AssetId | undefined;
  /**
   * Get the blinder index, if set
   */
  blinderIndex(): number | undefined;
}
/**
 * The details of the signatures in a PSET, divided in available and missing signatures.
 */
export class PsetSignatures {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Returns `Vec<(PublicKey, KeySource)>`
   */
  hasSignature(): any;
  missingSignature(): any;
}
/**
 * Recipient of a PSET, in other words outputs that doesn't belong to the wallet
 */
export class Recipient {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  asset(): AssetId | undefined;
  value(): bigint | undefined;
  address(): Address | undefined;
  vout(): number;
}
/**
 * A Registry, a repository to store and retrieve asset metadata, like the name or the ticker of an asset.
 */
export class Registry {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Create a new registry cache specifying the URL of the registry,
   * fetch the assets metadata identified by the given asset ids and cache them for later local retrieval.
   * Use `default_for_network()` to get the default registry for the given network.
   */
  static new(url: string, asset_ids: AssetIds): Promise<Registry>;
  /**
   * Return the default registry for the given network,
   * fetch the assets metadata identified by the given asset ids and cache them for later local retrieval.
   * Use `new()` to specify a custom URL
   */
  static defaultForNetwork(network: Network, asset_ids: AssetIds): Promise<Registry>;
  /**
   * Create a new registry cache, using only the hardcoded assets.
   *
   * Hardcoded assets are the policy assets (LBTC, tLBTC, rLBTC) and the USDT asset on mainnet.
   */
  static defaultHardcodedForNetwork(network: Network): Registry;
  /**
   * Fetch the contract and the issuance transaction of the given asset id from the registry
   */
  fetchWithTx(asset_id: AssetId, client: EsploraClient): Promise<AssetMeta>;
  /**
   * Post a contract to the registry for registration.
   */
  post(data: RegistryPost): Promise<void>;
  /**
   * Return the asset metadata related to the given asset id if it exists in this registry.
   */
  get(asset_id: AssetId): RegistryData | undefined;
  /**
   * Return the asset metadata related to the given token id,
   * in other words `token_id` is the reissuance token of the returned asset
   */
  getAssetOfToken(token_id: AssetId): RegistryData | undefined;
  /**
   * Add the contracts information of the assets used in the Pset
   * if available in this registry.
   * Without the contract information, the partially signed transaction
   * is valid but will not show asset information when signed with an hardware wallet.
   */
  addContracts(pset: Pset): Pset;
}
export class RegistryData {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  precision(): number;
  ticker(): string;
  name(): string;
  domain(): string;
}
/**
 * The data to post to the registry to publish a contract for an asset id
 */
export class RegistryPost {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Create a new registry post object to be used to publish a contract for an asset id in the registry.
   */
  constructor(contract: Contract, asset_id: AssetId);
  /**
   * Return a string representation of the registry post (mostly for debugging).
   */
  toString(): string;
}
/**
 * An Elements (Liquid) script
 */
export class Script {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates a `Script` from its hex string representation.
   */
  constructor(s: string);
  /**
   * Creates an empty `Script`.
   */
  static empty(): Script;
  /**
   * Return the consensus encoded bytes of the script.
   */
  bytes(): Uint8Array;
  /**
   * Returns SHA256 of the script's consensus bytes.
   *
   * Returns an equivalent value to the `jet::input_script_hash(index)`/`jet::output_script_hash(index)`.
   */
  jet_sha256_hex(): string;
  /**
   * Return the string of the script showing op codes and their arguments.
   *
   * For example: "OP_DUP OP_HASH160 OP_PUSHBYTES_20 088ac47276d105b91cf9aa27a00112421dd5f23c OP_EQUALVERIFY OP_CHECKSIG"
   */
  asm(): string;
  /**
   * Creates an OP_RETURN script with the given data.
   */
  static newOpReturn(data: Uint8Array): Script;
  /**
   * Returns true if the script is provably unspendable.
   *
   * A script is provably unspendable if it starts with OP_RETURN or is larger
   * than the maximum script size.
   */
  isProvablyUnspendable(): boolean;
  /**
   * Returns true if this script_pubkey is provably SegWit.
   *
   * This checks if the script_pubkey is provably SegWit based on the
   * script_pubkey itself and an optional redeem_script.
   */
  isProvablySegwit(redeem_script?: Script | null): boolean;
  /**
   * Return the string representation of the script (hex encoding of its consensus encoded bytes).
   * This representation can be used to recreate the script via `new()`
   */
  toString(): string;
}
/**
 * A Software signer.
 */
export class Signer {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * AMP0 signer data for login
   */
  amp0SignerData(): Amp0SignerData;
  /**
   * AMP0 sign login challenge
   */
  amp0SignChallenge(challenge: string): string;
  /**
   * AMP0 account xpub
   */
  amp0AccountXpub(account: number): string;
  /**
   * Creates a `Signer`
   */
  constructor(mnemonic: Mnemonic, network: Network);
  /**
   * Sign and consume the given PSET, returning the signed one
   */
  sign(pset: Pset): Pset;
  /**
   * Sign a message with the master key, return the signature as a base64 string
   */
  signMessage(message: string): string;
  /**
   * Return the witness public key hash, slip77 descriptor of this signer
   */
  wpkhSlip77Descriptor(): WolletDescriptor;
  /**
   * Return the extended public key of the signer
   */
  getMasterXpub(): Xpub;
  /**
   * Return keyorigin and xpub, like "[73c5da0a/84h/1h/0h]tpub..."
   */
  keyoriginXpub(bip: Bip): string;
  /**
   * Return the signer fingerprint
   */
  fingerprint(): string;
  /**
   * Return the mnemonic of the signer
   */
  mnemonic(): Mnemonic;
  /**
   * Return the derived BIP85 mnemonic
   */
  derive_bip85_mnemonic(index: number, word_count: number): Mnemonic;
}
export class Singlesig {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  static from(variant: string): Singlesig;
}
/**
 * Blockchain tip, the highest valid block in the blockchain
 */
export class Tip {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  height(): number;
  hash(): string;
  timestamp(): number | undefined;
}
/**
 * A Liquid transaction
 *
 * See `WalletTx` for the transaction as seen from the perspective of the wallet
 * where you can actually see unblinded amounts and tx net-balance.
 */
export class Transaction {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates a `Transaction`
   *
   * Deprecated: use `fromString()` instead.
   */
  constructor(tx_hex: string);
  /**
   * Creates a `Transaction` from hex-encoded consensus bytes.
   */
  static fromString(s: string): Transaction;
  /**
   * Creates a `Transaction` from consensus-encoded bytes.
   */
  static fromBytes(bytes: Uint8Array): Transaction;
  /**
   * Return the transaction identifier.
   */
  txid(): Txid;
  /**
   * Return the consensus encoded bytes of the transaction.
   */
  toBytes(): Uint8Array;
  /**
   * Return the consensus encoded bytes of the transaction.
   *
   * Deprecated: use `toBytes()` instead.
   */
  bytes(): Uint8Array;
  /**
   * Return the fee of the transaction in the given asset.
   * At the moment the only asset that can be used as fee is the policy asset (LBTC for mainnet).
   */
  fee(policy_asset: AssetId): bigint;
  /**
   * Return the hex representation of the transaction. More precisely, they are the consensus encoded bytes of the transaction converted in hex.
   */
  toString(): string;
}
/**
 * A transaction builder
 */
export class TxBuilder {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates a transaction builder
   */
  constructor(network: Network);
  /**
   * Build the transaction
   */
  finish(wollet: Wollet): Pset;
  /**
   * Build the transaction for AMP0
   */
  finishForAmp0(wollet: Wollet): Amp0Pset;
  /**
   * Set the fee rate
   */
  feeRate(fee_rate?: number | null): TxBuilder;
  /**
   * Select all available L-BTC inputs
   */
  drainLbtcWallet(): TxBuilder;
  /**
   * Sets the address to drain excess L-BTC to
   */
  drainLbtcTo(address: Address): TxBuilder;
  /**
   * Add a recipient receiving L-BTC
   *
   * Errors if address's network is incompatible
   */
  addLbtcRecipient(address: Address, satoshi: bigint): TxBuilder;
  /**
   * Add a recipient receiving the given asset
   *
   * Errors if address's network is incompatible
   */
  addRecipient(address: Address, satoshi: bigint, asset: AssetId): TxBuilder;
  /**
   * Burn satoshi units of the given asset
   */
  addBurn(satoshi: bigint, asset: AssetId): TxBuilder;
  /**
   * Add explicit recipient
   */
  addExplicitRecipient(address: Address, satoshi: bigint, asset: AssetId): TxBuilder;
  /**
   * Issue an asset
   *
   * There will be `asset_sats` units of this asset that will be received by
   * `asset_receiver` if it's set, otherwise to an address of the wallet generating the issuance.
   *
   * There will be `token_sats` reissuance tokens that allow token holder to reissue the created
   * asset. Reissuance token will be received by `token_receiver` if it's some, or to an
   * address of the wallet generating the issuance if none.
   *
   * If a `contract` is provided, it's metadata will be committed in the generated asset id.
   *
   * Can't be used if `reissue_asset` has been called
   */
  issueAsset(asset_sats: bigint, asset_receiver: Address | null | undefined, token_sats: bigint, token_receiver?: Address | null, contract?: Contract | null): TxBuilder;
  /**
   * Reissue an asset
   *
   * reissue the asset defined by `asset_to_reissue`, provided the reissuance token is owned
   * by the wallet generating te reissuance.
   *
   * Generated transaction will create `satoshi_to_reissue` new asset units, and they will be
   * sent to the provided `asset_receiver` address if some, or to an address from the wallet
   * generating the reissuance transaction if none.
   *
   * If the issuance transaction does not involve this wallet,
   * pass the issuance transaction in `issuance_tx`.
   */
  reissueAsset(asset_to_reissue: AssetId, satoshi_to_reissue: bigint, asset_receiver?: Address | null, issuance_tx?: Transaction | null): TxBuilder;
  /**
   * Switch to manual coin selection by giving a list of internal UTXOs to use.
   *
   * All passed UTXOs are added to the transaction.
   * No other wallet UTXO is added to the transaction, caller is supposed to add enough UTXOs to
   * cover for all recipients and fees.
   *
   * This method never fails, any error will be raised in [`TxBuilder::finish`].
   *
   * Possible errors:
   * * OutPoint doesn't belong to the wallet
   * * Insufficient funds (remember to include L-BTC utxos for fees)
   */
  setWalletUtxos(outpoints: OutPoint[]): TxBuilder;
  /**
   * Return a string representation of the transaction builder (mostly for debugging)
   */
  toString(): string;
  /**
   * Set data to create a PSET from which you
   * can create a LiquiDEX proposal
   */
  liquidexMake(utxo: OutPoint, address: Address, satoshi: bigint, asset_id: AssetId): TxBuilder;
  /**
   * Set data to take LiquiDEX proposals
   */
  liquidexTake(proposals: ValidatedLiquidexProposal[]): TxBuilder;
  /**
   * Add input rangeproofs
   */
  addInputRangeproofs(add_rangeproofs: boolean): TxBuilder;
}
/**
 * Transaction details
 */
export class TxDetails {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Transaction
   */
  tx(): Transaction | undefined;
  /**
   * Txid
   */
  txid(): Txid;
  /**
   * Blockchain height
   */
  height(): number | undefined;
  /**
   * Timestamp
   *
   * A reasonable timestamp, that however can be inaccurate.
   * If you need a precise timestamp, do not use this value.
   */
  timestamp(): number | undefined;
  /**
   * Transaction type
   *
   * A tentative description of the transaction type, which
   * however might be inaccurate. Use this if you want a simple
   * description of what this transaction is doing, but do
   * not rely on the value returned.
   */
  txType(): string;
  /**
   * Balance
   *
   * Net balance from the `Wollet` perspective
   */
  balance(): Balance;
  /**
   * Fees paid by this transaction.
   */
  fees(): Fees;
  /**
   * Asset fees
   */
  feesAsset(asset: AssetId): bigint;
  /**
   * Unblinded URL
   */
  unblindedUrl(explorer_url: string): string;
  /**
   * Inputs
   */
  inputs(): TxOutDetails[];
  /**
   * Outputs
   */
  outputs(): TxOutDetails[];
}
/**
 * Options for transaction details
 */
export class TxOpt {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  static default(): TxOpt;
}
/**
 * Transaction output details
 */
export class TxOutDetails {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Outpoint
   */
  outpoint(): OutPoint;
  /**
   * Scriptpubkey
   */
  script_pubkey(): Script | undefined;
  /**
   * Height
   */
  height(): number | undefined;
  /**
   * Address
   */
  address(): Address | undefined;
  /**
   * Unblinded values (asset, amount, blinders)
   */
  unblinded(): TxOutSecrets | undefined;
  /**
   * Whether the transaction output is explicit
   */
  is_explicit(): boolean;
  /**
   * Whether the output is spent by a previously downloaded transaction
   *
   * Note: this value might be inaccurate. We compute this from downloaded
   * transactions, however we only download transactions relevant for the
   * wallet (i.e. if they include inputs or outputs that belong to the
   * wallet), thus for non-wallet outputs we might set this value
   * incorrectly. For wallet outputs, it can be outdated.
   */
  is_spent(): boolean;
}
/**
 * Contains unblinded information such as the asset and the value of a transaction output
 */
export class TxOutSecrets {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates a new `TxOutSecrets` for an explicit (unblinded) output.
   *
   * The blinding factors are set to zero.
   */
  static fromExplicit(asset_id: AssetId, value: bigint): TxOutSecrets;
  /**
   * Return the asset of the output.
   */
  asset(): AssetId;
  /**
   * Return the asset blinding factor as a typed object.
   */
  assetBlindingFactor(): AssetBlindingFactor;
  /**
   * Return the value of the output.
   */
  value(): bigint;
  /**
   * Return the value blinding factor as a typed object.
   */
  valueBlindingFactor(): ValueBlindingFactor;
  /**
   * Return true if the output is explicit (no blinding factors).
   */
  isExplicit(): boolean;
  /**
   * Get the asset commitment
   *
   * If the output is explicit, returns the empty string
   */
  assetCommitment(): string;
  /**
   * Get the value commitment
   *
   * If the output is explicit, returns the empty string
   */
  valueCommitment(): string;
}
/**
 * A valid transaction identifier.
 *
 * 32 bytes encoded as hex string.
 */
export class Txid {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates a `Txid` from its hex string representation (64 characters).
   */
  constructor(tx_id: string);
  /**
   * Return the string representation of the transaction identifier as shown in the explorer.
   * This representation can be used to recreate the transaction identifier via `new()`
   */
  toString(): string;
}
/**
 * Options for transaction details
 */
export class TxsOpt {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  static default(): TxsOpt;
  static withPagination(offset: number, limit: number): TxsOpt;
}
/**
 * LiquiDEX swap proposal
 *
 * A LiquiDEX swap proposal is a transaction with one input and one output created by the "maker".
 * The transaction "swaps" the input for the output, meaning that the "maker" sends the input and
 * receives the output.
 * However the transaction is incomplete (unbalanced and without a fee output), thus it cannot be
 * broadcast.
 * The "taker" can "complete" the transaction (using [`crate::TxBuilder::liquidex_take()`]) by
 * adding more inputs and more outputs to balance the amounts, meaning that the "taker" sends the
 * output and receives the input.
 */
export class UnvalidatedLiquidexProposal {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  static new(s: string): UnvalidatedLiquidexProposal;
  static fromPset(pset: Pset): UnvalidatedLiquidexProposal;
  insecureValidate(): ValidatedLiquidexProposal;
  validate(tx: Transaction): ValidatedLiquidexProposal;
  toString(): string;
}
/**
 * An Update contains the delta of information to be applied to the wallet to reach the latest status.
 * It's created passing a reference to the wallet to the blockchain client
 */
export class Update {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates an `Update`
   */
  constructor(bytes: Uint8Array);
  /**
   * Serialize an update to a byte array
   */
  serialize(): Uint8Array;
  /**
   * Serialize an update to a base64 encoded string,
   * encrypted with a key derived from the descriptor.
   * Decrypt using `deserialize_decrypted_base64()`
   */
  serializeEncryptedBase64(desc: WolletDescriptor): string;
  /**
   * Deserialize an update from a base64 encoded string,
   * decrypted with a key derived from the descriptor.
   * Create the base64 using `serialize_encrypted_base64()`
   */
  static deserializeDecryptedBase64(base64: string, desc: WolletDescriptor): Update;
  /**
   * Whether this update only changes the tip
   */
  onlyTip(): boolean;
  /**
   * Prune the update, removing unneeded data from transactions.
   */
  prune(wollet: Wollet): void;
}
/**
 * Created by validating `UnvalidatedLiquidexProposal` via `validate()` or `insecure_validate()`
 */
export class ValidatedLiquidexProposal {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  input(): AssetAmount;
  output(): AssetAmount;
  toString(): string;
}
/**
 * A blinding factor for value commitments.
 */
export class ValueBlindingFactor {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates a `ValueBlindingFactor` from a string.
   */
  static fromString(s: string): ValueBlindingFactor;
  /**
   * Creates a `ValueBlindingFactor` from a byte slice.
   */
  static fromBytes(bytes: Uint8Array): ValueBlindingFactor;
  /**
   * Returns a zero value blinding factor.
   */
  static zero(): ValueBlindingFactor;
  /**
   * Returns the bytes (32 bytes) in little-endian byte order.
   *
   * This is the internal representation used by secp256k1. The byte order is
   * reversed compared to the hex string representation (which uses big-endian,
   * following Bitcoin display conventions).
   */
  toBytes(): Uint8Array;
  /**
   * Returns string representation of the VBF
   */
  toString(): string;
}
/**
 * Value returned by asking transactions to the wallet. Contains details about a transaction
 * from the perspective of the wallet, for example the net-balance of the transaction for the
 * wallet.
 */
export class WalletTx {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Return a copy of the transaction.
   */
  tx(): Transaction;
  /**
   * Return the height of the block containing the transaction if it's confirmed.
   */
  height(): number | undefined;
  /**
   * Return the net balance of the transaction for the wallet.
   */
  balance(): Balance;
  /**
   * Return the transaction identifier.
   */
  txid(): Txid;
  /**
   * Return the fee of the transaction.
   */
  fee(): bigint;
  /**
   * Return the type of the transaction. Can be "issuance", "reissuance", "burn", "redeposit", "incoming", "outgoing" or "unknown".
   */
  txType(): string;
  /**
   * Return the timestamp of the block containing the transaction if it's confirmed.
   */
  timestamp(): number | undefined;
  /**
   * Return a list with the same number of elements as the inputs of the transaction.
   * The element in the list is a `WalletTxOut` (the output spent to create the input)
   * if it belongs to the wallet, while it is None for inputs owned by others
   */
  inputs(): OptionWalletTxOut[];
  /**
   * Return a list with the same number of elements as the outputs of the transaction.
   * The element in the list is a `WalletTxOut` if it belongs to the wallet,
   * while it is None for inputs owned by others
   */
  outputs(): OptionWalletTxOut[];
  /**
   * Return the URL to the transaction on the given explorer including the information
   * needed to unblind the transaction in the explorer UI.
   */
  unblindedUrl(explorer_url: string): string;
}
/**
 * Details of a wallet transaction output used in `WalletTx`
 */
export class WalletTxOut {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Return the outpoint (txid and vout) of this `WalletTxOut`.
   */
  outpoint(): OutPoint;
  /**
   * Return the script pubkey of the address of this `WalletTxOut`.
   */
  scriptPubkey(): Script;
  /**
   * Return the height of the block containing this output if it's confirmed.
   */
  height(): number | undefined;
  /**
   * Return the unblinded values of this `WalletTxOut`.
   */
  unblinded(): TxOutSecrets;
  /**
   * Return the wildcard index used to derive the address of this `WalletTxOut`.
   */
  wildcardIndex(): number;
  /**
   * Return the chain of this `WalletTxOut`. Can be "Chain::External" or "Chain::Internal" (change).
   */
  extInt(): Chain;
  /**
   * Return the address of this `WalletTxOut`.
   */
  address(): Address;
}
/**
 * A watch-only wallet defined by a CT descriptor.
 */
export class Wollet {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Create a `Wollet`
   */
  constructor(network: Network, descriptor: WolletDescriptor);
  /**
   * Get a wallet address with the correspondong derivation index
   *
   * If Some return the address at the given index,
   * otherwise the last unused address.
   */
  address(index?: number | null): AddressResult;
  /**
   * Return the [ELIP152](https://github.com/ElementsProject/ELIPs/blob/main/elip-0152.mediawiki) deterministic wallet identifier.
   */
  dwid(): string;
  /**
   * Get the full derivation path for an address
   *
   * Note: will be removed once we add the full path to lwk_wollet::AddressResult
   */
  addressFullPath(index: number): Uint32Array;
  /**
   * Apply an update containing blockchain data
   *
   * To update the wallet you need to first obtain the blockchain data relevant for the wallet.
   * This can be done using a `full_scan()`, which
   * returns an `Update` that contains new transaction and other data relevant for the
   * wallet.
   * The update must then be applied to the `Wollet` so that wollet methods such as
   * `balance()` or `transactions()` include the new data.
   *
   * However getting blockchain data involves network calls, so between the full scan start and
   * when the update is applied it might elapse a significant amount of time.
   * In that interval, applying any update, or any transaction using `apply_transaction()`,
   * will cause this function to return a `Error::UpdateOnDifferentStatus`.
   * Callers should either avoid applying updates and transactions, or they can catch the error
   * and wait for a new full scan to be completed and applied.
   */
  applyUpdate(update: Update): void;
  /**
   * Apply a transaction to the wallet state
   *
   * Wallet transactions are normally obtained using a `full_scan()`
   * and applying the result with `apply_update()`. However a
   * full scan involves network calls and it can take a significant amount of time.
   *
   * If the caller does not want to wait for a full scan containing the transaction, it can
   * apply the transaction to the wallet state using this function.
   *
   * Note: if this transaction is *not* returned by a next full scan, after `apply_update()` it will disappear from the
   * transactions list, will not be included in balance computations, and by the remaining
   * wollet methods.
   *
   * Calling this method, might cause `apply_update()` to fail with a
   * `Error::UpdateOnDifferentStatus`, make sure to either avoid it or handle the error properly.
   */
  applyTransaction(tx: Transaction): Balance;
  /**
   * Get the wallet balance for each assets
   */
  balance(): Balance;
  /**
   * Get the asset identifiers owned by the wallet
   */
  assetsOwned(): AssetIds;
  /**
   * Get the wallet transactions, sorted by height descending, then txid descending with unconfirmed first
   */
  transactions(): WalletTx[];
  /**
   * Get the wallet transactions with pagination sorted by height descending, then txid descending with unconfirmed first
   */
  transactionsPaginated(offset: number, limit: number): WalletTx[];
  /**
   * Get the unspent transaction outputs of the wallet
   */
  utxos(): WalletTxOut[];
  /**
   * Get all the transaction outputs of the wallet, both spent and unspent
   */
  txos(): WalletTxOut[];
  /**
   * Finalize and consume the given PSET, returning the finalized one
   */
  finalize(pset: Pset): Pset;
  /**
   * Get the PSET details with respect to the wallet
   */
  psetDetails(pset: Pset): PsetDetails;
  /**
   * Get a copy of the wallet descriptor of this wallet.
   */
  descriptor(): WolletDescriptor;
  /**
   * A deterministic value derived from the descriptor, the config and the content of this wollet,
   * including what's in the wallet store (transactions etc)
   *
   * In this case, we don't need cryptographic assurance guaranteed by the std default hasher (siphash)
   * And we can use a much faster hasher, which is used also in the rust compiler.
   * ([source](https://nnethercote.github.io/2021/12/08/a-brutally-effective-hash-function-in-rust.html))
   */
  status(): bigint;
  /**
   * Get the blockchain tip at the time of the last update of this wollet.
   */
  tip(): Tip;
  /**
   * Returns true if this wollet has never received an updated applyed to it
   */
  neverScanned(): boolean;
  /**
   * Whether the wallet is AMP0
   */
  isAmp0(): boolean;
  /**
   * Get the transaction list
   *
   * **Experimental**: This API may change without notice.
   */
  txs(opt: TxsOpt): TxDetails[];
  /**
   * Number of transactions
   */
  numTxs(): number;
  /**
   * Get the details of a transaction
   *
   * **Experimental**: This API may change without notice.
   */
  txDetails(txid: Txid, opt: TxOpt): TxDetails | undefined;
}
/**
 * A builder for constructing [`Wollet`] instances.
 */
export class WolletBuilder {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Create a builder for a watch-only wallet.
   */
  constructor(network: Network, descriptor: WolletDescriptor);
  /**
   * Set the threshold used to merge persisted updates during build.
   *
   * **Experimental**: This API may change without notice.
   *
   * `None` disables merging (default behavior).
   */
  withMergeThreshold(merge_threshold?: number | null): WolletBuilder;
  /**
   * Set the wallet as "utxo only".
   *
   * **Experimental**: This API may change without notice.
   */
  utxoOnly(utxo_only: boolean): WolletBuilder;
  /**
   * Persist wallet updates in the given JavaScript storage object.
   *
   * **Experimental**: This API may change without notice.
   *
   * Wallet data is persisted in clear.
   *
   * The JS object must have `get(key)`, `put(key, value)`, and `remove(key)` methods.
   */
  withExperimentalStore(storage: any): WolletBuilder;
  /**
   * Persist wallet transactions in the given JavaScript
   * storage object.
   *
   * **Experimental**: This API may change without notice.
   *
   * The JS object must have `get(key)`, `put(key, value)`, and `remove(key)` methods.
   */
  withTxsStore(storage: any): WolletBuilder;
  /**
   * Set encryption for the transactions store.
   *
   * **Experimental**: This API may change without notice.
   *
   * Default: encrypted if the store is persisted.
   */
  setEncryptionTxsStore(encrypt: boolean): WolletBuilder;
  /**
   * Build the wallet from this builder.
   */
  build(): Wollet;
}
/**
 * A wrapper that contains only the subset of CT descriptors handled by wollet
 */
export class WolletDescriptor {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates a `WolletDescriptor`
   */
  constructor(descriptor: string);
  /**
   * Return the string representation of the descriptor, including the checksum.
   * This representation can be used to recreate the descriptor via `new()`
   */
  toString(): string;
  /**
   * Create a new multisig descriptor, where each participant is a keyorigin_xpub and it requires at least threshold signatures to spend.
   * Errors if the threshold is 0 or greater than the number of participants.
   * Uses slip77 for the blinding key.
   */
  static newMultiWshSlip77(threshold: number, participants: string[]): WolletDescriptor;
  /**
   * Whether the descriptor is for mainnet
   */
  isMainnet(): boolean;
  /**
   * Whether the descriptor is AMP0
   */
  isAmp0(): boolean;
}
/**
 * An extended public key
 */
export class Xpub {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates a Xpub
   */
  constructor(s: string);
  /**
   * Return the string representation of the Xpub.
   * This representation can be used to recreate the Xpub via `new()`
   */
  toString(): string;
  /**
   * Return the identifier of the Xpub.
   * This is a 40 hex characters string (20 bytes).
   */
  identifier(): string;
  /**
   * Return the first four bytes of the identifier as hex string
   * This is a 8 hex characters string (4 bytes).
   */
  fingerprint(): string;
  /**
   * Returns true if the passed string is a valid xpub with a valid keyorigin if present.
   * For example: "[73c5da0a/84h/1h/0h]tpub..."
   */
  static isValidWithKeyOrigin(s: string): boolean;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly __wbg_amp0_free: (a: number, b: number) => void;
  readonly amp0_newWithNetwork: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => any;
  readonly amp0_newTestnet: (a: number, b: number, c: number, d: number, e: number, f: number) => any;
  readonly amp0_newMainnet: (a: number, b: number, c: number, d: number, e: number, f: number) => any;
  readonly amp0_lastIndex: (a: number) => number;
  readonly amp0_ampId: (a: number) => [number, number];
  readonly amp0_address: (a: number, b: number) => any;
  readonly amp0_wollet: (a: number) => [number, number, number];
  readonly amp0_sign: (a: number, b: number) => any;
  readonly __wbg_amp0pset_free: (a: number, b: number) => void;
  readonly amp0pset_new: (a: number, b: number, c: number) => [number, number, number];
  readonly amp0pset_pset: (a: number) => number;
  readonly amp0pset_blindingNonces: (a: number) => [number, number];
  readonly __wbg_amp0signerdata_free: (a: number, b: number) => void;
  readonly signer_amp0SignerData: (a: number) => [number, number, number];
  readonly signer_amp0SignChallenge: (a: number, b: number, c: number) => [number, number, number, number];
  readonly signer_amp0AccountXpub: (a: number, b: number) => [number, number, number, number];
  readonly __wbg_amp0connected_free: (a: number, b: number) => void;
  readonly amp0connected_new: (a: number, b: number) => any;
  readonly amp0connected_connect: (a: number, b: number) => any;
  readonly amp0connected_getChallenge: (a: number) => any;
  readonly amp0connected_login: (a: number, b: number, c: number) => any;
  readonly __wbg_amp0loggedin_free: (a: number, b: number) => void;
  readonly amp0loggedin_getAmpIds: (a: number) => [number, number, number, number];
  readonly amp0loggedin_nextAccount: (a: number) => [number, number, number];
  readonly amp0loggedin_createAmp0Account: (a: number, b: number, c: number, d: number) => any;
  readonly amp0loggedin_createWatchOnly: (a: number, b: number, c: number, d: number, e: number) => any;
  readonly __wbg_magicroutinghint_free: (a: number, b: number) => void;
  readonly magicroutinghint_address: (a: number) => [number, number];
  readonly magicroutinghint_amount: (a: number) => bigint;
  readonly magicroutinghint_uri: (a: number) => [number, number];
  readonly __wbg_network_free: (a: number, b: number) => void;
  readonly network_mainnet: () => number;
  readonly network_testnet: () => number;
  readonly network_regtest: (a: number) => number;
  readonly network_regtestDefault: () => number;
  readonly network_defaultEsploraClient: (a: number) => number;
  readonly network_isMainnet: (a: number) => number;
  readonly network_isTestnet: (a: number) => number;
  readonly network_isRegtest: (a: number) => number;
  readonly network_toString: (a: number) => [number, number];
  readonly network_policyAsset: (a: number) => number;
  readonly network_genesisBlockHash: (a: number) => [number, number];
  readonly network_txBuilder: (a: number) => number;
  readonly network_defaultExplorerUrl: (a: number) => [number, number];
  readonly __wbg_txbuilder_free: (a: number, b: number) => void;
  readonly txbuilder_finish: (a: number, b: number) => [number, number, number];
  readonly txbuilder_finishForAmp0: (a: number, b: number) => [number, number, number];
  readonly txbuilder_feeRate: (a: number, b: number) => number;
  readonly txbuilder_drainLbtcWallet: (a: number) => number;
  readonly txbuilder_drainLbtcTo: (a: number, b: number) => number;
  readonly txbuilder_addLbtcRecipient: (a: number, b: number, c: bigint) => [number, number, number];
  readonly txbuilder_addRecipient: (a: number, b: number, c: bigint, d: number) => [number, number, number];
  readonly txbuilder_addBurn: (a: number, b: bigint, c: number) => number;
  readonly txbuilder_addExplicitRecipient: (a: number, b: number, c: bigint, d: number) => [number, number, number];
  readonly txbuilder_issueAsset: (a: number, b: bigint, c: number, d: bigint, e: number, f: number) => [number, number, number];
  readonly txbuilder_reissueAsset: (a: number, b: number, c: bigint, d: number, e: number) => [number, number, number];
  readonly txbuilder_setWalletUtxos: (a: number, b: number, c: number) => number;
  readonly txbuilder_toString: (a: number) => [number, number];
  readonly txbuilder_liquidexMake: (a: number, b: number, c: number, d: bigint, e: number) => [number, number, number];
  readonly txbuilder_liquidexTake: (a: number, b: number, c: number) => [number, number, number];
  readonly txbuilder_addInputRangeproofs: (a: number, b: number) => number;
  readonly __wbg_update_free: (a: number, b: number) => void;
  readonly update_new: (a: number, b: number) => [number, number, number];
  readonly update_serialize: (a: number) => [number, number, number, number];
  readonly update_serializeEncryptedBase64: (a: number, b: number) => [number, number, number, number];
  readonly update_deserializeDecryptedBase64: (a: number, b: number, c: number) => [number, number, number];
  readonly update_onlyTip: (a: number) => number;
  readonly update_prune: (a: number, b: number) => void;
  readonly __wbg_wollet_free: (a: number, b: number) => void;
  readonly wollet_new: (a: number, b: number) => [number, number, number];
  readonly wollet_address: (a: number, b: number) => [number, number, number];
  readonly wollet_dwid: (a: number) => [number, number, number, number];
  readonly wollet_addressFullPath: (a: number, b: number) => [number, number, number, number];
  readonly wollet_applyUpdate: (a: number, b: number) => [number, number];
  readonly wollet_applyTransaction: (a: number, b: number) => [number, number, number];
  readonly wollet_balance: (a: number) => [number, number, number];
  readonly wollet_assetsOwned: (a: number) => [number, number, number];
  readonly wollet_transactions: (a: number) => [number, number, number, number];
  readonly wollet_transactionsPaginated: (a: number, b: number, c: number) => [number, number, number, number];
  readonly wollet_utxos: (a: number) => [number, number, number, number];
  readonly wollet_txos: (a: number) => [number, number, number, number];
  readonly wollet_finalize: (a: number, b: number) => [number, number, number];
  readonly wollet_psetDetails: (a: number, b: number) => [number, number, number];
  readonly wollet_descriptor: (a: number) => [number, number, number];
  readonly wollet_status: (a: number) => bigint;
  readonly wollet_tip: (a: number) => number;
  readonly wollet_neverScanned: (a: number) => number;
  readonly wollet_isAmp0: (a: number) => number;
  readonly __wbg_tip_free: (a: number, b: number) => void;
  readonly tip_height: (a: number) => number;
  readonly tip_hash: (a: number) => [number, number];
  readonly tip_timestamp: (a: number) => number;
  readonly txbuilder_new: (a: number) => number;
  readonly __wbg_balance_free: (a: number, b: number) => void;
  readonly balance_toJSON: (a: number) => [number, number, number];
  readonly balance_entries: (a: number) => [number, number, number];
  readonly balance_toString: (a: number) => [number, number];
  readonly __wbg_bip_free: (a: number, b: number) => void;
  readonly bip_bip49: () => number;
  readonly bip_bip84: () => number;
  readonly bip_bip87: () => number;
  readonly bip_bip86: () => number;
  readonly bip_toString: (a: number) => [number, number];
  readonly __wbg_txoutsecrets_free: (a: number, b: number) => void;
  readonly txoutsecrets_fromExplicit: (a: number, b: bigint) => number;
  readonly txoutsecrets_asset: (a: number) => number;
  readonly txoutsecrets_assetBlindingFactor: (a: number) => number;
  readonly txoutsecrets_value: (a: number) => bigint;
  readonly txoutsecrets_valueBlindingFactor: (a: number) => number;
  readonly txoutsecrets_isExplicit: (a: number) => number;
  readonly txoutsecrets_assetCommitment: (a: number) => [number, number];
  readonly txoutsecrets_valueCommitment: (a: number) => [number, number];
  readonly __wbg_signer_free: (a: number, b: number) => void;
  readonly signer_new: (a: number, b: number) => [number, number, number];
  readonly signer_sign: (a: number, b: number) => [number, number, number];
  readonly signer_signMessage: (a: number, b: number, c: number) => [number, number, number, number];
  readonly signer_wpkhSlip77Descriptor: (a: number) => [number, number, number];
  readonly signer_getMasterXpub: (a: number) => [number, number, number];
  readonly signer_keyoriginXpub: (a: number, b: number) => [number, number, number, number];
  readonly signer_fingerprint: (a: number) => [number, number, number, number];
  readonly signer_mnemonic: (a: number) => number;
  readonly signer_derive_bip85_mnemonic: (a: number, b: number, c: number) => [number, number, number];
  readonly __wbg_xpub_free: (a: number, b: number) => void;
  readonly xpub_new: (a: number, b: number) => [number, number, number];
  readonly xpub_toString: (a: number) => [number, number];
  readonly xpub_identifier: (a: number) => [number, number];
  readonly xpub_fingerprint: (a: number) => [number, number];
  readonly xpub_isValidWithKeyOrigin: (a: number, b: number) => number;
  readonly __wbg_posconfig_free: (a: number, b: number) => void;
  readonly posconfig_new: (a: number, b: number) => number;
  readonly posconfig_withOptions: (a: number, b: number, c: number, d: number) => number;
  readonly posconfig_decode: (a: number, b: number) => [number, number, number];
  readonly posconfig_encode: (a: number) => [number, number, number, number];
  readonly posconfig_descriptor: (a: number) => number;
  readonly posconfig_currency: (a: number) => number;
  readonly posconfig_show_gear: (a: number) => number;
  readonly posconfig_show_description: (a: number) => number;
  readonly posconfig_toString: (a: number) => [number, number];
  readonly __wbg_externalutxo_free: (a: number, b: number) => void;
  readonly externalutxo_new: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
  readonly __wbg_outpoint_free: (a: number, b: number) => void;
  readonly outpoint_new: (a: number, b: number) => [number, number, number];
  readonly outpoint_fromParts: (a: number, b: number) => number;
  readonly outpoint_txid: (a: number) => number;
  readonly outpoint_vout: (a: number) => number;
  readonly __wbg_transaction_free: (a: number, b: number) => void;
  readonly transaction_new: (a: number, b: number) => [number, number, number];
  readonly transaction_fromString: (a: number, b: number) => [number, number, number];
  readonly transaction_fromBytes: (a: number, b: number) => [number, number, number];
  readonly transaction_txid: (a: number) => number;
  readonly transaction_bytes: (a: number) => [number, number];
  readonly transaction_fee: (a: number, b: number) => bigint;
  readonly transaction_toString: (a: number) => [number, number];
  readonly __wbg_txid_free: (a: number, b: number) => void;
  readonly txid_new: (a: number, b: number) => [number, number, number];
  readonly txid_toString: (a: number) => [number, number];
  readonly __wbg_wallettx_free: (a: number, b: number) => void;
  readonly wallettx_tx: (a: number) => number;
  readonly wallettx_height: (a: number) => number;
  readonly wallettx_balance: (a: number) => number;
  readonly wallettx_txid: (a: number) => number;
  readonly wallettx_fee: (a: number) => bigint;
  readonly wallettx_txType: (a: number) => [number, number];
  readonly wallettx_timestamp: (a: number) => number;
  readonly wallettx_inputs: (a: number) => [number, number];
  readonly wallettx_outputs: (a: number) => [number, number];
  readonly wallettx_unblindedUrl: (a: number, b: number, c: number) => [number, number];
  readonly __wbg_wallettxout_free: (a: number, b: number) => void;
  readonly wallettxout_outpoint: (a: number) => number;
  readonly wallettxout_scriptPubkey: (a: number) => number;
  readonly wallettxout_height: (a: number) => number;
  readonly wallettxout_unblinded: (a: number) => number;
  readonly wallettxout_wildcardIndex: (a: number) => number;
  readonly wallettxout_extInt: (a: number) => number;
  readonly wallettxout_address: (a: number) => number;
  readonly __wbg_optionwallettxout_free: (a: number, b: number) => void;
  readonly optionwallettxout_get: (a: number) => number;
  readonly __wbg_pset_free: (a: number, b: number) => void;
  readonly pset_new: (a: number, b: number) => [number, number, number];
  readonly pset_toString: (a: number) => [number, number];
  readonly pset_extractTx: (a: number) => [number, number, number];
  readonly pset_uniqueId: (a: number) => [number, number, number];
  readonly pset_combine: (a: number, b: number) => [number, number];
  readonly pset_inputs: (a: number) => [number, number];
  readonly pset_outputs: (a: number) => [number, number];
  readonly pset_addDetails: (a: number, b: number) => [number, number];
  readonly __wbg_psetinput_free: (a: number, b: number) => void;
  readonly psetinput_previousTxid: (a: number) => number;
  readonly psetinput_previousVout: (a: number) => number;
  readonly psetinput_previousScriptPubkey: (a: number) => number;
  readonly psetinput_redeemScript: (a: number) => number;
  readonly psetinput_issuanceAsset: (a: number) => number;
  readonly psetinput_issuanceToken: (a: number) => number;
  readonly psetinput_issuance: (a: number) => number;
  readonly psetinput_sighash: (a: number) => number;
  readonly psetinput_issuanceIds: (a: number) => [number, number];
  readonly __wbg_psetoutput_free: (a: number, b: number) => void;
  readonly psetoutput_scriptPubkey: (a: number) => number;
  readonly psetoutput_amount: (a: number) => [number, bigint];
  readonly psetoutput_asset: (a: number) => number;
  readonly psetoutput_blinderIndex: (a: number) => number;
  readonly __wbg_psetdetails_free: (a: number, b: number) => void;
  readonly __wbg_psetbalance_free: (a: number, b: number) => void;
  readonly __wbg_psetsignatures_free: (a: number, b: number) => void;
  readonly __wbg_issuance_free: (a: number, b: number) => void;
  readonly __wbg_recipient_free: (a: number, b: number) => void;
  readonly psetdetails_balance: (a: number) => number;
  readonly psetdetails_signatures: (a: number) => [number, number];
  readonly psetdetails_fingerprintsMissing: (a: number) => [number, number];
  readonly psetdetails_fingerprintsHas: (a: number) => [number, number];
  readonly psetdetails_inputsIssuances: (a: number) => [number, number];
  readonly psetbalance_fee: (a: number) => bigint;
  readonly psetbalance_fees: (a: number) => number;
  readonly psetbalance_feesIn: (a: number, b: number) => bigint;
  readonly psetbalance_balances: (a: number) => number;
  readonly psetbalance_recipients: (a: number) => [number, number];
  readonly psetsignatures_hasSignature: (a: number) => any;
  readonly psetsignatures_missingSignature: (a: number) => any;
  readonly issuance_asset: (a: number) => number;
  readonly issuance_token: (a: number) => number;
  readonly issuance_prevVout: (a: number) => number;
  readonly issuance_prevTxid: (a: number) => number;
  readonly issuance_isIssuance: (a: number) => number;
  readonly issuance_isReissuance: (a: number) => number;
  readonly recipient_asset: (a: number) => number;
  readonly recipient_value: (a: number) => [number, bigint];
  readonly recipient_address: (a: number) => number;
  readonly recipient_vout: (a: number) => number;
  readonly __wbg_registry_free: (a: number, b: number) => void;
  readonly __wbg_registrydata_free: (a: number, b: number) => void;
  readonly __wbg_assetmeta_free: (a: number, b: number) => void;
  readonly __wbg_registrypost_free: (a: number, b: number) => void;
  readonly assetmeta_contract: (a: number) => number;
  readonly assetmeta_tx: (a: number) => number;
  readonly registrypost_new: (a: number, b: number) => number;
  readonly registrypost_toString: (a: number) => [number, number];
  readonly registry_new: (a: number, b: number, c: number) => any;
  readonly registry_defaultForNetwork: (a: number, b: number) => any;
  readonly registry_defaultHardcodedForNetwork: (a: number) => [number, number, number];
  readonly registry_fetchWithTx: (a: number, b: number, c: number) => any;
  readonly registry_post: (a: number, b: number) => any;
  readonly registry_get: (a: number, b: number) => number;
  readonly registry_getAssetOfToken: (a: number, b: number) => number;
  readonly registry_addContracts: (a: number, b: number) => [number, number, number];
  readonly registrydata_precision: (a: number) => number;
  readonly registrydata_ticker: (a: number) => [number, number];
  readonly registrydata_name: (a: number) => [number, number];
  readonly registrydata_domain: (a: number) => [number, number];
  readonly __wbg_jsstorelink_free: (a: number, b: number) => void;
  readonly jsstorelink_new: (a: any) => number;
  readonly __wbg_jsteststore_free: (a: number, b: number) => void;
  readonly jsteststore_new: (a: any) => number;
  readonly jsteststore_write: (a: number, b: number, c: number, d: number, e: number) => [number, number];
  readonly jsteststore_read: (a: number, b: number, c: number) => [number, number, number, number];
  readonly jsteststore_remove: (a: number, b: number, c: number) => [number, number];
  readonly __wbg_txdetails_free: (a: number, b: number) => void;
  readonly txdetails_tx: (a: number) => number;
  readonly txdetails_txid: (a: number) => number;
  readonly txdetails_height: (a: number) => number;
  readonly txdetails_timestamp: (a: number) => number;
  readonly txdetails_txType: (a: number) => [number, number];
  readonly txdetails_balance: (a: number) => number;
  readonly txdetails_fees: (a: number) => number;
  readonly txdetails_feesAsset: (a: number, b: number) => bigint;
  readonly txdetails_unblindedUrl: (a: number, b: number, c: number) => [number, number];
  readonly txdetails_inputs: (a: number) => [number, number];
  readonly txdetails_outputs: (a: number) => [number, number];
  readonly __wbg_txoutdetails_free: (a: number, b: number) => void;
  readonly txoutdetails_outpoint: (a: number) => number;
  readonly txoutdetails_script_pubkey: (a: number) => number;
  readonly txoutdetails_height: (a: number) => number;
  readonly txoutdetails_address: (a: number) => number;
  readonly txoutdetails_unblinded: (a: number) => number;
  readonly txoutdetails_is_explicit: (a: number) => number;
  readonly txoutdetails_is_spent: (a: number) => number;
  readonly __wbg_txsopt_free: (a: number, b: number) => void;
  readonly txsopt_default: () => number;
  readonly txsopt_withPagination: (a: number, b: number) => number;
  readonly __wbg_txopt_free: (a: number, b: number) => void;
  readonly txopt_default: () => number;
  readonly wollet_txs: (a: number, b: number) => [number, number, number, number];
  readonly wollet_numTxs: (a: number) => [number, number, number];
  readonly wollet_txDetails: (a: number, b: number, c: number) => [number, number, number];
  readonly transaction_toBytes: (a: number) => [number, number];
  readonly __wbg_wolletdescriptor_free: (a: number, b: number) => void;
  readonly wolletdescriptor_new: (a: number, b: number) => [number, number, number];
  readonly wolletdescriptor_toString: (a: number) => [number, number];
  readonly wolletdescriptor_newMultiWshSlip77: (a: number, b: number, c: number) => [number, number, number];
  readonly wolletdescriptor_isMainnet: (a: number) => number;
  readonly wolletdescriptor_isAmp0: (a: number) => number;
  readonly __wbg_ledgerweb_free: (a: number, b: number) => void;
  readonly ledgerweb_new: (a: any, b: number) => number;
  readonly ledgerweb_getVersion: (a: number) => any;
  readonly ledgerweb_deriveXpub: (a: number, b: number, c: number) => any;
  readonly ledgerweb_slip77MasterBlindingKey: (a: number) => any;
  readonly ledgerweb_fingerprint: (a: number) => any;
  readonly ledgerweb_wpkhSlip77Descriptor: (a: number) => any;
  readonly ledgerweb_sign: (a: number, b: number) => any;
  readonly ledgerweb_getReceiveAddressSingle: (a: number, b: number) => any;
  readonly searchLedgerDevice: () => any;
  readonly __wbg_precision_free: (a: number, b: number) => void;
  readonly precision_new: (a: number) => [number, number, number];
  readonly precision_satsToString: (a: number, b: bigint) => [number, number];
  readonly precision_stringToSats: (a: number, b: number, c: number) => [bigint, number, number];
  readonly __wbg_wolletbuilder_free: (a: number, b: number) => void;
  readonly wolletbuilder_new: (a: number, b: number) => number;
  readonly wolletbuilder_withMergeThreshold: (a: number, b: number) => number;
  readonly wolletbuilder_utxoOnly: (a: number, b: number) => number;
  readonly wolletbuilder_withExperimentalStore: (a: number, b: any) => number;
  readonly wolletbuilder_withTxsStore: (a: number, b: any) => number;
  readonly wolletbuilder_setEncryptionTxsStore: (a: number, b: number) => number;
  readonly wolletbuilder_build: (a: number) => [number, number, number];
  readonly __wbg_amp2_free: (a: number, b: number) => void;
  readonly __wbg_amp2descriptor_free: (a: number, b: number) => void;
  readonly amp2descriptor_descriptor: (a: number) => number;
  readonly amp2descriptor_toString: (a: number) => [number, number];
  readonly amp2descriptor_newWithCustomDescriptor: (a: number) => number;
  readonly amp2_new: (a: number, b: number, c: number, d: number) => [number, number, number];
  readonly amp2_newTestnet: () => number;
  readonly amp2_descriptorFromStr: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
  readonly amp2_register: (a: number, b: number) => any;
  readonly amp2_cosign: (a: number, b: number) => any;
  readonly __wbg_boltzsessionbuilder_free: (a: number, b: number) => void;
  readonly __wbg_boltzsession_free: (a: number, b: number) => void;
  readonly boltzsessionbuilder_new: (a: number, b: number) => [number, number, number];
  readonly boltzsessionbuilder_createSwapTimeout: (a: number, b: bigint) => number;
  readonly boltzsessionbuilder_timeoutAdvance: (a: number, b: bigint) => number;
  readonly boltzsessionbuilder_mnemonic: (a: number, b: number) => number;
  readonly boltzsessionbuilder_polling: (a: number, b: number) => number;
  readonly boltzsessionbuilder_nextIndexToUse: (a: number, b: number) => number;
  readonly boltzsessionbuilder_referralId: (a: number, b: number, c: number) => number;
  readonly boltzsessionbuilder_apiUrl: (a: number, b: number, c: number) => number;
  readonly boltzsessionbuilder_bitcoinElectrumClient: (a: number, b: number, c: number) => [number, number, number];
  readonly boltzsessionbuilder_randomPreimages: (a: number, b: number) => number;
  readonly boltzsessionbuilder_build: (a: number) => any;
  readonly __wbg_preparepayresponse_free: (a: number, b: number) => void;
  readonly preparepayresponse_serialize: (a: number) => [number, number, number, number];
  readonly preparepayresponse_swapId: (a: number) => [number, number];
  readonly preparepayresponse_uri: (a: number) => [number, number];
  readonly preparepayresponse_uriAddress: (a: number) => [number, number, number];
  readonly preparepayresponse_uriAmount: (a: number) => bigint;
  readonly preparepayresponse_fee: (a: number) => [number, bigint];
  readonly preparepayresponse_completePay: (a: number) => any;
  readonly __wbg_invoice_free: (a: number, b: number) => void;
  readonly invoice_toString: (a: number) => [number, number];
  readonly invoice_amountSats: (a: number) => [bigint, number, number];
  readonly invoice_isBolt11: (a: number) => number;
  readonly invoice_isBolt12: (a: number) => number;
  readonly invoice_bolt11Invoice: (a: number) => [number, number];
  readonly invoice_bolt12Invoice: (a: number) => [number, number];
  readonly __wbg_invoiceresponse_free: (a: number, b: number) => void;
  readonly invoiceresponse_serialize: (a: number) => [number, number, number, number];
  readonly invoiceresponse_bolt11Invoice: (a: number) => [number, number];
  readonly invoiceresponse_swapId: (a: number) => [number, number];
  readonly invoiceresponse_fee: (a: number) => [number, bigint];
  readonly invoiceresponse_completePay: (a: number) => any;
  readonly boltzsession_rescueFile: (a: number) => [number, number, number, number];
  readonly boltzsession_preparePay: (a: number, b: number, c: number) => any;
  readonly boltzsession_fetchBolt12Invoice: (a: number, b: number) => any;
  readonly boltzsession_invoice: (a: number, b: bigint, c: number, d: number, e: number) => any;
  readonly boltzsession_restorePreparePay: (a: number, b: number, c: number) => any;
  readonly boltzsession_restoreInvoice: (a: number, b: number, c: number) => any;
  readonly __wbg_lightningpayment_free: (a: number, b: number) => void;
  readonly lightningpayment_new: (a: number, b: number) => [number, number, number];
  readonly lightningpayment_toString: (a: number) => [number, number];
  readonly lightningpayment_toUriQr: (a: number, b: number) => [number, number, number, number];
  readonly __wbg_fees_free: (a: number, b: number) => void;
  readonly fees_entries: (a: number) => [number, number, number];
  readonly fees_toJSON: (a: number) => [number, number, number];
  readonly fees_toString: (a: number) => [number, number];
  readonly stringToQr: (a: number, b: number, c: number) => [number, number, number, number];
  readonly __wbg_assetid_free: (a: number, b: number) => void;
  readonly __wbg_assetids_free: (a: number, b: number) => void;
  readonly assetid_fromString: (a: number, b: number) => [number, number, number];
  readonly assetid_fromBytes: (a: number, b: number) => [number, number, number];
  readonly assetid_toBytes: (a: number) => [number, number];
  readonly assetid_toString: (a: number) => [number, number];
  readonly assetids_empty: () => [number, number, number];
  readonly assetids_toString: (a: number) => [number, number];
  readonly __wbg_jade_free: (a: number, b: number) => void;
  readonly jade_new: (a: number, b: number) => any;
  readonly jade_fromSerial: (a: number, b: number) => any;
  readonly jade_getVersion: (a: number) => any;
  readonly jade_getMasterXpub: (a: number) => any;
  readonly jade_getReceiveAddressSingle: (a: number, b: number, c: number, d: number) => any;
  readonly jade_getReceiveAddressMulti: (a: number, b: number, c: number, d: number, e: number) => any;
  readonly jade_sign: (a: number, b: number) => any;
  readonly jade_wpkh: (a: number) => any;
  readonly jade_shWpkh: (a: number) => any;
  readonly jade_multi: (a: number, b: number, c: number) => any;
  readonly jade_getRegisteredMultisigs: (a: number) => any;
  readonly jade_keyoriginXpub: (a: number, b: number) => any;
  readonly jade_registerDescriptor: (a: number, b: number, c: number, d: number) => any;
  readonly __wbg_singlesig_free: (a: number, b: number) => void;
  readonly singlesig_from: (a: number, b: number) => [number, number, number];
  readonly __wbg_jadewebsocket_free: (a: number, b: number) => void;
  readonly jadewebsocket_new: (a: number, b: number, c: number) => any;
  readonly jadewebsocket_fromWebSocket: (a: number, b: number, c: number) => any;
  readonly jadewebsocket_getVersion: (a: number) => any;
  readonly jadewebsocket_getMasterXpub: (a: number) => any;
  readonly jadewebsocket_getReceiveAddressSingle: (a: number, b: number, c: number, d: number) => any;
  readonly jadewebsocket_getReceiveAddressMulti: (a: number, b: number, c: number, d: number, e: number) => any;
  readonly jadewebsocket_sign: (a: number, b: number) => any;
  readonly jadewebsocket_wpkh: (a: number) => any;
  readonly jadewebsocket_shWpkh: (a: number) => any;
  readonly jadewebsocket_multi: (a: number, b: number, c: number) => any;
  readonly jadewebsocket_getRegisteredMultisigs: (a: number) => any;
  readonly jadewebsocket_keyoriginXpub: (a: number, b: number) => any;
  readonly jadewebsocket_registerDescriptor: (a: number, b: number, c: number, d: number) => any;
  readonly __wbg_unvalidatedliquidexproposal_free: (a: number, b: number) => void;
  readonly __wbg_validatedliquidexproposal_free: (a: number, b: number) => void;
  readonly __wbg_assetamount_free: (a: number, b: number) => void;
  readonly unvalidatedliquidexproposal_new: (a: number, b: number) => [number, number, number];
  readonly unvalidatedliquidexproposal_fromPset: (a: number) => [number, number, number];
  readonly unvalidatedliquidexproposal_insecureValidate: (a: number) => [number, number, number];
  readonly unvalidatedliquidexproposal_validate: (a: number, b: number) => [number, number, number];
  readonly unvalidatedliquidexproposal_toString: (a: number) => [number, number];
  readonly assetamount_amount: (a: number) => bigint;
  readonly assetamount_asset: (a: number) => number;
  readonly validatedliquidexproposal_input: (a: number) => number;
  readonly validatedliquidexproposal_output: (a: number) => number;
  readonly validatedliquidexproposal_toString: (a: number) => [number, number];
  readonly __wbg_mnemonic_free: (a: number, b: number) => void;
  readonly mnemonic_new: (a: number, b: number) => [number, number, number];
  readonly mnemonic_toString: (a: number) => [number, number];
  readonly mnemonic_fromEntropy: (a: number, b: number) => [number, number, number];
  readonly mnemonic_fromRandom: (a: number) => [number, number, number];
  readonly __wbg_pricesfetcher_free: (a: number, b: number) => void;
  readonly __wbg_pricesfetcherbuilder_free: (a: number, b: number) => void;
  readonly pricesfetcher_new: () => [number, number, number];
  readonly pricesfetcher_rates: (a: number, b: number) => any;
  readonly __wbg_currencycode_free: (a: number, b: number) => void;
  readonly currencycode_new: (a: number, b: number) => [number, number, number];
  readonly currencycode_name: (a: number) => [number, number];
  readonly currencycode_alpha3: (a: number) => [number, number];
  readonly currencycode_exp: (a: number) => number;
  readonly __wbg_exchangerates_free: (a: number, b: number) => void;
  readonly exchangerates_median: (a: number) => number;
  readonly exchangerates_results: (a: number) => [number, number, number];
  readonly exchangerates_resultsCount: (a: number) => number;
  readonly exchangerates_serialize: (a: number) => [number, number, number, number];
  readonly assetid_new: (a: number, b: number) => [number, number, number];
  readonly __wbg_address_free: (a: number, b: number) => void;
  readonly address_new: (a: number, b: number) => [number, number, number];
  readonly address_parse: (a: number, b: number, c: number) => [number, number, number];
  readonly address_scriptPubkey: (a: number) => number;
  readonly address_isBlinded: (a: number) => number;
  readonly address_isMainnet: (a: number) => number;
  readonly address_toUnconfidential: (a: number) => number;
  readonly address_toString: (a: number) => [number, number];
  readonly address_QRCodeUri: (a: number, b: number) => [number, number, number, number];
  readonly address_QRCodeText: (a: number) => [number, number, number, number];
  readonly __wbg_addressresult_free: (a: number, b: number) => void;
  readonly addressresult_address: (a: number) => number;
  readonly addressresult_index: (a: number) => number;
  readonly __wbg_assetblindingfactor_free: (a: number, b: number) => void;
  readonly assetblindingfactor_fromString: (a: number, b: number) => [number, number, number];
  readonly assetblindingfactor_fromBytes: (a: number, b: number) => [number, number, number];
  readonly assetblindingfactor_zero: () => number;
  readonly assetblindingfactor_toBytes: (a: number) => [number, number];
  readonly assetblindingfactor_toString: (a: number) => [number, number];
  readonly __wbg_valueblindingfactor_free: (a: number, b: number) => void;
  readonly valueblindingfactor_fromString: (a: number, b: number) => [number, number, number];
  readonly valueblindingfactor_fromBytes: (a: number, b: number) => [number, number, number];
  readonly valueblindingfactor_zero: () => number;
  readonly valueblindingfactor_toBytes: (a: number) => [number, number];
  readonly valueblindingfactor_toString: (a: number) => [number, number];
  readonly __wbg_script_free: (a: number, b: number) => void;
  readonly script_new: (a: number, b: number) => [number, number, number];
  readonly script_empty: () => number;
  readonly script_bytes: (a: number) => [number, number];
  readonly script_jet_sha256_hex: (a: number) => [number, number];
  readonly script_asm: (a: number) => [number, number];
  readonly script_newOpReturn: (a: number, b: number) => number;
  readonly script_isProvablyUnspendable: (a: number) => number;
  readonly script_isProvablySegwit: (a: number, b: number) => number;
  readonly script_toString: (a: number) => [number, number];
  readonly __wbg_contract_free: (a: number, b: number) => void;
  readonly contract_new: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number) => [number, number, number];
  readonly contract_toString: (a: number) => [number, number];
  readonly contract_domain: (a: number) => [number, number];
  readonly contract_clone: (a: number) => number;
  readonly __wbg_lastusedindexresponse_free: (a: number, b: number) => void;
  readonly lastusedindexresponse_external: (a: number) => number;
  readonly lastusedindexresponse_internal: (a: number) => number;
  readonly lastusedindexresponse_tip: (a: number) => [number, number];
  readonly __wbg_esploraclient_free: (a: number, b: number) => void;
  readonly esploraclient_new: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
  readonly esploraclient_fullScan: (a: number, b: number) => any;
  readonly esploraclient_fullScanToIndex: (a: number, b: number, c: number) => any;
  readonly esploraclient_broadcastTx: (a: number, b: number) => any;
  readonly esploraclient_broadcast: (a: number, b: number) => any;
  readonly esploraclient_setWaterfallsServerRecipient: (a: number, b: number, c: number) => any;
  readonly esploraclient_waterfallsDescriptor: (a: number, b: number) => any;
  readonly esploraclient_lastUsedIndex: (a: number, b: number) => any;
  readonly rustsecp256k1_v0_12_context_create: (a: number) => number;
  readonly rustsecp256k1_v0_12_context_destroy: (a: number) => void;
  readonly rustsecp256k1_v0_12_default_illegal_callback_fn: (a: number, b: number) => void;
  readonly rustsecp256k1_v0_12_default_error_callback_fn: (a: number, b: number) => void;
  readonly rustsecp256k1zkp_v0_10_0_default_illegal_callback_fn: (a: number, b: number) => void;
  readonly rustsecp256k1zkp_v0_10_0_default_error_callback_fn: (a: number, b: number) => void;
  readonly rustsecp256k1_v0_10_0_context_create: (a: number) => number;
  readonly rustsecp256k1_v0_10_0_context_destroy: (a: number) => void;
  readonly rustsecp256k1_v0_10_0_default_illegal_callback_fn: (a: number, b: number) => void;
  readonly rustsecp256k1_v0_10_0_default_error_callback_fn: (a: number, b: number) => void;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_exn_store: (a: number) => void;
  readonly __externref_table_alloc: () => number;
  readonly __wbindgen_export_4: WebAssembly.Table;
  readonly __wbindgen_export_5: WebAssembly.Table;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __externref_table_dealloc: (a: number) => void;
  readonly __externref_drop_slice: (a: number, b: number) => void;
  readonly wasm_bindgen__convert__closures_____invoke__hcadcd9ad50d60798: (a: number, b: number) => void;
  readonly closure1282_externref_shim: (a: number, b: number, c: any) => void;
  readonly closure807_externref_shim: (a: number, b: number, c: any) => void;
  readonly wasm_bindgen__convert__closures_____invoke__hf60dda57b54e1240: (a: number, b: number) => void;
  readonly closure2133_externref_shim: (a: number, b: number, c: any) => void;
  readonly closure2925_externref_shim: (a: number, b: number, c: any, d: any) => void;
  readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;
/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
*
* @returns {InitOutput}
*/
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
