//! Shared helpers used by multiple handler modules.
//!
//! The two helpers below both convert a free-form device-type string into
//! the typed [`DeviceType`] that `asterism-xpub` consumes. They differ in
//! how they treat unknown values:
//!
//! - [`parse_device_type`] is *lenient* — used when re-hydrating the
//!   `signers.device_type` column, where an unknown value indicates DB
//!   drift rather than malicious user input. Falls through to
//!   [`DeviceType::Generic`].
//! - [`parse_onboard_device_type`] is *strict* — used at onboarding time
//!   where the value comes straight from a browser POST. Returns
//!   `Err(_)` for anything outside our supported set so the user sees
//!   a clear 400 instead of silently being saved as "Generic".

use emvault::xpub::DeviceType;

/// Map the `signers.device_type` column value back into a [`DeviceType`].
///
/// The column is populated via `format!("{:?}", signer.device_type())`,
/// so the round-trip is `DeviceType::Trezor` → `"Trezor"` →
/// `DeviceType::Trezor`. Unknown values fall through to
/// [`DeviceType::Generic`] rather than failing federation builds against
/// historical rows.
pub fn parse_device_type(s: &str) -> DeviceType {
    match s {
        "Trezor" => DeviceType::Trezor,
        "Jade" => DeviceType::Jade,
        "PassportPrime" => DeviceType::PassportPrime,
        "Ledger" => DeviceType::Ledger,
        "Coldcard" => DeviceType::Coldcard,
        _ => DeviceType::Generic,
    }
}

/// Strict variant for the onboarding endpoint: only accept the device types
/// the app actually has UI for today (Trezor, Jade, Ledger). Anything else
/// is rejected with the original input echoed back so the user can see
/// what the server received.
///
/// # Errors
/// Returns the offending input wrapped in a string when no match is found.
pub fn parse_onboard_device_type(s: &str) -> Result<DeviceType, String> {
    match s {
        "Trezor" => Ok(DeviceType::Trezor),
        "Jade" => Ok(DeviceType::Jade),
        "Ledger" => Ok(DeviceType::Ledger),
        other => Err(format!(
            "unsupported device_type `{other}` (expected Trezor, Jade, or Ledger)",
        )),
    }
}
