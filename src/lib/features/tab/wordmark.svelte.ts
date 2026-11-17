/**
 * Development builds only: the tweak window's wordmark length, which replaces `WORDMARK` in
 * `EdgeTab.svelte` while set.
 */
export const wordmarkOverride = $state<{ length: number | null }>({ length: null });
