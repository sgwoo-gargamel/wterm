/**
 * Settings are written next to the executable, which can be read-only depending on
 * where the app was installed or copied. Writes are fire-and-forget, so a failure
 * would otherwise lose the user's changes without a word.
 */
export const saveStatus = $state<{ failed: boolean; path: string; detail: string }>({
	failed: false,
	path: '',
	detail: ''
});

export function reportSaveFailure(detail: string, path: string) {
	saveStatus.detail = detail;
	saveStatus.path = path;
	saveStatus.failed = true;
}

/** A later write succeeding means the problem is gone, so the warning clears itself */
export function clearSaveFailure() {
	saveStatus.failed = false;
}
