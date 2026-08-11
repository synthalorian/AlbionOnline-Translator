// License state + Lemon Squeezy activation glue.
// Backend source of truth: src-tauri/src/license.rs
import { invoke } from "@tauri-apps/api/core";

/**
 * @typedef {{ mode: "trial", days_remaining: number } | { mode: "licensed", status: string } | { mode: "locked" }} LicenseMode
 * @typedef {LicenseMode & { buy_url: string }} LicenseStatus
 */

/** @type {LicenseStatus | null} */
let cached = null;

export async function loadLicenseStatus() {
  cached = await invoke("get_license_status");
  return cached;
}

/** @param {string} key */
export async function activateLicense(key) {
  cached = await invoke("activate_license", { key });
  return cached;
}

export async function deactivateLicense() {
  await invoke("deactivate_license");
  cached = await invoke("get_license_status");
  return cached;
}

export function getCachedLicense() {
  return cached;
}
