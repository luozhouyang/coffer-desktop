/**
 * True for Mac App Store builds. The App Store does not allow the app to
 * download and install its own updates, so the MAS build sets
 * `VITE_COFFER_MAS=1` and the update UI stays out of the bundle.
 */
export const IS_APP_STORE_BUILD = import.meta.env.VITE_COFFER_MAS === "1";
