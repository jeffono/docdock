import { writable } from "svelte/store";
// Refresh the address-bar state after a preference removal while Notes stays mounted.
export const blockingRevision = writable(0);
